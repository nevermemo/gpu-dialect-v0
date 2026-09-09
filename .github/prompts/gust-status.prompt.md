---
name: "gust-status"
description: "Summarize the GUST repository state from docs/development status files and git, then propose one claimable bounded task with file scope and first command. Read-only."
argument-hint: "Optional focus, e.g. 'compiler', 'engine', 'docs debt'"
agent: "agent"
tools: [read, search, execute]
---
Report the current GUST state and recommend the next claimable task. Do not edit any file.

Read, in this order:
1. [.ai/AGENT_CONTEXT.json](../../.ai/AGENT_CONTEXT.json) — current document pointers.
2. [docs/development/STATUS.md](../../docs/development/STATUS.md) — active ownership and baseline only.
3. [docs/development/NEXT_TASKS.md](../../docs/development/NEXT_TASKS.md) — active and deferred queue only.
4. [docs/development/HANDOFF.md](../../docs/development/HANDOFF.md) — the first 40 lines only.
5. Run `git status --short` and `git log --oneline -5`.
6. If `.ai/VALIDATION.json` exists, read its `finished_utc`, `mode`, and `passed` fields only.

Then answer with exactly these sections:

```
## Ownership
<Active owner and claimed scope, or "none claimed">. Uncommitted files: <count, list up to 8>.

## Last verified state
<HEAD short hash>, <test count from the newest STATUS evidence>, VALIDATION.json <mode/passed/timestamp or "no record">.

## Open tasks (from NEXT_TASKS)
- <Txx — title — status one-liner>

## Recommended next claim
Task: <one bounded task>
Why now: <one sentence tied to a decision or the ordered queue>
File scope: <paths>
Failing test to write first: <name and what it proves>
First command: <exact command>
```

Rules: prefer the ordered queue in the Active section over your own preference; if the tree has uncommitted changes owned by someone else, recommend coordinating instead of claiming those files; if `${input}` names a focus area, restrict the recommendation to it. Keep the whole answer under 300 words.
