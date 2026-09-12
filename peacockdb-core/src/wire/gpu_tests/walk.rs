//! The driver the walk and the schema catalog share: one executor session over a recipe
//! plan, and a walk of the tree making exactly the calls each recipe names, threading every
//! output handle into the next call's input and exporting at the root.
//!
//! No scheduling — every shape driven here plans one batch per lane, so a recipe's own
//! call order is the schedule. The walk's own assertions stay in `mod.rs`; a firing's
//! declared and exported schemas are recorded here for whichever test reads them.

use datafusion::arrow::array::RecordBatch;
use datafusion::arrow::datatypes::{
    DECIMAL128_MAX_PRECISION, DECIMAL256_MAX_PRECISION, DataType, Field, Schema as ArrowSchema,
    SchemaRef,
};
use datafusion::arrow::ipc::reader::StreamReader;

use super::super::{
    AbiSymbol, Call, CallPattern, FbKind, Input, Recipe, RecipePlan, Seq, attach_recipes,
};
use crate::executor::{BatchForwarder, forwarder_for};
use crate::plan::GpuNode;
use crate::plan::{ExecutorCategory, category_of};
use crate::plan::{NodeRef, as_node_ref};
use crate::planner;
use crate::planner::PlanKnobs;
use peacockdb_ffi::raw::{
    PeacockExecutor, PeacockNodeStats, peacock_executor_begin_plan, peacock_executor_create,
    peacock_executor_destroy, peacock_executor_end_plan, peacock_executor_execute_node,
    peacock_executor_execute_scan_rowgroups, peacock_last_error, peacock_result_free,
    peacock_result_from_handle,
};

use crate::test_support::{GPU_BUDGET, data_dir_for, total_rows};

/// The handles a node produced: one entry per lane, holding that lane's batches in
/// arrival order.
type Lanes = Vec<Vec<u64>>;

/// An executor with a recipe plan loaded, torn down in the order the header requires.
struct Session {
    executor: *mut PeacockExecutor,
}

impl Session {
    /// `begin_plan`'s `out_node_count` is asserted against the fb nodes the writer created
    /// — never against the plan tree's own count, which differs in most plans. Until a
    /// device has parsed a buffer we wrote, our agreement with the C++ post-order rested on
    /// two child-order functions having been read side by side; this is the first place
    /// both numbers exist at once.
    fn open(recipes: &RecipePlan) -> Self {
        let mut executor: *mut PeacockExecutor = std::ptr::null_mut();
        assert_eq!(
            unsafe { peacock_executor_create(GPU_BUDGET as u64, &mut executor) },
            0,
            "peacock_executor_create failed"
        );
        let session = Self { executor };
        let bytes = recipes.bytes();
        let mut nodes = 0u64;
        let rc = unsafe {
            peacock_executor_begin_plan(executor, bytes.as_ptr(), bytes.len() as u64, &mut nodes)
        };
        assert_eq!(rc, 0, "begin_plan failed: {}", session.last_error());
        assert_eq!(
            nodes as usize,
            recipes.wire_nodes(),
            "the C++ post-order holds {nodes} nodes and the writer created {} — every seq \
             a recipe publishes is an index into that walk, so the two numbering the same \
             tree is what makes a call address the node it names",
            recipes.wire_nodes()
        );
        session
    }

    fn last_error(&self) -> String {
        let message = unsafe { peacock_last_error(self.executor) };
        if message.is_null() {
            return String::new();
        }
        unsafe { std::ffi::CStr::from_ptr(message) }
            .to_string_lossy()
            .into_owned()
    }

    /// One batch's worth of a scan: the row groups the mapping named for it, overriding
    /// the list the node carries.
    fn scan(&self, seq: Seq, row_groups: &[u32]) -> u64 {
        let mut handle = 0u64;
        let mut stats = PeacockNodeStats::default();
        let rc = unsafe {
            peacock_executor_execute_scan_rowgroups(
                self.executor,
                seq as u64,
                row_groups.as_ptr(),
                row_groups.len() as u64,
                &mut handle,
                &mut stats,
            )
        };
        assert_eq!(
            rc,
            0,
            "execute_scan_rowgroups(#{seq}, {row_groups:?}) failed: {}",
            self.last_error()
        );
        handle
    }

    /// One `execute_node`, its input handles grouped by the child slot each fills.
    fn execute(&self, seq: Seq, inputs: &[Vec<u64>], out_cap: usize) -> Vec<u64> {
        let counts: Vec<u64> = inputs.iter().map(|group| group.len() as u64).collect();
        let flat: Vec<u64> = inputs.concat();
        let mut handles = vec![0u64; out_cap];
        let mut stats = vec![PeacockNodeStats::default(); out_cap];
        let mut produced = 0u64;
        let rc = unsafe {
            peacock_executor_execute_node(
                self.executor,
                seq as u64,
                flat.as_ptr(),
                counts.as_ptr(),
                counts.len() as u64,
                handles.as_mut_ptr(),
                out_cap as u64,
                &mut produced,
                stats.as_mut_ptr(),
            )
        };
        assert_eq!(
            rc,
            0,
            "execute_node(#{seq}, {counts:?} handles) failed: {}",
            self.last_error()
        );
        handles.truncate(produced as usize);
        handles
    }

    /// The whole handle across the boundary, and the schema the stream was written under.
    /// No shape here plans a limit, so the sink's range is always the batch it is handed.
    fn export(&self, handle: u64) -> Result<(SchemaRef, Vec<RecordBatch>), String> {
        self.read_export(handle, |reader| {
            let schema = reader.schema();
            let batches = reader
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| format!("decoding the exported IPC stream: {error}"))?;
            Ok((schema, batches))
        })
    }

    /// The schema the device produced for a handle, read off the IPC stream the export
    /// wrote and nothing more: `StreamReader::try_new` parses the schema message before
    /// any batch, so this answers for a firing that produced no rows.
    ///
    /// The export that already exists, under the production path, so a decimal reads at 38
    /// and nullability reads as `has_nulls()` — the exporter's rewrites, which
    /// `declared-schemas.md` names as the two dimensions this cannot answer.
    fn exported_schema(&self, handle: u64) -> Result<SchemaRef, String> {
        self.read_export(handle, |reader| Ok(reader.schema()))
    }

    /// A refusal is returned rather than asserted: a handle the device cannot export is a
    /// finding for the case reading it, not a failure of the harness.
    fn read_export<T>(
        &self,
        handle: u64,
        read: impl FnOnce(StreamReader<std::io::Cursor<&[u8]>>) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut ipc: *mut u8 = std::ptr::null_mut();
        let mut len = 0u64;
        let rc = unsafe {
            peacock_result_from_handle(self.executor, handle, 0, u64::MAX, &mut ipc, &mut len)
        };
        if rc != 0 {
            return Err(format!("result_from_handle refused: {}", self.last_error()));
        }
        // A whole-table range ships nothing only for a range naming no rows of a non-empty
        // table, which `0..MAX` never is; an empty table still ships its schema.
        if len == 0 {
            return Err("result_from_handle shipped no stream".to_string());
        }
        let bytes = unsafe { std::slice::from_raw_parts(ipc, len as usize) };
        let read = StreamReader::try_new(std::io::Cursor::new(bytes), None)
            .map_err(|error| format!("decoding the exported IPC stream: {error}"))
            .and_then(read);
        unsafe { peacock_result_free(ipc) };
        read
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        unsafe {
            peacock_executor_end_plan(self.executor);
            peacock_executor_destroy(self.executor);
        }
    }
}

/// Where a call's named inputs are, at the moment the walk makes it. Every one is a handle
/// the walk already holds, which is what lets a recipe be driven without re-reading the
/// node it came from.
#[derive(Default)]
struct At {
    batch: Option<u64>,
    build: Option<u64>,
    lane: Vec<u64>,
    all_lanes: Vec<u64>,
    prior: Option<u64>,
}

/// Where a call's named input comes from, in the handles the walk is already holding.
///
/// The two copies resolve to the handle itself. They name a device copy of something a
/// later call still needs and the ABI has no symbol for one (#152) — but every shape here
/// plans a single probe batch, so the handle is used once and handed over, which is what
/// the join arm asserts before anything reaches this.
fn resolve(input: Input, at: &At) -> Vec<u64> {
    let held = |handle: Option<u64>| vec![handle.unwrap_or_else(|| panic!("no {input:?} here"))];
    match input {
        Input::Batch | Input::BatchCopy => held(at.batch),
        Input::BuildSide | Input::BuildSideCopy => held(at.build),
        Input::PriorOutput => held(at.prior),
        Input::LaneBatches => at.lane.clone(),
        Input::AllLanes => at.all_lanes.clone(),
        Input::AccumulatedKeys | Input::RowGroups | Input::RowRange => {
            panic!("{input:?} is not a handle the walk holds")
        }
    }
}

/// A node's result where exactly one handle was expected.
fn only(handles: Vec<u64>, what: &str) -> u64 {
    match handles.as_slice() {
        [handle] => *handle,
        other => panic!("{what}: {} handles rather than one", other.len()),
    }
}

/// The walk: where the recipes are, which node is next in the post-order they are indexed
/// by, and what reached the sink.
struct Walk<'a> {
    session: &'a Session,
    recipes: &'a RecipePlan,
    next_node: usize,
    exported: Vec<RecordBatch>,
    /// Every call made, in the order it was made. A test reads it when its claim is about
    /// the calls rather than the answer, and a failure prints it: a wrong table is the
    /// symptom of one call, and the seq is what a reader looks up in the payload golden.
    made: Vec<(Seq, FbKind)>,
    /// One entry per handle a declared call produced, and per handle the sink exported.
    firings: Vec<Firing>,
}

impl Walk<'_> {
    /// Children first, then this node: the same post-order `attach_recipes` indexed its
    /// recipes by, so the position it takes here is the position they are stored at.
    fn node(&mut self, node: &dyn GpuNode) -> Lanes {
        let kids: Vec<Lanes> = node
            .children()
            .into_iter()
            .map(|child| self.node(child))
            .collect();
        let index = self.next_node;
        self.next_node += 1;
        let category = category_of(node);
        if category == ExecutorCategory::BatchForwarder {
            return route(node, &kids);
        }
        let recipe = self
            .recipes
            .get(index)
            .unwrap_or_else(|| panic!("{} makes ABI calls and carries no recipe", node.name()));
        match category {
            ExecutorCategory::Source => self.source(node, recipe),
            ExecutorCategory::Exec => self.per_batch(node, recipe, &kids[0]),
            ExecutorCategory::PartitionEmitter => self.emit_partitions(node, recipe, &kids[0]),
            ExecutorCategory::BatchAccumulator => self.per_lane(node, recipe, &kids[0]),
            ExecutorCategory::PartitionAccumulator => self.over_all_lanes(recipe, &kids[0]),
            ExecutorCategory::Join => self.join(node, recipe, &kids[0], &kids[1]),
            ExecutorCategory::Unload => self.unload(node, recipe, &kids[0]),
            ExecutorCategory::BatchForwarder => unreachable!("returned above"),
        }
    }

    /// The recipe's calls in order, each one's prior output being the last one's.
    fn chain(&mut self, calls: &[&Call], at: &mut At) -> Vec<u64> {
        let mut produced = Vec::new();
        for call in calls {
            produced = self.make(call, at);
            at.prior = produced.first().copied();
        }
        produced
    }

    fn make(&mut self, call: &Call, at: &At) -> Vec<u64> {
        let (seq, kind) = call.target.unwrap_or_else(|| {
            panic!(
                "{} takes runtime bounds rather than a seq, and no shape here plans one",
                call.symbol.name()
            )
        });
        let inputs: Vec<Vec<u64>> = call.inputs.iter().map(|from| resolve(*from, at)).collect();
        let out_cap = match kind {
            FbKind::Repartition { lanes } => lanes as usize,
            _ => 1,
        };
        self.made.push((seq, kind));
        let handles = self.session.execute(seq, &inputs, out_cap);
        for handle in &handles {
            self.measure(call, self.session.exported_schema(*handle));
        }
        handles
    }

    /// What this firing declared beside what the device handed back for it. A call with no
    /// declaration is one of the arms `declared-schemas-derived.md` takes; it fires and is
    /// not measured, and that is the one skip, here rather than in each case.
    fn measure(&mut self, call: &Call, exported: Result<SchemaRef, String>) {
        let Some(declared) = &call.output_schema else {
            return;
        };
        self.firings.push(Firing {
            symbol: call.symbol,
            target: call.target,
            declared: declared.fields.clone(),
            exported,
        });
    }

    fn source(&mut self, node: &dyn GpuNode, recipe: &Recipe) -> Lanes {
        let NodeRef::LoadParquet(load) = as_node_ref(node) else {
            unreachable!("the source category holds one node kind")
        };
        let [call] = recipe.calls.as_slice() else {
            panic!("a scan's recipe is one call per batch")
        };
        assert_eq!(call.symbol, AbiSymbol::ExecuteScanRowGroups);
        let (seq, kind) = call.target.expect("a scan addresses its own node");
        let mut lanes = Vec::with_capacity(load.partition_groups.len());
        for lane in &load.partition_groups {
            let mut batches = Vec::with_capacity(lane.len());
            for row_groups in lane {
                self.made.push((seq, kind));
                let handle = self.session.scan(seq, row_groups);
                self.measure(call, self.session.exported_schema(handle));
                batches.push(handle);
            }
            lanes.push(batches);
        }
        lanes
    }

    /// The map arms: one call chain per batch, output keeping its input's lane and batch
    /// structure.
    fn per_batch(&mut self, node: &dyn GpuNode, recipe: &Recipe, input: &Lanes) -> Lanes {
        let calls: Vec<&Call> = recipe.calls.iter().collect();
        let mut lanes = Vec::with_capacity(input.len());
        for lane in input {
            let mut batches = Vec::with_capacity(lane.len());
            for handle in lane {
                let mut at = At {
                    batch: Some(*handle),
                    ..At::default()
                };
                batches.push(only(self.chain(&calls, &mut at), node.name()));
            }
            lanes.push(batches);
        }
        lanes
    }

    /// The emitter's one call answers with a handle per output lane, so a batch of lane p
    /// is the p-th handle of every call its input made.
    fn emit_partitions(&mut self, node: &dyn GpuNode, recipe: &Recipe, input: &Lanes) -> Lanes {
        let calls: Vec<&Call> = recipe.calls.iter().collect();
        let out_lanes = match recipe.calls.first().and_then(|call| call.target) {
            Some((_, FbKind::Repartition { lanes })) => lanes as usize,
            other => panic!("{}: expected a repartition, got {other:?}", node.name()),
        };
        let mut lanes = vec![Vec::new(); out_lanes];
        for lane in input {
            for handle in lane {
                let mut at = At {
                    batch: Some(*handle),
                    ..At::default()
                };
                let scattered = self.chain(&calls, &mut at);
                assert_eq!(
                    scattered.len(),
                    out_lanes,
                    "{}: the scatter answered with {} handles",
                    node.name(),
                    scattered.len()
                );
                for (out, handle) in lanes.iter_mut().zip(scattered) {
                    out.push(handle);
                }
            }
        }
        lanes
    }

    /// An accumulator: whatever it does per batch, then its at-done calls once over the
    /// lane it accumulated. The two phases are the recipe's own grouping.
    fn per_lane(&mut self, node: &dyn GpuNode, recipe: &Recipe, input: &Lanes) -> Lanes {
        let (streamed, at_done) = phases(recipe);
        assert!(
            !at_done.is_empty(),
            "{}: nothing runs at done, so the lane it accumulated has no output — a \
             streaming limit is the shape that reaches here, and none is planned",
            node.name()
        );
        let mut lanes = Vec::with_capacity(input.len());
        for lane in input {
            let mut held = Vec::with_capacity(lane.len());
            for handle in lane {
                if streamed.is_empty() {
                    held.push(*handle);
                    continue;
                }
                let mut at = At {
                    batch: Some(*handle),
                    ..At::default()
                };
                held.push(only(self.chain(&streamed, &mut at), node.name()));
            }
            let mut at = At {
                lane: held,
                ..At::default()
            };
            lanes.push(vec![only(self.chain(&at_done, &mut at), node.name())]);
        }
        lanes
    }

    /// One call over every lane's handle, partition-major, answering with one lane.
    fn over_all_lanes(&mut self, recipe: &Recipe, input: &Lanes) -> Lanes {
        let calls: Vec<&Call> = recipe.calls.iter().collect();
        let mut at = At {
            all_lanes: input.concat(),
            ..At::default()
        };
        vec![vec![only(
            self.chain(&calls, &mut at),
            "a partition accumulator",
        )]]
    }

    fn join(&mut self, node: &dyn GpuNode, recipe: &Recipe, build: &Lanes, probe: &Lanes) -> Lanes {
        assert!(
            recipe
                .calls
                .iter()
                .all(|call| call.when == CallPattern::PerProbeBatch),
            "{}: a finish pass accumulates probe keys across batches (#136), which no shape \
             here plans",
            node.name()
        );
        assert_eq!(
            build.len(),
            probe.len(),
            "{}: lane p of one side must hold what can match lane p of the other",
            node.name()
        );
        let calls: Vec<&Call> = recipe.calls.iter().collect();
        let mut lanes = Vec::with_capacity(build.len());
        for (build_lane, probe_lane) in build.iter().zip(probe) {
            assert_eq!(
                probe_lane.len(),
                1,
                "{}: {} probe batches, and the call consumes the build handle with no ABI \
                 symbol to copy it (#152) — every shape here plans one probe batch",
                node.name(),
                probe_lane.len()
            );
            let mut at = At {
                batch: Some(probe_lane[0]),
                build: Some(only(build_lane.clone(), "a join's build side")),
                ..At::default()
            };
            lanes.push(vec![only(self.chain(&calls, &mut at), node.name())]);
        }
        lanes
    }

    /// The sink produces results rather than handles, so it answers with no lanes.
    fn unload(&mut self, node: &dyn GpuNode, recipe: &Recipe, input: &Lanes) -> Lanes {
        assert!(
            node.row_interval().is_none(),
            "a root-adjacent limit gives the sink a row range per handle, and no shape here \
             plans one"
        );
        let [call] = recipe.calls.as_slice() else {
            panic!("a sink's recipe is one call per handle")
        };
        assert_eq!(call.symbol, AbiSymbol::ResultFromHandle);
        for lane in input {
            for handle in lane {
                let exported = self.session.export(*handle).map(|(schema, batches)| {
                    self.exported.extend(batches);
                    schema
                });
                self.measure(call, exported);
            }
        }
        Vec::new()
    }
}

/// A recipe's calls split into the two phases a walk drives: what runs as batches arrive,
/// and what runs once the lane is complete. A compaction runs exactly what done runs, so
/// at one batch per lane the two are the same call.
fn phases(recipe: &Recipe) -> (Vec<&Call>, Vec<&Call>) {
    recipe.calls.iter().partition(|call| {
        matches!(
            call.when,
            CallPattern::PerBatch | CallPattern::PerProbeBatch
        )
    })
}

/// No calls at all: a forwarder renumbers lanes, and `forwarder_for` is the routing the
/// drivers read off the same node. One batch per visit, cycling the sources in the order
/// the forwarder lists them.
fn route(node: &dyn GpuNode, kids: &[Lanes]) -> Lanes {
    let forwarder = forwarder_for(node);
    let out_lanes = node.kind().layout().map_or(1, |layout| layout.n);
    (0..out_lanes)
        .map(|out_lane| {
            let mut queues: Vec<&[u64]> = forwarder
                .sources_of(out_lane)
                .iter()
                .map(|(child, lane)| kids[*child][*lane].as_slice())
                .collect();
            let mut forwarded = Vec::new();
            while queues.iter().any(|queue| !queue.is_empty()) {
                for queue in queues.iter_mut() {
                    if let Some((first, rest)) = queue.split_first() {
                        forwarded.push(*first);
                        *queue = rest;
                    }
                }
            }
            forwarded
        })
        .collect()
}

/// What one walk of one query produced.
pub(crate) struct Walked {
    pub(crate) batches: Vec<RecordBatch>,
    pub(crate) calls: Vec<(Seq, FbKind)>,
    /// Every firing of a declared call, in the order made, with what the device exported
    /// for its handle. Undeclared calls are in `calls` and not here.
    pub(crate) firings: Vec<Firing>,
}

/// One firing of one declared call: what it declared and what the device handed back.
pub(crate) struct Firing {
    pub(crate) symbol: AbiSymbol,
    pub(crate) target: Option<(Seq, FbKind)>,
    pub(crate) declared: SchemaRef,
    /// Raw, as the export wrote it — precision at 38 on every decimal and nullability
    /// from `has_nulls()`, both the exporter's own — or why the export refused the handle.
    pub(crate) exported: Result<SchemaRef, String>,
}

impl Firing {
    /// `#3 CudfScan` for a call with a seq, the ABI symbol for a bare one: the spelling
    /// section B of the payload golden uses, so a failure is looked up there.
    pub(crate) fn label(&self) -> String {
        match self.target {
            Some((seq, kind)) => format!("#{seq} {kind}"),
            None => self.symbol.name().to_string(),
        }
    }

    /// Declared beside exported as the comparison sees them, or `None` where the export
    /// refused the handle.
    pub(crate) fn declared_vs_exported(&self) -> Option<(Vec<Column>, Vec<Column>)> {
        let exported = self.exported.as_ref().ok()?;
        Some((columns(&self.declared), columns(exported)))
    }
}

/// A column as the comparison sees it: name and type, with the two things the exporter
/// rewrites set aside. Nullability is not carried, and a decimal reads at its maximum
/// precision on both sides — the value the exporter writes whatever cuDF held — so only
/// its scale can differ.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Column {
    pub(crate) name: String,
    pub(crate) data_type: DataType,
}

pub(crate) fn columns(schema: &ArrowSchema) -> Vec<Column> {
    schema
        .fields()
        .iter()
        .map(|field| Column {
            name: field.name().clone(),
            data_type: match field.data_type() {
                DataType::Decimal128(_, scale) => {
                    DataType::Decimal128(DECIMAL128_MAX_PRECISION, *scale)
                }
                DataType::Decimal256(_, scale) => {
                    DataType::Decimal256(DECIMAL256_MAX_PRECISION, *scale)
                }
                other => other.clone(),
            },
        })
        .collect()
}

pub(crate) async fn context(
    target_partitions: usize,
) -> datafusion::execution::context::SessionContext {
    peacockdb_core::register_tables_for(
        peacockdb_core::build_session_state(target_partitions),
        &data_dir_for("tpch", "1"),
    )
    .await
    .expect("register the tpch sf1 tables")
}

/// The query planned in the engine at these knobs, with its recipe plan attached.
pub(crate) async fn plan_recipes(sql: &str, knobs: PlanKnobs) -> (Box<dyn GpuNode>, RecipePlan) {
    let ctx = context(knobs.target_partitions).await;
    let plan = ctx
        .sql(sql)
        .await
        .expect("datafusion plans it")
        .create_physical_plan()
        .await
        .expect("datafusion lowers it");
    let (tree, _) = planner::plan(&plan, knobs).expect("this mode plans it");
    let recipes = attach_recipes(tree.as_ref()).expect("a planned tree has recipes");
    (tree, recipes)
}

/// Plan the query in the engine, hand the recipe plan to a device, and make the calls.
pub(crate) async fn walk(sql: &str, knobs: PlanKnobs) -> Walked {
    let (tree, recipes) = plan_recipes(sql, knobs).await;
    let session = Session::open(&recipes);
    let mut walk = Walk {
        session: &session,
        recipes: &recipes,
        next_node: 0,
        exported: Vec::new(),
        made: Vec::new(),
        firings: Vec::new(),
    };
    let left = walk.node(tree.as_ref());
    assert!(left.is_empty(), "the sink answered with resident handles");
    assert_eq!(
        walk.next_node,
        recipes.nodes(),
        "the walk visited {} of the plan's {} nodes",
        walk.next_node,
        recipes.nodes()
    );
    Walked {
        batches: walk.exported,
        calls: walk.made,
        firings: walk.firings,
    }
}

/// Two schemas that differ only in what the exporter rewrites — decimal precision, which
/// it writes as 38 whatever cuDF held, and nullability, which it derives from the data —
/// compare equal; a scale, a type or a name that differs still shows.
#[test]
fn columns_set_precision_and_nullability_aside() {
    let declared = ArrowSchema::new(vec![
        Field::new("a", DataType::Decimal128(15, 2), false),
        Field::new("b", DataType::Utf8, true),
    ]);
    let exported = ArrowSchema::new(vec![
        Field::new("a", DataType::Decimal128(38, 2), true),
        Field::new("b", DataType::Utf8, false),
    ]);
    assert_eq!(columns(&declared), columns(&exported));

    let one = |name: &str, data_type: DataType| {
        columns(&ArrowSchema::new(vec![Field::new(name, data_type, true)]))
    };
    assert_ne!(
        one("a", DataType::Decimal128(15, 2)),
        one("a", DataType::Decimal128(38, 3))
    );
    assert_ne!(one("b", DataType::Utf8View), one("b", DataType::Utf8));
    assert_ne!(one("b", DataType::Utf8), one("c", DataType::Utf8));
}

/// The aggregate shape fires calls of both kinds: the scan, the coalesce-all and the export
/// are declared; the aggregates, the repartition and the finalize are not. Each firing of a
/// declared call is measured once; an undeclared one fires and is not — the arms
/// `declared-schemas-derived.md` takes.
#[tokio::test]
async fn every_firing_of_a_declared_call_is_measured_and_no_undeclared_one_is() {
    let walked = walk(super::SUM_BY_FLAG, super::TWO_LANES).await;
    let kinds: Vec<Option<FbKind>> = walked
        .firings
        .iter()
        .map(|firing| firing.target.map(|(_, kind)| kind))
        .collect();
    let count = |kind: Option<FbKind>| kinds.iter().filter(|seen| **seen == kind).count();
    assert_eq!(count(Some(FbKind::Scan)), 2, "one scan per lane");
    assert_eq!(
        count(Some(FbKind::CoalescePartitions)),
        1,
        "the coalesce-all once at done; the aggregate's own concats are undeclared"
    );
    assert_eq!(count(None), 2, "the export, once per handle at the sink");
    assert_eq!(
        walked.firings.len(),
        5,
        "nothing else is declared in this shape"
    );
    let fired = |kind: FbKind| walked.calls.iter().any(|(_, made)| *made == kind);
    assert!(
        fired(FbKind::Aggregate { merge: true }) && fired(FbKind::Repartition { lanes: 2 }),
        "the undeclared arms fired: {:?}",
        walked.calls
    );
    for firing in &walked.firings {
        assert!(
            firing.exported.is_ok(),
            "{} refused the export: {:?}",
            firing.label(),
            firing.exported
        );
    }
}

/// A filter matching nothing hands the sink an empty table, and the export still carries
/// its schema: the walk has no rows assumption, so a zero-row query is measured like any
/// other. The predicate is arithmetic so that row-group pruning cannot see through it —
/// `n_nationkey < 0` prunes every row group and the planner refuses the scan (#209).
#[tokio::test]
async fn a_query_selecting_no_rows_is_walked_and_measured() {
    let walked = walk(
        "SELECT n_name FROM nation WHERE n_nationkey + 100 < 0",
        super::ONE_LANE,
    )
    .await;
    assert_eq!(total_rows(&walked.batches), 0);
    assert_eq!(
        walked.firings.len(),
        3,
        "the scan, the filter and the export"
    );
    for firing in &walked.firings {
        let (declared, exported) = firing
            .declared_vs_exported()
            .unwrap_or_else(|| panic!("{} refused the export", firing.label()));
        assert_eq!(declared.len(), exported.len(), "{}", firing.label());
    }
}

/// A handle the device refuses to export is the finding, returned rather than unwrapped, so
/// a case can name the capability gap instead of the harness failing on it.
#[tokio::test]
async fn a_refused_export_is_returned_rather_than_panicking() {
    let (_tree, recipes) = plan_recipes(super::BARE_SCAN, super::ONE_LANE).await;
    let session = Session::open(&recipes);
    let refused = session
        .exported_schema(u64::MAX)
        .expect_err("no handle was ever produced under this number");
    assert!(refused.contains("unknown handle"), "{refused}");
}
