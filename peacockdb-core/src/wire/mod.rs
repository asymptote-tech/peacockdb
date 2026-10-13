//! Everything that knows what a flat buffer looks like: the vocabulary, the writers, the
//! reader, and the two renderers.
//!
//! The C++ side never sees the plan tree. It is sent a menu of parameterized kernels whose
//! nodes exist to be addressed, and the driver calls the ABI as often as its schedule wants;
//! `llm-wiki/architecture.md` has the table this implements. A node that makes no ABI call
//! gets no recipe, which is a statement about the node rather than a gap.
//!
//! `generated` is private here, and that is why the wall is drawn where it is: eleven files
//! name flatc's output, so it is private only if all eleven are inside with it.

// Nothing in a production build calls the wire yet: the device backend that reads its
// vocabulary is itself dead until the CLI grows a device path (`executor/mod.rs`, `mod
// gpu_backend`), the corpus harness drives its writers, and the plan goldens its reader and
// renderers. So the lint is read where the callers are, and only there; the code stays in
// every build. The attribute leaves with the first production device caller.
#![cfg_attr(not(test), allow(dead_code))]

mod aggregate_writer;
mod attach;
mod expr_writer;
mod fb_text;
mod generated;
mod join;
mod node_writer;
mod read;
mod recipes;
mod serialize;
mod writer;

#[cfg(test)]
mod tests;

#[cfg(all(test, feature = "gpu"))]
mod gpu_tests;

use crate::plan::GpuNode;
use crate::plan::PlanError;
use crate::planner::{BatchSizing, PlanKnobs};

use generated::peacock::plan as fb;

// What a recipe is made of: the ABI symbols, the node kinds they address, the handles a
// call is passed, and when the driver makes it.

/// A node of the recipe plan, addressed by its position in it. The number is the whole
/// content of an address, which is why a call carries nothing else about the node it runs.
pub(crate) type Seq = u32;

/// The entry points. Two of them take runtime bounds instead of a seq, which is the reason
/// they exist: a frozen node cannot carry a number that is known only once the rows have
/// been counted. The three join symbols address one `CudfJoin` seq for the life of a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AbiSymbol {
    ExecuteNode,
    ExecuteScanRowGroups,
    SliceHandle,
    ResultFromHandle,
    JoinBuild,
    JoinProbe,
    JoinFinish,
}

impl AbiSymbol {
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::ExecuteNode => "execute_node",
            Self::ExecuteScanRowGroups => "execute_scan_rowgroups",
            Self::SliceHandle => "slice_handle",
            Self::ResultFromHandle => "result_from_handle",
            Self::JoinBuild => "join_build",
            Self::JoinProbe => "join_probe",
            Self::JoinFinish => "join_finish",
        }
    }
}

/// Which project a seq is, where a node's recipe holds one beside another call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectRole {
    /// An aggregate's finalize, which is ours rather than the executor's: both engines
    /// evaluate this expression, so they agree by construction.
    Finalize,
}

/// The node kinds the engine addresses, carrying the fields a reader cannot get from the
/// plan line above: a repartition's lane count is the one number a recipe repeats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FbKind {
    Scan,
    Filter,
    Project(ProjectRole),
    /// The one a map arm runs; a `CudfProject` with no role of its own.
    PlainProject,
    /// `Partial` builds state from raw values and `Merge` merges state into state.
    /// Never `Final`, which would also finalize, and a finalize here is a project.
    Aggregate {
        merge: bool,
    },
    Sort,
    SortPreservingMerge,
    CoalescePartitions,
    Repartition {
        lanes: u32,
    },
    /// One for all three join nodes: the session reads its type, keys and residual off the
    /// payload, and the plan line above already names them.
    Join,
}

impl FbKind {
    /// The node kind on the wire. What a recipe claims and what the buffer holds are
    /// checked against each other through this — see `read::check_seq_kinds`.
    pub(crate) fn wire_kind(&self) -> fb::PlanNodeKind {
        match self {
            Self::Scan => fb::PlanNodeKind::CudfScan,
            Self::Filter => fb::PlanNodeKind::CudfFilter,
            Self::Project(_) | Self::PlainProject => fb::PlanNodeKind::CudfProject,
            Self::Aggregate { .. } => fb::PlanNodeKind::CudfAggregate,
            Self::Sort => fb::PlanNodeKind::CudfSort,
            Self::SortPreservingMerge => fb::PlanNodeKind::CudfSortPreservingMerge,
            Self::CoalescePartitions => fb::PlanNodeKind::CudfCoalescePartitions,
            Self::Repartition { .. } => fb::PlanNodeKind::CudfRepartition,
            Self::Join => fb::PlanNodeKind::CudfJoin,
        }
    }
}

/// Where a call's input comes from. The driver owns every one; naming them is what makes
/// a recipe checkable against consume-on-use. A join's calls name none: each symbol takes
/// one handle, and which one is the symbol's own meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Input {
    /// The batch the call was scheduled for.
    Batch,
    /// Every batch this lane accumulated.
    LaneBatches,
    /// Every lane's handle, partition-major.
    AllLanes,
    /// What the previous call in this recipe returned.
    PriorOutput,
    /// Not a handle: the row groups this batch reads, overriding the node's own list.
    RowGroups,
    /// Not a handle either: an offset and a length counted at run time.
    RowRange,
}

impl Input {
    fn text(&self) -> &'static str {
        match self {
            Self::Batch => "batch",
            Self::LaneBatches => "lane batches",
            Self::AllLanes => "all lanes",
            Self::PriorOutput => "prior output",
            Self::RowGroups => "row groups",
            Self::RowRange => "row range",
        }
    }
}

/// When the driver makes a call. Two nodes emitting the same seq set differ by this and
/// nothing else — a sort per batch and a sort at done are different plans.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CallPattern {
    PerBatch,
    /// Once, with the build side or with none: a join session begins here.
    JoinBuild,
    PerProbeBatch,
    /// Once, when the node's input is complete.
    AtDone,
    /// Every time the accumulation compacts, and once more at done.
    PerCompaction,
    /// The two batches that straddle an interval's ends; a batch wholly inside is
    /// forwarded untouched and one wholly outside is released uncalled.
    PerStraddlingBatch,
    /// Once per handle that reaches the sink.
    PerHandle,
}

impl CallPattern {
    /// Empty where the symbol already says when — `join_build` is made once, at the build.
    fn text(&self) -> &'static str {
        match self {
            Self::PerBatch => "per batch",
            Self::JoinBuild => "",
            Self::PerProbeBatch => "per probe batch",
            Self::AtDone => "at done",
            Self::PerCompaction => "per compaction and at done",
            Self::PerStraddlingBatch => "per straddling batch",
            Self::PerHandle => "per handle",
        }
    }
}

/// One ABI call: the symbol, the seq it addresses where it takes one, and what it passes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Call {
    pub(crate) symbol: AbiSymbol,
    /// `None` for the two symbols whose arguments are runtime row counts.
    pub(crate) target: Option<(Seq, FbKind)>,
    pub(crate) inputs: Vec<Input>,
    pub(crate) when: CallPattern,
}

impl Call {
    /// A call against a recipe-plan node. The symbol follows from the kind: only a scan
    /// takes the row-group override, and everything else is the generic entry point.
    pub(crate) fn seq(seq: Seq, kind: FbKind, inputs: Vec<Input>, when: CallPattern) -> Self {
        Self {
            symbol: match kind {
                FbKind::Scan => AbiSymbol::ExecuteScanRowGroups,
                _ => AbiSymbol::ExecuteNode,
            },
            target: Some((seq, kind)),
            inputs,
            when,
        }
    }

    /// A join session's call: the seq is the `CudfJoin` the session was built from, and
    /// the handle each symbol takes is its own meaning, so the call names no input.
    pub(crate) fn join(seq: Seq, when: CallPattern) -> Self {
        Self {
            symbol: match when {
                CallPattern::JoinBuild => AbiSymbol::JoinBuild,
                CallPattern::PerProbeBatch => AbiSymbol::JoinProbe,
                CallPattern::AtDone => AbiSymbol::JoinFinish,
                other => unreachable!(
                    "a join session is called at build, per probe batch and at done, not {other:?}"
                ),
            },
            target: Some((seq, FbKind::Join)),
            inputs: Vec::new(),
            when,
        }
    }

    pub(crate) fn bare(symbol: AbiSymbol, inputs: Vec<Input>, when: CallPattern) -> Self {
        Self {
            symbol,
            target: None,
            inputs,
            when,
        }
    }
}

/// What one node does to the device, in call order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Recipe {
    pub(crate) calls: Vec<Call>,
}

impl Recipe {
    pub(crate) fn of(calls: Vec<Call>) -> Self {
        Self { calls }
    }

    /// The seqs this node addresses, in the order it emits them.
    pub(crate) fn seqs(&self) -> Vec<Seq> {
        self.calls
            .iter()
            .filter_map(|call| call.target.map(|(seq, _)| seq))
            .collect()
    }
}

/// Whether the recipes section prints what each call passes the executor, or only which
/// kernel it addresses. One renderer either way: two would drift, and the ten mode goldens
/// and the payload golden would then disagree about a plan neither of them changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Payloads {
    Omitted,
    Shown,
}

/// Every node's recipe, indexed by the post-order position the estimator and the memory
/// golden already number by, plus the bytes those recipes address.
///
/// That index is a position in the TREE and a [`Seq`] is an address in the recipe plan:
/// they part company at the first node with two calls, and a driver holding both at once
/// has to keep them apart.
#[derive(Debug, Default)]
pub(crate) struct RecipePlan {
    recipes: Vec<Option<Recipe>>,
    bytes: Vec<u8>,
    wire_nodes: Seq,
    join_scratch_bytes: u64,
}

impl RecipePlan {
    /// By post-order position in the tree, not by seq. `None` where that node makes no ABI
    /// call at all, which is what a forwarder does.
    pub(crate) fn get(&self, node: usize) -> Option<&Recipe> {
        self.recipes.get(node).and_then(|recipe| recipe.as_ref())
    }

    /// Nodes in the PLAN TREE — the engine's own, the length the memory model's per-node
    /// vector has, and what a consumer checks its own tree against before reading a `None`
    /// as an answer.
    pub(crate) fn nodes(&self) -> usize {
        self.recipes.len()
    }

    /// Nodes in the RECIPE PLAN — the fb tree, stubs and structural unions included, which
    /// is what `peacock_executor_begin_plan` reports through `out_node_count`.
    ///
    /// Deliberately not [`RecipePlan::nodes`] and deliberately named apart: the two count
    /// different trees and mostly disagree — a node with several calls, a stub, a union
    /// each separate them — so a driver that checked the wrong one against the C++ would
    /// be comparing two true numbers about two different things.
    // Its readers compare it with the C++ count, which `rust-only` does not link.
    #[cfg_attr(feature = "rust-only", allow(dead_code))]
    pub(crate) fn wire_nodes(&self) -> usize {
        self.wire_nodes as usize
    }

    /// The serialized recipe plan: what `peacock_executor_begin_plan` is given, and what
    /// every seq in every recipe indexes into.
    ///
    /// A plan that exists is one every seq of which holds the kind its recipe claims —
    /// [`attach_recipes`] fails rather than substituting anything for a payload it cannot
    /// write, so there is no second accessor and no caveat to remember.
    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// What every `CudfJoin` in the buffer carries as `chunk_bytes`: the scratch one join
    /// call may spend, 0 being the session's own 1 GiB. An executor pricing a join call
    /// reads it here, so the accountant and the session bound the same number.
    // Its reader is the device executor, which `rust-only` does not build.
    #[cfg_attr(feature = "rust-only", allow(dead_code))]
    pub(crate) fn join_scratch_bytes(&self) -> u64 {
        self.join_scratch_bytes
    }
}

/// Build the recipe plan for a finished tree. It runs after planning because a recipe is
/// a statement about a node, and a node is not finished until the plan is.
///
/// `join_scratch_bytes` is what each join call may spend on pairs — the planner's
/// `join_scratch_bytes(knobs)` for a planned query, 0 (the session's 1 GiB) for a tree no
/// mode planned.
///
/// `Err` where a node's payload cannot be written — an expression the wire has no shape
/// for (#168) — because a plan missing one node's arguments is not a plan the C++ can be
/// handed, and the alternative is a buffer whose every later seq has to be checked against
/// a list to be trusted.
pub(crate) fn attach_recipes(
    root: &dyn GpuNode,
    join_scratch_bytes: u64,
) -> Result<RecipePlan, PlanError> {
    attach::attach_recipes(root, join_scratch_bytes)
}

/// What one join call may spend on pairs and the cross product, written into every
/// `CudfJoin` as `chunk_bytes`: a sixteenth of a budgeted mode's budget, at most 1 GiB, and 0
/// — the session's own 1 GiB — where the mode sets no budget.
pub(crate) fn join_scratch_bytes(knobs: &PlanKnobs) -> u64 {
    match knobs.sizing {
        BatchSizing::Budgeted => (knobs.budget / 16).min(1 << 30),
        BatchSizing::OneBatchPerLane | BatchSizing::OneBatchPerRowGroup => 0,
    }
}

/// Every published seq holds a node of the kind its recipe claims.
pub(crate) fn check_seq_kinds(plan: &RecipePlan) -> Result<(), PlanError> {
    read::check_seq_kinds(plan)
}

/// How deep the serialized plan is, which the C++ verifier bounds.
pub(crate) fn depth(plan: &RecipePlan) -> Result<usize, PlanError> {
    read::depth(plan)
}

/// The `--- recipes ---` section: what each node asks of the device, under the same tree
/// the plan renders, so a line reads against the node above it.
pub(crate) fn render_plan_recipes(
    root: &dyn GpuNode,
    plan: &RecipePlan,
    payloads: Payloads,
) -> String {
    recipes::render_plan_recipes(root, plan, payloads)
}
