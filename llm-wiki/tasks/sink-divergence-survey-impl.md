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

- [ ] **Step 1: Read the site**

```bash
sed -n '165,190p' peacockdb-core/src/executor/gpu_backend/mod.rs
```

`decode` returns `Vec<RecordBatch>`; `concat_batches(&self.schema, …)` then fails with a message that
keeps only arrow's own text.

- [ ] **Step 2: Compare before concatenating**

```rust
        let batches = decoded?;
        // The survey's whole point: concat_batches reports that the schemas differ and
        // not how, so sixty disabled cells fail with sixty identical lines. Read the
        // device's own schema off the decoded batches and say what differs, per column.
        //
        // Every diverging column, not the first: a sink carrying a string and a narrow
        // decimal is two findings, and reporting one is how a rollout concludes that
        // fixing the string was enough.
        if let Some(first) = batches.first() {
            let exported = first.schema();
            let declared = &self.schema;
            let mut differ: Vec<String> = Vec::new();
            if exported.fields().len() != declared.fields().len() {
                differ.push(format!(
                    "column count: declared {} and exported {}",
                    declared.fields().len(),
                    exported.fields().len()
                ));
            }
            for (at, (d, e)) in declared
                .fields()
                .iter()
                .zip(exported.fields().iter())
                .enumerate()
            {
                if d.data_type() != e.data_type() {
                    differ.push(format!(
                        "column {at} {:?}: declared {} and exported {}",
                        d.name(),
                        d.data_type(),
                        e.data_type()
                    ));
                } else if d.is_nullable() != e.is_nullable() {
                    // Reported separately: a nullability difference is not a type
                    // difference, and the export derives the flag from the data rather
                    // than from a declaration. A survey that merged the two would file a
                    // batch's contents as a type divergence.
                    differ.push(format!(
                        "column {at} {:?}: declared nullable={} and exported nullable={}",
                        d.name(),
                        d.is_nullable(),
                        e.is_nullable()
                    ));
                }
            }
            if !differ.is_empty() {
                // The original sentence is kept as the prefix: every ticket quotes it and
                // a rollout greps for it.
                return Err(BackendError::new(format!(
                    "the exported stream is not the sink's rows: {}",
                    differ.join("; ")
                )));
            }
        }
        let batch = concat_batches(&self.schema, batches.iter()).map_err(|error| {
            BackendError::new(format!(
                "the exported stream is not the sink's rows: {error}"
            ))
        })?;
```

`zip` truncates on a width mismatch, which is why the count is checked first and reported in the same
list — the loop then says what it can about the columns both sides have.

The `concat_batches` arm stays. It catches whatever this comparison does not, and leaving it is what
keeps the change to one site instead of two.

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
