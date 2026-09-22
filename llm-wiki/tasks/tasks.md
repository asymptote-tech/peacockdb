# Task board

One `##` section per chain, lettered, naming its base. States and transitions: the "Board protocol"
section of `llm-wiki/prompts.md`. A coordinator writes this file only on its own chain
branch, which is why two chains need no locking.

## Chain A (base: master)

### 1. [`refcounted-tables.md`](refcounted-tables.md) — closes [#145](../tickets.md#t145), [#152](../tickets.md#t152) — state: new

The largest and the one that frees the most: 39 `.table` sites across 11 files, plus
`peacock_handle_retain`, a new ABI symbol — needed because `execute_one` takes its inputs by
value, so the registry cannot keep a handle and let an operator own its input unless the owner is
shared. 75 queries carry #152. Memory accounting is deliberately out of scope and will diverge.

## Chain B (base: master)

Schema divergence, end to end. Five of its six tasks merged 2026-09-21 as PRs #158–#160, #162 and
#163 (utf8-everywhere, decimal-precision-at-export, aggregate-state-types, device-schema-harness,
driver-output-hook; specs in `../archive/archived-tasks.md`). What remains is the one fix the human
held back: `date_part`'s device cast, pending a decision whether the return type is fixed on the
device or in DataFusion's function registry.

### 1. [`date-part-return-type.md`](date-part-return-type.md) — closes [#191](active-tickets.md#t191) — state: blocked(done) — PR #161

`expr.cpp`'s `date_part` arm casts cuDF's INT16 to the `return_type` the wire already names. Three
plan-executor cases, one harness case written first as the pin, `tpch/q7`, `q8`, `q9`. Rebased onto
the merged chain; its PR targets master. Held by the human: the alternative is a DataFusion-side
`ScalarUDF` replacement that declares what cuDF produces, which would make the device cast a refusal.

## Chain ENS-bp-benchmarks (base: master)

### 1. [`bp-benchmarks.md`](bp-benchmarks.md) — state: done — PR #139

The corpus benchmark at sf40: one timing tree per (dataset, mode), one record row per cuDF
call, two Nsight captures and the panels. The branch and PR predate the spec, so the
coordinator picks the branch up as unfinished work rather than branching afresh: it is one
squashed commit on master, re-homed into the component layout and never built there;
`bp-benchmarks-detail.md` says what it holds and what the spec removes.
