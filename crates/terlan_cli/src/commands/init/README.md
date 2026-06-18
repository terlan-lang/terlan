# CLI `init` Command Internals

This directory owns `terlc init`, the release project scaffolder.

## Responsibilities

- Create a minimal manifest-backed Terlan project.
- Use the default BEAM executable package layout unless another profile is
  selected.
- Support `--profile web` for the smallest browser-plus-handler project shape.
- Refuse to overwrite existing project files.
- Keep generated source target-neutral with an explicit
  `std.io.Console.{println}` import.
- Create a sample `*_test.terl` file that runs through `terlc test`.

## Public Surface

- `run`: command entry point called by the top-level CLI router.

## Core Model

The command accepts exactly one new project name/path and an optional profile:

```sh
terlc init hello
terlc init hello-web --profile web
```

The command creates that directory and writes the scaffold inside it. Existing
directories are rejected, even when they do not contain Terlan files.

Generated shape:

```text
hello/
  terlan.toml
  src/hello/Main.terl
  tests/hello/main_test.terl
```

The web profile adds a browser-side Terlan module and a BEAM-backed handler
module, plus the default asset directory declared by `terlan.toml`:

```text
hello-web/
  terlan.toml
  assets/
  src/hello_web/Main.terl
  src/hello_web/Web.terl
  src/hello_web/Http.terl
  tests/hello_web/main_test.terl
```

Package names may contain lowercase ASCII letters, digits, `_`, and `-`, and
must start with a lowercase ASCII letter. Source module roots replace `-` with
`_` because Terlan module path segments are identifiers.

The web profile keeps all Terlan source under `src` so
`terlc build --target erlang` can produce BEAM handler artifacts and
`terlc build --target js.browser` can produce the browser package. It adds the
minimal web asset contract:

```toml
[web.assets]
directory = "assets"
```

This is Terlan-owned project metadata. Browser packaging can translate it to
the selected Oxc/Rsbuild/Rspack boundary later without requiring users to write
direct bundler configuration. The HTTP handler currently uses the internal
server bridge shape until the public Rust-native `std.http` adapter is wired.

## Integration Points

- `main.rs`: routes the `init` verb here.
- `terlc build`: consumes the generated `terlan.toml` and source modules.
- `terlc serve`: serves the generated browser package for web-profile projects.
- `terlc test`: consumes the generated `main_test.terl` and embedded 0.0.1 std
  support modules.

## Testing Notes

- Unit tests cover argument parsing, profile selection, invalid names, file
  generation, web-profile files, and overwrite refusal.
