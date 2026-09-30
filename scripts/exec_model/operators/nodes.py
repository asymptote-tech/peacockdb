"""`GpuNode` implementations backed by the pandas operators, plus plan builders.

Same trait surface the mocks implement, so the same two drivers run both. Partition
layouts are declared here because the plan validator checks them; the executors never see
them.

Every builder records how it was called (`Recipe`), so a plan can be rebuilt with different
arguments — see `injection.py`. Layouts here are computed from constructor arguments and
captured in closures, not stored where they could be edited afterwards, so re-running the
builder is the only way to change one and keep the plan consistent.
"""

from __future__ import annotations

import functools
import inspect
import zlib
from dataclasses import dataclass
from typing import Callable, Iterable

from ..executors import Executor
from ..forwarder import (
    BatchForwarder,
    InterleaveForwarder,
    MergePartitionsForwarder,
    UnionForwarder,
)
from ..layout import (
    BatchLayout,
    ColumnOrder,
    KeyDistribution,
    KeyDistributionKind,
    NodeKind,
    PartitionLayout,
    SortOrder,
)
from ..layout import UniqueKeys, UniqueScope
from ..limit import RowInterval
from ..node import ExecutorBackends, ExecutorCategory, GpuNode, NodeExecutors
from ..schema import aggregate_schema, finalized_schema
from . import (
    accumulators,
    aggregates,
    exec_ops,
    joins,
    partition_ops,
    recipe_join,
    source,
    validation,
)


class PandasNode(GpuNode):
    def __init__(
        self,
        name: str,
        kind: NodeKind,
        layout: PartitionLayout | None,
        category: ExecutorCategory,
        children: Iterable[GpuNode] = (),
        factory: Callable[[int | None], Executor] | None = None,
        forwarder: BatchForwarder | None = None,
        row_interval: RowInterval | None = None,
        validator: Callable[["PandasNode"], None] | None = None,
        schema=None,
        recipe_factory: Callable[[int | None], Executor] | None = None,
    ):
        #: the second backend, where a node has one: an executor that answers by emitting
        #: FlatBuffers nodes and making `execute_node` calls (`recipe_join.py`). Only the
        #: joins carry one — the join is what the emulation was written to prove.
        self._recipe_factory = recipe_factory
        self._schema = schema
        self._row_interval = row_interval
        self._validator = validator
        self._name = name
        self._kind = kind
        self._layout = layout
        self._category = category
        self._children = list(children)
        self._factory = factory
        self._forwarder = forwarder
        #: set by the builder that made this node; None for a hand-constructed one
        self.recipe: Recipe | None = None

    def name(self) -> str:
        return self._name

    def kind(self) -> NodeKind:
        return self._kind

    def category(self) -> ExecutorCategory:
        return self._category

    def row_interval(self) -> RowInterval | None:
        return self._row_interval

    def output_partitions(self):
        return self._layout

    def output_schema(self):
        """Annotations, not types — see `schema.py`. None where nothing can be declared."""
        return self._schema

    def children(self):
        return self._children

    def make_executors(self) -> NodeExecutors:
        if self._forwarder is not None:
            return NodeExecutors(self._category, forwarder=self._forwarder)
        return NodeExecutors(
            self._category,
            backends=ExecutorBackends(cpu=self._factory, gpu=self._recipe_factory),
        )

    def validate_schemas_and_partitions(self) -> None:
        """What this node needs of its children — layout expectations and the aggregate
        state chain. Whole-tree facts (arity, lane agreement) stay in `plan.py`."""
        if self._validator is not None:
            self._validator(self)


@dataclass(frozen=True)
class Recipe:
    """The builder call that produced a node, so a rewriter can make the call again.

    `args` holds every parameter by name, children included, which is what lets a rewrite
    be expressed as "same call, these arguments replaced" without the rewriter knowing
    what any particular builder does with them.
    """

    builder: Callable[..., "PandasNode"]
    args: dict

    def child_args(self) -> dict:
        """The arguments that are child nodes, by parameter name."""
        found = {}
        for key, value in self.args.items():
            if isinstance(value, PandasNode):
                found[key] = value
            elif isinstance(value, (list, tuple)) and value and all(
                isinstance(item, PandasNode) for item in value
            ):
                found[key] = list(value)
        return found

    def rebuild(self, **overrides) -> "PandasNode":
        return self.builder(**{**self.args, **overrides})


def _records_its_recipe(builder):
    """Record the call on the node it returns."""
    signature = inspect.signature(builder)

    @functools.wraps(builder)
    def wrapper(*args, **kwargs):
        bound = signature.bind(*args, **kwargs)
        bound.apply_defaults()
        node = builder(*args, **kwargs)
        node.recipe = Recipe(wrapper, dict(bound.arguments))
        return node

    return wrapper


def _layout(n, batch_layout=BatchLayout.MULTIPLE_BATCHES, hash_keys=None, sort_by=None,
            unique_keys=(), distribution=None, order=None):
    return PartitionLayout(
        n=n,
        unique_keys=unique_keys,
        key_distribution=(
            distribution
            if distribution is not None
            else KeyDistribution.by_hash(range(len(hash_keys)))
            if hash_keys is not None
            else KeyDistribution.not_specified()
        ),
        sort_order=(
            SortOrder.batch_sorted(order) if order
            else SortOrder.batch_sorted([ColumnOrder(i) for i in range(len(sort_by))]) if sort_by
            else SortOrder.not_specified()
        ),
        batch_layout=batch_layout,
    )


def _lanes(child: PandasNode) -> int:
    return child.output_partitions().n


def _inherit(child: PandasNode):
    """A node that never moves a row between lanes keeps its child's distribution.

    Filtering, re-batching, sorting within a lane and folding a lane's batches together
    all leave every row where it was, so co-location survives them. Collapsing to one lane
    or building new columns does not, and those declare afresh.
    """
    return child.output_partitions().key_distribution


def _passthrough(child: PandasNode):
    """Re-laning and re-batching do not touch columns, so the declaration carries over."""
    return child.output_schema()


def _unique(schema, keys, scope: UniqueScope):
    """The key set an aggregate's output is unique on, at the scope its position gives it."""
    positions = tuple(schema.position_of(k) for k in keys)
    if any(p is None for p in positions):
        return ()
    return (UniqueKeys(positions, scope),)


# -- source -----------------------------------------------------------------------


@_records_its_recipe
def scan(
    name,
    frame,
    n_partitions=1,
    rows_per_group=4,
    target_batch_rows=None,
    empty_lanes=(),
    empty_batch_probability=0.0,
    seed=0,
) -> PandasNode:
    """A table split into row groups, mapped to (partition, batch) by the T2 policy.

    The last three arguments are shapes the policy would not choose but the runtime must
    survive: lanes with nothing to read, and zero-row batches arriving mid-stream.
    """
    row_groups = source.split_row_groups(len(frame), rows_per_group)
    mapping = source.partition_row_groups(row_groups, n_partitions, target_batch_rows)
    if empty_lanes:
        mapping = source.drain_lanes(mapping, empty_lanes)
    node = PandasNode(
        name,
        NodeKind.SOURCE,
        _layout(n_partitions),
        ExecutorCategory.SOURCE,
        factory=lambda lane: source.TableSource(
            frame,
            row_groups,
            mapping[lane],
            name,
            lane,
            empty_batch_probability,
            # Per lane, so lanes do not inject in lockstep, and derived from the name so
            # two scans in one plan differ. crc32 rather than hash(): str hashing is
            # salted per process and this has to reproduce.
            zlib.crc32(f"{name}.{lane}".encode(), seed),
        ),
    )
    node.partition_groups = mapping  # what the plan golden would render verbatim
    return node


@_records_its_recipe
def parquet_scan(name, frame, row_counts, partition_groups, limit=None) -> PandasNode:
    """An engine scan: a parquet file's own row groups, read as the planner mapped them.

    `frame` is the file's projected columns, every row in file order; `row_counts` its row
    groups' sizes; `partition_groups` the engine's lanes → batches → row groups, verbatim.
    `limit` is the one DataFusion pushed into the scan, which the planner gives one lane:
    over N lanes each would honour it and N times the rows would come out.
    """
    if sum(row_counts) != len(frame):
        raise ValueError(f"{name}: row groups hold {sum(row_counts)} rows, the frame {len(frame)}")
    if limit is not None and len(partition_groups) != 1:
        raise ValueError(f"{name}: a scan limit over {len(partition_groups)} lanes")
    row_groups = source.row_group_ranges(row_counts)
    node = PandasNode(
        name,
        NodeKind.SOURCE,
        _layout(len(partition_groups)),
        ExecutorCategory.SOURCE,
        factory=lambda lane: source.TableSource(
            frame, row_groups, partition_groups[lane], name, lane, limit=limit
        ),
    )
    node.partition_groups = partition_groups
    return node


@_records_its_recipe
def memory_source(name, frames, hash_positions=None) -> PandasNode:
    """A materialization, one frame per lane, laid out as it was made: one batch a lane, hashed
    on `hash_positions` where there are several lanes — so nothing above shuffles it again."""
    hashed = hash_positions is not None and len(frames) > 1
    return PandasNode(
        name,
        NodeKind.SOURCE,
        _layout(len(frames), batch_layout=BatchLayout.SINGLE_BATCH,
                distribution=KeyDistribution.by_hash(hash_positions) if hashed else None),
        ExecutorCategory.SOURCE,
        factory=lambda lane: source.MemorySource(frames[lane], name, lane),
    )


# -- exec -------------------------------------------------------------------------


@_records_its_recipe
def filter_(name, child, predicate, projection=None, sources=None) -> PandasNode:
    """`projection`, as an engine `GpuFilter` carries it: `Alias`es of the columns kept.
    `sources` as for `project`."""
    return PandasNode(
        name,
        NodeKind.INTERMEDIATE,
        (_layout(_lanes(child), distribution=_inherit(child)) if sources is None
         else _copied_layout(child, sources)),
        ExecutorCategory.EXEC,
        [child],
        factory=lambda lane: exec_ops.FilterExec(predicate, name, projection),
        schema=_passthrough(child) if projection is None else None,
    )


@_records_its_recipe
def project(name, child, exprs, sources=None) -> PandasNode:
    """`sources`, where given, is per output column the child ordinal it copies (`None`
    where it computes one): what lets the node declare the child's batching and hash, as an
    engine plan's layout does, rather than neither."""
    return PandasNode(
        name,
        NodeKind.INTERMEDIATE,
        _layout(_lanes(child)) if sources is None else _copied_layout(child, sources),
        ExecutorCategory.EXEC,
        [child],
        factory=lambda lane: exec_ops.ProjectExec(exprs, name),
    )


@_records_its_recipe
def sort(name, child, by, ascending=None, nulls_first=False, fetch=None, order=None) -> PandasNode:
    """`order`, where given, is the sort as positions (`ColumnOrder`s) — an engine plan's,
    declared as it is; a per-batch sort keeps its input's batching then, one batch to one."""
    batching = child.output_partitions().batch_layout if order else BatchLayout.MULTIPLE_BATCHES
    return PandasNode(
        name,
        NodeKind.INTERMEDIATE,
        _layout(_lanes(child), batch_layout=batching, sort_by=by, distribution=_inherit(child),
                order=order),
        ExecutorCategory.EXEC,
        [child],
        factory=lambda lane: exec_ops.SortExec(by, ascending, nulls_first, fetch, name),
        schema=_passthrough(child),
    )


@_records_its_recipe
def partial_aggregate(name, child, keys, aggs, grouping_sets=None) -> PandasNode:
    """`grouping_sets` makes this the expanding init: its output gains `__grouping_id`
    between the keys and the state, and every node above groups on keys + that column."""
    gid = aggregates.GROUPING_ID if grouping_sets else None
    schema = aggregate_schema(keys, aggs, gid)
    group_columns = list(keys) + ([gid] if gid else [])
    return PandasNode(
        name,
        NodeKind.INTERMEDIATE,
        # One row per group per batch, so the group columns are unique within a batch and
        # no further: the next batch groups the same keys again.
        _layout(_lanes(child),
                unique_keys=_unique(schema, group_columns, UniqueScope.PER_BATCH)),
        ExecutorCategory.EXEC,
        [child],
        factory=lambda lane: exec_ops.PartialAggregateExec(keys, aggs, name, grouping_sets),
        schema=schema,
    )


@_records_its_recipe
def limit(name, child, skip=0, fetch=None) -> PandasNode:
    """The mid-plan `GpuLimit`: one partition in, any number of batches, streaming out.

    Its input needs `GpuMergePartitions` beneath it — an interval over N lanes names no
    rows — but *not* a `GpuCoalesceAllBatches`. Requiring one batch would make
    `... JOIN (SELECT * FROM orders LIMIT 100)` read the whole of orders to answer a
    hundred rows. See `accumulators.LimitStream`.

    The output layout follows the input: a limit is a prefix of its stream, so it neither
    increases the batch count nor disturbs an order. Feeding only the sink it is not this
    node at all — `skip`/`fetch` go on `unload`.
    """
    child_layout = child.output_partitions()
    return PandasNode(
        name,
        NodeKind.INTERMEDIATE,
        PartitionLayout(
            n=1,
            key_distribution=KeyDistribution.not_specified(),
            sort_order=child_layout.sort_order,
            batch_layout=child_layout.batch_layout,
        ),
        ExecutorCategory.BATCH_ACCUMULATOR,
        [child],
        factory=lambda lane: accumulators.LimitStream(skip, fetch, name),
        row_interval=RowInterval(skip, fetch),
        schema=_passthrough(child),
        validator=validation.all_of(validation.one_partition_in,
                                    validation.prefix_is_meaningful),
    )


@_records_its_recipe
def unload(name, child, skip=0, fetch=None) -> PandasNode:
    """The boundary crossing, and where a root-adjacent limit's interval lives.

    `skip`/`fetch` are the limit: what the translation layer emits instead of a `GpuLimit`
    node when the limit feeds only the unload. Written here rather than as a rewrite of a
    limit node above, because that is what T4 will do — the node never exists.
    """
    return PandasNode(
        name,
        NodeKind.SINK,
        None,
        ExecutorCategory.UNLOAD,
        [child],
        factory=lambda lane: exec_ops.UnloadExec(name),
        row_interval=RowInterval(skip, fetch) if (skip or fetch is not None) else None,
        validator=validation.prefix_is_meaningful if fetch is not None else None,
    )


# -- accumulators -----------------------------------------------------------------


@_records_its_recipe
def coalesce_all(name, child, schema=None) -> PandasNode:
    return PandasNode(
        name,
        NodeKind.INTERMEDIATE,
        _layout(_lanes(child), batch_layout=BatchLayout.SINGLE_BATCH,
                distribution=_inherit(child)),
        ExecutorCategory.BATCH_ACCUMULATOR,
        [child],
        factory=lambda lane: accumulators.CoalesceAllBatches(name, schema),
        schema=_passthrough(child),
    )


@_records_its_recipe
def aggregate_batches(
    name, child, keys, aggs, final_exprs=None, schema=None,
    compact_bytes=accumulators.DEFAULT_COMPACT_BYTES,
) -> PandasNode:
    """Merges pre-aggregated state. `final_exprs` present = this node finalizes; absent =
    it emits state. There is no phase flag — see the spec's aggregate sequence."""
    declared = finalized_schema(keys, aggs) if final_exprs is not None else _passthrough(child)
    # Per lane the merge collapses every group it holds, so its keys are unique within the
    # lane. They are unique globally only when a shuffle put every row of a group in one
    # lane, which is exactly what a hash distribution over a subset of them means.
    hashed = child.output_partitions().key_distribution.is_subset_of(
        [p for p in ((declared.position_of(k) if declared else None) for k in keys)
         if p is not None]
    )
    scope = UniqueScope.GLOBAL if hashed or _lanes(child) == 1 else UniqueScope.PER_PARTITION
    return PandasNode(
        name,
        NodeKind.INTERMEDIATE,
        _layout(_lanes(child), batch_layout=BatchLayout.SINGLE_BATCH,
                distribution=_inherit(child),
                unique_keys=_unique(declared, keys, scope) if declared else ()),
        ExecutorCategory.BATCH_ACCUMULATOR,
        [child],
        factory=lambda lane: accumulators.AggregateBatches(
            keys, aggs, final_exprs, name, schema, compact_bytes
        ),
        schema=declared,
        validator=validation.all_of(
            validation.merges_its_own_partial(keys, aggs),
            validation.hash_keys_subset_of_groups(keys),
        ),
    )


@_records_its_recipe
def rebatch(name, child, target_rows) -> PandasNode:
    """Re-cut a lane's stream to `target_rows` — merging, splitting, or both (#139)."""
    return PandasNode(
        name,
        NodeKind.INTERMEDIATE,
        _layout(_lanes(child), distribution=_inherit(child)),
        ExecutorCategory.BATCH_ACCUMULATOR,
        [child],
        factory=lambda lane: accumulators.ReBatchToTarget(target_rows, name),
        schema=_passthrough(child),
    )


@_records_its_recipe
def accumulate_and_sort(name, child, by, ascending=None, nulls_first=False, fetch=None, schema=None,
                        order=None):
    return PandasNode(
        name,
        NodeKind.INTERMEDIATE,
        _layout(_lanes(child), batch_layout=BatchLayout.SINGLE_BATCH, sort_by=by,
                distribution=_inherit(child), order=order),
        ExecutorCategory.BATCH_ACCUMULATOR,
        [child],
        factory=lambda lane: accumulators.AccumulateBatchesAndSort(
            by, ascending, nulls_first, fetch, name, schema
        ),
        schema=_passthrough(child),
        validator=validation.sorted_input,
    )


@_records_its_recipe
def merge_sorted_partitions(name, child, by, ascending=None, nulls_first=False, fetch=None, schema=None,
                            order=None):
    n_in = _lanes(child)
    return PandasNode(
        name,
        NodeKind.INTERMEDIATE,
        _layout(1, batch_layout=BatchLayout.SINGLE_BATCH, sort_by=by, order=order),
        ExecutorCategory.PARTITION_ACCUMULATOR,
        [child],
        factory=lambda lane: accumulators.MergeSortedPartitions(
            n_in, by, ascending, nulls_first, fetch, name, schema
        ),
        schema=_passthrough(child),
        validator=validation.sorted_input,
    )


# -- partition ops ----------------------------------------------------------------


@_records_its_recipe
def merge_partitions(name, child) -> PandasNode:
    return PandasNode(
        name,
        NodeKind.INTERMEDIATE,
        _layout(1),
        ExecutorCategory.BATCH_FORWARDER,
        [child],
        forwarder=MergePartitionsForwarder(_lanes(child)),
        schema=_passthrough(child),
    )


@_records_its_recipe
def emit_partitions(name, child, keys, n_partitions, hash_fn=None, key_positions=None) -> PandasNode:
    """`hash_fn` defaults to the real placement; anything key-deterministic is legal.
    `key_positions`, where given, are the keys' ordinals — an engine plan's hash, declared
    where it is rather than as the leading columns."""
    distribution = None if key_positions is None else KeyDistribution.by_hash(key_positions)
    return PandasNode(
        name,
        NodeKind.INTERMEDIATE,
        _layout(n_partitions, hash_keys=keys, distribution=distribution),
        ExecutorCategory.PARTITION_EMITTER,
        [child],
        factory=lambda lane: partition_ops.EmitPartitions(keys, n_partitions, name, hash_fn),
        schema=_passthrough(child),
    )


@_records_its_recipe
def union(name, children) -> PandasNode:
    """Each output lane is one branch's lane, so what every branch declares of its lanes —
    their batching, the order within a batch — holds of the union's; the hash does not."""
    counts = [_lanes(c) for c in children]
    layouts = [c.output_partitions() for c in children]
    batching = {layout.batch_layout for layout in layouts}
    orders = {layout.sort_order for layout in layouts}
    return PandasNode(
        name,
        NodeKind.INTERMEDIATE,
        PartitionLayout(
            n=sum(counts),
            key_distribution=KeyDistribution.not_specified(),
            sort_order=orders.pop() if len(orders) == 1 else SortOrder.not_specified(),
            batch_layout=batching.pop() if len(batching) == 1 else BatchLayout.MULTIPLE_BATCHES,
        ),
        ExecutorCategory.BATCH_FORWARDER,
        children,
        forwarder=UnionForwarder(counts),
    )


@_records_its_recipe
def interleave(name, children) -> PandasNode:
    """Lane p is lane p of every branch, so a hash every branch carries is the output's."""
    n = _lanes(children[0])
    hashes = {c.output_partitions().key_distribution for c in children}
    return PandasNode(
        name,
        NodeKind.INTERMEDIATE,
        _layout(n, distribution=hashes.pop() if len(hashes) == 1 else None),
        ExecutorCategory.BATCH_FORWARDER,
        children,
        forwarder=InterleaveForwarder(len(children), n),
    )


# -- join -------------------------------------------------------------------------


@_records_its_recipe
def hash_join(
    name, build, probe, join_type, build_keys, probe_keys, null_equals_null=False,
    fanout=joins.TRIVIAL_FANOUT, probe_schema=None, residual=None,
) -> PandasNode:
    """`fanout` is the optimizer's cardinality estimate for this join, carried as a node
    property so the executor built from it can model its own scratch. `probe_schema` is
    the probe side's column list, consulted only when an outer finish saw no probe batch.
    `residual` is the non-equi predicate `CudfHashJoin.filter` carries, over the joined
    row's own column names."""
    return PandasNode(
        name,
        NodeKind.INTERMEDIATE,
        _layout(_lanes(build)),
        ExecutorCategory.JOIN,
        [build, probe],
        factory=lambda lane: joins.HashJoin(
            join_type, build_keys, probe_keys, null_equals_null, f"{name}.p{lane}", fanout,
            probe_schema, residual,
        ),
        recipe_factory=lambda lane: recipe_join.RecipeHashJoin(
            join_type, build_keys, probe_keys, null_equals_null, f"{name}.p{lane}", fanout,
            probe_schema, residual,
        ),
        validator=validation.co_partitioned_join(build_keys, probe_keys),
    )


@_records_its_recipe
def cross_join(name, build, probe) -> PandasNode:
    return PandasNode(
        name,
        NodeKind.INTERMEDIATE,
        _layout(_lanes(build)),
        ExecutorCategory.JOIN,
        [build, probe],
        factory=lambda lane: joins.CrossJoin(f"{name}.p{lane}"),
        recipe_factory=lambda lane: recipe_join.RecipeCrossJoin(f"{name}.p{lane}"),
    )


@_records_its_recipe
def nested_loop_join(name, build, probe, join_type, predicate) -> PandasNode:
    """`CudfNestedLoopJoin` — a cross product cut down by a predicate over both sides.

    Inner streams its probe side; Left takes a single-batch probe, so the planner puts a
    `GpuCoalesceAllBatches` under the probe side of one (see `join_types.capability`).
    """
    return PandasNode(
        name,
        NodeKind.INTERMEDIATE,
        _layout(_lanes(build)),
        ExecutorCategory.JOIN,
        [build, probe],
        factory=lambda lane: joins.NestedLoopJoin(join_type, predicate, f"{name}.p{lane}"),
        recipe_factory=lambda lane: recipe_join.RecipeNestedLoopJoin(
            join_type, predicate, f"{name}.p{lane}"
        ),
    )


# -- joins over an engine plan's positions ------------------------------------------------------


def _positional_join(name, build, probe, executor, validator=None, distribution=None) -> PandasNode:
    # No recipe executor: the FlatBuffers emulation is name-based, so the recipe backend
    # refuses these joins loudly rather than running them on pandas behind its back.
    return PandasNode(
        name,
        NodeKind.INTERMEDIATE,
        _layout(_lanes(build), distribution=distribution),
        ExecutorCategory.JOIN,
        [build, probe],
        factory=executor,
        validator=validator,
    )


@_records_its_recipe
def positional_hash_join(
    name, build, probe, columns, join_type, build_keys, probe_keys, null_equals_null=False,
    residual=None, fanout=joins.TRIVIAL_FANOUT,
) -> PandasNode:
    """An engine `GpuHashJoin`: keys and `residual` in `columns.joined` names, the output
    `columns.projection` of the join type's own, named `columns.output` (`joins.Positional`);
    `fanout` as `hash_join` has it."""
    return _positional_join(
        name, build, probe,
        lambda lane: joins.PositionalHashJoin(
            columns, join_type, build_keys, probe_keys, null_equals_null, f"{name}.p{lane}",
            fanout=fanout, residual=residual,
        ),
        validation.co_partitioned_join(build_keys, probe_keys),
        _join_distribution(columns, join_type, build_keys, probe_keys) if _lanes(build) > 1
        else KeyDistribution.not_specified(),
    )


#: The sides whose key every output row carries as it arrived, not null-padded, by join type.
_UNPADDED = {
    joins.JoinType.INNER: ("build", "probe"),
    joins.JoinType.LEFT: ("build",), joins.JoinType.LEFT_SEMI: ("build",),
    joins.JoinType.LEFT_ANTI: ("build",), joins.JoinType.LEFT_MARK: ("build",),
    joins.JoinType.RIGHT: ("probe",), joins.JoinType.RIGHT_SEMI: ("probe",),
    joins.JoinType.RIGHT_ANTI: ("probe",),
    joins.JoinType.FULL: (),
}


def _join_distribution(columns, join_type, build_keys, probe_keys):
    """A lane-wise join leaves each output row in the lane its keys hashed to, so its output
    is hashed on a side's keys wherever every row carries them unpadded and the projection
    keeps them. One lane is no hash, as the engine declares it."""
    own = joins.own_output(join_type, columns.build_width, len(columns.joined) - columns.build_width)
    picked = own if columns.projection is None else [own[i] for i in columns.projection]
    at = {joined: output for output, joined in enumerate(picked) if joined is not None}
    for side in _UNPADDED[join_type]:
        keys = build_keys if side == "build" else probe_keys
        positions = [at.get(columns.joined.index(key)) for key in keys]
        if None not in positions:
            return KeyDistribution.by_hash(positions)
    return KeyDistribution.not_specified()


@_records_its_recipe
def positional_nested_loop_join(name, build, probe, columns, join_type, predicate) -> PandasNode:
    return _positional_join(
        name, build, probe,
        lambda lane: joins.PositionalNestedLoopJoin(columns, join_type, predicate, f"{name}.p{lane}"),
    )


@_records_its_recipe
def positional_cross_join(name, build, probe, columns) -> PandasNode:
    return _positional_join(
        name, build, probe, lambda lane: joins.PositionalCrossJoin(columns, f"{name}.p{lane}")
    )


# -- aggregates as an engine plan states them --------------------------------------------------


def _copied_layout(child, sources) -> PartitionLayout:
    """A one-batch-in, one-batch-out node's layout over the columns `sources` copies."""
    return _layout(_lanes(child), batch_layout=child.output_partitions().batch_layout,
                   distribution=_kept_hash(child, list(sources)))


def _kept_hash(child, sources) -> KeyDistribution:
    """The child's hash, carried to the output positions `sources` (child ordinals, per
    output position) put its hashed columns at — or none where one of them is not kept."""
    distribution = child.output_partitions().key_distribution
    if distribution.kind is not KeyDistributionKind.BY_HASH:
        return KeyDistribution.not_specified()
    if any(key not in sources for key in distribution.hash_keys):
        return KeyDistribution.not_specified()
    return KeyDistribution.by_hash(sources.index(key) for key in distribution.hash_keys)


@_records_its_recipe
def plan_aggregate(name, child, body, key_sources=()) -> PandasNode:
    """An engine `GpuAggregate`, one batch to one batch (`aggregates.PlanAggregate`).
    `key_sources` are the child ordinals its keys read: rows grouped on the columns the child
    was hashed on stay in their lanes, unless grouping sets null those keys out."""
    layout = child.output_partitions()
    distribution = (KeyDistribution.not_specified() if body.masks is not None
                    else _kept_hash(child, list(key_sources)))
    return PandasNode(
        name,
        NodeKind.INTERMEDIATE,
        _layout(_lanes(child), batch_layout=layout.batch_layout, distribution=distribution),
        ExecutorCategory.EXEC,
        [child],
        factory=lambda lane: exec_ops.PlanAggregateExec(body, name),
    )


@_records_its_recipe
def plan_aggregate_batches(name, child, body, schema=None,
                           compact_bytes=accumulators.DEFAULT_COMPACT_BYTES) -> PandasNode:
    """An engine `GpuAggregateBatches`: its state is its input's layout, keys first, so the
    child's hash survives wherever it is on keys alone."""
    n_keys = len(body.state) - sum(len(call.outputs) for call in body.calls)
    return PandasNode(
        name,
        NodeKind.INTERMEDIATE,
        _layout(_lanes(child), batch_layout=BatchLayout.SINGLE_BATCH,
                distribution=_kept_hash(child, list(range(n_keys)))),
        ExecutorCategory.BATCH_ACCUMULATOR,
        [child],
        factory=lambda lane: accumulators.PlanAggregateBatches(body, name, schema, compact_bytes),
    )
