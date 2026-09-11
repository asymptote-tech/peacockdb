# Sink divergence survey implementation plan

**Goal:** Make the sink's mismatch message name what diverged, run the corpus, and write
`llm-wiki/reports/sink-divergence.md`.

**Architecture:** One error site in `executor/gpu_backend/mod.rs`. The decoded batches already carry
the device's schema; compare it to the sink's declared schema and report every diverging column
before `concat_batches` is asked for a generic failure.

**Tech stack:** Rust, arrow-rs. Device rollout on `shad-gpu`.

**Spec:** [`sink-divergence-survey.md`](sink-divergence-survey.md) — frozen. **Prototype**: its own
branch, CI may run, **no PR and no reviewer**, and `done` means the branch is pushed.

## Global constraints

- **One site changes.** No cast, no refusal, no registry edit, no ticket closed, no golden
  regenerated. A one-line fix that becomes irresistible is a finding for the report, not a commit.
- The branch is throw-away. Write the report as if the code is being deleted, because it is.
- Commit messages at most 10 lines.

---

### Task 1: The message names every diverging column

**Files:**
- Modify: `peacockdb-core/src/executor/gpu_backend/mod.rs:176-184`

- [ ] **Step 1: Read what the message already says**

```bash
sed -n '165,190p' peacockdb-core/src/executor/gpu_backend/mod.rs
```

`concat_batches` ends in `RecordBatch::try_new`, whose error is *"column types must match schema
types, expected {field_type:?} but found {col_type:?} at column index {i}"*. **Both types are
already there** — the tickets quote them. What is missing is the column name and the columns after
the first.

- [ ] **Step 2: Append the name, and report every column**

```rust
        let batches = decoded?;
        let batch = concat_batches(&self.schema, batches.iter()).map_err(|error| {
            // try_new names both types and the index, and stops at the first mismatch.
            // The name and the rest of the columns are what a rollout needs: a sink with
            // a string and a narrow decimal is two findings, and reporting one is how a
            // survey concludes that fixing the string was enough.
            //
            // Types only. try_new does not check nullability, so that difference never
            // reaches here and never disabled a cell -- reporting it would put a class in
            // the report that does not occur.
            let also: Vec<String> = batches
                .first()
                .map(|first| {
                    self.schema
                        .fields()
                        .iter()
                        .zip(first.schema().fields().iter())
                        .enumerate()
                        .filter(|(_, (d, e))| d.data_type() != e.data_type())
                        .map(|(at, (d, e))| {
                            format!("{at} {:?}: {} vs {}", d.name(), d.data_type(), e.data_type())
                        })
                        .collect()
                })
                .unwrap_or_default();
            BackendError::new(format!(
                "the exported stream is not the sink's rows: {error} (declared vs exported: {})",
                also.join("; ")
            ))
        })?;
```

`zip` truncates on a width mismatch, which is harmless here: `try_new` has already refused on the
count and its message carries it. This only enriches an error that is on its way out.

- [ ] **Step 3: Prove the message on the CPU, before spending a device cycle**

A unit test over `GpuExport`'s comparison with a hand-built declared schema and a hand-built
"exported" batch — no device, no executor. Assert the message names the column, both types, and that
two diverging columns produce two clauses.

If the comparison cannot be reached without an executor pointer, extract it as a free function taking
two `&ArrowSchema` and test that. **That extraction is the only structural change this task may
make.**

- [ ] **Step 4: Commit**

```bash
git add peacockdb-core/src/executor/gpu_backend/mod.rs
git commit -m "the sink says what diverged

concat_batches reports that the schemas differ and not how, so sixty
disabled cells fail with sixty identical lines. Every diverging column now,
with declared and exported type. Throw-away: this branch is a survey."
```

---

### Task 2: The rollout

**Files:** none — this task produces evidence, not a diff.

- [ ] **Step 1: Take the cell list from the registry, not from memory**

```bash
awk -F, 'NR>1 && $NF ~ /183|187|191|163/ {print $1, $3, $NF}' testdata/cost-registry.csv
```

Adjust the ticket set to whatever the registry actually names as a schema cause. **Write the list to
`sink-divergence-survey-detail.md` before running anything**, so a restarted run knows what it was
doing.

- [ ] **Step 2: Run in batches of about five**

```bash
./scripts/build-test-shadgpu.sh --build --push-binaries --patch --run
```

Then per batch, with `PCK_TEST_FILTER` narrowing to the queries. Append each batch's raw failures to
the detail file as they come — not at the end. A survey that loses its evidence to a crashed run has
to buy the device time twice.

- [ ] **Step 3: Collect verbatim, change nothing**

No registry edit. No ticket edit. No cell enabled. A cell whose failure disagrees with its ticket is
a row in the report.

---

### Task 3: The report

**Files:**
- Create: `llm-wiki/reports/sink-divergence.md`
- Modify: `llm-wiki/tasks/sink-divergence-survey.md` — the signoff, appended once

- [ ] **Step 1: The class table, which is what the task is for**

One row per (declared type → exported type), with a query count and the tickets that predict it.
Sorted by count, because the point is to know what is common.

- [ ] **Step 2: The three other questions the spec asks**

Classes no ticket names; cells whose failure disagrees with the ticket they are disabled against; and
what produced no evidence — with, for each, whether it does not occur or cannot reach the sink. Name
them either way: an absent row and an untested row read the same.

- [ ] **Step 3: Say what it means for the tasks waiting on it**

One short section each for `declared-schemas`, `walk-drives-every-plan`, and the `casts` and
`wire-schema` rewrites: what this evidence changes about their scope. That section is the reason the
report exists rather than a log.

- [ ] **Step 4: Commit and push the branch**

No PR. `done` is the push.

---

## Self-review against the spec

- **§1 the message** — Task 1, including nullability kept separate from type so a batch's contents
  are not filed as a type divergence.
- **§2 run and collect** — Task 2, with the no-edit rule repeated at the step where it would be
  broken.
- **§3 the report, four questions** — Task 3 steps 1 and 2.
- **Restriction: one site** — the only permitted structural change is extracting the comparison so it
  can be tested without a device, named in Task 1 step 3.
- **Not covered, deliberately:** the spec's "what the sink cannot see" needs a judgement rather than a
  procedure, so Task 3 step 2 asks for it without prescribing how to decide. It is the one part of
  this task that cannot be mechanical.
