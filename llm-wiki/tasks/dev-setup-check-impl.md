# Every workflow once, from dev — implementation plan

> **For agentic workers:** the coordinator dispatches one task per developer. Steps use
> checkbox (`- [ ]`) syntax. A task ends by appending its state to
> `llm-wiki/tasks/dev-setup-check-detail.md` and handing back — it does **not** commit.

**Goal:** run every documented build and test workflow from the host `dev` and write down what
each one did, behind a two-line diff that makes CI run.

**Architecture:** there is none to build. One `#[test]` pins a property `parse_node_line`'s
reader already has; one comment line changes a GPU test file; then four workflows run in order
and the detail file records each. A workflow that fails is recorded and left; the next one runs.

**Tech Stack:** cargo (`rust-only` feature), `scripts/build-test.sh`,
`scripts/build-test-shadgpu.sh`, pytest.

**Spec:** [`dev-setup-check.md`](dev-setup-check.md).

## Global Constraints

- **You do not mutate git state.** Leave work in the tree; the coordinator commits.
- **Three files change and no other**: `peacockdb-core/tests/test_golden_format.rs`,
  `cpp/tests/gpu/test_cudf.cpp`, `llm-wiki/build-test.md`. No golden is regenerated. No
  dependency is added.
- **A failing workflow is recorded, not repaired.** Its first failure signature goes in the detail
  file and you move to the next workflow. Do not edit a script, install a package, or change a
  path to make it pass.
- **Every foreground command carries `timeout <seconds>`**, with the bound named below. A
  command that hangs is a finding too: record it as `timed out after N s`.
- **The shad-gpu cycle is three foreground calls** — `--build`, `--push-binaries`,
  `--patch --run` — never one backgrounded chain. A backgrounded cycle dies mid-build with no
  error.
- **Read-only on `~/peacockdb`** on dev. It is the remote-side tree the 26.02 workflow ships into;
  it is not a checkout and nothing here edits it by hand.

## The detail file

Create `llm-wiki/tasks/dev-setup-check-detail.md` in Task 1 and append to it in every task. One
section per workflow, this shape, nothing looser:

```markdown
## <workflow name>

- command: `<exactly what was typed, including the timeout and any env>`
- host: dev | shad-gpu (via dev)
- wall time: <m>m<s>s
- outcome: green | red | timed out after <n> s
- signature: <first failing line, verbatim> — or "none"
- notes: <anything the next person needs; one to three lines>
```

---

### Task 1: The rust-only loop, with the new case

**Files:**
- Modify: `peacockdb-core/tests/test_golden_format.rs` — after
  `a_bare_node_name_is_a_node_line` (line 126), before
  `nothing_else_in_a_golden_reads_as_a_node_line`
- Modify: `llm-wiki/build-test.md:7` and the `Golden text format (Rust)` row at `:36`
- Create: `llm-wiki/tasks/dev-setup-check-detail.md`

**Interfaces:**
- Consumes: `parse_node_line(&str) -> Option<NodeLine>` and `NodeLine::count(&self, &str) ->
  Option<u64>` from `peacockdb-core/tests/common/golden_text.rs`. `count` panics when the field
  is present and not a number — the doc comment at `:26` says so, and no case pins it.
- Produces: the detail file, in the shape above.

- [ ] **Step 1: Write the failing test**

Insert after the closing brace of `a_bare_node_name_is_a_node_line`:

```rust
/// A field that is present and not a number is a renderer defect, not a line of another
/// kind — so `count` panics naming the field, rather than reading it as absent.
#[test]
#[should_panic(expected = "field `output_rows=many` is not a count")]
fn a_field_that_is_not_a_number_is_not_read_as_absent() {
    let node = parse_node_line("  GpuFilter: output_rows=many").expect("a node line");
    let _ = node.count("output_rows");
}
```

- [ ] **Step 2: Run it and see it fail**

The assertion cannot fail as written, so make it: temporarily change the expected string to
`"not a count of anything"`, run, and confirm the harness reports the panic message did not
contain it. Then restore the string above. This is the red step; it proves the case reads the
real panic.

Run: `timeout 900 cargo test --features rust-only -p peacockdb-core --test test_golden_format
a_field_that_is_not_a_number -- --nocapture`
Expected with the wrong string: `panic did not contain expected string`. With the right one:
`test result: ok. 1 passed`.

- [ ] **Step 3: Run the whole target, then the plan goldens**

Run: `timeout 900 cargo test --features rust-only -p peacockdb-core --test test_golden_format`
Expected: `27 passed`.

Run: `timeout 1200 cargo test --features rust-only -p peacockdb-core --test test_plan_goldens`
Expected: `19 passed`. The sf1 parquet is a symlink in `testdata/` on dev, so this needs no
generation step.

- [ ] **Step 4: Move the two counts**

In `llm-wiki/build-test.md:7`, `1569` becomes `1570` and `Rust 1135` becomes `Rust 1136`. In the
`Golden text format (Rust)` row, the trailing `| 26 |` becomes `| 27 |`. Nothing else on either
line.

- [ ] **Step 5: Create the detail file**

Write `llm-wiki/tasks/dev-setup-check-detail.md` with a title line and two sections in the shape
above: `## rust-only: test_golden_format` and `## rust-only: test_plan_goldens`, both with the
commands from Step 3 and their wall times.

---

### Task 2: cost-report and its python tiers

**Files:**
- Modify: `llm-wiki/tasks/dev-setup-check-detail.md` — append three sections

**Interfaces:**
- Consumes: nothing from Task 1 beyond the detail file.
- Produces: three more sections.

- [ ] **Step 1: The report crate**

Run: `timeout 1200 cargo test -p cost-report`
Expected: `37 passed` in the last `test result:` line. This shares `target/` with the rust-only
loop, which is the documented arrangement.

- [ ] **Step 2: The extractor test**

Run: `timeout 300 python3 -m pytest -q testdata/test_duckdb_cost.py`
Expected: `41 passed`.

- [ ] **Step 3: The exec-model prototype**

Run: `timeout 900 python3 -m pytest -q scripts/exec_model/tests/
--ignore=scripts/exec_model/tests/test_tpch_corpus.py -p no:cacheprovider`
Expected: every test passes; the count is what the harness prints. `-p no:cacheprovider` keeps
`.pytest_cache` out of the tree.

- [ ] **Step 4: Record**

Append `## cost-report: cargo`, `## cost-report: test_duckdb_cost.py` and `## cost-report:
exec_model` to the detail file.

Then: `git status --short` must show only the three files the spec names plus the detail file.
Anything else — a `.pytest_cache`, a `__pycache__` outside `.gitignore` — is removed before
handing back.

---

### Task 3: C++ + staged Rust on cudf 26.02, dev as its own remote

**Files:**
- Modify: `llm-wiki/tasks/dev-setup-check-detail.md` — append three sections

**Interfaces:**
- Consumes: `scripts/build-test.sh`. Its remote defaults — `REMOTE_DIR=/home/dmitry/peacockdb`,
  `REMOTE_CUDF_ROOT=/home/dmitry/miniforge3/envs/rapids-26.02` — are what dev has, and
  `/media/data/peacockdb` is a symlink to `~/peacockdb` there. `--host dev` resolves through
  `~/.ssh/config` on dev to dev itself.
- Produces: `cpp/build26/`, `target-cudf-rapids-26.02/` (both ignored) and three sections.

- [ ] **Step 1: Build**

Run: `timeout 5400 scripts/build-test.sh --host dev --local-cudf-root
~/miniforge3/envs/rapids-26.02 --build`
Expected: cmake configures with `Using host cudf: 26.02.01` and `Using cuVS: 26.02.0`, the C++
build and install finish, then the CPU rust test binaries are staged under
`cpp/build26/install/rust-tests/`. This is a cold DataFusion build at opt-3 plus a CUDA compile of
the library; an hour is the bound, not the estimate. If the configure fails naming `cuvs`, that is
the signature — record it and stop this workflow.

- [ ] **Step 2: Push**

Run: `timeout 900 scripts/build-test.sh --host dev --push-binaries`
Expected: rsync of `cpp/build26/install/` plus goldens and the registry into `~/peacockdb` on
dev, over ssh to itself.

- [ ] **Step 3: Run**

Run: `timeout 3600 scripts/build-test.sh --host dev --run`
Expected: `peacock_cpu_tests` and every staged rust binary run on dev and pass. The binaries look
for testdata at the path baked at compile time — this worktree's `testdata/` — which exists on
dev, so the `/media/data/peacockdb` symlink is not what they use here; note in the detail file
which path the run actually read, from the first lines of the log.

- [ ] **Step 4: Record**

Append `## 26.02: build`, `## 26.02: push-binaries`, `## 26.02: run`.

---

### Task 4: The shad-gpu cycle on cudf 25.02

**Files:**
- Modify: `cpp/tests/gpu/test_cudf.cpp:37` — one line
- Modify: `llm-wiki/tasks/dev-setup-check-detail.md` — append three sections

**Interfaces:**
- Consumes: `scripts/build-test-shadgpu.sh` and `scripts/lib/shadgpu-env.sh`, which hardcode
  `CUDF_ROOT=/home/dmitry/data/miniforge3/envs/rapids-cuda-12.2` and `/usr/bin/gcc-12`. On dev
  `~/data/miniforge3` is a symlink to `~/miniforge3`, so the path resolves. `REMOTE=shad-gpu` is
  in dev's `~/.ssh/config` and dev's key is authorised there.
- Produces: `cpp/build/`, `target-cudf-rapids-cuda-12.2/`, `cpp/install/` (all ignored) and three
  sections.

- [ ] **Step 1: The one comment line**

Above the comment block that precedes `TEST(NodeTiming, FloorRestoresTheSwitch)` — that block
starts at line 37 with `// measure_timing_floor_us turns` — insert one line and one blank line:

```cpp
// dev-setup-check: a changed line, so the GPU job compiles a file this branch touched.

```

- [ ] **Step 2: Build**

Run: `PCK_TEST_FILTER=q6 timeout 5400 scripts/build-test-shadgpu.sh --build`
Expected: cmake into `cpp/build` against the 25.02 env with gcc-12, install, then the five GPU
rust binaries staged. Same bound and same caveat as the 26.02 build: a configure failure naming
`cuvs` is a signature, not something to fix.

- [ ] **Step 3: Push**

Run: `timeout 1800 scripts/build-test-shadgpu.sh --push-binaries`
Expected: `cpp/install/` mirrored to `shad-gpu:/home/info/peacockdb`, goldens and registry with
it. The link is flaky; the script retries by itself, and the bound covers that.

- [ ] **Step 4: Patch and run**

Run: `PCK_TEST_FILTER=q6 timeout 5400 scripts/build-test-shadgpu.sh --patch --run`
Expected: binaries patched for the host's glibc; every `peacock_*_tests` runs whole — the sf40
suites included, which is where the time goes; the rust binaries run with the filter, so
`test_gpu_corpus` runs its `q6` cells and the other four report `0 passed` without the script
calling that a fault. A `std::bad_alloc` from `pool_memory_resource` is a neighbour on the card:
record it as the signature, run this step once more, and record that too.

- [ ] **Step 5: Record and hand back**

Append `## 25.02: build`, `## 25.02: push-binaries`, `## 25.02: patch+run`. Then the final
message to the coordinator: the four files touched, the proving command for the new case
(`cargo test --features rust-only -p peacockdb-core --test test_golden_format`), and one line
per workflow saying green, red or timed out.
