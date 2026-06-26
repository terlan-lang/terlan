use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde::Serialize;

use crate::commands::build::project_manifest::{
    read_project_manifest, ProjectArtifactKind, ProjectDependency, ProjectDependencyScope,
    ProjectDependencySource, ProjectErlangPackageAdapter, ProjectManifest, ProjectServerTls,
    ProjectServerTlsMode, ProjectServerTlsProvider, ProjectTarget, ProjectWebAssets,
};
use crate::{CliCommand, CliState};

const PROJECT_MANIFEST_FILE: &str = "terlan.toml";
const DEPLOY_PLAN_SCHEMA: &str = "terlan-cloud-deploy-plan-v1";
const DEPLOY_PLAN_FILE: &str = "deploy-plan.json";

/// Runs the hidden experimental deploy command group.
///
/// Inputs:
/// - `cmd`: parsed `deploy` command and command-local arguments.
/// - `state`: global CLI state, including the hidden experimental gate and
///   output directory.
///
/// Output:
/// - Process exit code for usage errors, manifest errors, or successful plan
///   emission.
///
/// Transformation:
/// - Requires `--experimental`, then routes `deploy plan` to a deterministic
///   manifest projection consumed by Terlan Cloud prototypes.
pub(crate) fn run(cmd: CliCommand, state: CliState) -> ExitCode {
    if !state.experimental {
        eprintln!("terlc deploy is experimental; rerun with --experimental to enable it.");
        return ExitCode::from(2);
    }

    match parse_deploy_args(&cmd.args) {
        DeployArgs::Help => {
            print_deploy_usage();
            ExitCode::SUCCESS
        }
        DeployArgs::Plan(args) => match write_deploy_plan(&args.project_dir, &state.out_dir) {
            Ok(path) => {
                println!("wrote {}", path.display());
                ExitCode::SUCCESS
            }
            Err(err) => {
                eprintln!("{err}");
                ExitCode::from(2)
            }
        },
        DeployArgs::Error(err) => {
            eprintln!("{err}");
            print_deploy_usage();
            ExitCode::from(2)
        }
    }
}

/// Parsed hidden deploy command variants.
///
/// Inputs:
/// - Produced from command-local arguments after the top-level parser has
///   stripped global options.
///
/// Output:
/// - Help, plan arguments, or a usage error.
///
/// Transformation:
/// - Keeps hidden deploy parsing local to the command so the public CLI usage
///   registry does not need to know about experimental subcommands.
enum DeployArgs {
    Help,
    Plan(DeployPlanArgs),
    Error(String),
}

/// Arguments for `terlc --experimental deploy plan`.
///
/// Inputs:
/// - Optional project directory operand.
///
/// Output:
/// - Normalized project directory path.
///
/// Transformation:
/// - Defaults omitted project directories to the current working directory.
struct DeployPlanArgs {
    project_dir: PathBuf,
}

/// Parses hidden deploy command arguments.
///
/// Inputs:
/// - `args`: command-local arguments after `deploy`.
///
/// Output:
/// - Parsed command shape or an error string.
///
/// Transformation:
/// - Accepts only `plan [project-dir]` and help flags for the experimental
///   cloud prototype surface.
fn parse_deploy_args(args: &[String]) -> DeployArgs {
    match args {
        [] => DeployArgs::Error("terlc deploy requires a subcommand: plan".to_string()),
        [flag] if matches!(flag.as_str(), "--help" | "-h") => DeployArgs::Help,
        [subcommand, rest @ ..] if subcommand == "plan" => parse_deploy_plan_args(rest),
        [subcommand, ..] => {
            DeployArgs::Error(format!("unknown terlc deploy subcommand: {subcommand}"))
        }
    }
}

/// Parses `deploy plan` operands.
///
/// Inputs:
/// - `args`: command-local arguments after `deploy plan`.
///
/// Output:
/// - Parsed plan arguments or a usage error.
///
/// Transformation:
/// - Accepts one optional project directory and rejects additional operands to
///   keep the cloud plan contract deterministic.
fn parse_deploy_plan_args(args: &[String]) -> DeployArgs {
    match args {
        [] => DeployArgs::Plan(DeployPlanArgs {
            project_dir: PathBuf::from("."),
        }),
        [flag] if matches!(flag.as_str(), "--help" | "-h") => DeployArgs::Help,
        [project_dir] => DeployArgs::Plan(DeployPlanArgs {
            project_dir: PathBuf::from(project_dir),
        }),
        _ => {
            DeployArgs::Error("terlc deploy plan accepts at most one project directory".to_string())
        }
    }
}

/// Prints hidden deploy command usage.
///
/// Inputs:
/// - None.
///
/// Output:
/// - Usage text written to stdout.
///
/// Transformation:
/// - Keeps experimental help reachable only after the hidden command is known,
///   while excluding it from top-level public help.
fn print_deploy_usage() {
    println!("terlc --experimental deploy plan [project-dir] [--out-dir <dir>]");
}

/// Writes a deterministic Terlan Cloud deploy plan artifact.
///
/// Inputs:
/// - `project_dir`: directory containing `terlan.toml`.
/// - `out_dir`: compiler output root selected by global `--out-dir`.
///
/// Output:
/// - Filesystem path to the generated JSON plan.
///
/// Transformation:
/// - Reads the existing project manifest parser output, projects it into a
///   cloud-facing schema, and writes `_build/cloud/deploy-plan.json`.
fn write_deploy_plan(project_dir: &Path, out_dir: &Path) -> Result<PathBuf, String> {
    let manifest_path = project_dir.join(PROJECT_MANIFEST_FILE);
    let manifest = read_project_manifest(&manifest_path)?;
    let plan = build_deploy_plan(&manifest);
    let cloud_dir = out_dir.join("cloud");
    fs::create_dir_all(&cloud_dir).map_err(|err| {
        format!(
            "cannot create deploy plan directory {}: {err}",
            cloud_dir.display()
        )
    })?;
    let output_path = cloud_dir.join(DEPLOY_PLAN_FILE);
    let json = serde_json::to_string_pretty(&plan)
        .map_err(|err| format!("cannot serialize deploy plan: {err}"))?;
    fs::write(&output_path, format!("{json}\n"))
        .map_err(|err| format!("cannot write deploy plan {}: {err}", output_path.display()))?;
    Ok(output_path)
}

/// Builds the cloud-facing deploy plan data model.
///
/// Inputs:
/// - `manifest`: parsed Terlan project manifest.
///
/// Output:
/// - Serializable deploy plan.
///
/// Transformation:
/// - Converts compiler-owned project metadata into stable cloud schema fields
///   without resolving dependencies, building source code, or contacting any
///   external service.
fn build_deploy_plan(manifest: &ProjectManifest) -> DeployPlan {
    let mut capabilities = deploy_capabilities(manifest);
    capabilities.sort();
    capabilities.dedup();

    let mut dependencies = manifest
        .dependencies
        .iter()
        .map(plan_dependency)
        .collect::<Vec<_>>();
    dependencies.sort_by(|left, right| {
        left.scope
            .cmp(&right.scope)
            .then_with(|| left.alias.cmp(&right.alias))
    });

    DeployPlan {
        schema: DEPLOY_PLAN_SCHEMA,
        generated_by: DeployPlanGenerator {
            tool: "terlc",
            version: env!("CARGO_PKG_VERSION"),
            experimental: true,
        },
        package: DeployPlanPackage {
            name: manifest.package.name.clone(),
            version: manifest.package.version.clone(),
            namespace: manifest.package.namespace.clone(),
        },
        build: DeployPlanBuild {
            artifact: manifest.artifact.as_str(),
            source_roots: manifest.source_roots.clone(),
            erlang_package_adapter: manifest.erlang_package_adapter.map(erlang_adapter_name),
        },
        capabilities,
        web_assets: manifest.web_assets.as_ref().map(plan_web_assets),
        server_tls: manifest.server_tls.as_ref().map(plan_server_tls),
        dependencies,
    }
}

/// Derives capability labels from project manifest sections.
///
/// Inputs:
/// - `manifest`: parsed project manifest.
///
/// Output:
/// - Unsorted capability labels.
///
/// Transformation:
/// - Records deploy-relevant manifest features without interpreting runtime
///   policy or dependency manager semantics.
fn deploy_capabilities(manifest: &ProjectManifest) -> Vec<&'static str> {
    let mut capabilities = match manifest.artifact {
        ProjectArtifactKind::BeamThin => vec!["runtime.beam"],
        ProjectArtifactKind::Library => vec!["artifact.library"],
        ProjectArtifactKind::WasmCore => vec!["runtime.wasm.core"],
        ProjectArtifactKind::WasmBrowser => vec!["runtime.wasm.browser"],
        ProjectArtifactKind::WasmComponent => vec!["runtime.wasm.component"],
        ProjectArtifactKind::WasiCli => vec!["runtime.wasi.cli"],
        ProjectArtifactKind::WasiHttp => vec!["runtime.wasi.http"],
        ProjectArtifactKind::WasiWorker => vec!["runtime.wasi.worker"],
    };
    if manifest.web_assets.is_some() {
        capabilities.push("web.assets");
    }
    if manifest
        .web_assets
        .as_ref()
        .and_then(|assets| assets.rsbuild_config.as_ref())
        .is_some()
    {
        capabilities.push("web.rsbuild");
    }
    if manifest.server_tls.is_some() {
        capabilities.push("http.tls");
    }
    if manifest.erlang_package_adapter.is_some() {
        capabilities.push("target.erlang.package");
    }
    for dependency in &manifest.dependencies {
        match dependency.scope {
            ProjectDependencyScope::Local => capabilities.push("dependency.local"),
            ProjectDependencyScope::Target(ProjectTarget::Erlang) => {
                capabilities.push("dependency.target.erlang")
            }
            ProjectDependencyScope::Target(ProjectTarget::Js) => {
                capabilities.push("dependency.target.js")
            }
            ProjectDependencyScope::Target(ProjectTarget::Rust) => {
                capabilities.push("dependency.target.rust")
            }
        }
    }
    capabilities
}

/// Converts web asset manifest metadata into plan metadata.
fn plan_web_assets(assets: &ProjectWebAssets) -> DeployPlanWebAssets {
    DeployPlanWebAssets {
        directory: assets.directory.clone(),
        public_path: assets.public_path.clone(),
        inline_limit: assets.inline_limit,
        rsbuild_config: assets.rsbuild_config.clone(),
    }
}

/// Converts server TLS manifest metadata into plan metadata.
fn plan_server_tls(tls: &ProjectServerTls) -> DeployPlanServerTls {
    DeployPlanServerTls {
        mode: tls_mode_name(tls.mode),
        domains: tls.domains.clone(),
        email: tls.email.clone(),
        primary_provider: tls.primary_provider.map(tls_provider_name),
        fallback_provider: tls.fallback_provider.map(tls_provider_name),
        cert: tls.cert.clone(),
        key: tls.key.clone(),
        passphrase_env: tls.passphrase_env.clone(),
        ca: tls.ca.clone(),
        server_name: tls.server_name.clone(),
        trust_local: tls.trust_local,
    }
}

/// Converts one manifest dependency into plan metadata.
fn plan_dependency(dependency: &ProjectDependency) -> DeployPlanDependency {
    DeployPlanDependency {
        alias: dependency.alias.clone(),
        scope: dependency_scope_name(dependency.scope),
        source: match &dependency.source {
            ProjectDependencySource::Path { path } => {
                DeployPlanDependencySource::Path { path: path.clone() }
            }
            ProjectDependencySource::Hex { package, version } => DeployPlanDependencySource::Hex {
                package: package.clone(),
                version: version.clone(),
            },
            ProjectDependencySource::Npm { package, version } => DeployPlanDependencySource::Npm {
                package: package.clone(),
                version: version.clone(),
            },
            ProjectDependencySource::Cargo {
                package,
                version,
                features,
            } => DeployPlanDependencySource::Cargo {
                package: package.clone(),
                version: version.clone(),
                features: features.clone(),
            },
        },
    }
}

/// Returns the deploy-plan spelling for a dependency scope.
///
/// Inputs: project dependency scope from `terlan.toml`.
/// Output: stable deploy-plan scope label.
/// Transformation: maps typed manifest variants to cloud artifact strings.
fn dependency_scope_name(scope: ProjectDependencyScope) -> &'static str {
    match scope {
        ProjectDependencyScope::Local => "local",
        ProjectDependencyScope::Target(ProjectTarget::Erlang) => "target.erlang",
        ProjectDependencyScope::Target(ProjectTarget::Js) => "target.js",
        ProjectDependencyScope::Target(ProjectTarget::Rust) => "target.rust",
    }
}

/// Returns the deploy-plan spelling for an Erlang package adapter.
///
/// Inputs: typed Erlang adapter from project manifest parsing.
/// Output: stable adapter label.
/// Transformation: preserves the adapter contract without exposing enum names.
fn erlang_adapter_name(adapter: ProjectErlangPackageAdapter) -> &'static str {
    match adapter {
        ProjectErlangPackageAdapter::Rebar3Compatible => "rebar3-compatible",
    }
}

/// Returns the deploy-plan spelling for TLS mode.
///
/// Inputs: typed server TLS mode.
/// Output: stable TLS mode label.
/// Transformation: maps compiler manifest enum values into deploy JSON values.
fn tls_mode_name(mode: ProjectServerTlsMode) -> &'static str {
    match mode {
        ProjectServerTlsMode::Auto => "auto",
        ProjectServerTlsMode::Manual => "manual",
        ProjectServerTlsMode::Internal => "internal",
    }
}

/// Returns the deploy-plan spelling for an ACME provider.
///
/// Inputs: typed TLS provider.
/// Output: stable provider label.
/// Transformation: maps provider enum values into public deploy-plan strings.
fn tls_provider_name(provider: ProjectServerTlsProvider) -> &'static str {
    match provider {
        ProjectServerTlsProvider::LetsEncrypt => "lets-encrypt",
        ProjectServerTlsProvider::ZeroSsl => "zerossl",
    }
}

/// Serializable Terlan Cloud deploy plan.
///
/// Inputs: projected from a parsed project manifest.
/// Output: JSON artifact consumed by cloud tooling.
/// Transformation: groups manifest data into package, build, capability,
/// assets, TLS, and dependency sections.
#[derive(Serialize)]
struct DeployPlan {
    schema: &'static str,
    generated_by: DeployPlanGenerator,
    package: DeployPlanPackage,
    build: DeployPlanBuild,
    capabilities: Vec<&'static str>,
    web_assets: Option<DeployPlanWebAssets>,
    server_tls: Option<DeployPlanServerTls>,
    dependencies: Vec<DeployPlanDependency>,
}

/// Generator metadata embedded in deploy plans.
///
/// Inputs: compiler identity and experimental status.
/// Output: JSON metadata describing the producing tool.
/// Transformation: records release provenance for future cloud validation.
#[derive(Serialize)]
struct DeployPlanGenerator {
    tool: &'static str,
    version: &'static str,
    experimental: bool,
}

/// Package identity section of a deploy plan.
///
/// Inputs: project manifest package metadata.
/// Output: JSON package identity.
/// Transformation: preserves name/version/namespace without target details.
#[derive(Serialize)]
struct DeployPlanPackage {
    name: String,
    version: String,
    namespace: Option<String>,
}

/// Build section of a deploy plan.
///
/// Inputs: manifest build roots, artifact kind, and BEAM adapter.
/// Output: JSON build contract.
/// Transformation: converts filesystem-oriented source roots into strings.
#[derive(Serialize)]
struct DeployPlanBuild {
    artifact: &'static str,
    source_roots: Vec<String>,
    erlang_package_adapter: Option<&'static str>,
}

/// Web asset section of a deploy plan.
///
/// Inputs: optional manifest web asset settings.
/// Output: JSON asset configuration.
/// Transformation: preserves asset directory, public path, inline limit, and
/// bundler config as cloud-visible metadata.
#[derive(Serialize)]
struct DeployPlanWebAssets {
    directory: String,
    public_path: Option<String>,
    inline_limit: Option<u64>,
    rsbuild_config: Option<String>,
}

/// Server TLS section of a deploy plan.
///
/// Inputs: optional manifest TLS configuration.
/// Output: JSON TLS deployment requirements.
/// Transformation: keeps auto/manual/internal TLS fields explicit for cloud
/// validation and provisioning.
#[derive(Serialize)]
struct DeployPlanServerTls {
    mode: &'static str,
    domains: Vec<String>,
    email: Option<String>,
    primary_provider: Option<&'static str>,
    fallback_provider: Option<&'static str>,
    cert: Option<String>,
    key: Option<String>,
    passphrase_env: Option<String>,
    ca: Option<String>,
    server_name: Option<String>,
    trust_local: Option<bool>,
}

/// Dependency section entry of a deploy plan.
///
/// Inputs: one named manifest dependency.
/// Output: JSON dependency entry with scope and source.
/// Transformation: separates alias, scope label, and source payload.
#[derive(Serialize)]
struct DeployPlanDependency {
    alias: String,
    scope: &'static str,
    source: DeployPlanDependencySource,
}

/// Dependency source payload for deploy plans.
///
/// Inputs: typed dependency source from project manifest parsing.
/// Output: tagged JSON source payload.
/// Transformation: preserves path, Hex, npm, and Cargo-specific fields behind
/// stable `kind` tags.
#[derive(Serialize)]
#[serde(tag = "kind")]
enum DeployPlanDependencySource {
    #[serde(rename = "path")]
    Path { path: String },
    #[serde(rename = "hex")]
    Hex { package: String, version: String },
    #[serde(rename = "npm")]
    Npm { package: String, version: String },
    #[serde(rename = "cargo")]
    Cargo {
        package: String,
        version: String,
        features: Vec<String>,
    },
}

#[cfg(test)]
#[path = "deploy_test.rs"]
mod deploy_test;
