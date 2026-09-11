Repo: /media/data/peacockdb, branch master. Research job for the helper: no task, no branch. Your single write target is the output file named below, under the session scratchpad — treat it as the detail file the Analyst section allows.

HARD RULES: do NOT build, run, or test any code. Do NOT modify any repo file. Read-only except the one output file.

On start read llm-wiki/prompts.md (Shared rules, Analyst), architecture.md, build-test.md, coding-style.md, llm-wiki/reports/hacks-audit.md, llm-wiki/tasks/tasks.md, and the "approach rejected" entries in llm-wiki/archive/archived-tasks.md.

Job: produce the next version of a consolidated fix list. Read SCRATCH/<INPUT> (the current list) and SCRATCH/<REVIEW> (an independent critique of it). For each finding in the review: open the code it cites and decide whether the finding is right; apply the correction if it is, and if it is not, keep the list as it was and record why under a "Findings not taken" section at the end. Do not accept a finding by its severity label — a "blocking" finding that the code does not support is not taken, and a "minor" one that it does is. Where a finding needs the underlying research, SCRATCH/digest-A..D.md and SCRATCH/NN-proposal.md / NN-review.md are the sources.

Keep the shape of the input exactly (preamble, "The two decisions" if present, Fix list ordered by complexity lowest first, Not fixes, Tickets to file, Coverage arithmetic), renumbering fixes if any were split or merged and keeping a "was F<n> in v1" note on each renumbered one. Every fix keeps all its fields; a field that changed carries no changelog — the list is read as a whole, not as a diff.

Write SCRATCH/<OUTPUT>. Reply with only: the path written, the number of fixes by complexity band, the findings taken (by number) and the findings not taken (by number, one clause each).
