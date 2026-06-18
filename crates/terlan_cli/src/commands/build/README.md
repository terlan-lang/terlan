# Build Command Context

This module owns `terlc build`.

## Current Scope

The release-candidate build command supports single-file builds and recursive
directory source discovery for the Erlang backend:

```sh
terlc build
terlc build path/to/module.terl --target erlang --out-dir build/terlan
terlc build path/to/project --target erlang --out-dir build/terlan
```

It writes:

```text
_build/
  terlan-debug-map.json
  terlan-package-build.json   # manifest-backed project builds only
  bin/<package>               # manifest-backed beam-thin launcher only
  src/<module>.erl
  ebin/<module>.beam
```

`terlan-debug-map.json` is a build-local source-to-artifact map. It records the
Terlan module name, source path, CoreIR hash, generated Erlang source path, and
generated BEAM path for each emitted module. Manifest-backed project builds also
record project package name and declared source roots. This artifact is required
by the debuggability contract so backend output can be traced back to formal
compiler identity without rerunning validation.

`terlan-package-build.json` is a manifest-backed package/build metadata file,
not a debug map and not a package-manager lockfile. It records the package name,
version, selected target, selected artifact mode, executable artifact metadata,
declared source roots, and normalized dependency metadata from `terlan.toml`.
Downstream tools can consume this file to distinguish package shape from
generated-source traceability.
When `[target.erlang.package] adapter = "rebar3-compatible"` is present, the
file records that adapter marker under package adapters.

The directory slice recursively scans package-rooted source layouts for `.terl`
files, validates that source-root-relative paths match declared module names
through the existing directory check path using an interface cache, then emits
and compiles modules that the current Erlang backend can compile independently.

Directories containing `terlan.toml` use the project-manifest path instead of
this plain source-root path. The current project path parses package metadata,
manifest-declared source roots, and dependency metadata. Local `path`
dependencies are recursively resolved before backend emission: dependency
manifests must exist, dependency source roots are validated before dependents,
dependency source roots are emitted before dependents, and local dependency
cycles are rejected. Project builds still write one combined debug map across
all selected source roots. Target-scoped external dependency metadata for
`hex`, `npm`, and `cargo` is parsed and preserved by the manifest layer, but
current builds reject those entries with stable diagnostics before backend
emission. Fetching, linking, or packaging those dependencies here would make
`terlc build <project>` look more complete than it is.
The Erlang backend also rejects `std.native.*` modules and imports before
lowering because those packages require the Rust/native target capability.

Manifest-backed source roots are package-namespace-rooted. For a package named
`app`, a source file under `src/app/Main.terl` declares `module app.Main.`. A
source file under `src/other/Main.terl` is rejected before module-layout
validation, CoreIR lowering, or backend emission. Package names that contain
`-` use `_` for the default source package namespace because Terlan module path
segments are `LowerIdent` tokens. A manifest can override the source namespace
without changing the package-manager-safe package name:

```toml
[package]
name = "std-native-polars"
version = "0.0.4"
namespace = "std.native.polars"
```

With that manifest, source files live under `src/std/native/polars/...` and
declare modules such as `module std.native.polars.DataFrame.`. The same
namespace validation applies to local `path` dependency manifests.

Local `path` dependencies are emitted in package dependency order, but
`terlan-package-build.json` records the root package manifest's dependency
metadata rather than pretending to be a full dependency lockfile.

Manifest-backed `beam-thin` builds emit `bin/<package>` as the single
user-facing executable artifact. It is a small launcher that expects
Erlang/ERTS on the target machine and starts `erl` with the generated `ebin`
directory on the BEAM code path. A0.46 defines the boring project entrypoint
convention: the launcher must invoke `<namespace>.Main.main(): Unit`, where the
namespace is either `[package] namespace` or the manifest package name with `-`
converted to `_`. Manifest-backed executable builds must reject missing,
private, argument-taking, or non-`Unit` entrypoints before writing the launcher.
Library artifacts skip entrypoint validation and launcher generation. The A0.46
hello-world surface is `std.io.Console.println(value: String): Unit`; Terlan
source must not call Erlang `io` directly. The BEAM backend owns the lowering
to `io:format("~ts~n", [Text])` or an equivalent runtime helper. Generated
`.erl` and `.beam` files remain intermediate compiler artifacts.

Erlang package adapter metadata is also metadata-only in the current build
path. A manifest can reserve `rebar3-compatible` adapter intent, but `terlc
build --target erlang` does not generate `rebar.config`, `.app.src`, release
files, or invoke Rebar3.

## Responsibilities

- Parse command-local build arguments.
- Compile Terlan source files through the formal compiler pipeline.
- Emit Erlang source into the build `src` directory.
- Invoke `erlc` to produce BEAM artifacts in the build `ebin` directory.
- Emit a source-to-artifact debug map after successful artifact generation.
- Emit a single `beam-thin` executable launcher for successful manifest-backed
  project builds.
- Emit package/build metadata for successful manifest-backed project builds.
- Reject backend targets and target profiles that are not Erlang-compatible.

## Boundaries

- Do not fetch external registries or generate target package-manager files
  here until the roadmap explicitly opens those slices. Project-manifest builds
  may walk local `path` dependency manifests only.
- Do not generate Rebar3 or OTP application files from adapter metadata until
  the Erlang target packaging adapter slice explicitly opens that work.
- Do not bypass the formal pipeline; `build` must use the same checked artifacts
  as release-supported compiler commands.
- Keep command-specific process execution local unless another release command
  needs the same runner.
