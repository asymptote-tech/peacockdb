Repo: /media/data/peacockdb, branch master. Research job for the helper: no task, no branch. Your single write target is the output file named below, under the session scratchpad — treat it as the detail file the Analyst section allows.

HARD RULES: do NOT build, run, or test any code. Do NOT modify any repo file. Read-only except the one output file.

On start read llm-wiki/prompts.md (Shared rules, Analyst), architecture.md, build-test.md, coding-style.md, llm-wiki/reports/hacks-audit.md, llm-wiki/tasks/tasks.md, and the "approach rejected" entries in llm-wiki/archive/archived-tasks.md.

Job: find what is wrong with a consolidated fix list, SCRATCH/<INPUT>. You did not write it and you do not see its author's reasoning. Its sources are SCRATCH/00-tickets.md, SCRATCH/digest-A..D.md and SCRATCH/NN-proposal.md / NN-review.md; open them, and open the code every claim cites — file:line in the list is to be opened, not trusted.

Look for, at least: a grouping that merges two tickets whose mechanisms differ (or fails to merge two that share one); a fix whose "localized" change is actually the rejected approach (casts / wire-schema / empty-answers) or duplicates an approved task; a fix that breaks a cell enabled today, a pinning test, a golden, or CPU/GPU agreement; a sequence that cannot be executed in the order given; a minimum query that would not plan, would be answered by DataFusion's optimizer before the executor, would not reach the failing code, or is not minimal; coverage arithmetic that double-counts cells or ignores the next wall; a complexity that is wrong by a band; a "stale" verdict resting on inference rather than git evidence; a ticket-to-file that is cosmetic rather than production behaviour; and anything a developer would hit in the first hour of the top three fixes that the list did not say.

Write SCRATCH/<OUTPUT> with sections:
1. Verdict — sound / needs changes / reject, one line why.
2. Findings — numbered, each: the fix (F<n>) or section, what is wrong, evidence (file:line), severity (blocking / important / minor), the correction.
3. Grouping check — for each fix, "grouping holds" or the split/merge you propose and why.
4. Ordering check — the order you would use if it differs, with the reason per move.
5. Verified — the claims you opened and found true (brief).

Reply with only: the path written, the verdict, and the number of blocking / important / minor findings.
