//! Fusing: which scans share, the union load each group reads, what each member reads in place
//! of its own load, and the loop that resolves every held reader of a shared load. The rules
//! are `git show d290698f:llm-wiki/tasks/query-fuser-impl.md`, Task 1.

#[cfg(test)]
mod tests;

use std::collections::{BTreeSet, HashMap};
use std::fmt;
use std::num::NonZeroUsize;
use std::sync::Arc;

use datafusion::arrow::datatypes::Schema as ArrowSchema;

use crate::plan::{Batching, Expr, GpuLoadParquet, GpuNode, GpuProject, NamedExpr, NodeRef};
use crate::plan::{PlanDag, PlanError, Schema, SharedSource, as_node_ref, new_hold_buffer};
use crate::plan::{rebuild, validate_dag};
use crate::planner::{partition, row_group_metadata};

use super::state_under;
use super::{Choice, Chooser, Fused, HeldReader, HoldRecord, Origin, Query, ShareGroup, Unshared};

impl fmt::Display for Unshared {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LimitCut => write!(f, "limit-cut"),
            Self::Nulls { column } => write!(f, "nulls:{column}"),
            Self::RowsOnly => write!(f, "rows-only"),
        }
    }
}

pub(crate) fn fuse(
    queries: &[Query<'_>],
    batching: Batching,
    choose: &mut Chooser<'_>,
) -> Result<Fused, PlanError> {
    if let Batching::Sized { .. } = batching {
        return Err(PlanError::Unsupported(
            "a fused plan under sized batching: the fuser plans tp1_single, one batch per lane"
                .to_string(),
        ));
    }
    let scans = scans_of(queries);
    let (groups, unshared) = share_groups(&scans, batching)?;
    let mut reads_shared: HashMap<Origin, usize> = HashMap::new();
    let mut shared = Vec::new();
    let mut sources = Vec::new();
    for (at, (members, load)) in groups.into_iter().enumerate() {
        for member in &members {
            reads_shared.insert(scans[*member].origin, at);
        }
        shared.push(ShareGroup {
            table: load.table.clone(),
            consumers: members.iter().map(|m| scans[*m].origin.query).collect(),
            members: members.iter().map(|m| scans[*m].origin).collect(),
        });
        sources.push(SharedSource::new(load));
    }
    let roots = queries
        .iter()
        .enumerate()
        .map(|(query, q)| sharing(q.tree, query, &mut 0, &reads_shared, &sources))
        .collect();
    let fused = Fused {
        roots,
        shared,
        unshared,
        holds: Vec::new(),
        reads: Vec::new(),
        origins: Vec::new(),
        originals: queries.iter().map(|q| copy_of(q.tree)).collect(),
    };
    resolve(refreshed(fused), None, choose)
}

pub(crate) fn resolve_cap_holds(
    fused: Fused,
    cap: NonZeroUsize,
    choose: &mut Chooser<'_>,
) -> Result<Fused, PlanError> {
    resolve(fused, Some(cap), choose)
}

/// A load as one query's own tree holds it.
struct Scan<'q> {
    origin: Origin,
    load: &'q GpuLoadParquet,
    /// A limit trimmed its row groups to a covering prefix, so its plan rejects nothing past it.
    limit_cut: bool,
}

/// Every load of every query, the queries in order and each tree in pre-order.
fn scans_of<'q>(queries: &[Query<'q>]) -> Vec<Scan<'q>> {
    fn walk<'q>(
        node: &'q dyn GpuNode,
        cut: bool,
        query: usize,
        next: &mut usize,
        out: &mut Vec<Scan<'q>>,
    ) {
        let origin = Origin { query, node: *next };
        *next += 1;
        if let NodeRef::LoadParquet(load) = as_node_ref(node) {
            out.push(Scan {
                origin,
                load,
                limit_cut: cut,
            });
        }
        let cuts = cuts_its_scan(node);
        for child in node.children() {
            walk(child, cuts, query, next, out);
        }
    }
    let mut out = Vec::new();
    for (query, q) in queries.iter().enumerate() {
        walk(q.tree, false, query, &mut 0, &mut out);
    }
    out
}

/// The two shapes `covering_prefix` leaves above a scan it trimmed.
fn cuts_its_scan(node: &dyn GpuNode) -> bool {
    match as_node_ref(node) {
        NodeRef::Limit(_) => true,
        NodeRef::Unload(unload) => unload.interval.is_some(),
        _ => false,
    }
}

type Groups = Vec<(Vec<usize>, GpuLoadParquet)>;

/// The groups that share, as their members' indices into `scans` with the union load each
/// reads, ordered by first member; and why each scan of a shared file stayed out.
fn share_groups(
    scans: &[Scan<'_>],
    batching: Batching,
) -> Result<(Groups, Vec<(usize, String, Unshared)>), PlanError> {
    let mut files: Vec<(&str, Vec<usize>)> = Vec::new();
    for (at, scan) in scans.iter().enumerate() {
        match files.iter_mut().find(|(file, _)| *file == scan.load.file) {
            Some((_, members)) => members.push(at),
            None => files.push((scan.load.file.as_str(), vec![at])),
        }
    }
    let mut groups = Vec::new();
    let mut unshared = Vec::new();
    let mut refuse = |at: usize, why: Unshared| {
        unshared.push((scans[at].origin.query, scans[at].load.table.clone(), why))
    };
    for (_, members) in files.into_iter().filter(|(_, m)| m.len() > 1) {
        let (cut, whole): (Vec<usize>, Vec<usize>) =
            members.into_iter().partition(|at| scans[*at].limit_cut);
        for at in cut {
            refuse(at, Unshared::LimitCut);
        }
        let (rows, columns): (Vec<usize>, Vec<usize>) = whole
            .into_iter()
            .partition(|at| scans[*at].load.rows_only());
        let both = !rows.is_empty() && !columns.is_empty();
        for kind in [rows, columns] {
            match kind.len() {
                0 => {}
                1 if both => refuse(kind[0], Unshared::RowsOnly),
                1 => {}
                _ => groups.extend(without_gained_nulls(scans, kind, batching, &mut refuse)?),
            }
        }
    }
    groups.sort_by_key(|(members, _)| members[0]);
    Ok((groups, unshared))
}

/// `members` and their union load, less each member whose columns the union would give NULLs
/// its own row groups do not hold: one at a time, the union re-read after each, until none is
/// left to drop. `None` once fewer than two remain.
fn without_gained_nulls(
    scans: &[Scan<'_>],
    mut members: Vec<usize>,
    batching: Batching,
    refuse: &mut impl FnMut(usize, Unshared),
) -> Result<Option<(Vec<usize>, GpuLoadParquet)>, PlanError> {
    while members.len() > 1 {
        let union = union_load(scans, &members, batching)?;
        let gained = members
            .iter()
            .enumerate()
            .find_map(|(at, member)| Some((at, gained_null(scans[*member].load, &union)?)));
        match gained {
            Some((at, column)) => refuse(members.remove(at), Unshared::Nulls { column }),
            None => return Ok(Some((members, union))),
        }
    }
    Ok(None)
}

/// The first of `own`'s columns its row groups hold no NULL in and the union's can.
fn gained_null(own: &GpuLoadParquet, union: &GpuLoadParquet) -> Option<String> {
    let fields = schema_of(own).fields;
    own.projection
        .iter()
        .enumerate()
        .find(|(position, ordinal)| {
            let at = union.projection.iter().position(|o| o == *ordinal);
            !own.can_be_null[*position] && at.is_some_and(|at| union.can_be_null[at])
        })
        .map(|(position, _)| fields.field(position).name().clone())
}

fn union_load(
    scans: &[Scan<'_>],
    members: &[usize],
    batching: Batching,
) -> Result<GpuLoadParquet, PlanError> {
    let loads: Vec<&GpuLoadParquet> = members.iter().map(|at| scans[*at].load).collect();
    union_of(&loads, batching)
}

/// One load of the loads' file over the union of their columns and of their row groups, its
/// metadata read again so its bytes and NULLs are the union's, and its batches cut again.
fn union_of(loads: &[&GpuLoadParquet], batching: Batching) -> Result<GpuLoadParquet, PlanError> {
    let projection: Vec<u32> = loads
        .iter()
        .flat_map(|load| load.projection.iter().copied())
        .collect::<BTreeSet<u32>>()
        .into_iter()
        .collect();
    let kept: Vec<u32> = loads
        .iter()
        .flat_map(|load| load.survivors.iter().map(|group| group.index))
        .collect::<BTreeSet<u32>>()
        .into_iter()
        .collect();
    let ordinals: Vec<usize> = projection.iter().map(|o| *o as usize).collect();
    let scan = row_group_metadata(&loads[0].file, &ordinals, Some(&kept))?;
    // One lane with no batch where nothing survives, as the translator maps it (#282).
    let partition_groups = match scan.groups.is_empty() {
        true => vec![Vec::new()],
        false => partition(&scan.groups, 1, batching)?,
    };
    let schema = match projection.is_empty() {
        true => Schema::rowmarker(),
        false => {
            let fields: Vec<_> = projection
                .iter()
                .map(|ordinal| field_of(&loads, *ordinal))
                .collect();
            Schema::new(Arc::new(ArrowSchema::new(fields)))
        }
    };
    Ok(GpuLoadParquet::new(
        loads[0].table.clone(),
        projection,
        partition_groups,
        &scan,
        schema,
    ))
}

/// The field a member that reads file ordinal `ordinal` declares for it.
fn field_of(loads: &[&GpuLoadParquet], ordinal: u32) -> Arc<datafusion::arrow::datatypes::Field> {
    loads
        .iter()
        .find_map(|load| {
            let at = load.projection.iter().position(|o| *o == ordinal)?;
            Some(schema_of(*load).fields.fields()[at].clone())
        })
        .expect("a union ordinal is some member's")
}

fn schema_of(node: &dyn GpuNode) -> Schema {
    node.kind()
        .schema()
        .expect("a load declares a schema")
        .clone()
}

/// `node`'s tree rebuilt, each member load replaced by what it reads of its group's source.
fn sharing(
    node: &dyn GpuNode,
    query: usize,
    next: &mut usize,
    reads_shared: &HashMap<Origin, usize>,
    sources: &[SharedSource],
) -> Box<dyn GpuNode> {
    let origin = Origin { query, node: *next };
    *next += 1;
    if let Some(group) = reads_shared.get(&origin) {
        return consumer(node, &sources[*group]);
    }
    let children = node
        .children()
        .into_iter()
        .map(|child| sharing(child, query, next, reads_shared, sources))
        .collect();
    rebuild(node, children)
}

/// What a member reads in place of its own load: the source itself where it reads every column
/// the union does, else a column-ref projection of its own columns, which copies nothing.
fn consumer(own: &dyn GpuNode, source: &SharedSource) -> Box<dyn GpuNode> {
    let (NodeRef::LoadParquet(load), NodeRef::LoadParquet(union)) =
        (as_node_ref(own), as_node_ref(source))
    else {
        unreachable!("a member and its source are loads");
    };
    if load.projection == union.projection {
        return Box::new(source.clone());
    }
    let schema = schema_of(own);
    let exprs = load
        .projection
        .iter()
        .zip(schema.fields.fields())
        .map(|(ordinal, field)| {
            let at = union
                .projection
                .iter()
                .position(|o| o == ordinal)
                .expect("the union reads every member's columns");
            NamedExpr::new(Expr::column(at as u32, field.name()), field.name())
        })
        .collect();
    Box::new(GpuProject::new(Box::new(source.clone()), exprs, schema))
}

/// A query's own tree, owned, which `Fused` keeps beside the fused roots.
fn copy_of(node: &dyn GpuNode) -> Box<dyn GpuNode> {
    rebuild(node, node.children().into_iter().map(copy_of).collect())
}

fn refs(roots: &[Box<dyn GpuNode>]) -> Vec<&dyn GpuNode> {
    roots.iter().map(|root| root.as_ref()).collect()
}

/// `fused` with every held reader resolved: validation names one, the chooser decides it,
/// and the plan is validated again, until it passes. `cap` adds the readers of the roots that
/// wait for it. A buffer a join's hold placed above state, in a root that now waits, is taken
/// out, so validation names its reader again, to be decided as a waiting root's. Any other
/// refusal ends the loop with that refusal.
fn resolve(
    mut fused: Fused,
    cap: Option<NonZeroUsize>,
    choose: &mut Chooser<'_>,
) -> Result<Fused, PlanError> {
    loop {
        let (hold, reader, slot) = match validate_dag(&refs(&fused.roots), cap) {
            Ok(()) => return regrouped(fused),
            Err(PlanError::UnbufferedHold {
                hold, reader, slot, ..
            }) => (hold, reader, slot),
            Err(PlanError::UncappedAccumulator { node, .. }) if cap.is_some() => {
                fused = without_the_buffer_above(fused, node);
                continue;
            }
            Err(other) => return Err(other),
        };
        let walk = Walk::of(&fused);
        let mut held = walk.held_reader(&fused, hold, reader, slot);
        held.waits = cap.is_some_and(|cap| held.query >= cap.get());
        let member = held.member;
        let decision = choose(&fused, &held);
        if let (true, Choice::Buffer { edge }) = (held.waits, decision.choice) {
            refuse_state_below(&fused, &held, edge)?;
        }
        let above = match decision.choice {
            Choice::ReRead => None,
            Choice::Buffer { edge } => {
                assert!(
                    held.path.contains(&edge),
                    "the chooser placed a buffer on {edge:?}, off the reader's path {:?}",
                    held.path
                );
                Some(walk.stands_for(edge).node)
            }
        };
        fused.roots = match decision.choice {
            Choice::ReRead => {
                let own = copy_of(nth(fused.originals[member.query].as_ref(), member.node));
                let group = &mut fused.shared[held.group];
                let at = group
                    .members
                    .iter()
                    .position(|m| *m == member)
                    .expect("the reader is a member");
                group.members.remove(at);
                group.consumers.remove(at);
                edited(&fused.roots, held.path[0], |_| own)
            }
            Choice::Buffer { edge } => {
                edited(&fused.roots, edge, |child| Box::new(new_hold_buffer(child)))
            }
        };
        fused.holds.push(HoldRecord {
            query: held.query,
            table: fused.shared[held.group].table.clone(),
            hold: walk.names[hold].to_string(),
            above,
            decision,
        });
        fused = refreshed(fused);
    }
}

/// A waiting reader's buffer on `edge` refused where state lies at or under it: the cap would
/// not hold that state, validation would take the buffer out, and the chooser would put it
/// back, for ever.
fn refuse_state_below(
    fused: &Fused,
    held: &HeldReader,
    edge: (usize, usize),
) -> Result<(), PlanError> {
    let roots = refs(&fused.roots);
    let dag = PlanDag::of(&roots);
    let Some(state) = state_under(&dag, dag.nodes[edge.0].children[edge.1]) else {
        return Ok(());
    };
    Err(PlanError::Invalid(format!(
        "the chooser placed a buffer on edge {edge:?} for query {}'s reader of {}, above {} \
         (node {state}), which keeps state, though its root waits for the cap",
        held.query,
        fused.shared[held.group].table,
        dag.nodes[state].node.name()
    )))
}

/// `fused` with each group's shared load rebuilt over the members still reading it, since a
/// re-read takes its member out: the union of their own loads, every reader re-pointed at it
/// as [`consumer`] reads one. A member left alone reads its own load, and its group is gone.
fn regrouped(mut fused: Fused) -> Result<Fused, PlanError> {
    let own_load = |member: Origin| match as_node_ref(original(&fused, member)) {
        NodeRef::LoadParquet(load) => load,
        _ => unreachable!("a group's member is a load"),
    };
    let mut sources = Vec::new();
    for group in &fused.shared {
        let loads: Vec<&GpuLoadParquet> = group.members.iter().map(|m| own_load(*m)).collect();
        sources.push(match loads.len() {
            0 | 1 => None,
            _ => Some(SharedSource::new(union_of(&loads, Batching::Off)?)),
        });
    }
    let group_of: HashMap<Origin, usize> = fused
        .shared
        .iter()
        .enumerate()
        .flat_map(|(at, group)| group.members.iter().map(move |member| (*member, at)))
        .collect();
    let read = |member: Origin| {
        let own = original(&fused, member);
        match &sources[group_of[&member]] {
            Some(source) => consumer(own, source),
            None => copy_of(own),
        }
    };
    let walk = Walk::of(&fused);
    let roots = refs(&fused.roots);
    let dag = PlanDag::of(&roots);
    let rebuilt: Vec<Box<dyn GpuNode>> = dag
        .roots
        .iter()
        .map(|root| reread_through(&dag, &walk, *root, None, &read))
        .collect();
    fused.roots = rebuilt;
    fused.shared.retain(|group| group.members.len() > 1);
    Ok(refreshed(fused))
}

/// The tree under `at`, reached over `edge`, copied with each member's reader of a shared load,
/// the source or the project inserted over it, replaced by `read` of that member.
fn reread_through(
    dag: &PlanDag<'_>,
    walk: &Walk,
    at: usize,
    edge: Option<(usize, usize)>,
    read: &dyn Fn(Origin) -> Box<dyn GpuNode>,
) -> Box<dyn GpuNode> {
    if walk.inserted_project[at] {
        return read(walk.members[&(at, 0)]);
    }
    if dag.nodes[at].node.sharing_key().is_some() {
        return read(walk.members[&edge.expect("a shared source has a reader")]);
    }
    let children = dag.nodes[at]
        .children
        .iter()
        .enumerate()
        .map(|(slot, child)| reread_through(dag, walk, *child, Some((at, slot)), read))
        .collect();
    rebuild(dag.nodes[at].node, children)
}

/// `fused` without the lowest `GpuHoldBuffer` above node `at`, and without its hold's record.
fn without_the_buffer_above(mut fused: Fused, at: usize) -> Fused {
    let walk = Walk::of(&fused);
    let roots = refs(&fused.roots);
    let dag = PlanDag::of(&roots);
    let mut buffer = at;
    while !matches!(as_node_ref(dag.nodes[buffer].node), NodeRef::HoldBuffer(_)) {
        buffer = walk.parent[buffer].expect("a buffer lies above the node").0;
    }
    let above = walk.stands_for((buffer, 0));
    let record = fused
        .holds
        .iter()
        .position(|record| {
            record.query == above.query
                && record.above == Some(above.node)
                && matches!(record.decision.choice, Choice::Buffer { .. })
        })
        .expect("every buffer has its hold's record");
    let mut shared = HashMap::new();
    fused.roots = dag
        .roots
        .iter()
        .map(|root| copied_skipping(&dag, *root, buffer, &mut shared))
        .collect();
    fused.holds.remove(record);
    refreshed(fused)
}

/// The tree under `at` copied, as [`edited`] copies it, with node `skip` replaced by its child.
fn copied_skipping(
    dag: &PlanDag<'_>,
    at: usize,
    skip: usize,
    shared: &mut HashMap<usize, SharedSource>,
) -> Box<dyn GpuNode> {
    if at == skip {
        return copied_skipping(dag, dag.nodes[at].children[0], skip, shared);
    }
    let node = dag.nodes[at].node;
    if let Some(source) = shared_copy(node, shared) {
        return source;
    }
    let children = dag.nodes[at]
        .children
        .iter()
        .map(|child| copied_skipping(dag, *child, skip, shared))
        .collect();
    rebuild(node, children)
}

/// A shared source's copy, one per key so every reader of it reads one source; `None` for any
/// other node.
fn shared_copy(
    node: &dyn GpuNode,
    shared: &mut HashMap<usize, SharedSource>,
) -> Option<Box<dyn GpuNode>> {
    let key = node.sharing_key()?;
    let source = shared.entry(key).or_insert_with(|| {
        let load = rebuild(node, Vec::new())
            .into_any_box()
            .downcast::<GpuLoadParquet>()
            .expect("only a load is shared");
        SharedSource::new(*load)
    });
    Some(Box::new(source.clone()))
}

/// `fused` with what is read off its roots recomputed: the origins, and the loads per table.
fn refreshed(mut fused: Fused) -> Fused {
    fused.origins = Walk::of(&fused).origins;
    let roots = refs(&fused.roots);
    let dag = PlanDag::of(&roots);
    let mut reads: Vec<(String, usize)> = Vec::new();
    for node in &dag.nodes {
        if let NodeRef::LoadParquet(load) = as_node_ref(node.node) {
            match reads.iter_mut().find(|(table, _)| *table == load.table) {
                Some((_, count)) => *count += 1,
                None => reads.push((load.table.clone(), 1)),
            }
        }
    }
    fused.reads = reads;
    fused
}

pub(crate) fn original(fused: &Fused, origin: Origin) -> &dyn GpuNode {
    nth(fused.originals[origin.query].as_ref(), origin.node)
}

/// The node at pre-order index `at` of a tree.
fn nth(tree: &dyn GpuNode, at: usize) -> &dyn GpuNode {
    fn walk<'t>(node: &'t dyn GpuNode, left: &mut usize) -> Option<&'t dyn GpuNode> {
        if *left == 0 {
            return Some(node);
        }
        *left -= 1;
        node.children()
            .into_iter()
            .find_map(|child| walk(child, left))
    }
    let mut left = at;
    walk(tree, &mut left).expect("an origin indexes its own tree")
}

/// The roots rebuilt with `change` applied to the child on `edge`, indexed as
/// `PlanDag::of(roots)` numbers them. Each shared source is rebuilt once and shared again.
fn edited(
    roots: &[Box<dyn GpuNode>],
    edge: (usize, usize),
    change: impl FnOnce(Box<dyn GpuNode>) -> Box<dyn GpuNode>,
) -> Vec<Box<dyn GpuNode>> {
    let refs = refs(roots);
    let dag = PlanDag::of(&refs);
    let mut change = Some(change);
    let mut shared = HashMap::new();
    dag.roots
        .iter()
        .map(|root| copied(&dag, *root, edge, &mut change, &mut shared))
        .collect()
}

fn copied<F: FnOnce(Box<dyn GpuNode>) -> Box<dyn GpuNode>>(
    dag: &PlanDag<'_>,
    at: usize,
    edge: (usize, usize),
    change: &mut Option<F>,
    shared: &mut HashMap<usize, SharedSource>,
) -> Box<dyn GpuNode> {
    let node = dag.nodes[at].node;
    if let Some(source) = shared_copy(node, shared) {
        return source;
    }
    let children = dag.nodes[at]
        .children
        .iter()
        .enumerate()
        .map(|(slot, child)| {
            let child = copied(dag, *child, edge, change, shared);
            match (at, slot) == edge {
                true => change.take().expect("one edge changes once")(child),
                false => child,
            }
        })
        .collect();
    rebuild(node, children)
}

/// The fused DAG read against the queries' own trees, by `PlanDag::of(&fused.roots)` index.
struct Walk {
    origins: Vec<Vec<Origin>>,
    /// Per edge into a shared source, the member load it stands for.
    members: HashMap<(usize, usize), Origin>,
    /// The one edge above each node no two parents read.
    parent: Vec<Option<(usize, usize)>>,
    inserted_project: Vec<bool>,
    names: Vec<&'static str>,
}

impl Walk {
    fn of(fused: &Fused) -> Walk {
        let roots = refs(&fused.roots);
        let dag = PlanDag::of(&roots);
        let n = dag.nodes.len();
        let mut walk = Walk {
            origins: vec![Vec::new(); n],
            members: HashMap::new(),
            parent: dag
                .nodes
                .iter()
                .map(|node| node.readers.first().map(|r| (r.parent, r.slot)))
                .collect(),
            inserted_project: vec![false; n],
            names: dag.nodes.iter().map(|node| node.node.name()).collect(),
        };
        for (query, (original, root)) in fused.originals.iter().zip(&dag.roots).enumerate() {
            walk.pair(&dag, *root, None, original.as_ref(), query, &mut 0);
        }
        walk
    }

    /// Fused node `at`, reached over `edge`, against `original`, query `query`'s node at
    /// pre-order `*next`. An inserted buffer or project stands for nothing, so the walk goes
    /// through it to its child with the same original.
    fn pair(
        &mut self,
        dag: &PlanDag<'_>,
        at: usize,
        edge: Option<(usize, usize)>,
        original: &dyn GpuNode,
        query: usize,
        next: &mut usize,
    ) {
        let fused = dag.nodes[at].node;
        let inserted = match (as_node_ref(fused), as_node_ref(original)) {
            (NodeRef::HoldBuffer(_), NodeRef::HoldBuffer(_)) => false,
            (NodeRef::HoldBuffer(_), _) => true,
            (NodeRef::Project(_), NodeRef::LoadParquet(_)) => {
                self.inserted_project[at] = true;
                true
            }
            _ => false,
        };
        if inserted {
            let child = dag.nodes[at].children[0];
            return self.pair(dag, child, Some((at, 0)), original, query, next);
        }
        let origin = Origin { query, node: *next };
        *next += 1;
        self.origins[at].push(origin);
        if fused.sharing_key().is_some() {
            self.members
                .insert(edge.expect("a shared source has a reader"), origin);
        }
        let children = dag.nodes[at].children.iter().zip(original.children());
        for (slot, (child, original_child)) in children.enumerate() {
            self.pair(dag, *child, Some((at, slot)), original_child, query, next);
        }
    }

    /// The reader validation named.
    fn held_reader(&self, fused: &Fused, hold: usize, reader: usize, slot: usize) -> HeldReader {
        let (member, mut edge) = match self.inserted_project[reader] {
            true => (
                self.members[&(reader, 0)],
                self.parent[reader].expect("an inserted project has a reader"),
            ),
            false => (self.members[&(reader, slot)], (reader, slot)),
        };
        let mut path = vec![edge];
        // Inside a hold every node but a shared source has one reader, so the climb is unique.
        while edge.0 != hold {
            edge = self.parent[edge.0].expect("the hold lies above its reader");
            path.push(edge);
        }
        let group = fused
            .shared
            .iter()
            .position(|group| group.members.contains(&member))
            .expect("a shared source's reader is a member of its group");
        HeldReader {
            query: member.query,
            group,
            hold,
            path,
            member,
            waits: false,
        }
    }

    /// The node of its query's own tree that the child on `edge` stands for: the member load
    /// where the child is a shared source or the project selecting from one.
    fn stands_for(&self, edge: (usize, usize)) -> Origin {
        if let Some(member) = self.members.get(&edge) {
            return *member;
        }
        let child = self.child_on(edge);
        match self.inserted_project[child] {
            true => self.members[&(child, 0)],
            false => self.origins[child][0],
        }
    }

    fn child_on(&self, edge: (usize, usize)) -> usize {
        self.parent
            .iter()
            .position(|parent| *parent == Some(edge))
            .expect("every edge has a child")
    }
}
