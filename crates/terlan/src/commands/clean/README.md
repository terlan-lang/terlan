# CLI `clean` Command Internals

This directory owns `terlc clean`, the project cleanup command.

## Responsibilities

- Remove compiler-owned generated output from a project directory.
- Keep cleanup conservative: source, tests, manifests, assets, templates, and
  user configuration are never deleted.
- Match the generated scaffold's `.gitignore` contract.

## Public Surface

- `run`: command entry point called by the top-level CLI router.

## Core Model

`terlc clean` accepts zero or one positional project directory:

```sh
terlc clean
terlc clean path/to/project
```

The command removes:

```text
_build/
.terlan/tmp/
```

It skips absent paths and reports every removed path.

## Integration Points

- `main.rs`: routes the `clean` verb here.
- `terlc init`: generates `.gitignore` and `Makefile` entries that reference
  this command.

## Testing Notes

- Unit tests cover parsing, conservative path deletion, and idempotent clean
  runs against temporary project directories.
