Repo: /media/data/peacockdb, branch master. Research job for the helper: no task, no branch. Your single write target is the output file named below, under the session scratchpad — treat it as the detail file the Analyst section allows.

HARD RULES: do NOT build, run, or test any code. Do NOT modify any repo file. Read-only except the one output file.

On start read llm-wiki/prompts.md (Shared rules, Analyst), architecture.md, build-test.md, coding-style.md, llm-wiki/reports/hacks-audit.md, llm-wiki/tasks/tasks.md (the board: which tasks are approved to build, and what each closes), and the "approach rejected" entries in llm-wiki/archive/archived-tasks.md (casts, wire-schema, empty-answers — rejected 2026-09-10; a fix that is the rejected approach in new clothes is not a fix here).

Inputs: SCRATCH/00-tickets.md (the 27 corpus-blocking tickets), SCRATCH/digest-A.md … digest-D.md (one reconciled record per ticket, built from a proposal and an independent critique), and — when a digest is not enough to state a fix precisely — the underlying SCRATCH/NN-proposal.md and SCRATCH/NN-review.md.

Job: turn 27 per-ticket records into a list of FIXES. One fix may close several tickets (same code, same mechanism, or one dissolving another); one ticket may need two fixes; a ticket may turn out to need no code (stale — closed with a pin), or to be a wall that no localized fix reaches. Group by shared code and shared mechanism, not by ticket section. Order the list by complexity, lowest first; inside a complexity band, by how many cells it brings back. Sequence against the board: a fix that duplicates or must wait for an approved task says so, and a fix that an approved task will make unnecessary is dropped or demoted.

Write SCRATCH/10-fixes-v1.md:

# Fixes, v1
A four-line preamble: what was read, at what commit (`git rev-parse --short HEAD`), how many tickets became how many fixes.

## Fix list (lowest complexity first)
Per fix, exactly this shape:
### F<n> — <short name> — <S|M|L|XL>
- **Closes / moves:** tickets it closes, tickets it re-attributes (from → to), tickets it partially addresses.
- **Issue:** what the engine does wrong for a user, where (file:line), which corpus cells it keeps off (count them; name the queries).
- **Root cause:** 2–5 lines, file:line.
- **Localized fix:** files and functions, the change at line level in developer-implementable detail, what it deliberately does not touch, how CPU and GPU stay one engine, which pins / goldens / registry rows / corpus_cases.inc lines / build-test.md rows move with it, what hacks-audit scaffolding it removes. Frozen surface: name it or "none".
- **Minimum corpus query:** SQL against tpch sf1 or tpcds sf1 in testdata/, mode(s), backend(s), what it shows today, whether it plans today and where it is refused, whether it exists in testdata. One query per fix; if two tickets in the fix need different shapes, give both and say why.
- **Cells re-enabled / next wall:** counts, queries, and the ticket that blocks next.
- **Order and dependencies:** must land after / before which fixes and which approved tasks; conflicts with which.
- **Complexity reason:** files, LOC, surfaces, golden regen, device run needed.
- **Confidence:** what was verified by two readers, what remains unverified without a run.

## Not fixes
- Stale tickets: closed with which pin test, which registry cells drop the number.
- Walls: tickets no localized fix reaches, what non-local change they want, why it was not proposed.
- Duplicates of approved tasks: ticket → task, what to do (nothing / wait / add a case).

## Tickets to file
Unticketed defects the research found, one line each with file:line and the fix that will expose or remove it (a ticket is filed only for production behaviour — a wrong answer, a crash, a refusal, a leak, a regression).

## Coverage arithmetic
A table: fix → cpu cells back, gpu cells back, cumulative, assumptions.

Reply with only: the path written, the number of fixes by complexity band, and the three fixes with the best cells-per-complexity.
