"""`validate_schemas_and_partitions()` — what each node needs of its children.

Split from `plan.py`'s whole-tree rules because a node knows what it requires and can name the
fix: a limit over four lanes should read *the planner inserts `GpuMergePartitions` below it*,
not *this category is 1:1 per lane*. `Plan.validate` runs these first for that reason.

What it checks are the parts of a child's `PartitionLayout` a node depends on — hash
distribution, sortedness, batch layout: the properties that make a plan silently wrong.
"""

from __future__ import annotations

from ..errors import PlanError
from ..engine.layout import KeyDistributionKind


def _child(node, slot: int = 0):
    return node.children()[slot]


def one_partition_in(node) -> None:
    """A limit is an interval over one stream; over N lanes it names no rows."""
    lanes = _child(node).output_partitions().n
    if lanes != 1:
        raise PlanError(
            f"{node.name()}: a limit is an interval over one stream, and its input has "
            f"{lanes} lanes — the planner inserts GpuMergePartitions below it"
        )


def prefix_is_meaningful(node) -> None:
    """A limit under a sort needs the *stream* ordered, not merely each batch.

    `BatchSorted` without `SingleBatch` means every batch is ordered and the stream is not,
    so a prefix of it is not the top-N anyone asked for — it is the first rows of whichever
    batches arrived first. Unsorted input is fine: an unordered LIMIT is allowed to return
    any rows, which the determinism scope note already covers.
    """
    layout = _child(node).output_partitions()
    if layout.sort_order.is_batch_sorted and not layout.is_stream_sorted:
        raise PlanError(
            f"{node.name()}: its input is sorted per batch but not across them, so a "
            "prefix is not a top-N — the planner puts GpuAccumulateBatchesAndSort or "
            "GpuMergeSortedPartitions below it"
        )


def sorted_input(node) -> None:
    """A k-way merge merges runs; unsorted input would make it a concatenation."""
    layout = _child(node).output_partitions()
    if not layout.sort_order.is_batch_sorted:
        raise PlanError(
            f"{node.name()}: merging sorted partitions requires BatchSorted input — the "
            "planner puts a GpuSort below it"
        )


def co_partitioned_join(build_keys, probe_keys):
    """Both sides hash-distributed on their join keys, whenever there is more than one lane.

    At one lane every row meets every other and distribution is irrelevant. Above one, a
    join runs lane-wise: rows of lane p on the left meet only rows of lane p on the right.
    That is correct exactly when equal keys hash to equal lanes on both sides, and silently
    lossy otherwise — the injector guards the same rule by convention when it decides
    whether a join's lane count may be rewritten, and this is that rule made checkable.
    """

    def check(node) -> None:
        lanes = node.output_partitions().n
        if lanes <= 1:
            return
        for side, keys, slot in (("build", build_keys, 0), ("probe", probe_keys, 1)):
            distribution = _child(node, slot).output_partitions().key_distribution
            if distribution.kind is not KeyDistributionKind.BY_HASH:
                raise PlanError(
                    f"{node.name()}: the {side} side of a {lanes}-lane join is not "
                    "hash-distributed, so lane p would be joined against rows whose "
                    "matches live in another lane — the planner shuffles both sides on "
                    "their join keys"
                )
            if len(distribution.hash_keys) != len(keys):
                raise PlanError(
                    f"{node.name()}: the {side} side is hashed on "
                    f"{len(distribution.hash_keys)} columns against {len(keys)} join keys"
                )

    return check


def all_of(*checks):
    """Run several checks in order; the first to fail names the fix."""

    def check(node) -> None:
        for one in checks:
            if one is not None:
                one(node)

    return check
