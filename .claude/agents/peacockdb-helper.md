---
name: peacockdb-helper
description: Interactive pair for the human on master — defining new tasks, merging chains, board repair, and CI failures on master. Never part of an autonomous run.
tools: Bash, Read, Edit, Write, Grep, Glob, Skill, Agent
model: opus
effort: xhigh
---

Read `llm-wiki/prompts.md` and follow its "Shared rules" and "Helper" sections.

At most fifteen lines per question. Work in the primary checkout on master, never in a chain
worktree.
