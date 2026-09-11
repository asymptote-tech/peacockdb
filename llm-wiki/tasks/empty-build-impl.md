# Empty build implementation plan

**Goal:** A lane whose build side produced no rows keeps its typed zero-row table where the join
above owes rows, so `Right`, `Full` and `RightAnti` answer instead of refusing.

**Architecture:** One derived field on the driver's index, computed during the tree walk: does this
node's output reach the **build** child of a join whose type owes rows when empty. The scatter's
unconditional drop then reads it. No plan field, no wire change, no new ABI symbol — the C++ already
computes both answers once a zero-row build batch arrives.

**Tech stack:** Rust. Device rollout on `shad-gpu`.

**Spec:** [`empty-build.md`](empty-build.md) — frozen. Read §1 and §2 before Task 1; they are the
argument that no marker is needed, and the plan is worthless without it.

## Global constraints

- **The drop stays the default.** Everything except a build side under a join that owes rows keeps
  being dropped. Removing the drop and relying on something downstream is the 389,331-batch version.
- **No marker on the plan** — no `GpuNode` field, no recipe field, no plan-text attribute, nothing on
  the wire. The index field is derived at index time, not declared by anyone.
- **Not touched:** `operators/join.cpp`, `finish_without_keys` (#173's site, on the probe side), the
  ABI, `empty_build_answers_nothing` itself.
- Commit messages at most 10 lines.

## File structure

| file | responsibility |
|---|---|
| `executor/driver/index.rs` | the derived field and the parent climb that computes it |
| `executor/driver/partitioned.rs` | the conditional drop |
| `executor/gpu_backend/join.rs`, `cpu_backend/join.rs` | whichever of §2's two shapes wins |
| `executor/{cpu,gpu}_backend/accumulate.rs` | two doc comments resting on a false premise |
| `llm-wiki/` | #173's text, #175's corpus reach, `architecture.md:606` and `:856` |

---

### Task 1: The index knows which lanes feed a build side that owes rows

**Files:**
- Modify: `peacockdb-core/src/executor/driver/index.rs:72-100`

**Interfaces:**
- Produces: `IndexedNode::feeds_owing_build: bool`, and `PlanIndex::feeds_owing_build(node) -> bool`.
  Task 2 is its only consumer.

- [ ] **Step 1: Confirm the three facts the climb rests on**

```bash
grep -n 'PROBE_CHILD\|parent\|children' peacockdb-core/src/executor/driver/index.rs | head
grep -n 'build side is its first child' -B2 -A2 peacockdb-core/src/executor/driver/index/tests.rs
```

Expect `IndexedNode` to carry `parent: Option<usize>` and `children: Vec<usize>`, `PROBE_CHILD` to be
1, and the test to record that the build side is child zero. **If any of the three has moved, stop**
— §2's argument depends on all three and the spec would need rewriting, not the plan.

- [ ] **Step 2: Write the failing test**

```rust
/// A scatter whose lane feeds the build child of a Right join is kept; one feeding a
/// probe side, or an Inner join, is not. The probe case is the one that matters: keeping
/// empties there adds a probe call per empty lane and the second is refused (#152), so
/// this task would cause the hazard it was written to avoid.
#[test]
fn the_index_marks_only_lanes_feeding_a_build_side_that_owes_rows() {
    // Three hand-built trees, using the fixtures index/tests.rs already has:
    //   scatter -> coalesce -> Right join, build child     => true
    //   scatter -> coalesce -> Right join, probe child     => false
    //   scatter -> coalesce -> Inner join, build child     => false
    // The middle node is deliberately not the join: the climb is a walk, not a parent
    // lookup, and a test with the join directly above would pass on a one-step version.
}
```

- [ ] **Step 3: Run it and watch it fail**

```bash
cargo test --features rust-only -p peacockdb-core feeds_owing_build
```

Expected: the method does not exist.

- [ ] **Step 4: Add the field and compute it in the walk**

```rust
    /// Whether this node's output reaches the build child of a join whose type owes rows
    /// when its build side is empty. Derived from the tree, once, here — a walk per
    /// emitted batch would put a tree climb in the hot path to answer a question whose
    /// answer cannot change.
    ///
    /// Three conditions and not one: the lane may feed no join; it may feed one through
    /// intermediate nodes, so this is a climb rather than a parent lookup; and it may feed
    /// the PROBE side, where keeping empty batches adds a probe call per empty lane and
    /// the second is refused (#152).
    feeds_owing_build: bool,
```

Compute after the walk has filled `parent` and `children`, since the climb needs both:

```rust
fn feeds_owing_build(nodes: &[IndexedNode<'_>], from: usize) -> bool {
    let mut child = from;
    let mut at = nodes[from].parent;
    while let Some(node) = at {
        if nodes[node].category == ExecutorCategory::Join {
            // The build side is child zero, which is what makes BUILD_SLOT zero.
            // Anything else under a join is the probe side and keeps dropping.
            if nodes[node].children.first() != Some(&child) {
                return false;
            }
            return match join_type_of(nodes[node].node) {
                Some(join_type) => !empty_build_answers_nothing(join_type),
                // Cross and nested-loop joins carry no type and owe nothing, which is
                // the same answer `without_build` reaches by a different route.
                None => false,
            };
        }
        child = node;
        at = nodes[node].parent;
    }
    false
}
```

- [ ] **Step 5: Run green, then run the whole rust-only suite**

```bash
cargo test --features rust-only -p peacockdb-core
```

- [ ] **Step 6: Commit**

```bash
git add peacockdb-core/src/executor/driver/index.rs
git commit -m "the index knows which lanes feed a build side that owes rows

A climb over parent and children, computed once during the walk. Three
conditions, not one -- and the probe-side case is what stops the next
commit turning every empty lane into a refused second probe call (#152)."
```

---

### Task 2: The drop becomes conditional

**Files:**
- Modify: `peacockdb-core/src/executor/driver/partitioned.rs:379-386`

- [ ] **Step 1: Write the failing test**

An end-to-end driver test over a plan whose scatter feeds a `Right` join's build side with a lane
that receives no rows. Assert the join is asked to `SetBuild` rather than `NoBuild`.

```rust
/// The driver-level claim, asserted where the routing happens rather than through the
/// answer it eventually produces: a test on the rows would pass for the wrong reason if
/// the join happened to produce them some other way.
#[test]
fn an_empty_build_lane_reaches_set_build_rather_than_no_build() { … }
```

- [ ] **Step 2: Watch it fail, then make the drop conditional**

```rust
        for (lane, out) in outputs.into_iter().enumerate() {
            // Empty scatter outputs are dropped so nothing empty traverses a chain because
            // of hash skew -- except where the join above owes rows for an empty build
            // side, which is the one case the drop turns into a refusal (#175). The index
            // worked that out from the tree; this is a lookup, not a walk.
            if out.num_rows() == 0 && !self.index.feeds_owing_build(node) {
                continue;
            }
```

- [ ] **Step 3: Settle §2's open shape and record it**

With the batch kept, `avail.has[BUILD_SLOT]` is true and the lane takes `SetBuild`, so
`without_build`'s error branch may now be unreachable rather than merely unused.

**Establish which, and write it in `empty-build-detail.md`.** If unreachable, say so where the branch
is and leave it — a refusal that cannot be reached is still the right answer for a plan shape this
task did not anticipate. If reachable, it needs the same treatment and a test naming the shape that
reaches it.

- [ ] **Step 4: Commit**

```bash
git add peacockdb-core/src/executor/driver/partitioned.rs
git commit -m "a lane feeding a build side that owes rows keeps its empty table

The scatter already builds a typed zero-row table and the driver threw it
away, so one lane later the join was told its build side was empty and
refused (#175). Kept only where the index says a join owes rows."
```

---

### Task 3: The answers the C++ already computes

**Files:** tests only.

- [ ] **Step 1: Assert both end to end**

```rust
/// `Right` is left_join(probe, build) with left_policy = NULLIFY, which *is* the
/// build-side pad; `RightAnti` is left_anti_join over empty keys, which returns every
/// probe row. Both are in operators/join.cpp:299 already -- these tests are what turn
/// "the C++ already does this" from an argument into a fact.
#[test] fn a_right_join_with_an_empty_build_pads_every_probe_row() { … }
#[test] fn a_right_anti_join_with_an_empty_build_returns_every_probe_row() { … }
```

- [ ] **Step 2: The guard from the other side**

```rust
/// Without this the change is "keep every empty lane", which is 389,331 batches instead
/// of a few hundred.
#[test] fn a_join_type_that_owes_nothing_still_drops_its_empty_lanes() { … }

/// The condition that stops this task *causing* #152. A test written only around the
/// build side never reaches this plan shape.
#[test] fn a_scatter_feeding_a_probe_side_still_drops_its_empties() { … }

/// Zero rows is not zero bytes: a kept empty batch still carries buffers, takes
/// acct.hold and owes a matching release. An imbalance surfaces days later as a budget
/// error on an unrelated query.
#[test] fn holds_and_releases_balance_over_a_plan_with_empty_lanes() { … }
```

- [ ] **Step 3: Commit**

---

### Task 4: The goldens, read rather than accepted

- [ ] **Step 1: Regenerate and read the batch-count delta**

```bash
UPDATE_CANONICAL=1 cargo test --features rust-only -p peacockdb-core
git diff --stat testdata/goldens/
```

Six `.cpu.txt` and six `.cost.txt` move. **Expect the batch-count increase in the low hundreds.** If
it is in the tens of thousands, the guard is not working and Task 1's index field is answering `true`
too widely — that is the 389,331-batch failure, and this diff is the only thing that catches it.

Write the actual number into `empty-build-detail.md`. The spike predicted 244 from the goldens; a
number far from it, in either direction, is worth understanding before going further.

- [ ] **Step 2: Confirm what did not move**

`.plans.txt`, `recipe-payloads.txt`, the `--- memory ---` sections and `.result.txt` should all be
untouched — the first three are plan-time and the last is digest-sorted. Any movement there is a
finding.

- [ ] **Step 3: Commit**

---

### Task 5: The comments that lie, and the two tickets

**Files:**
- Modify: `executor/cpu_backend/accumulate.rs:130`, `executor/gpu_backend/accumulate.rs:222`
- Modify: `llm-wiki/tickets.md`, `llm-wiki/architecture.md:606,856`

- [ ] **Step 1: Rewrite both comments**

`cpu_backend/accumulate.rs:130` says the CPU emits nothing *because* "the device's collapse of no
handles is a refusal (#173)". That premise is false — the device returns `Ok(empty)` at that site —
so the two engines agree today for a reason that is not true. `gpu_backend/accumulate.rs:222` claims
a refusal where the code returns `Ok(Vec::new())`.

- [ ] **Step 2: Cut #173 to its one real site**

Four sites named, one refuses: `gpu_backend/join.rs:209`, and it is on the **probe** side, so this
task does not touch it. The other two return `Ok(empty)` and the C++ throw is unreachable. #173 stays
open, corrected, with no task — it blocks zero cells.

- [ ] **Step 3: Correct #175's corpus reach**

The registry names `tpcds q77` and `tpch q16`. Not `q21`, which is enabled at all five CPU modes and
carries no #175. Check the registry rather than the ticket.

- [ ] **Step 4: Commit**

---

### Task 6: The device rollout and #175's cells

- [ ] **Step 1: Run the two waiting queries on the device**

`build-test-shadgpu.sh`. Five things to prove, in the order they are likely to fail:

1. `filtered_join` can be **constructed** from a zero-row build — the likeliest failure;
2. `left_join` and `left_anti_join` over a zero-row build;
3. `concatenate` and `merge` with zero-row members;
4. `to_arrow_host` on a zero-row string column;
5. `q77` and `q16` green on the CPU, with their device cells landing on #152.

- [ ] **Step 2: Move the cells, or the ticket**

A cell that now reaches a different cause is a cell whose ticket changes. Expect #152.

- [ ] **Step 3: Close #175 only when its cells are gone from the registry**, not when the code lands.

---

## Self-review against the spec

- **§1 the false branch** — Task 2 step 3, which settles whether it is reachable rather than assuming.
- **§2 the table the driver drops** — Tasks 1 and 2; the three conditions are Task 1's test.
- **§3 what the C++ computes** — Task 3 step 1, asserted rather than argued.
- **§4 the lying comments and two tickets** — Task 5.
- **§5 the #152 hazard** — Task 3 step 2's probe-side test, which is the condition rather than a
  watch.
- **§6 tests** — all six, including accounting balance.
- **Goldens** — Task 4, with the batch-count delta as the check the Restriction names.
- **Not covered, deliberately:** a test asserting the exact count of kept empties. It is a property of
  the current corpus, so it would go red on any query change and teach people to update the number
  rather than ask why it moved. The golden review in Task 4 is the check instead.
