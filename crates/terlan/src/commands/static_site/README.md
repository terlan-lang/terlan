# CLI Static Site Command Internals

This directory owns the public `terlc static` command group and the internal
static emit/serve runners it delegates to. The implementation in `mod.rs` is
centered on static HTML generation, asset copying, local development serving,
and live-reload support.

## Responsibilities

- Parse `static emit`, `static serve`, and `static check` command-local
  arguments.
- Compile source modules through the formal compiler phases.
- Render static entrypoints and routes into HTML files.
- Copy file/CSS imports with include/exclude filtering.
- Inject an optional static base path for project-prefix hosts such as GitHub
  Pages.
- Serve generated files over HTTP and broadcast live-reload events.
- Validate static generation through a check-only serve path for CI.

## Public Surface

- `run`: public command entry point for `terlc static`.
- `command`: public `terlc static <emit|serve|check>` adapter.
- `filters`: asset include/exclude filter type and wildcard matcher.
- `run_emit_static`: internal command entry point for static emission.
- `run_serve_static`: internal command entry point for static development
  serving.
- `parse_emit_static_args`: typed parser for static emit arguments.
- `parse_serve_static_args`: typed parser for static serve arguments.
- `html_usage`: detects syntax-output HTML usage for runtime emission.
- `render`: static syntax-output and external-template HTML rendering.
- `render_lookup`: shared static template declaration and prop lookup helpers.
- `render_markdown`: Markdown layout rendering and Markdown field access
  rendering.
- `render_values`: static template value model, slot lookup, literal decoding,
  and HTML escaping.
- `routes`: static entrypoint discovery, route parsing, route validation, and
  route output path mapping.
- `AssetFilters`, `EmitStaticArgs`, and `ServeStaticArgs`: parsed command data
  used by focused tests.

Public methods or values exposed to callers include `run_emit_static`,
`run_serve_static`, `parse_emit_static_args`, `parse_serve_static_args`,
`static_request_path`, `inject_reload_script`, and `directory_fingerprint`.

## Core Model

`static emit` is a one-shot static generation command. `static serve` reuses
that command path, then starts a minimal HTTP server and polling loop for local
development. `static check` delegates to the same serve runner with `--check`
and `--validate-output`, rendering and validating once without binding a
socket.

The main flow is:

1. Parse command-local arguments and asset filters.
2. Compile the source module through compiler phases.
3. Discover static entrypoints and routes from syntax output.
4. Render HTML, copy assets, and optionally validate output.
5. Optionally inject `<base href="...">` when `--base-path` is supplied.
6. For dev serving, serve output files and broadcast reloads after changes.

Important invariants:

- `static serve` uses the same static emit path as `static emit`.
- `static check` must never start the server or watcher.
- Asset filters apply to both full normalized paths and filenames.
- `--base-path` is normalized to a slash-prefixed, slash-terminated path and is
  applied only to generated HTML, not copied assets.
- Request paths reject traversal-like segments.
- HTML responses receive a reload script unless already present.

## Lifecycle

`main.rs` creates `CliCommand` and `CliState`, then transfers ownership to the
command entry point. `static emit` exits after one render pass. `static check`
exits after one render pass in check mode. `static serve` owns a long-running
server thread, reload client list, and polling loop.

## Scheduling And Ordering

- Source compilation happens before output directory writes.
- Static route validation happens before rendering.
- Static route declarations are accepted only after the formal Terlan parser
  has emitted syntax-output config declarations; this module may parse that
  parser-preserved route text as a manifest bridge, but must not read source
  files or become an independent route grammar.
- Asset copying happens before CSS validation.
- Dev-server polling recompiles on source changes before broadcasting output
  reload changes.

## Data Structures

- `AssetFilters`: include/exclude wildcard filters.
- `EmitStaticArgs`: one-shot static generation settings.
- `ServeStaticArgs`: dev-server settings plus embedded static emit settings.
- Reload client list: senders for active server-sent-event connections.

## Integration Points

- `main.rs`: routes commands into this module.
- `html_usage`: tells regular `emit` when generated Erlang needs the HTML
  runtime module.
- `render`: renders formal syntax-output modules, external templates,
  component tags, slots, and inline static HTML.
- `render_markdown`: adapts Markdown documents into static layout template
  values and renders imported Markdown HTML field access.
- `render_values`: owns compile-time-renderable value conversion and HTML
  escaping helpers shared by inline and external template renderers.
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

`static emit` and `static check` own no long-lived resources. `static serve` is
intentionally long-running and relies on process exit to stop server and
polling threads.

## Types And Interfaces

`AssetFilters`
: Include/exclude wildcard filters used while copying static imports.

`EmitStaticArgs`
: Parsed static generation command settings.

`ServeStaticArgs`
: Parsed static development server settings for the delegated serve runner.

## Testing Notes

- Focused static-site tests live beside this module in `mod_test.rs`,
  `render_test.rs`, and `routes_test.rs`.
- Public CLI help tests assert that `terlc static` is the advertised command
  surface and that hidden dispatch verbs stay out of user-facing help.
- `make static-profile-preflight` exercises a scaffolded static project through
  `terlc static emit` and `terlc static check`.
