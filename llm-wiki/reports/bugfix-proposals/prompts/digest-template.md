Repo: /media/data/peacockdb, branch master. Research job for the helper: no task, no branch. Your single write target is the output file named below, under the session scratchpad — treat it as the detail file the Analyst section allows.

HARD RULES: do NOT build, run, or test any code. Do NOT modify any repo file. Read-only except the one output file.

On start read llm-wiki/prompts.md (Shared rules, Analyst) and skim architecture.md and build-test.md so the vocabulary is right.

Job: digest a group of ticket proposals and their independent critiques into one compact, reconciled record per ticket, so a consolidator can group fixes without reading 50 KB per ticket. For each ticket NN in your group read SCRATCH/NN-proposal.md and SCRATCH/NN-review.md in full, and its row in SCRATCH/00-tickets.md. Where the review contradicts the proposal, open the code (file:line both cite) and decide which side the code supports; say so. Do not soften a review's finding to keep the proposal whole, and do not accept a review's finding without checking it.

Write SCRATCH/digest-<group>.md with, per ticket, exactly this shape (aim for 40–70 lines per ticket; cite file:line for every code claim):

## #NN — <ticket title>
- **Status after research:** live | stale (fixed by <sha>) | misdiagnosed (real cause: …) | wall behind another ticket — one line.
- **Issue:** what goes wrong, where (file:line), which corpus cells it keeps off.
- **Root cause:** the mechanism, 2–5 lines, file:line.
- **Fix (as corrected by the review):** files, functions, the change at line level, what it does not touch; whether any frozen surface moves (C ABI / fbs / wire / declared-schema contract) — name it or say "none"; goldens/registry/comments/build-test.md rows that must move with it.
- **Contested points:** each disagreement between proposal and review, the evidence, and which side the code supports. "none" if none.
- **Minimum corpus query:** the SQL (the review's corrected version where the proposal's was found not to plan / not to reach the code / not minimal), the mode(s), the backend(s), what it shows today, whether it exists in testdata.
- **Cells re-enabled / next wall:** which cells come back, which ticket blocks them next.
- **Overlaps and dependencies:** other tickets whose fix shares code, must land first, or is dissolved by this one — by number, with one clause each.
- **Complexity:** S/M/L/XL, reconciled, one-line reason (files, LOC, surfaces, golden regen).
- **Unticketed defects found:** one line each, with file:line, or "none".

End the file with a section "## Cross-ticket observations" listing anything you noticed that spans tickets in your group (shared mechanism, the same unticketed wall named by several reviews, contradictions between two tickets' proposals).

Reply with only: the path written and one line per ticket: "#NN — status — complexity".
