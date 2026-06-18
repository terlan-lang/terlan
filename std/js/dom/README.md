# Std JS DOM Internals

This directory owns the first generated browser DOM bindings for `std.js`.
The files here are generated from pinned TypeScript declaration inputs and are
committed so normal builds do not need network or npm discovery.

## Responsibilities

- Provide browser DOM bindings under `std.js.Dom`.
- Keep generated Terlan names idiomatic while preserving JavaScript runtime
  mappings.
- Record generated interfaces and summaries needed by import/type checking.
- Keep the seed DOM surface intentionally small and reproducible.

## Public Surface

- `std.js.Dom.Document`: generated document wrapper functions.
- `std.js.Dom.HtmlElement`: generated HTML element wrapper functions.
- `*.terli`: generated interface summaries for DOM modules.

## Core Model

The DOM generator reads pinned TypeScript declarations, maps supported shapes
into Terlan source/interfaces, and records skipped shapes in manifests. The
compiler then treats these files like any other std module while target-profile
validation enforces that DOM imports require `js.browser`.

The main flow is:

1. Parse pinned TypeScript DOM declarations through the selected JS tooling.
2. Generate Terlan module and interface files under `std/js/dom`.
3. Commit the generated outputs and manifests.
4. Validate generated outputs through stdlib and JS target gates.

Important invariants:

- DOM modules are browser-only.
- Generated files are reproducible from pinned inputs.
- Runtime JS names are preserved through backend mapping, not by leaking
  camelCase into Terlan source.

## Integration Points

- `std/js/manifests/std_js_dom_inputs.json`: records declaration inputs.
- `std/js/manifests/std_js_bindings.json`: records generated outputs.
- `std/js/manifests/std_js_skipped.json`: records skipped declarations.
- `terlan_typeck`: rejects DOM imports outside browser-capable profiles.

## Edge Cases

- Optional DOM parameters use `Option[T]`.
- Mutable DOM properties need explicit setter methods.
- Complex DOM unions remain skipped until the type model can represent them
  honestly.

## Types And Interfaces

`Document`
: Browser document wrapper generated from TypeScript DOM declarations.

`HtmlElement`
: Browser HTML element wrapper generated from TypeScript DOM declarations.

## Testing Notes

- Positive generated binding tests live beside the generated modules.
- Maintainer regeneration checks must prove committed outputs are deterministic.
- Target-profile tests must reject DOM imports from `js.shared` and non-JS
  targets.
