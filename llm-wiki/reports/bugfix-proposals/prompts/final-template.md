Repo: /media/data/peacockdb, branch master. This job writes ONE file in the repo: llm-wiki/reports/corpus-fixes.md. Nothing else in the repo is touched, nothing is committed. Do NOT build, run, or test any code.

On start read llm-wiki/prompts.md (Shared rules), coding-style.md, and the two existing reports llm-wiki/reports/hacks-audit.md and llm-wiki/reports/benchmark-minimal.md for the house style of a report: short sentences, plain words, the subject near its verb, file:line cites, no narration of how the research was done, no hedging filler.

Inputs: SCRATCH/20-fixes-v2.md (the consolidated fix list, second version), SCRATCH/21-fixes-v2-review.md (its critique) and, where the critique corrects something, the code. Apply every critique finding the code supports; where you do not take one, you do not mention it in the report — the report is a list of proposals, not a changelog.

Write llm-wiki/reports/corpus-fixes.md:

# Corpus fixes
A preamble of at most eight lines: what the page is (localized fixes for the tickets that keep corpus cells disabled), the commit it was read at (`git rev-parse --short HEAD`), the sources (tickets.md, tasks/active-tickets.md, corpus_cases.inc, cost-registry.csv, reports/hacks-audit.md, the code), how the list is ordered (complexity, lowest first; within a band, cells returned), and that the raw proposals and critiques are in llm-wiki/reports/bugfix-proposals/ (one file per ticket per role, named NN-proposal.md and NN-review.md, plus the consolidation rounds).

## Contents
A table: # | fix | closes | complexity | cells back (cpu / gpu) — one row per proposal, in page order.

## Proposals
One section per fix, in complexity order lowest first. Each section has exactly these four parts, in this order, with these headings:

### <n>. <name> — <S|M|L|XL>
**Closes:** tickets closed, tickets re-attributed (from → to).
**Issue.** What the engine does wrong, where (file:line), which corpus cells it keeps off.
**Localized fix.** Files and functions, the change at line level, what it does not touch, how CPU and GPU stay one engine, the frozen surface it moves or "none", the pins / goldens / registry rows / corpus_cases.inc lines that move with it, ordering constraints against other proposals and against approved tasks (name them by tasks.md number).
**Minimum corpus query.** The SQL, the mode(s), the backend(s), what it shows today, whether it exists in testdata. In a fenced sql block.

Keep each section tight: a developer should be able to start from it, but the detail that only matters once the branch exists (which sentence in architecture.md moves, which golden section is byte-affected) belongs in one line, not a paragraph. Roughly 25–45 lines per S fix, up to 60 for M/L.

## Decisions for the human
The items 20-fixes-v2.md holds under that heading, each with both readings and the cells at stake, in the same tight style.

## Not fixes
Stale tickets (with the pin that closes each), walls, duplicates of approved tasks — one line each.

## Tickets to file
One line each: the production behaviour, file:line, which proposal exposes or removes it.

## Coverage
The coverage arithmetic table from 20-fixes-v2.md, corrected per the critique: proposal → cpu cells back, gpu cells back, cumulative, assumption.

Markdown: `#NN` ticket references are plain text (this page is not the ticket registry), file paths in backticks, no HTML anchors, no tables wider than five columns. No LaTeX. Reply with only: the path written, its line count, and the number of proposals by complexity band.
