use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::commands::build::project_manifest::{
    ProjectArtifactKind, ProjectDeployment, ProjectManifest, ProjectRollbackCompatibility,
};
use crate::compiler::api_contract::{imports_std_http_router, routes_from_syntax_module};
use crate::terlan_syntax::parse_module_as_syntax_output;

use super::{
    deploy_capabilities, plan_dependency, plan_server_tls, plan_web_assets, DeployPlanDependency,
    DeployPlanGenerator, DeployPlanServerTls, DeployPlanWebAssets, SEMANTIC_DEPLOY_PLAN_SCHEMA,
};

#[derive(Serialize)]
pub(super) struct SemanticDeployPlan {
    schema: &'static str,
    generated_by: DeployPlanGenerator,
    release: SemanticRelease,
    target: SemanticTarget,
    services: Vec<SemanticService>,
    routes: Vec<SemanticRoute>,
    capabilities: Vec<&'static str>,
    web_assets: Option<DeployPlanWebAssets>,
    server_tls: Option<DeployPlanServerTls>,
    native_packages: Vec<SemanticNativePackage>,
    configuration: SemanticConfiguration,
    migrations: Vec<SemanticMigration>,
    resources: SemanticResources,
    outbound_network: Vec<String>,
    sources: Vec<SemanticSource>,
    rollback: SemanticRollback,
    dependencies: Vec<DeployPlanDependency>,
}

#[derive(Debug, Serialize)]
struct SemanticRelease {
    id: String,
    package: String,
    version: String,
}

#[derive(Debug, Serialize)]
struct SemanticTarget {
    artifact: &'static str,
    runtime: &'static str,
}

#[derive(Debug, Serialize)]
struct SemanticService {
    name: String,
    runtime: &'static str,
    process: SemanticProcess,
    health_check: Option<SemanticHealthCheck>,
}

#[derive(Debug, Serialize)]
struct SemanticProcess {
    entrypoint: String,
    count: u64,
}

#[derive(Debug, Serialize)]
struct SemanticHealthCheck {
    path: String,
    interval_secs: u64,
    timeout_secs: u64,
}

#[derive(Debug, Serialize)]
struct SemanticRoute {
    service: String,
    method: String,
    path: String,
    handler: String,
    source: SemanticRouteSource,
}

#[derive(Debug, Serialize)]
struct SemanticRouteSource {
    module: String,
    path: String,
}

#[derive(Debug, Serialize)]
struct SemanticConfiguration {
    environment: Vec<String>,
    secrets: Vec<SemanticSecretReference>,
}

#[derive(Debug, Serialize)]
struct SemanticSecretReference {
    name: String,
    provider: &'static str,
}

#[derive(Debug, Serialize)]
struct SemanticNativePackage {
    crate_name: String,
    path: String,
    helper: String,
    helper_env: String,
    features: Vec<String>,
}

#[derive(Debug, Serialize)]
struct SemanticMigration {
    id: String,
    path: String,
    sha256: String,
}

#[derive(Debug, Serialize)]
struct SemanticResources {
    cpu_millis: Option<u64>,
    memory_mb: Option<u64>,
    processes: u64,
}

#[derive(Debug, Serialize)]
struct SemanticSource {
    module: String,
    path: String,
    sha256: String,
}

#[derive(Debug, Serialize)]
struct SemanticRollback {
    policy: &'static str,
    automatic: bool,
}

pub(super) fn build_semantic_deploy_plan(
    project_dir: &Path,
    manifest: &ProjectManifest,
) -> Result<SemanticDeployPlan, String> {
    validate_semantic_target(manifest)?;
    let deployment = manifest.deployment.as_ref();
    let sources = collect_sources(project_dir, manifest)?;
    let routes = collect_routes(project_dir, &manifest.package.name, &sources)?;
    validate_health_route(deployment, &routes)?;

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

    let package_module = manifest
        .package
        .namespace
        .clone()
        .unwrap_or_else(|| manifest.package.name.replace('-', "_"));
    let resources = deployment.and_then(|value| value.resources.as_ref());
    let process_count = resources.map_or(1, |value| value.processes);
    let health_check = deployment
        .and_then(|value| value.health.as_ref())
        .map(|health| SemanticHealthCheck {
            path: health.path.clone(),
            interval_secs: health.interval_secs,
            timeout_secs: health.timeout_secs,
        });
    let service_name = manifest.package.name.clone();

    Ok(SemanticDeployPlan {
        schema: SEMANTIC_DEPLOY_PLAN_SCHEMA,
        generated_by: DeployPlanGenerator {
            tool: "terlc",
            version: env!("CARGO_PKG_VERSION"),
            experimental: true,
        },
        release: SemanticRelease {
            id: format!("{}@{}", manifest.package.name, manifest.package.version),
            package: manifest.package.name.clone(),
            version: manifest.package.version.clone(),
        },
        target: SemanticTarget {
            artifact: manifest.artifact.as_str(),
            runtime: "terlan-vm",
        },
        services: vec![SemanticService {
            name: service_name.clone(),
            runtime: "terlan-vm",
            process: SemanticProcess {
                entrypoint: format!("{package_module}.Main.main/0"),
                count: process_count,
            },
            health_check,
        }],
        routes: routes
            .into_iter()
            .map(|route| SemanticRoute {
                service: service_name.clone(),
                method: route.method,
                path: route.path,
                handler: format!("{}.{}/1", route.module, route.handler),
                source: SemanticRouteSource {
                    module: route.module,
                    path: route.source_path,
                },
            })
            .collect(),
        capabilities,
        web_assets: manifest.web_assets.as_ref().map(plan_web_assets),
        server_tls: manifest.server_tls.as_ref().map(plan_server_tls),
        native_packages: manifest
            .native_rust
            .iter()
            .map(|native| SemanticNativePackage {
                crate_name: native.crate_name.clone(),
                path: native.path.clone(),
                helper: native.helper.clone(),
                helper_env: native.helper_env.clone(),
                features: native.features.clone(),
            })
            .collect(),
        configuration: SemanticConfiguration {
            environment: deployment
                .map(|value| value.environment.clone())
                .unwrap_or_default(),
            secrets: deployment
                .map(|value| {
                    value
                        .secrets
                        .iter()
                        .map(|name| SemanticSecretReference {
                            name: name.clone(),
                            provider: "environment",
                        })
                        .collect()
                })
                .unwrap_or_default(),
        },
        migrations: collect_migrations(project_dir, deployment)?,
        resources: SemanticResources {
            cpu_millis: resources.map(|value| value.cpu_millis),
            memory_mb: resources.map(|value| value.memory_mb),
            processes: process_count,
        },
        outbound_network: deployment
            .map(|value| value.outbound_network.clone())
            .unwrap_or_default(),
        sources: sources
            .into_iter()
            .map(|source| SemanticSource {
                module: source.module,
                path: source.relative_path,
                sha256: source.sha256,
            })
            .collect(),
        rollback: rollback_contract(deployment),
        dependencies,
    })
}

pub(super) fn render_semantic_deploy_plan(plan: &SemanticDeployPlan) -> String {
    let mut output = String::new();
    output.push_str(&format!("Release: {}\n", plan.release.id));
    output.push_str(&format!(
        "Target: {} ({})\n",
        plan.target.artifact, plan.target.runtime
    ));
    output.push_str("Services:\n");
    for service in &plan.services {
        output.push_str(&format!(
            "  - {}: {} x{}\n",
            service.name, service.process.entrypoint, service.process.count
        ));
        if let Some(health) = &service.health_check {
            output.push_str(&format!(
                "    health: {} every {}s, timeout {}s\n",
                health.path, health.interval_secs, health.timeout_secs
            ));
        }
    }
    output.push_str("Routes:\n");
    for route in &plan.routes {
        output.push_str(&format!(
            "  - {} {} -> {} ({})\n",
            route.method, route.path, route.handler, route.source.path
        ));
    }
    output.push_str(&format!(
        "Configuration: {} environment names, {} secret references\n",
        plan.configuration.environment.len(),
        plan.configuration.secrets.len()
    ));
    output.push_str(&format!("Migrations: {}\n", plan.migrations.len()));
    output.push_str(&format!(
        "Native packages: {}\n",
        plan.native_packages.len()
    ));
    output.push_str(&format!(
        "Resources: cpu={}m memory={}MB processes={}\n",
        plan.resources
            .cpu_millis
            .map_or_else(|| "unspecified".to_string(), |value| value.to_string()),
        plan.resources
            .memory_mb
            .map_or_else(|| "unspecified".to_string(), |value| value.to_string()),
        plan.resources.processes
    ));
    output.push_str(&format!(
        "Outbound network: {} declaration(s)\n",
        plan.outbound_network.len()
    ));
    output.push_str(&format!("Rollback: {}\n", plan.rollback.policy));
    output
}

fn validate_semantic_target(manifest: &ProjectManifest) -> Result<(), String> {
    if manifest.artifact != ProjectArtifactKind::TerlanVm {
        return Err(format!(
            "error[deploy.target_capability]: semantic Cloud deploy plans currently support only `terlan-vm`, found `{}`",
            manifest.artifact.as_str()
        ));
    }
    Ok(())
}

#[derive(Debug)]
struct CollectedSource {
    module: String,
    relative_path: String,
    sha256: String,
    text: String,
}

#[derive(Debug)]
struct CollectedRoute {
    method: String,
    path: String,
    handler: String,
    module: String,
    source_path: String,
}

fn collect_sources(
    project_dir: &Path,
    manifest: &ProjectManifest,
) -> Result<Vec<CollectedSource>, String> {
    let mut paths = Vec::new();
    for source_root in &manifest.source_roots {
        collect_source_paths(&project_dir.join(source_root), &mut paths)?;
    }
    paths.sort();
    let mut modules = BTreeSet::new();
    let mut sources = Vec::new();
    for path in paths {
        let bytes = fs::read(&path)
            .map_err(|error| format!("cannot read deploy source {}: {error}", path.display()))?;
        let text = String::from_utf8(bytes.clone())
            .map_err(|_| format!("deploy source is not UTF-8: {}", path.display()))?;
        let syntax = parse_module_as_syntax_output(&text).map_err(|error| {
            format!(
                "error[deploy.source_parse]: cannot parse {}: {error:?}",
                path.display()
            )
        })?;
        if !modules.insert(syntax.module_name.clone()) {
            return Err(format!(
                "error[deploy.source_identity]: duplicate module `{}`",
                syntax.module_name
            ));
        }
        sources.push(CollectedSource {
            module: syntax.module_name,
            relative_path: package_relative_path(project_dir, &path)?,
            sha256: sha256_hex(&bytes),
            text,
        });
    }
    Ok(sources)
}

fn collect_source_paths(directory: &Path, output: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(directory).map_err(|error| {
        format!(
            "cannot read deploy source root {}: {error}",
            directory.display()
        )
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            format!(
                "cannot read deploy source entry in {}: {error}",
                directory.display()
            )
        })?;
        let file_type = entry
            .file_type()
            .map_err(|error| format!("cannot inspect {}: {error}", entry.path().display()))?;
        if file_type.is_symlink() {
            return Err(format!(
                "error[deploy.source_identity]: source symlinks are not portable: {}",
                entry.path().display()
            ));
        }
        if file_type.is_dir() {
            collect_source_paths(&entry.path(), output)?;
        } else if file_type.is_file()
            && entry.path().extension().and_then(|value| value.to_str()) == Some("terl")
        {
            output.push(entry.path());
        }
    }
    Ok(())
}

fn collect_routes(
    project_dir: &Path,
    service: &str,
    sources: &[CollectedSource],
) -> Result<Vec<CollectedRoute>, String> {
    let mut routes = Vec::new();
    let mut identities = BTreeMap::new();
    for source in sources {
        let syntax = parse_module_as_syntax_output(&source.text).map_err(|error| {
            format!(
                "error[deploy.route_parse]: cannot parse {}: {error:?}",
                source.relative_path
            )
        })?;
        if !imports_std_http_router(&syntax) {
            continue;
        }
        for route in routes_from_syntax_module(&syntax)? {
            let identity = (route.method.clone(), route.path.clone());
            if let Some(existing) = identities.insert(identity.clone(), source.module.clone()) {
                return Err(format!(
                    "error[deploy.route_conflict]: duplicate {} {} routes in `{existing}` and `{}` for service `{service}`",
                    identity.0, identity.1, source.module
                ));
            }
            routes.push(CollectedRoute {
                method: route.method,
                path: route.path,
                handler: route.handler,
                module: source.module.clone(),
                source_path: source.relative_path.clone(),
            });
        }
    }
    routes.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then(left.method.cmp(&right.method))
            .then(left.handler.cmp(&right.handler))
    });
    let _ = project_dir;
    Ok(routes)
}

fn validate_health_route(
    deployment: Option<&ProjectDeployment>,
    routes: &[CollectedRoute],
) -> Result<(), String> {
    let Some(health) = deployment.and_then(|value| value.health.as_ref()) else {
        return Ok(());
    };
    if routes
        .iter()
        .any(|route| route.method == "GET" && route.path == health.path)
    {
        return Ok(());
    }
    Err(format!(
        "error[deploy.health_route]: health path `{}` has no compiler-discovered GET handler",
        health.path
    ))
}

fn collect_migrations(
    project_dir: &Path,
    deployment: Option<&ProjectDeployment>,
) -> Result<Vec<SemanticMigration>, String> {
    let mut migrations = Vec::new();
    for path in deployment
        .map(|value| value.migrations.as_slice())
        .unwrap_or_default()
    {
        let absolute = project_dir.join(path);
        let bytes = fs::read(&absolute).map_err(|error| {
            format!("error[deploy.migration]: cannot read migration `{path}`: {error}")
        })?;
        migrations.push(SemanticMigration {
            id: Path::new(path)
                .file_stem()
                .and_then(|value| value.to_str())
                .ok_or_else(|| {
                    format!("error[deploy.migration]: migration `{path}` has no UTF-8 file id")
                })?
                .to_string(),
            path: path.clone(),
            sha256: sha256_hex(&bytes),
        });
    }
    migrations.sort_by(|left, right| left.path.cmp(&right.path));
    let mut ids = BTreeSet::new();
    for migration in &migrations {
        if !ids.insert(&migration.id) {
            return Err(format!(
                "error[deploy.migration]: duplicate migration id `{}`",
                migration.id
            ));
        }
    }
    Ok(migrations)
}

fn rollback_contract(deployment: Option<&ProjectDeployment>) -> SemanticRollback {
    match deployment.map(|value| value.rollback) {
        None | Some(ProjectRollbackCompatibility::Stateless) => SemanticRollback {
            policy: "stateless",
            automatic: true,
        },
        Some(ProjectRollbackCompatibility::MigrationCompatible) => SemanticRollback {
            policy: "migration-compatible",
            automatic: true,
        },
        Some(ProjectRollbackCompatibility::Manual) => SemanticRollback {
            policy: "manual",
            automatic: false,
        },
    }
}

fn package_relative_path(project_dir: &Path, path: &Path) -> Result<String, String> {
    let relative = path.strip_prefix(project_dir).map_err(|error| {
        format!(
            "error[deploy.source_identity]: cannot relativize {} against {}: {error}",
            path.display(),
            project_dir.display()
        )
    })?;
    Ok(relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/"))
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
