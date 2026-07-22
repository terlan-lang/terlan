# Function Head Pattern Migration

Version: 0.0.7.

This versioned migration guide is the public closeout document for function-head
pattern parameters. It turns the Slice 11 diagnostic IDs into user-facing
documentation, CLI/IDE assist workflow expectations, and release lifecycle
rules. The guide includes before/after examples for accepted and rejected
forms.

## Accepted Pattern Forms

Pattern-first parameters are accepted on the VM target when the compiler can
rewrite them without changing behavior.

```text
Before:
pub full_name({name, family_name}: User): String ->
    name + " " + family_name.

After:
pub full_name({name, family_name}: User): String ->
    name + " " + family_name.
```

Keeping the whole value uses the Erlang/Elixir-style alias order: pattern first,
then the binding name.

```text
Before:
pub full_name({name, family_name} = user: User): String ->
    user.id.to_string() + ": " + name + " " + family_name.

After:
pub full_name({name, family_name} = user: User): String ->
    user.id.to_string() + ": " + name + " " + family_name.
```

Accepted forms include tuple, constructor, list, map, record, wildcard, and
pattern-first alias parameters. Binary layout patterns remain documented as a
reserved scaffold until bitstring lowering is implemented.

VM route handlers use the same function-head pattern rules. The request
descriptor is passed as the first argument and decoded route captures follow in
route order:

```terlan
import std.collections.Map.
import std.core.Option.
import std.http.Response.
import type std.http.Response.{Response}.

pub show(
    {:request, _method, _path, params, _body, _query, _query_pairs, _headers, _cookies}: {Atom["request"], String, String, Map[String, String], String, String, Map[String, String], Map[String, String], Map[String, String]},
    id: String
): Response ->
    Response.text(Option.with_default(params.get("id"), id)).
```

If the descriptor pattern or a guard does not match, dispatch reports the same
`pattern_head_failed` clause diagnostics as normal VM function calls.

## Rejected Pattern Forms

Legacy alias order is rejected or accepted with warning depending on mode and
tooling phase. The supported spelling is pattern-first alias order.

```text
Before:
pub full_name(user = {name, family_name}: User): String ->
    name + " " + family_name.

After:
pub full_name({name, family_name} = user: User): String ->
    name + " " + family_name.
```

Defaulted pattern parameters are rejected because a default expression and a
destructuring pattern both want to own the same call-site fallback semantics.

```text
Rejected:
pub value({left, right}: Pair = default_pair()): Int ->
    left + right.
```

## Strict-Mode Behavior

The parser output for 0.0.7 may mark legacy pattern-head syntax as accepted with
warning while migration tooling is active. Strict mode escalates that warning to
an error with the same migration ID. Once a file has already been migrated by
tool output, stale legacy alias order emits
`migration.function_head_pattern.remains` once per file in strict mode only.

## CLI/IDE Assist Workflow

CLI diagnostics and IDE code actions must expose the same migration ID and the
same docs link in CLI diagnostics. The diagnostic-to-doc-id round-trip is:

- `migration.function_head_pattern.invalid_alias_style`:
  `#migrationfunction_head_patterninvalid_alias_style`; rewrite to
  pattern-first alias order.
- `migration.function_head_pattern.safe_reject`:
  `#migrationfunction_head_patternsafe_reject`; explain why automated rewrite
  is unsafe.
- `migration.function_head_pattern.unsupported_backend`:
  `#migrationfunction_head_patternunsupported_backend`; keep VM form and reject
  unsupported JS fallback.
- `migration.function_head_pattern.remains`:
  `#migrationfunction_head_patternremains`; report stale migrated-once syntax.

## Backend Fallback Caveats

VM is the reference target for accepted function-head patterns in 0.0.7. JS
targets must not silently lower a pattern form when doing so would alter target
behavior. The support matrix for VM/JS targets is:

| Form | VM | JS |
| --- | --- | --- |
| Tuple/list/map/record parameter patterns | accepted | explicit diagnostic if not supported |
| Pattern-first alias parameters | accepted | explicit diagnostic if not supported |
| Legacy alias order | accepted with warning or strict error | explicit diagnostic |
| Binary layout parameter scaffold | rejected until bitstring lowering exists | rejected |

## migration.function_head_pattern.invalid_alias_style

This ID is emitted when source uses legacy alias order. The safe migration is to
move the pattern before the binding name.

## migration.function_head_pattern.safe_reject

This ID is emitted when the compiler cannot prove that a pattern-head rewrite is
behavior-preserving. The tool must show the source span and leave the code
unchanged.

## migration.function_head_pattern.unsupported_backend

This ID is emitted when a target cannot preserve accepted VM semantics. The
compiler must reject the target fallback instead of lowering to a weaker runtime
shape.

## migration.function_head_pattern.remains

This ID is emitted by the stale syntax lint when legacy alias order remains in a
file that was already migrated once by tool output. It is advisory and appears
once per file in strict mode only.

## Release Closeout Proof

The `function-head-pattern-migration-docs-check` gate is the
markdown snapshot assertion for migration docs sections and link targets. It
also checks the CLI
diagnostic-to-doc-id round-trip, the changelog/release-note anchor, the README
quickstart migration example, and the legacy-only codebase release-note example.
The `function-head-migration-lint-check` gate writes
`target/quality/function-head-pattern-migration-manifest.json` so future
codemods and editor actions reuse the same migration IDs and docs anchors as the
compiler diagnostic.
The `function-head-pattern-migration-assist-check` gate proves the public
`terlc migrate pattern-head` command is dry-run-first, rewrites only safe
reverse-alias candidates when `--write` is passed, keeps already-correct heads
idempotent, and emits JSON report rows with the same manifest migration IDs.
