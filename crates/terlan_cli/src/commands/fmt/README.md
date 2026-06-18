# CLI `fmt` Command

This command module owns the `terlc fmt` execution path.
Its behavior stays intentionally small:

- parse command-local arguments from `Vec<String>`
- read one source path
- parse module/interface first through `terlan_syntax` syntax-output parser entrypoints
  (`parse_module_as_syntax_output`, `parse_interface_module_as_syntax_output`)
  for formal-path contract validation, then format using the canonical formatter.
- reject removed source-only grammar such as `.terl` export-list declarations
  before formatter output can normalize it; `.terli` interface export summaries
  remain valid interface metadata.
- collapse redundant default-export type imports such as
  `import type std.core.Error.Error.` into `import type std.core.Error.` while
  preserving aliases and multi-import lists.
- print formatted output

The module should stay narrow and avoid carrying compiler-wide orchestration state.
