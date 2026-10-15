# duckdb-oracle implementation plan

> **For agentic workers:** the chain coordinator dispatches this plan to a developer, task by
> task; steps use checkbox (`- [ ]`) syntax. Commits at most 10 lines; device cycles foreground.
> `- [~]` marks a step the developer does not own: the commits, which the coordinator makes,
> and the device cycle, which needs a shad-gpu that answers. Deviations from a step as
> written are recorded in [`duckdb-oracle-detail.md`](duckdb-oracle-detail.md).

**Goal:** every corpus answer — the cpu's `mini.result.txt` and the device's new
`gpu-result.txt` — is compared with DuckDB's `duckdb-result.txt` under an oracle each
`corpus_query!` line names explicitly; sections over the 256 KB cap compare by fingerprint.

**Architecture:** a new first oracle argument on the macro, carried on `CorpusDeclaration`; a
comparator in `test_support` that reads two rendered sections by column position; a fingerprint
written by both result writers (Rust and Python) in place of the over-cap marker; one generated
case per line in `test_cpu_corpus` (rust-only) for each of the two engines' files.

**Tech stack:** Rust (`macro_rules!`, `inventory`, `sha2`), Python 3 with DuckDB 1.5.4, the
shad-gpu script.

**Spec:** [`duckdb-oracle.md`](duckdb-oracle.md) — committed 9e563348.

## Global constraints

- `duckdb_oracle` is the first oracle argument and is written on every line — no default:
  `corpus_query!(dataset, sf, query, cpu_modes, gpu_modes, duckdb_oracle, cpu_oracle, gpu_oracle, schema_validation)`.
- Five values, `DuckdbOracle::ALL`: `duckdb_exact`, `duckdb_approx`, `duckdb_divergent(<ticket>[, <positions>])`,
  `duckdb_fingerprint`, `duckdb_none`. `duckdb_columns(<positions>)` is added (Task 6 Step 2) only
  if the first run finds a line that needs it; otherwise it is not added at all.
- `duckdb_approx`: a **decimal** cell (by our declared output type) is equal within one unit in our
  last rendered place, `|ours − duck| ≤ 10^−s` with `s` the column's declared scale; a **float**
  cell within `1e-11` relative (`golden_approx_std`'s, `test_support/corpus_gpu.rs:119`); every
  other cell exactly. Measured cpu-vs-DuckDB decimal differences reach 1.2e-5 relative (tpch q1),
  so a relative bound cannot serve decimals.
- `duckdb_divergent(<ticket>[, <positions>])` still checks: the row count exactly, and every column
  *not* named in `<positions>` under `duckdb_approx`'s rule (rows matched by those columns). With no
  positions only the row count is checked. The named columns must still differ somewhere, or the
  line "stopped diverging".
- The fingerprint hashes every column whose cells render identically on both engines (integers,
  strings, dates, booleans — any column with no `.`/`e`/`inf`/`nan` cell); only float and decimal
  columns (any cell with a `.`, an exponent, `inf` or `nan`) get the approximate `sum`/`min`/`max`
  triple, summed in value order on both sides so reassociation cannot move it.
- `duckdb_result.py` renders timestamps as arrow-rs does (`NaiveDateTime`'s `Debug`: `T`, and the
  fraction as `.mmm`, `.uuuuuu` or `.nnnnnnnnn`, none when zero), so a timestamp cell compares exactly.
- `gpu-result.txt` is written **before** the device asserts, so a device answer the cpu rejects is
  still recorded for DuckDB to judge.
- `all_modes` is expanded wherever a test parses the include as text (`test_corpus_goldens/benchmark.rs::modes()`).
- The cap is `RESULT_GOLDEN_MAX_BYTES` = 262144 (`duckdb_result.py`'s `CAP`); unchanged.
- `gpu-result.txt` is a record, never an authority: the device still asserts against
  `mini.result.txt`, and `a_device_run_under_a_regeneration_writes_no_golden` stays.
- The oracle only: no fix to any divergence it finds; each is a ticket and a `duckdb_divergent` line.
- #235 closes whole: the helpers' negative tests (Task 7b) and `ALL` on `CpuOracle`/`GpuResultMode`
  (Task 7c) are in this task.

## Review Focus

1. **A fingerprinted section declared `duckdb_exact`** — must fail naming `duckdb_fingerprint`, not
   pass on an empty row set. Pinned in Task 4.
2. **A `duckdb_divergent(N)` whose ticket was archived** — must fail ("#N is not open"), so a fixed
   divergence cannot hide. Pinned in Task 4.
3. **A section whose two sides disagree on column count** (one engine drops a column) — must fail
   before any row compare, naming both counts. Pinned in Task 4.
4. **The over-cap predicate** — two existing checks read `section.starts_with(SKIPPED)` as "over the
   cap" (`test_cpu_corpus.rs` `each_declarations_two_oracles_suit_each_other`,
   `corpus_gpu.rs` `assert_oracle_suits_the_golden`); a fingerprint section must still read as over
   the cap there, or `live_cpu` lines start failing. Pinned in Task 3.
5. **`all_modes` in the gpu position with `none` in the cpu position, and vice versa** — the macro's
   arm order must expand both; and a text reader of the include (`benchmark.rs::modes()`) must read
   it as the five. Pinned in Task 2.
6. **A section near the cap that one writer fingerprints and the other renders** (DuckDB's `repr`
   prints longer floats than arrow-rs) — the comparator fingerprints the table side itself rather
   than fail to parse. Pinned in Task 4.

## File structure

| file | responsibility |
|---|---|
| `peacockdb-core/src/test_support/duckdb_oracle.rs` (new) | `DuckdbOracle`, `ALL`, `parse`, the section comparator, `duckdb_case` |
| `peacockdb-core/src/test_support/fingerprint.rs` (new) | the fingerprint: write from batches, parse, compare |
| `peacockdb-core/src/test_support/mod.rs` | `mod` lines; `CorpusDeclaration.duckdb_oracle`; re-exports |
| `peacockdb-core/src/test_support/corpus.rs:416-460` | over-cap branch writes the fingerprint |
| `peacockdb-core/src/test_support/corpus_gpu.rs` | `gpu-result.txt` writer; the over-cap predicate |
| `peacockdb-core/tests/test_cpu_corpus.rs`, `test_gpu_corpus.rs` | the argument, `all_modes`, the cases, the `ALL` test |
| `peacockdb-core/tests/common/corpus_cases.inc` | every line's oracle; `all_modes` |
| `peacockdb-core/tests/test_golden_format.rs` | fingerprint round trip |
| `testdata/duckdb_result.py` | the fingerprint for over-cap sections |
| `testdata/goldens/{tpch,tpcds}.sf1/` | regenerated `duckdb-result.txt`, `mini.result.txt`; `gpu-result.txt` |
| `scripts/build-test-shadgpu.sh` | `PCK_WRITE_GPU_RESULT`; `--pull-results` |
| `llm-wiki/build-test.md`, `tickets/corpus-coverage.md` | the tier; #235 archived |

---

### Task 1: `DuckdbOracle`, parsed and listed

**Files:**
- Create: `peacockdb-core/src/test_support/duckdb_oracle.rs`
- Modify: `peacockdb-core/src/test_support/mod.rs` (`mod duckdb_oracle;` beside `mod corpus_gpu;` at `:17`; `pub use duckdb_oracle::{DuckdbOracle};`)

**Interfaces:**
- Produces: `pub enum DuckdbOracle { Exact, Approx, Divergent { ticket: u32, columns: Vec<usize> }, Fingerprint, None }` (and `Columns(Vec<usize>)` only if Task 6 Step 2 adds it)`;
  `DuckdbOracle::parse(&str) -> DuckdbOracle` (panics naming the accepted set);
  `DuckdbOracle::ALL: [&'static str; 5]`; `DuckdbOracle::name(&self) -> &'static str`.

- [x] **Step 1: The failing tests** (module tests at the bottom of the new file):

```rust
#[cfg(test)]
mod tests {
    use super::DuckdbOracle;

    #[test]
    fn every_spelling_parses_and_names_itself() {
        assert_eq!(DuckdbOracle::parse("duckdb_exact"), DuckdbOracle::Exact);
        assert_eq!(DuckdbOracle::parse("duckdb_divergent(243)"),
                   DuckdbOracle::Divergent { ticket: 243, columns: vec![] });
        assert_eq!(DuckdbOracle::parse("duckdb_divergent (243, 1, 2)"),
                   DuckdbOracle::Divergent { ticket: 243, columns: vec![1, 2] });
        // stringify! of `duckdb_divergent(243)` may put spaces in; the parse ignores them.
        assert_eq!(DuckdbOracle::parse("duckdb_fingerprint"), DuckdbOracle::Fingerprint);
        for name in DuckdbOracle::ALL {
            let sample = match name {
                "duckdb_divergent" => "duckdb_divergent(1)".to_string(),
                other => other.to_string(),
            };
            assert_eq!(DuckdbOracle::parse(&sample).name(), name);
        }
    }

    #[test]
    #[should_panic(expected = "duckdb_exact|duckdb_approx")]
    fn a_typo_names_the_accepted_set() {
        DuckdbOracle::parse("duckdb_exakt");
    }
}
```

- [x] **Step 2: Run red.** `cargo test --features rust-only -p peacockdb-core --lib test_support::duckdb_oracle`
  — fails to compile (`DuckdbOracle` undefined).
- [x] **Step 3: The enum.**

```rust
//! What a corpus line asks of DuckDB's answer (#235). One argument per line, explicit on each.

/// The oracle a `corpus_query!` line names first. Every variant compares the line's section in
/// `duckdb-result.txt` with the same query's section in `mini.result.txt` (the cpu) and, where it
/// holds one, `gpu-result.txt` (the device), by column position — names differ by engine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DuckdbOracle {
    /// Rows as a multiset, every cell equal as rendered.
    Exact,
    /// Decimal cells within one unit in our last place, float cells within `APPROX` relative,
    /// the rest equal.
    Approx,
    /// The named columns must differ somewhere and `ticket` must be open; the row count and every
    /// column not named still compare as `Approx` does. No columns: only the row count.
    Divergent { ticket: u32, columns: Vec<usize> },
    /// Both sides are over the cap and hold a fingerprint.
    Fingerprint,
    /// Nothing to compare: one side does not answer.
    None,
}

/// `golden_approx_std`'s tolerance (`corpus_gpu.rs`), for float cells only. A decimal cell's
/// tolerance is one unit in its declared scale's last place (`decimal_tolerance`).
pub const APPROX: f64 = 1e-11;

/// `10^-scale`: the gap between two adjacent values of a decimal rendered at `scale` digits.
pub fn decimal_tolerance(scale: i8) -> f64 {
    10f64.powi(-(scale as i32))
}

impl DuckdbOracle {
    pub const ALL: [&'static str; 5] = [
        "duckdb_exact", "duckdb_approx", "duckdb_divergent", "duckdb_fingerprint", "duckdb_none",
    ];

    pub fn name(&self) -> &'static str {
        match self {
            Self::Exact => "duckdb_exact",
            Self::Approx => "duckdb_approx",
            Self::Divergent { .. } => "duckdb_divergent",
            Self::Fingerprint => "duckdb_fingerprint",
            Self::None => "duckdb_none",
        }
    }

    pub fn parse(spelled: &str) -> Self {
        let s: String = spelled.chars().filter(|c| !c.is_whitespace()).collect();
        let args = |prefix: &str| -> Option<Vec<String>> {
            s.strip_prefix(prefix)?.strip_prefix('(')?.strip_suffix(')')
                .map(|inner| inner.split(',').map(str::to_string).collect())
        };
        let number = |a: &str| a.parse::<usize>().unwrap_or_else(|_| panic!("{spelled}: `{a}` is not a number"));
        match s.as_str() {
            "duckdb_exact" => Self::Exact,
            "duckdb_approx" => Self::Approx,
            "duckdb_fingerprint" => Self::Fingerprint,
            "duckdb_none" => Self::None,
            _ => {
                if let Some(a) = args("duckdb_divergent") {
                    assert!(!a.is_empty(), "{spelled}: a ticket first");
                    return Self::Divergent {
                        ticket: number(&a[0]) as u32,
                        columns: a[1..].iter().map(|x| number(x)).collect(),
                    };
                }
                panic!(
                    "corpus_query!: unknown duckdb_oracle '{spelled}' (expected \
                     duckdb_exact|duckdb_approx|duckdb_divergent(<ticket>[, <positions>])|\
                     duckdb_fingerprint|duckdb_none)"
                )
            }
        }
    }
}
```

- [x] **Step 4: Run green**, same command.
- [~] **Step 5: Commit.** `git commit -m "#235: DuckdbOracle, its five spellings and ALL"`.

### Task 2: The macro's new argument and `all_modes`

**Files:**
- Modify: `peacockdb-core/tests/test_cpu_corpus.rs:13-70` (doc, `corpus_query!`, `declare_corpus_query!`)
- Modify: `peacockdb-core/tests/test_gpu_corpus.rs:12-45`
- Modify: `peacockdb-core/src/test_support/mod.rs:310-316` (`CorpusDeclaration` gains `pub duckdb_oracle: &'static str`)
- Modify: `peacockdb-core/tests/common/corpus_cases.inc` (all 120 `corpus_query!` lines; the header comment `:1-10`)
- Modify: `peacockdb-core/tests/test_corpus_goldens/benchmark.rs:356-362` (`modes()` reads `all_modes`)

**Interfaces:**
- Consumes: Task 1's `DuckdbOracle::parse`.
- Produces: `CorpusDeclaration { dataset, sf, query, duckdb_oracle, cpu_oracle, gpu_oracle }`, with
  `duckdb_oracle` the `stringify!` of the line's argument (e.g. `"duckdb_divergent (243)"`).

- [x] **Step 1: The cpu macro.** Two sugar arms first, then the two existing arms with the new
  argument. `$duck` is an ident with an optional parenthesized list:

```rust
macro_rules! corpus_query {
    // all_modes, in either position, expands to the five and recurses.
    ($d:ident, $sf:expr, $q:ident, all_modes, $($rest:tt)*) => {
        corpus_query!($d, $sf, $q, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, $($rest)*);
    };
    ($d:ident, $sf:expr, $q:ident, $($cpu:ident)|+, all_modes, $($rest:tt)*) => {
        corpus_query!($d, $sf, $q, $($cpu)|+, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, $($rest)*);
    };
    ($dataset:ident, $sf:expr, $query:ident, none, $($gpu:ident)|+,
     $duck:ident $(($($duck_arg:literal),*))?, $cpu_oracle:ident, $gpu_oracle:ident, $validation:ident) => {
        declare_corpus_query!($dataset, $sf, $query, stringify!($duck $(($($duck_arg),*))?), $cpu_oracle, $gpu_oracle);
    };
    ($dataset:ident, $sf:expr, $query:ident, $($cpu:ident)|+, $($gpu:ident)|+,
     $duck:ident $(($($duck_arg:literal),*))?, $cpu_oracle:ident, $gpu_oracle:ident, $validation:ident) => {
        declare_corpus_query!($dataset, $sf, $query, stringify!($duck $(($($duck_arg),*))?), $cpu_oracle, $gpu_oracle);
        // ... the existing per-mode `cpu_` case and RegistryEntry, unchanged ...
    };
}
```

  `declare_corpus_query!` takes `$duck:expr` and submits `duckdb_oracle: $duck`. The device
  macro gets the same two sugar arms and the new argument in both of its arms, consumed and
  dropped as the cpu oracle is.
- [x] **Step 2: Every line.** One script, run once, committed with the change:

```bash
python3 - <<'EOF'
import re, pathlib
p = pathlib.Path("peacockdb-core/tests/common/corpus_cases.inc")
five = "tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized"
out = []
for line in p.read_text().splitlines(keepends=True):
    if line.startswith("corpus_query!("):
        line = line.replace(five, "all_modes")
        # the first oracle argument: insert before the cpu oracle (data_fusion_*), provisional
        line = re.sub(r", (data_fusion_\w+),", r", duckdb_none, \1,", line, count=1)
    out.append(line)
p.write_text("".join(out))
EOF
```

  `duckdb_none` is the provisional value only; Task 6 sets every line from the first run, and
  Task 5's case fails a `duckdb_none` over two sections that both exist, so none can stay
  wrong. The header comment (`:1-10`) shows the new signature and `all_modes`.
- [x] **Step 3: The sugar's own test** (Review Focus 5), in `test_cpu_corpus.rs`: a declaration
  `corpus_query!(tpch, 1, q6_sugar_probe, ...)` would add a registry row, so instead assert on
  what the include already holds — every line whose cpu or gpu modes were the five now
  registers five cells:

```rust
#[test]
fn all_modes_expands_to_the_five_in_either_position() {
    let rows = load_csv();
    let q6 = rows.iter().find(|r| r.dataset == "tpch" && r.query == "q6").expect("q6");
    for mode in &MODES {
        assert_eq!(q6.states[&format!("cpu_{}", mode.ident())], "enabled");
        assert_eq!(q6.states[&format!("gpu_{}", mode.ident())], "enabled");
    }
    // and the registration side: inventory holds five cpu entries for q6
    let n = inventory::iter::<RegistryEntry>
        .into_iter()
        .filter(|e| e.kind == "cpu" && e.dataset == "tpch" && e.query == "q6")
        .count();
    assert_eq!(n, 5); // tpcds has a q6 too: without the dataset filter this counts 10
}
```

  `test_corpus_goldens/benchmark.rs::modes()` parses the include as text, so it learns the sugar
  in the same commit, or `every_timed_case_is_enabled_on_a_device` reads `{"all_modes"}` for tpch
  q6 (timed at `tp1_single | tp4_sized`) and goes red:

```rust
/// A `mode1 | mode2` argument as a set. `none` is the empty set; `all_modes` the five.
fn modes(argument: &str) -> BTreeSet<String> {
    match argument {
        "none" => BTreeSet::new(),
        "all_modes" => ["tp1_single", "tp1_rowgroup", "tp4_single", "tp4_rowgroup", "tp4_sized"]
            .into_iter()
            .map(String::from)
            .collect(),
        named => named.split('|').map(|m| m.trim().to_string()).collect(),
    }
}
```

  Any other reader of the include as text is found by `git grep -n 'corpus_cases.inc' -- '*.rs' '*.py' '*.sh'`
  and taught the same; each is named in the commit.

  (`tpch/q14` — `all_modes, none` — covers the other order through `the_registry_matches_the_cpu_corpus_in_both_directions`.)
- [x] **Step 4: Run.** `cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- --list | wc -l`
  equals the count before plus one (the new test); `the_registry_matches_the_cpu_corpus_in_both_directions`
  and `all_modes_expands_to_the_five_in_either_position` green; `cargo test --features rust-only -p peacockdb-core --test test_corpus_goldens benchmark`
  green (the `modes()` change). The device binary compiles:
  `CUDF_ROOT=~/data/miniforge3/envs/rapids scripts/cargo-cudf.sh test -p peacockdb-core --test test_gpu_corpus --no-run`
  in a workspace, never the primary checkout.
- [~] **Step 5: Commit.** `git commit -m "#235: corpus_query! names its DuckDB oracle first; all_modes"`.

### Task 3: The fingerprint, on both writers

**Files:**
- Create: `peacockdb-core/src/test_support/fingerprint.rs` (`pub mod fingerprint;` in `test_support/mod.rs`, so `test_golden_format.rs` reaches it)
- Create: `testdata/test_duckdb_result.py`; Modify: `.github/workflows/pipeline.yml` (beside `python3 testdata/test_duckdb_cost.py`, `:718`)
- Modify: `peacockdb-core/src/test_support/corpus.rs:416-460` (`over_cap`, `assert_result_section`)
- Modify: `peacockdb-core/src/test_support/corpus_gpu.rs:66-95` (`assert_oracle_suits_the_golden`)
- Modify: `peacockdb-core/tests/test_cpu_corpus.rs` (`each_declarations_two_oracles_suit_each_other`, `over_cap` local)
- Modify: `testdata/duckdb_result.py:96-100`
- Test: `peacockdb-core/tests/test_golden_format.rs`

**Interfaces:**
- Produces: `pub const FINGERPRINT: &str = "fingerprint: "`;
  `pub fn fingerprint_of(batches: &[RecordBatch]) -> String`;
  `pub fn is_fingerprint(section: &str) -> bool`;
  `pub fn compare_fingerprints(ours: &str, duckdb: &str, tol: f64) -> Result<(), String>`;
  `pub fn is_over_cap(section: &str) -> bool` (a `SKIPPED` marker naming the cap, or a fingerprint).

- [x] **Step 1: The format**, written identically by Rust and Python:

```
fingerprint: rows=<n>
col <i>: nonnull=<n> sum=<x> min=<x> max=<x>     -- an approximate column
col <i>: nonnull=<n>                             -- an exact column
hash: <hex sha256>
```

  A cell renders as in the result goldens (Arrow's display; the Python `cell()` at
  `duckdb_result.py:36-47`), NULL as empty. The column's class is read off its rendered cells, so
  both writers classify alike whatever type each engine declared:
  - **approximate**: some non-null cell contains `.`, `e`/`E`, or is `inf`/`-inf`/`NaN`/`nan`, and
    every non-null cell parses as `f64` — floats, and decimals (ours a decimal, DuckDB's often a
    double);
  - **exact**: everything else — integers, strings, dates, timestamps, booleans.

  `sum` is the `f64` sum of the column's non-null values **sorted by value**, so both sides add in
  one order; `sum`/`min`/`max` print as `{:e}` (Rust) / `'{:e}'.format` (Python), the same text.
  `hash` is SHA-256 of each row's **exact** cells joined by `|`, rows sorted as byte strings and
  joined by `\n` — so an over-cap join of integer columns is checked row by row, not by sums alone.
- [x] **Step 2: Failing tests** (`test_golden_format.rs`):

```rust
use datafusion::arrow::array::{Float64Array, Int64Array, StringArray};
use datafusion::arrow::datatypes::{DataType, Field, Schema};
use datafusion::arrow::record_batch::RecordBatch;
use peacockdb_core::test_support::fingerprint::{compare_fingerprints, fingerprint_of, is_over_cap};
use std::sync::Arc;

fn batch(ids: Vec<i64>, names: Vec<&str>, x: Vec<f64>) -> RecordBatch {
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("s", DataType::Utf8, false),
        Field::new("x", DataType::Float64, false),
    ]));
    RecordBatch::try_new(schema, vec![
        Arc::new(Int64Array::from(ids)),
        Arc::new(StringArray::from(names)),
        Arc::new(Float64Array::from(x)),
    ]).unwrap()
}

#[test]
fn a_fingerprint_does_not_depend_on_row_order() {
    let a = fingerprint_of(&[batch(vec![1, 2], vec!["a", "b"], vec![1.5, 1e17])]);
    let b = fingerprint_of(&[batch(vec![2], vec!["b"], vec![1e17]), batch(vec![1], vec!["a"], vec![1.5])]);
    assert_eq!(a, b);
}

#[test]
fn integers_and_strings_are_hashed_and_floats_are_summed() {
    let fp = fingerprint_of(&[batch(vec![1, 2], vec!["a", "b"], vec![1.5, 2.5])]);
    assert!(fp.contains("col 0: nonnull=2\n"), "{fp}");       // id: exact, hashed
    assert!(fp.contains("col 1: nonnull=2\n"), "{fp}");       // s: exact, hashed
    assert!(fp.contains("col 2: nonnull=2 sum=4e0"), "{fp}");  // x: approximate
    // a mis-paired join: the same ids and names, paired differently, changes the hash
    let swapped = fingerprint_of(&[batch(vec![1, 2], vec!["b", "a"], vec![1.5, 2.5])]);
    assert!(compare_fingerprints(&fp, &swapped, 1e-11).unwrap_err().contains("hash"));
}

#[test]
fn a_sum_off_in_the_thirteenth_digit_passes_and_in_the_third_fails() {
    let fp = fingerprint_of(&[batch(vec![1], vec!["a"], vec![1.0])]);
    let near = fp.replace("sum=1e0", "sum=1.000000000001e0");
    let far = fp.replace("sum=1e0", "sum=1.01e0");
    assert!(compare_fingerprints(&fp, &near, 1e-11).is_ok());
    assert!(compare_fingerprints(&fp, &far, 1e-11).unwrap_err().contains("col 2"));
}

#[test]
fn the_over_cap_predicate_reads_both_markers() {
    let fp = fingerprint_of(&[batch(vec![1], vec!["a"], vec![1.0])]);
    assert!(is_over_cap(&fp));
    assert!(is_over_cap("skipped: the result is at or above the 262144-byte cap\n"));
    assert!(!is_over_cap("skipped: not enabled at any mode\n"));
}
```

  And `testdata/test_duckdb_result.py` (new, stdlib `unittest`, run as `python3 testdata/test_duckdb_result.py`
  beside `test_duckdb_cost.py` in `pipeline.yml`'s cost-report job): the Python writer's
  fingerprint of the same three rows equals the Rust one's text, byte for byte (the expected
  strings are pasted from the Rust test's output in Step 5), and `cell()` renders timestamps as
  arrow-rs:

```python
import datetime, os, sys, unittest
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import duckdb_result as dr  # noqa: E402

class Cells(unittest.TestCase):
    def test_timestamps_render_as_arrow_rs(self):
        t = datetime.datetime(2024, 1, 1, 0, 0, 0)
        self.assertEqual(dr.cell(t), "2024-01-01T00:00:00")
        self.assertEqual(dr.cell(t.replace(microsecond=1000)), "2024-01-01T00:00:00.001")
        self.assertEqual(dr.cell(t.replace(microsecond=1500)), "2024-01-01T00:00:00.001500")
        self.assertEqual(dr.cell(datetime.date(2024, 1, 2)), "2024-01-02")

if __name__ == "__main__":
    unittest.main()
```
- [x] **Step 3: Run red**, `cargo test --features rust-only -p peacockdb-core --test test_golden_format`
  and `python3 testdata/test_duckdb_result.py`.
- [x] **Step 4: Implement** `fingerprint.rs`; in `assert_result_section` the two `over_cap(...)`
  arms become `format!("{}mode={}\n", fingerprint_of(batches), mode.name)` — the fingerprint is
  computed from the batches without rendering the table, so the size bound at `:440` still spares
  memory. Replace `section.starts_with(SKIPPED)` at the two over-cap reads with
  `is_over_cap(&section)`. Python: in `generate`, the over-cap branch writes
  `fingerprint(names, rows)` (same rules) instead of `skipped:`; and `cell()`'s datetime arm
  renders as arrow-rs does:

```python
def cell(value):
    if value is None:
        return ""
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, float):
        return repr(value)
    if isinstance(value, decimal.Decimal):
        return format(value, "f")
    if isinstance(value, datetime.datetime):
        # arrow-rs prints NaiveDateTime's Debug: a 'T', and the fraction in groups of three
        # digits, none when zero (DuckDB hands TIMESTAMP_NS back at microsecond precision)
        base = value.strftime("%Y-%m-%dT%H:%M:%S")
        us = value.microsecond
        if us == 0:
            return base
        if us % 1000 == 0:
            return f"{base}.{us // 1000:03d}"
        return f"{base}.{us:06d}"
    if isinstance(value, datetime.date):
        return value.isoformat()
    return str(value)
```

  `datetime.datetime` is checked before `datetime.date` because it is a subclass of it.
- [x] **Step 5: Run green**; then `UPDATE_CANONICAL=1 PCK_UPDATE_SECTIONS=1 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- filter_project q16 anti_join semi_join q11 q98`
  regenerates the six over-cap sections in `mini.result.txt` (the others must not move — `git diff --stat`
  shows only those sections), and `python3 testdata/duckdb_result.py` (DuckDB 1.5.4) rewrites both
  `duckdb-result.txt`; only the six sections move.
- [~] **Step 6: Commit.** `git commit -m "#235: over-cap sections hold a fingerprint on both writers"`.

### Task 4: The section comparator

**Files:**
- Modify: `peacockdb-core/src/test_support/duckdb_oracle.rs`

**Interfaces:**
- Consumes: Task 1; Task 3's `fingerprint::{is_fingerprint, compare_fingerprints}`.
- Produces: `pub enum CellKind { Exact, Float, Decimal(i8) }`;
  `pub fn compare_sections(oracle: &DuckdbOracle, ours: &str, duckdb: &str, kinds: &[CellKind], tickets_open: &dyn Fn(u32) -> bool) -> Result<(), String>`
  where `ours` is a `mini.result.txt` / `gpu-result.txt` section body (its `mode=` line stripped by
  the caller), `duckdb` the `duckdb-result.txt` body, and `kinds[i]` column `i`'s class from our
  declared output schema (Task 5 plans the query to get it).

- [x] **Step 1: The failing tests**, doctored sections, one per variant and per Review Focus 1–3:

```rust
const OURS: &str = "+---+-----+\n| a | b   |\n+---+-----+\n| 1 | x   |\n| 2 | 1.5 |\n+---+-----+";
const DUCK: &str = "+---+-----+\n| a | sum |\n+---+-----+\n| 1 | x   |\n| 2 | 1.5 |\n+---+-----+\n";
fn open(_: u32) -> bool { true }
fn closed(_: u32) -> bool { false }
const KINDS: [CellKind; 2] = [CellKind::Exact, CellKind::Exact];

fn div(ticket: u32, columns: Vec<usize>) -> DuckdbOracle { DuckdbOracle::Divergent { ticket, columns } }

#[test] fn exact_passes_by_position_whatever_the_names() {
    assert!(compare_sections(&DuckdbOracle::Exact, OURS, DUCK, &KINDS, &open).is_ok());
}
#[test] fn a_wrong_row_fails_exact() {
    let bad = DUCK.replace("| 2 | 1.5 |", "| 3 | 1.5 |");
    assert!(compare_sections(&DuckdbOracle::Exact, OURS, &bad, &KINDS, &open).unwrap_err().contains("row"));
}
#[test] fn approx_holds_a_float_to_1e11_relative() {
    let ours = "+-----+\n| x   |\n+-----+\n| 1.5 |\n+-----+";
    let near = "+-----+\n| x   |\n+-----+\n| 1.5000000000001 |\n+-----+\n";
    let far = "+-----+\n| x   |\n+-----+\n| 1.5001 |\n+-----+\n";
    assert!(compare_sections(&DuckdbOracle::Approx, ours, near, &[CellKind::Float], &open).is_ok());
    assert!(compare_sections(&DuckdbOracle::Approx, ours, far, &[CellKind::Float], &open).is_err());
    assert!(compare_sections(&DuckdbOracle::Exact, ours, near, &[CellKind::Float], &open).is_err());
}
#[test] fn approx_holds_a_decimal_to_one_unit_in_our_last_place() {
    // tpch q1's avg_qty: ours truncates at scale 6, DuckDB answers a double
    let ours = "+-----------+\n| avg_qty   |\n+-----------+\n| 25.522005 |\n+-----------+";
    let duck = "+--------------------+\n| avg_qty            |\n+--------------------+\n| 25.522005853257337 |\n+--------------------+\n";
    let off = duck.replace("25.522005853257337", "25.522007");
    assert!(compare_sections(&DuckdbOracle::Approx, ours, duck, &[CellKind::Decimal(6)], &open).is_ok());
    assert!(compare_sections(&DuckdbOracle::Approx, ours, &off, &[CellKind::Decimal(6)], &open).is_err());
}
#[test] fn divergent_needs_a_difference_and_an_open_ticket() {
    let bad = DUCK.replace("| 2 | 1.5 |", "| 2 | 9.5 |");
    assert!(compare_sections(&div(80, vec![1]), OURS, &bad, &KINDS, &open).is_ok());
    assert!(compare_sections(&div(80, vec![1]), OURS, DUCK, &KINDS, &open).unwrap_err().contains("stopped diverging"));
    assert!(compare_sections(&div(80, vec![1]), OURS, &bad, &KINDS, &closed).unwrap_err().contains("#80 is not open"));
}
#[test] fn divergent_still_checks_the_row_count_and_the_columns_it_does_not_name() {
    // column 1 is declared divergent; column 0 must still match, and so must the count
    let col0_wrong = DUCK.replace("| 2 | 1.5 |", "| 7 | 9.5 |");
    assert!(compare_sections(&div(80, vec![1]), OURS, &col0_wrong, &KINDS, &open).unwrap_err().contains("column 0"));
    let extra_row = DUCK.replace("+---+-----+\n", "").replace("| 2 | 1.5 |", "| 2 | 9.5 |\n| 3 | z   |");
    let extra_row = format!("+---+-----+\n{extra_row}+---+-----+\n");
    assert!(compare_sections(&div(80, vec![]), OURS, &extra_row, &KINDS, &open).unwrap_err().contains("rows"));
}
#[test] fn a_column_count_difference_fails_first() {
    let wide = "+---+---+---+\n| a | b | c |\n+---+---+---+\n| 1 | x | 0 |\n| 2 | 1.5 | 0 |\n+---+---+---+\n";
    assert!(compare_sections(&DuckdbOracle::Exact, OURS, wide, &KINDS, &open).unwrap_err().contains("2 columns against 3"));
}
#[test] fn a_fingerprinted_section_needs_the_fingerprint_oracle() {
    let fp = "fingerprint: rows=2\ncol 0: nonnull=2 sum=3e0 min=1e0 max=2e0\nhash: 00\n";
    assert!(compare_sections(&DuckdbOracle::Exact, fp, fp, &KINDS, &open).unwrap_err().contains("duckdb_fingerprint"));
}
#[test] fn one_side_fingerprinted_near_the_cap_is_fingerprinted_here_too() {
    // DuckDB's repr prints longer floats, so its rendering crossed the cap and ours did not
    let ours_fp = fingerprint::fingerprint_of_rendered(OURS);
    assert!(compare_sections(&DuckdbOracle::Fingerprint, OURS, &ours_fp, &KINDS, &open).is_ok());
}
```

- [x] **Step 2: Run red**, `cargo test --features rust-only -p peacockdb-core --lib test_support::duckdb_oracle`.
- [x] **Step 3: The comparator.** `fingerprint.rs` gains `pub fn fingerprint_of_rendered(table: &str) -> String`,
  the same fingerprint computed from a rendered table's cells (Task 3's rules read cells anyway).
  Parse a rendered table into `(width, rows: Vec<Vec<String>>)` with
  the cell split `assert_sorted_str_approx`'s `split_cells` uses (`corpus_gpu.rs:186-196`) — lift
  it into this file as `pub(crate) fn split_cells` and have `corpus_gpu.rs` call it, rather than a
  copy. Then:

```rust
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CellKind { Exact, Float, Decimal(i8) }

pub fn compare_sections(
    oracle: &DuckdbOracle, ours: &str, duckdb: &str, kinds: &[CellKind],
    tickets_open: &dyn Fn(u32) -> bool,
) -> Result<(), String> {
    use fingerprint::{compare_fingerprints, fingerprint_of_rendered, is_fingerprint};
    let (of, df) = (is_fingerprint(ours), is_fingerprint(duckdb));
    match oracle {
        DuckdbOracle::Fingerprint => {
            if !of && !df {
                return Err("duckdb_fingerprint over a section that is not fingerprinted".into());
            }
            // near the cap one writer may have rendered what the other fingerprinted
            let o = if of { ours.to_string() } else { fingerprint_of_rendered(ours) };
            let d = if df { duckdb.to_string() } else { fingerprint_of_rendered(duckdb) };
            return compare_fingerprints(&o, &d, APPROX);
        }
        DuckdbOracle::None => return Err("duckdb_none over two sections that both exist".into()),
        _ if of || df => {
            return Err(format!("{}: the section is fingerprinted (over the cap) — declare duckdb_fingerprint", oracle.name()))
        }
        _ => {}
    }
    let (ow, orows) = parse_table(ours);
    let (dw, drows) = parse_table(duckdb);
    if ow != dw { return Err(format!("ours has {ow} columns against {dw} in DuckDB's")); }
    if orows.len() != drows.len() {
        return Err(format!("ours has {} rows, DuckDB {}", orows.len(), drows.len()));
    }
    let all: Vec<usize> = (0..ow).collect();
    match oracle {
        DuckdbOracle::Exact => same_multiset(&orows, &drows, &all, None),
        DuckdbOracle::Approx => same_multiset(&orows, &drows, &all, Some(kinds)),
        DuckdbOracle::Divergent { ticket, columns } => {
            // empty positions: a row-level divergence, so only the row count (checked above) holds
            let rest: Vec<usize> = if columns.is_empty() { vec![] }
                                   else { all.iter().copied().filter(|c| !columns.contains(c)).collect() };
            same_multiset(&orows, &drows, &rest, Some(kinds))?; // the undeclared columns still agree
            let named: Vec<usize> = if columns.is_empty() { all.clone() } else { columns.clone() };
            match same_multiset(&orows, &drows, &named, Some(kinds)) {
                Ok(()) => Err(format!("declared divergent on #{ticket} and it stopped diverging")),
                Err(_) if !tickets_open(*ticket) => Err(format!("#{ticket} is not open")),
                Err(_) => Ok(()),
            }
        }
        DuckdbOracle::Fingerprint | DuckdbOracle::None => unreachable!(),
    }
}

/// Equal tolerance per cell: exact text, `APPROX` relative for a float, one unit in the last place
/// for a decimal of declared scale `s`.
fn cell_equal(kind: CellKind, ours: &str, duck: &str) -> bool {
    if ours == duck { return true; }
    let (Ok(a), Ok(b)) = (ours.parse::<f64>(), duck.parse::<f64>()) else { return false };
    match kind {
        CellKind::Exact => false,
        CellKind::Float => {
            if a.is_nan() && b.is_nan() { return true; }
            (a - b).abs() <= APPROX * a.abs().max(b.abs())
        }
        CellKind::Decimal(s) => (a - b).abs() <= decimal_tolerance(s) * (1.0 + 1e-12),
    }
}
```

  `same_multiset(ours, duck, columns, kinds)` projects both row lists onto `columns`, sorts them
  (a tolerance groups rows by their `CellKind::Exact` cells first, then sorts by the approximate
  cells as numbers), and pairs row `i` with row `i`, comparing each kept column with `cell_equal`
  (`kinds = None` means every cell exact). The first mismatch is `Err("row {i}, column {c}: ours
  {a}, DuckDB {b}")` — never a panic, so `Divergent` can use it as a predicate. `tickets_open` in production reads `llm-wiki/tickets/*.md` for
  `<a id="t{N}">` (`env!("CARGO_MANIFEST_DIR")/../llm-wiki/tickets`); an archived ticket lives in
  `archive/archived-tickets.md`, which is not read.
- [x] **Step 4: Run green.**
- [~] **Step 5: Commit.** `git commit -m "#235: the DuckDB section comparator, by position"`.

### Task 5: One case per line, for each engine's file

**Files:**
- Modify: `peacockdb-core/src/test_support/duckdb_oracle.rs` (`duckdb_case`)
- Modify: `peacockdb-core/tests/test_cpu_corpus.rs` (the macro's two arms; the `ALL` test)

**Interfaces:**
- Consumes: Tasks 1–4; `corpus_golden::{result_golden, section_of}`; `golden_dir_for`.
- Produces: `pub fn duckdb_case(dataset: &str, sf: &str, query: &str, oracle: &str, engine: Engine)`,
  `pub enum Engine { Cpu, Device }`.

- [x] **Step 1:** In both arms of the cpu macro, after `declare_corpus_query!`:

```rust
paste::paste! {
    #[test]
    fn [<duckdb_ $dataset _ $query>]() {
        duckdb_case(stringify!($dataset), stringify!($sf), &stringify!($query).replace('_', "-"),
                    stringify!($duck $(($($duck_arg),*))?), Engine::Cpu);
    }
    #[test]
    fn [<duckdb_gpu_ $dataset _ $query>]() {
        duckdb_case(stringify!($dataset), stringify!($sf), &stringify!($query).replace('_', "-"),
                    stringify!($duck $(($($duck_arg),*))?), Engine::Device);
    }
}
```

  `duckdb_case` reads DuckDB's section (`duckdb-result.txt` beside `mini.result.txt`; a `failed:`
  or absent section means DuckDB did not answer), ours (`mini.result.txt` for `Cpu`;
  `gpu-result.txt` for `Device`, where an absent file or section is "nothing to compare" and the
  case returns), strips our `mode=` line, and: under `duckdb_none` requires that one side not answer
  (`skipped: not enabled`, absent, or `failed:`); under every other oracle requires both to answer
  and calls `compare_sections`, panicking with `{dataset}/{query} ({engine}): {err}`. The `kinds`
  come from our declared output schema: the query planned once at `tp1-single` with
  `corpus::plan_at` (`test_support/corpus.rs:41`, the planning call both corpora share), and each
  output field mapped `Float16|Float32|Float64 → CellKind::Float`, `Decimal128(_, s) →
  CellKind::Decimal(s)`, anything else `CellKind::Exact`. Planning only, no execution, so the case
  stays rust-only and fast.
- [x] **Step 2: The `ALL` test**:

```rust
#[test]
fn every_duckdb_oracle_is_named_by_some_line() {
    for name in DuckdbOracle::ALL {
        assert!(
            inventory::iter::<CorpusDeclaration>.into_iter()
                .any(|d| DuckdbOracle::parse(d.duckdb_oracle).name() == name),
            "{name} is named by no corpus_query! line — delete it rather than keep it"
        );
    }
}
```

  It goes red until Task 6 names `duckdb_divergent` on some line, or the first run shows none is
  needed — in which case that variant is deleted from the enum and `ALL` with the reason in the
  commit, as the spec's rule wants. Task 5's red cases and this test
  land in the same PR as Task 6, which turns them green; Task 5's commit message says so.
- [x] **Step 3: Run** `cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus duckdb_ 2>&1 | tee /tmp/duckdb-first-run.txt`
  — every line is `duckdb_none` from Task 2, so every case that has two answers fails with
  "duckdb_none over two sections that both exist". That output is Task 6's worklist.
- [x] **Step 4: No commit yet.** The first run's red cases are what Task 6 reads to set every
  line's oracle; this task's code is committed together with Task 6's line changes, so no commit is
  red: `git commit -m "#235: a DuckDB case per corpus line, cpu and device, and every line's oracle"`
  is Task 6's Step 4.

### Task 6: Every line's oracle

**Files:**
- Modify: `peacockdb-core/tests/common/corpus_cases.inc`
- Modify: `llm-wiki/tickets/*.md` (new tickets for real divergences)

- [x] **Step 1:** For each failing `duckdb_tpch_*`/`duckdb_tpcds_*` case, set the line's oracle by
  trying in order and keeping the first that passes: `duckdb_exact`; `duckdb_approx` (#235's digit
  rows: tpch q1, q8, q14, shuffle-stddev, shuffle-additive-avg; tpcds q7, q9, q13, q18, q26, q39,
  q58, q59, q61, q66, q75, q85, q90); `duckdb_fingerprint` (five today: tpch q16, anti-join,
  filter-project, semi-join; tpcds q98 — tpch q11 joins them when join-backend turns its cpu cells
  on). `duckdb_none` stays only where one side does not answer
  (the 18 DuckDB-only tpcds queries, tpch q11 and q22, tpcds q24 and q54 — #190 — if their cpu
  cells are off). A row that fits none: a real divergence — file a ticket (`corpus-coverage.md`,
  next free number from `tickets.md`), and the line says `duckdb_divergent(<n>, <the diverging
  positions>)`, so the rest of the row still checks. #235's decimal `avg` and division rows (tpch
  q1, q8, shuffle-additive-avg; tpcds q7, q26, q58, q61, q66, q90) pass `duckdb_approx` under the
  one-unit-in-the-last-place rule — they are not divergences. A decimal row that misses by more
  than one unit is a real divergence and gets its ticket.
- [x] **Step 2: Only if a tie is found.** A row that differs only because a LIMIT window's cutoff
  ties (both engines keep valid, different rows), confirmed by reading the query and both sections
  by hand, is the one case that adds `duckdb_columns`. If no line needs it, skip this step: the
  variant is not added. If one does, add it in this commit:

```rust
    /// Only these column positions, as a multiset: a LIMIT window whose cutoff ties.
    Columns(Vec<usize>),
```
  with `"duckdb_columns"` appended to `ALL` (now `[&str; 6]`), `Self::Columns(_) => "duckdb_columns"`
  in `name`, in `parse` before the panic

```rust
                if let Some(a) = args("duckdb_columns") {
                    return Self::Columns(a.iter().map(|x| number(x)).collect());
                }
```
  the panic message's accepted set gaining `duckdb_columns(<positions>)`, the comparator's arm
  (`DuckdbOracle::Columns(keep) => same_multiset(&orows, &drows, keep, None),` beside `Exact`'s),
  a parse case `DuckdbOracle::parse("duckdb_columns (0, 2)") == DuckdbOracle::Columns(vec![0, 2])`,
  and a comparator case:

```rust
#[test] fn columns_compares_only_the_named_positions() {
    let other_b = DUCK.replace("| 1 | x   |", "| 1 | y   |");
    assert!(compare_sections(&DuckdbOracle::Columns(vec![0]), OURS, &other_b, &KINDS, &open).is_ok());
    assert!(compare_sections(&DuckdbOracle::Columns(vec![1]), OURS, &other_b, &KINDS, &open).is_err());
}
```
  The line says `duckdb_columns(<the ORDER BY positions>)`; the commit message names the query and the tie.
- [x] **Step 3: Run** `cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus` —
  all green, `every_duckdb_oracle_is_named_by_some_line` included (or the unused variant deleted,
  Task 5 Step 2). The `duckdb_gpu_*` cases pass vacuously until Task 7 writes `gpu-result.txt`.
- [~] **Step 4: Commit** Task 5's code with these line changes, green:
  `git commit -m "#235: a DuckDB case per corpus line, cpu and device, and every line's oracle"`.

### Task 7: `gpu-result.txt`

**Files:**
- Modify: `peacockdb-core/src/test_support/corpus_gpu.rs` (`gpu_case`, **before** `assert_result`)
- Modify: `peacockdb-core/src/test_support/corpus_golden.rs` (`gpu_result_golden(dataset, sf)`)
- Modify: `peacockdb-core/tests/test_gpu_corpus.rs` (`a_device_run_under_a_regeneration_writes_no_golden`)
- Modify: `scripts/build-test-shadgpu.sh` (forward `PCK_WRITE_GPU_RESULT`; `--pull-results`)

**Interfaces:**
- Produces: `testdata/goldens/<dataset>.sf1/gpu-result.txt`, sections as `mini.result.txt`'s
  (`mode=<name>` then the table, or the fingerprint), written only when `PCK_WRITE_GPU_RESULT=1`.

- [x] **Step 1:** In `gpu_case`, **before** `assert_result` runs — so a device answer the cpu
  rejects is still recorded, which is exactly where DuckDB says which engine is right (#243) —
  when `std::env::var("PCK_WRITE_GPU_RESULT").as_deref() == Ok("1")`,
  render the device's batches as `assert_result_section` does (fingerprint over the cap) and
  write it into `gpu_result_golden(dataset, sf)` as the section **keyed by query and mode**,
  `== <query> mode=<mode>` — every enabled device cell keeps its own, replacing only its own on a
  rerun (`merge_mode_section(path, query, mode, body)`, a sibling of `merge_section` that matches
  both header fields). When `PCK_WRITE_GPU_RESULT` holds a version other than `1` (verify-26.02:
  `PCK_WRITE_GPU_RESULT=26.02`), the target is `gpu-result-26.02.txt` beside it, which
  `testdata/.gitignore` lists. The `duckdb_gpu_*` cases read `gpu-result-<v>.txt` instead when `PCK_GPU_RESULT_VERSION=<v>` is set. Never under `UPDATE_CANONICAL`/`PCK_UPDATE_SECTIONS` alone:
  the existing no-golden test sets those two and must still see the three cpu files unchanged; add
  `gpu-result.txt` to its `files` list only if it exists, and assert it unchanged too (that run does
  not set `PCK_WRITE_GPU_RESULT`).
- [x] **Step 1b: The coverage guard**, in `test_cpu_corpus` (rust-only, so CI runs it):

```rust
#[test]
fn every_enabled_device_cell_has_its_gpu_result_section_and_no_other() {
    for (dataset, sf) in [("tpch", "1"), ("tpcds", "1")] {
        let enabled: BTreeSet<(String, String)> = registry_rows(dataset, sf)
            .flat_map(|r| r.enabled_gpu_modes().map(move |m| (r.query.replace('_', "-"), m.to_string())))
            .collect();
        let written: BTreeSet<(String, String)> = gpu_result_sections(dataset, sf)
            .map(|s| (s.query, s.mode)).collect();
        let missing: Vec<_> = enabled.difference(&written).collect();
        let extra: Vec<_> = written.difference(&enabled).collect();
        assert!(missing.is_empty() && extra.is_empty(),
            "{dataset}: regenerate gpu-result.txt (PCK_WRITE_GPU_RESULT=1, --pull-results): \
             missing {missing:?}, not enabled {extra:?}");
    }
}
```
  (`registry_rows` and `enabled_gpu_modes` read `cost-registry.csv` as the registry tests do;
  `gpu_result_sections` parses the `== <query> mode=<mode>` headers. pbench joins the list in its
  own task.)
- [x] **Step 2:** `build-test-shadgpu.sh`: `PCK_WRITE_GPU_RESULT` joins the forwarded knobs in
  `remote_gate_script` (`:375-385`, beside `PEACOCK_GPU_DEBUG`); `--pull-results` copies
  `testdata/goldens/*/gpu-result.txt` home with the same `pull_one` the benchmark pull uses
  (`:638-690`), refusing while a detached run is going.
- [~] **Step 3: Device cycle** (foreground):
  `PCK_WRITE_GPU_RESULT=1 PCK_TEST_FILTER=gpu_ ./scripts/build-test-shadgpu.sh --all`, then
  `./scripts/build-test-shadgpu.sh --pull-results`. Locally:
  `cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus duckdb_gpu_` — each
  failing case is a device divergence from DuckDB: a ticket, and if the cpu agrees with DuckDB it is
  the device's (the line's oracle stays as Task 6 set it; the device's ticket goes on the registry
  row's gpu cells).
- [x] **Step 3b: `build-test.md`.** In the device section, beside the regeneration variables:
  "**`gpu-result.txt` is regenerated whenever a device answer might move.** After a device code
  change, a cell turned on, or a cuDF update, run a cycle with `PCK_WRITE_GPU_RESULT=1`, then
  `--pull-results`, and read `git diff testdata/goldens/*/gpu-result.txt` before committing it. A
  moved section the change did not intend is a finding, not a regeneration. Float cells move a
  little from run to run (GPU reductions are not reproducible), which is why nothing compares the
  file with its previous version; the DuckDB cases and the coverage test are what CI checks."
- [~] **Step 4: Commit** the pulled `gpu-result.txt` files with the code:
  `git commit -m "#235: the device's answers recorded, and compared with DuckDB"`.

### Task 7b: The helpers fail on a wrong answer (#235)

**Files:**
- Modify: `peacockdb-core/src/test_support/corpus.rs:430` (`assert_result_section`) and a new ungated
  `device_answer_matches`; `corpus_gpu.rs:97` (`assert_result` calls it; `GpuResultMode` and
  `gpu_result_mode` move to `corpus.rs`); their callers
- Test: `peacockdb-core/src/test_support/corpus.rs` (its `#[cfg(test)]` module)

`corpus_gpu` is `#[cfg(not(feature = "rust-only"))]` (`test_support/mod.rs:16-17`), so a test of
the device helper there cannot run in the rust-only tier. The comparison the device helper makes is
pure Rust over batches and a section, so it moves out of the gated module; `assert_result` keeps
the device-only parts (the live-cpu run) and calls it.

- [x] **Step 1: The seam.** In `corpus.rs`, ungated:

```rust
/// The device's answer against a golden section, under a `gpu_oracle` keyword. Pure: no device,
/// no file — `assert_result` (corpus_gpu.rs) reads the section and calls this.
pub(crate) fn device_answer_matches(section: &str, gpu_oracle: &str, batches: &[RecordBatch])
    -> Result<(), String> {
    match gpu_result_mode(gpu_oracle) {
        GpuResultMode::GoldenExact => compare_section(section, batches, None),
        GpuResultMode::GoldenApproxStddev => compare_section(section, batches, Some(STDDEV_TOL)),
        GpuResultMode::LiveCpu => Err("live_cpu compares against a cpu run, not a section".into()),
    }
}
```
  `compare_section` is the body `assert_result` runs today for the golden modes, lifted out and
  returning `Err(message)` where it panicked; `STDDEV_TOL` is the tolerance `golden_approx_std`
  uses now. `assert_result_section` takes its section: `fn assert_result_section(section: &str,
  query: &str, mode: &Mode, batches: &[RecordBatch])`; the corpus cases read the section as the
  helper does today (`golden_section(dataset, sf, query, "mini.result.txt")`) and pass it.
  `assert_result` becomes `panic!` on `device_answer_matches(..)`'s `Err` for the golden modes.
  No behaviour changes: `test_cpu_corpus` green, `test_gpu_corpus` builds (`--no-run`).
- [x] **Step 2: The failing-on-purpose tests**, in `corpus.rs`'s test module (rust-only):

```rust
fn answer() -> Vec<RecordBatch> {
    let s = Arc::new(Schema::new(vec![Field::new("k", DataType::Int32, false),
                                      Field::new("v", DataType::Float64, false)]));
    vec![RecordBatch::try_new(s, vec![Arc::new(Int32Array::from(vec![1, 2])),
                                      Arc::new(Float64Array::from(vec![0.5, 1.25]))]).unwrap()]
}
fn section(rows: &[&str]) -> String {
    let mut s = "+---+------+\n| k | v    |\n+---+------+\n".to_string();
    for r in rows { s.push_str(r); s.push('\n'); }
    s + "+---+------+\n"
}

#[test]
fn the_cpu_helper_accepts_the_right_answer() {
    assert_result_section(&section(&["| 1 | 0.5  |", "| 2 | 1.25 |"]), "q", &MODES[0], &answer());
}
#[test]
#[should_panic(expected = "result")]
fn the_cpu_helper_fails_on_a_wrong_row() {
    assert_result_section(&section(&["| 1 | 0.5  |", "| 3 | 1.25 |"]), "q", &MODES[0], &answer());
}
#[test]
#[should_panic(expected = "result")]
fn the_cpu_helper_fails_on_a_missing_row() {
    assert_result_section(&section(&["| 1 | 0.5  |"]), "q", &MODES[0], &answer());
}
#[test]
fn the_device_comparison_fails_on_a_wrong_row_under_golden_exact() {
    assert!(device_answer_matches(&section(&["| 1 | 0.5  |", "| 3 | 1.25 |"]), "golden_exact", &answer()).is_err());
}
#[test]
fn the_device_comparison_fails_past_the_tolerance_under_golden_approx_std() {
    assert!(device_answer_matches(&section(&["| 1 | 0.5  |", "| 2 | 1.26 |"]), "golden_approx_std", &answer()).is_err());
}
#[test]
fn the_device_comparison_accepts_the_right_answer() {
    assert!(device_answer_matches(&section(&["| 1 | 0.5  |", "| 2 | 1.25 |"]), "golden_exact", &answer()).is_ok());
}
```
  The `expected` text is `assert_result_section`'s failure message prefix; read it off the helper
  and use it exactly.
- [x] **Step 3: Run.** `scripts/cargo-cudf.sh test --features rust-only -p peacockdb-core --lib test_support::corpus`
  → green; then break each comparison by hand (return early) and see each negative test go red —
  record in the detail file.
- [~] **Step 4: Commit.** `git commit -m "#235: the corpus helpers take their section, and are shown failing on a wrong answer"`.

### Task 7c: Every oracle enum is held to its lines (#235)

**Files:**
- Modify: `peacockdb-core/src/test_support/corpus.rs` (`CpuOracle` at `:470`; `GpuResultMode`, moved here by Task 7b)
- Test: `peacockdb-core/src/test_support/corpus.rs` (its test module)

The test lives in the library, where `CpuOracle` and `GpuResultMode` are visible (both `pub(crate)`)
and nothing is gated; it reads the corpus lines from `tests/common/corpus_cases.inc` as text, the
file both corpus binaries include.

- [x] **Step 1: Delete the unused variants.** `GpuResultMode::Skip` and `GpuResultMode::GoldenApprox`
  and their keywords `skip`, `golden_approx` go, with their match arms; the panic text lists
  `golden_exact|golden_approx_std|live_cpu`.
- [x] **Step 2: The consts and the test.**

```rust
impl CpuOracle {
    pub(crate) const ALL: [CpuOracle; 3] =
        [CpuOracle::DataFusionExact, CpuOracle::DataFusionApproximate, CpuOracle::DataFusionSubset];
}
impl GpuResultMode {
    pub(crate) const ALL: [GpuResultMode; 3] =
        [GpuResultMode::GoldenExact, GpuResultMode::GoldenApproxStddev, GpuResultMode::LiveCpu];
}

/// The (cpu_oracle, gpu_oracle) keywords of every live `corpus_query!` line: the 7th and 8th
/// arguments, after `duckdb_oracle`.
fn declared_oracles() -> Vec<(String, String)> {
    let inc = std::fs::read_to_string(
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/common/corpus_cases.inc")).unwrap();
    inc.lines()
        .map(str::trim_start)
        .filter(|l| l.starts_with("corpus_query!("))
        .map(|l| {
            let args: Vec<&str> = l["corpus_query!(".len()..].split(',').map(str::trim).collect();
            (args[6].to_string(), args[7].to_string())
        })
        .collect()
}

#[test]
fn every_oracle_variant_is_named_by_some_line() {
    let lines = declared_oracles();
    for v in CpuOracle::ALL {
        assert!(lines.iter().any(|(c, _)| cpu_oracle_mode(c) == v), "{v:?} is unused: delete it");
    }
    for v in GpuResultMode::ALL {
        assert!(lines.iter().any(|(_, g)| gpu_result_mode(g) == v), "{v:?} is unused: delete it");
    }
}
```
  (`GpuResultMode` gains `PartialEq, Eq, Debug` derives. A commented-out line — `// corpus_query!(`
  — is skipped by the `starts_with`.) Together with Task 1's `DuckdbOracle::ALL` test, the three
  enums are each held to the lines.
- [x] **Step 3: Run.** `scripts/cargo-cudf.sh test --features rust-only -p peacockdb-core --lib every_oracle_variant` green;
  `test_gpu_corpus` builds (`--no-run` locally).
- [~] **Step 4: Commit.** `git commit -m "#235: every oracle enum lists its variants, and an unused one is deleted"`.

### Task 7d: `build-test.md`'s goldens table and diagram

**Files:**
- Modify: `llm-wiki/build-test.md` (the goldens table, `:642-652`, and the diagram under it, `:656-697`)

- [x] **Step 1: The table.** `duckdb-result.txt`'s "Asserted by" becomes "`test_cpu_corpus`'s
  `duckdb_<ds>_<q>` cases, per the line's `duckdb_oracle`"; its "Produced by" notes timestamps
  rendered as arrow-rs does and the fingerprint over the cap. A new row:
  `gpu-result.txt` — produced by the device corpus tier under `PCK_WRITE_GPU_RESULT=1`, before it
  asserts, one section per (query, mode), fingerprint over the cap, pulled home with
  `--pull-results` (`=<v>` writes `gpu-result-<v>.txt`, gitignored) — asserted by the
  `duckdb_gpu_<ds>_<q>_<mode>` cases and the coverage test. `<tier>.result.txt`'s row: an over-cap
  section is a fingerprint, written and not asserted (the device reads it only as `live_cpu`).
  The device oracles listed are `golden_exact`, `golden_approx_std`, `live_cpu` (Task 7c deleted
  `skip` and `golden_approx`).
- [x] **Step 2: The diagram.** Replace the corpus-tier and `duckdb_result.py` branches with:

```
          ├── the corpus cpu tier, UPDATE_CANONICAL=1   (the author of the cpu goldens)
          │     ├──► <mode>-<tier>.cpu.txt        (a == <query> section each)
          │     │       └──× cost_model.conf ──► <mode>-<tier>.cost.txt
          │     └──► <tier>.result.txt   (one section per query, from its last mode; over 256 KB a
          │               │               fingerprint, written and never asserted)
          │               ▼  read-only, never written
          │     the corpus device tier (test_gpu_corpus, shad-gpu)
          │       ├── <mode>-<tier>.cpu.txt, .cost.txt ──► every device cell, per node
          │       ├── <tier>.result.txt ──► the device's answer, per gpu_oracle:
          │       │     golden_exact            text for text, one section serving all modes
          │       │     golden_approx_std       within 1e-11
          │       │     live_cpu                a cpu run at the same mode (every over-cap line)
          │       └──► gpu-result.txt   only under PCK_WRITE_GPU_RESULT=1, before asserting:
          │                              one section per (query, mode), a fingerprint over the cap;
          │                              --pull-results brings it home; a record, never an authority
          │                              (=<v>: gpu-result-<v>.txt beside it, gitignored)
          │
          ├── duckdb_result.py  (DuckDB 1.5.4, threads=1; timestamps as arrow-rs renders them)
          │     └──► duckdb-result.txt   (one section per query; a fingerprint over the cap)
          │
          │   test_cpu_corpus, rust-only (CI), per the line's duckdb_oracle
          │   (exact | approx | divergent(t, cols) | fingerprint | none):
          │     duckdb_<ds>_<q>             <tier>.result.txt ↔ duckdb-result.txt
          │     duckdb_gpu_<ds>_<q>_<mode>  gpu-result.txt   ↔ duckdb-result.txt
          │     coverage                    every enabled device cell has its gpu-result section, no other
```
  and, beside the table, Task 7's paragraph on when to regenerate `gpu-result.txt` (a device change,
  a cell turned on, a cuDF update: `PCK_WRITE_GPU_RESULT=1`, `--pull-results`, read the diff).
- [x] **Step 3: Check.** The diagram's names match the code: the case prefixes `duckdb_` and
  `duckdb_gpu_`, the variable `PCK_WRITE_GPU_RESULT`, the flag `--pull-results`, the five oracle
  keywords; `test_ci_coverage`'s wiki checks (if any read `build-test.md`) green.
- [~] **Step 4: Commit.** `git commit -m "build-test.md: the DuckDB oracle and gpu-result.txt in the goldens table and diagram"`.

### Task 8: The record

- [x] `build-test.md`: the DuckDB tier — `duckdb_*` and `duckdb_gpu_*` in `test_cpu_corpus`'s count
  (twice the line count, plus the `ALL` and sugar tests), the `--lib` count (Tasks 1, 4), the
  `test_golden_format` count (Task 3); the run table's `PCK_WRITE_GPU_RESULT` and `--pull-results`.
- [~] `corpus-coverage.md` #235: **not archived.** The device cycle has not run, so its 26
  `duckdb_gpu_*` cases and the coverage test are red and the ticket still names something not
  done. Its text records what landed and that one `PCK_WRITE_GPU_RESULT=1` cycle closes it;
  archive it in the round that runs the cycle, with the `tickets.md` row and open counts.
  #251 (the decimal quotient, Task 6) was filed and indexed instead.
- [~] `git commit -m "duckdb-oracle: the tier recorded; #235 archived"`.
