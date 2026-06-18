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
- `tests/**`
- `docs/grammar/**`
- `tools/**`
- `install.sh`
- `Makefile`
- compiler workflow configuration

It installs Rust and Erlang/OTP 29, then runs:

```sh
make check
make test
make release-0-0-4-preflight
```

The 0.0.4 preflight is the compiler CI gate for the JavaScript target path,
generated `std.js` binding drift, Oxc validation, and target-profile rejection
fixtures.

## Release Artifacts

`release.yml` runs manually or when a version tag is pushed:

```text
v0.0.4
```

It validates the compiler and generated std summaries with:

```sh
make check
make test
make release-0-0-4-preflight
```

Then it builds the Linux x86_64 `terlc` artifact with:

```sh
make release-artifact-linux
```

That target also smoke-tests the packaged tarball by extracting `terlc`,
checking its version, initializing a web-profile project, building the project
for Erlang and `js.browser`, and validating the generated web artifact with
`terlc serve --check`.

Tagged runs upload `terlc-linux-x86_64.tar.gz` to the matching GitHub release.
The release body is generated from the matching `CHANGELOG.md` section, such as
`## 0.0.4` for tag `v0.0.4`.
