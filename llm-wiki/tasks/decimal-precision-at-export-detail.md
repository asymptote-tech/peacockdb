# decimal-precision-at-export — run record

Chain B, task 2. Branch `ENS-decimal-precision-at-export` off `ENS-utf8-everywhere` at
`0b8c05fb`; PR targets `ENS-utf8-everywhere`. Task 1 sits at `completeness approved` with its
PR #158 conflicting against master and no CI run possible until the human calls a rebase; that
rebase will reach this branch as `rebase needed(...)` in its turn.

## Dispatch 1 — 2026-09-17

- Hosts: **verda down** (name resolution), so rust-only proofs run locally; **shad-gpu up**,
  0 MiB of 144 GiB held.
- Caches: `target-cudf-rapids-cuda-12.2` warm from task 1's cycles; `cpp/build` present,
  `cpp/build26` absent.
- Pre-dispatch greps match the spec's baseline: 28 `output_schema` hits across `cpp`,
  `peacockdb-core`, `flatbuffers`; 64 `Utf8View|BinaryView` hits in `cpp` and `flatbuffers`;
  `DECIMAL32|DECIMAL64` in `cpp/src/gpu_executor.cpp` and `scan.cpp`; no
  `peacock_handle_schema` anywhere.
- Regeneration: `peacockdb-core/build.rs` runs the vendored flatc over `gpu_plan.fbs`, so a
  rust-only build regenerates the Rust bindings; `cpp/CMakeLists.txt:135` does the C++ side.
- Routing: the developer works `decimal-precision-at-export-impl.md` task by task — the ABI
  and export with its C++ cases, the Rust callers, the plan rule red then green, the one wire
  rebuild, `peacock_handle_schema`, then one shad-gpu cycle for the harness and one for the
  53-query rollout at `tp1_single`, then the record. Both sides rebuild together on the device
  cycle: staged binaries from before this task cannot read the plan after it.
- The spec's restriction holds: no cast, no range check, no relabel on the Rust side after
  decode; the label is set once, on the imported Arrow schema in `export_table_to_ipc`.

## Developer notes
