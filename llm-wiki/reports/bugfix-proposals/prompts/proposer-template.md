Repo: /media/data/peacockdb, branch master, primary checkout. This is a research job for the helper, not a run: there is no task, no branch, no detail file. Your write target for this job is exactly one file, named below, under the session scratchpad — treat it as the detail file the Analyst section allows.

HARD RULES: do NOT build, run, or test any code (no cargo, cmake, ctest, pytest, python, peacockdb CLI). Do NOT modify any file in the repo. Read-only except the one output file.

On start read llm-wiki/prompts.md (Shared rules, Analyst), architecture.md, build-test.md, coding-style.md, and llm-wiki/reports/hacks-audit.md (all of it — it names the code that grew around known defects, and a localized fix must not leave that scaffolding standing or fight it).

Ticket: #NN. Read its text in llm-wiki/tickets.md or llm-wiki/tasks/active-tickets.md, and its row in SCRATCH/00-tickets.md (the enumeration of corpus-blocking tickets, with the cells it disables and the evidence lines). Then read the code thoroughly: every path the ticket names, what those call, the planner/wire/executor pieces on the same path, the tests that pin the current behaviour (refusal tests, goldens, corpus_cases.inc comments, cost-registry.csv rows), and the CPU/GPU twin of whatever is changed, since the two backends must stay one engine.

Question: does a LOCALIZED solution exist — a fix confined to a few functions/files, no new ABI symbol or wire-format change unless truly unavoidable, that re-enables the disabled corpus cells without a redesign? If none exists, say so and give the smallest non-local one, naming what surface it must change.

Write SCRATCH/NN-proposal.md with exactly these sections:
1. Issue — what the engine does wrong, where (file:line), which corpus cells/queries it disables (from 00-tickets.md, corrected if the code says otherwise).
2. Root cause — the mechanism, traced through code, with file:line cites.
3. Localized fix — the concrete change: files, functions, what each line-level change is, in enough detail that a developer could implement it without re-deriving; what it deliberately does NOT touch; how CPU and GPU stay in agreement; which pinning tests/goldens/registry rows/comments must change with it; what hacks-audit scaffolding it removes or must respect.
4. Alternatives rejected — one line each, why.
5. Minimum corpus query — the smallest SQL that exposes the issue, against the tpch sf1 or tpcds sf1 schema in testdata/, with the planning mode(s) (tp1-single, tp1-rowgroup, tp4-single, tp4-rowgroup, tp4-sized) it must run at, which backend(s), and what wrong behaviour it shows today. It need not exist in testdata; prefer a query smaller than the corpus ones. State whether it plans at all today and where it is refused.
6. Cells re-enabled — which corpus_query! cells / registry rows come back, and which stay off behind another ticket.
7. Risks and unknowns — what you could not verify by reading.
8. Complexity — S / M / L / XL with the reason: files touched, rough LOC, whether a frozen surface (C ABI, FlatBuffers schema, wire format, declared-schema contract) changes, whether goldens must be regenerated.

Reply with only: the path written, the complexity letter, and a three-line summary of the fix.
