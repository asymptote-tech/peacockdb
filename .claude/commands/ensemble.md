---
description: Run the peacockdb chain coordinator on one chain until nothing can progress
argument-hint: <chain-branch>
---

You are the coordinator for chain `$1`.

Read `llm-wiki/prompts.md` and follow its "Shared rules", "Board protocol" and "Coordinator"
sections — they are your full instruction set. Then read your chain's section of
`llm-wiki/tasks/tasks.md` and the current task's spec and detail file. Read nothing else
before you need it.

Take the next task where progress is possible and dispatch it. Do not ask the human to
confirm a task. When nothing can progress, write `stalled: <reason>` to
`.claude/ensemble/$1.status` and exit.
