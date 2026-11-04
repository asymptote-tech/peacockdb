"""Tests for plan_census.py: Comet native-plan text, classified against peacockdb's vocabulary."""
import sys
from pathlib import Path

SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))

import plan_census  # noqa: E402

# One native block in protobuf text format: a final aggregate over a filter whose predicate
# holds an rlike (missing in peacockdb) over a Parquet scan with an is_not_null data filter.
BLOCK = """\
children {
  children {
    plan_id: 3
    native_scan {
      common {
        data_filters {
          is_not_null {
            child {
              bound {
                index: 1
              }
            }
          }
        }
      }
    }
  }
  plan_id: 2
  filter {
    predicate {
      rlike {
        left {
          bound {
          }
        }
        right {
          literal {
            string_val: "a.*"
          }
        }
      }
    }
  }
}
plan_id: 1
hash_agg {
  agg_exprs {
    sum {
      child {
        bound {
        }
      }
    }
  }
  agg_exprs {
    count {
    }
    filter {
      scalarFunc {
        func: "substring"
      }
    }
  }
  mode: Final
}
"""


def block():
    return plan_census.operator_tree(plan_census.parse(BLOCK))


def test_parse_reads_nested_blocks_and_scalars():
    root = plan_census.parse('a {\n  b: 1\n  c {\n    d: "x y"\n  }\n}\n')
    (a,) = root.children
    assert a.name == "a"
    assert a.scalars == [("b", "1")]
    assert a.children[0].scalars == [("d", '"x y"')]


def test_operator_tree_finds_the_three_operators_bottom_up():
    op = block()
    assert op.kind == "hash_agg"
    assert [c.kind for c in op.children] == ["filter"]
    assert [c.kind for c in op.children[0].children] == ["native_scan"]


def test_expressions_are_counted_per_operator():
    op = block()
    assert sorted(op.exprs) == ["bound", "func:substring"]
    assert sorted(op.aggs) == ["count", "sum"]
    assert sorted(op.children[0].exprs) == ["bound", "literal", "rlike"]
    assert op.children[0].children[0].exprs == ["bound", "is_not_null"]


def test_default_enum_values_are_named():
    assert block().attrs["mode"] == "Final"
    scan_only = plan_census.operator_tree(plan_census.parse("hash_join {\n}\n"))
    assert scan_only.attrs == {"join_type": "Inner", "build_side": "BuildLeft"}


def test_a_missing_expression_makes_its_operator_and_ancestors_ineligible():
    op = block()
    eligible = plan_census.eligible_ops(op)
    assert eligible == {"native_scan": 1}
    assert plan_census.count_ops(op) == 3


def test_classes_follow_the_vocabulary_table():
    assert plan_census.op_class("filter", {}) == "supported"
    assert plan_census.op_class("sort_merge_join", {"join_type": "Inner"}) == "adaptable"
    assert plan_census.op_class("hash_join", {"join_type": "Existence"}) == "adaptable"
    assert plan_census.op_class("window", {}) == "missing"
    assert plan_census.expr_class("rlike") == "missing"
    assert plan_census.expr_class("func:substring") == "supported"
    assert plan_census.expr_class("func:no_such_function") == "missing"
    assert plan_census.agg_class("bloomFilterAgg") == "missing"


def test_scan_columns_come_from_the_formatted_plan():
    text = """\
(1) CometNativeScan parquet
Output [2]: [d_date_sk#141L, d_year#147L]
Batched: true
Location: InMemoryFileIndex [file:/home/dmitry/comet/data/tpcds-sf40/date_dim.parquet]

(2) CometNativeScan parquet
Output [1]: [ss_item_sk#282L]
Location: InMemoryFileIndex [s3a://tpcds-sf40/store_sales.parquet]
"""
    assert plan_census.scan_columns(text) == {
        ("date_dim", "d_date_sk"), ("date_dim", "d_year"), ("store_sales", "ss_item_sk")}
