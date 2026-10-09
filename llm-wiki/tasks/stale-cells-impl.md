# stale-cells implementation plan

> **For agentic workers:** the chain coordinator dispatches this plan to a developer, task by
> task; steps use checkbox (`- [ ]`) syntax. Commits at most 10 lines; device cycles foreground.

**Goal:** the 16 device cells four tpch rows keep off under closed tickets (#183, #187) are run;
each is enabled or carries the ticket it actually fails on, and the stale tags come off.

**Architecture:** registry and corpus-line edits only, driven by one shad-gpu cycle filtered to the
four rows. No engine code; a failure is a ticket, not a fix.

**Tech stack:** `corpus_cases.inc`, `cost-registry.csv`, `build-test-shadgpu.sh`, the DuckDB
comparison duckdb-oracle added.

**Spec:** [`stale-cells.md`](stale-cells.md) — committed 9e563348.

## Global constraints

- The 16 cells only: `gpu_tp1_rowgroup`, `gpu_tp4_single`, `gpu_tp4_rowgroup`, `gpu_tp4_sized` on
  `tpch/aggregate_groupby`, `tpch/shuffle_additive`, `tpch/shuffle_stddev`, `tpch/filter_project`
  (`cost-registry.csv:123,126,137,139`; `corpus_cases.inc:46,47,49,73`).
- No fix to anything they show. A failure keeps its cell off under an open ticket, or a new one.
- `shuffle_stddev` keeps `schema_validation_disabled` (#225); a cell failing on schema alone keeps
  `225` on the row.
- A tag is struck only when every off cell left in the row carries another ticket
  (`test_support/registry.rs:229-240`: a row with a disabled cell must name a ticket).
- duckdb-oracle has landed: each line already names its `duckdb_oracle`, and `all_modes` exists.

## Review Focus

1. **A cell that passes the cpu golden but differs from DuckDB on the device** — `gpu-result.txt`'s
   `duckdb_gpu_*` case must be green before the cell counts as enabled. Task 2 Step 3.
2. **`filter_project` is over the cap** — its device oracle is `live_cpu`, so at tp4 the device
   compare runs the cpu live at the same mode; a time-out there is not a failure of the cell.
   Task 1 Step 2 uses the foreground gate, which has no per-case limit.
3. **A cell that fails only at one tp4 mode** (sized vs single) — the row keeps exactly that cell
   off, not all four. Task 2 Step 1.
4. **The registry tests in both binaries** — `the_registry_matches_the_cpu_corpus_in_both_directions`
   and the device's twin must both be green; the device one runs only on shad-gpu. Task 2 Step 4.
5. **The `183` comment at `corpus_cases.inc:56`** — it must not be left describing rows that no
   longer keep `183`. Task 3.

## File structure

| file | responsibility |
|---|---|
| `peacockdb-core/tests/common/corpus_cases.inc:46,47,49,73` | the four lines' `gpu_modes` |
| `testdata/cost-registry.csv:123,126,137,139` | the four rows' gpu cells and tags |
| `testdata/goldens/tpch.sf1/gpu-result.txt` | the device's answers, pulled home |
| `llm-wiki/tickets/*.md`, `tickets.md` | any ticket a failure needs |
| `llm-wiki/build-test.md` | the gpu corpus counts |

---

### Task 1: Run the 16 cells

**Files:**
- Modify: `peacockdb-core/tests/common/corpus_cases.inc:46,47,49,73`
- Modify: `testdata/cost-registry.csv:123,126,137,139`

- [ ] **Step 1: Enable all 16 for the run.** Each line's `gpu_modes` becomes `all_modes`; e.g.

```
corpus_query!(tpch, 1, aggregate_groupby, all_modes, all_modes, <duckdb_oracle as set>, data_fusion_exact, golden_exact, schema_validation_enabled);
```

  and the four registry rows' `gpu_tp1_rowgroup … gpu_tp4_sized` go `disabled` → `enabled` (tags
  untouched for now). `cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus the_registry_matches`
  and `every_device_cell_has_a_cpu_cell_at_the_same_mode` green (the four rows' cpu cells are on at
  all five modes already).
- [ ] **Step 2: The device cycle**, foreground, from a workspace:

```bash
PCK_RUN_CPP=0 PCK_WRITE_GPU_RESULT=1 \
PCK_TEST_FILTER='gpu_tpch_aggregate_groupby_|gpu_tpch_shuffle_additive_|gpu_tpch_shuffle_stddev_|gpu_tpch_filter_project_' \
  ./scripts/build-test-shadgpu.sh --all
./scripts/build-test-shadgpu.sh --pull-results
```

  If `PCK_TEST_FILTER` takes one substring only (`build-test-shadgpu.sh:372-373` forwards it to
  cargo's name filter, which is a substring, not a regex), run four cycles, one per query prefix,
  in one session. Record every case's outcome, with its failure text, in the detail file.
- [ ] **Step 3: DuckDB for the device.** Locally:
  `cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus duckdb_gpu_tpch_`
  — the four rows' `duckdb_gpu_*` cases against the pulled `gpu-result.txt`; record outcomes.
  `gpu-result.txt` keeps one section per query and mode (duckdb-oracle), so each of the 16 cells
  meets DuckDB in its own `duckdb_gpu_*` case. The section is written before the device asserts,
  so a cell the gate fails is still recorded. The coverage test then requires a section for every
  cell left enabled, and none for a cell left off: regenerate after settling the cells.
- [ ] **Step 4: Commit** the pulled `gpu-result.txt` sections with the run record:
  `git commit -m "stale-cells: the 16 cells run, outcomes recorded"`.

### Task 2: Settle each cell

**Files:**
- Modify: `peacockdb-core/tests/common/corpus_cases.inc:46,47,49,73`
- Modify: `testdata/cost-registry.csv:123,126,137,139`
- Modify (if needed): `llm-wiki/tickets/*.md`, `llm-wiki/tickets.md`

- [ ] **Step 1: Per cell.** A cell whose gate case and `duckdb_gpu_*` case are green stays enabled.
  A cell that failed goes back to `disabled` in the registry and out of the line's `gpu_modes`
  (spell the remaining modes; `all_modes` only if all five are on), and the row's `tickets` names
  the ticket it fails on: search `llm-wiki/tickets/*.md` for the failure text's operator and
  message; if none fits, file one in `corpus-coverage.md` with the query, the mode and the failure
  text, the number from `tickets.md`'s counter (bump the counter and the open count).
- [ ] **Step 2: Tags.** Strike `183` from `aggregate_groupby`, `shuffle_additive`, `shuffle_stddev`
  and `187` from `filter_project` where every disabled cell left in the row carries another ticket;
  `shuffle_stddev` keeps `225` if a cell failed on schema alone.
- [ ] **Step 3: Rerun** what changed: the rust-only registry tests and
  `cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus duckdb_` (all green).
- [ ] **Step 4: Device registry test.** One cycle with
  `PCK_RUN_CPP=0 PCK_TEST_FILTER=the_registry_matches_the_gpu_corpus ./scripts/build-test-shadgpu.sh --all`
  — green with the final cells.
- [ ] **Step 5: Commit.** `git commit -m "stale-cells: N cells on, M ticketed; 183/187 struck where nothing needs them"`.

### Task 3: The record

- [ ] `corpus_cases.inc:56`: the comment "A row keeping `183` has modes never run past the string
  class" reworded to the rows that still keep `183` (the join rows join-backend runs), or deleted
  if none of this file's batch keeps it.
- [ ] `build-test.md`: the gpu tier's `test_gpu_corpus` count and the enabled-cell prose that names
  the device column (`:405-418`).
- [ ] Detail file: the 16 outcomes table (query, mode, gate, DuckDB, decision, ticket).
- [ ] `git commit -m "stale-cells: the record"`.
