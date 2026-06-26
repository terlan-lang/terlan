# Std Template Internals

This directory owns the portable source-level template API used by HTML and
artifact-template compilation. Concrete parsing, escaping, validation, and
rendering are compiler-owned; source code works with typed template fragment
values instead of untyped strings.

## Responsibilities

- Define the trusted HTML fragment type used by template interpolation.
- Provide explicit helpers for trusted conversion and fragment joining.
- Keep raw HTML escape hatches rare and visible in source.
- Preserve a target-neutral path for Markdown, JSON, YAML, TOML, and text
  artifact templates.

## Public Surface

- `std.template.Template.Html`: opaque trusted HTML fragment.
- `std.template.Template.trusted`: explicit trusted HTML conversion.
- `std.template.Template.empty`: empty trusted HTML fragment.
- `std.template.Template.join`: concatenates trusted fragments.

## Core Model

Plain `String` interpolation is escaped as text by the template compiler.
`Html` is the separate type for values that are already trusted to contain HTML.
The standard library only declares the source-facing shape; the compiler owns
escaping rules, diagnostics, generated template functions, and final output.

Important invariants:

- `String` is not implicitly trusted HTML.
- `trusted` is explicit and should be rare.
- `join` accepts already trusted fragments only.
- Template helpers do not introduce template-specific control flow.

## Integration Points

- `terlan_html`: parses and validates template source files.
- `terlc emit-static`: renders static template output.
- `std.http.Response`: later accepts rendered template output for HTML
  responses.
- Tree-sitter/editor tooling: highlights mixed Terlan/template regions.

## Edge Cases

- User-provided strings must be escaped before entering HTML output unless they
  are explicitly converted through `trusted`.
- Optional and result values must be handled by user code before interpolation.
- Lists do not auto-render; callers join trusted fragments explicitly.

## Types And Interfaces

`Html`
: Opaque trusted HTML fragment type.

`trusted`
: Explicit conversion from `String` to trusted `Html`.

`empty`
: Empty trusted fragment used by generated templates and helpers.

`join`
: Concatenates trusted fragments in source order.

## Testing Notes

- Positive source tests live beside the module as `TemplateTest.terl`.
- Runtime rendering tests live in the compiler/template command tests.
