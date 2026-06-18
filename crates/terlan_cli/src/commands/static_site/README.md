# CLI Static Site Command Internals

This directory owns the `terlc emit-static` and `terlc serve-static` command
runtimes. The implementation in `mod.rs` is centered on static HTML generation,
asset copying, local development serving, and live-reload support.

## Responsibilities

- Parse `emit-static` and `serve-static` command-local arguments.
- Compile source modules through the formal compiler phases.
- Render static entrypoints and routes into HTML files.
- Copy file/CSS imports with include/exclude filtering.
- Serve generated files over HTTP and broadcast live-reload events.

## Public Surface

- `run_emit_static`: command entry point for `terlc emit-static`.
- `run_serve_static`: command entry point for `terlc serve-static`.
- `parse_emit_static_args`: typed parser for static emit arguments.
- `parse_serve_static_args`: typed parser for static serve arguments.
- `html_usage`: detects syntax-output HTML usage for runtime emission.
- `render`: static syntax-output and external-template HTML rendering.
- `routes`: static entrypoint discovery, route parsing, route validation, and
  route output path mapping.
- `AssetFilters`, `EmitStaticArgs`, and `ServeStaticArgs`: parsed command data
  used by focused tests.

Public methods or values exposed to callers include `run_emit_static`,
`run_serve_static`, `parse_emit_static_args`, `parse_serve_static_args`,
`static_request_path`, `inject_reload_script`, and `directory_fingerprint`.

## Core Model

`emit-static` is a one-shot static generation command. `serve-static` reuses
that command path, then starts a minimal HTTP server and polling loop for local
development.

The main flow is:

1. Parse command-local arguments and asset filters.
2. Compile the source module through compiler phases.
3. Discover static entrypoints and routes from syntax output.
4. Render HTML, copy assets, and optionally validate output.
5. For dev serving, serve output files and broadcast reloads after changes.

Important invariants:

- `serve-static` uses the same static emit path as `emit-static`.
- Asset filters apply to both full normalized paths and filenames.
- Request paths reject traversal-like segments.
- HTML responses receive a reload script unless already present.

## Lifecycle

`main.rs` creates `CliCommand` and `CliState`, then transfers ownership to the
command entry point. `emit-static` exits after one render pass. `serve-static`
owns a long-running server thread, reload client list, and polling loop.

## Scheduling And Ordering

- Source compilation happens before output directory writes.
- Static route validation happens before rendering.
- Asset copying happens before CSS validation.
- Dev-server polling recompiles on source changes before broadcasting output
  reload changes.

## Data Structures

- `AssetFilters`: include/exclude wildcard filters.
- `EmitStaticArgs`: one-shot static generation settings.
- `ServeStaticArgs`: dev-server settings plus embedded emit-static settings.
- Reload client list: senders for active server-sent-event connections.

## Integration Points

- `main.rs`: routes commands into this module.
- `html_usage`: tells regular `emit` when generated Erlang needs the HTML
  runtime module.
- `render`: renders formal syntax-output modules, external templates,
  component tags, slots, and static Markdown HTML.
- `routes`: owns static route discovery and validation for formal syntax-output
  modules.
- Static output validation: checks generated HTML and copied CSS assets.
- `terlan_syntax`: syntax output supplies imports, entrypoints, routes, and
  template data.

## Edge Cases

- Missing/duplicate path arguments return exit code `2`.
- Invalid serve port or zero polling interval returns exit code `2`.
- Route, render, asset copy, validation, and write failures return exit code `1`.
- Static server bind failures return exit code `1`.

## Destruction And Cleanup

`emit-static` owns no long-lived resources. `serve-static` is intentionally
long-running and relies on process exit to stop server and polling threads.

## Types And Interfaces

`AssetFilters`
: Include/exclude wildcard filters used while copying static imports.

`EmitStaticArgs`
: Parsed static generation command settings.

`ServeStaticArgs`
: Parsed static development server settings.

## Testing Notes

- Existing focused static tests still live in the large `main.rs` test module
  while helper extraction is pending.
- Tests import this module for argument parsing and HTTP/path helper coverage.
