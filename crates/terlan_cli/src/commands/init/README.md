# CLI `init` Command Internals

This directory owns `terlc init`, the release project scaffolder.

## Responsibilities

- Create a minimal manifest-backed Terlan project.
- Use the default BEAM executable package layout unless another profile is
  selected.
- Support `--profile web` for the smallest browser-plus-handler project shape.
- Support `--profile static` for the smallest static-site project shape.
- Refuse to overwrite existing project files.
- Keep generated source target-neutral with an explicit
  `std.io.Console.{println}` import.
- Create a sample `*Test.terl` file that runs through `terlc test`.
- Create `.gitignore` and a thin Makefile for the standard project lifecycle.

## Public Surface

- `run`: command entry point called by the top-level CLI router.

## Core Model

The command accepts exactly one new project name/path and an optional profile:

```sh
terlc init hello
terlc init hello-web --profile web
terlc init docs-site --profile static
```

The command creates that directory and writes the scaffold inside it. Existing
directories are rejected, even when they do not contain Terlan files.

Generated shape:

```text
hello/
  terlan.toml
  .gitignore
  Makefile
  src/hello/Main.terl
  tests/hello/MainTest.terl
```

The generated `.gitignore` excludes compiler-owned output:

```text
_build/
.terlan/tmp/
```

The generated Makefile is intentionally small:

```sh
make
make build
make test
make run
make clean
```

Each target delegates directly to `terlc`; `make` defaults to `make build`.

The web profile adds a browser-side Terlan module and a BEAM-backed handler
module, plus the default asset directory declared by `terlan.toml`:

```text
hello-web/
  terlan.toml
  docker-compose.yml
  assets/
  templates/page.terl.html
  src/hello_web/Main.terl
  src/hello_web/Web.terl
  src/hello_web/Http.terl
  tests/hello_web/MainTest.terl
```

Package names may contain lowercase ASCII letters, digits, `_`, and `-`, and
must start with a lowercase ASCII letter. Source module roots replace `-` with
`_` because Terlan module path segments are identifiers.

The web profile keeps all Terlan source under `src` so
`terlc build --target erlang` can produce BEAM handler artifacts and
`terlc build --target js.browser` can produce the browser package. It also
creates one typed HTML template and a small `std.http.Router` example with an
exact route, parameter route, and fallback route so generated projects exercise
the intended web source surface from the start. The generated router uses the
canonical receiver-chain style (`Router.new().get(...).fallback(...)`) rather
than the older static-call helper style. It adds the minimal web asset contract:

```toml
[web.assets]
directory = "assets"
```

This is Terlan-owned project metadata. Browser packaging can translate it to
the selected Oxc/Rsbuild/Rspack boundary later without requiring users to write
direct bundler configuration. The HTTP module uses the public typed
`std.http.Router`, `std.http.Request`, and `std.http.Response` surface; internal
server bridge values are not emitted into generated user projects.

The web profile also writes a project-owned `docker-compose.yml` with the
default Postgres development service. `terlc serve --check` validates that
Compose service shape before accepting an adjacent web package; container
startup remains a later serve-runtime slice.

The static profile adds a static-site entry module, content directory, template
directory, and asset directory:

```text
docs-site/
  terlan.toml
  assets/
  content/index.terl.md
  templates/layout.terl.html
  src/docs_site/Main.terl
  src/docs_site/Site.terl
  tests/docs_site/MainTest.terl
```

The static profile intentionally reuses the supported `[web.assets]` manifest
section so generated projects remain compatible with the current project
manifest parser while static-specific configuration is still being designed.
The generated `Site.terl` can be passed to `terlc static emit` and previewed
with `terlc static serve`, which renders into `_build/web`, serves the
generated directory, and watches source changes.

## Integration Points

- `main.rs`: routes the `init` verb here.
- `terlc build`: consumes the generated `terlan.toml` and source modules.
- `terlc clean`: removes generated `_build/` and `.terlan/tmp/` output.
- `terlc serve`: serves the generated browser package for web-profile projects.
- `terlc static serve`: renders and serves static-profile projects during local
  development.
- `terlc static emit`: renders the generated static profile entrypoint into
  `_build/web`.
- `terlc test`: consumes the generated `MainTest.terl` and embedded 0.0.1 std
  support modules.

## Testing Notes

- Unit tests cover argument parsing, profile selection, invalid names, file
  generation, web-profile files, static-profile files, next steps, and
  overwrite refusal.
