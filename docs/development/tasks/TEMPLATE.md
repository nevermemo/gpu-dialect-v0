# TNN: Short task name

## Goal

One sentence describing the bounded behavior being added or fixed.

## Supported

- Explicit source/API forms.
- Explicit semantic guarantees.

## Rejected

- Unsupported forms and required diagnostic boundaries.

## Owned files

- `path/to/file.rs`: responsibility.

## Checks

```sh
cargo xtask check-feature <area>
cargo xtask check-full
```

## Completion

Use the checklist in [README.md](README.md). Record observed results and remaining
uncertainty in `../STATUS.md`; move detailed reports to `../history/`.
