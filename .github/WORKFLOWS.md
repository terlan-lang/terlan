# GitHub Workflows

Terlan uses separate docs, compiler, and release flows so lightweight
documentation checks do not run compiler builds, compiler-facing source changes
are checked continuously, and release artifacts are built only for tagged
releases or manual runs.

## Docs CI

`docs.yml` runs on pull requests and `main` pushes when documentation-facing
files change:

- `README.md`
- `CHANGELOG.md`
- `docs/**`
- `.github/WORKFLOWS.md`
- docs workflow configuration

It performs lightweight Markdown hygiene only. It does not install Rust, Erlang,
or run compiler release gates.

## Compiler CI

`ci.yml` runs on pull requests and `main` pushes when compiler-facing sources
change:

- Cargo workspace files
- `crates/**`
- `std/**`
- `editors/**`
- `tree-sitter-terlan/**`
- `tests/**`
- `docs/grammar/**`
- `tools/**`
- `install.sh`
- `Makefile`
- compiler workflow configuration

It installs Rust, Erlang/OTP 29, Node.js, and the local Tree-sitter package
dependencies, then runs a fast developer gate followed by the release-scale
gate:

```sh
make check
make test          # fast workspace library tests plus CLI smoke tests
make test-release  # full workspace tests plus ignored release-scale sweeps
make editor-check
make tree-sitter-cli-check
make erlang-runtime-matrix-check  # when TERLAN_OTP_RUNTIME_BIN is configured
make release-0-0-5-preflight
```

The 0.0.5 preflight is the compiler CI gate for the JavaScript target path,
generated `std.js` binding drift, Oxc validation, target-profile rejection
fixtures, editor package checks, and current web/runtime regressions. Compiler
CI installs the local `tree-sitter-terlan` npm dependencies before running the
real Tree-sitter CLI grammar check; the default local `make editor-check` path
stays dependency-free.

## Release Artifacts

`release.yml` runs manually or when a version tag is pushed:

```text
v0.0.4
```

It validates the compiler and generated std summaries with the same fast/full
split:

```sh
make check
make test
make test-release
make editor-check
make tree-sitter-cli-check
make erlang-runtime-matrix-check  # when TERLAN_OTP_RUNTIME_BIN is configured
make release-0-0-5-preflight
```

Then the release workflow builds platform artifacts through the release matrix:

```sh
terlc-linux-x86_64.tar.gz
terlc-linux-aarch64.tar.gz
terlc-macos-x86_64.tar.gz
terlc-macos-aarch64.tar.gz
terlc-windows-x86_64.zip
```

Each matrix lane runs the current-platform package helper, smoke-tests the
packaged artifact by extracting `terlc` and `terlan-vm`, checking their
versions, initializing a web-profile project, building the project for Erlang
and `js.browser`, validating the generated web artifact with
`terlc serve --check`, running the packaged `terlan-vm`, and running the public
installer against the artifact through a local file-backed release mirror.

Tagged runs upload every matrix artifact to the matching GitHub release. The
release body is generated from the matching `CHANGELOG.md` section, such as
`## 0.0.4` for tag `v0.0.4`.
