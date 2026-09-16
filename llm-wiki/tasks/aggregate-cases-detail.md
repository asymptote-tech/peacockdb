# aggregate-cases — run record

Chain `ENS-join-cases`, task 2. Branch `ENS-aggregate-cases` off `ENS-join-cases`; PR targets
`ENS-join-cases`.

## Handoff from join-cases — 2026-09-16

The cpu executors refuse `Utf8` data under a `Utf8View` declaration everywhere but the unload
(`cpu_backend/mod.rs` `declared_as` → `try_new`, which the aggregate path reaches at `mod.rs`
and `accumulate.rs`). So the spec's `Utf8View` group-key row cannot be read with `.same()`:
join-cases read such cases on the device under the declaration against the cpu on `Utf8` over
the same strings, asserting the cpu's refusal so a harness that closes the gap turns them red
(`device_on_a_declared_utf8view_key_answers_as_the_cpu_on_a_utf8_key` in
`gpu_tests/join_dimension_cases.rs`; the gap itself in `join-cases-detail.md`). Take the same
reading here unless a `run_both` that casts the cpu's upload to the leaf's declared types has
landed. The #183 pin asserts the exported type in the slot, as the four operator pins do.

Superseded on 2026-09-16: the amended specs (master `c6b7f63c`) declare no view type, the
`Utf8View` group-key row is gone, and join-cases' Task 8 deleted the device-only helper and
the four operator pins named above. The harness gap itself is still true and still recorded in
`join-cases-detail.md`; nothing here reads a case through it any more.
