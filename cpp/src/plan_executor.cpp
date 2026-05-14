#include "plan_executor.h"
#include "generated/gpu_plan_generated.h"

#include <cudf/ast/expressions.hpp>
#include <cudf/binaryop.hpp>
#include <cudf/column/column_view.hpp>
#include <cudf/copying.hpp>
#include <cudf/column/column_factories.hpp>
#include <cudf/datetime.hpp>
#include <cudf/filling.hpp>
#include <cudf/groupby.hpp>
#include <cudf/io/parquet.hpp>
#if __has_include(<cudf/join/join.hpp>)
#include <cudf/join/join.hpp>
#else
#include <cudf/join.hpp>
#endif
#if __has_include(<cudf/join/filtered_join.hpp>)
#include <cudf/join/filtered_join.hpp>
#define PEACOCK_HAVE_FILTERED_JOIN 1
#endif
#include <cudf/reduction.hpp>
#include <cudf/scalar/scalar.hpp>
#include <cudf/strings/contains.hpp>
#include <cudf/strings/slice.hpp>
#include <cudf/unary.hpp>
#include <cudf/scalar/scalar_factories.hpp>
#include <cudf/sorting.hpp>
#include <cudf/stream_compaction.hpp>
#include <cudf/table/table.hpp>
#include <cudf/table/table_view.hpp>
#include <cudf/transform.hpp>
#include <cudf/types.hpp>
#include <cudf/wrappers/durations.hpp>
#include <cudf/wrappers/timestamps.hpp>

#include <cctype>
#include <cstdio>
#include <cstdlib>
#include <stdexcept>

#include <cuda_runtime.h>
#include <rmm/cuda_stream_view.hpp>

namespace peacock {
namespace fb = peacock::plan;

// ---------------------------------------------------------------------------
// Debug instrumentation (PEACOCK_GPU_DEBUG=1 to enable)
// ---------------------------------------------------------------------------
// Prints each plan node + expression as it executes and synchronizes the
// default cuDF stream after each step so async CUDA errors surface at the
// call site instead of cascading several ops later.

static bool debug_enabled() {
  static const bool e = []() {
    const char* v = std::getenv("PEACOCK_GPU_DEBUG");
    return v && v[0] && v[0] != '0';
  }();
  return e;
}

#define PCK_TRACE(...) do {                                  \
    if (debug_enabled()) {                                   \
      std::fprintf(stderr, "[peacock] " __VA_ARGS__);        \
      std::fprintf(stderr, "\n");                            \
    }                                                        \
  } while (0)

// Synchronize the default stream and check for errors. When debug is on,
// we always sync (to localize errors); when off, this is a no-op.
static void debug_sync(const char* tag) {
  if (!debug_enabled()) return;
  auto err = cudaStreamSynchronize(cudf::get_default_stream().value());
  if (err != cudaSuccess) {
    std::fprintf(stderr, "[peacock] CUDA sync after %s: %s\n",
                 tag, cudaGetErrorString(err));
    throw std::runtime_error(std::string("CUDA error after ") + tag +
                             ": " + cudaGetErrorString(err));
  }
}

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
        case fb::DataType_Utf8:
        case fb::DataType_LargeUtf8:
        case fb::DataType_Utf8View: {
          // cuDF AST literals accept string_scalar; cuDF doesn't distinguish
          // owned vs. view strings on the device side, so all three flavors
          // map to the same scalar type.
          auto s = std::make_unique<cudf::string_scalar>(
              std::string(sv->string_val() ? sv->string_val()->str() : ""), true);
          auto& ref = *s;
          ctx.scalars.push_back(std::move(s));
          return ctx.keep(std::make_unique<cudf::ast::literal>(ref));
        }
        case fb::DataType_Date32: {
          // Date32 = days since UNIX epoch (int32).
          auto s = std::make_unique<cudf::timestamp_scalar<cudf::timestamp_D>>(
              cudf::duration_D{static_cast<int32_t>(sv->int_val())}, true);
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
// Column-producing expression evaluator
// ============================================================================
//
// cuDF AST has no operators for LIKE, substr, date_part (extract), or CASE
// WHEN. Expressions that contain any of these nodes are evaluated outside the
// AST: each subexpression produces a `cudf::column`, which we combine via
// cudf row-wise APIs (binary_operation, copy_if_else, strings::like, ...).
//
// AST-able subtrees still go through `compute_column` for fusion; the
// column-path is a recursive fallback that calls into the AST evaluator
// whenever it encounters a fully AST-able subexpression.

// String/binary literal types whose AST evaluation isn't supported by cuDF
// (compute_column allocates fixed-width output, so a string compare aborts).
static bool is_string_like_literal(const fb::Expr* expr) {
  if (expr->node_type() != fb::ExprNode_LiteralExpr) return false;
  auto* sv = expr->node_as_LiteralExpr()->value();
  if (!sv) return false;
  switch (sv->type()) {
    case fb::DataType_Utf8:
    case fb::DataType_LargeUtf8:
    case fb::DataType_Utf8View:
    case fb::DataType_Binary:
    case fb::DataType_LargeBinary:
    case fb::DataType_BinaryView:
      return true;
    default:
      return false;
  }
}

static bool is_ast_able(const fb::Expr* expr) {
  switch (expr->node_type()) {
    case fb::ExprNode_LikeExprNode:
    case fb::ExprNode_CaseExprNode:
    case fb::ExprNode_ScalarFunctionExprNode:
      return false;
    case fb::ExprNode_BinaryExprNode: {
      auto* b = expr->node_as_BinaryExprNode();
      // cuDF AST has no string ops; string literal on either side forces the
      // column path (cudf::binary_operation, which does support strings).
      if (is_string_like_literal(b->left()) || is_string_like_literal(b->right()))
        return false;
      return is_ast_able(b->left()) && is_ast_able(b->right());
    }
    case fb::ExprNode_UnaryExprNode:
      return is_ast_able(expr->node_as_UnaryExprNode()->arg());
    case fb::ExprNode_CastExprNode:
      return is_ast_able(expr->node_as_CastExprNode()->expr());
    default:
      return true;
  }
}

static std::unique_ptr<cudf::scalar> build_scalar(const fb::ScalarValue* sv) {
  switch (sv->type()) {
    case fb::DataType_Boolean:
      return std::make_unique<cudf::numeric_scalar<bool>>(sv->bool_val(), true);
    case fb::DataType_Int8:
      return std::make_unique<cudf::numeric_scalar<int8_t>>(
          static_cast<int8_t>(sv->int_val()), true);
    case fb::DataType_Int16:
      return std::make_unique<cudf::numeric_scalar<int16_t>>(
          static_cast<int16_t>(sv->int_val()), true);
    case fb::DataType_Int32:
      return std::make_unique<cudf::numeric_scalar<int32_t>>(
          static_cast<int32_t>(sv->int_val()), true);
    case fb::DataType_Int64:
      return std::make_unique<cudf::numeric_scalar<int64_t>>(sv->int_val(), true);
    case fb::DataType_Float32:
      return std::make_unique<cudf::numeric_scalar<float>>(
          static_cast<float>(sv->float_val()), true);
    case fb::DataType_Float64:
      return std::make_unique<cudf::numeric_scalar<double>>(sv->float_val(), true);
    case fb::DataType_Utf8:
    case fb::DataType_LargeUtf8:
    case fb::DataType_Utf8View:
      return std::make_unique<cudf::string_scalar>(
          std::string(sv->string_val() ? sv->string_val()->str() : ""), true);
    case fb::DataType_Date32:
      return std::make_unique<cudf::timestamp_scalar<cudf::timestamp_D>>(
          cudf::duration_D{static_cast<int32_t>(sv->int_val())}, true);
    default:
      throw std::runtime_error(
          "unsupported scalar type in column path: " + std::to_string(sv->type()));
  }
}

static cudf::binary_operator fb_to_binop(fb::BinaryOp op) {
  switch (op) {
    case fb::BinaryOp_Eq:    return cudf::binary_operator::EQUAL;
    case fb::BinaryOp_NotEq: return cudf::binary_operator::NOT_EQUAL;
    case fb::BinaryOp_Lt:    return cudf::binary_operator::LESS;
    case fb::BinaryOp_LtEq:  return cudf::binary_operator::LESS_EQUAL;
    case fb::BinaryOp_Gt:    return cudf::binary_operator::GREATER;
    case fb::BinaryOp_GtEq:  return cudf::binary_operator::GREATER_EQUAL;
    case fb::BinaryOp_Plus:  return cudf::binary_operator::ADD;
    case fb::BinaryOp_Minus: return cudf::binary_operator::SUB;
    case fb::BinaryOp_Multiply: return cudf::binary_operator::MUL;
    case fb::BinaryOp_Divide:   return cudf::binary_operator::DIV;
    case fb::BinaryOp_Modulo:   return cudf::binary_operator::MOD;
    case fb::BinaryOp_And:      return cudf::binary_operator::LOGICAL_AND;
    case fb::BinaryOp_Or:       return cudf::binary_operator::LOGICAL_OR;
    case fb::BinaryOp_BitwiseAnd: return cudf::binary_operator::BITWISE_AND;
    case fb::BinaryOp_BitwiseOr:  return cudf::binary_operator::BITWISE_OR;
    case fb::BinaryOp_BitwiseXor: return cudf::binary_operator::BITWISE_XOR;
    default:
      throw std::runtime_error(
          "unsupported BinaryOp in column path: " + std::to_string(op));
  }
}

// Forward declaration.
static std::unique_ptr<cudf::column> build_column(
    const fb::Expr* expr, cudf::table_view const& table);

// Evaluate an AST-able subtree by routing it through cudf::compute_column.
static std::unique_ptr<cudf::column> eval_ast_subtree(
    const fb::Expr* expr, cudf::table_view const& table) {
  ExprContext ctx;
  auto& ast = build_expr(expr, ctx);
  return cudf::compute_column(table, ast);
}

// Returns true if the binary op produces a bool column (comparison/logical).
static bool is_predicate_op(fb::BinaryOp op) {
  switch (op) {
    case fb::BinaryOp_Eq:
    case fb::BinaryOp_NotEq:
    case fb::BinaryOp_Lt:
    case fb::BinaryOp_LtEq:
    case fb::BinaryOp_Gt:
    case fb::BinaryOp_GtEq:
    case fb::BinaryOp_And:
    case fb::BinaryOp_Or:
      return true;
    default:
      return false;
  }
}

// Pick an output type for binary_operation. Boolean for predicates; otherwise
// promote to the wider of the two input types (cuDF's binary_operation does
// the actual coercion under the hood, but it needs us to declare an output).
static cudf::data_type binop_output_type(
    fb::BinaryOp op, cudf::data_type lhs, cudf::data_type rhs) {
  if (is_predicate_op(op)) return cudf::data_type{cudf::type_id::BOOL8};
  // Fall back to lhs type — adequate for the queries we care about
  // (arithmetic in projections is rare in this code path; the heavy
  // arithmetic still goes through AST).
  (void)rhs;
  return lhs;
}

static std::unique_ptr<cudf::column> build_column_binary(
    const fb::BinaryExprNode* bin, cudf::table_view const& table) {
  auto* lhs = bin->left();
  auto* rhs = bin->right();
  auto op = fb_to_binop(bin->op());

  // Column-scalar fast path when one side is a literal.
  if (rhs->node_type() == fb::ExprNode_LiteralExpr &&
      lhs->node_type() != fb::ExprNode_LiteralExpr) {
    auto lcol = build_column(lhs, table);
    auto rsv = rhs->node_as_LiteralExpr()->value();
    auto rscalar = build_scalar(rsv);
    auto out = binop_output_type(bin->op(), lcol->type(), rscalar->type());
    return cudf::binary_operation(lcol->view(), *rscalar, op, out);
  }
  if (lhs->node_type() == fb::ExprNode_LiteralExpr &&
      rhs->node_type() != fb::ExprNode_LiteralExpr) {
    auto rcol = build_column(rhs, table);
    auto lsv = lhs->node_as_LiteralExpr()->value();
    auto lscalar = build_scalar(lsv);
    auto out = binop_output_type(bin->op(), lscalar->type(), rcol->type());
    return cudf::binary_operation(*lscalar, rcol->view(), op, out);
  }

  // Both sides materialise to columns.
  auto lcol = build_column(lhs, table);
  auto rcol = build_column(rhs, table);
  auto out = binop_output_type(bin->op(), lcol->type(), rcol->type());
  return cudf::binary_operation(lcol->view(), rcol->view(), op, out);
}

static std::unique_ptr<cudf::column> build_column_scalar_fn(
    const fb::ScalarFunctionExprNode* sf, cudf::table_view const& table) {
  auto name = sf->name() ? sf->name()->str() : std::string{};
  auto* args = sf->args();
  if (!args || args->size() == 0)
    throw std::runtime_error("ScalarFunction " + name + ": no args");

  // date_part(field, ts) — DataFusion encodes the field as a string literal.
  if (name == "date_part") {
    if (args->size() != 2)
      throw std::runtime_error("date_part expects 2 args");
    auto* field_expr = args->Get(0);
    if (field_expr->node_type() != fb::ExprNode_LiteralExpr)
      throw std::runtime_error("date_part: field must be a literal");
    auto* fsv = field_expr->node_as_LiteralExpr()->value();
    auto field = fsv && fsv->string_val() ? fsv->string_val()->str() : std::string{};
    for (auto& c : field) c = static_cast<char>(std::toupper(c));
    cudf::datetime::datetime_component comp;
    if      (field == "YEAR")    comp = cudf::datetime::datetime_component::YEAR;
    else if (field == "MONTH")   comp = cudf::datetime::datetime_component::MONTH;
    else if (field == "DAY")     comp = cudf::datetime::datetime_component::DAY;
    else if (field == "HOUR")    comp = cudf::datetime::datetime_component::HOUR;
    else if (field == "MINUTE")  comp = cudf::datetime::datetime_component::MINUTE;
    else if (field == "SECOND")  comp = cudf::datetime::datetime_component::SECOND;
    else throw std::runtime_error("date_part: unsupported field " + field);
    auto ts = build_column(args->Get(1), table);
    return cudf::datetime::extract_datetime_component(ts->view(), comp);
  }

  // substr(s, start, length) — SQL semantics: 1-based start.
  if (name == "substr" || name == "substring") {
    if (args->size() < 2 || args->size() > 3)
      throw std::runtime_error("substr expects 2 or 3 args");
    auto strcol = build_column(args->Get(0), table);

    auto lit_int = [&](const fb::Expr* e) -> int32_t {
      if (e->node_type() != fb::ExprNode_LiteralExpr)
        throw std::runtime_error("substr: position/length must be literals");
      auto* v = e->node_as_LiteralExpr()->value();
      return static_cast<int32_t>(v->int_val());
    };

    int32_t start_1 = lit_int(args->Get(1));            // 1-based
    int32_t start = start_1 > 0 ? start_1 - 1 : start_1;  // → 0-based
    cudf::numeric_scalar<cudf::size_type> start_s(start, true);

    if (args->size() == 3) {
      int32_t len = lit_int(args->Get(2));
      int32_t stop = start + len;
      cudf::numeric_scalar<cudf::size_type> stop_s(stop, true);
      cudf::numeric_scalar<cudf::size_type> step_s(1, true);
      return cudf::strings::slice_strings(
          cudf::strings_column_view{strcol->view()}, start_s, stop_s, step_s);
    }
    // No length → slice through end.
    cudf::numeric_scalar<cudf::size_type> stop_s(0, false);  // null = "to end"
    cudf::numeric_scalar<cudf::size_type> step_s(1, true);
    return cudf::strings::slice_strings(
        cudf::strings_column_view{strcol->view()}, start_s, stop_s, step_s);
  }

  throw std::runtime_error("unsupported scalar function in column path: " + name);
}

static std::unique_ptr<cudf::column> build_column_case(
    const fb::CaseExprNode* c, cudf::table_view const& table) {
  // Search-form CASE only (DataFusion always rewrites value-form).
  if (c->expr())
    throw std::runtime_error("value-form CASE not supported in column path");
  auto* whens = c->when_thens();
  if (!whens || whens->size() == 0)
    throw std::runtime_error("CASE has no WHEN/THEN pairs");

  // Build the ELSE column first (or null if none); fold from the last WHEN
  // backward so each step produces `if cond_i then then_i else accumulated`.
  std::unique_ptr<cudf::column> result;
  if (c->else_expr()) {
    result = build_column(c->else_expr(), table);
  } else {
    // Use the THEN type of the last branch as a reference for null fill.
    auto last_then = build_column(whens->Get(whens->size() - 1)->then(), table);
    auto null_scalar = cudf::make_default_constructed_scalar(last_then->type());
    result = cudf::make_column_from_scalar(*null_scalar, last_then->size());
  }

  for (cudf::size_type i = static_cast<cudf::size_type>(whens->size()) - 1; i >= 0; --i) {
    auto* wt = whens->Get(static_cast<flatbuffers::uoffset_t>(i));
    auto cond = build_column(wt->when(), table);
    auto then = build_column(wt->then(), table);
    result = cudf::copy_if_else(then->view(), result->view(), cond->view());
  }
  return result;
}

static const char* expr_kind_name(fb::ExprNode k) {
  switch (k) {
    case fb::ExprNode_ColumnRef:               return "ColumnRef";
    case fb::ExprNode_LiteralExpr:             return "Literal";
    case fb::ExprNode_BinaryExprNode:          return "Binary";
    case fb::ExprNode_UnaryExprNode:           return "Unary";
    case fb::ExprNode_CastExprNode:            return "Cast";
    case fb::ExprNode_LikeExprNode:            return "Like";
    case fb::ExprNode_CaseExprNode:            return "Case";
    case fb::ExprNode_ScalarFunctionExprNode:  return "ScalarFn";
    default:                                    return "?";
  }
}

static std::unique_ptr<cudf::column> build_column(
    const fb::Expr* expr, cudf::table_view const& table) {
  if (debug_enabled()) {
    PCK_TRACE("  build_column kind=%s rows=%d cols=%d",
              expr_kind_name(expr->node_type()),
              table.num_rows(), table.num_columns());
  }
  // Plain literal: broadcast scalar to the table's row count. cudf::ast
  // doesn't have a defined behaviour for literal-only expressions in
  // compute_column, so handle this case before the AST fast path.
  if (expr->node_type() == fb::ExprNode_LiteralExpr) {
    auto sc = build_scalar(expr->node_as_LiteralExpr()->value());
    auto out = cudf::make_column_from_scalar(*sc, table.num_rows());
    debug_sync("Literal->make_column_from_scalar");
    return out;
  }

  // Bare column reference: copy the column view directly. compute_column
  // would allocate fixed-width output and reject strings/lists/structs.
  if (expr->node_type() == fb::ExprNode_ColumnRef) {
    auto* c = expr->node_as_ColumnRef();
    auto idx = static_cast<cudf::size_type>(c->index());
    if (idx < 0 || idx >= table.num_columns()) {
      throw std::runtime_error(
          "ColumnRef index " + std::to_string(idx) +
          " out of range (cols=" + std::to_string(table.num_columns()) + ")");
    }
    auto cv = table.column(idx);
    if (debug_enabled()) {
      PCK_TRACE("  ColumnRef idx=%d type_id=%d size=%d null_count=%d",
                static_cast<int>(idx),
                static_cast<int>(cv.type().id()),
                static_cast<int>(cv.size()),
                static_cast<int>(cv.null_count()));
    }
    auto out = std::make_unique<cudf::column>(cv);
    debug_sync("ColumnRef->copy");
    return out;
  }

  // AST-able expressions go through cudf::compute_column for fusion.
  if (is_ast_able(expr)) {
    auto out = eval_ast_subtree(expr, table);
    debug_sync("AST->compute_column");
    return out;
  }

  switch (expr->node_type()) {
    case fb::ExprNode_BinaryExprNode:
      return build_column_binary(expr->node_as_BinaryExprNode(), table);

    case fb::ExprNode_UnaryExprNode: {
      auto* un = expr->node_as_UnaryExprNode();
      auto arg = build_column(un->arg(), table);
      switch (un->op()) {
        case fb::UnaryOp_Not:
          return cudf::unary_operation(arg->view(), cudf::unary_operator::NOT);
        case fb::UnaryOp_IsNull:
          return cudf::is_null(arg->view());
        case fb::UnaryOp_IsNotNull:
          return cudf::is_valid(arg->view());
        default:
          throw std::runtime_error(
              "UnaryOp not supported in column path: " + std::to_string(un->op()));
      }
    }

    case fb::ExprNode_LikeExprNode: {
      auto* l = expr->node_as_LikeExprNode();
      auto strcol = build_column(l->expr(), table);
      auto* psv = l->pattern() && l->pattern()->node_type() == fb::ExprNode_LiteralExpr
                      ? l->pattern()->node_as_LiteralExpr()->value()
                      : nullptr;
      if (!psv || !psv->string_val())
        throw std::runtime_error("LIKE pattern must be a string literal");
      cudf::string_scalar pattern(psv->string_val()->str(), true);
      auto mask = cudf::strings::like(
          cudf::strings_column_view{strcol->view()}, pattern);
      if (l->negated()) {
        return cudf::unary_operation(mask->view(), cudf::unary_operator::NOT);
      }
      return mask;
    }

    case fb::ExprNode_CaseExprNode:
      return build_column_case(expr->node_as_CaseExprNode(), table);

    case fb::ExprNode_ScalarFunctionExprNode:
      return build_column_scalar_fn(expr->node_as_ScalarFunctionExprNode(), table);

    case fb::ExprNode_CastExprNode: {
      auto* cast = expr->node_as_CastExprNode();
      auto inner = build_column(cast->expr(), table);
      cudf::data_type target{fb_to_type_id(cast->target_type())};
      return cudf::cast(inner->view(), target);
    }

    default:
      // Other nodes (Column, Literal) are AST-able and were handled above.
      throw std::runtime_error(
          "unexpected non-AST expression: " + std::to_string(expr->node_type()));
  }
}

// ============================================================================
// GpuScan — read Parquet files
// ============================================================================

static TableResult execute_scan(const fb::GpuScan* scan) {
  if (!scan->file_paths() || scan->file_paths()->size() == 0)
    throw std::runtime_error("GpuScan: no file paths");

  // Wire-format contract (see gpu_plan.fbs::GpuScan): every path must be
  // absolute. We reject anything else with a clear error rather than
  // resolving against an implicit root.
  std::vector<std::string> paths;
  paths.reserve(scan->file_paths()->size());
  for (auto* p : *scan->file_paths()) {
    auto s = p->str();
    if (s.empty() || s.front() != '/') {
      throw std::runtime_error(
          "GpuScan: file path must be absolute (got \"" + s + "\")");
    }
    paths.push_back(std::move(s));
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

  // AST fast path when the predicate has no LIKE / CASE / ScalarFunction nodes;
  // otherwise produce the bool mask via the column-producing evaluator.
  std::unique_ptr<cudf::column> mask;
  if (is_ast_able(filter->predicate())) {
    ExprContext ctx;
    auto& predicate = build_expr(filter->predicate(), ctx);
    mask = cudf::compute_column(input.table->view(), predicate);
  } else {
    mask = build_column(filter->predicate(), input.table->view());
  }
  auto filtered = cudf::apply_boolean_mask(input.table->view(), mask->view());

  // Optional projection (set when the planner fused a downstream
  // ProjectionExec into the filter). Without this, all input columns survive
  // and downstream column indices are wrong by exactly the number of dropped
  // columns.
  if (filter->projection() && filter->projection()->size() > 0) {
    auto fv = filtered->view();
    std::vector<std::unique_ptr<cudf::column>> proj_cols;
    std::vector<std::string> proj_names;
    proj_cols.reserve(filter->projection()->size());
    proj_names.reserve(filter->projection()->size());
    for (auto idx : *filter->projection()) {
      proj_cols.push_back(std::make_unique<cudf::column>(fv.column(idx)));
      proj_names.push_back(input.column_names[idx]);
    }
    return {std::make_unique<cudf::table>(std::move(proj_cols)),
            std::move(proj_names)};
  }

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
    } else if (is_ast_able(expr)) {
      // Pure AST expression: fuse via cudf::compute_column.
      ExprContext ctx;
      auto& ast = build_expr(expr, ctx);
      columns.push_back(cudf::compute_column(tv, ast));
    } else {
      // Contains LIKE / CASE / ScalarFunction — column-producing path.
      columns.push_back(build_column(expr, tv));
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

static std::unique_ptr<cudf::reduce_aggregation> make_reduce_agg(
    const std::string& func_name) {
  if (func_name == "count" || func_name == "COUNT")
    throw std::runtime_error("count handled inline — make_reduce_agg should not be called for count");
  if (func_name == "sum" || func_name == "SUM")
    return cudf::make_sum_aggregation<cudf::reduce_aggregation>();
  if (func_name == "min" || func_name == "MIN")
    return cudf::make_min_aggregation<cudf::reduce_aggregation>();
  if (func_name == "max" || func_name == "MAX")
    return cudf::make_max_aggregation<cudf::reduce_aggregation>();
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

  // Helper to determine the values column for a function node.
  auto get_values_col = [&](const fb::AggregateFuncNode* func) -> cudf::column_view {
    if (func->args() && func->args()->size() > 0) {
      auto* arg = func->args()->Get(0);
      if (arg->node_type() == fb::ExprNode_ColumnRef)
        return tv.column(static_cast<cudf::size_type>(arg->node_as_ColumnRef()->index()));
    }
    return tv.column(0);  // count(*) or no args: dummy first column
  };

  std::vector<std::unique_ptr<cudf::column>> out_cols;
  std::vector<std::string> out_names;

  if (key_cols.empty()) {
    // Global aggregation (no group-by): use cudf::reduce to produce one row.
    if (agg->aggr_funcs()) {
      for (flatbuffers::uoffset_t i = 0; i < agg->aggr_funcs()->size(); ++i) {
        auto* func = agg->aggr_funcs()->Get(i);
        std::string name = func->name() ? func->name()->str() : "count";

        cudf::column_view values_col = get_values_col(func);
        bool is_count = (name == "count" || name == "COUNT");

        std::unique_ptr<cudf::scalar> scalar_result;
        if (is_count) {
          // Avoid make_count_aggregation<reduce_aggregation> which is not
          // exported in all cudf versions. Count = size - null_count.
          int64_t cnt = static_cast<int64_t>(values_col.size()) -
                        static_cast<int64_t>(values_col.null_count());
          auto s = std::make_unique<cudf::numeric_scalar<int64_t>>(cnt, true);
          scalar_result = std::move(s);
        } else {
          scalar_result = cudf::reduce(values_col, *make_reduce_agg(name),
                                       values_col.type());
        }
        out_cols.push_back(cudf::make_column_from_scalar(*scalar_result, 1));

        if (func->alias())
          out_names.push_back(func->alias()->str());
        else
          out_names.push_back(name);
      }
    }
    return {std::make_unique<cudf::table>(std::move(out_cols)), std::move(out_names)};
  }

  cudf::table_view keys_view{key_cols};
  cudf::groupby::groupby gb{keys_view};

  // Build aggregation requests — one per aggregate function.
  std::vector<cudf::groupby::aggregation_request> requests;
  std::vector<std::string> agg_names;
  std::vector<bool> agg_is_count;
  if (agg->aggr_funcs()) {
    for (flatbuffers::uoffset_t i = 0; i < agg->aggr_funcs()->size(); ++i) {
      auto* func = agg->aggr_funcs()->Get(i);
      std::string name = func->name() ? func->name()->str() : "count";

      cudf::groupby::aggregation_request req;
      req.values = get_values_col(func);
      req.aggregations.push_back(make_agg(name, is_final));
      requests.push_back(std::move(req));

      if (func->alias())
        agg_names.push_back(func->alias()->str());
      else
        agg_names.push_back(name);
      agg_is_count.push_back(name == "count" || name == "COUNT");
    }
  }

  auto [group_keys, agg_results] = gb.aggregate(requests);

  // Assemble output: key columns + aggregation result columns.
  for (cudf::size_type i = 0; i < group_keys->num_columns(); ++i) {
    out_cols.push_back(std::make_unique<cudf::column>(group_keys->view().column(i)));
    out_names.push_back(key_names[i]);
  }
  for (size_t i = 0; i < agg_results.size(); ++i) {
    // Each aggregation_result has one column per aggregation; we have one each.
    auto col = std::move(agg_results[i].results[0]);
    // cuDF count returns INT32; cast to INT64 for SQL BIGINT compatibility.
    if (agg_is_count[i] && col->type().id() == cudf::type_id::INT32)
      col = cudf::cast(*col, cudf::data_type{cudf::type_id::INT64});
    out_cols.push_back(std::move(col));
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

  // Semi/anti joins emit only one side's columns and use a different cuDF API
  // (single index vector instead of a pair). Right{Semi,Anti} = Left{Semi,Anti}
  // with sides swapped, so we normalise to Left{Semi,Anti} and remember which
  // side to gather from.
  bool is_semi_or_anti = false;
  bool emit_left = true;  // false → emit right side instead
  std::unique_ptr<rmm::device_uvector<cudf::size_type>> single_indices;
  // cuDF replaced free `left_{semi,anti}_join` with `filtered_join` (build the
  // hash table from one side, then probe). For Left{Semi,Anti} the right side
  // is the filter; for Right{Semi,Anti} we swap.
  switch (join->join_type()) {
    case fb::JoinType_LeftSemi: {
#ifdef PEACOCK_HAVE_FILTERED_JOIN
      cudf::filtered_join fj(right_keys, cudf::null_equality::EQUAL,
                             cudf::set_as_build_table::RIGHT, 0.5);
      single_indices = fj.semi_join(left_keys);
#else
      single_indices = cudf::left_semi_join(left_keys, right_keys);
#endif
      is_semi_or_anti = true;
      emit_left = true;
      break;
    }
    case fb::JoinType_LeftAnti: {
#ifdef PEACOCK_HAVE_FILTERED_JOIN
      cudf::filtered_join fj(right_keys, cudf::null_equality::EQUAL,
                             cudf::set_as_build_table::RIGHT, 0.5);
      single_indices = fj.anti_join(left_keys);
#else
      single_indices = cudf::left_anti_join(left_keys, right_keys);
#endif
      is_semi_or_anti = true;
      emit_left = true;
      break;
    }
    case fb::JoinType_RightSemi: {
#ifdef PEACOCK_HAVE_FILTERED_JOIN
      cudf::filtered_join fj(left_keys, cudf::null_equality::EQUAL,
                             cudf::set_as_build_table::RIGHT, 0.5);
      single_indices = fj.semi_join(right_keys);
#else
      single_indices = cudf::left_semi_join(right_keys, left_keys);
#endif
      is_semi_or_anti = true;
      emit_left = false;
      break;
    }
    case fb::JoinType_RightAnti: {
#ifdef PEACOCK_HAVE_FILTERED_JOIN
      cudf::filtered_join fj(left_keys, cudf::null_equality::EQUAL,
                             cudf::set_as_build_table::RIGHT, 0.5);
      single_indices = fj.anti_join(right_keys);
#else
      single_indices = cudf::left_anti_join(right_keys, left_keys);
#endif
      is_semi_or_anti = true;
      emit_left = false;
      break;
    }
    default:
      break;
  }

  if (is_semi_or_anti) {
    auto& side_tv = emit_left ? ltv : rtv;
    auto& side_names = emit_left ? left.column_names : right.column_names;
    auto m = static_cast<cudf::size_type>(single_indices->size());
    cudf::column_view idx_col{cudf::data_type{cudf::type_id::INT32}, m,
                              single_indices->data(), nullptr, 0, 0, {}};
    auto gathered = cudf::gather(side_tv, idx_col);
    auto gtv = gathered->view();
    std::vector<std::unique_ptr<cudf::column>> cols;
    std::vector<std::string> names;
    for (cudf::size_type i = 0; i < gtv.num_columns(); ++i) {
      cols.push_back(std::make_unique<cudf::column>(gtv.column(i)));
      names.push_back(side_names[i]);
    }
    auto t = std::make_unique<cudf::table>(std::move(cols));
    if (join->projection() && join->projection()->size() > 0) {
      auto tv = t->view();
      std::vector<std::unique_ptr<cudf::column>> p_cols;
      std::vector<std::string> p_names;
      for (auto idx : *join->projection()) {
        p_cols.push_back(std::make_unique<cudf::column>(tv.column(idx)));
        p_names.push_back(names[idx]);
      }
      return {std::make_unique<cudf::table>(std::move(p_cols)),
              std::move(p_names)};
    }
    return {std::move(t), std::move(names)};
  }

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
  //
  // For LEFT/FULL outer joins, cuDF signals unmatched rows with
  // JoinNoneValue (INT32_MIN) in the corresponding index vector — gathering
  // those with the default DONT_CHECK policy reads out of bounds and faults
  // with cudaErrorIllegalAddress. NULLIFY converts sentinel indices to nulls.
  using cudf::out_of_bounds_policy;
  auto kind = join->join_type();
  auto right_policy = (kind == fb::JoinType_Left || kind == fb::JoinType_Full)
                          ? out_of_bounds_policy::NULLIFY
                          : out_of_bounds_policy::DONT_CHECK;
  auto left_policy = (kind == fb::JoinType_Full)
                         ? out_of_bounds_policy::NULLIFY
                         : out_of_bounds_policy::DONT_CHECK;

  auto n = static_cast<cudf::size_type>(left_indices->size());
  cudf::column_view left_idx_col{cudf::data_type{cudf::type_id::INT32},
                                  n, left_indices->data(),
                                  nullptr, 0, 0, {}};
  cudf::column_view right_idx_col{cudf::data_type{cudf::type_id::INT32},
                                   n, right_indices->data(),
                                   nullptr, 0, 0, {}};
  auto left_gathered = cudf::gather(ltv, left_idx_col, left_policy);
  auto right_gathered = cudf::gather(rtv, right_idx_col, right_policy);

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

static const char* plan_node_kind_name(fb::PlanNodeKind k) {
  switch (k) {
    case fb::PlanNodeKind_GpuScan:                return "GpuScan";
    case fb::PlanNodeKind_GpuFilter:              return "GpuFilter";
    case fb::PlanNodeKind_GpuProject:             return "GpuProject";
    case fb::PlanNodeKind_GpuAggregate:           return "GpuAggregate";
    case fb::PlanNodeKind_GpuHashJoin:            return "GpuHashJoin";
    case fb::PlanNodeKind_GpuSort:                return "GpuSort";
    case fb::PlanNodeKind_GpuCoalesceBatches:     return "GpuCoalesceBatches";
    case fb::PlanNodeKind_GpuCoalescePartitions:  return "GpuCoalescePartitions";
    case fb::PlanNodeKind_GpuRepartition:         return "GpuRepartition";
    case fb::PlanNodeKind_GpuSortPreservingMerge: return "GpuSortPreservingMerge";
    default:                                       return "Unknown";
  }
}

static TableResult execute_node(const fb::PlanNode* node) {
  if (!node) throw std::runtime_error("null PlanNode");

  const char* kind = plan_node_kind_name(node->node_type());
  PCK_TRACE("enter %s", kind);

  TableResult result;
  try {
    switch (node->node_type()) {
      case fb::PlanNodeKind_GpuScan:
        result = execute_scan(node->node_as_GpuScan()); break;
      case fb::PlanNodeKind_GpuFilter:
        result = execute_filter(node->node_as_GpuFilter()); break;
      case fb::PlanNodeKind_GpuProject:
        result = execute_project(node->node_as_GpuProject()); break;
      case fb::PlanNodeKind_GpuAggregate:
        result = execute_aggregate(node->node_as_GpuAggregate()); break;
      case fb::PlanNodeKind_GpuHashJoin:
        result = execute_hash_join(node->node_as_GpuHashJoin()); break;
      case fb::PlanNodeKind_GpuSort:
        result = execute_sort(node->node_as_GpuSort()); break;
      case fb::PlanNodeKind_GpuCoalesceBatches:
        result = execute_passthrough(node->node_as_GpuCoalesceBatches()->input()); break;
      case fb::PlanNodeKind_GpuCoalescePartitions:
        result = execute_passthrough(node->node_as_GpuCoalescePartitions()->input()); break;
      case fb::PlanNodeKind_GpuRepartition:
        result = execute_passthrough(node->node_as_GpuRepartition()->input()); break;
      case fb::PlanNodeKind_GpuSortPreservingMerge:
        result = execute_passthrough(node->node_as_GpuSortPreservingMerge()->input()); break;
      default:
        throw std::runtime_error(
            "unsupported PlanNodeKind: " + std::to_string(node->node_type()));
    }
  } catch (const std::exception& e) {
    std::string msg = e.what();
    if (msg.find("[in ") == std::string::npos) {
      throw std::runtime_error(std::string("[in ") + kind + "] " + msg);
    }
    throw;
  }

  debug_sync(kind);
  if (debug_enabled()) {
    auto tv = result.table->view();
    PCK_TRACE("leave %s rows=%d cols=%d", kind, tv.num_rows(), tv.num_columns());
  }
  return result;
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
