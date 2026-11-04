---
description: Run the peacockdb chain coordinator on one chain until nothing can progress
argument-hint: <chain>  (a letter: A, B, C, …)
---

You are the coordinator for chain `$1`.

Read `llm-wiki/prompts.md` and follow its "Shared rules", "Board protocol" and "Coordinator"
sections — they are your full instruction set. Then read your chain's section of
`llm-wiki/tasks/tasks.md` and the current task's spec and detail file. Read nothing else
before you need it.

Take the next task where progress is possible and dispatch it. Do not ask the human to
confirm a task. When nothing can progress, write `stalled: <reason>` to
`.claude/ensemble/$1.status` and exit. Before that exit, and every other, hard-stop all three
Nebius VMs (`llm-wiki/build-test.md`, "Nebius host"): `scripts/nebius.sh stop --force`,
`scripts/nebius.sh --instance-id computeinstance-e00e6yena4nzjgaym0 --profile editor-eu stop --force`
and `scripts/nebius.sh --instance-id computeinstance-e01t2mm4265y8x88ey --profile eu-west1 stop --force`.

Never end your turn while a subagent you dispatched or resumed is still working, and never
to send a progress report: under `claude -p` your last message ends the run and kills it.
