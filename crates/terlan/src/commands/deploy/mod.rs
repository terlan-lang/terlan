use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde::Serialize;

use crate::commands::build::project_manifest::{
    read_project_manifest, ProjectArtifactKind, ProjectDependency, ProjectDependencyScope,
    ProjectDependencySource, ProjectManifest, ProjectServerTls, ProjectServerTlsMode,
    ProjectServerTlsProvider, ProjectTarget, ProjectWebAssets,
};
use crate::{CliCommand, CliState};

mod semantic;

use semantic::{build_semantic_deploy_plan, render_semantic_deploy_plan};

const PROJECT_MANIFEST_FILE: &str = "terlan.toml";
const DEPLOY_PLAN_SCHEMA: &str = "terlan-cloud-deploy-plan-v1";
const SEMANTIC_DEPLOY_PLAN_SCHEMA: &str = "terlan-cloud-deploy-plan-v2";
const DEPLOY_PLAN_FILE: &str = "deploy-plan.json";
const DEPLOY_PLAN_INSPECTION_FILE: &str = "deploy-plan.txt";

/// Produces the semantic deploy plan as a deterministic JSON value for other
/// compiler-owned artifact emitters.
///
/// Release-bundle generation reuses this function so build and deploy cannot
/// drift into separate route, capability, migration, or source contracts.
pub(crate) fn semantic_deploy_plan_value(
    project_dir: &Path,
    manifest: &ProjectManifest,
) -> Result<serde_json::Value, String> {
    serde_json::to_value(build_semantic_deploy_plan(project_dir, manifest)?)
        .map_err(|error| format!("cannot serialize semantic deploy plan: {error}"))
}

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
        DeployArgs::Plan(args) => match write_deploy_plan(&args, &state.out_dir) {
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
    schema: DeployPlanSchema,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeployPlanSchema {
    V1,
    V2,
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
    let mut project_dir = None;
    let mut schema = DeployPlanSchema::V2;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--help" | "-h" => return DeployArgs::Help,
            "--schema" => {
                let Some(value) = args.get(index + 1) else {
                    return DeployArgs::Error(
                        "terlc deploy plan --schema requires v1 or v2".to_string(),
                    );
                };
                schema = match value.as_str() {
                    "v1" => DeployPlanSchema::V1,
                    "v2" => DeployPlanSchema::V2,
                    other => {
                        return DeployArgs::Error(format!(
                            "unsupported deploy plan schema `{other}`; supported schemas: v1, v2"
                        ))
                    }
                };
                index += 2;
            }
            value if value.starts_with('-') => {
                return DeployArgs::Error(format!(
                    "unexpected terlc deploy plan argument: {value}"
                ));
            }
            value => {
                if project_dir.is_some() {
                    return DeployArgs::Error(
                        "terlc deploy plan accepts at most one project directory".to_string(),
                    );
                }
                project_dir = Some(PathBuf::from(value));
                index += 1;
            }
        }
    }
    DeployArgs::Plan(DeployPlanArgs {
        project_dir: project_dir.unwrap_or_else(|| PathBuf::from(".")),
        schema,
    })
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
    println!("terlc --experimental deploy plan [project-dir] [--schema v1|v2] [--out-dir <dir>]");
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
fn write_deploy_plan(args: &DeployPlanArgs, out_dir: &Path) -> Result<PathBuf, String> {
    let manifest_path = args.project_dir.join(PROJECT_MANIFEST_FILE);
    let manifest = read_project_manifest(&manifest_path)?;
    let cloud_dir = out_dir.join("cloud");
    fs::create_dir_all(&cloud_dir).map_err(|err| {
        format!(
            "cannot create deploy plan directory {}: {err}",
            cloud_dir.display()
        )
    })?;
    let output_path = cloud_dir.join(DEPLOY_PLAN_FILE);
    let (json, inspection) = match args.schema {
        DeployPlanSchema::V1 => (
            serde_json::to_string_pretty(&build_deploy_plan(&manifest)?)
                .map_err(|err| format!("cannot serialize deploy plan: {err}"))?,
            None,
        ),
        DeployPlanSchema::V2 => {
            let plan = build_semantic_deploy_plan(&args.project_dir, &manifest)?;
            let inspection = render_semantic_deploy_plan(&plan);
            (
                serde_json::to_string_pretty(&plan)
                    .map_err(|err| format!("cannot serialize deploy plan: {err}"))?,
                Some(inspection),
            )
        }
    };
    fs::write(&output_path, format!("{json}\n"))
        .map_err(|err| format!("cannot write deploy plan {}: {err}", output_path.display()))?;
    if let Some(inspection) = inspection {
        let inspection_path = cloud_dir.join(DEPLOY_PLAN_INSPECTION_FILE);
        fs::write(&inspection_path, inspection).map_err(|err| {
            format!(
                "cannot write deploy plan inspection {}: {err}",
                inspection_path.display()
            )
        })?;
    }
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
fn build_deploy_plan(manifest: &ProjectManifest) -> Result<DeployPlan, String> {
    validate_manifest_deployable(manifest)?;

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
            .cmp(right.scope)
            .then_with(|| left.alias.cmp(&right.alias))
    });

    Ok(DeployPlan {
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
        },
        capabilities,
        web_assets: manifest.web_assets.as_ref().map(plan_web_assets),
        server_tls: manifest.server_tls.as_ref().map(plan_server_tls),
        dependencies,
    })
}

/// Validates that the manifest can produce a 0.0.7 deploy plan.
///
/// Inputs:
/// - `manifest`: parsed project manifest.
///
/// Output:
/// - `Ok(())` when deploy planning can stay on current Terlan-owned runtime
///   surfaces.
/// - Stable error text for deploy-incompatible manifest metadata.
///
/// Transformation:
/// - Keeps deploy planning on current Terlan-owned runtime surfaces.
/// - Deploy planning does not support legacy `beam-thin` artifacts and does
///   not support legacy [target.erlang.dependencies] metadata.
/// - Audit marker: does not support legacy [target.erlang.dependencies] metadata.
fn validate_manifest_deployable(manifest: &ProjectManifest) -> Result<(), String> {
    let _ = manifest;
    Ok(())
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
        ProjectArtifactKind::TerlanVm => vec!["runtime.terlan-vm"],
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
    if manifest.native_rust.is_some() {
        capabilities.push("native.rust");
        capabilities.push("native.helper-process");
    }
    for dependency in &manifest.dependencies {
        match dependency.scope {
            ProjectDependencyScope::Local => capabilities.push("dependency.local"),
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
            ProjectDependencySource::Git { url, rev } => DeployPlanDependencySource::Git {
                url: url.clone(),
                rev: rev.clone(),
            },
            ProjectDependencySource::Registry { registry, version } => {
                DeployPlanDependencySource::Registry {
                    registry: registry.clone(),
                    version: version.clone(),
                }
            }
            ProjectDependencySource::Npm {
                package,
                version,
                integrity,
            } => DeployPlanDependencySource::Npm {
                package: package.clone(),
                version: version.clone(),
                integrity: integrity.clone(),
            },
            ProjectDependencySource::Cargo {
                package,
                version,
                integrity,
                features,
            } => DeployPlanDependencySource::Cargo {
                package: package.clone(),
                version: version.clone(),
                integrity: integrity.clone(),
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
        ProjectDependencyScope::Target(ProjectTarget::Js) => "target.js",
        ProjectDependencyScope::Target(ProjectTarget::Rust) => "target.rust",
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
/// Inputs: manifest build roots and artifact kind.
/// Output: JSON build contract.
/// Transformation: converts filesystem-oriented source roots into strings.
#[derive(Serialize)]
struct DeployPlanBuild {
    artifact: &'static str,
    source_roots: Vec<String>,
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
/// Transformation: preserves path, Git, npm, and Cargo-specific fields
/// behind stable `kind` tags.
#[derive(Serialize)]
#[serde(tag = "kind")]
enum DeployPlanDependencySource {
    #[serde(rename = "path")]
    Path { path: String },
    #[serde(rename = "git")]
    Git { url: String, rev: String },
    #[serde(rename = "registry")]
    Registry { registry: String, version: String },
    #[serde(rename = "npm")]
    Npm {
        package: String,
        version: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        integrity: Option<String>,
    },
    #[serde(rename = "cargo")]
    Cargo {
        package: String,
        version: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        integrity: Option<String>,
        features: Vec<String>,
    },
}

#[cfg(test)]
#[path = "deploy_test.rs"]
#[cfg(test)]
mod deploy_test;
