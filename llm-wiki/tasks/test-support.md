# 5 — the corpus harness moves behind the feature

Kind: production

Fifth of six, after [`test-layout.md`](test-layout.md) and before
[`visibility.md`](visibility.md). Task 4 created `src/test_support/` for the helpers that had two
audiences and moved most of them; this task moves the last two files, and with them the reason the
final eight items are `pub`.

Those eight are `GpuNode`, `validate`, `RunReport`, `render_run`, `GpuBackend`, `GpuContext`,
`RecipePlan` and `attach_recipes`. They exist for `test_cpu_corpus` and `test_gpu_corpus`, which
deliberately stay external. Once the harness they share is inside the crate, nothing outside names
an engine type — and [`visibility.md`](visibility.md) can then take the whole surface down to what
the CLI calls.

It is a small task with one hard claim, and that is deliberate: 698 lines of test code changing
compilation unit is a diff a reviewer has to read line by line, and it should not be sharing a
branch with three hundred one-word demotions.

## The corpus facade

`corpus.rs` (508 lines) and `corpus_gpu.rs` (190) — 698 lines — join the helpers already in
`src/test_support/`. `mode.rs` and `memory_limit.rs` moved with task 4, which needed them. Inside
the crate these two reach `pub(crate)` items, so the eight stop being `pub`.

The two corpus targets stay because they are the genuine end-to-end tier — SQL in, rows out, against committed
goldens, 456 cases — and because keeping them as two binaries keeps `inventory`'s
one-binary-per-engine property resting on two `--test` targets rather than on the `gpu` feature
producing two compilations. The feature route would work and would make the registry guard depend
on something that reads as unrelated to it.

The two binaries name **none** of the eight. They call functions whose signatures name no component type: `cpu_case(dataset, sf, query, mode, oracle)`, `authoritative_mode`, `gpu_case`.
That is what makes the facade real rather than a rename. `over_cap` travels with `corpus.rs` and
`test_corpus_goldens` reaches it the same way. `golden_text.rs` and `registry.rs` are already in
`test_support` — task 4 moved them, because targets it moved needed them too. `corpus_golden.rs`,
`result_text.rs` and `cost_model.rs` name zero crate items and are read only by binaries that stay,
so they stay in `tests/common/` untouched.

## The mechanism is already here

`test-layout.md` declared the `test-support` feature and the self dev-dependency, because the
helpers it moved had two audiences — in-crate tests and the binaries that stayed — and duplicating
one across the boundary guarantees drift. This task adds no mechanism; it adds the last two files
to the module that mechanism created, and the eight items stop being `pub` as a result.

What `#[cfg(test)]` still cannot do is unchanged: the library is compiled without `cfg(test)` when
cargo builds an integration test, which is the whole reason these eight are `pub` today.

## test_support is shaped like a component

`src/test_support/mod.rs` declares the whole API the integration tests may reach. Task 4 put the
harness half there — `Mode`, `MODES`, `MemoryLimit`, the golden-text reader, the registry loader
and the testdata root — and this task adds `cpu_case`, `gpu_case`, `authoritative_mode` and
`over_cap`. Every module below it is private with `pub(crate)` items. Same rules as a component, for a reason beyond symmetry: the signature
check below is a scan of one file only if the API lives in one file.

Being a child of the crate root it sees component facades and not their internals — the same level
as `src/tests/`, and the reason it reaches `pub(crate)` items without any of them becoming `pub`.

**No `pub` in `test_support` names a type from `plan`, `planner`, `executor`, `wire` or
`plan_text`.** That is the rule, stated by what it forbids rather than by a list of what it
allows — the module holds a `PathBuf` root, a golden-text reader and a registry loader, none of
which a list of "strings, `Mode`, `MemoryLimit`" would have permitted. A
signature mentioning `GpuNode` or `RunReport` puts the item straight back on the surface under
another name, and it compiles. This goes in `coding-style.md` beside the visibility rules, and the
layout test enforces it.

## Validation

No test case moves and no golden is touched. What moves is 698 lines between compilation units,
and the failure mode is a harness that changed behaviour while moving.

### Baselines

1. `--list` for all three lib shapes and the seven binaries, and their leaf-name sets.
2. `sha256sum` over `testdata/goldens/`.
3. The `pub`/`pub(crate)` dump from the end of task 4, taken with `scripts/visibility-dump.py`.

### The checks

- **Nothing under `peacockdb-core/tests/` names any of the eight.** Grep the whole directory, not
  the two binaries: the names live in `tests/common/corpus.rs` and `corpus_gpu.rs` today, so a grep
  of the binaries alone is green before the task starts and proves nothing. A hit means
  the facade is a rename, which is the one way this task can look done and not be.
- **No engine type in a `test_support` signature.** Scan every `pub` in `test_support/mod.rs` and
  assert no parameter or return type comes from a component. Then construct the violation, `pub fn tree() -> Box<dyn GpuNode>`, and watch the
  layout test go red. This is the guard the whole facade rests on and the one that would otherwise
  never be exercised.
- **The feature is off in a plain build**, proven by construction: reference `crate::test_support`
  from a non-test path in `lib.rs`, confirm `cargo build` fails with `E0433`, revert. A passing
  build is not evidence — the module simply is not there to break anything.
- **No CI step passes `--features test-support`.** Grep the workflows and assert absence; if one
  does, the self dev-dependency is not doing its job.
- **`inventory` still sees two binaries.** Both corpus targets keep their own registry assertion
  and both must pass — the property the "keep them external" decision exists to protect.
- **Case counts, leaf-name sets and goldens unchanged.** Nothing moves tiers here; a count that
  shifts means a test followed the harness by accident.

## Done when

`corpus.rs` and `corpus_gpu.rs` are in `src/test_support/`; the two corpus binaries reach them
through `cpu_case`, `gpu_case`, `authoritative_mode` and `over_cap` and name none of the eight;
every `pub` in `test_support` has a signature free of engine types and that guard has been seen
red; a plain `cargo build` cannot name `test_support`; no workflow passes the feature; `inventory`
still sees two binaries; case counts, leaf-name sets and goldens are unchanged; and CI is green.
