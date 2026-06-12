# GitHub Workflows

Terlan uses separate CI and release flows so source changes are checked
continuously while release artifacts are built only for tagged releases or
manual runs.

## Compiler CI

`ci.yml` runs on pull requests and `main` pushes when compiler-facing sources
change:

- Cargo workspace files
- `crates/**`
- `std/**`
- `tests/**`
- `docs/grammar/**`
- `tools/**`
- `Makefile`
- compiler workflow configuration

It installs Rust and Erlang/OTP 29, then runs:

```sh
make check
make test
```

## Release Artifacts

`release.yml` runs manually or when a version tag is pushed:

```text
v0.0.1
v0.0.2
```

It builds the Linux x86_64 `terlc` artifact with:

```sh
make release-artifact-linux
```

Tagged runs upload `terlc-linux-x86_64.tar.gz` to the matching GitHub release.
