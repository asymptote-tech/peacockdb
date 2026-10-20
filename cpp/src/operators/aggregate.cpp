// CudfAggregate -- group-by and scalar (whole-table) aggregation.
//
// One request builder, `add_requests`, turns a function node and the node's phase into cuDF
// requests and the state columns they fill, read where the node's args point and named from
// its state_names (#225, #280). The grouped and grouping-set paths run it, and so does a
// keyless node holding a stddev or var, on one constant key; any other keyless node reduces.

#include "peacock/operators.h"
#include "peacock/expr.h"

#include <cudf/groupby.hpp>
#include <cudf/reduction.hpp>
#include <cudf/replace.hpp>
#include <cudf/table/table.hpp>
#include <cudf/concatenate.hpp>
#include <cudf/unary.hpp>
#include <cudf/column/column_factories.hpp>
#include <cudf/scalar/scalar_factories.hpp>
#include <cudf/utilities/default_stream.hpp>

#include <algorithm>
#include <cstdint>
#include <numeric>
#include <stdexcept>
#include <string>

namespace peacock {

// What a node does with its rows: Partial builds state from values, Merge merges state into
// state. The planner writes nothing else; every aggregate finalizes in a project (#25).
enum class AggPhase { Partial, Merge };

static AggPhase agg_phase(fb::AggregateMode mode) {
  switch (mode) {
    case fb::AggregateMode_Partial:
      return AggPhase::Partial;
    case fb::AggregateMode_Merge:
      return AggPhase::Merge;
    default:
      throw std::runtime_error(std::string("CudfAggregate: mode ") +
                               fb::EnumNameAggregateMode(mode) +
                               " is not one the planner writes — an aggregate builds state "
                               "(Partial) or merges it (Merge), and finalizes in a project");
  }
}

static const char* phase_name(AggPhase phase) {
  return phase == AggPhase::Merge ? "Merge" : "Partial";
}

/// What a function node runs, by the name state_funcs gives it.
enum class AggKind { Sum, Min, Max, Count, Welford };

static AggKind agg_kind(std::string const& name) {
  if (name == "sum") return AggKind::Sum;
  if (name == "min") return AggKind::Min;
  if (name == "max") return AggKind::Max;
  if (name == "count") return AggKind::Count;
  if (name == "stddev" || name == "stddev_pop" || name == "var" || name == "var_pop")
    return AggKind::Welford;
  throw std::runtime_error("unsupported aggregate function: " + name);
}

// What `func` reads and emits, checked against its kind: a node's state is never located by
// counting, so a function of another arity would read or name the wrong columns.
static void check_shape(const fb::AggregateFuncNode* func, std::string const& name, AggKind kind,
                        AggPhase phase) {
  if (kind == AggKind::Count && phase == AggPhase::Merge)
    throw std::runtime_error(
        "CudfAggregate: `count` at Merge: the plan merges a count's state "
        "with `sum`, so no count reaches a Merge");
  const bool welford = kind == AggKind::Welford;
  const size_t want_args = welford && phase == AggPhase::Merge ? 3 : 1;
  const size_t args = func->args() ? func->args()->size() : 0;
  if (args != want_args)
    throw std::runtime_error("CudfAggregate: `" + name + "` at " + phase_name(phase) + " reads " +
                             std::to_string(want_args) + " args and was given " +
                             std::to_string(args));
  const size_t want_states = welford ? 3 : 1;
  const size_t states = func->state_names() ? func->state_names()->size() : 0;
  if (states != want_states)
    throw std::runtime_error("CudfAggregate: `" + name + "` emits " + std::to_string(want_states) +
                             " state columns and names " + std::to_string(states));
}

// The column `arg` names in `input`, borrowed, or the column it computes over `input`, kept in
// `owned` for as long as the requests view it.
static cudf::column_view value_of(const fb::Expr* arg, cudf::table_view input,
                                  std::vector<std::string> const& names,
                                  std::vector<std::unique_ptr<cudf::column>>& owned) {
  if (arg->node_type() == fb::ExprNode_ColumnRef)
    return column_at(input, names, arg->node_as_ColumnRef());
  // DataFusion inlines a computed argument (no preceding ProjectionExec).
  owned.push_back(build_column(arg, input, names));
  return owned.back()->view();
}

/// How one state column is read back out of the groupby's results.
enum class StateReadback {
  AsIs,
  CountToInt64,  // cuDF's COUNT answers INT32; every count on the wire is INT64
  NullAsZero,    // a Welford init's mean or m2: NULL over a group of no valid value, 0.0 here
  MergedChild,   // one child of MERGE_M2's struct: the count widened to INT64, a moment NullAsZero
};

struct StateColumn {
  std::string name;  // from state_names
  size_t request;    // into AggregateRequests::requests
  size_t result;     // into that request's aggregations
  StateReadback readback;
  int child = -1;  // MergedChild only
};

/// A node's cuDF requests and the state columns they fill, in state order, plus every column a
/// request's view points into.
struct AggregateRequests {
  std::vector<cudf::groupby::aggregation_request> requests;
  std::vector<StateColumn> columns;
  std::vector<std::unique_ptr<cudf::column>> owned;
};

/// MERGE_M2's input: a Welford state's three columns as one struct, its count in
/// kMergeM2CountType (plan_executor_internal.h), the type the linked cuDF takes;
/// as_int64_count widens the merged count back to the wire's INT64, a no-op from 25.06. The one
/// place a MERGE_M2 input is built.
static std::unique_ptr<cudf::column> merge_m2_input(cudf::column_view count, cudf::column_view mean,
                                                    cudf::column_view m2) {
  // A struct owns its children, so the moments are copied in: per-group state (#154).
  std::vector<std::unique_ptr<cudf::column>> members;
  members.push_back(cudf::cast(count, cudf::data_type{kMergeM2CountType}));
  members.push_back(std::make_unique<cudf::column>(mean));
  members.push_back(std::make_unique<cudf::column>(m2));
  return cudf::make_structs_column(count.size(), std::move(members), 0, rmm::device_buffer{});
}

/// Appends `func`'s requests and state columns to `into`, evaluating each computed argument
/// (and a Welford's FLOAT64 cast, a merge's MERGE_M2 struct) into `into.owned` once. Every input
/// is the column its args name in `input` (whose columns are `names`).
static void add_requests(const fb::AggregateFuncNode* func, AggPhase phase, cudf::table_view input,
                         std::vector<std::string> const& names, AggregateRequests& into) {
  const std::string name = func->name() ? func->name()->str() : std::string();
  const AggKind kind = agg_kind(name);
  check_shape(func, name, kind, phase);
  auto state = [&](flatbuffers::uoffset_t i) { return func->state_names()->Get(i)->str(); };
  auto arg = [&](flatbuffers::uoffset_t i) {
    return value_of(func->args()->Get(i), input, names, into.owned);
  };
  const size_t r = into.requests.size();
  cudf::groupby::aggregation_request req;
  switch (kind) {
    case AggKind::Sum:
      req.values = arg(0);
      req.aggregations.push_back(cudf::make_sum_aggregation<cudf::groupby_aggregation>());
      into.columns.push_back({state(0), r, 0, StateReadback::AsIs});
      break;
    case AggKind::Min:
      req.values = arg(0);
      req.aggregations.push_back(cudf::make_min_aggregation<cudf::groupby_aggregation>());
      into.columns.push_back({state(0), r, 0, StateReadback::AsIs});
      break;
    case AggKind::Max:
      req.values = arg(0);
      req.aggregations.push_back(cudf::make_max_aggregation<cudf::groupby_aggregation>());
      into.columns.push_back({state(0), r, 0, StateReadback::AsIs});
      break;
    case AggKind::Count:
      // Partial only: check_shape refused a count at Merge.
      req.values = arg(0);
      req.aggregations.push_back(cudf::make_count_aggregation<cudf::groupby_aggregation>());
      into.columns.push_back({state(0), r, 0, StateReadback::CountToInt64});
      break;
    case AggKind::Welford:
      if (phase == AggPhase::Partial) {
        // DataFusion's stddev and var are float-valued: the moments of the value as FLOAT64.
        into.owned.push_back(cudf::cast(arg(0), cudf::data_type{cudf::type_id::FLOAT64}));
        req.values = into.owned.back()->view();
        req.aggregations.push_back(cudf::make_count_aggregation<cudf::groupby_aggregation>());
        req.aggregations.push_back(cudf::make_mean_aggregation<cudf::groupby_aggregation>());
        req.aggregations.push_back(cudf::make_m2_aggregation<cudf::groupby_aggregation>());
        into.columns.push_back({state(0), r, 0, StateReadback::CountToInt64});
        into.columns.push_back({state(1), r, 1, StateReadback::NullAsZero});
        into.columns.push_back({state(2), r, 2, StateReadback::NullAsZero});
      } else {
        into.owned.push_back(merge_m2_input(arg(0), arg(1), arg(2)));
        req.values = into.owned.back()->view();
        req.aggregations.push_back(cudf::make_merge_m2_aggregation<cudf::groupby_aggregation>());
        for (int child = 0; child < 3; ++child)
          into.columns.push_back({state(child), r, 0, StateReadback::MergedChild, child});
      }
      break;
  }
  into.requests.push_back(std::move(req));
}

/// The node's requests, every function's in order.
static AggregateRequests node_requests(const fb::CudfAggregate* agg, AggPhase phase,
                                       cudf::table_view input,
                                       std::vector<std::string> const& names) {
  AggregateRequests built;
  if (agg->aggr_funcs())
    for (auto const* func : *agg->aggr_funcs()) add_requests(func, phase, input, names, built);
  return built;
}

static std::unique_ptr<cudf::column> as_int64(std::unique_ptr<cudf::column> count) {
  return count->type().id() == cudf::type_id::INT32
             ? cudf::cast(count->view(), cudf::data_type{cudf::type_id::INT64})
             : std::move(count);
}

// One child of a merged struct as a column of its own: an INT32 count cast straight from the
// view, anything else copied, since the struct owns its children (#154).
static std::unique_ptr<cudf::column> as_int64_count(cudf::column_view child) {
  return child.type().id() == cudf::type_id::INT32
             ? cudf::cast(child, cudf::data_type{cudf::type_id::INT64})
             : std::make_unique<cudf::column>(child);
}

// DataFusion's Welford state is never NULL: a group of no valid value is (0, 0.0, 0.0). cuDF
// answers NULL there (MEAN and M2 over no valid value, MERGE_M2 where every merged count is 0),
// so a moment's NULL is 0.0 and no MERGE_M2 downstream meets a NULL child. NaN stays NaN.
static std::unique_ptr<cudf::column> zero_for_null(std::unique_ptr<cudf::column> moment) {
  if (moment->null_count() == 0) return moment;
  return cudf::replace_nulls(moment->view(), cudf::numeric_scalar<double>(0.0));
}

/// The state columns `built` declared, read out of `results`, appended in order. Reads only
/// `built.columns`, so one `built` serves every grouping set's results.
static void append_state_columns(AggregateRequests const& built,
                                 std::vector<cudf::groupby::aggregation_result>& results,
                                 std::vector<std::unique_ptr<cudf::column>>& columns,
                                 std::vector<std::string>& names) {
  for (auto const& state : built.columns) {
    auto& result = results.at(state.request).results.at(state.result);
    switch (state.readback) {
      case StateReadback::AsIs:
        columns.push_back(std::move(result));
        break;
      case StateReadback::CountToInt64:
        columns.push_back(as_int64(std::move(result)));
        break;
      case StateReadback::NullAsZero:
        columns.push_back(zero_for_null(std::move(result)));
        break;
      case StateReadback::MergedChild: {
        // Three columns read one struct, so none can take its child over.
        auto child = result->view().child(state.child);
        columns.push_back(state.child == 0 ? as_int64_count(child)
                                           : zero_for_null(std::make_unique<cudf::column>(child)));
        break;
      }
    }
    names.push_back(state.name);
  }
}

/// The groupby over `keys`: the key columns under `key_names`, then every function's state
/// columns.
static TableResult grouped_aggregate(const fb::CudfAggregate* agg, AggPhase phase,
                                     std::vector<cudf::column_view> const& keys,
                                     std::vector<std::string> key_names, cudf::table_view input,
                                     std::vector<std::string> const& names) {
  auto built = node_requests(agg, phase, input, names);
  // INCLUDE: SQL puts NULL keys in a group of their own; cuDF's default drops those rows
  // (tpcds q15's NULL ca_zip).
  cudf::groupby::groupby gb{cudf::table_view{keys}, cudf::null_policy::INCLUDE};
  auto [group_keys, results] = gb.aggregate(built.requests);
  // Groupby's key table is fresh and ours, so its columns are handed over (#154).
  auto columns = group_keys->release();
  append_state_columns(built, results, columns, key_names);
  return TableResult::owning(std::make_unique<cudf::table>(std::move(columns)),
                             std::move(key_names));
}

// DataFusion's `__grouping_id` (`group_id_array`, datafusion-physical-plan aggregates/mod.rs):
// the mask folds in key order, the first key highest, held at the narrowest unsigned width
// with a bit per key (`Aggregate::grouping_id_type`). GROUPING() over every key reads it, so
// the value is an answer. DataFusion refuses past 64 keys, and so does this.
static std::unique_ptr<cudf::column> grouping_id_column(const flatbuffers::Vector<uint8_t>* mask,
                                                        cudf::size_type nkeys,
                                                        cudf::size_type rows) {
  if (nkeys > 64)
    throw std::runtime_error("grouping sets over " + std::to_string(nkeys) +
                             " keys: DataFusion's grouping id holds at most 64");
  uint64_t gid = 0;
  for (cudf::size_type i = 0; i < nkeys; ++i) gid = (gid << 1) | (mask->Get(i) ? 1u : 0u);
  auto column = [&](auto width) {
    using T = decltype(width);
    cudf::numeric_scalar<T> id(static_cast<T>(gid), true);
    return cudf::make_column_from_scalar(id, rows);
  };
  if (nkeys <= 8) return column(uint8_t{});
  if (nkeys <= 16) return column(uint16_t{});
  if (nkeys <= 32) return column(uint32_t{});
  return column(uint64_t{});
}

// The table a done call aggregates: the node's declared input at zero rows, so a keyless
// node answers its identity row through the same reduce a batch takes, and a grouped one no
// groups. A decimal takes its scale from the field, which the type id cannot carry.
static TableResult zero_rows_of(const fb::CudfAggregate* agg) {
  const auto* schema = agg->aggr_input_schema();
  if (!schema || !schema->fields())
    throw std::runtime_error(
        "CudfAggregate handed no input carries no aggr_input_schema to build one from");
  std::vector<std::unique_ptr<cudf::column>> columns;
  std::vector<std::string> names;
  for (const auto* field : *schema->fields()) {
    std::string name = field->name() ? field->name()->str() : "";
    cudf::type_id id = fb_to_type_id(field->data_type());
    if (id == cudf::type_id::EMPTY)
      throw std::runtime_error("CudfAggregate handed no input: its input column `" + name +
                               "` has no cuDF type");
    cudf::data_type type = id == cudf::type_id::DECIMAL128
                               ? cudf::data_type{id, -static_cast<int32_t>(field->decimal_scale())}
                               : cudf::data_type{id};
    columns.push_back(cudf::make_empty_column(type));
    names.push_back(std::move(name));
  }
  return TableResult::owning(std::make_unique<cudf::table>(std::move(columns)), std::move(names));
}

/// One groupby per grouping set over the one set of requests the node's functions build (so a
/// computed argument is evaluated once per node, not once per set), NULL placeholders for the
/// masked keys, grouping_id_column after the keys, concatenated (architecture.md, "Grouping
/// sets").
static TableResult grouping_set_aggregate(const fb::CudfAggregate* agg, AggPhase phase,
                                          std::vector<cudf::column_view> const& keys,
                                          std::vector<std::string> const& key_names,
                                          cudf::table_view input,
                                          std::vector<std::string> const& names) {
  auto* sets = agg->grouping_sets();
  const auto nkeys = static_cast<cudf::size_type>(keys.size());
  if (agg->null_exprs()->size() != static_cast<flatbuffers::uoffset_t>(nkeys))
    throw std::runtime_error("grouping sets: null_exprs length != group_exprs");
  // A masked position contributes an all-NULL key of its type, concatenable with the real one.
  std::vector<std::unique_ptr<cudf::column>> placeholders;
  for (cudf::size_type i = 0; i < nkeys; ++i)
    placeholders.push_back(build_column(agg->null_exprs()->Get(i), input, names));
  auto built = node_requests(agg, phase, input, names);
  std::vector<std::unique_ptr<cudf::table>> set_tables;
  std::vector<std::string> out_names;
  for (auto const* set : *sets) {
    auto* mask = set->values();
    if (!mask || mask->size() != static_cast<flatbuffers::uoffset_t>(nkeys))
      throw std::runtime_error("grouping set mask length != group_exprs length");
    std::vector<cudf::column_view> set_keys;
    for (cudf::size_type i = 0; i < nkeys; ++i)
      set_keys.push_back(mask->Get(i) ? placeholders[i]->view() : keys[i]);
    cudf::groupby::groupby gb{cudf::table_view{set_keys}, cudf::null_policy::INCLUDE};
    auto [group_keys, results] = gb.aggregate(built.requests);
    // The row count is read before the release empties the key table.
    const auto rows = group_keys->num_rows();
    auto columns = group_keys->release();
    columns.push_back(grouping_id_column(mask, nkeys, rows));
    out_names = key_names;
    out_names.push_back("__grouping_id");
    append_state_columns(built, results, columns, out_names);
    set_tables.push_back(std::make_unique<cudf::table>(std::move(columns)));
  }
  std::vector<cudf::table_view> views;
  for (auto const& t : set_tables) views.push_back(t->view());
  return TableResult::owning(cudf::concatenate(views), std::move(out_names));
}

/// One row: `cudf::reduce` per function, its argument read through its args. A count is
/// counted and a decimal sum is a one-group groupby. A node holding a Welford never gets here:
/// it is keyless_welford_aggregate's.
static TableResult keyless_aggregate(const fb::CudfAggregate* agg, AggPhase phase,
                                     cudf::table_view input,
                                     std::vector<std::string> const& names) {
  std::vector<std::unique_ptr<cudf::column>> owned;
  std::vector<std::unique_ptr<cudf::column>> out_cols;
  std::vector<std::string> out_names;
  if (agg->aggr_funcs()) {
    for (auto const* func : *agg->aggr_funcs()) {
      const std::string name = func->name() ? func->name()->str() : std::string();
      check_shape(func, name, agg_kind(name), phase);
      cudf::column_view values = value_of(func->args()->Get(0), input, names, owned);
      std::unique_ptr<cudf::column> result;
      if (name == "count") {
        // size - null_count: make_count_aggregation<reduce_aggregation> is not exported in
        // every cuDF version.
        cudf::numeric_scalar<int64_t> s(values.size() - values.null_count(), true);
        result = cudf::make_column_from_scalar(s, 1);
      } else if (name == "sum" && values.type().id() == cudf::type_id::DECIMAL128) {
        // cudf::reduce sums no fixed_point type; the groupby does, over one constant key.
        // Over zero rows SQL's sum is NULL, where a groupby would emit no group.
        if (values.size() == 0) {
          result = cudf::make_column_from_scalar(
              *cudf::make_default_constructed_scalar(values.type()), 1);
        } else {
          cudf::numeric_scalar<int32_t> zero(0, true);
          auto key = cudf::make_column_from_scalar(zero, values.size());
          cudf::groupby::groupby gb{cudf::table_view{{key->view()}}};
          std::vector<cudf::groupby::aggregation_request> reqs(1);
          reqs[0].values = values;
          reqs[0].aggregations.push_back(cudf::make_sum_aggregation<cudf::groupby_aggregation>());
          auto [group_keys, results] = gb.aggregate(reqs);
          result = std::move(results[0].results[0]);
        }
      } else {
        std::unique_ptr<cudf::reduce_aggregation> ragg;
        if (name == "sum") ragg = cudf::make_sum_aggregation<cudf::reduce_aggregation>();
        if (name == "min") ragg = cudf::make_min_aggregation<cudf::reduce_aggregation>();
        if (name == "max") ragg = cudf::make_max_aggregation<cudf::reduce_aggregation>();
        if (!ragg) throw std::runtime_error("unsupported aggregate function: " + name);
        auto s = cudf::reduce(values, *ragg, values.type());
        result = cudf::make_column_from_scalar(*s, 1);
      }
      out_cols.push_back(std::move(result));
      out_names.push_back(func->state_names()->Get(0)->str());
    }
  }
  return TableResult::owning(std::make_unique<cudf::table>(std::move(out_cols)),
                             std::move(out_names));
}

/// Whether any function of `agg` is a stddev or var: what sends a keyless node to the builder.
static bool holds_a_welford(const fb::CudfAggregate* agg) {
  if (!agg->aggr_funcs()) return false;
  return std::any_of(agg->aggr_funcs()->begin(), agg->aggr_funcs()->end(), [](auto const* func) {
    return func->name() && agg_kind(func->name()->str()) == AggKind::Welford;
  });
}

// The row a routed node owes over no rows: plan/aggregates.rs's empty_state, which the identity
// cases hold both engines to. Typed as the groupby typed its zero-row answer `no_groups`, which
// keeps a decimal's scale, and named as it was, from state_names.
static TableResult keyless_empty_state(const fb::CudfAggregate* agg, TableResult const& no_groups) {
  auto one = [](cudf::scalar const& value) { return cudf::make_column_from_scalar(value, 1); };
  std::vector<std::unique_ptr<cudf::column>> columns;
  for (auto const* func : *agg->aggr_funcs()) {
    switch (agg_kind(func->name()->str())) {
      case AggKind::Count:  // Partial only: check_shape refused a count at Merge
        columns.push_back(one(cudf::numeric_scalar<int64_t>(0)));
        break;
      case AggKind::Welford:
        columns.push_back(one(cudf::numeric_scalar<int64_t>(0)));
        columns.push_back(one(cudf::numeric_scalar<double>(0.0)));
        columns.push_back(one(cudf::numeric_scalar<double>(0.0)));
        break;
      case AggKind::Sum:
      case AggKind::Min:
      case AggKind::Max: {
        // A default-constructed scalar is invalid: the NULL, at the column's type and scale.
        auto type = no_groups.view().column(static_cast<cudf::size_type>(columns.size())).type();
        columns.push_back(one(*cudf::make_default_constructed_scalar(type)));
        break;
      }
    }
  }
  return TableResult::owning(std::make_unique<cudf::table>(std::move(columns)),
                             no_groups.column_names);
}

// cuDF has M2 and MERGE_M2 for groupby only, so a keyless node holding a stddev or var runs
// whole through the grouped builder, every row under one constant key, dropped after (#216).
static TableResult keyless_welford_aggregate(const fb::CudfAggregate* agg, AggPhase phase,
                                             cudf::table_view input,
                                             std::vector<std::string> const& names) {
  auto key = cudf::make_column_from_scalar(cudf::numeric_scalar<int32_t>(0), input.num_rows());
  TableResult grouped = grouped_aggregate(agg, phase, {key->view()}, {"__keyless"}, input, names);
  std::vector<cudf::size_type> state(static_cast<size_t>(grouped.num_columns() - 1));
  std::iota(state.begin(), state.end(), 1);
  TableResult keyless = grouped.select(state);
  return keyless.num_rows() == 0 ? keyless_empty_state(agg, keyless) : keyless;
}

TableResult execute_aggregate(const fb::CudfAggregate* agg, NodeInputs* in) {
  const AggPhase phase = agg_phase(agg->mode());
  // Only an init's done call is handed nothing; a merge so handed still refuses.
  const bool done_call = was_handed_nothing(in) && phase == AggPhase::Partial;
  auto input = done_call ? zero_rows_of(agg) : take_input(in);
  auto tv = input.view();
  std::vector<cudf::column_view> keys;
  std::vector<std::string> key_names;
  if (agg->group_exprs()) {
    for (flatbuffers::uoffset_t i = 0; i < agg->group_exprs()->size(); ++i) {
      auto* expr = agg->group_exprs()->Get(i);
      if (expr->node_type() != fb::ExprNode_ColumnRef)
        throw std::runtime_error("CudfAggregate: only ColumnRef group exprs supported");
      auto* ref = expr->node_as_ColumnRef();
      keys.push_back(column_at(tv, input.column_names, ref));
      key_names.push_back(agg->group_names() && i < agg->group_names()->size()
                              ? agg->group_names()->Get(i)->str()
                              : ref->name()->str());
    }
  }
  if (keys.empty())
    return holds_a_welford(agg) ? keyless_welford_aggregate(agg, phase, tv, input.column_names)
                                : keyless_aggregate(agg, phase, tv, input.column_names);
  // Non-empty null_exprs, not grouping_sets, is the discriminator: a plain GROUP BY and a
  // grouping set's merge carry one all-false mask and no null_exprs.
  if (agg->null_exprs() && agg->null_exprs()->size() > 0 && agg->grouping_sets() &&
      agg->grouping_sets()->size() > 0)
    return grouping_set_aggregate(agg, phase, keys, key_names, tv, input.column_names);
  return grouped_aggregate(agg, phase, keys, std::move(key_names), tv, input.column_names);
}

}  // namespace peacock
