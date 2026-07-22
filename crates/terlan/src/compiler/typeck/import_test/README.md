# Import Typechecking Test Internals

This directory contains split tests for imported module and member context.

## Responsibilities

- Verify imported types and values resolve in the correct namespace.
- Cover ambiguity, missing members, and context-sensitive lookup failures.
- Keep import fixtures separate from production resolver implementation.

## Testing Notes

Run focused import tests plus package and interface contract gates.
