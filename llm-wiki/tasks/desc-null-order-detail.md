# desc-null-order — detail

## Run log

- 2026-10-21 23:01Z: branch `ENS-desc-null-order` forked off `ENS-oracle-columns` 0441b83a.
  Developer round 1 dispatched. Verda not reachable (no credentials): CPU tests run locally.
  nebius-gpu started by the coordinator; sync with `gpu-host-sync.sh --dest peacockdb-K`.

## Round 1 (developer)

### The PR comment (Task 1, #258)

- Before: `64808 bytes of 65536, 728 spare — about 2 more queries at 301 bytes a row`.
- After Task 1, before the six rows: `37343 bytes of 65536, 28193 spare — about 162 more
  queries at 173 bytes a row`. That per-row figure divided by every registry row, pbench's
  included, which the comment no longer renders, so it overstated the rows that fit. Review
  round 1 (nit 5) divides by the tabled datasets' rows only; see "Developer, review round 1".
- **Plan gap found and closed.** `scripts/ticket_widget.py` builds `testdata/report.html` from
  cost-report's `--md` comment, not from `render_html`. Shrinking the comment alone took the
  Features column and all of pbench off the checked-in page (`report.html` lost 256 lines on the
  first regeneration). The spec's Restriction forbids that. Fix: `render_markdown` takes a
  `Markdown::{Comment, Whole}` shape; `--widget-md FILE` writes the `Whole` tables (Features and
  every pbench row, uncapped), and `ticket_widget.py` asks for `--widget-md` instead of `--md`.
  The comment and the Pages html are unchanged by that. Test:
  `the_widget_tables_keep_features_and_every_pbench_row`.

### The six pbench queries (Task 2)

- DuckDB 1.5.4 over `fact WHERE f_id % 20 = 0`: 1,000 rows; nulls `f_k` 54, `f_s` 53, `f_kb` 59.
  `sort-desc-limit` keeps `LIMIT 100`: 100 > 54, so the cut falls past the nulls.
- `f_kb` under that filter is never `false`: 941 `true`, 59 null (every kept `f_id` is even).
  `sort-mixed-keys` still has nulls in two keys and three directions, but its first key has no
  false/true boundary, only the true/null one.
- This host has no `ensurepip`, so `python3 -m venv` fails; the system `python3` already carries
  `duckdb` 1.5.4 and ran `testdata/duckdb_result.py` directly.
- Every new `mini.result.txt` section holds rows (1,000, or 100 for the limit), none
  fingerprinted. `sort-desc-nulls-first` opens with its null `f_k` rows in `f_id` order and
  `sort-desc-nulls-last` ends with them.

### The device before the fix (red build, 2026-10-21, nebius-gpu `~/peacockdb-K`)

The tree with the fix reversed (`git diff -R` of `sort.cpp`, `node_session.cpp`,
`operators.h`), the tests and corpus edits in place.

- `peacockdb_core_gpu_lib descending_key`: 3 passed, 7 failed. Failing: the five renamed tests
  and the two new `…_nulls_last_agrees` cases, each `cpu and gpu differ: … rows differ`. Passing:
  `a_merge_on_a_descending_key_agrees`, `an_accumulating_sort_on_a_descending_key_agrees`,
  `a_fetch_with_a_descending_key_keeps_the_same_top_n` (keys with no nulls).
- `test_gpu_corpus tpcds_q34_ tpcds_q71_ sort_desc_ sort_mixed_keys`: 0 passed, 40 failed. Every
  cell fails at row 0 of `golden(no_sort, exact)` against the cpu's section; none passes, so no
  query stays in as coverage-only. The first difference, the same at all five modes:

  | query | golden row 0 | device row 0 |
  |---|---|---|
  | `sort-desc-nulls-first`, `sort-desc-default`, `sort-desc-limit` | `f_id` 60 (null `f_k`) | 13940 (`f_k` 998): nulls last |
  | `sort-desc-nulls-last` | 13940 | 60: nulls first |
  | `sort-desc-string` | 1260 (`f_s` `v96`) | 0 (null `f_s`): nulls first |
  | `sort-mixed-keys` | 19640 (`true`, null `f_k`) | 14940 (`true`, `f_k` 995): the second key's nulls last |
  | tpcds q34 | column 3 null | `Y` |
  | tpcds q71 | column 0 `5003001` | `6008005` |

  `sort-mixed-keys` fails on its second key only: its first, ascending, was already right.
- The size test on the final tree (six rows in, `202` struck from eight rows): `37317 bytes of
  65536, 28219 spare — about 167 more queries at 168 bytes a row`. The per-row figure is
  pbench-diluted; corrected in review round 1 to `about 108 more queries at 260 bytes a row`.

### The device after the fix (green build)

- `peacockdb_core_gpu_lib`: `descending_key` 10 passed; `gpu_tests::accumulate_cases` 39 passed;
  `gpu_tests::exec_cases` 59 passed.
- `PCK_WRITE_GPU_RESULT=1 test_gpu_corpus tpcds_q34_ tpcds_q71_ sort_desc_ sort_mixed_keys`: 40
  passed. The rsync brought home ten tpcds sections and thirty pbench ones, nothing removed.
- `git grep -n null_order cpp/src`: `null_order_of`'s comment and body in `sort.cpp`, its
  declaration in `operators.h`, the two callers. No third site.

### Final tree

- Local, rust-only, `--test-threads=2`: `--lib` 895 passed (2 ignored); `test_cpu_corpus` 2399;
  `test_corpus_goldens` 28; `test_cost_model` 3. `cargo test -p cost-report`: 47 + 32.
  Python, as the cost-report job runs them: all pass (`test_plot.py` needs matplotlib, which this
  host lacks; run with it installed to a scratch `--target` dir on `PYTHONPATH`).
- The L40S list on nebius-gpu, `~/peacockdb-K`, synced from the final tree: `ALL rc=0`.
  `peacock_cpu_tests` 26, `peacock_gpu_tests` 2, `peacock_join_session_tests` 56,
  `peacock_plan_tests` 126, `peacock_gpu_benchmarks` 8 (3 `bench_` skipped),
  `peacockdb_core_gpu_lib` 743, `test_gpu_corpus` 989, `test_node_timing` 1. VM soft-stopped.
- Not done here: the dispatched `GPU Tests (nebius-gpu)` CI job (the coordinator's), and the
  commits. sf40, benchmarks and Nsight are out for this task.

### Commit split, as the plan orders it

1. `cost-report: the PR comment drops Features and collapses pbench (#258)`:
   `cost-report/src/main.rs`, `scripts/ticket_widget.py`.
2. `corpus: pbench sorts on a nullable key, on the cpu`: the six `sort-*.sql`, the pbench cpu
   goldens, `duckdb-result.txt`. The `corpus_cases.inc` and registry rows in this commit are the
   Task-2 shape (device `none`, ticket 202), which the working tree no longer holds; committing
   the final tree as one commit is the simpler path.
3. `sort: a descending key's null order is relative to its direction (#202)`: the three C++
   files, both `gpu_tests` files, `corpus_cases.inc`, the registry, both `gpu-result.txt`,
   `report.html`, `architecture.md`.
4. `wiki: #202 and #258 archived; pbench sorts on a nullable key`: the ticket files, the
   archive, `tickets.md`, `build-test.md`, this file.
- 2026-10-21T23:38:57Z: developer round 1 green (CPU local; L40S ALL rc=0; 40 device cells recorded). Deviation: `--widget-md` so report.html stays whole (spec Restriction). Committed as one commit; PR #191 → ENS-oracle-columns. Reviewer round 1 dispatched.

## Review round 1 (0 blocking, 2 important, 3 nits)

- important 1: `cost-report/src/main.rs:2321` `the_comment_carries_no_features_column` checks only
  the word `Features`; a Comment arm printing feature cells without the header stays green. Fix:
  assert a feature-only code (e.g. `corr_subquery`) is absent, or every `<tr>`'s cell count
  (colspan counted) equals its header's `<th>` count.
- important 2: nothing guards `testdata/report.html` keeping Features and every pbench row;
  reverting `ticket_widget.py:608` to `--md`, or running `ticket_widget.py --md` per its old usage
  line, strips them with every test green. Fix: a test over the checked-in `report.html` — one
  `<sub>Features</sub>` header per dataset and a `<tr id="pbench-<stem>">` per pbench registry row;
  rename the widget's own `--md` flag (`ticket_widget.py:581`) to remove the trap.
- nit 3: `main.rs:1153-1156` comment still says the comment and html carry the same table.
- nit 4: `main.rs:2350` `html.contains(&r.query)` — `nl_left`, `nl_right`, `exists_null_keys`
  are substrings of other names; match `>{}<`.
- nit 5: `main.rs:2288` `per_row` divides by every registry row including pbench, which the
  comment no longer renders; divide by the tabled datasets' rows and correct the detail note.
- Taken to the developer: all five.

## Developer, review round 1

- important 1: `the_comment_carries_no_features_column` also counts every comment row's cells,
  colspan counted, against its header's `<th>`s. Red: the Comment arm printing
  `<td><sub>{features}</sub></td>` failed it (`a row with 9 cells under 8 headers`) while its
  old word check, still its first line, passed.
- important 2: `the_checked_in_report_keeps_features_and_every_pbench_row` reads
  `testdata/report.html`: one `<sub>Features</sub></th>` per dataset, and
  `<tr id="pbench-<stem>">` per pbench registry row. Red: the page regenerated the old way
  (`cost-report --md comment.md`, then `ticket_widget.py --md comment.md`, 320 → 214 lines,
  0 pbench rows) failed on the Features count; the good page with one pbench row id removed
  failed on that row. The page was restored byte-identical. The widget's flag is now
  `--widget-md`; `test_the_old_md_flag_is_refused_before_the_page_is_written` was red (exit 0)
  before the rename. No caller used the widget's `--md`: the workflow's two `--md` are
  cost-report's own comment flag, unchanged.
- nit 3: the comment above the markdown table says the comment drops Features.
- nit 4: the html page test matches `>{query}<`.
- nit 5: `Markdown::tables` says which datasets a rendering tables; `render_markdown` and the
  size test both use it. The margin now prints `37317 bytes of 65536, 28219 spare — about 108
  more queries at 260 bytes a row`.
- Also removed: the test helper `sha_links`, unused since 3c0750ee (its dead-code warning).
- `testdata/report.html` is unchanged: the widget regenerated to a scratch path is `cmp`-identical.
- 2026-10-21T23:57:51Z: review round 2: 0/0/1 (nit: `Markdown::tables` reads as a noun; dropped). completing: completeness reviewer + analyst dispatched.
