# Static Output Validation Internals

This module owns validation for generated static-site artifacts. The
implementation in `mod.rs` is centered on delegating HTML and CSS checks to
`terlan_html` while preserving CLI-oriented path context in error messages.
It is called only after static output has already been rendered or copied.

## Responsibilities

- Validate generated HTML files before `terlc static emit --validate-output`
  succeeds.
- Validate copied CSS imports that are emitted as static assets.
- Convert structured `terlan_html` diagnostics into newline-separated CLI
  messages.
- Keep validation side effects limited to reading generated CSS files.

## Public Surface

- `validate_static_html_output`: validates one rendered HTML string against its
  target path.
- `validate_static_css_output_files`: validates generated or copied CSS output
  files by path.

Public values exposed to callers include only these two functions.

## Core Model

The module has no persistent state. HTML validation receives the generated text
directly. CSS validation receives paths, reads each file, and delegates parsing
to `terlan_html`.

The main flow is:

1. `terlc static emit` renders or copies static artifacts.
2. The command calls the appropriate validator when `--validate-output` is set.
3. The validator delegates to `terlan_html`.
4. Diagnostics are formatted with file paths and returned to the command.

Important invariants:

- HTML validation must report the generated target path, not only the source
  module path.
- CSS validation must validate CSS imports, but raw `import file` assets remain
  outside this validator.
- Validators return messages and never print or exit.

## Integration Points

- `run_emit_static`: validates generated route and entrypoint HTML output.
- `copy_syntax_static_asset_imports`: supplies copied CSS output paths.
- `terlan_html`: owns HTML and CSS parsing/validation diagnostics.

## Edge Cases

- Missing CSS output files return a read error with the file path.
- Multiple diagnostics are joined with newlines for CLI display.
- Diagnostics without a path still return the diagnostic message.

## Testing Notes

- `reports_static_html_output_validation_diagnostics_with_generated_path`
  protects generated HTML path reporting.
- `validates_static_css_outputs_in_output_dir` protects valid CSS acceptance.
- `reports_static_css_output_validation_diagnostics_with_generated_path`
  protects copied CSS path reporting.
- `run_emit_static_validate_output_validates_copied_css_imports` protects the
  command-level integration with copied CSS imports.
