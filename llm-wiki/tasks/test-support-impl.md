# The corpus harness moves behind the feature — implementation plan

> **For agentic workers:** the coordinator dispatches one task per developer. Steps use
> checkbox (`- [ ]`) syntax. A task ends by appending its state to
> `llm-wiki/tasks/test-support-detail.md` and handing back — it does **not** commit.

**Goal:** move `corpus.rs` and `corpus_gpu.rs` into `src/test_support/` so the two corpus binaries
reach the harness through signatures that carry no engine type, and the last eight `pub` items
stop being forced.

**Architecture:** the feature and the module already exist — task 4 built them for helpers with
two audiences. This task adds the last two files, gives the binaries three functions to call, and
adds the guard that keeps the facade from becoming a rename.

**Tech Stack:** Rust 2024, the `test-support` feature and its self dev-dependency,
`test_module_layout.rs`, `scripts/visibility-dump.py`.

**Spec:** [`test-support.md`](test-support.md) — read it before Task 1.

## Global Constraints

- **You do not mutate git state.** Leave work in the tree; the coordinator commits.
- **No test case moves and no golden is touched.** Both are invariants, not outcomes: check them
  at the end of every task.
- **Never run cudf-feature cargo builds in `./target`.** `rust-only` is plain `cargo`; the other
  two shapes go through `scripts/cargo-cudf.sh` with `CUDF_ROOT`.
- **Bare `pub` only in `test_support/mod.rs`.** Everything below it is `pub(crate)`, or the layout
  test fails and task 6's `unreachable_pub` would fire on every one.
- **rustfmt only the files you touched.** Comment caps: four lines in a body, ten above a
  declaration.
- `CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2` on this host.

---

### Task 1: Baselines

**Files:**
- Create: `llm-wiki/tasks/test-support-baselines/{inv-*.txt,goldens.sha256,visibility.txt}`

**Interfaces:**
- Consumes: task 4's end state.
- Produces: the leaf-name sets and the golden digest every later step compares against.

- [ ] **Step 1: Confirm task 4 finished**

```bash
ls peacockdb-core/src/test_support/
grep -n 'test-support' peacockdb-core/Cargo.toml
```

Expected: `mod.rs`, `testdata.rs`, `mode.rs`, `memory_limit.rs`, the golden-text reader and the
registry loader; the feature and the self dev-dependency. If any is missing, say so rather than
creating it — that is task 4's work and this plan assumes it.

- [ ] **Step 2: Take the baselines**

```bash
scripts/case-inventory.sh rust-only > llm-wiki/tasks/test-support-baselines/inv-rust-only.txt
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 \
  scripts/case-inventory.sh cudf > llm-wiki/tasks/test-support-baselines/inv-cudf.txt
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 \
  scripts/case-inventory.sh gpu > llm-wiki/tasks/test-support-baselines/inv-gpu.txt
scripts/visibility-dump.py > llm-wiki/tasks/test-support-baselines/visibility.txt
find testdata/goldens -type f -print0 | LC_ALL=C sort -z | xargs -0 sha256sum \
  > llm-wiki/tasks/test-support-baselines/goldens.sha256
```

- [ ] **Step 3: Record the count this task must not change**

Run: `scripts/visibility-dump.py | awk '$2=="pub" && $3!="mod"' | wc -l`
Expected: around 174. This task moves the *reason* eight of them are `pub`, not the count — task 6
takes the count down. Record it so task 6 starts from a measured figure.

- [ ] **Step 4: Append and hand back**

---

### Task 2: The corpus harness joins the module, with the rule that keeps it a facade

`src/test_support/` and the `test-support` feature already exist — task 4 built them for the
helpers that had two audiences. This task adds the last two files and the guard.

**Files:**
- Create: `peacockdb-core/src/test_support/{corpus.rs,corpus_gpu.rs}`
- Delete: `peacockdb-core/tests/common/{corpus.rs,corpus_gpu.rs}`
- Modify: `peacockdb-core/src/test_support/mod.rs` (declare the two, add their API),
  `peacockdb-core/tests/{test_cpu_corpus.rs,test_gpu_corpus.rs,test_corpus_goldens.rs}`,
  `peacockdb-core/tests/common/mod.rs`, `peacockdb-core/tests/test_module_layout.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: `test_support::{cpu_case, gpu_case, authoritative_mode, over_cap, Mode, MODES,
  MemoryLimit, TIER, BUDGET}` — the only names the two corpus binaries may use.

- [ ] **Step 1: Confirm the module and the feature are already there**

```bash
grep -n 'test-support' peacockdb-core/Cargo.toml
ls peacockdb-core/src/test_support/
```

Expected: the feature, the self dev-dependency, and `mod.rs` with `testdata.rs`, `mode.rs`,
`memory_limit.rs`, the golden-text reader and the registry loader. If any is missing, task 4 did
not finish; say so rather than declaring it here.

- [ ] **Step 2: Move the two files, 698 lines**

`corpus.rs` (508) and `corpus_gpu.rs` (190). Inside the crate they reach `pub(crate)` items, which
is what stops the eight from needing `pub`. Add their API to the existing `mod.rs`; the two are
private `mod` with `pub(crate)` items like the rest.

- [ ] **Step 3: Rewire the three consumers**

`test_cpu_corpus.rs`, `test_gpu_corpus.rs` and `test_corpus_goldens.rs` call
`peacockdb_core::test_support::…` and must name none of the eight directly. Verify:

```bash
git grep -n 'GpuNode\|RunReport\|GpuBackend\|GpuContext\|RecipePlan\|attach_recipes\|render_run\|validate' \
  -- peacockdb-core/tests/test_cpu_corpus.rs peacockdb-core/tests/test_gpu_corpus.rs
```

Expected: no hits. A hit means the facade is a rename.

- [ ] **Step 4: Add the signature rule to the layout test**

Every `pub` in `test_support/mod.rs` takes and returns strings, `Mode`, `MemoryLimit` or nothing.
Scan that one file and assert its parameter and return types come from that set.

- [ ] **Step 5: Watch it go red**

Add `pub fn tree() -> Box<dyn crate::plan::GpuNode> { unimplemented!() }` to `test_support/mod.rs`.
Run: `cargo test --features rust-only -p peacockdb-core --test test_module_layout`
Expected: FAIL naming `tree`. Revert. This is the guard the whole task rests on.

- [ ] **Step 6: Run both corpus binaries and check `inventory` still sees two**

```bash
cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus
```

Each binary keeps its own registry assertion covering only its own engine's columns. Both must
pass — that is the property the "keep them external" decision exists to protect.

- [ ] **Step 7: Inventories, goldens, warning count, append, hand back**

---

---

### Task 3: Prove it, and hand back for the completeness pass

**Files:** none — this task runs things.

- [ ] **Step 1: The three lib shapes and every binary still build**

```bash
cargo test --features rust-only -p peacockdb-core --no-run
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 scripts/cargo-cudf.sh test -p peacockdb-core --no-run
```

- [ ] **Step 2: Compare the inventories**

```bash
scripts/compare-inventory.sh rust-only llm-wiki/tasks/test-support-baselines/inv-rust-only.txt /tmp/inv.txt
```

Expected: no difference. Nothing moves tiers here; a count that shifts means a test followed the
harness by accident.

- [ ] **Step 3: Goldens byte-identical**

```bash
find testdata/goldens -type f -print0 | LC_ALL=C sort -z | xargs -0 sha256sum \
  | diff - llm-wiki/tasks/test-support-baselines/goldens.sha256
```

Expected: no output. A diff means the harness changed behaviour while moving, which is the failure
mode of moving 698 lines of test code.

- [ ] **Step 4: The count is unchanged**

Run: `scripts/visibility-dump.py | awk '$2=="pub" && $3!="mod"' | wc -l`
Expected: Task 1's figure. The eight are no longer *forced*, but they are still `pub` — demoting
them is task 6's, and doing it here would put a 300-hunk diff in a branch a reviewer is reading
line by line.

- [ ] **Step 5: Append and hand back**
