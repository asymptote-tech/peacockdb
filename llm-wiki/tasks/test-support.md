# 4 — the crate's API becomes the CLI's

Kind: production

Last of four, after [`test-layout.md`](test-layout.md). That task moved eleven targets into `src/`
and took the public surface from 108 test-driven items to eight. This one removes the last eight, so
that **`peacockdb-core` exposes exactly what the CLI needs and nothing else**.

The eight are `GpuNode`, `validate`, `RunReport`, `render_run`, `GpuBackend`, `GpuContext`,
`RecipePlan` and `attach_recipes`. They exist for two integration targets that deliberately stay
external — `test_cpu_bp_corpus` and `test_gpu_bp_corpus` — through the harness they share.

## Why those two targets stay

They are the genuine end-to-end tier: SQL in, rows out, checked against committed goldens, 456 cases
between them. And keeping them as two binaries keeps `inventory`'s one-binary-per-engine property
resting on two `--test` targets rather than on the `gpu` feature producing two compilations. The
feature route would work, but it would make the registry guard depend on something that reads as
unrelated to it.

## The move

`corpus.rs` (508 lines), `corpus_gpu.rs` (184) and `bp_mode.rs` (88) go to `src/test_support/`,
behind a feature. Inside the crate they reach `pub(crate)` items, so the eight stop being `pub`.

The two binaries name **none** of the eight directly. They call three functions whose signatures are
strings and test-local types: `cpu_case(dataset, sf, query, mode, oracle)`, `authoritative_mode`,
and `gpu_case`. That is what makes the facade real rather than a rename.

`over_cap` travels with `corpus.rs` and `test_corpus_goldens` reaches it through the same facade.
`MemoryLimit` lands here — [`module-layout.md`](module-layout.md) parks it in `tests/common/` in the
interim, and this is its home. `corpus_golden.rs`, `registry.rs`, `golden_text.rs`, `result_text.rs`
and `cost_model.rs` name zero crate items and stay in `tests/common/` untouched.

## The mechanism

```toml
[features]
test-support = []
[dev-dependencies]
peacockdb-core = { path = ".", features = ["test-support"] }
```

The self dev-dependency turns the feature on for `cargo test` and leaves it off for `cargo build`,
so **no CI step passes a flag** and a plain build cannot see the module. `#[cfg(test)]` cannot do
this job: the library is compiled without `cfg(test)` when cargo builds an integration test, which
is the whole reason these eight exist.

## test_support is shaped like a component

`src/test_support/mod.rs` declares the whole API the integration tests may reach — `cpu_case`,
`gpu_case`, `authoritative_mode`, `over_cap`, `BpMode`, `BP_MODES`, `MemoryLimit`, `TIER`, `BUDGET`
— and `mod corpus; mod corpus_gpu; mod bp_mode;` are private implementation modules with
`pub(crate)` items. Same rules as a component, for a reason beyond symmetry: **the check below is a
scan of one file only if the API lives in one file.**

Being a child of the crate root it sees component facades and not their internals — the same level
as `src/tests/`, and the reason it reaches `pub(crate)` items without any of them becoming `pub`.

## The rule that makes this encapsulation rather than renaming

**Every `pub` in `test_support` takes and returns strings, `BpMode`, `MemoryLimit` or nothing.** A
signature mentioning `GpuNode` or `RunReport` puts the item straight back on the surface under
another name, and it compiles. This goes in `coding-style.md` beside the visibility rules, and the
layout test enforces it.

## The surface, after

Bare `pub` appears **eight times in `src/` outside the feature gate, in three files**.

| Item | Declared in |
|---|---|
| `build_session_state`, `register_tables_for` | `lib.rs` |
| `plan`, `PlanKnobs`, `BatchSizing`, `SMALL_TABLE_BYTES` | `planner/mod.rs` |
| `run`, `CpuBackend` | `executor/mod.rs` |

`test_support/mod.rs` is a fourth file carrying bare `pub` and the only one behind a feature. The
guard distinguishes them: eight unconditional items checked by count and location, a feature-gated
set checked by signature.

## Validation

No test case moves in this task and no golden is touched, so both are pinned as invariants rather
than checked as outcomes. What moves is visibility, and visibility regressions compile.

### Baselines

1. The `pub`/`pub(crate)` item dump with declaring files, from the end of task 3 — sixteen `pub`.
2. `--list` for `--lib`, `--lib --features gpu` and the six binaries: 519, 55, 509.
3. `sha256sum` over `testdata/goldens/`.

### The checks

- **The surface lands on exactly eight**, and the assertion is the table above — file and item, not
  a count. A count passes when one item is dropped and another added. The other eight must appear
  in the dump as `pub(crate)`, not merely as absent: an item deleted and an item demoted look the
  same to a count and different to this.
- **No engine type in a `test_support` signature.** Scan every `pub` in `test_support/mod.rs` and
  assert its parameter and return types are drawn from a small allowed set. Then construct the
  violation — add a `pub fn tree() -> Box<dyn GpuNode>` — and watch the layout test go red. This is
  the guard the whole task rests on, and it is the one that would otherwise never be exercised.
- **The feature is off in a plain build**, proven by construction: add a line in `lib.rs`
  referencing `crate::test_support`, confirm `cargo build` fails with `E0433`, revert. A passing
  `cargo build` is not evidence — the module simply is not there to break anything.
- **No CI step passes `--features test-support`.** If one does, the self dev-dependency is not
  doing its job and a plain consumer build would differ from CI's. Grep the workflows for it and
  assert absence.
- **Case counts unchanged**: 519 / 55 / 509, and the leaf-name sets identical to task 3's. Nothing
  moves tiers here; a count that shifts means a test followed the harness by accident.
- **Goldens byte-identical.** Nothing in this task can reach them; a diff means the harness changed
  behaviour while moving, which is the failure mode of moving 780 lines of test code.
- **`inventory` still sees two binaries.** Both corpus targets keep their own registry assertion,
  and both must still pass — that is the property the whole "keep them external" decision protects.
  Verify by running each and confirming its registry check covers only its own engine's columns.

### Done when

`peacockdb-core` exposes exactly the eight items in the table, with the other eight demoted to
`pub(crate)` rather than deleted; every `pub` in `test_support` has a signature free of engine
types, and the guard for that has been seen red; a plain `cargo build` cannot name `test_support`;
no workflow passes the feature; case counts and goldens are unchanged; `coding-style.md` carries the
signature rule; and CI is green.
