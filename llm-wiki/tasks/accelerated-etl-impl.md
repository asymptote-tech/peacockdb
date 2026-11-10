# accelerated-etl: implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this
> plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Write `llm-wiki/designs/accelerated-etl.md`, a proposal of at most two pages that builds
the product step by step from the draft.

**Architecture:** First a checker script, adapted from the one used for the comet-gpu rewrite.
Then the document, written in the spec's nine-step order, with the checker run after it. Then one
squashed commit on the branch.

**Tech stack:** Markdown, Python 3 (standard library), git.

**Spec:** `llm-wiki/tasks/accelerated-etl.md`

## Global constraints

- At most about 900 words. No appendices.
- Numbers come only from `llm-wiki/artifacts/accelerated-etl/draft-product.md`, plus the 50%
  expectation and "DuckDB 1.5.4 on 128 threads".
- Sentences have 25 words or fewer; paragraphs have six sentences or fewer.
- No comet-research findings. No research.
- Branch `ENS-accelerated-etl`; one commit at the end; no PR, no reviewer.

## Review focus

1. A number that is not in the draft (the checker covers this).
2. A term ("fused query", "pinned RAM", "egress", "neocloud", "PCIe") used before it is
   explained (the checker's term list covers this).
3. The 50% read as a measurement. Task 2 states it as an expectation, against a CPU VM at the same
   price.
4. Over two pages (the checker counts words).
5. A broken link to the evals (the links stay exactly as the draft gives them).

---

### Task 1: Checker

**Files:** Create `/tmp/claude-1000/-media-data-peacockdb/8810e74b-a492-486d-90fc-aa8b47be9bde/scratchpad/check-etl.py`
(scratch, not committed).

- [ ] **Step 1:** Copy `~/tmp/comet-gpu-rewrite/check.py`. Change the sources to the draft only,
  plus the allowed tokens `50`, `1.5.4`. Change `TERMS` to: batch, time window, fused query,
  data loading, pinned RAM, egress, neocloud, NIC, cost ratio. Add a word count that fails above
  950. Drop the link-to-commit and the PST checks.
- [ ] **Step 2:** Run it on the draft. Expected: exit 1 (order and sentence findings).

### Task 2: The document

**Files:** Create `llm-wiki/designs/accelerated-etl.md`.

- [ ] **Step 1:** Write the nine steps of the spec as about seven short sections, each term in
  bold at first use. Include the spec's calls: the two-stage data path, the 50% against a CPU VM
  at the same price, the on-prem alternative, DuckDB 1.5.4 on 128 threads.
- [ ] **Step 2:** Run the checker. Expected: exit 0.
- [ ] **Step 3:** Read it from the top as a design partner would. Fix any term that is not clear.

### Task 3: Land it

- [ ] **Step 1:** Commit the four files (`designs/accelerated-etl.md`,
  `artifacts/accelerated-etl/draft-product.md`, the spec and this plan) as one commit on
  `ENS-accelerated-etl`, with the attribution line.
- [ ] **Step 2:** `git log --oneline master..ENS-accelerated-etl` shows exactly one commit.
