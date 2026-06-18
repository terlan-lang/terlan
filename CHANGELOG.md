# Changelog

All notable release-facing changes to Terlan are tracked here.

## Unreleased

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
- Add Oxc validation for emitted JavaScript before build artifacts are written.

## 0.0.3

- Promote `.terl` as the canonical Terlan source extension and `.terli` as the
  interface extension.
- Harden `terlc init`, `terlc build`, `terlc test`, `terlc repl`, command help,
  version reporting, and installed-compiler smoke coverage.
- Add REPL-backed documentation validation and generated stdlib documentation.
- Expand implicit prelude support for core types and target-neutral type
  introspection.
- Add typed `std.core.Error`, derive-aware error inheritance, and broader
  `Option`, `Result`, `Equal`, `Ordering`, `Atom`, `Unit`, and `String`
  coverage.
- Expand `std.collections` contracts and tests for `List`, `Map`, `Set`,
  `Iterable`, `Iterator`, `Enumerable`, and indexed access traits.
- Add SafeNative metadata, runtime-bridge contracts, and native package binding
  probes for Rust-backed packages.

## 0.0.2

- Publish the 0.0.2 language-feature and base-standard-library release.
- Add semicolon-separated expression sequencing support for function bodies.
- Add receiver methods, mutable receiver command-style calls, and receiver-aware
  pipe dispatch.
- Add trait conformance support through `implements`, explicit `impl` blocks,
  trait default methods, and generic-bound dispatch.
- Add function-value invocation with `f.(args)`.
- Add portable `Atom["name"]` singleton aliases.
- Expand `std.core` coverage for `Unit`, `Option`, `Result`, `Ordering`,
  `Bool`, `Int`, `Float`, and `String` with summaries and Terlan tests.
- Add `std.collections` contracts for `List`, `Map`, `Set`, `Iterable`,
  `Iterator`, and `Enumerable`, including list-backed traversal lowering.
- Add `std.io.File` text APIs and constrained negative diagnostics for invalid
  `std.io` calls.
- Generate `std/summaries/*.typi` and `.typi.deps` from Terlan std source, and
  add CI checks that reject stale committed summaries before release.
- Remove redundant early std modules and aliases that are not part of the
  release surface: `std.core.Atom`, `std.core.Function`, `std.core.Identity`,
  and `std.test.Test.assert`.
- Expand syntax, phase-contract, lowering, and standard-library test coverage
  for the released surface.

## 0.0.1

- Publish the first usable-program milestone.
- Include `terlc init`, `terlc build`, Erlang/BEAM source lowering, BEAM artifact generation, and launcher generation.
- Include initial `std.core` and `std.io` support for simple programs.
- Add release packaging for Linux x86_64.
