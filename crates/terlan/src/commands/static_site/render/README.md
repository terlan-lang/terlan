# Static Site Render Internals

This directory owns static-site template rendering helpers.

## Responsibilities

- Render checked template source into static output.
- Keep evaluation, lookup, Markdown, and HTML rendering responsibilities split.
- Fail without writing partial artifacts when template evaluation is invalid.

## Public Surface

- Module-local render helpers consumed by `commands::static_site`.

## Integration Points

- `html`: supplies parsed template structures.
- `commands::static_site`: discovers source files and writes output.

## Testing Notes

- Add adjacent render tests for every new template expression form.
- Prefer small fixtures that show the input template and exact rendered output.
