# Terlan CLI Run Command

## Purpose

The `run` command is the user-facing shortcut for building a Terlan package and
executing the generated package launcher.

## Responsibilities

- Delegate compilation to the existing `build` command.
- Read `terlan-package-build.json` from the selected output directory.
- Execute the package launcher recorded in build metadata.
- Return the launched program's exit status.

## Boundaries

- `run` does not implement a separate compiler path.
- `run` does not execute single-file builds until those builds emit executable
  package metadata.
- `run` currently supports Erlang package launchers only.

## Validation

Tests cover command argument rejection, executable metadata parsing, and an
end-to-end package run through the generated launcher.
