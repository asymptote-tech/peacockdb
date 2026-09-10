//! Everything that knows what a flat buffer looks like: the recipe vocabulary, the writers
//! that build the plan the C++ is handed, the reader, and the two renderers.
//!
//! The C++ side never sees the plan tree. It is sent a plan in the legacy vocabulary whose
//! nodes exist to be addressed — a menu of parameterized kernels — and the driver then calls
//! the ABI as often as its own schedule wants. One walk builds that buffer and records, per
//! plan node, the calls it makes and the seqs they address; `llm-wiki/architecture.md` has
//! the table it implements.
//!
//! A node that makes no ABI call gets no recipe, and the absence is a statement about the
//! node rather than a gap: a forwarder routes batches and touches no device.
//!
//! `generated` is private to this component, which is the whole reason the wall is drawn
//! here: eleven files name flatc's output and use 73 of its types between them, so it can
//! only be private if all eleven are inside with it.

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

use datafusion::common::JoinType;

use crate::plan::GpuNode;
use crate::plan::PlanError;

use generated::peacock::plan as fb;

// What a recipe is made of: the four ABI symbols, the legacy node kinds they address, the
// handles a call is passed, and when the driver makes it.

/// A node of the recipe plan, addressed by its position in it. The number is the whole
/// content of an address, which is why a call carries nothing else about the node it runs.
pub type Seq = u32;

/// The frozen entry points. Two of them take runtime bounds instead of a seq, which is
/// the reason they exist: a frozen node cannot carry a number that is known only once the
/// rows have been counted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbiSymbol {
    ExecuteNode,
    ExecuteScanRowGroups,
    SliceHandle,
    ResultFromHandle,
}

impl AbiSymbol {
    pub fn name(&self) -> &'static str {
        match self {
            Self::ExecuteNode => "execute_node",
            Self::ExecuteScanRowGroups => "execute_scan_rowgroups",
            Self::SliceHandle => "slice_handle",
            Self::ResultFromHandle => "result_from_handle",
        }
    }
}

/// Which of a join's two projects a seq is. Both are `CudfProject`, and telling them
/// apart in a recipe of five calls is the difference between reading it and decoding it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectRole {
    /// The probe keys this batch contributes to the accumulation (#136).
    ProbeKeys,
    /// An aggregate's finalize, which is ours rather than the executor's: both engines
    /// evaluate this expression, so they agree by construction.
    Finalize,
    /// Build columns straight through, plus one typed NULL per probe column the join's
    /// projection keeps — what makes the anti join's output the joined schema.
    NullPad { nulls: usize },
    /// The columns the node's projection keeps, out of what the finish emitted. No literal
    /// in it: the build-side semi family's finish already emits the row, and a projection
    /// only narrows it. Its absence was two engines answering with different columns.
    Narrow,
}

/// The legacy node kinds this mode addresses, carrying the fields a reader cannot get
/// from the plan line above: a per-call join type is not the node's own, and a
/// repartition's lane count is the one number a recipe repeats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FbKind {
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
    HashJoin {
        join_type: JoinType,
    },
    CrossJoin,
    NestedLoopJoin,
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
            Self::HashJoin { .. } => fb::PlanNodeKind::CudfHashJoin,
            Self::CrossJoin => fb::PlanNodeKind::CudfCrossJoin,
            Self::NestedLoopJoin => fb::PlanNodeKind::CudfNestedLoopJoin,
        }
    }
}

/// Where a call's input comes from. The driver owns every one; naming them is what makes
/// a recipe checkable against consume-on-use (#152), since a copy appears here as a copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    /// The batch the call was scheduled for.
    Batch,
    /// A copy of it, because the call below consumes it and something else needs it too.
    /// The surface has no copy symbol, so a recipe naming this is one an executor refuses
    /// until [#145](../../../../llm-wiki/tickets.md#t145) — see #152.
    BatchCopy,
    /// The build side, handed over: the call consumes it and nothing needs it again.
    BuildSide,
    /// A copy of the build side, because the next probe batch needs it as well. Same
    /// absence as [`Input::BatchCopy`]: the first call gets the handle and a second is
    /// refused naming #152.
    BuildSideCopy,
    /// Every batch this lane accumulated.
    LaneBatches,
    /// Every lane's handle, partition-major.
    AllLanes,
    /// The probe keys kept per batch, which the finish pass joins against (#136).
    AccumulatedKeys,
    /// What the previous call in this recipe returned.
    PriorOutput,
    /// Not a handle: the row groups this batch reads, overriding the node's own list.
    RowGroups,
    /// Not a handle either: an offset and a length counted at run time.
    RowRange,
}

impl Input {
    /// Whether this input is the build side, handed over or copied. Asked by an executor
    /// pricing a call: a probe call that names neither is a call the build side does not
    /// have to be there for.
    pub fn is_build_side(&self) -> bool {
        matches!(self, Self::BuildSide | Self::BuildSideCopy)
    }

    fn text(&self) -> &'static str {
        match self {
            Self::Batch => "batch",
            Self::BatchCopy => "batch copy",
            Self::BuildSide => "build",
            Self::BuildSideCopy => "build copy",
            Self::LaneBatches => "lane batches",
            Self::AllLanes => "all lanes",
            Self::AccumulatedKeys => "accumulated keys",
            Self::PriorOutput => "prior output",
            Self::RowGroups => "row groups",
            Self::RowRange => "row range",
        }
    }
}

/// When the driver makes a call. Two nodes emitting the same seq set differ by this and
/// nothing else — a sort per batch and a sort at done are different plans.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallPattern {
    PerBatch,
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
    fn text(&self) -> &'static str {
        match self {
            Self::PerBatch => "per batch",
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
pub struct Call {
    pub symbol: AbiSymbol,
    /// `None` for the two symbols whose arguments are runtime row counts.
    pub target: Option<(Seq, FbKind)>,
    pub inputs: Vec<Input>,
    pub when: CallPattern,
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
pub struct Recipe {
    pub calls: Vec<Call>,
}

impl Recipe {
    pub(crate) fn of(calls: Vec<Call>) -> Self {
        Self { calls }
    }

    /// The seqs this node addresses, in the order it emits them.
    pub fn seqs(&self) -> Vec<Seq> {
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
pub enum Payloads {
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
pub struct RecipePlan {
    recipes: Vec<Option<Recipe>>,
    bytes: Vec<u8>,
    wire_nodes: Seq,
}

impl RecipePlan {
    /// By post-order position in the tree, not by seq. `None` where that node makes no ABI
    /// call at all, which is what a forwarder does.
    pub fn get(&self, node: usize) -> Option<&Recipe> {
        self.recipes.get(node).and_then(|recipe| recipe.as_ref())
    }

    /// Nodes in the PLAN TREE — this mode's own, the length the memory model's per-node
    /// vector has, and what a consumer checks its own tree against before reading a `None`
    /// as an answer.
    pub fn nodes(&self) -> usize {
        self.recipes.len()
    }

    /// Nodes in the RECIPE PLAN — the fb tree, stubs and structural unions included, which
    /// is what `peacock_executor_begin_plan` reports through `out_node_count`.
    ///
    /// Deliberately not [`RecipePlan::nodes`] and deliberately named apart: the two count
    /// different trees and mostly disagree — a node with several calls, a stub, a union
    /// each separate them — so a driver that checked the wrong one against the C++ would
    /// be comparing two true numbers about two different things.
    pub fn wire_nodes(&self) -> usize {
        self.wire_nodes as usize
    }

    /// The serialized recipe plan: what `peacock_executor_begin_plan` is given, and what
    /// every seq in every recipe indexes into.
    ///
    /// A plan that exists is one every seq of which holds the kind its recipe claims —
    /// [`attach_recipes`] fails rather than substituting anything for a payload it cannot
    /// write, so there is no second accessor and no caveat to remember.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// Build the recipe plan for a finished tree. It runs after planning because a recipe is
/// a statement about a node, and a node is not finished until the plan is.
///
/// `Err` where a node's payload cannot be written — an expression the wire has no shape
/// for (#168) — because a plan missing one node's arguments is not a plan the C++ can be
/// handed, and the alternative is a buffer whose every later seq has to be checked against
/// a list to be trusted.
pub fn attach_recipes(root: &dyn GpuNode) -> Result<RecipePlan, PlanError> {
    attach::attach_recipes(root)
}

/// Every published seq holds a node of the kind its recipe claims.
pub fn check_seq_kinds(plan: &RecipePlan) -> Result<(), PlanError> {
    read::check_seq_kinds(plan)
}

/// How deep the serialized plan is, which the C++ verifier bounds.
pub fn depth(plan: &RecipePlan) -> Result<usize, PlanError> {
    read::depth(plan)
}

/// The `--- recipes ---` section: what each node asks of the device, under the same tree
/// the plan renders, so a line reads against the node above it.
pub fn render_plan_recipes(root: &dyn GpuNode, plan: &RecipePlan, payloads: Payloads) -> String {
    recipes::render_plan_recipes(root, plan, payloads)
}
