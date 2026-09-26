//! Authenticated Terlan Cloud CLI client.
//!
//! Product policy remains in the Terlan Cloud application. This module owns
//! only local credential custody, deterministic release packaging, bounded
//! HTTP transport, and presentation of the typed Cloud API responses.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::Duration;

use rand::RngCore as _;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest as _, Sha256};
use url::{Host, Url};

use crate::commands::build::project_manifest::read_project_manifest;
use crate::{CliCommand, CliState};

const PROFILE_SCHEMA: &str = "terlan-cloud-cli-profile-v1";
const SESSION_SCHEMA: &str = "terlan-cloud-session-v1";
const DEPLOYMENT_SCHEMA: &str = "terlan-cloud-deployment-v1";
const ARTIFACT_RESULT_SCHEMA: &str = "terlan-cloud-artifact-result-v1";
const ROLLBACK_SCHEMA: &str = "terlan-cloud-rollback-request-v1";
const MAX_RESPONSE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_RELEASE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_UPLOAD_BYTES: u64 = 512 * 1024 * 1024;
const RELEASE_COMPRESSION_LEVEL: i32 = 9;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CloudProfile {
    schema: String,
    endpoint: String,
    user_id: String,
    api_token: String,
    artifact_token: Option<String>,
    routing_host: Option<String>,
    last_project: Option<String>,
    last_deployment_id: Option<String>,
}

#[derive(Debug)]
struct HttpResponse {
    status: u16,
    body: Vec<u8>,
}

#[derive(Default)]
struct LoginArgs {
    endpoint: Option<String>,
    user_id: Option<String>,
    token_file: Option<PathBuf>,
    artifact_token_file: Option<PathBuf>,
    routing_host: Option<String>,
    profile: Option<PathBuf>,
}

#[derive(Default)]
struct DeploymentArgs {
    project: Option<String>,
    deployment_id: Option<String>,
    profile: Option<PathBuf>,
}

#[derive(Default)]
struct DeployArgs {
    project_dir: Option<PathBuf>,
    project: Option<String>,
    release_id: Option<String>,
    repository: Option<String>,
    revision: Option<String>,
    deployment_id: Option<String>,
    idempotency_key: Option<String>,
    profile: Option<PathBuf>,
}

#[derive(Default)]
struct LogsArgs {
    deployment: DeploymentArgs,
    lines: u16,
}

pub(crate) fn run_login(args: &[String]) -> ExitCode {
    command_result(login(args))
}

pub(crate) fn run_status(args: &[String]) -> ExitCode {
    command_result(status(args))
}

pub(crate) fn run_logs(args: &[String]) -> ExitCode {
    command_result(logs(args))
}

pub(crate) fn run_rollback(args: &[String]) -> ExitCode {
    command_result(rollback(args))
}

pub(crate) fn run_deploy(args: &[String], state: CliState) -> ExitCode {
    match deploy(args, state) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
    }
}

fn command_result(result: Result<(), String>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
    }
}

fn login(args: &[String]) -> Result<(), String> {
    let args = parse_login_args(args)?;
    let endpoint = cloud_origin(args.endpoint.as_deref().ok_or_else(|| {
        "error[cloud_login_args]: terlc login requires --cloud <url>".to_string()
    })?)?;
    let user_id = args.user_id.ok_or_else(|| {
        "error[cloud_login_args]: terlc login requires --user-id <id>".to_string()
    })?;
    validate_user_id(&user_id)?;
    let api_token = read_token(
        &args.token_file.ok_or_else(|| {
            "error[cloud_login_args]: terlc login requires --token-file <path>".to_string()
        })?,
        "API",
    )?;
    let artifact_token = args
        .artifact_token_file
        .as_deref()
        .map(|path| read_token(path, "artifact"))
        .transpose()?;
    if let Some(host) = args.routing_host.as_deref() {
        validate_routing_host(host)?;
    }
    let profile = CloudProfile {
        schema: PROFILE_SCHEMA.to_string(),
        endpoint,
        user_id,
        api_token,
        artifact_token,
        routing_host: args.routing_host,
        last_project: None,
        last_deployment_id: None,
    };
    let response = request(&profile, "GET", "/api/v1/session", &[], None)?;
    require_status("login", &response, &[200])?;
    let session = parse_json(&response.body, "Cloud session")?;
    require_schema(&session, SESSION_SCHEMA, "Cloud session")?;
    if session.get("user_id").and_then(Value::as_str) != Some(profile.user_id.as_str()) {
        return Err("error[cloud_session]: Cloud returned a different user identity".into());
    }
    let path = args.profile.unwrap_or(profile_path()?);
    save_profile(&path, &profile)?;
    println!("logged in to {} as {}", profile.endpoint, profile.user_id);
    Ok(())
}

fn deploy(args: &[String], state: CliState) -> Result<(), String> {
    let args = parse_deploy_args(args)?;
    let profile_path = args.profile.clone().unwrap_or(profile_path()?);
    let mut profile = load_profile(&profile_path)?;
    let project_dir = args.project_dir.unwrap_or_else(|| PathBuf::from("."));
    let manifest = read_project_manifest(&project_dir.join("terlan.toml"))?;
    let project = args
        .project
        .unwrap_or_else(|| manifest.package.name.clone());
    validate_project_slug(&project)?;
    let deployment_id = args.deployment_id.unwrap_or_else(|| random_id("dep_"));
    validate_deployment_id(&deployment_id)?;
    let idempotency_key = args
        .idempotency_key
        .unwrap_or_else(|| format!("deploy-{}", random_hex()));
    validate_bounded_value("idempotency key", &idempotency_key, 16, 200)?;
    let release_id = match args.release_id {
        Some(release_id) => {
            validate_release_id(&release_id)?;
            release_id
        }
        None => build_and_upload_release(
            &profile,
            &project,
            &project_dir,
            args.repository.or(manifest.package.repository),
            args.revision,
            &state,
        )?,
    };
    let path = format!("/api/v1/projects/{project}/deployments");
    let request_id = format!("request-{}", random_hex());
    let headers = [
        ("x-terlan-deployment-id", deployment_id.as_str()),
        ("x-terlan-deploy-release-id", release_id.as_str()),
        ("x-terlan-idempotency-key", idempotency_key.as_str()),
        ("x-terlan-request-id", request_id.as_str()),
    ];
    let response = request(&profile, "POST", &path, &headers, Some(&mut &b""[..]))?;
    require_status("deploy", &response, &[200, 201])?;
    let deployment = parse_json(&response.body, "deployment")?;
    require_schema(&deployment, DEPLOYMENT_SCHEMA, "deployment")?;
    if deployment.get("deployment_id").and_then(Value::as_str) != Some(&deployment_id) {
        return Err("error[cloud_deploy_response]: Cloud returned a different deployment".into());
    }
    profile.last_project = Some(project);
    profile.last_deployment_id = Some(deployment_id);
    save_profile(&profile_path, &profile)?;
    print_json(&deployment)?;
    Ok(())
}

fn build_and_upload_release(
    profile: &CloudProfile,
    project: &str,
    project_dir: &Path,
    repository: Option<String>,
    revision: Option<String>,
    state: &CliState,
) -> Result<String, String> {
    let artifact_token = profile.artifact_token.as_deref().ok_or_else(|| {
        "error[cloud_artifact_credential]: source deployment requires an artifact token; log in with --artifact-token-file"
            .to_string()
    })?;
    let build = crate::commands::build::run(
        CliCommand {
            verb: Some("build".to_string()),
            args: vec![project_dir.display().to_string(), "--release".to_string()],
        },
        state.clone(),
    );
    if build != ExitCode::SUCCESS {
        return Err("error[cloud_release_build]: release build failed".into());
    }
    let release_root = state.out_dir.join(".terlan/release");
    let manifest = read_json(&release_root.join("manifest.json"), "release manifest")?;
    require_schema(
        &manifest,
        "terlan-cloud-release-bundle-v1",
        "release manifest",
    )?;
    let release = manifest
        .get("release")
        .ok_or_else(|| "error[cloud_release_manifest]: release identity is missing".to_string())?;
    let package = required_json_string(release, "package", "release manifest")?;
    let version = required_json_string(release, "version", "release manifest")?;
    if package != project {
        return Err(format!(
            "error[cloud_project_mismatch]: project `{project}` does not match release package `{package}`"
        ));
    }
    validate_release_version(&version)?;
    let release_name = format!("{package}@{version}");
    let release_id = random_id("rel_");
    let repository = repository.ok_or_else(|| {
        "error[cloud_source_repository]: [package] repository or --repository is required"
            .to_string()
    })?;
    validate_repository(&repository)?;
    let revision = revision
        .or_else(|| git_revision(project_dir))
        .ok_or_else(|| {
            "error[cloud_source_revision]: a Git revision or --revision is required".to_string()
        })?;
    validate_bounded_value("source revision", &revision, 7, 160)?;
    let archive = state.out_dir.join("cloud/deploy-release.tar.zst");
    create_release_archive(&release_root, &archive)?;
    let (sha256, bytes) = file_sha256(&archive)?;
    if bytes > MAX_UPLOAD_BYTES {
        return Err(format!(
            "error[cloud_release_archive]: compressed release is {bytes} bytes; Cloud accepts at most {MAX_UPLOAD_BYTES}"
        ));
    }
    let artifact_request_id = format!("artifact-{}", random_hex());
    let bytes_text = bytes.to_string();
    let headers = [
        ("content-type", "application/vnd.terlan.release+tar.zstd"),
        ("x-terlan-artifact-token", artifact_token),
        ("x-terlan-artifact-release-id", release_id.as_str()),
        ("x-terlan-package", package.as_str()),
        ("x-terlan-version", version.as_str()),
        ("x-terlan-artifact-sha256", sha256.as_str()),
        ("x-terlan-artifact-bytes", bytes_text.as_str()),
        ("x-terlan-source-repository", repository.as_str()),
        ("x-terlan-source-revision", revision.as_str()),
        ("x-terlan-request-id", artifact_request_id.as_str()),
    ];
    let path = format!("/api/v1/projects/{project}/releases/{release_name}/artifact");
    let mut file = File::open(&archive)
        .map_err(|error| format!("cannot read release archive {}: {error}", archive.display()))?;
    let response = request(profile, "PUT", &path, &headers, Some(&mut file))?;
    require_status("artifact upload", &response, &[200, 201])?;
    let admission = parse_json(&response.body, "artifact admission")?;
    require_validated_artifact(&admission, &release_id)?;
    Ok(release_id)
}

fn require_validated_artifact(value: &Value, release_id: &str) -> Result<(), String> {
    require_schema(value, ARTIFACT_RESULT_SCHEMA, "artifact admission")?;
    if value.get("release_id").and_then(Value::as_str) != Some(release_id)
        || value.get("state").and_then(Value::as_str) != Some("validated")
        || value
            .get("sha256")
            .and_then(Value::as_str)
            .is_none_or(|sha256| {
                sha256.len() != 64
                    || !sha256
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            })
    {
        return Err("error[cloud_artifact_response]: release was not validated".into());
    }
    Ok(())
}

fn status(args: &[String]) -> Result<(), String> {
    let args = parse_deployment_args(args)?;
    let profile = load_profile(&args.profile.clone().unwrap_or(profile_path()?))?;
    let (project, deployment) = resolve_deployment(&profile, &args)?;
    let response = request(
        &profile,
        "GET",
        &format!("/api/v1/projects/{project}/deployments/{deployment}"),
        &[],
        None,
    )?;
    require_status("status", &response, &[200])?;
    let value = parse_json(&response.body, "deployment status")?;
    require_schema(&value, DEPLOYMENT_SCHEMA, "deployment status")?;
    print_json(&value)
}

fn logs(args: &[String]) -> Result<(), String> {
    let args = parse_logs_args(args)?;
    let profile = load_profile(&args.deployment.profile.clone().unwrap_or(profile_path()?))?;
    let (project, deployment) = resolve_deployment(&profile, &args.deployment)?;
    let response = request(
        &profile,
        "GET",
        &format!(
            "/api/v1/projects/{project}/deployments/{deployment}/logs?lines={}",
            args.lines
        ),
        &[],
        None,
    )?;
    require_status("logs", &response, &[200])?;
    let text = std::str::from_utf8(&response.body)
        .map_err(|_| "error[cloud_logs]: response is not UTF-8".to_string())?;
    print!("{text}");
    if !text.ends_with('\n') {
        println!();
    }
    Ok(())
}

fn rollback(args: &[String]) -> Result<(), String> {
    let args = parse_deployment_args(args)?;
    let profile = load_profile(&args.profile.clone().unwrap_or(profile_path()?))?;
    let (project, deployment) = resolve_deployment(&profile, &args)?;
    let request_id = format!("rollback-{}", random_hex());
    let response = request(
        &profile,
        "POST",
        &format!("/api/v1/projects/{project}/deployments/{deployment}/rollback"),
        &[("x-terlan-request-id", request_id.as_str())],
        Some(&mut &b""[..]),
    )?;
    require_status("rollback", &response, &[200, 202])?;
    let value = parse_json(&response.body, "rollback")?;
    require_schema(&value, ROLLBACK_SCHEMA, "rollback")?;
    print_json(&value)
}

fn resolve_deployment(
    profile: &CloudProfile,
    args: &DeploymentArgs,
) -> Result<(String, String), String> {
    let project = args
        .project
        .clone()
        .or_else(|| profile.last_project.clone())
        .ok_or_else(|| "error[cloud_project]: pass --project or deploy first".to_string())?;
    let deployment = args
        .deployment_id
        .clone()
        .or_else(|| profile.last_deployment_id.clone())
        .ok_or_else(|| "error[cloud_deployment]: pass --deployment or deploy first".to_string())?;
    validate_project_slug(&project)?;
    validate_deployment_id(&deployment)?;
    Ok((project, deployment))
}

fn request(
    profile: &CloudProfile,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: Option<&mut dyn Read>,
) -> Result<HttpResponse, String> {
    if !path.starts_with('/') || path.contains(['\r', '\n']) {
        return Err("error[cloud_route]: invalid API route".into());
    }
    let agent = ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .max_redirects(0)
            .max_redirects_will_error(false)
            .http_status_as_error(false)
            .timeout_connect(Some(Duration::from_secs(10)))
            .timeout_recv_body(Some(Duration::from_secs(30)))
            .timeout_send_body(Some(Duration::from_secs(120)))
            .build(),
    );
    let url = format!("{}{path}", profile.endpoint);
    let result = match method {
        "GET" => {
            if body.is_some() {
                return Err("error[cloud_transport]: GET requests cannot contain a body".into());
            }
            let mut request = agent
                .get(&url)
                .header("accept", "application/json")
                .header("x-terlan-api-token", &profile.api_token)
                .header("x-terlan-user-id", &profile.user_id);
            if let Some(host) = profile.routing_host.as_deref() {
                request = request.header("host", host);
            }
            for (name, value) in headers {
                request = request.header(*name, *value);
            }
            request.call()
        }
        "POST" => {
            let mut request = agent
                .post(&url)
                .header("accept", "application/json")
                .header("x-terlan-api-token", &profile.api_token)
                .header("x-terlan-user-id", &profile.user_id);
            if let Some(host) = profile.routing_host.as_deref() {
                request = request.header("host", host);
            }
            for (name, value) in headers {
                request = request.header(*name, *value);
            }
            let reader = body.ok_or_else(|| {
                "error[cloud_transport]: POST requests require an explicit body".to_string()
            })?;
            request.send(ureq::SendBody::from_reader(reader))
        }
        "PUT" => {
            let mut request = agent
                .put(&url)
                .header("accept", "application/json")
                .header("x-terlan-api-token", &profile.api_token)
                .header("x-terlan-user-id", &profile.user_id);
            if let Some(host) = profile.routing_host.as_deref() {
                request = request.header("host", host);
            }
            for (name, value) in headers {
                request = request.header(*name, *value);
            }
            let reader = body.ok_or_else(|| {
                "error[cloud_transport]: PUT requests require an explicit body".to_string()
            })?;
            request.send(ureq::SendBody::from_reader(reader))
        }
        _ => return Err("error[cloud_transport]: unsupported HTTP method".into()),
    };
    let mut response = result
        .map_err(|error| format!("error[cloud_transport]: {method} {path} failed: {error}"))?;
    let status = response.status().as_u16();
    let mut bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(MAX_RESPONSE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("error[cloud_transport]: response read failed: {error}"))?;
    if bytes.len() as u64 > MAX_RESPONSE_BYTES {
        return Err("error[cloud_transport]: response exceeded 2 MiB".into());
    }
    Ok(HttpResponse {
        status,
        body: bytes,
    })
}

fn require_status(action: &str, response: &HttpResponse, accepted: &[u16]) -> Result<(), String> {
    if accepted.contains(&response.status) {
        return Ok(());
    }
    let detail = String::from_utf8_lossy(&response.body);
    let detail = detail.trim().chars().take(240).collect::<String>();
    Err(format!(
        "error[cloud_http]: {action} returned HTTP {}{}",
        response.status,
        if detail.is_empty() {
            String::new()
        } else {
            format!(": {detail}")
        }
    ))
}

fn cloud_origin(value: &str) -> Result<String, String> {
    let url =
        Url::parse(value).map_err(|_| "error[cloud_url]: Cloud URL is invalid".to_string())?;
    let local_http = url.scheme() == "http"
        && matches!(
            url.host(),
            Some(Host::Ipv4(address)) if address.is_loopback()
        );
    if value.trim() != value
        || (!local_http && url.scheme() != "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
    {
        return Err(
            "error[cloud_url]: Cloud requires public HTTPS or loopback HTTP for local development"
                .into(),
        );
    }
    Ok(value.trim_end_matches('/').to_string())
}

fn profile_path() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("TERLAN_CLOUD_PROFILE") {
        return Ok(PathBuf::from(path));
    }
    if let Some(root) = std::env::var_os("XDG_CONFIG_HOME") {
        return Ok(PathBuf::from(root).join("terlan/cloud.json"));
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|root| root.join(".config/terlan/cloud.json"))
        .ok_or_else(|| {
            "error[cloud_profile]: set TERLAN_CLOUD_PROFILE or XDG_CONFIG_HOME".to_string()
        })
}

fn load_profile(path: &Path) -> Result<CloudProfile, String> {
    let bytes = fs::read(path)
        .map_err(|error| format!("cannot read Cloud profile {}: {error}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        if fs::metadata(path)
            .map_err(|error| format!("cannot inspect Cloud profile {}: {error}", path.display()))?
            .permissions()
            .mode()
            & 0o077
            != 0
        {
            return Err(
                "error[cloud_profile_permissions]: Cloud profile must have mode 0600".into(),
            );
        }
    }
    let profile: CloudProfile = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Cloud profile {} is invalid: {error}", path.display()))?;
    validate_profile(&profile)?;
    Ok(profile)
}

fn save_profile(path: &Path, profile: &CloudProfile) -> Result<(), String> {
    validate_profile(profile)?;
    let parent = path.parent().ok_or_else(|| {
        format!(
            "Cloud profile path {} has no parent directory",
            path.display()
        )
    })?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("cannot create Cloud profile directory: {error}"))?;
    #[cfg(unix)]
    fs::set_permissions(parent, std::os::unix::fs::PermissionsExt::from_mode(0o700))
        .map_err(|error| format!("cannot secure Cloud profile directory: {error}"))?;
    let temporary = parent.join(format!(".cloud-{}.tmp", random_hex()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = options
        .open(&temporary)
        .map_err(|error| format!("cannot create Cloud profile: {error}"))?;
    let mut bytes = serde_json::to_vec_pretty(profile)
        .map_err(|error| format!("cannot serialize Cloud profile: {error}"))?;
    bytes.push(b'\n');
    let write_result = file.write_all(&bytes).and_then(|()| file.sync_all());
    if let Err(error) = write_result {
        let _ = fs::remove_file(&temporary);
        return Err(format!("cannot persist Cloud profile: {error}"));
    }
    fs::rename(&temporary, path).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        format!("cannot activate Cloud profile {}: {error}", path.display())
    })?;
    Ok(())
}

fn validate_profile(profile: &CloudProfile) -> Result<(), String> {
    if profile.schema != PROFILE_SCHEMA {
        return Err("error[cloud_profile_schema]: unsupported Cloud profile".into());
    }
    cloud_origin(&profile.endpoint)?;
    validate_user_id(&profile.user_id)?;
    validate_token(&profile.api_token, "API")?;
    if let Some(token) = profile.artifact_token.as_deref() {
        validate_token(token, "artifact")?;
    }
    if let Some(host) = profile.routing_host.as_deref() {
        validate_routing_host(host)?;
    }
    Ok(())
}

fn parse_login_args(args: &[String]) -> Result<LoginArgs, String> {
    let mut parsed = LoginArgs::default();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--cloud" => parsed.endpoint = Some(next_value(args, &mut index, "--cloud")?),
            "--user-id" => parsed.user_id = Some(next_value(args, &mut index, "--user-id")?),
            "--token-file" => {
                parsed.token_file =
                    Some(PathBuf::from(next_value(args, &mut index, "--token-file")?))
            }
            "--artifact-token-file" => {
                parsed.artifact_token_file = Some(PathBuf::from(next_value(
                    args,
                    &mut index,
                    "--artifact-token-file",
                )?))
            }
            "--routing-host" => {
                parsed.routing_host = Some(next_value(args, &mut index, "--routing-host")?)
            }
            "--profile" => {
                parsed.profile = Some(PathBuf::from(next_value(args, &mut index, "--profile")?))
            }
            "--help" | "-h" => return Err(login_usage().to_string()),
            value => {
                return Err(format!(
                    "error[cloud_login_args]: unexpected argument `{value}`"
                ))
            }
        }
    }
    Ok(parsed)
}

fn parse_deploy_args(args: &[String]) -> Result<DeployArgs, String> {
    let mut parsed = DeployArgs::default();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--project" => parsed.project = Some(next_value(args, &mut index, "--project")?),
            "--release-id" => {
                parsed.release_id = Some(next_value(args, &mut index, "--release-id")?)
            }
            "--repository" => {
                parsed.repository = Some(next_value(args, &mut index, "--repository")?)
            }
            "--revision" => parsed.revision = Some(next_value(args, &mut index, "--revision")?),
            "--deployment-id" => {
                parsed.deployment_id = Some(next_value(args, &mut index, "--deployment-id")?)
            }
            "--idempotency-key" => {
                parsed.idempotency_key = Some(next_value(args, &mut index, "--idempotency-key")?)
            }
            "--profile" => {
                parsed.profile = Some(PathBuf::from(next_value(args, &mut index, "--profile")?))
            }
            "--help" | "-h" => return Err(deploy_usage().to_string()),
            value if value.starts_with('-') => {
                return Err(format!(
                    "error[cloud_deploy_args]: unexpected argument `{value}`"
                ))
            }
            value => {
                if parsed.project_dir.is_some() {
                    return Err(
                        "error[cloud_deploy_args]: deploy accepts one project directory".into(),
                    );
                }
                parsed.project_dir = Some(PathBuf::from(value));
                index += 1;
            }
        }
    }
    Ok(parsed)
}

fn parse_deployment_args(args: &[String]) -> Result<DeploymentArgs, String> {
    let mut parsed = DeploymentArgs::default();
    let mut index = 0;
    while index < args.len() {
        parse_deployment_option(args, &mut index, &mut parsed)?;
    }
    Ok(parsed)
}

fn parse_logs_args(args: &[String]) -> Result<LogsArgs, String> {
    let mut parsed = LogsArgs {
        deployment: DeploymentArgs::default(),
        lines: 100,
    };
    let mut index = 0;
    while index < args.len() {
        if args[index] == "--lines" {
            let value = next_value(args, &mut index, "--lines")?;
            parsed.lines = value
                .parse::<u16>()
                .map_err(|_| "error[cloud_logs_args]: --lines requires 1..1000".to_string())?;
            if !(1..=1000).contains(&parsed.lines) {
                return Err("error[cloud_logs_args]: --lines requires 1..1000".into());
            }
        } else {
            parse_deployment_option(args, &mut index, &mut parsed.deployment)?;
        }
    }
    Ok(parsed)
}

fn parse_deployment_option(
    args: &[String],
    index: &mut usize,
    parsed: &mut DeploymentArgs,
) -> Result<(), String> {
    match args[*index].as_str() {
        "--project" => parsed.project = Some(next_value(args, index, "--project")?),
        "--deployment" => parsed.deployment_id = Some(next_value(args, index, "--deployment")?),
        "--profile" => parsed.profile = Some(PathBuf::from(next_value(args, index, "--profile")?)),
        "--help" | "-h" => return Err(deployment_usage().to_string()),
        value => {
            return Err(format!(
                "error[cloud_command_args]: unexpected argument `{value}`"
            ))
        }
    }
    Ok(())
}

fn next_value(args: &[String], index: &mut usize, option: &str) -> Result<String, String> {
    let value = args
        .get(*index + 1)
        .ok_or_else(|| format!("error[cloud_args]: {option} requires a value"))?
        .clone();
    *index += 2;
    Ok(value)
}

fn create_release_archive(release_root: &Path, archive: &Path) -> Result<(), String> {
    let checksums = read_json(&release_root.join("checksums.json"), "release checksums")?;
    require_schema(
        &checksums,
        "terlan-cloud-release-checksums-v1",
        "release checksums",
    )?;
    let files = checksums
        .get("files")
        .and_then(Value::as_array)
        .ok_or_else(|| "error[cloud_release_checksums]: files are missing".to_string())?;
    let mut paths = Vec::with_capacity(files.len() + 1);
    for file in files {
        let path = required_json_string(file, "path", "release checksums")?;
        paths.push(PathBuf::from(path));
    }
    paths.push(PathBuf::from("checksums.json"));
    if let Some(parent) = archive.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create Cloud output directory: {error}"))?;
    }
    if archive.exists() {
        fs::remove_file(archive)
            .map_err(|error| format!("cannot replace release archive: {error}"))?;
    }
    terlan_archive::create_tar_zstd_files_with_limits(
        release_root,
        &paths,
        archive,
        terlan_archive::TarZstdCreateLimits {
            max_files: 16_384,
            max_path_bytes: 240,
            max_unpacked_bytes: MAX_RELEASE_BYTES,
            compression_level: RELEASE_COMPRESSION_LEVEL,
        },
    )
    .map_err(|error| format!("error[cloud_release_archive]: {error}"))?;
    Ok(())
}

fn file_sha256(path: &Path) -> Result<(String, u64), String> {
    let mut file =
        File::open(path).map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let mut digest = Sha256::new();
    let mut bytes = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| format!("cannot hash {}: {error}", path.display()))?;
        if count == 0 {
            break;
        }
        bytes = bytes
            .checked_add(count as u64)
            .ok_or_else(|| "release archive byte count overflow".to_string())?;
        digest.update(&buffer[..count]);
    }
    let sha256 = digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    Ok((sha256, bytes))
}

fn git_revision(project_dir: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["-C", project_dir.to_str()?, "rev-parse", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let revision = String::from_utf8(output.stdout).ok()?;
    Some(revision.trim().to_string()).filter(|value| !value.is_empty())
}

fn read_token(path: &Path, purpose: &str) -> Result<String, String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("cannot inspect {purpose} token {}: {error}", path.display()))?;
    if !metadata.is_file() {
        return Err(format!(
            "error[cloud_token]: {purpose} token path is not a regular file"
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(format!(
                "error[cloud_token_permissions]: {purpose} token file must not be accessible by group or other users"
            ));
        }
    }
    let token = fs::read_to_string(path)
        .map_err(|error| format!("cannot read {purpose} token {}: {error}", path.display()))?;
    let token = token.trim().to_string();
    validate_token(&token, purpose)?;
    Ok(token)
}

fn validate_token(token: &str, purpose: &str) -> Result<(), String> {
    if token.len() < 24 || token.len() > 512 || token.chars().any(char::is_whitespace) {
        return Err(format!("error[cloud_token]: {purpose} token is malformed"));
    }
    Ok(())
}

fn validate_user_id(value: &str) -> Result<(), String> {
    validate_prefixed_identity(value, "usr_", "user")
}

fn validate_release_id(value: &str) -> Result<(), String> {
    validate_prefixed_identity(value, "rel_", "release")
}

fn validate_deployment_id(value: &str) -> Result<(), String> {
    validate_prefixed_identity(value, "dep_", "deployment")
}

fn validate_prefixed_identity(value: &str, prefix: &str, purpose: &str) -> Result<(), String> {
    let suffix_length = value.len().saturating_sub(prefix.len());
    if !(16..=64).contains(&suffix_length)
        || !value.starts_with(prefix)
        || !value[prefix.len()..]
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
    {
        return Err(format!("error[cloud_identity]: invalid {purpose} identity"));
    }
    Ok(())
}

fn validate_project_slug(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 63
        || value.starts_with('-')
        || value.ends_with('-')
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err("error[cloud_project]: invalid project slug".into());
    }
    Ok(())
}

fn validate_repository(value: &str) -> Result<(), String> {
    let url = Url::parse(value)
        .map_err(|_| "error[cloud_source_repository]: repository URL is invalid".to_string())?;
    if url.scheme() != "https"
        || url.host().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || value.len() > 512
    {
        return Err("error[cloud_source_repository]: repository must use HTTPS".into());
    }
    Ok(())
}

fn validate_release_version(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'+' | b'.'))
    {
        return Err("error[cloud_release_version]: release version is invalid".into());
    }
    Ok(())
}

fn validate_routing_host(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 253
        || value.contains(['/', ':', '\r', '\n', ' '])
        || !value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-')
        })
    {
        return Err("error[cloud_routing_host]: routing host is invalid".into());
    }
    Ok(())
}

fn validate_bounded_value(
    purpose: &str,
    value: &str,
    minimum: usize,
    maximum: usize,
) -> Result<(), String> {
    if value.len() < minimum
        || value.len() > maximum
        || value.chars().any(|character| character.is_control())
    {
        return Err(format!("error[cloud_value]: invalid {purpose}"));
    }
    Ok(())
}

fn random_id(prefix: &str) -> String {
    format!("{prefix}{}", random_hex())
}

fn random_hex() -> String {
    let mut bytes = [0_u8; 16];
    rand::rng().fill_bytes(&mut bytes);
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn read_json(path: &Path, purpose: &str) -> Result<Value, String> {
    let bytes = fs::read(path)
        .map_err(|error| format!("cannot read {purpose} {}: {error}", path.display()))?;
    parse_json(&bytes, purpose)
}

fn parse_json(bytes: &[u8], purpose: &str) -> Result<Value, String> {
    serde_json::from_slice(bytes)
        .map_err(|error| format!("error[cloud_json]: {purpose} is invalid: {error}"))
}

fn require_schema(value: &Value, schema: &str, purpose: &str) -> Result<(), String> {
    if value.get("schema").and_then(Value::as_str) != Some(schema) {
        return Err(format!("error[cloud_schema]: {purpose} schema is invalid"));
    }
    Ok(())
}

fn required_json_string(value: &Value, field: &str, purpose: &str) -> Result<String, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("error[cloud_json]: {purpose} field `{field}` is missing"))
}

fn print_json(value: &Value) -> Result<(), String> {
    println!(
        "{}",
        serde_json::to_string_pretty(value)
            .map_err(|error| format!("cannot render Cloud response: {error}"))?
    );
    Ok(())
}

fn login_usage() -> &'static str {
    "terlc login --cloud <url> --user-id <id> --token-file <path> [--artifact-token-file <path>] [--routing-host <host>] [--profile <path>]"
}

pub(crate) fn deploy_usage() -> &'static str {
    "terlc deploy [project-dir] [--project <slug>] [--release-id <id>] [--repository <url>] [--revision <rev>] [--profile <path>] [--out-dir <dir>]"
}

fn deployment_usage() -> &'static str {
    "terlc status|logs|rollback [--project <slug>] [--deployment <id>] [--profile <path>]"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloud_origin_requires_https_or_loopback_http() {
        assert_eq!(
            cloud_origin("https://cloud.terlan.dev").unwrap(),
            "https://cloud.terlan.dev"
        );
        assert_eq!(
            cloud_origin("http://127.0.0.1:4100").unwrap(),
            "http://127.0.0.1:4100"
        );
        assert!(cloud_origin("http://cloud.example").is_err());
        assert!(cloud_origin("https://user@cloud.example").is_err());
    }

    #[test]
    fn identities_and_log_bounds_fail_closed() {
        assert!(validate_user_id("usr_00000000000000000000000000000001").is_ok());
        assert!(validate_user_id("usr_0000000000000001").is_ok());
        assert!(validate_deployment_id("dep_00000000000000000000000000000001").is_ok());
        assert!(validate_deployment_id("dep_../escape").is_err());
        assert!(parse_logs_args(&["--lines".into(), "1001".into()]).is_err());
    }

    #[test]
    fn deploy_defaults_to_source_build_and_accepts_admitted_release() {
        let source = parse_deploy_args(&[]).unwrap();
        assert!(source.release_id.is_none());
        let admitted = parse_deploy_args(&[
            "--project".into(),
            "terlan-registry".into(),
            "--release-id".into(),
            "rel_00000000000000000000000000000002".into(),
        ])
        .unwrap();
        assert_eq!(admitted.project.as_deref(), Some("terlan-registry"));
        assert_eq!(
            admitted.release_id.as_deref(),
            Some("rel_00000000000000000000000000000002")
        );
    }

    #[test]
    fn artifact_result_requires_exact_validated_release_shape() {
        let release_id = "rel_0000000000000001";
        let valid = serde_json::json!({
            "schema": "terlan-cloud-artifact-result-v1",
            "state": "validated",
            "release_id": release_id,
            "sha256": "a".repeat(64),
        });
        assert!(require_validated_artifact(&valid, release_id).is_ok());
        let mut drift = valid.clone();
        drift["state"] = Value::String("uploaded".into());
        assert!(require_validated_artifact(&drift, release_id).is_err());
        drift = valid;
        drift["release_id"] = Value::String("rel_0000000000000002".into());
        assert!(require_validated_artifact(&drift, release_id).is_err());
        let mut bad_digest = serde_json::json!({
            "schema": "terlan-cloud-artifact-result-v1",
            "state": "validated",
            "release_id": release_id,
            "sha256": "z".repeat(64),
        });
        assert!(require_validated_artifact(&bad_digest, release_id).is_err());
        bad_digest["sha256"] = Value::String("a".repeat(63));
        assert!(require_validated_artifact(&bad_digest, release_id).is_err());
    }

    #[test]
    fn release_source_metadata_is_url_and_path_safe() {
        assert!(validate_release_version("0.0.2-rc.1+build").is_ok());
        assert!(validate_release_version("0.0.2?redirect=https://bad.example").is_err());
        assert!(validate_repository("https://github.com/terlan/terlan-registry").is_ok());
        assert!(validate_repository("https://token@github.com/terlan/private").is_err());
        assert!(validate_repository("https://github.com/terlan/repo?token=secret").is_err());
    }
}
