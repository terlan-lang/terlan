# CLI Artifact Helpers

This module owns shared artifact and dependency helpers used by command modules
after formal syntax-output compilation.

## Responsibilities

- Encode and decode incremental dependency manifests.
- Fingerprint source, interface, file import, template, and Markdown inputs.
- Resolve source-relative import paths.
- Collect file, CSS, Markdown, and normalized template frontend inputs needed
  by emit/static flows.

## Public Surface

- `DependencyManifest`: incremental dependency fingerprint record.
- `read_manifest`: reads and decodes a dependency manifest.
- `collect_syntax_dependency_hashes`: derives dependency fingerprints.
- `collect_syntax_file_import_bytes`: loads file and CSS imports.
- `collect_syntax_markdown_inputs`: loads and parses Markdown imports.
- `collect_syntax_template_frontend_inputs`: resolves template declarations,
  parses external templates, and preserves declaration spans/props.
- `collect_syntax_template_inputs`: loads and parses external templates.
- `resolve_import_path`: resolves source-relative paths.
- `fingerprint`: stable local hash helper for CLI caches.

## Core Model

Commands pass already parsed formal syntax output into this module. The module
does not parse CLI flags, print diagnostics, or exit. It returns structured data
or user-facing error strings for the command to handle.

## Integration Points

- `emit`: loads imports/templates/Markdown and writes dependency manifests.
- `check`: computes cache invalidation fingerprints.
- `static_site`: loads templates and Markdown for static rendering.
- `template_contract`: consumes normalized template frontend inputs for
  validation.

## Testing Notes

Current coverage is through command integration tests plus focused module-local
coverage for normalized template frontend inputs. Add more module-local tests if
manifest encoding or dependency hashing becomes more complex.
