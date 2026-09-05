# Changelog

User-facing features, behavior changes, compatibility, and security updates
are tracked here. Internal engineering work belongs in maintainer documentation.

## Unreleased

## 0.0.8

A maintenance and security release for the native AOT platform introduced in
0.0.7.

- Fix incorrect results and crashes in AOT-compiled applications and scripts
  using short-circuit Boolean expressions with I/O.
- Strengthen native-worker isolation by preventing unintended access to
  inherited file descriptors.
- Remove vulnerable OpenTelemetry dependencies. Tracing and OTLP support are
  temporarily unavailable in the Foundations adapter; logging and metrics
  remain supported.

## 0.0.7

- Adopt ahead-of-time compilation and `terlan-vm` as the native
  execution path for applications, tests, and the REPL, replacing the
  interpreter-based runtime.
- Run actors concurrently across multiple CPU cores.
- Add executable `.terls` scripts with top-level expressions, no explicit
  `main` function, and inline assertions that report failures to the caller.
- Package the compiler, VM, native worker, standard library, and editor
  support together for Linux, macOS, and Windows on x86_64 and ARM64.
- Remove legacy Erlang/OTP target options and move VM-facing standard-library
  APIs to `std.vm`.
- Add `terlc migrate pattern-head` as a dry-run-first migration assist for
  rejected reverse-alias function-head patterns, with `--write` and `--json`
  modes.

## 0.0.6

- Add an experimental `terlan-vm` distribution alongside the existing
  Erlang/OTP runtime.

## 0.0.5

- Add static-site project scaffolding and `terlc static` commands for emitting,
  checking, and serving static pages from Terlan source, templates, Markdown,
  and imported assets.
- Add typed template validation for HTML, Markdown, JSON, YAML, TOML, and text
  artifact templates, including escaped interpolation and component prop checks.
- Improve HTTP serving, route matching, live
  reload, structured handler logs, dev error pages, cookies, and response
  metadata.
- Add TLS configuration support for manual certificates, local internal TLS,
  and ACME planning with Let's Encrypt defaults and ZeroSSL fallback metadata.
- Add `std.db.Postgres`, typed SQL form validation, and `terlc db` migration
  commands.
- Expand `std.js` bindings for JavaScript and browser DOM APIs.
- Add editor packages for VS Code, Neovim, Emacs, IntelliJ, shared Terlan file
  icons, and Terlan language-server support.
- Add `std.sync.Resource`, `std.log`, `std.template`, `std.http.Router`,
  `std.http.Tls`, and `std.core.Object` release surfaces.

## 0.0.4

- Add the experimental JavaScript build target for library-style ES module
  output through `terlc build --target js`.
- Add explicit JavaScript target profiles for shared, browser, and worker
  output validation.
- Add generated `std.js` bindings for the first standard JavaScript surface:
  `String`, `Array`, `Promise`, `Dom.Document`, and `Dom.HTMLElement`.
- Add browser packaging with `terlc build --target js.browser`, producing a
  runnable `_build/web` artifact with JavaScript modules, imported assets, and
  manifest-declared static assets.
- Add `terlc serve` for local validation and serving of packaged web artifacts.
- Add `terlc init --profile web` to scaffold a minimal browser module, HTTP
  handler module, web assets directory, and project manifest.
- Add `std.http.Request`, `std.http.Response`, `std.http.Error`, and
  `std.data.Json` as the first HTTP/JSON standard-library surface for web
  handlers.
- Add target-profile diagnostics that reject JavaScript-only standard-library
  imports on non-JavaScript targets.

## 0.0.3

- Promote `.terl` as the canonical Terlan source extension and `.terli` as the
  interface extension.
- Improve `terlc init`, `terlc build`, `terlc test`, `terlc repl`, command help,
  and version reporting.
- Add generated standard-library documentation.
- Expand implicit prelude support for core types and target-neutral type
  introspection.
- Add typed `std.core.Error` and derive-aware error inheritance.

## 0.0.2

- Publish the 0.0.2 language-feature and base-standard-library release.
- Add semicolon-separated expression sequencing support for function bodies.
- Add receiver methods, mutable receiver command-style calls, and receiver-aware
  pipe dispatch.
- Add trait conformance support through `implements`, explicit `impl` blocks,
  trait default methods, and generic-bound dispatch.
- Add function-value invocation with `f.(args)`.
- Add portable `Atom["name"]` singleton aliases.
- Expand `std.core` support for `Unit`, `Option`, `Result`, `Ordering`,
  `Bool`, `Int`, `Float`, and `String`.
- Add `std.collections` APIs for `List`, `Map`, `Set`, `Iterable`,
  `Iterator`, and `Enumerable`.
- Add `std.io.File` text APIs and constrained negative diagnostics for invalid
  `std.io` calls.
- Remove redundant early std modules and aliases that are not part of the
  release surface: `std.core.Atom`, `std.core.Function`, `std.core.Identity`,
  and `std.test.Test.assert`.

## 0.0.1

- Publish the first usable-program milestone.
- Include `terlc init` and `terlc build` for applications running on Erlang/BEAM.
- Include initial `std.core` and `std.io` support for simple programs.
- Add release packaging for Linux x86_64.
