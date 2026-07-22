# Build Project Manifest Internals

This file owns `terlan.toml` package-contract parsing for `terlc build`. The
implementation in `project_manifest.rs` is centered on a deliberately small
manifest subset, so the build command can recognize project roots without
silently treating them as plain source directories or pretending unsupported
package-manager integrations are available.

## Responsibilities

- Parse the reviewed manifest package-contract subset plus current dependency
  metadata.
- Report path- and line-aware diagnostics for unsupported manifest syntax.
- Preserve package identity, source-root metadata, and requested artifact kind
  for project builds.
- Parse `path`, `npm`, and `cargo` dependency metadata without fetching,
  linking, or packaging external dependencies.
- Reject legacy OTP/Erlang manifest sections at parse time so removed runtime
  surfaces cannot reach build or deploy planning.
- Parse `[web.assets]` as Terlan-owned browser asset metadata without exposing
  Rsbuild/Rspack configuration as user-facing project syntax.
- Keep package parsing separate from artifact emission.

## Public Surface

- `ProjectManifest`: parsed project metadata.
- `ProjectPackage`: parsed `[package]` identity.
- `ProjectArtifactKind`: parsed executable artifact kind.
- `read_project_manifest`: reads and parses a manifest file from disk.
- `parse_project_manifest`: parses manifest text for tests and callers that
  already own file loading.

## Core Model

The parser accepts:

```toml
[package]
name = "demo"
version = "0.0.1"
namespace = "std.native.polars"

[build]
source_roots = ["src", "lib"]
artifact = "terlan-vm"
```

`namespace` is optional. When omitted, the source namespace defaults to the
package name with `-` converted to `_`. When present, source files must live
under the namespace path inside each source root, such as
`src/std/native/polars/DataFrame.terl` for `namespace = "std.native.polars"`.

`[build]` is optional. When omitted, source roots default to `["src"]`.
The manifest artifact kind defaults to `terlan-vm`, matching the compiler-owned
runtime artifact path. For ordinary projects, bare `terlc build` selects the
Terlan VM target and emits VM artifacts. `beam-thin` is rejected at parse time.

Audit marker: bare `terlc build` selects the Terlan VM target.
Audit marker: `beam-thin` is rejected at parse time.

Library packages use the same source-root contract but skip executable
entrypoint validation and launcher generation:

```toml
[build]
source_roots = ["src"]
artifact = "library"
```

The parser also accepts dependency metadata:

```toml
[dependencies]
local_utils = { path = "../local_utils" }

[target.js.dependencies]
zod = { npm = "zod", version = "3.25.0" }

[target.rust.dependencies]
serde = { cargo = "serde", version = "1.0.0" }
polars = { cargo = "polars", version = "0.54.4", features = ["lazy", "csv"] }
```

Dependency entries are metadata only. The manifest parser preserves them, but
does not fetch registries, walk dependency closure, link target packages, or
generate target package-manager files. `[target.erlang.dependencies]` and
`[target.erlang.package]` are rejected legacy sections.

The parser also accepts the first web asset packaging metadata:

```toml
[web.assets]
directory = "assets"
public_path = "/assets"
inline_limit = 8192
```

`directory` is required when `[web.assets]` is present. `public_path` and
`inline_limit` are optional. This section is the Terlan-facing contract; browser
packaging can translate it to Rsbuild/Rspack later without requiring users to
write direct bundler configuration in `terlan.toml`.

The main flow is:

1. Read or receive manifest text.
2. Strip comments outside strings.
3. Track the current supported section.
4. Parse supported keys into typed manifest metadata.

Important invariants:

- `terlan.toml` is not ignored by the plain recursive source-root build path.
- `[package] name` and `[package] version` are required.
- Package names must start with a lowercase ASCII letter and may contain
  lowercase ASCII letters, digits, `_`, or `-`.
- Optional package namespaces must be dot-separated lowercase Terlan module
  segments. They control source layout and module prefix validation without
  changing the package-manager-safe package name.
- Package versions must have a `major.minor.patch` numeric core.
- `terlan-vm` is the default manifest artifact and emits compiler-owned VM
  artifacts without a generated Erlang launcher.
- `beam-thin` is rejected at parse time.
- `library` emits module artifacts and package metadata without requiring
  `<package>.Main.main(): Unit` or writing a launcher.
- `[dependencies]` accepts only local `{ path = "..." }` Terlan package
  metadata.
- `[target.js.dependencies]` accepts only `{ npm = "...", version = "..." }`
  metadata.
- `[target.rust.dependencies]` accepts only
  `{ cargo = "...", version = "..." }` metadata, plus an optional
  `features = ["..."]` string array for Rust crate feature flags.
- `[web.assets]` accepts only `directory`, `public_path`, and `inline_limit`
  metadata.
- Dependency metadata parsing does not imply dependency closure, registry
  access, target linking, or release packaging support.
- Web asset metadata does not imply direct Rsbuild/Rspack configuration support
  or asset rule parsing.
- Parsed metadata does not imply project builds are complete.

## Integration Points

- `build::run_manifest_project_build`: detects `terlan.toml`, parses it, and
  delegates the selected manifest source root to the formal source-root build
  path.
- `docs/compiler/TERLAN_0_0_1_LANGUAGE_INVENTORY.md`: records the current
  A0.42.2 dependency-metadata boundary.

## Edge Cases

- Missing package names fail before build emission.
- Missing package versions fail before build emission.
- Invalid package names, invalid versions, and unsupported artifact kinds fail
  before build emission.
- Unsupported sections and keys fail before build emission.
- Registry dependency entries in `[dependencies]` fail before build emission.
- Wrong target registry source kinds fail before build emission.
- Target registry entries without `version` fail before build emission.
- Incomplete or malformed web asset metadata fails before build emission.
- Empty `source_roots` arrays fail before build emission.
- Unknown string escapes fail before build emission.

## Testing Notes

- Parser tests live in `project_manifest_test.rs`.
- Build-command behavior is covered by
  `build_command_rejects_project_manifest_before_silent_directory_scan`,
  `build_command_compiles_project_manifest_source_root`, and
  `build_deploy_plan_rejects_legacy_beam_artifact`.
