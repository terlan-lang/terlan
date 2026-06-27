use std::path::PathBuf;

use serde::Serialize;

use super::wasm_model::{BuildWasiTargetMetadata, BuildWasmTargetMetadata};
use super::{
    package_layout::source_package_module_prefix, project_manifest, BUILD_PACKAGE_METADATA_SCHEMA,
};

/// Serializable source-to-artifact debug map for one build invocation.
///
/// Inputs:
/// - Produced from successfully compiled build module entries.
///
/// Output:
/// - JSON-ready metadata written to the build output directory.
///
/// Transformation:
/// - Groups backend artifact paths under a stable schema so debuggers,
///   release tools, and future backend runners can trace generated artifacts
///   back to Terlan source and CoreIR identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct BuildDebugMap {
    pub(super) schema: &'static str,
    pub(super) target: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) project: Option<BuildDebugProject>,
    pub(super) modules: Vec<BuildDebugModuleEntry>,
}

/// Serializable project metadata for a manifest-backed build invocation.
///
/// Inputs:
/// - Produced from parsed `terlan.toml` metadata.
///
/// Output:
/// - Optional project entry inside `terlan-debug-map.json`.
///
/// Transformation:
/// - Records package identity, manifest source roots, and selected artifact
///   kind so project-level build artifacts can be traced back to package
///   metadata as well as source files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct BuildDebugProject {
    pub(super) package: String,
    pub(super) version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) namespace: Option<String>,
    pub(super) source_roots: Vec<String>,
    pub(super) artifact: String,
}

/// Serializable package/build metadata for a manifest-backed build.
///
/// Inputs:
/// - Produced from a parsed root `terlan.toml`.
///
/// Output:
/// - JSON-ready package metadata written beside backend artifacts.
///
/// Transformation:
/// - Separates package identity, artifact selection, source roots, and
///   dependency metadata from the source-to-backend debug map so downstream
///   tools can reason about package shape without consuming debug traces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct BuildPackageMetadata {
    pub(super) schema: &'static str,
    pub(super) target: &'static str,
    pub(super) package: BuildPackageIdentity,
    pub(super) artifact: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) executable: Option<BuildPackageExecutable>,
    pub(super) source_roots: Vec<String>,
    pub(super) dependencies: Vec<BuildPackageDependency>,
    pub(super) adapters: Vec<BuildPackageAdapter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) wasm: Option<BuildWasmTargetMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) wasi: Option<BuildWasiTargetMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) native: Option<BuildPackageNative>,
}

/// Serializable package identity inside build metadata.
///
/// Inputs:
/// - Produced from the manifest `[package]` table.
///
/// Output:
/// - Stable package name/version payload.
///
/// Transformation:
/// - Copies the validated package identity into the build artifact metadata
///   schema without adding target-specific package-manager semantics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct BuildPackageIdentity {
    pub(super) name: String,
    pub(super) version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) namespace: Option<String>,
}

/// Serializable single executable artifact metadata.
///
/// Inputs:
/// - Produced from the selected package artifact mode and package identity.
///
/// Output:
/// - Executable artifact entry inside `terlan-package-build.json`.
///
/// Transformation:
/// - Records the user-facing executable path and runtime expectation while
///   keeping backend `.erl` and `.beam` files classified as intermediate
///   compiler artifacts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct BuildPackageExecutable {
    pub(super) mode: String,
    pub(super) path: String,
    pub(super) runtime: String,
    pub(super) entrypoint: BuildPackageEntrypoint,
}

/// Serializable entrypoint metadata inside executable build metadata.
///
/// Inputs:
/// - Produced from the manifest package name and selected artifact mode.
///
/// Output:
/// - Stable package entrypoint module/function/arity payload.
///
/// Transformation:
/// - Converts the package-root convention into metadata consumed by the
///   launcher writer and future release/debug tools.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct BuildPackageEntrypoint {
    pub(super) module: String,
    pub(super) function: String,
    pub(super) arity: usize,
}

/// Serializable dependency metadata inside build metadata.
///
/// Inputs:
/// - Produced from parsed manifest dependency entries.
///
/// Output:
/// - One normalized dependency entry in `terlan-package-build.json`.
///
/// Transformation:
/// - Represents every accepted dependency source kind with stable string
///   fields while omitting fields that do not apply to that source kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct BuildPackageDependency {
    pub(super) alias: String,
    pub(super) scope: String,
    pub(super) source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) package: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) features: Option<Vec<String>>,
}

/// Serializable target package-adapter metadata inside build metadata.
///
/// Inputs:
/// - Produced from target package-adapter reservations in `terlan.toml`.
///
/// Output:
/// - One normalized adapter entry in `terlan-package-build.json`.
///
/// Transformation:
/// - Records target-owned adapter intent without generating adapter files or
///   making target package tools part of the generic build path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct BuildPackageAdapter {
    pub(super) target: String,
    pub(super) adapter: String,
}

/// Serializable native runtime metadata inside build metadata.
///
/// Inputs:
/// - Produced from manifest native adapter declarations.
///
/// Output:
/// - Optional native-helper discovery metadata for package consumers.
///
/// Transformation:
/// - Separates native runtime metadata from dependency and adapter metadata so
///   package tools can find helper executables without inferring Cargo layout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct BuildPackageNative {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) rust: Option<BuildPackageRustNative>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(super) rust_dependencies: Vec<BuildPackageRustNativeDependency>,
}

/// Serializable Rust native helper metadata.
///
/// Inputs:
/// - Produced from `[native.rust]`.
///
/// Output:
/// - Stable crate path, helper executable name, and environment variable name.
///
/// Transformation:
/// - Copies parsed manifest fields into the package metadata schema without
///   invoking Cargo or resolving host-specific binary paths.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct BuildPackageRustNative {
    #[serde(rename = "crate")]
    pub(super) crate_name: String,
    pub(super) path: String,
    pub(super) helper: String,
    pub(super) helper_env: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(super) features: Vec<String>,
    pub(super) package_dir: String,
}

/// Serializable Rust native helper metadata for a local dependency.
///
/// Inputs:
/// - Produced from resolved local path dependency manifests that declare
///   `[native.rust]`.
///
/// Output:
/// - Stable dependency package identity plus helper metadata in the root
///   package build artifact.
///
/// Transformation:
/// - Carries enough package-directory context for `terlc run` to discover
///   already-built helper executables without reparsing dependency manifests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct BuildPackageRustNativeDependency {
    pub(super) package: String,
    pub(super) version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) namespace: Option<String>,
    pub(super) rust: BuildPackageRustNative,
}

/// Resolved project package build roots.
///
/// Inputs:
/// - Produced from a root project manifest plus recursively parsed local
///   `path` dependencies.
///
/// Output:
/// - Ordered source roots for validation/emission.
///
/// Transformation:
/// - Keeps dependency source roots before the root package source roots so
///   imports from the root package can resolve through the shared build cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ProjectBuildRoots {
    pub(super) source_roots: Vec<ProjectSourceRoot>,
    pub(super) native_rust_dependencies: Vec<ProjectNativeRustDependency>,
}

/// Resolved source root with package identity.
///
/// Inputs:
/// - Produced from a project manifest or local path dependency manifest.
///
/// Output:
/// - Filesystem source root plus the source package root required under that
///   root for module-layout validation.
///
/// Transformation:
/// - Carries manifest package identity into the shared source-root build path
///   so package-root imports are validated before CoreIR/backend emission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ProjectSourceRoot {
    pub(super) path: PathBuf,
    pub(super) package_path: Vec<String>,
}

/// Resolved Rust native helper metadata for a local dependency package.
///
/// Inputs:
/// - Produced during local dependency source-root resolution.
///
/// Output:
/// - Dependency package identity and helper discovery metadata.
///
/// Transformation:
/// - Stores canonical package-directory context alongside parsed native Rust
///   metadata so package build metadata can be generated without another
///   dependency traversal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ProjectNativeRustDependency {
    pub(super) package: project_manifest::ProjectPackage,
    pub(super) package_dir: PathBuf,
    pub(super) native: project_manifest::ProjectNativeRust,
}

/// Serializable debug metadata for one compiled module.
///
/// Inputs:
/// - Produced after Erlang source and BEAM artifact generation succeeds.
///
/// Output:
/// - One module entry inside `terlan-debug-map.json`.
///
/// Transformation:
/// - Records the source path, CoreIR hash, generated Erlang source path, and
///   generated BEAM path for source-to-artifact debugging.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct BuildDebugModuleEntry {
    pub(super) module: String,
    pub(super) source_path: String,
    pub(super) core_ir_hash: u64,
    pub(super) erl_path: String,
    pub(super) beam_path: String,
}

/// Built module artifact plus entrypoint-relevant CoreIR summary.
///
/// Inputs:
/// - Produced after one source file has compiled to Erlang and BEAM artifacts.
///
/// Output:
/// - Debug-map entry plus public/private function summaries used by package
///   executable validation.
///
/// Transformation:
/// - Keeps executable entrypoint validation on CoreIR facts without adding
///   function signatures to the public debug-map JSON schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct BuildModuleArtifact {
    pub(super) debug_entry: BuildDebugModuleEntry,
    pub(super) functions: Vec<BuildEntrypointFunction>,
}

/// Entrypoint-relevant function summary for one built module.
///
/// Inputs:
/// - Extracted from `CoreFunction` after the formal compiler path succeeds.
///
/// Output:
/// - Minimal name/arity/visibility/return-type payload for launcher contract
///   validation.
///
/// Transformation:
/// - Projects CoreIR function declarations into a build-local summary so the
///   executable gate does not depend on backend syntax.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct BuildEntrypointFunction {
    pub(super) name: String,
    pub(super) arity: usize,
    pub(super) public: bool,
    pub(super) return_type: String,
}

/// Validated executable entrypoint for a package build.
///
/// Inputs:
/// - Produced by checking manifest-derived executable metadata against built
///   module CoreIR summaries.
///
/// Output:
/// - Terlan module/function identity and backend Erlang module/function names.
///
/// Transformation:
/// - Bridges the target-neutral package entrypoint convention to the concrete
///   BEAM invocation owned by the `beam-thin` launcher.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct BuildEntrypoint {
    pub(super) module: String,
    pub(super) function: String,
    pub(super) arity: usize,
    pub(super) erlang_module: String,
    pub(super) erlang_function: String,
}

/// Builds deterministic package metadata from a parsed project manifest.
///
/// Inputs:
/// - `manifest`: parsed root project manifest.
///
/// Output:
/// - Serializable package/build metadata for artifact consumers.
///
/// Transformation:
/// - Copies validated package fields and converts dependency enum variants to a
///   sorted, string-keyed metadata schema without resolving external packages.
pub(super) fn build_package_metadata(
    project_dir: &std::path::Path,
    manifest: &project_manifest::ProjectManifest,
    native_rust_dependencies: &[ProjectNativeRustDependency],
) -> BuildPackageMetadata {
    let mut dependencies = manifest
        .dependencies
        .iter()
        .map(build_package_dependency_metadata)
        .collect::<Vec<_>>();
    dependencies.sort_by(|left, right| {
        (
            left.scope.as_str(),
            left.alias.as_str(),
            left.source.as_str(),
            left.path.as_deref().unwrap_or(""),
            left.package.as_deref().unwrap_or(""),
            left.version.as_deref().unwrap_or(""),
        )
            .cmp(&(
                right.scope.as_str(),
                right.alias.as_str(),
                right.source.as_str(),
                right.path.as_deref().unwrap_or(""),
                right.package.as_deref().unwrap_or(""),
                right.version.as_deref().unwrap_or(""),
            ))
    });

    BuildPackageMetadata {
        schema: BUILD_PACKAGE_METADATA_SCHEMA,
        target: "erlang",
        package: BuildPackageIdentity {
            name: manifest.package.name.clone(),
            version: manifest.package.version.clone(),
            namespace: manifest.package.namespace.clone(),
        },
        artifact: manifest.artifact.as_str().to_string(),
        executable: build_package_executable_metadata(manifest),
        source_roots: manifest.source_roots.clone(),
        dependencies,
        adapters: build_package_adapter_metadata(manifest),
        wasm: build_wasm_target_metadata(manifest),
        wasi: build_wasi_target_metadata(manifest),
        native: build_package_native_metadata(project_dir, manifest, native_rust_dependencies),
    }
}

/// Builds deterministic Wasm target metadata.
///
/// Inputs:
/// - Parsed project manifest.
///
/// Output:
/// - Optional Wasm package metadata when `[target.wasm]` is present.
///
/// Transformation:
/// - Copies reserved Wasm manifest fields into the package metadata schema
///   without selecting an engine or emitting a module.
fn build_wasm_target_metadata(
    manifest: &project_manifest::ProjectManifest,
) -> Option<BuildWasmTargetMetadata> {
    manifest
        .wasm_target
        .as_ref()
        .map(|target| BuildWasmTargetMetadata {
            profile: target.profile.as_str().to_string(),
            exports: target.exports.clone(),
            bridge: target.bridge.clone(),
            capabilities: target.capabilities.clone(),
            world: target.world.clone(),
            validation_engine: target.validation_engine.clone(),
        })
}

/// Builds deterministic WASI target metadata.
///
/// Inputs:
/// - Parsed project manifest.
///
/// Output:
/// - Optional WASI package metadata when `[target.wasi]` is present.
///
/// Transformation:
/// - Copies reserved WASI manifest fields into the package metadata schema
///   without selecting an engine or emitting a component.
fn build_wasi_target_metadata(
    manifest: &project_manifest::ProjectManifest,
) -> Option<BuildWasiTargetMetadata> {
    manifest
        .wasi_target
        .as_ref()
        .map(|target| BuildWasiTargetMetadata {
            profile: target.profile.as_str().to_string(),
            world: target.world.clone(),
            capabilities: target.capabilities.clone(),
            validation_engine: target.validation_engine.clone(),
        })
}

/// Builds deterministic executable artifact metadata when the artifact is runnable.
///
/// Inputs:
/// - `manifest`: parsed root project manifest.
///
/// Output:
/// - Serializable executable artifact metadata for runnable artifact modes.
/// - `None` for library artifact modes.
///
/// Transformation:
/// - Converts `beam-thin` into launcher metadata and treats `library` as a
///   non-executable package artifact.
fn build_package_executable_metadata(
    manifest: &project_manifest::ProjectManifest,
) -> Option<BuildPackageExecutable> {
    match manifest.artifact {
        project_manifest::ProjectArtifactKind::BeamThin => Some(BuildPackageExecutable {
            mode: "beam-thin".to_string(),
            path: format!("bin/{}", manifest.package.name),
            runtime: "external-erts".to_string(),
            entrypoint: BuildPackageEntrypoint {
                module: format!("{}.Main", source_package_module_prefix(&manifest.package)),
                function: "main".to_string(),
                arity: 0,
            },
        }),
        project_manifest::ProjectArtifactKind::Library
        | project_manifest::ProjectArtifactKind::WasmCore
        | project_manifest::ProjectArtifactKind::WasmBrowser
        | project_manifest::ProjectArtifactKind::WasmComponent
        | project_manifest::ProjectArtifactKind::WasiCli
        | project_manifest::ProjectArtifactKind::WasiHttp
        | project_manifest::ProjectArtifactKind::WasiWorker => None,
    }
}

/// Builds deterministic target package-adapter metadata.
///
/// Inputs:
/// - `manifest`: parsed root project manifest.
///
/// Output:
/// - Ordered adapter metadata entries for the package build artifact.
///
/// Transformation:
/// - Preserves supported target adapter reservations as metadata only; it does
///   not generate Rebar3 files, package-manager manifests, or release configs.
fn build_package_adapter_metadata(
    manifest: &project_manifest::ProjectManifest,
) -> Vec<BuildPackageAdapter> {
    manifest
        .erlang_package_adapter
        .map(|adapter| BuildPackageAdapter {
            target: "erlang".to_string(),
            adapter: adapter.as_str().to_string(),
        })
        .into_iter()
        .collect()
}

/// Builds deterministic native runtime metadata.
///
/// Inputs:
/// - `manifest`: parsed root project manifest.
///
/// Output:
/// - Optional native metadata when `[native.rust]` is declared.
///
/// Transformation:
/// - Converts the parsed helper contract into the JSON schema consumed by
///   package build and runtime tooling.
fn build_package_native_metadata(
    project_dir: &std::path::Path,
    manifest: &project_manifest::ProjectManifest,
    native_rust_dependencies: &[ProjectNativeRustDependency],
) -> Option<BuildPackageNative> {
    let rust = manifest
        .native_rust
        .as_ref()
        .map(|native| build_package_rust_native(project_dir, native));
    let mut rust_dependencies = native_rust_dependencies
        .iter()
        .map(|dependency| BuildPackageRustNativeDependency {
            package: dependency.package.name.clone(),
            version: dependency.package.version.clone(),
            namespace: dependency.package.namespace.clone(),
            rust: build_package_rust_native(&dependency.package_dir, &dependency.native),
        })
        .collect::<Vec<_>>();
    rust_dependencies.sort_by(|left, right| {
        (
            left.package.as_str(),
            left.version.as_str(),
            left.namespace.as_deref().unwrap_or(""),
            left.rust.helper_env.as_str(),
        )
            .cmp(&(
                right.package.as_str(),
                right.version.as_str(),
                right.namespace.as_deref().unwrap_or(""),
                right.rust.helper_env.as_str(),
            ))
    });

    (rust.is_some() || !rust_dependencies.is_empty()).then_some(BuildPackageNative {
        rust,
        rust_dependencies,
    })
}

/// Converts parsed Rust native metadata into package build metadata.
///
/// Inputs:
/// - `package_dir`: canonical or user-selected package directory.
/// - `native`: parsed `[native.rust]` manifest metadata.
///
/// Output:
/// - Serializable helper metadata including package-directory context.
///
/// Transformation:
/// - Copies native helper fields and records the package directory used as the
///   base for helper executable discovery.
fn build_package_rust_native(
    package_dir: &std::path::Path,
    native: &project_manifest::ProjectNativeRust,
) -> BuildPackageRustNative {
    BuildPackageRustNative {
        crate_name: native.crate_name.clone(),
        path: native.path.clone(),
        helper: native.helper.clone(),
        helper_env: native.helper_env.clone(),
        features: native.features.clone(),
        package_dir: package_dir.display().to_string(),
    }
}

/// Builds one deterministic dependency metadata entry.
///
/// Inputs:
/// - `dependency`: parsed manifest dependency.
///
/// Output:
/// - Serializable dependency metadata for the package build artifact.
///
/// Transformation:
/// - Converts local and target-scoped dependency source variants into stable
///   strings while preserving the original package alias and source metadata.
fn build_package_dependency_metadata(
    dependency: &project_manifest::ProjectDependency,
) -> BuildPackageDependency {
    match &dependency.source {
        project_manifest::ProjectDependencySource::Path { path } => BuildPackageDependency {
            alias: dependency.alias.clone(),
            scope: package_dependency_scope(&dependency.scope).to_string(),
            source: "path".to_string(),
            path: Some(path.clone()),
            package: None,
            version: None,
            features: None,
        },
        project_manifest::ProjectDependencySource::Hex { package, version } => {
            BuildPackageDependency {
                alias: dependency.alias.clone(),
                scope: package_dependency_scope(&dependency.scope).to_string(),
                source: "hex".to_string(),
                path: None,
                package: Some(package.clone()),
                version: Some(version.clone()),
                features: None,
            }
        }
        project_manifest::ProjectDependencySource::Npm { package, version } => {
            BuildPackageDependency {
                alias: dependency.alias.clone(),
                scope: package_dependency_scope(&dependency.scope).to_string(),
                source: "npm".to_string(),
                path: None,
                package: Some(package.clone()),
                version: Some(version.clone()),
                features: None,
            }
        }
        project_manifest::ProjectDependencySource::Cargo {
            package,
            version,
            features,
        } => BuildPackageDependency {
            alias: dependency.alias.clone(),
            scope: package_dependency_scope(&dependency.scope).to_string(),
            source: "cargo".to_string(),
            path: None,
            package: Some(package.clone()),
            version: Some(version.clone()),
            features: (!features.is_empty()).then(|| features.clone()),
        },
    }
}

/// Returns the package metadata spelling for a dependency scope.
///
/// Inputs:
/// - `scope`: parsed dependency scope.
///
/// Output:
/// - Stable scope string for build metadata.
///
/// Transformation:
/// - Converts local and target-specific dependency scopes to the same section
///   names used by the manifest contract.
fn package_dependency_scope(scope: &project_manifest::ProjectDependencyScope) -> &'static str {
    match scope {
        project_manifest::ProjectDependencyScope::Local => "local",
        project_manifest::ProjectDependencyScope::Target(
            project_manifest::ProjectTarget::Erlang,
        ) => "target.erlang",
        project_manifest::ProjectDependencyScope::Target(project_manifest::ProjectTarget::Js) => {
            "target.js"
        }
        project_manifest::ProjectDependencyScope::Target(project_manifest::ProjectTarget::Rust) => {
            "target.rust"
        }
    }
}
