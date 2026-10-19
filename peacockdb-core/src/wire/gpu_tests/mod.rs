//! A recipe plan on a live GPU, driven by hand: the first time anything the engine plans
//! meets a device.
//!
//! One helper walks the tree making exactly the calls each recipe names, threading every
//! output handle into the next call's input and exporting at the root; one test per query,
//! so a failure names the query rather than a stage. No driver and no scheduling — every
//! shape here plans one batch per lane, so a recipe's own call order is the schedule.
//!
//! The oracle is DataFusion on the same SQL: a golden would pin a wrong finalize on its
//! first run, and our CPU executor evaluates the very finalize the device is sent.

use datafusion::arrow::array::RecordBatch;
use datafusion::arrow::ipc::reader::StreamReader;

use super::{
    AbiSymbol, Call, CallPattern, FbKind, Input, ProjectRole, Recipe, RecipePlan, Seq,
    attach_recipes, join_scratch_bytes,
};
use crate::executor::{BatchForwarder, forwarder_for};
use crate::plan::GpuNode;
use crate::plan::{ExecutorCategory, category_of};
use crate::plan::{NodeRef, as_node_ref};
use crate::planner;
use crate::planner::{BatchSizing, PlanKnobs};
use peacockdb_ffi::raw::{
    PeacockExecutor, PeacockNodeStats, peacock_executor_begin_plan, peacock_executor_create,
    peacock_executor_destroy, peacock_executor_end_plan, peacock_executor_execute_node,
    peacock_executor_execute_scan_rowgroups, peacock_join_build, peacock_join_finish,
    peacock_join_probe, peacock_join_release, peacock_last_error, peacock_result_free,
    peacock_result_from_handle,
};

use crate::test_support::{
    DeviceSchema, DeviceType, GPU_BUDGET, TypeId, assert_results_match, data_dir_for, schema_at,
    total_rows,
};

// The value the plan goldens are canonized at, so every shape below is one that tier
// already renders.
use crate::planner::SMALL_TABLE_BYTES;

mod aggregate_init;

/// Everything but the aggregates: one lane and one batch, which makes every recipe a
/// single call per node and the walk a straight line.
const ONE_LANE: PlanKnobs = PlanKnobs {
    target_partitions: 1,
    sizing: BatchSizing::OneBatchPerLane,
    budget: GPU_BUDGET as u64,
    small_table_bytes: SMALL_TABLE_BYTES,
};

/// The aggregates, at one batch and two lanes: a merge is the operator the engine adds and
/// one lane never performs one.
const TWO_LANES: PlanKnobs = PlanKnobs {
    target_partitions: 2,
    sizing: BatchSizing::OneBatchPerLane,
    budget: GPU_BUDGET as u64,
    small_table_bytes: SMALL_TABLE_BYTES,
};

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

    /// One `execute_node`, its input handles grouped by the child slot each fills: each
    /// handle it answered, beside the rows in it.
    fn execute(&self, seq: Seq, inputs: &[Vec<u64>], out_cap: usize) -> Vec<(u64, u64)> {
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
        handles
            .into_iter()
            .zip(stats)
            .take(produced as usize)
            .map(|(handle, stats)| (handle, stats.rows))
            .collect()
    }

    /// A join session's build, over `build` or over nothing (0): the session's id.
    fn join_build(&self, seq: Seq, build: u64) -> u64 {
        let mut join = 0u64;
        let mut stats = PeacockNodeStats::default();
        let rc = unsafe {
            peacock_join_build(self.executor, seq as u64, build, &mut join, &mut stats)
        };
        assert_eq!(rc, 0, "join_build(#{seq}) failed: {}", self.last_error());
        join
    }

    /// A probe or a finish: the handle it answered, 0 where it answered nothing.
    fn join_call(&self, join: u64, probe: Option<u64>) -> u64 {
        let mut handle = 0u64;
        let mut stats = PeacockNodeStats::default();
        let rc = unsafe {
            match probe {
                Some(batch) => {
                    peacock_join_probe(self.executor, join, batch, &mut handle, &mut stats)
                }
                None => peacock_join_finish(self.executor, join, &mut handle, &mut stats),
            }
        };
        assert_eq!(rc, 0, "a call on join {join} failed: {}", self.last_error());
        handle
    }

    fn join_release(&self, join: u64) {
        unsafe { peacock_join_release(self.executor, join) }
    }

    /// The whole handle across the boundary. No shape here plans a limit, so the sink's
    /// range is always the batch it is handed; nothing here declares a precision.
    fn export(&self, handle: u64) -> Vec<RecordBatch> {
        let mut ipc: *mut u8 = std::ptr::null_mut();
        let mut len = 0u64;
        let rc = unsafe {
            peacock_result_from_handle(
                self.executor,
                handle,
                0,
                u64::MAX,
                std::ptr::null(),
                0,
                &mut ipc,
                &mut len,
            )
        };
        assert_eq!(rc, 0, "result_from_handle failed: {}", self.last_error());
        if len == 0 {
            return Vec::new();
        }
        let bytes = unsafe { std::slice::from_raw_parts(ipc, len as usize) };
        let batches = StreamReader::try_new(std::io::Cursor::new(bytes), None)
            .and_then(|stream| stream.collect::<Result<Vec<_>, _>>())
            .expect("the exported IPC stream decodes");
        unsafe { peacock_result_free(ipc) };
        batches
    }

    /// What the device holds at `handle`, read where it sits: an intermediate has no
    /// declaration, so nothing is told and a decimal reads at its scale alone.
    fn schema_of(&self, handle: u64) -> DeviceSchema {
        schema_at(self.executor, handle)
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
    lane: Vec<u64>,
    all_lanes: Vec<u64>,
    prior: Option<u64>,
}

/// Where a call's named input comes from, in the handles the walk is already holding.
fn resolve(input: Input, at: &At) -> Vec<u64> {
    let held = |handle: Option<u64>| vec![handle.unwrap_or_else(|| panic!("no {input:?} here"))];
    match input {
        Input::Batch => held(at.batch),
        Input::PriorOutput => held(at.prior),
        Input::LaneBatches => at.lane.clone(),
        Input::AllLanes => at.all_lanes.clone(),
        Input::RowGroups | Input::RowRange => {
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
    /// Told every call `make` made and the handles it answered, while they are resident and
    /// before the next call consumes them — the one moment an intermediate can be read.
    on_call: &'a mut dyn FnMut(Seq, FbKind, &[u64], &Session),
    /// The rows in every handle a call answered, which is what an init's done call reads.
    rows: std::collections::HashMap<u64, u64>,
    /// Calls handed no input at all: an init's done calls.
    unfed: usize,
}

/// The hook that reads nothing.
fn no_hook(_: Seq, _: FbKind, _: &[u64], _: &Session) {}

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
        // An aggregate's init, whichever category drives it: `aggregate_init.rs`.
        if let NodeRef::Aggregate(_) = as_node_ref(node) {
            return self.aggregate_init(node, recipe, &kids[0]);
        }
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
        if inputs.is_empty() {
            self.unfed += 1;
        }
        self.made.push((seq, kind));
        let answered = self.session.execute(seq, &inputs, out_cap);
        let handles: Vec<u64> = answered.iter().map(|(handle, _)| *handle).collect();
        self.rows.extend(answered);
        (self.on_call)(seq, kind, &handles, self.session);
        handles
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
                batches.push(self.session.scan(seq, row_groups));
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
            // A lane that got no batch makes no call at done and emits nothing, as every
            // device accumulator but an aggregate's init does (`aggregate_init.rs`): a
            // collapse of no handles is refused.
            if held.is_empty() {
                lanes.push(Vec::new());
                continue;
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

    /// A session per lane: built over the lane's build batch, probed with every probe
    /// batch, finished where the recipe says so. Each answer that is a handle is the lane's
    /// next batch; a probe of the build-side semi family answers none.
    fn join(&mut self, node: &dyn GpuNode, recipe: &Recipe, build: &Lanes, probe: &Lanes) -> Lanes {
        assert_eq!(
            build.len(),
            probe.len(),
            "{}: lane p of one side must hold what can match lane p of the other",
            node.name()
        );
        let seq = match recipe.calls.first().map(|call| (call.symbol, call.target)) {
            Some((AbiSymbol::JoinBuild, Some((seq, FbKind::Join)))) => seq,
            other => panic!("{}: a join's recipe opens with join_build, not {other:?}", node.name()),
        };
        let finishes = recipe.calls.iter().any(|call| call.symbol == AbiSymbol::JoinFinish);
        let mut lanes = Vec::with_capacity(build.len());
        for (build_lane, probe_lane) in build.iter().zip(probe) {
            let build_handle = match build_lane.as_slice() {
                [] => 0,
                [one] => *one,
                more => panic!("{}: {} build batches in one lane", node.name(), more.len()),
            };
            let join = self.session.join_build(seq, build_handle);
            self.made.push((seq, FbKind::Join));
            (self.on_call)(seq, FbKind::Join, &[], self.session);
            let mut out = Vec::new();
            let answers: Vec<Option<u64>> = probe_lane
                .iter()
                .map(|batch| Some(*batch))
                .chain(finishes.then_some(None))
                .collect();
            for answer in answers {
                let handle = self.session.join_call(join, answer);
                self.made.push((seq, FbKind::Join));
                let handles: Vec<u64> = (handle != 0).then_some(handle).into_iter().collect();
                (self.on_call)(seq, FbKind::Join, &handles, self.session);
                out.extend(handles);
            }
            self.session.join_release(join);
            lanes.push(out);
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
                let batches = self.session.export(*handle);
                self.exported.extend(batches);
            }
        }
        Vec::new()
    }
}

/// A recipe's calls split into the two phases a walk drives: what runs as batches arrive,
/// and what runs once the lane is complete. A compaction runs exactly what done runs, so
/// at one batch per lane the two are the same call. `AtDoneIfNothingOut` is on the done
/// side, which the init's walk runs only where nothing went out.
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
struct Walked {
    batches: Vec<RecordBatch>,
    calls: Vec<(Seq, FbKind)>,
    /// Calls handed no input: an init's done calls.
    unfed: usize,
}

/// The calls in the order they were made, `#14 CudfAggregate{Merge}` each. Three of the
/// four defects this file found were found by whoever had just written the code and the
/// fourth by a grep; neither is available to whoever runs it next, and a wrong table names
/// no call on its own.
fn trail(calls: &[(Seq, FbKind)]) -> String {
    calls
        .iter()
        .map(|(seq, kind)| format!("#{seq} {kind}"))
        .collect::<Vec<_>>()
        .join(", ")
}

async fn context(target_partitions: usize) -> datafusion::execution::context::SessionContext {
    peacockdb_core::register_tables_for(
        peacockdb_core::build_session_state(target_partitions),
        &data_dir_for("tpch", "1"),
    )
    .await
    .expect("register the tpch sf1 tables")
}

/// Plan the query in the engine, hand the recipe plan to a device, and make the calls.
async fn walk(sql: &str, knobs: PlanKnobs) -> Walked {
    walk_with(sql, knobs, &mut no_hook).await
}

/// `walk`, with `on_call` told every call as it is made.
async fn walk_with(
    sql: &str,
    knobs: PlanKnobs,
    on_call: &mut dyn FnMut(Seq, FbKind, &[u64], &Session),
) -> Walked {
    let ctx = context(knobs.target_partitions).await;
    let plan = ctx
        .sql(sql)
        .await
        .expect("datafusion plans it")
        .create_physical_plan()
        .await
        .expect("datafusion lowers it");
    let (tree, _) = planner::plan(&plan, knobs).expect("this mode plans it");
    let recipes = attach_recipes(tree.as_ref(), join_scratch_bytes(&knobs))
        .expect("a planned tree has recipes");
    let session = Session::open(&recipes);
    let mut walk = Walk {
        session: &session,
        recipes: &recipes,
        next_node: 0,
        exported: Vec::new(),
        made: Vec::new(),
        on_call,
        rows: Default::default(),
        unfed: 0,
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
        unfed: walk.unfed,
    }
}

/// The walk's answer against DataFusion's on the same SQL, compared as sorted multisets
/// since a GPU join's output order is not deterministic. Returns the calls it made.
async fn assert_walk_matches_datafusion(sql: &str, knobs: PlanKnobs) -> Vec<(Seq, FbKind)> {
    assert_walk_matches_datafusion_with(sql, knobs, &mut no_hook)
        .await
        .calls
}

async fn assert_walk_matches_datafusion_with(
    sql: &str,
    knobs: PlanKnobs,
    on_call: &mut dyn FnMut(Seq, FbKind, &[u64], &Session),
) -> Walked {
    let walked = walk_with(sql, knobs, on_call).await;
    // An exact compare of two empty results holds having compared nothing, so a query whose
    // predicate selected none would prove only that the walk did not crash.
    assert!(
        total_rows(&walked.batches) > 0,
        "the walk exported no rows for {sql}"
    );
    let expected = context(1)
        .await
        .sql(sql)
        .await
        .expect("the oracle plans it")
        .collect()
        .await
        .expect("the oracle runs it");
    assert_results_match(
        &expected,
        &walked.batches,
        None,
        &format!(
            "{sql}\n  the walk made: {}\n  and its",
            trail(&walked.calls)
        ),
    );
    walked
}

fn times(calls: &[(Seq, FbKind)], kind: FbKind) -> usize {
    calls.iter().filter(|(_, made)| *made == kind).count()
}

/// Finalize projects whose call directly before them is `kind`. Which aggregate a finalize
/// belongs to is not in the counts — both branches emit one — so the adjacency is what
/// separates an init that finalized itself from a merge that did.
fn finalizing(calls: &[(Seq, FbKind)], kind: FbKind) -> usize {
    calls
        .windows(2)
        .filter(|pair| pair[0].1 == kind && pair[1].1 == FbKind::Project(ProjectRole::Finalize))
        .count()
}

/// The queries, named so the coverage read at the end of the file is over the very set the
/// tests above run rather than a second list that could drift from it.
const BARE_SCAN: &str = "SELECT * FROM nation";
const FILTER: &str = "SELECT o_orderkey, o_totalprice FROM orders WHERE o_totalprice > 500000";
const PROJECT_OVER_FILTER: &str =
    "SELECT c_custkey * 2 AS doubled FROM customer WHERE c_nationkey = 3";
const INNER_JOIN: &str =
    "SELECT n_name, r_name FROM nation JOIN region ON n_regionkey = r_regionkey";
const SEMI_JOIN: &str = "SELECT c_custkey FROM customer c WHERE c_nationkey = 3 AND EXISTS \
     (SELECT 1 FROM orders o WHERE o.o_custkey = c.c_custkey AND o.o_totalprice > c.c_acctbal)";
const SUM_BY_FLAG: &str =
    "SELECT l_returnflag, sum(l_quantity) FROM lineitem GROUP BY l_returnflag";
const AVG_BY_FLAG: &str =
    "SELECT l_returnflag, avg(l_quantity) FROM lineitem GROUP BY l_returnflag";
const MAX_OF_SUMS: &str = "SELECT max(per_flag) FROM \
     (SELECT l_returnflag, sum(l_quantity) AS per_flag FROM lineitem GROUP BY l_returnflag)";
const ROLLUP: &str = "SELECT l_returnflag, l_linestatus, sum(l_quantity) FROM lineitem \
     GROUP BY ROLLUP(l_returnflag, l_linestatus)";
const ROLLUP_GROUPING: &str = "SELECT n_regionkey, n_nationkey, \
     grouping(n_regionkey, n_nationkey) AS g, count(*) FROM nation \
     GROUP BY ROLLUP (n_regionkey, n_nationkey)";
const DISTINCT_UNDER_A_WIDE_ROLLUP: &str = "SELECT l_orderkey, l_partkey, l_linenumber, \
     l_returnflag, l_linestatus, l_shipinstruct, l_shipmode, l_comment, \
     grouping(l_orderkey, l_partkey, l_linenumber, l_returnflag, l_linestatus, l_shipinstruct, \
     l_shipmode, l_comment) AS g, \
     count(DISTINCT l_suppkey), count(*) FROM lineitem WHERE l_orderkey <= 7 \
     GROUP BY ROLLUP (l_orderkey, l_partkey, l_linenumber, l_returnflag, l_linestatus, \
     l_shipinstruct, l_shipmode, l_comment)";
const SUM_OF_QUOTIENTS: &str = "SELECT sum(per_flag) FROM (SELECT l_returnflag, \
     sum(l_extendedprice / l_linenumber) AS per_flag FROM lineitem GROUP BY l_returnflag)";
const DISTINCT_BESIDE_COMPANIONS: &str = "SELECT l_returnflag, count(DISTINCT l_suppkey), \
     avg(l_quantity), count(*) FROM lineitem GROUP BY l_returnflag";

#[tokio::test]
async fn a_bare_scan_comes_back_as_the_table() {
    assert_walk_matches_datafusion(BARE_SCAN, ONE_LANE).await;
}

#[tokio::test]
async fn a_filter_keeps_the_rows_its_predicate_names() {
    assert_walk_matches_datafusion(FILTER, ONE_LANE).await;
}

#[tokio::test]
async fn a_project_over_a_filter_evaluates_on_what_the_filter_left() {
    assert_walk_matches_datafusion(PROJECT_OVER_FILTER, ONE_LANE).await;
}

#[tokio::test]
async fn an_inner_join_matches_the_oracle_as_a_multiset() {
    assert_walk_matches_datafusion(INNER_JOIN, ONE_LANE).await;
}

/// A semi join whose residual the AST cannot take — `o_totalprice > c_acctbal` over two
/// decimals — is one session like any other: built, probed once, and finished, where its
/// answer is.
#[tokio::test]
async fn a_semi_join_with_a_residual_the_ast_cannot_take_is_one_session() {
    let calls = assert_walk_matches_datafusion(SEMI_JOIN, ONE_LANE).await;
    assert_eq!(
        times(&calls, FbKind::Join),
        3,
        "build, one probe batch and the finish: {}",
        trail(&calls)
    );
}

/// Two lanes each merge their own state, the cross-lane merge folds them, and the finalize
/// runs once per lane at done — the first time `AggregateMode::Merge` meets a device.
#[tokio::test]
async fn each_lane_merges_its_own_state_before_the_cross_lane_merge_folds_them() {
    let calls = assert_walk_matches_datafusion(SUM_BY_FLAG, TWO_LANES).await;
    assert_eq!(
        times(&calls, FbKind::Aggregate { merge: true }),
        4,
        "two lanes below the shuffle and two above it merge state: {}",
        trail(&calls)
    );
    assert_eq!(
        times(&calls, FbKind::Project(ProjectRole::Finalize)),
        2,
        "the finalize runs once per lane, at done: {}",
        trail(&calls)
    );
}

/// q66's shape (#55). DataFusion casts the divisor of `sum(decimal / int)` in the partial
/// alone; a merge that evaluated the argument again over state would find no such column, or
/// divide twice. The inner sum merges on each lane and across them; the outer folds its two
/// partials in one merge.
#[tokio::test]
async fn a_summed_quotient_divides_once_in_the_partial_and_merges_by_reference() {
    let calls = assert_walk_matches_datafusion(SUM_OF_QUOTIENTS, TWO_LANES).await;
    assert_eq!(
        times(&calls, PARTIAL),
        4,
        "an init per lane for each sum: {}",
        trail(&calls)
    );
    assert_eq!(
        times(&calls, MERGE),
        5,
        "the inner sum's four merges and the outer's one: {}",
        trail(&calls)
    );
    assert_eq!(
        times(&calls, FINALIZE),
        3,
        "two inner finalizes, one outer: {}",
        trail(&calls)
    );
}

/// The DISTINCT lowering (#262). An inner stage grouping on the argument and the key runs
/// the companions' inits. The outer stage's init counts the deduplicated argument and runs
/// `avg`'s and `count`'s merge rules over the inner's state. DataFusion 45 answers these
/// shapes right; `avg` and `stddev` DISTINCT it does not.
#[tokio::test]
async fn a_distinct_count_beside_its_companions_runs_both_stages_on_a_device() {
    let calls = assert_walk_matches_datafusion(DISTINCT_BESIDE_COMPANIONS, TWO_LANES).await;
    assert_eq!(
        times(&calls, PARTIAL),
        4,
        "two stages, each its init on both lanes: {}",
        trail(&calls)
    );
    assert_eq!(
        times(&calls, MERGE),
        6,
        "the inner stage's four merges and the outer's two: {}",
        trail(&calls)
    );
    assert_eq!(
        times(&calls, FINALIZE),
        2,
        "the outer stage finalizes once per lane: {}",
        trail(&calls)
    );
}

/// `avg` finalizes by dividing a decimal by a count, and cuDF takes a divide's result
/// scale from its operands where arrow takes it from the declared output type — so a wrong
/// cast is invisible on a CPU host and wrong here, in a column whose type reads correctly
/// either way. The compare is on the rendered digits.
#[tokio::test]
async fn an_average_finalizes_to_the_digits_the_oracle_computes() {
    let calls = assert_walk_matches_datafusion(AVG_BY_FLAG, TWO_LANES).await;
    assert_eq!(
        times(&calls, FbKind::Project(ProjectRole::Finalize)),
        2,
        "the divide never reached the device: {}",
        trail(&calls)
    );
}

/// Two aggregates, one per branch of the rule. The inner one groups the whole table and
/// its lane holds many batches, so it takes the merge route and the merge carries the
/// finalize. The outer `max` reads that merge's one output batch, which is already the
/// whole of its single group, so the translation hands the finalize to the init itself and
/// no merge is built for it — the shape the goldens hold 39 of and no device had run.
///
/// The adjacency is the claim, not the counts: both branches emit one init and one
/// finalize, so a plan that lost the self-finalizing arm would still show two of each.
#[tokio::test]
async fn an_aggregate_whose_input_is_one_batch_finalizes_without_a_merge() {
    let calls = assert_walk_matches_datafusion(MAX_OF_SUMS, ONE_LANE).await;
    assert_eq!(
        finalizing(&calls, FbKind::Aggregate { merge: false }),
        1,
        "no init finalized itself, so the single-batch arm never ran: {}",
        trail(&calls)
    );
    assert_eq!(
        finalizing(&calls, FbKind::Aggregate { merge: true }),
        1,
        "the inner aggregate's merge stopped carrying its finalize: {}",
        trail(&calls)
    );
}

/// A ROLLUP is the one shape whose payload is not derivable from the plan line: the masks
/// and the per-position NULL placeholders decide how many groups come back. Dropping them
/// made this query refuse on the device rather than answer wrongly, because the state then
/// carried no `__grouping_id` and the merge read a state column as a key — a width
/// coincidence of this shape, not a property of the class. Where the widths line up, the
/// same omission answers with one grouping set out of three and looks right.
#[tokio::test]
async fn a_rollup_answers_with_every_grouping_set() {
    let calls = assert_walk_matches_datafusion(ROLLUP, ONE_LANE).await;
    assert_eq!(
        times(&calls, FbKind::Aggregate { merge: false }),
        1,
        "the sets are expanded once, by the init: {}",
        trail(&calls)
    );
}

/// `GROUPING()` over every key plans as the grouping id cast to Int32, so the id's value is
/// the answer here rather than a tag that only keeps the sets apart.
#[tokio::test]
async fn a_grouping_over_every_key_answers_datafusions_id() {
    assert_walk_matches_datafusion(ROLLUP_GROUPING, TWO_LANES).await;
}

// What the device holds between calls, read by hand. An intermediate has no declaration
// to be held to, so every expectation below was written from the node line of the query's
// plan, rendered by `render_plan` rust-only, before any ran on a device; one the device
// contradicts becomes a `bug_` with a ticket, never a rewritten expectation.

/// Every handle a call answered and the schema the device held at it, in call order; a
/// scatter contributes one entry per lane. The answer is still held to the oracle.
async fn held(sql: &str, knobs: PlanKnobs) -> Vec<(FbKind, DeviceSchema)> {
    let mut seen = Vec::new();
    let mut read = |_: Seq, kind: FbKind, handles: &[u64], session: &Session| {
        for handle in handles {
            seen.push((kind, session.schema_of(*handle)));
        }
    };
    assert_walk_matches_datafusion_with(sql, knobs, &mut read).await;
    seen
}

/// The schemas held after every call of `kind`, in call order.
fn after(held: &[(FbKind, DeviceSchema)], kind: FbKind) -> Vec<&DeviceSchema> {
    held.iter()
        .filter(|(made, _)| *made == kind)
        .map(|(_, schema)| schema)
        .collect()
}

fn column(name: &str, id: TypeId) -> (String, DeviceType) {
    (name.to_string(), DeviceType { id, scale: None })
}

fn decimal(name: &str, scale: i32) -> (String, DeviceType) {
    (
        name.to_string(),
        DeviceType {
            id: TypeId::Decimal128,
            scale: Some(scale),
        },
    )
}

const PARTIAL: FbKind = FbKind::Aggregate { merge: false };
const MERGE: FbKind = FbKind::Aggregate { merge: true };
const FINALIZE: FbKind = FbKind::Project(ProjectRole::Finalize);

/// `avg`'s state as `decomposition(Avg)` orders it, `[$sum, $count]`, under the key.
fn avg_state() -> DeviceSchema {
    DeviceSchema(vec![
        column("l_returnflag", TypeId::String),
        decimal("avg(lineitem.l_quantity)$sum", 2),
        column("avg(lineitem.l_quantity)$count", TypeId::Int64),
    ])
}

#[tokio::test]
async fn an_avg_partial_holds_a_string_key_a_scale_2_sum_and_an_int64_count() {
    let held = held(AVG_BY_FLAG, TWO_LANES).await;
    assert_eq!(after(&held, PARTIAL), vec![&avg_state(); 2]);
}

#[tokio::test]
async fn an_avg_merge_holds_the_partials_state_unchanged() {
    let held = held(AVG_BY_FLAG, TWO_LANES).await;
    assert_eq!(after(&held, MERGE), vec![&avg_state(); 4]);
}

#[tokio::test]
async fn an_avg_finalize_holds_a_string_key_and_a_scale_6_average() {
    let held = held(AVG_BY_FLAG, TWO_LANES).await;
    let expected = DeviceSchema(vec![
        column("l_returnflag", TypeId::String),
        decimal("avg(lineitem.l_quantity)", 6),
    ]);
    assert_eq!(after(&held, FINALIZE), vec![&expected; 2]);
}

#[tokio::test]
async fn a_sum_partial_holds_a_string_key_and_a_scale_2_sum() {
    let held = held(SUM_BY_FLAG, TWO_LANES).await;
    let expected = DeviceSchema(vec![
        column("l_returnflag", TypeId::String),
        decimal("sum(lineitem.l_quantity)", 2),
    ]);
    assert_eq!(after(&held, PARTIAL), vec![&expected; 2]);
}

#[tokio::test]
async fn a_rollup_partial_holds_the_uint8_grouping_id_the_plan_declares() {
    let held = held(ROLLUP, ONE_LANE).await;
    let expected = DeviceSchema(vec![
        column("l_returnflag", TypeId::String),
        column("l_linestatus", TypeId::String),
        column("__grouping_id", TypeId::UInt8),
        decimal("sum(lineitem.l_quantity)", 2),
    ]);
    assert_eq!(after(&held, PARTIAL), vec![&expected]);
}

/// Eight keys and the DISTINCT argument make nine, so both stages carry a `UInt16` id where
/// DataFusion's over the eight is `UInt8`, and a project narrows it above the outer stage.
/// The argument is the first key and never masked, so its bit, the highest, is 0 and the
/// narrowed value is DataFusion's; `g`, `GROUPING()` over the eight, puts it in the answer
/// `held` holds to the oracle.
#[tokio::test]
async fn a_distinct_under_an_eight_key_rollup_narrows_a_uint16_id_to_datafusions() {
    let held = held(DISTINCT_UNDER_A_WIDE_ROLLUP, ONE_LANE).await;
    let ids: Vec<TypeId> = after(&held, PARTIAL)
        .iter()
        .map(|schema| {
            schema
                .0
                .iter()
                .find(|(name, _)| name == "__grouping_id")
                .map(|(_, held)| held.id)
                .expect("each stage's init carries the id")
        })
        .collect();
    assert_eq!(ids, vec![TypeId::UInt16, TypeId::UInt16]);
}

#[tokio::test]
async fn a_filter_holds_the_one_column_its_projection_keeps() {
    let held = held(PROJECT_OVER_FILTER, ONE_LANE).await;
    let expected = DeviceSchema(vec![column("c_custkey", TypeId::Int64)]);
    assert_eq!(after(&held, FbKind::Filter), vec![&expected]);
}

#[tokio::test]
async fn a_project_holds_the_int64_it_computed() {
    let held = held(PROJECT_OVER_FILTER, ONE_LANE).await;
    let expected = DeviceSchema(vec![column("doubled", TypeId::Int64)]);
    assert_eq!(after(&held, FbKind::PlainProject), vec![&expected]);
}

/// DataFusion builds on `region`, and the join's projection keeps neither key; the
/// project above the join is what restores the SELECT's order.
#[tokio::test]
async fn an_inner_join_holds_its_two_projected_strings_build_side_first() {
    let held = held(INNER_JOIN, ONE_LANE).await;
    let expected = DeviceSchema(vec![
        column("r_name", TypeId::String),
        column("n_name", TypeId::String),
    ]);
    assert_eq!(after(&held, FbKind::Join), vec![&expected], "the probe's one answer");
}

/// The inner aggregate's finalize emits its key and its sum under the aggregate's own
/// names; `per_flag` is the plain project above it.
#[tokio::test]
async fn the_inner_finalize_of_nested_aggregates_holds_the_key_and_the_scale_2_sum() {
    let held = held(MAX_OF_SUMS, ONE_LANE).await;
    let expected = DeviceSchema(vec![
        column("l_returnflag", TypeId::String),
        decimal("sum(lineitem.l_quantity)", 2),
    ]);
    let finalizes = after(&held, FINALIZE);
    assert_eq!(finalizes.len(), 2, "one finalize per aggregate");
    assert_eq!(finalizes[0], &expected);
}

/// `max` keeps its input's type, `state_type(Max)`: the outer init is the second partial.
#[tokio::test]
async fn the_outer_max_holds_one_scale_2_decimal() {
    let held = held(MAX_OF_SUMS, ONE_LANE).await;
    let expected = DeviceSchema(vec![decimal("max(per_flag)", 2)]);
    let partials = after(&held, PARTIAL);
    assert_eq!(partials.len(), 2, "one init per aggregate");
    assert_eq!(partials[1], &expected);
}

#[tokio::test]
async fn a_semi_join_holds_the_build_sides_int64_key() {
    let held = held(SEMI_JOIN, ONE_LANE).await;
    let expected = DeviceSchema(vec![column("c_custkey", TypeId::Int64)]);
    assert_eq!(after(&held, FbKind::Join), vec![&expected], "the finish's one answer");
}

/// Which fb kinds a device has now run, and which this walk refuses — the set T15 and T16
/// inherit as already proven, and the one they inherit as still open.
#[derive(Debug, PartialEq, Eq)]
enum Driven {
    Handled,
    /// Why the walk cannot make this call, in the words its panic uses.
    Refused(&'static str),
}

/// A match rather than a list: a kind added to the mapping stops this compiling, where a
/// list would go quietly short. The two facts a reader wants are here and nowhere else —
/// the spec's prose says the same thing and cannot go red.
fn driven(kind: FbKind) -> Driven {
    match kind {
        FbKind::Scan
        | FbKind::Filter
        | FbKind::PlainProject
        | FbKind::CoalescePartitions
        | FbKind::Repartition { .. }
        | FbKind::Aggregate { .. }
        | FbKind::Project(ProjectRole::Finalize)
        | FbKind::Join => Driven::Handled,
        FbKind::Sort | FbKind::SortPreservingMerge => Driven::Refused("no shape here plans a sort"),
    }
}

/// The kinds the queries above put on a device. Checked against the walk both ways: a kind
/// here that no query produces is a claim on paper, and a kind produced that is not here is
/// an arm that quietly gained a shape.
const PROVEN: [FbKind; 9] = [
    FbKind::Scan,
    FbKind::Filter,
    FbKind::PlainProject,
    FbKind::CoalescePartitions,
    FbKind::Repartition { lanes: 2 },
    FbKind::Aggregate { merge: false },
    FbKind::Aggregate { merge: true },
    FbKind::Project(ProjectRole::Finalize),
    FbKind::Join,
];

#[tokio::test]
async fn the_kinds_a_device_has_run_are_the_kinds_this_file_claims() {
    let mut made: Vec<FbKind> = Vec::new();
    for (sql, knobs) in [
        (BARE_SCAN, ONE_LANE),
        (FILTER, ONE_LANE),
        (PROJECT_OVER_FILTER, ONE_LANE),
        (INNER_JOIN, ONE_LANE),
        (SEMI_JOIN, ONE_LANE),
        (MAX_OF_SUMS, ONE_LANE),
        (ROLLUP, ONE_LANE),
        (DISTINCT_UNDER_A_WIDE_ROLLUP, ONE_LANE),
        (SUM_BY_FLAG, TWO_LANES),
        (AVG_BY_FLAG, TWO_LANES),
        (ROLLUP_GROUPING, TWO_LANES),
        (SUM_OF_QUOTIENTS, TWO_LANES),
        (DISTINCT_BESIDE_COMPANIONS, TWO_LANES),
        (aggregate_init::KEYLESS_OVER_AN_EMPTY_LANE, TWO_LANES),
        (aggregate_init::KEYLESS_SUM, TWO_LANES),
    ] {
        for (_, kind) in walk(sql, knobs).await.calls {
            if !made.contains(&kind) {
                made.push(kind);
            }
        }
    }
    for kind in &made {
        assert_eq!(
            driven(*kind),
            Driven::Handled,
            "{kind} reached a device and is classified as refused"
        );
        assert!(
            PROVEN.contains(kind),
            "{kind} reached a device and is not in PROVEN"
        );
    }
    for kind in PROVEN {
        assert!(
            made.contains(&kind),
            "PROVEN claims {kind} and no query here produces it"
        );
    }
}
