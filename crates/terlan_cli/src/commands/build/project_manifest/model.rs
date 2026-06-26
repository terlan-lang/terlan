/// Parsed Terlan project manifest.
///
/// Inputs:
/// - Produced from `terlan.toml`.
///
/// Output:
/// - Build-owned project metadata used by future project-directory builds.
///
/// Transformation:
/// - Stores the package identity, declared source roots, and requested artifact
///   kind after project package validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectManifest {
    pub(crate) package: ProjectPackage,
    pub(crate) source_roots: Vec<String>,
    pub(crate) artifact: ProjectArtifactKind,
    pub(crate) wasm_target: Option<ProjectWasmTarget>,
    pub(crate) wasi_target: Option<ProjectWasiTarget>,
    pub(crate) web_assets: Option<ProjectWebAssets>,
    pub(crate) server_tls: Option<ProjectServerTls>,
    pub(crate) native_rust: Option<ProjectNativeRust>,
    pub(crate) dependencies: Vec<ProjectDependency>,
    pub(crate) erlang_package_adapter: Option<ProjectErlangPackageAdapter>,
}

/// Parsed package metadata from `[package]`.
///
/// Inputs:
/// - Produced from the manifest `[package]` table.
///
/// Output:
/// - Stable package identity for project build diagnostics and future artifact
///   metadata.
///
/// Transformation:
/// - Keeps only fields admitted into the A0.42.1 project package boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectPackage {
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) namespace: Option<String>,
}

/// Parsed executable artifact kind from `[build]`.
///
/// Inputs:
/// - Produced from the manifest `[build] artifact` value or its default.
///
/// Output:
/// - Stable artifact kind for target packaging decisions.
///
/// Transformation:
/// - Narrows manifest text to the package artifact modes admitted by the
///   current 0.0.1 package contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectArtifactKind {
    BeamThin,
    Library,
    WasmCore,
    WasmBrowser,
    WasmComponent,
    WasiCli,
    WasiHttp,
    WasiWorker,
}

impl ProjectArtifactKind {
    /// Returns the manifest spelling for the artifact kind.
    ///
    /// Inputs:
    /// - `self`: parsed artifact kind.
    ///
    /// Output:
    /// - Static manifest spelling.
    ///
    /// Transformation:
    /// - Converts the enum to the package-contract string used in diagnostics
    ///   and build metadata.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            ProjectArtifactKind::BeamThin => "beam-thin",
            ProjectArtifactKind::Library => "library",
            ProjectArtifactKind::WasmCore => "wasm-core",
            ProjectArtifactKind::WasmBrowser => "wasm-browser",
            ProjectArtifactKind::WasmComponent => "wasm-component",
            ProjectArtifactKind::WasiCli => "wasi-cli",
            ProjectArtifactKind::WasiHttp => "wasi-http",
            ProjectArtifactKind::WasiWorker => "wasi-worker",
        }
    }
}

/// Parsed WebAssembly target reservation from `[target.wasm]`.
///
/// Inputs:
/// - Produced from user-authored Wasm target metadata.
///
/// Output:
/// - Stable Wasm profile, export, world, and validation-engine metadata.
///
/// Transformation:
/// - Records the target contract without enabling Wasm byte emission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectWasmTarget {
    pub(crate) profile: ProjectWasmProfile,
    pub(crate) exports: Vec<String>,
    pub(crate) bridge: Option<String>,
    pub(crate) capabilities: Vec<String>,
    pub(crate) world: Option<String>,
    pub(crate) validation_engine: Option<String>,
}

/// Supported reserved WebAssembly manifest profiles.
///
/// Inputs:
/// - Produced from `[target.wasm] profile`.
///
/// Output:
/// - Typed Wasm target profile marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectWasmProfile {
    Core,
    Browser,
    Component,
}

impl ProjectWasmProfile {
    /// Returns the manifest spelling for the Wasm profile.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            ProjectWasmProfile::Core => "core",
            ProjectWasmProfile::Browser => "browser",
            ProjectWasmProfile::Component => "component",
        }
    }
}

/// Parsed WASI target reservation from `[target.wasi]`.
///
/// Inputs:
/// - Produced from user-authored WASI target metadata.
///
/// Output:
/// - Stable WASI profile, world, capability, and validation-engine metadata.
///
/// Transformation:
/// - Records the target contract without enabling WASI component emission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectWasiTarget {
    pub(crate) profile: ProjectWasiProfile,
    pub(crate) world: Option<String>,
    pub(crate) capabilities: Vec<String>,
    pub(crate) validation_engine: Option<String>,
}

/// Supported reserved WASI manifest profiles.
///
/// Inputs:
/// - Produced from `[target.wasi] profile`.
///
/// Output:
/// - Typed WASI target profile marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectWasiProfile {
    Cli,
    Http,
    Worker,
}

impl ProjectWasiProfile {
    /// Returns the manifest spelling for the WASI profile.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            ProjectWasiProfile::Cli => "cli",
            ProjectWasiProfile::Http => "http",
            ProjectWasiProfile::Worker => "worker",
        }
    }
}

/// Parsed Terlan-owned web asset configuration from `[web.assets]`.
///
/// Inputs:
/// - Produced from user-authored `terlan.toml`.
///
/// Output:
/// - Stable web asset configuration for browser packaging.
///
/// Transformation:
/// - Keeps the user-facing asset shape independent from Rsbuild/Rspack config
///   while preserving enough metadata for later packaging translation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectWebAssets {
    pub(crate) directory: String,
    pub(crate) public_path: Option<String>,
    pub(crate) inline_limit: Option<u64>,
    pub(crate) rsbuild_config: Option<String>,
}

/// Parsed Terlan-owned HTTP server TLS configuration from `[server.tls]`.
///
/// Inputs:
/// - Produced from user-authored `terlan.toml`.
///
/// Output:
/// - Stable TLS configuration metadata for local server/runtime validation.
///
/// Transformation:
/// - Narrows declarative TLS settings to the roadmap-approved modes without
///   loading certificates, contacting ACME providers, or starting a server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectServerTls {
    pub(crate) mode: ProjectServerTlsMode,
    pub(crate) domains: Vec<String>,
    pub(crate) email: Option<String>,
    pub(crate) primary_provider: Option<ProjectServerTlsProvider>,
    pub(crate) fallback_provider: Option<ProjectServerTlsProvider>,
    pub(crate) cert: Option<String>,
    pub(crate) key: Option<String>,
    pub(crate) passphrase_env: Option<String>,
    pub(crate) ca: Option<String>,
    pub(crate) server_name: Option<String>,
    pub(crate) trust_local: Option<bool>,
}

/// Parsed Rust native helper metadata from `[native.rust]`.
///
/// Inputs:
/// - Produced from a package manifest that declares a Rust native adapter.
///
/// Output:
/// - Stable helper discovery metadata for package build artifacts.
///
/// Transformation:
/// - Records the Rust crate directory and BEAM helper executable contract
///   without building Cargo targets or mutating the host environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectNativeRust {
    pub(crate) crate_name: String,
    pub(crate) path: String,
    pub(crate) helper: String,
    pub(crate) helper_env: String,
    pub(crate) features: Vec<String>,
}

/// Supported TLS configuration modes.
///
/// Inputs:
/// - Produced from `[server.tls] mode`.
///
/// Output:
/// - Typed TLS mode for runtime/server validation.
///
/// Transformation:
/// - Keeps automatic ACME, manual certificate, and internal development CA
///   modes explicit so later runtime code can validate capabilities by mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectServerTlsMode {
    Auto,
    Manual,
    Internal,
}

/// Supported automatic TLS ACME providers.
///
/// Inputs:
/// - Produced from `[server.tls] primary_provider` or `fallback_provider`.
///
/// Output:
/// - Typed provider marker for future ACME runtime integration.
///
/// Transformation:
/// - Narrows provider names to the documented Let's Encrypt / ZeroSSL pair
///   without binding the compiler to one ACME implementation yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectServerTlsProvider {
    LetsEncrypt,
    ZeroSsl,
}

/// Parsed Erlang target packaging adapter reservation.
///
/// Inputs:
/// - Produced from `[target.erlang.package] adapter`.
///
/// Output:
/// - Stable adapter marker for downstream Erlang packaging metadata.
///
/// Transformation:
/// - Narrows manifest text to the adapter metadata admitted by A0.42.6 without
///   generating Rebar3 files or requiring Rebar3 during normal builds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectErlangPackageAdapter {
    Rebar3Compatible,
}

impl ProjectErlangPackageAdapter {
    /// Returns the manifest spelling for the Erlang package adapter.
    ///
    /// Inputs:
    /// - `self`: parsed Erlang package adapter.
    ///
    /// Output:
    /// - Static manifest spelling.
    ///
    /// Transformation:
    /// - Converts the enum to the package metadata string used by build
    ///   artifacts and diagnostics.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            ProjectErlangPackageAdapter::Rebar3Compatible => "rebar3-compatible",
        }
    }
}

/// Parsed project dependency metadata.
///
/// Inputs:
/// - Produced from `[dependencies]` or `[target.<name>.dependencies]` manifest
///   entries.
///
/// Output:
/// - Typed dependency metadata for future dependency-closure and target-adapter
///   validation.
///
/// Transformation:
/// - Keeps the manifest alias, dependency scope, and source kind without
///   fetching, resolving, linking, or packaging the dependency.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectDependency {
    pub(crate) alias: String,
    pub(crate) scope: ProjectDependencyScope,
    pub(crate) source: ProjectDependencySource,
}

/// Scope where a project dependency applies.
///
/// Inputs:
/// - Produced from the manifest dependency section name.
///
/// Output:
/// - Portable/local dependency scope or target-specific dependency scope.
///
/// Transformation:
/// - Separates local Terlan package dependencies from target ecosystem
///   dependency metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectDependencyScope {
    Local,
    Target(ProjectTarget),
}

/// Target namespace for target-scoped dependency metadata.
///
/// Inputs:
/// - Produced from `[target.<name>.dependencies]` section names.
///
/// Output:
/// - Known target namespace.
///
/// Transformation:
/// - Narrows manifest target names to package metadata scopes currently
///   recognized by the compiler roadmap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectTarget {
    Erlang,
    Js,
    Rust,
}

/// Parsed dependency source kind.
///
/// Inputs:
/// - Produced from one inline dependency table.
///
/// Output:
/// - Source-specific dependency metadata.
///
/// Transformation:
/// - Preserves `path`, `hex`, `npm`, and `cargo` dependency source kinds
///   without performing dependency resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProjectDependencySource {
    Path {
        path: String,
    },
    Hex {
        package: String,
        version: String,
    },
    Npm {
        package: String,
        version: String,
    },
    Cargo {
        package: String,
        version: String,
        features: Vec<String>,
    },
}
