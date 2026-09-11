Repo: /media/data/peacockdb, branch master, primary checkout. This is a research job for the helper, not a run: there is no task, no branch, no detail file. Your write target for this job is exactly one file, named below, under the session scratchpad — treat it as the detail file the Analyst section allows.

HARD RULES: do NOT build, run, or test any code (no cargo, cmake, ctest, pytest, python, peacockdb CLI). Do NOT modify any file in the repo. Read-only except the one output file.

On start read llm-wiki/prompts.md (Shared rules, Analyst), architecture.md, build-test.md, coding-style.md, and llm-wiki/reports/hacks-audit.md.

Job: find what is wrong with a fix proposal for ticket #NN. Read the ticket text (llm-wiki/tickets.md or llm-wiki/tasks/active-tickets.md), SCRATCH/00-tickets.md (its row), and SCRATCH/NN-proposal.md. You do not see how the proposer reasoned; you see only the proposal, and you check every claim in it against the code by reading it yourself — file:line cites in the proposal are to be opened, not trusted.

Look for, at least: a wrong or incomplete root cause; a fix that is not actually localized (touches a frozen surface it claims not to, or needs a change it does not list); a fix that breaks a cell that is enabled today, a pinning test, a golden, or the CPU/GPU agreement; missed semantics (NULLs, three-valued logic, decimals/precision, empty lanes, multi-lane merges, nullability declarations); scaffolding from hacks-audit.md the fix leaves in place or fights; a "minimum corpus query" that would not plan, would not reach the failing code, would be answered by DataFusion's optimizer before reaching the executor, or is not minimal; a wrong complexity estimate; anything a developer would hit in the first hour that the proposal did not say.

Write SCRATCH/NN-review.md with sections:
1. Verdict — sound / needs changes / reject, one line why.
2. Findings — numbered, each: what is wrong, the evidence (file:line), severity (blocking / important / minor), and the correction you propose.
3. Claims verified — the proposal's claims you opened and found true (brief; this is what lets the consolidator trust the rest).
4. Corrected proposal — if the verdict is "needs changes" or "reject", the fix as you would state it, in the same 8-section shape the proposal used, but only the sections that change.
5. Complexity — your own S/M/L/XL and why, if it differs.

Reply with only: the path written, the verdict, and the number of blocking / important / minor findings.
