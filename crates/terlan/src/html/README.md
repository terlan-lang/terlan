# Terlan HTML Source Internals

This directory owns Terlan artifact-template target classification, typed
template data, and the current HTML and Markdown template parsers. It converts `.terl.html` and
`.terl.md` files into a small structured template model used by compiler
packaging and static-site paths. JSON, TOML, YAML, and text template suffixes
are recognized here so validators share one target contract. JSON structure
validation is implemented with `serde_json`; TOML validation is implemented
with `basic-toml`; text templates validate interpolation boundaries while
leaving text content otherwise free-form.

## Responsibilities

- Recognize Terlan artifact-template file suffixes.
- Parse HTML templates into nodes, attributes, comments, doctypes, and slots.
- Render Markdown templates to HTML before parsing the resulting nodes.
- Validate template names and report path-aware diagnostics.
- Validate `.terl.json` static structure while accounting for `${...}`
  interpolation islands.
- Validate `.terl.toml` static structure while accounting for `${...}`
  interpolation islands.
- Validate `.terl.yaml` and `.terl.yml` static structure while accounting for
  `${...}` interpolation islands.
- Validate `.terl.txt` interpolation island boundaries.
- Inject static-site `<base href>` metadata through the shared HTML boundary.
- Escape generated HTML text nodes and attribute values through the shared HTML
  boundary.

## Public Surface

- `artifact`: owns artifact-template suffix constants and target
  classification.
- `parse_template`: parses HTML or Markdown Terlan templates by suffix.
- `parse_html_template`: parses `.terl.html` templates.
- `parse_markdown_template`: parses `.terl.md` templates.
- `artifact_template_target_from_path`: classifies `.terl.html`, `.terl.md`,
  `.terl.json`, `.terl.toml`, `.terl.yaml`, `.terl.yml`, and `.terl.txt`.
- `ArtifactTemplateTarget`: stable target enum for template discovery and
  diagnostics.
- `inject_html_base_path`: injects static-site base metadata into generated
  HTML without letting command modules own HTML mutation details.
- `escape_html_text` and `escape_html_attr`: shared escaping helpers for
  generated HTML text and attribute contexts.
- `validate_artifact_template_structure`: dispatches structure validation by
  artifact-template suffix.
- `validate_json_template_structure`: validates `.terl.json` structure after
  masking interpolation islands.
- `validate_toml_template_structure`: validates `.terl.toml` structure after
  masking interpolation islands.
- `validate_yaml_template_structure`: validates `.terl.yaml` / `.terl.yml`
  structure after masking interpolation islands.
- `validate_text_template_structure`: validates `.terl.txt` interpolation
  boundaries.
- `HtmlTemplate`, `HtmlNode`, `HtmlElement`, and `HtmlSlot`: structured
  template model.

## Core Model

The parser keeps template structure target-neutral. Slots are represented as
explicit path values, and diagnostics carry source path context where possible.
This crate does not own browser packaging, JavaScript emission, or HTTP
serving.

The main flow is:

1. Classify the artifact-template filename suffix.
2. Validate the HTML/Markdown template filename and suffix when parsing to an
   HTML tree.
3. Parse Markdown to HTML when needed.
4. Tokenize HTML and build structured nodes.
5. Return a template or a list of diagnostics.

For structure-only artifact validation:

1. Classify the suffix with `ArtifactTemplateTarget`.
2. Delegate `.terl.html` and `.terl.md` to existing template parsers.
3. Delegate `.terl.json` to `serde_json` after interpolation masking.
4. Delegate `.terl.toml` to `basic-toml` after interpolation masking.
5. Delegate `.terl.yaml` / `.terl.yml` to `yaml-rust` after interpolation
   masking.
6. Validate `.terl.txt` interpolation boundaries while accepting arbitrary
   surrounding text until expression-island typechecking is implemented.

Important invariants:

- HTML-tree template filenames must end in `.terl.html` or `.terl.md`.
- Artifact-template discovery also recognizes `.terl.json`, `.terl.toml`,
  `.terl.yaml`, `.terl.yml`, and `.terl.txt`.
- Template tag names are derived deterministically from file paths.
- Parser diagnostics must not emit partial templates on failure.

## Integration Points

- `html5ever`: owns HTML tokenization.
- `comrak`: owns Markdown rendering.
- `cssparser`: validates CSS-shaped values where needed.
- `serde_json`: validates JSON artifact-template structure.
- `basic-toml`: validates TOML artifact-template structure.
- `yaml-rust`: validates YAML artifact-template structure.
- CLI static-site/browser packaging code consumes parsed templates.

## File Layout

- `artifact.rs`: target suffix constants and artifact-template classification.
- `artifact_test.rs`: artifact-template target tests.
- `base_path.rs`: generated HTML base-path injection helper.
- `base_path_test.rs`: base-path injection tests.
- `escaping.rs`: generated HTML escaping helpers.
- `escaping_test.rs`: escaping helper tests.
- `lib.rs`: public model types, tag helpers, and crate re-exports.
- `lib_test.rs`: HTML/Markdown parser tests.
- `parser.rs`: HTML/Markdown parsing, CSS validation, and interpolation slot
  parsing.
- `structured.rs`: structured artifact-template validators.
- `structured_test.rs`: structured validator tests.

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
