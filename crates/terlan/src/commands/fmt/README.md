# CLI `fmt` Command

This command module owns the `terlc fmt` execution path.
Its behavior stays intentionally small:

- parse command-local arguments from `Vec<String>`
- read one source path or one directory path
- parse each module/interface once, validate that same tree through formal
  syntax output, then render it with the canonical formatter;
- recursively format `.terl`, `.terli`, and `.terls` files in place when a directory is
  provided.
- preserve `.terls` shebangs and top-level execution while keeping the
  compiler-synthesized `main/0` out of formatted source;
- leave generator-owned files carrying both `@generated true` and
  `@do-not-edit true` untouched in recursive directory mode;
- check one or more files/directories without mutation via
  `terlc fmt --check <path>...`;
  every non-canonical path is reported and the command exits unsuccessfully.
- rewrite one parsed source safely via `terlc fmt --write <file>`; malformed
  input is rejected before the original file is touched.
- reject removed source-only grammar such as `.terl` export-list declarations
  before formatter output can normalize it; `.terli` interface export summaries
  remain valid interface metadata.
- collapse redundant default-export type imports such as
  `import type std.core.Error.Error.` into `import type std.core.Error.` while
  preserving aliases and multi-import lists.
- print formatted output for single files when no mode flag is supplied;
- migrate retired implicit continuation bindings without changing surrounding
  source text via `terlc fmt --migrate-repeated-lets <path>`; directory mode
  rewrites parseable `.terl` files in place, reports intentionally invalid
  fixtures that it skips, and single-file mode prints migrated source.

Canonical layout follows a rustfmt-style structural policy:

- four spaces per indentation level and a 100-column line-width target;
- calls, lists, records, maps, and declarations that do not fit become one
  item per line;
- multiline comma-separated forms use a trailing comma where the grammar
  permits one;
- `case` and `if` arms are indented one level inside their structural block;
- multiline fluent chains place each method call on its own continuation line;
- imports are normalized and ordered deterministically;
- formatting is deterministic and idempotent.

The module should stay narrow and avoid carrying compiler-wide orchestration state.
