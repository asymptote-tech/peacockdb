#include "plan_executor.h"
#include "generated/gpu_plan_generated.h"

#include <cudf/ast/expressions.hpp>
#include <cudf/column/column_view.hpp>
#include <cudf/copying.hpp>
#include <cudf/groupby.hpp>
#include <cudf/io/parquet.hpp>
#if __has_include(<cudf/join/join.hpp>)
#include <cudf/join/join.hpp>
#else
#include <cudf/join.hpp>
#endif
#include <cudf/scalar/scalar.hpp>
#include <cudf/scalar/scalar_factories.hpp>
#include <cudf/sorting.hpp>
#include <cudf/stream_compaction.hpp>
#include <cudf/table/table.hpp>
#include <cudf/table/table_view.hpp>
#include <cudf/transform.hpp>
#include <cudf/types.hpp>

#include <stdexcept>

namespace peacock {
namespace fb = peacock::plan;

// ============================================================================
// Expression evaluation context
// ============================================================================

/// Owns all AST sub-expressions so references remain valid for cuDF.
struct ExprContext {
  std::vector<std::unique_ptr<cudf::ast::expression>> owned;
  std::vector<std::unique_ptr<cudf::scalar>> scalars;

  cudf::ast::expression& keep(std::unique_ptr<cudf::ast::expression> e) {
    owned.push_back(std::move(e));
    return *owned.back();
  }
};

// Forward declarations
static TableResult execute_node(const fb::PlanNode* node);
static cudf::ast::expression& build_expr(const fb::Expr* expr, ExprContext& ctx);

// ============================================================================
// FlatBuffer DataType → cuDF type_id
// ============================================================================

static cudf::type_id fb_to_type_id(fb::DataType dt) {
  switch (dt) {
    case fb::DataType_Boolean:    return cudf::type_id::BOOL8;
    case fb::DataType_Int8:       return cudf::type_id::INT8;
    case fb::DataType_Int16:      return cudf::type_id::INT16;
    case fb::DataType_Int32:      return cudf::type_id::INT32;
    case fb::DataType_Int64:      return cudf::type_id::INT64;
    case fb::DataType_UInt8:      return cudf::type_id::UINT8;
    case fb::DataType_UInt16:     return cudf::type_id::UINT16;
    case fb::DataType_UInt32:     return cudf::type_id::UINT32;
    case fb::DataType_UInt64:     return cudf::type_id::UINT64;
    case fb::DataType_Float32:    return cudf::type_id::FLOAT32;
    case fb::DataType_Float64:    return cudf::type_id::FLOAT64;
    case fb::DataType_Utf8:
    case fb::DataType_LargeUtf8:
    case fb::DataType_Utf8View:   return cudf::type_id::STRING;
    case fb::DataType_Date32:     return cudf::type_id::TIMESTAMP_DAYS;
    case fb::DataType_Date64:     return cudf::type_id::TIMESTAMP_MILLISECONDS;
    case fb::DataType_Decimal128: return cudf::type_id::DECIMAL128;
    default:                      return cudf::type_id::EMPTY;
  }
}

// ============================================================================
// FlatBuffer BinaryOp → cuDF AST operator
// ============================================================================

static cudf::ast::ast_operator fb_to_ast_op(fb::BinaryOp op) {
  switch (op) {
    case fb::BinaryOp_Eq:     return cudf::ast::ast_operator::EQUAL;
    case fb::BinaryOp_NotEq:  return cudf::ast::ast_operator::NOT_EQUAL;
    case fb::BinaryOp_Lt:     return cudf::ast::ast_operator::LESS;
    case fb::BinaryOp_LtEq:   return cudf::ast::ast_operator::LESS_EQUAL;
    case fb::BinaryOp_Gt:     return cudf::ast::ast_operator::GREATER;
    case fb::BinaryOp_GtEq:   return cudf::ast::ast_operator::GREATER_EQUAL;
    case fb::BinaryOp_Plus:   return cudf::ast::ast_operator::ADD;
    case fb::BinaryOp_Minus:  return cudf::ast::ast_operator::SUB;
    case fb::BinaryOp_Multiply: return cudf::ast::ast_operator::MUL;
    case fb::BinaryOp_Divide: return cudf::ast::ast_operator::DIV;
    case fb::BinaryOp_Modulo: return cudf::ast::ast_operator::MOD;
    case fb::BinaryOp_And:    return cudf::ast::ast_operator::LOGICAL_AND;
    case fb::BinaryOp_Or:     return cudf::ast::ast_operator::LOGICAL_OR;
    case fb::BinaryOp_BitwiseAnd: return cudf::ast::ast_operator::BITWISE_AND;
    case fb::BinaryOp_BitwiseOr:  return cudf::ast::ast_operator::BITWISE_OR;
    case fb::BinaryOp_BitwiseXor: return cudf::ast::ast_operator::BITWISE_XOR;
    default:
      throw std::runtime_error("unsupported BinaryOp: " + std::to_string(op));
  }
}

// ============================================================================
// AST expression builder
// ============================================================================

static cudf::ast::expression& build_expr(const fb::Expr* expr, ExprContext& ctx) {
  if (!expr || !expr->node())
    throw std::runtime_error("null expression");

  switch (expr->node_type()) {
    case fb::ExprNode_ColumnRef: {
      auto* col = expr->node_as_ColumnRef();
      return ctx.keep(std::make_unique<cudf::ast::column_reference>(
          static_cast<cudf::size_type>(col->index())));
    }

    case fb::ExprNode_LiteralExpr: {
      auto* lit = expr->node_as_LiteralExpr();
      auto* sv = lit->value();
      if (!sv) throw std::runtime_error("LiteralExpr has no value");

      switch (sv->type()) {
        case fb::DataType_Int8: {
          auto s = std::make_unique<cudf::numeric_scalar<int8_t>>(
              static_cast<int8_t>(sv->int_val()), true);
          auto& ref = *s;
          ctx.scalars.push_back(std::move(s));
          return ctx.keep(std::make_unique<cudf::ast::literal>(ref));
        }
        case fb::DataType_Int16: {
          auto s = std::make_unique<cudf::numeric_scalar<int16_t>>(
              static_cast<int16_t>(sv->int_val()), true);
          auto& ref = *s;
          ctx.scalars.push_back(std::move(s));
          return ctx.keep(std::make_unique<cudf::ast::literal>(ref));
        }
        case fb::DataType_Int32: {
          auto s = std::make_unique<cudf::numeric_scalar<int32_t>>(
              static_cast<int32_t>(sv->int_val()), true);
          auto& ref = *s;
          ctx.scalars.push_back(std::move(s));
          return ctx.keep(std::make_unique<cudf::ast::literal>(ref));
        }
        case fb::DataType_Int64: {
          auto s = std::make_unique<cudf::numeric_scalar<int64_t>>(
              sv->int_val(), true);
          auto& ref = *s;
          ctx.scalars.push_back(std::move(s));
          return ctx.keep(std::make_unique<cudf::ast::literal>(ref));
        }
        case fb::DataType_Float32: {
          auto s = std::make_unique<cudf::numeric_scalar<float>>(
              static_cast<float>(sv->float_val()), true);
          auto& ref = *s;
          ctx.scalars.push_back(std::move(s));
          return ctx.keep(std::make_unique<cudf::ast::literal>(ref));
        }
        case fb::DataType_Float64: {
          auto s = std::make_unique<cudf::numeric_scalar<double>>(
              sv->float_val(), true);
          auto& ref = *s;
          ctx.scalars.push_back(std::move(s));
          return ctx.keep(std::make_unique<cudf::ast::literal>(ref));
        }
        case fb::DataType_Decimal128: {
          // cuDF AST does not directly support Decimal128 literals.
          // Promote to float64 for comparison.
          __int128 val = (static_cast<__int128>(sv->decimal_hi()) << 64) |
                         static_cast<unsigned __int128>(sv->decimal_lo());
          int8_t scale = sv->decimal_scale();
          double dval = static_cast<double>(val);
          for (int8_t i = 0; i < scale; ++i) dval /= 10.0;
          for (int8_t i = 0; i > scale; --i) dval *= 10.0;
          auto s = std::make_unique<cudf::numeric_scalar<double>>(dval, true);
          auto& ref = *s;
          ctx.scalars.push_back(std::move(s));
          return ctx.keep(std::make_unique<cudf::ast::literal>(ref));
        }
        default:
          throw std::runtime_error(
              "unsupported literal type: " + std::to_string(sv->type()));
      }
    }

    case fb::ExprNode_BinaryExprNode: {
      auto* bin = expr->node_as_BinaryExprNode();
      auto& left = build_expr(bin->left(), ctx);
      auto& right = build_expr(bin->right(), ctx);
      auto op = fb_to_ast_op(bin->op());
      return ctx.keep(std::make_unique<cudf::ast::operation>(op, left, right));
    }

    case fb::ExprNode_UnaryExprNode: {
      auto* un = expr->node_as_UnaryExprNode();
      auto& arg = build_expr(un->arg(), ctx);
      switch (un->op()) {
        case fb::UnaryOp_Not:
          return ctx.keep(std::make_unique<cudf::ast::operation>(
              cudf::ast::ast_operator::NOT, arg));
        case fb::UnaryOp_IsNull:
          return ctx.keep(std::make_unique<cudf::ast::operation>(
              cudf::ast::ast_operator::IS_NULL, arg));
        case fb::UnaryOp_Negative: {
          // -x = 0 - x
          auto zero = std::make_unique<cudf::numeric_scalar<int64_t>>(0, true);
          auto& zref = *zero;
          ctx.scalars.push_back(std::move(zero));
          auto& lit = ctx.keep(std::make_unique<cudf::ast::literal>(zref));
          return ctx.keep(std::make_unique<cudf::ast::operation>(
              cudf::ast::ast_operator::SUB, lit, arg));
        }
        default:
          throw std::runtime_error(
              "unsupported UnaryOp: " + std::to_string(un->op()));
      }
    }

    case fb::ExprNode_CastExprNode: {
      auto* cast = expr->node_as_CastExprNode();
      auto& inner = build_expr(cast->expr(), ctx);
      auto target = fb_to_type_id(cast->target_type());
      cudf::ast::ast_operator cast_op;
      switch (target) {
        case cudf::type_id::INT64:  cast_op = cudf::ast::ast_operator::CAST_TO_INT64; break;
        case cudf::type_id::FLOAT64: cast_op = cudf::ast::ast_operator::CAST_TO_FLOAT64; break;
        default:
          throw std::runtime_error("unsupported CAST target type");
      }
      return ctx.keep(std::make_unique<cudf::ast::operation>(cast_op, inner));
    }

    default:
      throw std::runtime_error(
          "unsupported expression node type: " + std::to_string(expr->node_type()));
  }
}

// ============================================================================
// GpuScan — read Parquet files
// ============================================================================

static TableResult execute_scan(const fb::GpuScan* scan) {
  if (!scan->file_paths() || scan->file_paths()->size() == 0)
    throw std::runtime_error("GpuScan: no file paths");

  // Collect file paths.
  std::vector<std::string> paths;
  for (auto* p : *scan->file_paths()) {
    paths.push_back(p->str());
  }

  // Build column name list from file_schema + projection.
  std::vector<std::string> all_names;
  if (scan->file_schema() && scan->file_schema()->fields()) {
    for (auto* f : *scan->file_schema()->fields()) {
      all_names.push_back(f->name()->str());
    }
  }

  std::vector<std::string> projected_names;
  if (scan->projection() && scan->projection()->size() > 0) {
    for (auto idx : *scan->projection()) {
      if (idx < all_names.size()) {
        projected_names.push_back(all_names[idx]);
      }
    }
  } else {
    projected_names = all_names;
  }

  auto opts = cudf::io::parquet_reader_options::builder(
                  cudf::io::source_info{paths})
                  .columns(projected_names)
                  .build();

  if (scan->limit() > 0) {
    opts.set_num_rows(static_cast<cudf::size_type>(scan->limit()));
  }

  auto result = cudf::io::read_parquet(opts);

  // Use column names from the reader metadata.
  std::vector<std::string> col_names;
  for (auto& ci : result.metadata.schema_info) {
    col_names.push_back(ci.name);
  }

  return {std::move(result.tbl), std::move(col_names)};
}

// ============================================================================
// GpuFilter — apply boolean predicate
// ============================================================================

static TableResult execute_filter(const fb::GpuFilter* filter) {
  auto input = execute_node(filter->input());

  ExprContext ctx;
  auto& predicate = build_expr(filter->predicate(), ctx);
  auto mask = cudf::compute_column(input.table->view(), predicate);
  auto filtered = cudf::apply_boolean_mask(input.table->view(), mask->view());

  return {std::move(filtered), std::move(input.column_names)};
}

// ============================================================================
// GpuProject — column selection / renaming
// ============================================================================

static TableResult execute_project(const fb::GpuProject* proj) {
  auto input = execute_node(proj->input());

  if (!proj->exprs() || proj->exprs()->size() == 0)
    throw std::runtime_error("GpuProject: no expressions");

  auto tv = input.table->view();
  std::vector<std::unique_ptr<cudf::column>> columns;
  std::vector<std::string> names;

  for (flatbuffers::uoffset_t i = 0; i < proj->exprs()->size(); ++i) {
    auto* expr = proj->exprs()->Get(i);

    // Fast path: simple column reference → just copy the column view.
    if (expr->node_type() == fb::ExprNode_ColumnRef) {
      auto* col = expr->node_as_ColumnRef();
      auto idx = static_cast<cudf::size_type>(col->index());
      columns.push_back(std::make_unique<cudf::column>(tv.column(idx)));
    } else {
      // General expression: evaluate via AST.
      ExprContext ctx;
      auto& ast = build_expr(expr, ctx);
      columns.push_back(cudf::compute_column(tv, ast));
    }

    if (proj->aliases() && i < proj->aliases()->size()) {
      names.push_back(proj->aliases()->Get(i)->str());
    } else {
      names.push_back("col" + std::to_string(i));
    }
  }

  auto result = std::make_unique<cudf::table>(std::move(columns));
  return {std::move(result), std::move(names)};
}

// ============================================================================
// GpuAggregate — group-by aggregation
// ============================================================================

static std::unique_ptr<cudf::groupby_aggregation> make_agg(
    const std::string& func_name, bool is_final) {
  // In Final mode, count→sum (sum partial counts), others stay the same.
  if (func_name == "count" || func_name == "COUNT") {
    if (is_final)
      return cudf::make_sum_aggregation<cudf::groupby_aggregation>();
    else
      return cudf::make_count_aggregation<cudf::groupby_aggregation>();
  }
  if (func_name == "sum" || func_name == "SUM")
    return cudf::make_sum_aggregation<cudf::groupby_aggregation>();
  if (func_name == "min" || func_name == "MIN")
    return cudf::make_min_aggregation<cudf::groupby_aggregation>();
  if (func_name == "max" || func_name == "MAX")
    return cudf::make_max_aggregation<cudf::groupby_aggregation>();
  throw std::runtime_error("unsupported aggregate function: " + func_name);
}

static TableResult execute_aggregate(const fb::GpuAggregate* agg) {
  auto input = execute_node(agg->input());
  auto tv = input.table->view();

  bool is_final = (agg->mode() == fb::AggregateMode_Final ||
                   agg->mode() == fb::AggregateMode_FinalPartitioned);

  // Build group-by keys.
  std::vector<cudf::size_type> key_indices;
  std::vector<std::string> key_names;
  if (agg->group_exprs()) {
    for (flatbuffers::uoffset_t i = 0; i < agg->group_exprs()->size(); ++i) {
      auto* expr = agg->group_exprs()->Get(i);
      if (expr->node_type() != fb::ExprNode_ColumnRef)
        throw std::runtime_error("GpuAggregate: only ColumnRef group exprs supported");
      auto* col = expr->node_as_ColumnRef();
      key_indices.push_back(static_cast<cudf::size_type>(col->index()));
      if (agg->group_names() && i < agg->group_names()->size())
        key_names.push_back(agg->group_names()->Get(i)->str());
      else
        key_names.push_back(input.column_names[col->index()]);
    }
  }

  // Build key table.
  std::vector<cudf::column_view> key_cols;
  for (auto idx : key_indices) key_cols.push_back(tv.column(idx));
  cudf::table_view keys_view{key_cols};

  cudf::groupby::groupby gb{keys_view};

  // Build aggregation requests — one per aggregate function.
  std::vector<cudf::groupby::aggregation_request> requests;
  std::vector<std::string> agg_names;
  if (agg->aggr_funcs()) {
    for (flatbuffers::uoffset_t i = 0; i < agg->aggr_funcs()->size(); ++i) {
      auto* func = agg->aggr_funcs()->Get(i);
      std::string name = func->name() ? func->name()->str() : "count";

      // Determine the values column.
      cudf::column_view values_col;
      if (func->args() && func->args()->size() > 0) {
        auto* arg = func->args()->Get(0);
        if (arg->node_type() == fb::ExprNode_ColumnRef) {
          auto idx = arg->node_as_ColumnRef()->index();
          values_col = tv.column(static_cast<cudf::size_type>(idx));
        } else {
          // For count(*), use the first column as a dummy.
          values_col = tv.column(0);
        }
      } else {
        values_col = tv.column(0);
      }

      cudf::groupby::aggregation_request req;
      req.values = values_col;
      req.aggregations.push_back(make_agg(name, is_final));
      requests.push_back(std::move(req));

      if (func->alias())
        agg_names.push_back(func->alias()->str());
      else
        agg_names.push_back(name);
    }
  }

  auto [group_keys, agg_results] = gb.aggregate(requests);

  // Assemble output: key columns + aggregation result columns.
  std::vector<std::unique_ptr<cudf::column>> out_cols;
  std::vector<std::string> out_names;

  for (cudf::size_type i = 0; i < group_keys->num_columns(); ++i) {
    out_cols.push_back(std::make_unique<cudf::column>(group_keys->view().column(i)));
    out_names.push_back(key_names[i]);
  }
  for (size_t i = 0; i < agg_results.size(); ++i) {
    // Each aggregation_result has one column per aggregation; we have one each.
    out_cols.push_back(std::move(agg_results[i].results[0]));
    out_names.push_back(agg_names[i]);
  }

  return {std::make_unique<cudf::table>(std::move(out_cols)), std::move(out_names)};
}

// ============================================================================
// GpuHashJoin — equi-join
// ============================================================================

static TableResult execute_hash_join(const fb::GpuHashJoin* join) {
  auto left = execute_node(join->left());
  auto right = execute_node(join->right());

  auto ltv = left.table->view();
  auto rtv = right.table->view();

  // Build key tables.
  std::vector<cudf::column_view> left_key_cols, right_key_cols;
  if (join->keys()) {
    for (flatbuffers::uoffset_t i = 0; i < join->keys()->size(); ++i) {
      auto* key = join->keys()->Get(i);
      auto* lk = key->left();
      auto* rk = key->right();
      if (!lk || !rk || lk->node_type() != fb::ExprNode_ColumnRef ||
          rk->node_type() != fb::ExprNode_ColumnRef)
        throw std::runtime_error("GpuHashJoin: only ColumnRef keys supported");
      left_key_cols.push_back(
          ltv.column(static_cast<cudf::size_type>(lk->node_as_ColumnRef()->index())));
      right_key_cols.push_back(
          rtv.column(static_cast<cudf::size_type>(rk->node_as_ColumnRef()->index())));
    }
  }

  cudf::table_view left_keys{left_key_cols};
  cudf::table_view right_keys{right_key_cols};

  // Execute join — returns index pairs.
  auto [left_indices, right_indices] = [&]() {
    switch (join->join_type()) {
      case fb::JoinType_Inner:
        return cudf::inner_join(left_keys, right_keys);
      case fb::JoinType_Left:
        return cudf::left_join(left_keys, right_keys);
      case fb::JoinType_Full:
        return cudf::full_join(left_keys, right_keys);
      default:
        throw std::runtime_error(
            "unsupported join type: " + std::to_string(join->join_type()));
    }
  }();

  // Gather rows from both sides.
  auto n = static_cast<cudf::size_type>(left_indices->size());
  cudf::column_view left_idx_col{cudf::data_type{cudf::type_id::INT32},
                                  n, left_indices->data(),
                                  nullptr, 0, 0, {}};
  cudf::column_view right_idx_col{cudf::data_type{cudf::type_id::INT32},
                                   n, right_indices->data(),
                                   nullptr, 0, 0, {}};
  auto left_gathered = cudf::gather(ltv, left_idx_col);
  auto right_gathered = cudf::gather(rtv, right_idx_col);

  // Concatenate columns: [left_cols..., right_cols...].
  std::vector<std::unique_ptr<cudf::column>> all_cols;
  std::vector<std::string> all_names;

  auto lgv = left_gathered->view();
  for (cudf::size_type i = 0; i < lgv.num_columns(); ++i) {
    all_cols.push_back(std::make_unique<cudf::column>(lgv.column(i)));
    all_names.push_back(left.column_names[i]);
  }
  auto rgv = right_gathered->view();
  for (cudf::size_type i = 0; i < rgv.num_columns(); ++i) {
    all_cols.push_back(std::make_unique<cudf::column>(rgv.column(i)));
    all_names.push_back(right.column_names[i]);
  }

  auto full_table = std::make_unique<cudf::table>(std::move(all_cols));

  // Apply output projection if present.
  if (join->projection() && join->projection()->size() > 0) {
    auto ftv = full_table->view();
    std::vector<std::unique_ptr<cudf::column>> proj_cols;
    std::vector<std::string> proj_names;
    for (auto idx : *join->projection()) {
      proj_cols.push_back(std::make_unique<cudf::column>(ftv.column(idx)));
      proj_names.push_back(all_names[idx]);
    }
    return {std::make_unique<cudf::table>(std::move(proj_cols)),
            std::move(proj_names)};
  }

  return {std::move(full_table), std::move(all_names)};
}

// ============================================================================
// GpuSort — sort by expressions
// ============================================================================

static TableResult execute_sort(const fb::GpuSort* sort) {
  auto input = execute_node(sort->input());
  auto tv = input.table->view();

  if (!sort->exprs() || sort->exprs()->size() == 0)
    return std::move(input);

  // Build the key table and sort orders.
  std::vector<cudf::column_view> key_cols;
  std::vector<cudf::order> orders;
  std::vector<cudf::null_order> null_orders;

  for (flatbuffers::uoffset_t i = 0; i < sort->exprs()->size(); ++i) {
    auto* se = sort->exprs()->Get(i);
    auto* expr = se->expr();
    if (!expr || expr->node_type() != fb::ExprNode_ColumnRef)
      throw std::runtime_error("GpuSort: only ColumnRef sort keys supported");
    auto idx = static_cast<cudf::size_type>(expr->node_as_ColumnRef()->index());
    key_cols.push_back(tv.column(idx));
    orders.push_back(se->asc() ? cudf::order::ASCENDING : cudf::order::DESCENDING);
    null_orders.push_back(se->nulls_first() ? cudf::null_order::BEFORE
                                            : cudf::null_order::AFTER);
  }

  cudf::table_view keys{key_cols};
  auto sorted_indices = cudf::sorted_order(keys, orders, null_orders);
  auto result = cudf::gather(tv, sorted_indices->view());

  // Apply fetch (LIMIT).
  if (sort->fetch() > 0) {
    auto n = std::min(static_cast<cudf::size_type>(sort->fetch()),
                      result->view().num_rows());
    std::vector<cudf::size_type> slice_indices{0, n};
    auto sliced = cudf::slice(result->view(), slice_indices);
    result = std::make_unique<cudf::table>(sliced[0]);
  }

  return {std::move(result), std::move(input.column_names)};
}

// ============================================================================
// Pass-through nodes (single-GPU: just execute input)
// ============================================================================

static TableResult execute_passthrough(const fb::PlanNode* input_node) {
  return execute_node(input_node);
}

// ============================================================================
// Plan node dispatcher
// ============================================================================

static TableResult execute_node(const fb::PlanNode* node) {
  if (!node) throw std::runtime_error("null PlanNode");

  switch (node->node_type()) {
    case fb::PlanNodeKind_GpuScan:
      return execute_scan(node->node_as_GpuScan());

    case fb::PlanNodeKind_GpuFilter:
      return execute_filter(node->node_as_GpuFilter());

    case fb::PlanNodeKind_GpuProject:
      return execute_project(node->node_as_GpuProject());

    case fb::PlanNodeKind_GpuAggregate:
      return execute_aggregate(node->node_as_GpuAggregate());

    case fb::PlanNodeKind_GpuHashJoin:
      return execute_hash_join(node->node_as_GpuHashJoin());

    case fb::PlanNodeKind_GpuSort:
      return execute_sort(node->node_as_GpuSort());

    // Pass-through nodes — on a single GPU, just forward to input.
    case fb::PlanNodeKind_GpuCoalesceBatches:
      return execute_passthrough(node->node_as_GpuCoalesceBatches()->input());

    case fb::PlanNodeKind_GpuCoalescePartitions:
      return execute_passthrough(node->node_as_GpuCoalescePartitions()->input());

    case fb::PlanNodeKind_GpuRepartition:
      return execute_passthrough(node->node_as_GpuRepartition()->input());

    case fb::PlanNodeKind_GpuSortPreservingMerge:
      // On single GPU with single partition, just forward to input.
      // Sort order is already established by the child GpuSort.
      return execute_passthrough(node->node_as_GpuSortPreservingMerge()->input());

    default:
      throw std::runtime_error(
          "unsupported PlanNodeKind: " + std::to_string(node->node_type()));
  }
}

// ============================================================================
// Public API
// ============================================================================

TableResult execute_plan(const uint8_t* plan_bytes, uint64_t plan_len) {
  auto* gpu_plan = fb::GetGpuPlan(plan_bytes);
  if (!gpu_plan)
    throw std::runtime_error("failed to parse FlatBuffer GpuPlan");

  flatbuffers::Verifier verifier(plan_bytes, plan_len);
  if (!gpu_plan->Verify(verifier))
    throw std::runtime_error("FlatBuffer verification failed");

  auto* root = gpu_plan->root();
  if (!root)
    throw std::runtime_error("GpuPlan has no root node");

  return execute_node(root);
}

}  // namespace peacock
