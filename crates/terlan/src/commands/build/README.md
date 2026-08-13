# Build Command Context

This module owns `terlc build`.

## Current Scope

The 0.0.7 build command emits a target-native `.tvm` application image.
Single-file and manifest-backed builds lower supported scalar functions from
CoreIR through Terlan NativeIR and Cranelift into a relocatable object, perform
one native link for the application, embed and seal the canonical binary TVM
descriptor, and record public typed exports for runtime worker dispatch. The
resulting `.tvm` is a statically admissible, self-describing native image that
`terlan-vm run` and `terlan-vm load` consume directly.

The current direct profile includes checked integer and Boolean values,
arithmetic and comparisons, local calls, sequential scalar `let` bindings, and
ordered scalar `if` branches. A branch set with no matching condition returns a
typed native status rather than falling back to interpretation.

```sh
terlc build
terlc build path/to/module.terl
terlc build path/to/project
terlc build path/to/project --release
```

Development builds are the default. They use fast Cranelift code generation
and independently reusable module objects so an implementation-only edit can
rebuild one module before the final application link. `--release` selects
speed-optimized Cranelift lowering, emits one whole-application object, and
enables native-linker optimization. Cranelift objects do not carry LLVM IR, so
the release path does not claim an external LLVM LTO stage. Development and
release policy identities are included in object, image, and warm-reuse cache
keys and can never share a cached native image.

The default VM path writes:

```text
_build/
  vm/<application>.tvm
  .terlan/native-aot/<digest>/<application>.native.o
  .terlan/native-aot/<digest>/<application>.descriptor.o
```

The application-code path does not generate Rust or invoke `rustc`; Cranelift
object emission occurs in the compiler process. Native objects and descriptor
objects are content-addressed compiler cache entries and are not runtime
artifacts. Manifest-backed executable packages also bundle `terlan-vm`,
`terlan-native-worker`, a launcher, and metadata naming all package members.

`terlan-package-build.json` is a manifest-backed package/build metadata file,
not a debug map and not a package-manager lockfile. It records the package name,
version, selected target, selected artifact mode, executable artifact metadata,
declared source roots, and normalized dependency metadata from `terlan.toml`.
Downstream tools can consume this file to distinguish package shape from VM,
JavaScript, Wasm, and future target-specific artifact metadata.
The directory slice recursively scans package-rooted source layouts for `.terl`
files, validates that source-root-relative paths match declared module names,
then emits VM artifacts by default.

A direct source-file build inside a manifest-backed project discovers the
nearest owning `terlan.toml`, prepares interfaces for its resolved source and
dependency roots, validates the selected file against the owning package
namespace, and emits only that file's artifact. This lets `terlc run
path/to/Entry.terl` resolve sibling and dependency modules without executing or
emitting every source file as another entrypoint. A direct file with no owning
manifest remains an isolated single-file build.

Directories containing `terlan.toml` use the project-manifest path instead of
this plain source-root path. The current project path parses package metadata,
manifest-declared source roots, and dependency metadata. Local `path`
dependencies are recursively resolved before artifact emission: dependency
manifests must exist, dependency source roots are validated before dependents,
dependency source roots are emitted before dependents, and local dependency
cycles are rejected. Target-scoped external dependency metadata for
`hex`, `npm`, and `cargo` is parsed and preserved by the manifest layer, but
current builds reject those entries with stable diagnostics before artifact
emission. Fetching, linking, or packaging those dependencies here would make
`terlc build <project>` look more complete than it is.

Manifest-backed source roots are package-namespace-rooted. For a package named
`app`, a source file under `src/app/Main.terl` declares `module app.Main.`. A
source file under `src/other/Main.terl` is rejected before module-layout
validation, CoreIR lowering, or artifact emission. Package names that contain
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

The legacy `beam-thin` artifact is rejected at manifest parse time.
A0.46 defines the historical project entrypoint convention:
`<namespace>.Main.main(): Unit`, where the namespace is either `[package]
namespace` or the manifest package name with `-` converted to `_`. Library
artifacts skip entrypoint validation and launcher generation. The A0.46
hello-world surface is `std.io.Console.println(value: String): Unit`; Terlan
source must not call Erlang `io` directly.

## Responsibilities

- Parse command-local build arguments.
- Compile Terlan source files through the formal compiler pipeline.
- Emit one descriptor-bearing native application image for supported scalar
  exports and expose only public functions at the worker boundary.
- Reject legacy `beam-thin` launcher generation from the public build path.
- Emit package/build metadata for successful manifest-backed project builds.
- Reject target profiles that are incompatible with the
  selected target.
- For JS browser builds, emit `_build/web/manifest.json` with copied assets,
  static responses, dynamic VM handler routes, and source metadata that
  `terlc serve` can use for request logs and development errors.
## File Layout

- `mod.rs` routes build command execution and coordinates target-specific
  output.
- `project_roots.rs` resolves manifest source roots, local path dependencies,
  and locked Git package closures for VM, JavaScript, and future
  target-specific project builds.
- `package_git.rs` owns explicit immutable Git fetching, deterministic
  `terlan.lock` generation, cache provenance/checksum validation, and
  network-free build resolution.
- `target_gate.rs` owns build-target compatibility checks for native packages
  and target-specific std imports.
- `project_manifest.rs` parses `terlan.toml`, assigns TOML keys to typed
  builders, and reports stable manifest diagnostics.
- `project_manifest/model.rs` defines the typed project-manifest data model
  re-exported through `project_manifest`.
- `project_manifest/config.rs` owns optional `[web.assets]` and `[server.tls]`
  builders plus scalar config parsers.
- `project_manifest/strings.rs` owns shared string constants used by manifest
  parsing and diagnostics.
- `project_manifest/validation.rs` owns manifest-facing identifier and package
  validation helpers.

## Boundaries

- Do not fetch external registries or generate target package-manager files
  here until the roadmap explicitly opens those slices. Git network access is
  restricted to explicit `terlc package fetch`. That command also admits local
  target archives through `--artifact`, verifies their manifest and complete
  payload checksum inventory, and records their content-addressed cache entry
  in `terlan.lock`. Build, run, and test consume only locked, reverified cache
  entries for the active target.
- Do not restore removed OTP/Rebar adapter generation here. Future package
  publishing work must enter through target-neutral package metadata.
- Do not bypass the formal pipeline; `build` must use the same checked artifacts
  as release-supported compiler commands.
- Keep command-specific process execution local unless another release command
  needs the same runner.
