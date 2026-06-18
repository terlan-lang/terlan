# Terlan HTML Source Internals

This directory owns Terlan HTML and Markdown template parsing. It converts
`.terl.html` and `.terl.md` files into a small structured template model used
by compiler packaging and static-site paths.

## Responsibilities

- Recognize Terlan template file suffixes.
- Parse HTML templates into nodes, attributes, comments, doctypes, and slots.
- Render Markdown templates to HTML before parsing the resulting nodes.
- Validate template names and report path-aware diagnostics.

## Public Surface

- `parse_template`: parses HTML or Markdown Terlan templates by suffix.
- `parse_html_template`: parses `.terl.html` templates.
- `parse_markdown_template`: parses `.terl.md` templates.
- `HtmlTemplate`, `HtmlNode`, `HtmlElement`, and `HtmlSlot`: structured
  template model.

## Core Model

The parser keeps template structure target-neutral. Slots are represented as
explicit path values, and diagnostics carry source path context where possible.
This crate does not own browser packaging, JavaScript emission, or HTTP
serving.

The main flow is:

1. Validate the template filename and suffix.
2. Parse Markdown to HTML when needed.
3. Tokenize HTML and build structured nodes.
4. Return a template or a list of diagnostics.

Important invariants:

- Template filenames must end in `.terl.html` or `.terl.md`.
- Template tag names are derived deterministically from file paths.
- Parser diagnostics must not emit partial templates on failure.

## Integration Points

- `html5ever`: owns HTML tokenization.
- `comrak`: owns Markdown rendering.
- `cssparser`: validates CSS-shaped values where needed.
- CLI static-site/browser packaging code consumes parsed templates.

## Edge Cases

- Non-UTF-8 template filenames are rejected.
- Invalid template names produce path-aware diagnostics.
- Markdown rendering errors must be surfaced before packaging artifacts are
  written.

## Types And Interfaces

`HtmlTemplate`
: Parsed template with optional source path and generated tag name.

`HtmlNode`
: Structured template node enum.

`HtmlDiagnostic`
: Path-aware template parsing diagnostic.

## Testing Notes

- Template parsing tests should remain adjacent to this crate.
- Add focused tests for path normalization, slot parsing, Markdown rendering,
  and malformed HTML diagnostics.
- Browser packaging tests should cover integration separately in the CLI crate.
