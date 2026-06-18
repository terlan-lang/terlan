# [terlan_html] Internals

This package owns shared HTML/template data structures for Terlan external
templates and static output. It is the boundary between compiler-native
`html { ... }`, external `*.terl.html` files, Markdown-derived HTML, and future
static output validation.

## Responsibilities

- Model parsed HTML templates with shared Rust types.
- Derive registered template tag names from `*.terl.html` filenames.
- Represent interpolation slots and attributes independently from a specific parser.
- Provide diagnostics that preserve template paths for compiler errors.

## Public Surface

- `HtmlTemplate`: parsed or constructed template with optional source path and tag name.
- `HtmlNode`: shared node enum for text, elements, comments, and slots.
- `HtmlElement`: element tag name, attributes, and children.
- `HtmlAttr`: HTML attribute name and optional value.
- `HtmlSlot`: single-brace interpolation path such as `{user.name}`.
- `template_tag_from_path`: derive a kebab-case tag from a `*.terl.html` path.
- `is_terlan_template_path`: check whether a path names a Terlan template file.

## Core Model

External Terlan templates use the `*.terl.html` suffix. The filename stem
before that suffix registers the template tag name used by other templates:

```text
user_card.terl.html -> user-card
main-layout.terl.html -> main-layout
```

The main flow is:

1. A compiler stage discovers or declares a `*.terl.html` template.
2. `template_tag_from_path` derives the HTML tag name from the filename.
3. A parser converts HTML into `HtmlTemplate` / `HtmlNode` values.
4. Later compiler stages typecheck slots and component tags against Terlan declarations.

Important invariants:

- Plain `.html` is not a Terlan template file.
- Template tag names are derived from filenames, not from front matter.
- Template metadata, routes, layouts, and props belong in `.terl` declarations.
- The shared model does not own browser runtime behavior, hydration, or DOM diffing.

## Files

- `src/lib.rs`: shared template types, diagnostics, path helpers, and tests.

## Integration Points

- `terlan_syntax` will parse template declarations and imports.
- `terlan_typeck` will validate slots, props, and component tags.
- `terlan_erlang` will lower templates to Erlang iodata.
- `terlan_cli` will use this crate for static output and asset validation.

## Edge Cases

- Non-UTF-8 template filenames are rejected with diagnostics.
- Paths not ending in `.terl.html` are not Terlan templates.
- Invalid filename characters are rejected before tag registration.
- Underscores in filenames normalize to hyphens in HTML tag names.

## Cleanup

- The crate has no global mutable state.
- Template values are plain owned Rust data structures.

## Testing Notes

- Filename-to-tag derivation is covered by crate-local unit tests.
- Future parser tests should cover malformed HTML, slot spans, attributes, and nested template tags.
