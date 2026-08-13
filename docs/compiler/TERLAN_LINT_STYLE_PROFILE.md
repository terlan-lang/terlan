# Terlan Lint Style Profile

The Terlan lint profile is the opinionated style contract for source code that
is valid but harder to maintain than it should be. The formatter owns layout.
Lint owns semantic style diagnostics and safe semantic rewrites.

The profile is synthesized from Google-style large-codebase style-guide principles:
clarity, simplicity, concision, maintainability, and consistency. Terlan keeps
those principles, but Terlan syntax, target inference, explicit imports,
functional control flow, and VM ownership rules override language-specific
rules from any source style guide.

## Command Surface

- `terlc lint <file.terl|file.terli|dir>` reports diagnostics only.
- `terlc lint --fix <file.terl|file.terli|dir>` applies only proven-safe rewrites.
- `terlc fmt` must not perform semantic call rewrites.

## Severity Policy

- `error`: correctness risks, misleading tests, unsafe target use, unsafe
  generated-code use, or repository policy violations.
- `warning`: maintainability, confusing control flow, risky naming, hidden
  side effects, or fragile API shape.
- `suggestion`: equivalent readability improvements such as safe pipe
  canonicalization.

## Diagnostic Contract

Every lint diagnostic must have:

- stable rule ID
- severity
- file and span
- short explanation
- fix availability marker: `fix-safe`, `fix-unsafe`, or `fix-unavailable`

Rule IDs use `TL` plus a four-digit number, for example `TL0001`.

## Rule Families

### Readability

Purpose: keep ordinary Terlan code easy to scan.

Initial rules:

- `TL0001 readability.semicolon-chain`: reject dense same-line semicolon
  expression chains after formatting.
- `TL0002 readability.deep-expression`: flag deeply nested expression trees
  when a named `let`, helper, or pipe would clarify intent.
- `TL0003 readability.callback-name`: require meaningful callback parameter
  names when the callback has multiple expressions or captures outer state.
- `TL0004 readability.unused-destructure-binding`: require destructured names
  to be used or explicitly marked as intentionally ignored.
- `TL0005 readability.redundant-comment`: reject comments that merely restate
  the following expression.
- `TL0006 readability.public-docs`: require documentation on public source
  declarations outside test-only modules.
- `TL0007 readability.doc-comment-spacing`: enforce canonical block-doc star
  spacing.
- `TL0008 readability.boolean-heavy-branch`: require complex boolean branch
  conditions to be factored into `case`, guards, or a named predicate.
- `TL0009 readability.grouped-binding`: require two-or-more-deep linear
  refutable `case` chains with one structurally repeated fallback to use
  grouped `let { ... } else { ... }` bindings. Distinct failure behavior stays
  explicit and receives no diagnostic. The lint fix remains unavailable;
  `terlc fmt` performs the guarded syntax-tree rewrite only when capture and
  evaluation-order preservation are proven structurally.
- `TL0010 readability.function-reference`: reject a lambda whose sole clause
  forwards every simple parameter, unchanged and in order, to one named local,
  selected-import, or module function. Use the named function value directly;
  the typechecker verifies its arity, contravariant parameter compatibility,
  and covariant result compatibility at the receiving callback type. `terlc
  fmt` performs this canonical rewrite; transformed or reordered arguments,
  guards, captures, and multi-clause lambdas remain explicit.

### Imports

Purpose: keep source dependencies explicit and target-aware.

Initial rules:

- `TL0101 imports.unused`: reject imports proven unused by name resolution.
- `TL0102 imports.duplicate`: reject duplicate imports and duplicate selected
  imports.
- `TL0103 imports.target-mismatch`: reject `std.js.*`, `std.wasm.*`,
  native-boundary, or VM-only imports in incompatible targets.

### Naming

Purpose: keep Terlan names stable, searchable, and target-boundary aware.

Initial rules:

- `TL0201 naming.snake-case`: enforce lower-snake-case for functions, values,
  and fields owned by Terlan source.
- `TL0202 naming.upper-camel`: enforce UpperCamelCase for modules, types, and
  constructors owned by Terlan source.
- `TL0203 naming.foreign-boundary`: preserve foreign names only at generated
  interop boundaries.

### Docs

Purpose: make public APIs usable through docs and editor hover.

Initial rules:

- `TL0301 docs.public-api`: require public modules, public types, public
  structs, public traits, constructors, public functions, and receiver methods
  to have documentation.
- `TL0302 docs.generated-binding`: require generated bindings to keep source
  manifest documentation.
- `TL0303 docs.drift`: flag public API docs that diverge between source,
  generated summaries, and README material.

### Tests

Purpose: make tests prove behavior instead of checking that code exists.

Initial rules:

- `TL0401 tests.fake`: reject `assert(true)`, identity assertions, and
  declaration-only tests.
- `TL0402 tests.table-needed`: suggest table tests for repeated input/output
  examples.
- `TL0403 tests.property-needed`: suggest property tests for roundtrip,
  ordering, parser/renderer, generated-value, collection invariant, and
  serialization APIs.

### Std

Purpose: keep standard-library APIs covered and release-visible.

Initial rules:

- `TL0501 std.release-api-coverage`: require release API coverage metadata for
  std modules.
- `TL0502 std.adversarial-coverage`: require adversarial coverage for parsers,
  validators, generated-value behavior, and collection invariants.
- `TL0503 std.example-coverage`: require executable examples or tests for
  public std APIs.

### Effects

Purpose: keep side effects explicit and VM-owned.

Initial rules:

- `TL0601 effects.hidden-ordering`: flag hidden side-effect ordering inside
  dense expressions.
- `TL0602 effects.pure-violation`: flag impure bodies under `@pure`.
- `TL0603 effects.native-boundary`: reject NativeBoundary calls without
  explicit resource or capability ownership.

### Targets

Purpose: keep compiler target inference predictable.

Initial rules:

- `TL0701 targets.inference-ambiguous`: reject target inference when imports
  and types do not make the target unambiguous.
- `TL0702 targets.incompatible-std`: reject target-specific std packages in
  incompatible modules.
- `TL0703 targets.abi-noise`: flag redundant target annotations when imported
  ABI types already imply the target.

### Interop

Purpose: keep generated bindings complete, reproducible, and honest.

Initial rules:

- `TL0801 interop.skip-manifest`: require generated binding skip manifests for
  unsupported source constructs.
- `TL0802 interop.handwritten-wrapper`: reject handwritten wrappers where a
  generator is expected unless an exemption is documented.
- `TL0803 interop.unsupported-diagnostic`: require stable diagnostics for
  unsupported generated declarations.
- `TL0804 interop.generated-source-manifest`: require generated files to carry
  source manifest metadata.
- `TL0805 interop.generated-lint-suppression`: require generated lint
  suppressions to use structured manifest metadata instead of inline comments.

### Complexity

Purpose: keep implementation and tests maintainable.

Initial rules:

- `TL0901 complexity.function-size`: flag functions over configured size or
  complexity thresholds.
- `TL0902 complexity.file-size`: flag files over configured size thresholds
  unless covered by a generated-file policy.
- `TL0903 complexity.match-arm-size`: flag oversized match/case arms that need
  named helpers.

### Format Boundary

Purpose: keep the formatter deterministic and semantic rewrites in lint.

Initial rules:

- `TL1001 format-boundary.semantic-fmt`: reject formatter behavior that rewrites
  ordinary call topology into pipe form.
- `TL1002 format-boundary.pipe-fix`: offer pipe canonicalization only through
  lint when the compiler proves the rewrite is behavior-preserving.
- `TL1003 format-boundary.semicolon-split`: flag semicolon-separated expression chains
  that remain dense after formatting and require one expression per line when
  side effects, assertions, mutations, or test setup steps are present.

Policy boundary:

- pipe canonicalization belongs to lint, because it can change call topology
  and needs semantic proof before `--fix` may apply it.
- fmt may split semicolon chains when it can preserve the exact expression
  order and does not need semantic proof.

## Pipe Canonicalization

Lint may suggest pipe form when the inner call result is passed as the first
argument to the outer call:

```terl
Iterator.each(Set.iterator(collection), cb).
```

Suggested form:

```terl
collection
    |> Set.iterator()
    |> Iterator.each(cb).
```

`terlc lint --fix` must reject:

- named-argument ambiguity
- default-argument ambiguity
- function-value calls
- nested argument contexts
- side-effect-sensitive duplicated expressions
- target-specific intrinsic calls unless proven safe
