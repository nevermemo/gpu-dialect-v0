# Agent workflow

Agents and humans share the same source of truth. Do not use chat history as project memory.

## Pickup sequence

1. Read `docs/development/STATUS.md` active section.
2. Read `docs/development/NEXT_TASKS.md` for claimable work.
3. Read `docs/development/HANDOFF.md` for compact recent context.
4. Read the matching `docs/development/tasks/` contract when one exists; consult
	`history/` only when current state or an invariant remains unresolved.
5. Check `docs/DECISIONS.md` and `docs/ARCHITECTURE.md` before touching compiler, runtime or engine boundaries.
6. Run `git status --short --branch` and one focused baseline command.

## Claiming work

Before editing, claim a bounded task and file scope in `docs/development/STATUS.md`. If another owner has overlapping files, coordinate with the human instead of overwriting.

A claim should include:

```text
owner: <agent/person>
claim: <bounded task>
next_focused_check: <exact command>
full_check_needed_before_commit: yes|no
```

## Working pattern

Use the smallest falsifying check first. Good defaults:

```sh
cargo xtask status
cargo xtask check-changed
cargo xtask check-feature macro
cargo xtask check-feature reflection
cargo xtask check-feature gpu-smoke
cargo xtask check-workspace
```

Use `cargo xtask check-full` only for major/release evidence or before committing a broad change.

## Review pattern

For meaningful changes, use Scout -> Builder -> Verifier:

- Scout: read-only discovery and dependency tracing.
- Builder: implementation and focused validation.
- Verifier: independent read-only review, adversarial checks and exact findings.

Skip Scout when the files are obvious. Skip Verifier only for trivial text-only changes.

## Handoff

Before handing off, update `docs/development/STATUS.md` with exact files changed, tests run, known failures, uncertainty and the next command. Update `docs/development/HANDOFF.md` when the next agent would otherwise need chat history.

Do not mark researched plans complete. Commands alone are not evidence; record observed results, counts, adapter/tool versions when relevant, and whether generated artifacts drifted.
