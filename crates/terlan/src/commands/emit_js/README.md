# CLI `emit-js` Command

This module owns the `terlc emit-js` execution path and intentionally contains the
full conversion routine used by the backend emitter. It does not implement generic
compiler pipeline concerns directly; it reuses shared phase helpers from the CLI
command runtime.

Responsibilities:

- parse `emit-js` command-local flags
- run compile/diagnostic flow through shared compiler helpers
- own the 0.0.4 JavaScript target contract used by the future
  `terlc build --target js` path
- emit JavaScript from the CoreIR public function surface, lowering the current
  supported expression subset and using stubs as the fallback for unsupported
  bodies
- emit optional TypeScript declarations from the CoreIR public type/function
  surface
- treat Oxc as the selected JavaScript backend implementation for parsing,
  validation, AST/codegen, and future bundling

Current implementation note:

- JavaScript source is still produced by a small string emitter while the
  backend subset is being proven out, then parsed and reprinted through Oxc
  codegen before the artifact is written.
- The next migration target is direct `CoreIR -> Oxc AST -> Oxc codegen -> JS`.
- Oxc types must remain inside JS backend modules. They must not become part of
  CoreIR, typechecking, target-profile validation, or formal proof gates.

## JavaScript Target Contract

`target_contract` owns the release-facing JavaScript target constants before
`terlc build --target js` is wired into the build command. This prevents the
build path, manifest writer, and target-profile validation from inventing
different paths or target spellings.

Supported 0.0.4 target spellings:

- `js`, normalized to `js.shared`
- `js.shared`
- `js.browser`
- `js.worker`

0.0.4 emits library-style ES modules with plain `.js` extension. It does not
emit `.mjs`, a bundle, `package.json`, or npm metadata.

Default artifact layout:

```text
_build/js/
  manifest.json
  modules/
    examples/js/Add.js
  metadata/
    target-profile.json
    diagnostics.json
```

Unsupported JS target features use the `js_emit_unsupported` diagnostic family
unless they are rejected earlier by the general `target_profile` validator.

## Oxc Codegen Migration

When the backend moves beyond the bootstrap string emitter, use this module
split:

- `core_lowering`: converts `CoreModule` / `CoreExpr` into backend decisions
  and owns Terlan semantics.
- `direct_ast`: converts the currently supported direct CoreIR subset into Oxc
  AST nodes and prints them with Oxc codegen.
- `direct_helpers`: owns the small shared Oxc helper surface for direct AST
  emission, including arena string copies, conservative identifier validation,
  safe numeric-literal checks, and CoreIR operator mappings.
- `direct_reachability`: owns direct backend reachability filtering for public
  exports and private local helpers before Oxc AST construction begins.
- `oxc_backend`: exposes the command-facing JS backend facade and the
  parser/codegen fallback while bootstrap string lowering remains in use.
- `target_contract`: owns target spelling, output layout, module format, and
  unsupported-feature diagnostic constants for `--target js` build work.
- `declarations`: emits `.d.ts` declarations from CoreIR public type/function
  metadata. This module owns declaration string output unless Oxc provides a
  clearly better declaration path.

Dependency rules:

- `oxc_backend` is now part of the backend path; direct CoreIR-to-Oxc-AST
  construction should land incrementally by expression subset in `direct_ast`.
- A test-only direct Oxc AST smoke path exists in `direct_ast` to prove Oxc
  AST construction compiles before production lowering switches over.
- Production `emit-js` now attempts direct `CoreModule -> Oxc AST -> Oxc
  codegen` lowering for the first real CoreIR expression subset: public,
  single-clause functions with variable parameters, integer/string-like literal
  returns, boolean literal returns, finite float literal returns,
  tuple/list/fixed-array literal returns, expression-side list cons, index
  expressions, identifier-key map literals, field/record access, record
  construction/update, literal-pattern case expressions with a variable
  scrutinee, atom/boolean/integer/finite-float literal clauses, and a final
  wildcard fallback, total if expressions with a final `true` clause, anonymous
  function values with direct variable parameters, single-generator unguarded
  list comprehensions with a direct variable pattern, unary negation, template
  instantiation as object-like artifact values, local named calls, focused
  pipe-forward expressions into local named calls, selected primitive
  intrinsics such as `core.string.contains`, `core.string.starts_with`, and
  `core.string.length`, and arithmetic/comparison expressions.
  Terlan integer division `div` lowers as `Math.trunc(left / right)` so it
  stays distinct from floating-point `/`.
  CoreIR binary payloads preserve source string delimiters during earlier
  compiler phases; JS emission normalizes them to runtime string values before
  constructing JavaScript string literals.
  Unsupported modules still fall back to bootstrap CoreIR-to-JS text followed
  by Oxc parse/codegen.
- Partial if expressions without a final `true` fallback remain unsupported in
  `emit-js` until their no-match runtime behavior is represented explicitly.
  They are covered by fallback regression so direct Oxc lowering cannot
  silently invent missing-branch behavior.
- Case expressions with guards, binding patterns, destructuring patterns, or no
  final wildcard fallback remain unsupported in `emit-js` until pattern
  dispatch semantics are represented explicitly. Binding-pattern cases, guarded
  literal cases, destructuring-pattern cases, and partial literal cases without
  a final wildcard fallback are covered by fallback regressions so direct Oxc
  lowering cannot silently invent binding capture, guard dispatch,
  destructuring dispatch, or missing-branch behavior.
- Anonymous function values lower to JavaScript arrow functions. Callable-value
  invocation uses Terlan's dedicated `f.(args)` syntax.
- Pipe-forward expressions lower only for the focused local-call shape
  `value |> f(extra)` as `f(value, extra)`. General pipe targets such as
  `value |> target` are rejected by the typechecker before backend lowering,
  because the right side of `|>` must be a function call.
- List comprehensions lower only for the simple `[yield | value <- source]`
  shape. The current formal grammar admits one generator and no generator
  guard. Destructuring generator patterns lower into CoreIR but remain
  unsupported in `emit-js`; they are covered by a fallback regression until JS
  pattern dispatch semantics are designed.
- Remote calls and remote function references, including Erlang interop forms
  such as `erlang.abs(1)` and `fun erlang:abs/1`, remain unsupported in
  `emit-js` until the JavaScript target interop policy is selected. They are
  covered by fallback regressions so direct Oxc lowering cannot silently invent
  module-call or function-reference semantics.
- Constructor calls such as `Ok(1)` and constructor chains such as
  `User(id, name) with Admin { ... }` remain unsupported in `emit-js` until the
  JavaScript constructor runtime representation is selected. They are covered by
  fallback regressions so resolved CoreIR constructor identities cannot silently
  lower to ad hoc JavaScript value shapes.
- `receive` expressions remain unsupported in `emit-js` until JavaScript target
  mailbox/actor semantics are selected. They are covered by fallback regression
  so direct Oxc lowering cannot silently invent receive-loop behavior.
- `try` expressions remain unsupported in `emit-js` until JavaScript target
  exception and cleanup semantics are selected. They are covered by fallback
  regression so direct Oxc lowering cannot silently map Terlan `try/of/catch`
  behavior onto JavaScript `try/catch/finally`.
- Message-send expressions such as `target ! message` remain unsupported in
  `emit-js` until JavaScript target process/mailbox semantics are selected.
  They are covered by fallback regression so direct Oxc lowering cannot
  silently invent actor delivery behavior.
- Quote/unquote expressions remain unsupported in `emit-js` until JavaScript
  target macro-AST runtime semantics are selected. Quote and unquote bodies are
  covered by fallback regressions so runtime-boundary macro values cannot
  silently lower to ad hoc JavaScript objects or splices.
- `emit-js` does not render inline HTML blocks or HTML templates. Inline
  `html { ... }` blocks are covered by fallback regression.
  `CoreExpr::TemplateInstantiate` currently lowers to a plain JavaScript object
  with template prop names; static HTML rendering remains owned by the
  static-site command path.
- The command-facing Oxc backend facade must keep fallback behavior explicit:
  if direct AST lowering declines a module, unsupported public function bodies
  continue to emit the existing bootstrap JS stub instead of failing `emit-js`.
- Direct Oxc lowering emits private Terlan functions as local JavaScript
  declarations when they are reachable from public functions and fit the direct
  subset. Only public Terlan functions are wrapped in JavaScript exports, and
  unreachable private functions do not force direct lowering to fall back.
- Oxc parser/codegen crates may be imported by JS backend implementation files
  and JS backend tests only.
- `formal-cli-js-gate` enforces this boundary through JS backend Rust tests
  that exercise Oxc parser/codegen validation.
- Do not import Oxc crates from `formal_pipeline`, `validation`,
  `terlan_typeck`, `terlan_hir`, `terlan_syntax`, or any CoreIR model file.
- Keep formal gates focused on CoreIR contracts and emitted artifact behavior;
  they should not depend on Oxc AST shapes.

The command should remain under the command size limit and keep helper functions
close to the transformation behavior they implement.
