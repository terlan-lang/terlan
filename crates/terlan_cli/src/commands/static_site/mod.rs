use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, UNIX_EPOCH};

use terlan_syntax::{SyntaxDeclarationPayload, SyntaxImportKind, SyntaxModuleOutput};

use crate::commands::artifacts::{
    collect_syntax_markdown_inputs, collect_syntax_template_inputs, resolve_import_path,
};
use crate::validation::static_output::{
    validate_static_css_output_files, validate_static_html_output,
};
use crate::validation::target_profile::TargetProfileCheckOptions;
use crate::{CliCommand, CliState};

mod html_usage;
mod render;
mod routes;
pub(crate) use html_usage::*;
pub(crate) use render::{render_syntax_static_entrypoint, StaticSyntaxRenderError};
pub(crate) use routes::*;

/// Reserved template prop name used for component children.
pub(crate) const TEMPLATE_CHILDREN_SLOT: &str = "children";

/// Asset include/exclude filters for static output copying.
///
/// Inputs:
/// - `includes`: optional wildcard patterns that allow matching assets.
/// - `excludes`: wildcard patterns that reject matching assets.
///
/// Output:
/// - Filter state consumed by static asset copying.
///
/// Transformation:
/// - A path is allowed when it matches at least one include, or no includes are
///   configured, and does not match any exclude.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct AssetFilters {
    pub(crate) includes: Vec<String>,
    pub(crate) excludes: Vec<String>,
}

impl AssetFilters {
    /// Returns whether a static asset path should be copied.
    ///
    /// Inputs:
    /// - `path`: resolved asset source path.
    ///
    /// Output:
    /// - `true` when include/exclude rules allow the path.
    ///
    /// Transformation:
    /// - Matches patterns against both normalized full paths and file names.
    pub(crate) fn allows(&self, path: &Path) -> bool {
        let included = self.includes.is_empty()
            || self
                .includes
                .iter()
                .any(|pattern| asset_pattern_matches(pattern, path));
        let excluded = self
            .excludes
            .iter()
            .any(|pattern| asset_pattern_matches(pattern, path));

        included && !excluded
    }
}

/// Parsed command-local arguments for `emit-static`.
///
/// Inputs:
/// - Produced by `parse_emit_static_args`.
///
/// Output:
/// - Source file path, validation mode, and asset filters.
///
/// Transformation:
/// - Carries normalized static emit settings into command execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EmitStaticArgs {
    pub(crate) file: String,
    pub(crate) validate_output: bool,
    pub(crate) asset_filters: AssetFilters,
}

/// Parsed command-local arguments for `serve-static`.
///
/// Inputs:
/// - Produced by `parse_serve_static_args`.
///
/// Output:
/// - Static source, bind address, polling interval, source directory override,
///   and embedded emit-static settings.
///
/// Transformation:
/// - Combines dev-server flags with reusable static emit settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ServeStaticArgs {
    pub(crate) file: String,
    pub(crate) host: String,
    pub(crate) port: u16,
    pub(crate) poll_ms: u64,
    pub(crate) source_dir: Option<PathBuf>,
    pub(crate) emit_args: EmitStaticArgs,
}

type ReloadClients = Arc<Mutex<Vec<Sender<u64>>>>;

/// Parses command-local arguments for `emit-static`.
///
/// Inputs:
/// - `args`: arguments after the `emit-static` verb.
///
/// Output:
/// - Parsed emit-static settings or an error message.
///
/// Transformation:
/// - Scans one file argument plus `--validate-output`, `--asset-include`, and
///   `--asset-exclude` flags.
pub(crate) fn parse_emit_static_args(args: &[String]) -> Result<EmitStaticArgs, String> {
    let mut file = None;
    let mut validate_output = false;
    let mut includes = Vec::new();
    let mut excludes = Vec::new();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--validate-output" => {
                validate_output = true;
                index += 1;
            }
            "--asset-include" => {
                let Some(pattern) = args.get(index + 1) else {
                    return Err("--asset-include requires a pattern".to_string());
                };
                includes.push(pattern.clone());
                index += 2;
            }
            "--asset-exclude" => {
                let Some(pattern) = args.get(index + 1) else {
                    return Err("--asset-exclude requires a pattern".to_string());
                };
                excludes.push(pattern.clone());
                index += 2;
            }
            option if option.starts_with("--") => {
                return Err(format!("unsupported emit-static option: {}", option));
            }
            path => {
                if file.replace(path.to_string()).is_some() {
                    return Err("emit-static expects exactly one file argument".to_string());
                }
                index += 1;
            }
        }
    }

    let Some(file) = file else {
        return Err("emit-static expects exactly one file argument".to_string());
    };

    Ok(EmitStaticArgs {
        file,
        validate_output,
        asset_filters: AssetFilters { includes, excludes },
    })
}

/// Returns whether an asset pattern matches a path.
///
/// Inputs:
/// - `pattern`: wildcard pattern.
/// - `path`: resolved path to test.
///
/// Output:
/// - `true` when the pattern matches the normalized path or filename.
///
/// Transformation:
/// - Normalizes separators to `/` before wildcard matching.
fn asset_pattern_matches(pattern: &str, path: &Path) -> bool {
    let normalized_path = path.to_string_lossy().replace('\\', "/");
    if wildcard_match(pattern, &normalized_path) {
        return true;
    }

    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| wildcard_match(pattern, name))
}

/// Matches a simple `*` wildcard pattern.
///
/// Inputs:
/// - `pattern`: wildcard pattern with zero or more `*` wildcards.
/// - `value`: candidate text.
///
/// Output:
/// - `true` when `value` satisfies `pattern`.
///
/// Transformation:
/// - Performs ordered substring matching with anchored edges when the pattern
///   does not start or end with `*`.
fn wildcard_match(pattern: &str, value: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if !pattern.contains('*') {
        return pattern == value;
    }

    let starts_with_wildcard = pattern.starts_with('*');
    let ends_with_wildcard = pattern.ends_with('*');
    let mut rest = value;
    let mut parts = pattern
        .split('*')
        .filter(|part| !part.is_empty())
        .peekable();

    if !starts_with_wildcard {
        let Some(first) = parts.next() else {
            return true;
        };
        let Some(stripped) = rest.strip_prefix(first) else {
            return false;
        };
        rest = stripped;
    }

    while let Some(part) = parts.next() {
        if parts.peek().is_none() && !ends_with_wildcard {
            return rest.ends_with(part);
        }
        let Some(position) = rest.find(part) else {
            return false;
        };
        rest = &rest[position + part.len()..];
    }

    true
}

/// Executes the `emit-static` CLI command.
///
/// Inputs:
/// - `cmd`: parsed command containing one source path and static flags.
/// - `state`: global CLI state, including output directory, cache directory,
///   diagnostic format, and native policy.
///
/// Output:
/// - `ExitCode::SUCCESS` when static output files are written.
/// - `ExitCode::from(2)` for malformed arguments.
/// - `ExitCode::from(1)` for read, compile, route, render, validation, asset,
///   directory, or write failures.
///
/// Transformation:
/// - Compiles one source module, discovers static entrypoints/routes, renders
///   HTML, copies static assets, and optionally validates generated output.
pub(crate) fn run_emit_static(cmd: CliCommand, state: CliState) -> ExitCode {
    let args = match parse_emit_static_args(&cmd.args) {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{}", message);
            return ExitCode::from(2);
        }
    };

    let path = &args.file;
    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(err) => {
            eprintln!("failed to read {}: {}", path, err);
            return ExitCode::from(1);
        }
    };

    let compiled =
        match crate::formal_pipeline::compile_syntax_module_through_phases_with_profile_options(
            path,
            &source,
            state.diagnostic_format,
            state.cache_dir.as_deref(),
            state.native_policy,
            state.target_profile,
            TargetProfileCheckOptions {
                allow_asset_imports: true,
            },
        ) {
            Ok(compiled) => compiled,
            Err(exit_code) => return exit_code,
        };
    let syntax_output = compiled.syntax_output;
    let entrypoints = discover_syntax_static_entrypoints(&syntax_output);
    let routes = match discover_syntax_static_routes(&syntax_output) {
        Ok(routes) => routes,
        Err(message) => {
            eprintln!("{}", message);
            return ExitCode::from(1);
        }
    };
    if let Err(message) = validate_syntax_static_route_handlers(&syntax_output, &routes) {
        eprintln!("{}", message);
        return ExitCode::from(1);
    }
    let templates = match collect_syntax_template_inputs(&syntax_output, Path::new(path)) {
        Ok(templates) => templates,
        Err(message) => {
            eprintln!("{}", message);
            return ExitCode::from(1);
        }
    };
    let markdown_imports = match collect_syntax_markdown_inputs(&syntax_output, Path::new(path)) {
        Ok(markdown_imports) => markdown_imports,
        Err(message) => {
            eprintln!("{}", message);
            return ExitCode::from(1);
        }
    };

    if let Err(err) = fs::create_dir_all(&state.out_dir) {
        eprintln!(
            "failed to create static output directory `{}`: {}",
            state.out_dir.display(),
            err
        );
        return ExitCode::from(1);
    }

    let copied_css_outputs = match copy_syntax_static_asset_imports(
        &syntax_output,
        Path::new(path),
        &state.out_dir,
        &args.asset_filters,
    ) {
        Ok(paths) => paths,
        Err(message) => {
            eprintln!("{}", message);
            return ExitCode::from(1);
        }
    };
    for entrypoint in entrypoints {
        let html = match render_syntax_static_entrypoint(
            &syntax_output,
            &templates,
            &markdown_imports,
            &entrypoint,
        ) {
            Ok(html) => html,
            Err(StaticSyntaxRenderError::Invalid(message)) => {
                eprintln!("{}", message);
                return ExitCode::from(1);
            }
        };
        let target = state.out_dir.join(format!("{}.html", entrypoint));
        if args.validate_output {
            if let Err(message) = validate_static_html_output(&html, &target) {
                eprintln!("{}", message);
                return ExitCode::from(1);
            }
        }
        if let Err(err) = fs::write(&target, html.as_bytes()) {
            eprintln!(
                "failed to write static output `{}`: {}",
                target.display(),
                err
            );
            return ExitCode::from(1);
        }
    }

    for route in routes {
        let html = match render_syntax_static_entrypoint(
            &syntax_output,
            &templates,
            &markdown_imports,
            &route.handler,
        ) {
            Ok(html) => html,
            Err(StaticSyntaxRenderError::Invalid(message)) => {
                eprintln!("{}", message);
                return ExitCode::from(1);
            }
        };
        let relative_target = match static_route_output_path(&route.path) {
            Ok(path) => path,
            Err(message) => {
                eprintln!("{}", message);
                return ExitCode::from(1);
            }
        };
        let target = state.out_dir.join(relative_target);
        if let Some(parent) = target.parent() {
            if let Err(err) = fs::create_dir_all(parent) {
                eprintln!(
                    "failed to create static route output directory `{}`: {}",
                    parent.display(),
                    err
                );
                return ExitCode::from(1);
            }
        }
        if args.validate_output {
            if let Err(message) = validate_static_html_output(&html, &target) {
                eprintln!("{}", message);
                return ExitCode::from(1);
            }
        }
        if let Err(err) = fs::write(&target, html.as_bytes()) {
            eprintln!(
                "failed to write static route output `{}`: {}",
                target.display(),
                err
            );
            return ExitCode::from(1);
        }
    }

    if args.validate_output {
        if let Err(message) = validate_static_css_output_files(&copied_css_outputs) {
            eprintln!("{}", message);
            return ExitCode::from(1);
        }
    }

    ExitCode::SUCCESS
}

/// Parses command-local arguments for `serve-static`.
///
/// Inputs:
/// - `args`: arguments after the `serve-static` verb.
///
/// Output:
/// - Parsed dev-server settings or an error message.
///
/// Transformation:
/// - Scans bind, polling, source-dir, validation, asset-filter, and single file
///   arguments.
pub(crate) fn parse_serve_static_args(args: &[String]) -> Result<ServeStaticArgs, String> {
    let mut file = None;
    let mut host = "127.0.0.1".to_string();
    let mut port = 8080;
    let mut poll_ms = 500;
    let mut source_dir = None;
    let mut validate_output = false;
    let mut includes = Vec::new();
    let mut excludes = Vec::new();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--host" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--host requires a value".to_string());
                };
                host = value.clone();
                index += 2;
            }
            "--port" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--port requires a value".to_string());
                };
                port = value
                    .parse()
                    .map_err(|_| format!("invalid --port value: {}", value))?;
                index += 2;
            }
            "--poll-ms" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--poll-ms requires a value".to_string());
                };
                poll_ms = value
                    .parse()
                    .map_err(|_| format!("invalid --poll-ms value: {}", value))?;
                if poll_ms == 0 {
                    return Err("--poll-ms must be greater than 0".to_string());
                }
                index += 2;
            }
            "--source-dir" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--source-dir requires a value".to_string());
                };
                source_dir = Some(PathBuf::from(value));
                index += 2;
            }
            "--validate-output" => {
                validate_output = true;
                index += 1;
            }
            "--asset-include" => {
                let Some(pattern) = args.get(index + 1) else {
                    return Err("--asset-include requires a pattern".to_string());
                };
                includes.push(pattern.clone());
                index += 2;
            }
            "--asset-exclude" => {
                let Some(pattern) = args.get(index + 1) else {
                    return Err("--asset-exclude requires a pattern".to_string());
                };
                excludes.push(pattern.clone());
                index += 2;
            }
            option if option.starts_with("--") => {
                return Err(format!("unsupported serve-static option: {}", option));
            }
            path => {
                if file.replace(path.to_string()).is_some() {
                    return Err("serve-static expects exactly one file argument".to_string());
                }
                index += 1;
            }
        }
    }

    let Some(file) = file else {
        return Err("serve-static expects exactly one file argument".to_string());
    };

    let asset_filters = AssetFilters { includes, excludes };
    let emit_args = EmitStaticArgs {
        file: file.clone(),
        validate_output,
        asset_filters,
    };

    Ok(ServeStaticArgs {
        file,
        host,
        port,
        poll_ms,
        source_dir,
        emit_args,
    })
}

/// Executes the `serve-static` CLI command.
///
/// Inputs:
/// - `cmd`: parsed command containing source path and dev-server flags.
/// - `state`: global CLI state used for static output generation.
///
/// Output:
/// - `ExitCode::from(2)` for malformed arguments.
/// - `ExitCode::from(1)` for initial compile or bind failures.
/// - Otherwise this command runs until the process exits.
///
/// Transformation:
/// - Performs an initial static emit, starts an HTTP server, polls source/output
///   directories, recompiles on source changes, and broadcasts reload events.
pub(crate) fn run_serve_static(cmd: CliCommand, state: CliState) -> ExitCode {
    let args = match parse_serve_static_args(&cmd.args) {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{}", message);
            return ExitCode::from(2);
        }
    };

    let source_dir = args.source_dir.clone().unwrap_or_else(|| {
        Path::new(&args.file)
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf()
    });
    let out_dir = state.out_dir.clone();
    let exclude_dir = canonicalize_optional(&out_dir);

    eprintln!(
        "terlc serve-static: compiling {} -> {}",
        args.file,
        out_dir.display()
    );
    if run_emit_static_with_args(&args.emit_args, state.clone()) != ExitCode::SUCCESS {
        return ExitCode::from(1);
    }

    let clients: ReloadClients = Arc::new(Mutex::new(Vec::new()));
    let listener = match TcpListener::bind(format!("{}:{}", args.host, args.port)) {
        Ok(listener) => listener,
        Err(err) => {
            eprintln!("failed to bind dev server: {}", err);
            return ExitCode::from(1);
        }
    };
    let local_addr = match listener.local_addr() {
        Ok(addr) => addr.to_string(),
        Err(_) => format!("{}:{}", args.host, args.port),
    };
    eprintln!("terlc serve-static: serving {}", out_dir.display());
    eprintln!("terlc serve-static: http://{}", local_addr);
    eprintln!("terlc serve-static: reload stream /__terlan/reload");

    let server_clients = Arc::clone(&clients);
    let server_out_dir = out_dir.clone();
    thread::spawn(move || run_static_http_server(listener, server_out_dir, server_clients));

    let poll_interval = Duration::from_millis(args.poll_ms);
    let mut source_hash = directory_fingerprint(&source_dir, exclude_dir.as_deref());
    let mut dist_hash = directory_fingerprint(&out_dir, None);
    let mut reload_version = 0;

    loop {
        thread::sleep(poll_interval);

        let next_source_hash = directory_fingerprint(&source_dir, exclude_dir.as_deref());
        if next_source_hash != source_hash {
            eprintln!("terlc serve-static: source changed; recompiling");
            source_hash = next_source_hash;
            if run_emit_static_with_args(&args.emit_args, state.clone()) != ExitCode::SUCCESS {
                eprintln!("terlc serve-static: compile failed; keeping previous output");
                continue;
            }
        }

        let next_dist_hash = directory_fingerprint(&out_dir, None);
        if next_dist_hash != dist_hash {
            dist_hash = next_dist_hash;
            reload_version += 1;
            broadcast_reload(&clients, reload_version);
        }
    }
}

/// Replays parsed emit-static arguments through the command runner.
///
/// Inputs:
/// - `args`: parsed static emit arguments.
/// - `state`: global CLI state.
///
/// Output:
/// - Same exit-code contract as `run_emit_static`.
///
/// Transformation:
/// - Reconstructs command-local strings so serve-static can reuse the exact
///   emit-static command path.
fn run_emit_static_with_args(args: &EmitStaticArgs, state: CliState) -> ExitCode {
    let mut cmd_args = vec![args.file.clone()];
    if args.validate_output {
        cmd_args.push("--validate-output".to_string());
    }
    for include in &args.asset_filters.includes {
        cmd_args.push("--asset-include".to_string());
        cmd_args.push(include.clone());
    }
    for exclude in &args.asset_filters.excludes {
        cmd_args.push("--asset-exclude".to_string());
        cmd_args.push(exclude.clone());
    }

    run_emit_static(
        CliCommand {
            verb: Some("emit-static".to_string()),
            args: cmd_args,
        },
        state,
    )
}

/// Runs the static development HTTP server accept loop.
///
/// Inputs:
/// - `listener`: bound TCP listener.
/// - `out_dir`: static output directory to serve.
/// - `clients`: reload SSE subscribers.
///
/// Output:
/// - No return value; runs until listener errors or process exits.
///
/// Transformation:
/// - Accepts connections and spawns one handler thread per connection.
fn run_static_http_server(listener: TcpListener, out_dir: PathBuf, clients: ReloadClients) {
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let out_dir = out_dir.clone();
                let clients = Arc::clone(&clients);
                thread::spawn(move || handle_static_http_connection(stream, &out_dir, clients));
            }
            Err(err) => eprintln!("terlc serve-static: connection failed: {}", err),
        }
    }
}

/// Handles one static HTTP connection.
///
/// Inputs:
/// - `stream`: accepted TCP stream.
/// - `out_dir`: static output directory.
/// - `clients`: reload SSE subscribers.
///
/// Output:
/// - No return value; writes one HTTP/SSE response.
///
/// Transformation:
/// - Parses a minimal HTTP request, routes reload SSE, serves files, injects
///   reload script into HTML, and writes a response.
fn handle_static_http_connection(mut stream: TcpStream, out_dir: &Path, clients: ReloadClients) {
    let mut buffer = [0; 8192];
    let read = match stream.read(&mut buffer) {
        Ok(read) => read,
        Err(_) => return,
    };
    if read == 0 {
        return;
    }

    let request = String::from_utf8_lossy(&buffer[..read]);
    let Some(first_line) = request.lines().next() else {
        return;
    };
    let mut parts = first_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("/");

    if method != "GET" && method != "HEAD" {
        let _ = write_http_response(
            &mut stream,
            405,
            "Method Not Allowed",
            "text/plain; charset=utf-8",
            b"method not allowed",
            method == "HEAD",
        );
        return;
    }

    let path = target.split('?').next().unwrap_or("/");
    if path == "/__terlan/reload" {
        handle_reload_sse(stream, clients);
        return;
    }

    let Some(file_path) = static_request_path(out_dir, path) else {
        let _ = write_http_response(
            &mut stream,
            400,
            "Bad Request",
            "text/plain; charset=utf-8",
            b"bad request",
            method == "HEAD",
        );
        return;
    };

    let response_path = if file_path.is_dir() {
        file_path.join("index.html")
    } else if file_path.exists() {
        file_path
    } else if file_path.extension().is_none() {
        file_path.join("index.html")
    } else {
        file_path
    };

    let bytes = match fs::read(&response_path) {
        Ok(bytes) => bytes,
        Err(_) => {
            let _ = write_http_response(
                &mut stream,
                404,
                "Not Found",
                "text/plain; charset=utf-8",
                b"not found",
                method == "HEAD",
            );
            return;
        }
    };
    let content_type = content_type_for_path(&response_path);
    let body = if content_type.starts_with("text/html") {
        String::from_utf8(bytes)
            .map(|html| inject_reload_script(&html).into_bytes())
            .unwrap_or_else(|err| err.into_bytes())
    } else {
        bytes
    };
    let _ = write_http_response(
        &mut stream,
        200,
        "OK",
        content_type,
        &body,
        method == "HEAD",
    );
}

/// Handles one reload server-sent-events connection.
///
/// Inputs:
/// - `stream`: accepted TCP stream.
/// - `clients`: shared reload subscriber list.
///
/// Output:
/// - No return value; writes events until the client disconnects.
///
/// Transformation:
/// - Registers a sender, writes SSE headers, and streams reload versions.
fn handle_reload_sse(mut stream: TcpStream, clients: ReloadClients) {
    let (tx, rx) = mpsc::channel();
    if let Ok(mut locked) = clients.lock() {
        locked.push(tx);
    }

    let headers = concat!(
        "HTTP/1.1 200 OK\r\n",
        "Content-Type: text/event-stream\r\n",
        "Cache-Control: no-cache\r\n",
        "Connection: keep-alive\r\n",
        "Access-Control-Allow-Origin: *\r\n",
        "\r\n",
        ": connected\n\n"
    );
    if stream.write_all(headers.as_bytes()).is_err() {
        return;
    }
    let _ = stream.flush();

    while let Ok(version) = rx.recv() {
        if write!(stream, "event: reload\ndata: {}\n\n", version).is_err() {
            break;
        }
        if stream.flush().is_err() {
            break;
        }
    }
}

/// Broadcasts a reload event to active clients.
///
/// Inputs:
/// - `clients`: reload subscriber list.
/// - `version`: monotonically increasing reload version.
///
/// Output:
/// - No return value.
///
/// Transformation:
/// - Sends to each client and drops disconnected senders.
fn broadcast_reload(clients: &ReloadClients, version: u64) {
    eprintln!("terlc serve-static: output changed; broadcasting reload");
    if let Ok(mut locked) = clients.lock() {
        locked.retain(|client| client.send(version).is_ok());
    }
}

/// Writes a minimal HTTP response.
///
/// Inputs:
/// - `stream`: TCP stream to write.
/// - `status`: numeric HTTP status.
/// - `reason`: status reason phrase.
/// - `content_type`: response content type.
/// - `body`: response body bytes.
/// - `head_only`: whether to omit the body for HEAD requests.
///
/// Output:
/// - I/O result for response writing.
///
/// Transformation:
/// - Emits headers and optionally writes body bytes.
fn write_http_response(
    stream: &mut TcpStream,
    status: u16,
    reason: &str,
    content_type: &str,
    body: &[u8],
    head_only: bool,
) -> std::io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        status,
        reason,
        content_type,
        body.len()
    )?;
    if !head_only {
        stream.write_all(body)?;
    }
    stream.flush()
}

/// Converts a request path into a static output path.
///
/// Inputs:
/// - `root`: static output directory.
/// - `request_path`: URL path component.
///
/// Output:
/// - Safe filesystem path under `root`, or `None` for invalid traversal-like
///   paths.
///
/// Transformation:
/// - Splits URL segments, rejects dangerous segments, and maps `/` to
///   `index.html`.
pub(crate) fn static_request_path(root: &Path, request_path: &str) -> Option<PathBuf> {
    let mut path = root.to_path_buf();
    let trimmed = request_path.trim_start_matches('/');
    if trimmed.is_empty() {
        return Some(path.join("index.html"));
    }

    for segment in trimmed.split('/') {
        if segment.is_empty() {
            continue;
        }
        if segment == "." || segment == ".." || segment.contains('\\') || segment.contains('\0') {
            return None;
        }
        path.push(segment);
    }

    Some(path)
}

/// Returns a content type for a static file path.
///
/// Inputs:
/// - `path`: response file path.
///
/// Output:
/// - Static content-type string.
///
/// Transformation:
/// - Maps common extensions to MIME types and falls back to octet stream.
fn content_type_for_path(path: &Path) -> &'static str {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("ico") => "image/x-icon",
        Some("txt") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// Injects the live-reload script into HTML.
///
/// Inputs:
/// - `html`: rendered HTML document.
///
/// Output:
/// - HTML with reload script appended or inserted before `</body>`.
///
/// Transformation:
/// - Preserves documents that already reference the reload endpoint.
pub(crate) fn inject_reload_script(html: &str) -> String {
    const SCRIPT: &str = r#"<script>
(() => {
  const events = new EventSource('/__terlan/reload');
  events.addEventListener('reload', () => location.reload());
})();
</script>"#;

    if html.contains("/__terlan/reload") {
        return html.to_string();
    }
    if let Some(index) = html.rfind("</body>") {
        let mut out = String::with_capacity(html.len() + SCRIPT.len());
        out.push_str(&html[..index]);
        out.push_str(SCRIPT);
        out.push_str(&html[index..]);
        return out;
    }

    let mut out = String::with_capacity(html.len() + SCRIPT.len());
    out.push_str(html);
    out.push_str(SCRIPT);
    out
}

/// Canonicalizes a path when possible.
///
/// Inputs:
/// - `path`: path to canonicalize.
///
/// Output:
/// - Canonical path or `None` when canonicalization fails.
///
/// Transformation:
/// - Converts filesystem errors into an optional result for exclusion checks.
fn canonicalize_optional(path: &Path) -> Option<PathBuf> {
    fs::canonicalize(path).ok()
}

/// Computes a coarse fingerprint for a directory tree.
///
/// Inputs:
/// - `root`: directory to fingerprint.
/// - `exclude`: optional canonical directory subtree to ignore.
///
/// Output:
/// - Hash of paths, file lengths, and modification times.
///
/// Transformation:
/// - Recursively collects files, sorts paths, and hashes metadata.
pub(crate) fn directory_fingerprint(root: &Path, exclude: Option<&Path>) -> u64 {
    let mut files = Vec::new();
    collect_directory_files(root, exclude, &mut files);
    files.sort();

    let mut hasher = DefaultHasher::new();
    root.hash(&mut hasher);
    for path in files {
        path.hash(&mut hasher);
        if let Ok(metadata) = fs::metadata(&path) {
            metadata.len().hash(&mut hasher);
            if let Ok(modified) = metadata.modified() {
                if let Ok(duration) = modified.duration_since(UNIX_EPOCH) {
                    duration.as_nanos().hash(&mut hasher);
                }
            }
        }
    }
    hasher.finish()
}

/// Collects files under a directory for fingerprinting.
///
/// Inputs:
/// - `root`: directory to scan.
/// - `exclude`: optional canonical subtree to ignore.
/// - `files`: output accumulator.
///
/// Output:
/// - No return value; mutates `files`.
///
/// Transformation:
/// - Recursively descends directories and records regular files.
fn collect_directory_files(root: &Path, exclude: Option<&Path>, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if exclude.is_some_and(|exclude| {
            fs::canonicalize(&path)
                .map(|canonical| canonical.starts_with(exclude))
                .unwrap_or(false)
        }) {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if metadata.is_dir() {
            collect_directory_files(&path, exclude, files);
        } else if metadata.is_file() {
            files.push(path);
        }
    }
}

/// Copies file and CSS imports for static output.
///
/// Inputs:
/// - `module`: syntax output containing import declarations.
/// - `source_path`: source file path used to resolve relative imports.
/// - `out_dir`: static output directory.
/// - `filters`: asset include/exclude filters.
///
/// Output:
/// - Copied CSS output paths or an error message.
///
/// Transformation:
/// - Resolves file/CSS imports relative to source, filters assets, copies them
///   into the output directory, and tracks copied CSS files for validation.
fn copy_syntax_static_asset_imports(
    module: &SyntaxModuleOutput,
    source_path: &Path,
    out_dir: &Path,
    filters: &AssetFilters,
) -> Result<Vec<PathBuf>, String> {
    let base_dir = source_path.parent().unwrap_or_else(|| Path::new("."));
    let mut copied_css_outputs = Vec::new();

    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::Import {
            import_kind,
            source_path,
            ..
        } = &declaration.payload
        else {
            continue;
        };
        if !matches!(import_kind, SyntaxImportKind::File | SyntaxImportKind::Css) {
            continue;
        }
        let Some(source) = source_path.as_deref() else {
            continue;
        };
        let resolved_path = resolve_import_path(base_dir, source);
        if !filters.allows(&resolved_path) {
            continue;
        }
        let Some(file_name) = resolved_path.file_name() else {
            return Err(format!(
                "static asset import `{}` has no filename",
                resolved_path.display()
            ));
        };
        let target = out_dir.join(file_name);
        fs::copy(&resolved_path, &target).map_err(|err| {
            format!(
                "failed to copy static asset `{}` to `{}`: {}",
                resolved_path.display(),
                target.display(),
                err
            )
        })?;
        if *import_kind == SyntaxImportKind::Css {
            copied_css_outputs.push(target);
        }
    }

    copied_css_outputs.sort();
    Ok(copied_css_outputs)
}
