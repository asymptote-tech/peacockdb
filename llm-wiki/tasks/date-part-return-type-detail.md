# date-part-return-type — run record

Chain B, task 4. Branch `ENS-date-part-return-type` off `ENS-aggregate-state-types` at
`121047e3`; PR targets `ENS-aggregate-state-types`. Tasks 1–3 are `done` (PRs #158, #159, #160
green) awaiting the human's merge; the chain sits on master `0a338ead`.

## Dispatch 1 — 2026-09-17

- Hosts: **verda down**; rust-only proofs local. **shad-gpu up**, 0 MiB held. Caches warm.
- Pre-dispatch: the `bug_` pin the spec's item 2 says to write first already exists — master's
  chain D added `bug_a_year_extracted_from_a_date_is_exported_as_int16` to
  `tests/gpu_tests/exec_cases.rs`, and task 2 kept it — so the developer flips it rather than
  writes it; `tpch/q7` and `q9` already carry `191` from task 1's rollout. The join-batching
  ticket is #220. The export is told each decimal's precision (task 2) and every count is
  `Int64` (task 3), so a `tp1_single` cell for q7/q8/q9 that fails now fails on something new.
- Routing: the developer works `date-part-return-type-impl.md` — the three C++ cases red, the
  cast, green; the pin flipped; one shad-gpu cycle for the harness and the three rows and, for
  any that passes, its other four modes; item 3's neighbour survey into this file; the record.
- Restriction: `date_part` alone; no Rust-side cast; no other scalar arm; no golden moves.

## Developer notes
