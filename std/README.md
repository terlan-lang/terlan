# Terlan standard library layout

The `std/` tree contains Terlan standard-library source, generated summaries,
and feature-focused support code. The active stdlib surface is intentionally
focused: `std/core`, `std/collections`, `std/io`, `std/test`, and generated
summaries under `std/summaries`.

Out-of-scope proof-of-concept modules do not belong in this tree. A removed or
new stdlib package can enter the public surface only with source, docs,
generated summaries, and executable Terlan tests.

Source modules map to public Terlan modules by directory:

```text
std/core/          -> std.core.*
std/collections/   -> std.collections.*
std/io/            -> std.io.*
std/test/          -> std.test.*
```

## Module naming convention

Stdlib source files use lowercase snake-case filenames and directories.
Terlan stdlib source module names use the lowercase package root `std`,
lowercase package segments, and a final UpperCamelCase public module segment.

Examples:

```text
std/core/option.tl                    -> std.core.Option
std/core/result.tl                   -> std.core.Result
std/core/int.tl                      -> std.core.Int
std/core/float.tl                    -> std.core.Float
std/core/string.tl                   -> std.core.String
std/collections/list.tl              -> std.collections.List
std/collections/map.tl               -> std.collections.Map
std/collections/set.tl               -> std.collections.Set
std/io/console.tl                   -> std.io.Console
std/test/test.tl                     -> std.test.Test
```

Rules:

- Public stdlib modules must start with the package root `std`.
- Package segments after `std` must be lowercase.
- The final public module segment must be UpperCamelCase.
- File paths must be lowercase snake-case.
- Acronyms in module names stay readable: `CRDT`, `ORSet`, `LWWRegister`.
- One public module per `.tl` source file.
- Internal helper modules must live under an `internal/` directory and must not export public `.typi` APIs.

## Generated Erlang naming convention

Generated Erlang module names use lowercase atoms derived from the Terlan
source module name. Namespace dots are replaced with underscores.

Examples:

```text
std.core.Option                 -> std_core_option
std.core.Result                -> std_core_result
std.core.Int                   -> std_core_int
std.core.Float                 -> std_core_float
std.core.String                -> std_core_string
std.collections.List           -> std_collections_list
std.collections.Map            -> std_collections_map
std.collections.Set            -> std_collections_set
std.io.Console                 -> std_io_console
std.test.Test                  -> std_test_test
```

Generated Erlang files follow the same atom name. Generated interface
summaries keep the Terlan module name:

```text
std.core.Option -> std_core_option.erl
std.core.Option -> std.core.Option.typi
std.core.String -> std_core_string.erl
std.core.String -> std.core.String.typi
```

Rules:

- Generated Erlang module atoms must be deterministic.
- Generated Erlang module atoms must not collide with user modules.
- The `std_` prefix is reserved for standard-library generated Erlang modules.
- Acronyms are converted consistently: `CRDT -> crdt`, `ORSet -> or_set`, `LWWRegister -> lww_register`.
- Internal helper modules include `__internal__` in the generated atom.

Compiler summary artifacts live under:

```text
std/summaries/
```

See `std/summaries/README.md` for `.typi` summary naming, contents, generation, and validation rules.
The active stdlib contract requires every public API to have source, docs,
generated summaries, and executable Terlan tests before release.

Rules:

- Public stdlib APIs must have docs before being considered complete.
- User-facing stdlib behavior must have annotated `.tl` tests.
- Pure implementations define semantics; native acceleration can only be optional.
- Expensive law checks and distributed simulations belong to explicit commands, not normal compile.
