use std::fs;
use std::net as std_net;
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;
use std::sync::{mpsc as std_mpsc, Arc, Mutex};
use std::thread;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

use crate::{CliCommand, CliState};

mod handler;
mod manifest;
mod watch;

use handler::{execute_beam_handler, http_reason_phrase, manifest_handler_for_request};
use manifest::manifest_static_file_for_request;
pub(crate) use manifest::validate_web_package;
use watch::{spawn_reload_watcher, ReloadHub, ReloadWatchBackend};

/// Default host for `terlc serve`.
const DEFAULT_SERVE_HOST: &str = "127.0.0.1";

/// Default port for `terlc serve`.
const DEFAULT_SERVE_PORT: u16 = 3000;

/// Local live-reload endpoint reserved by `terlc serve`.
const RELOAD_ENDPOINT: &str = "/__terlan/reload";

/// Default live-reload polling interval in milliseconds.
const DEFAULT_POLL_MS: u64 = 500;

/// Parsed `terlc serve` arguments.
///
/// Inputs:
/// - Produced from command-local CLI arguments and global CLI state.
///
/// Output:
/// - Normalized web package root, host, port, and validation-only mode.
///
/// Transformation:
/// - Keeps path and network settings explicit so command execution can validate
///   the package before binding a socket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ServeArgs {
    pub(crate) web_root: PathBuf,
    pub(crate) host: String,
    pub(crate) port: u16,
    pub(crate) poll_ms: u64,
    pub(crate) check_only: bool,
}

/// Background local directory server handle.
///
/// Inputs:
/// - Produced by `spawn_directory_server` after a successful bind.
///
/// Output:
/// - Bound local address for command diagnostics.
///
/// Transformation:
/// - Keeps the detached runtime thread internal while exposing enough metadata
///   for callers such as `serve-static` to report the local URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DirectoryServerHandle {
    pub(crate) local_addr: String,
}

/// Executes the `terlc serve` command.
///
/// Inputs:
/// - `cmd`: parsed CLI command with command-local arguments.
/// - `state`: global CLI state carrying the default output directory.
///
/// Output:
/// - CLI exit code representing package validation or server startup success.
///
/// Transformation:
/// - Parses command-local flags, validates the browser package, returns early
///   for `--check`, or starts the local file-serving HTTP loop.
pub(crate) fn run(cmd: CliCommand, state: CliState) -> ExitCode {
    let args = match parse_serve_args(&cmd.args, &state) {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };

    if let Err(message) = validate_web_package(&args.web_root) {
        eprintln!("{message}");
        return ExitCode::from(1);
    }
    if args.check_only {
        return ExitCode::SUCCESS;
    }

    match serve_web_package(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(1)
        }
    }
}

/// Parses command-local `terlc serve` arguments.
///
/// Inputs:
/// - `args`: arguments after the `serve` verb.
/// - `state`: global CLI state used for the default `_build/web` directory.
///
/// Output:
/// - Parsed serve arguments or a user-facing error string.
///
/// Transformation:
/// - Accepts at most one package directory, parses `--host`, `--port`,
///   `--poll-ms`, and `--check`, and preserves unknown option failures as
///   stable CLI errors.
pub(crate) fn parse_serve_args(args: &[String], state: &CliState) -> Result<ServeArgs, String> {
    let mut web_root = None;
    let mut host = DEFAULT_SERVE_HOST.to_string();
    let mut port = DEFAULT_SERVE_PORT;
    let mut poll_ms = DEFAULT_POLL_MS;
    let mut check_only = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--host" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("terlc serve --host requires a value".to_string());
                };
                host = value.clone();
            }
            "--port" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("terlc serve --port requires a value".to_string());
                };
                port = value.parse::<u16>().map_err(|_| {
                    format!("terlc serve --port expects a u16 value, got `{value}`")
                })?;
            }
            "--poll-ms" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("terlc serve --poll-ms requires a value".to_string());
                };
                poll_ms = value.parse::<u64>().map_err(|_| {
                    format!("terlc serve --poll-ms expects a u64 value, got `{value}`")
                })?;
                if poll_ms == 0 {
                    return Err("terlc serve --poll-ms must be greater than 0".to_string());
                }
            }
            "--check" => {
                check_only = true;
            }
            option if option.starts_with('-') => {
                return Err(format!("unsupported serve option: {option}"));
            }
            path => {
                if web_root.is_some() {
                    return Err("terlc serve expects at most one web package directory".to_string());
                }
                web_root = Some(PathBuf::from(path));
            }
        }
        index += 1;
    }

    Ok(ServeArgs {
        web_root: web_root.unwrap_or_else(|| state.out_dir.join("web")),
        host,
        port,
        poll_ms,
        check_only,
    })
}

/// Starts the local browser package HTTP server.
///
/// Inputs:
/// - `args`: parsed serve arguments with a validated package root.
///
/// Output:
/// - `Ok(())` only if the server loop exits without listener errors.
/// - `Err(String)` when binding the listener fails.
///
/// Transformation:
/// - Builds the Tokio runtime boundary for the CLI command and delegates the
///   async listener loop to `serve_web_package_async`.
fn serve_web_package(args: &ServeArgs) -> Result<(), String> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_io()
        .build()
        .map_err(|err| format!("error[serve_runtime]: failed to start Tokio runtime: {err}"))?;
    runtime.block_on(serve_web_package_async(args))
}

/// Spawns a detached server for an already-generated directory.
///
/// Inputs:
/// - `web_root`: directory to serve.
/// - `host`: bind host.
/// - `port`: bind port.
/// - `poll_ms`: reload polling interval.
/// - `log_prefix`: command prefix for diagnostics.
///
/// Output:
/// - Bound local address when the server thread starts successfully.
///
/// Transformation:
/// - Binds a standard listener synchronously so startup errors return to the
///   caller, transfers it into a Tokio listener on a background thread, and
///   serves the directory through the same HTTP/SSE implementation as
///   `terlc serve`.
pub(crate) fn spawn_directory_server(
    web_root: PathBuf,
    host: String,
    port: u16,
    poll_ms: u64,
    log_prefix: &'static str,
) -> Result<DirectoryServerHandle, String> {
    let listener = bind_std_listener(&host, port)?;
    let local_addr = listener
        .local_addr()
        .map(|addr| addr.to_string())
        .unwrap_or_else(|_| format!("{host}:{port}"));
    let (startup_tx, startup_rx) = std_mpsc::channel();
    let thread_addr = local_addr.clone();

    thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_multi_thread()
            .enable_io()
            .build()
        {
            Ok(runtime) => runtime,
            Err(err) => {
                let _ = startup_tx.send(Err(format!(
                    "error[serve_runtime]: failed to start Tokio runtime: {err}"
                )));
                return;
            }
        };
        let _ = startup_tx.send(Ok(thread_addr));
        if let Err(message) = runtime.block_on(serve_bound_directory_async(
            listener, web_root, poll_ms, log_prefix,
        )) {
            eprintln!("{message}");
        }
    });

    match startup_rx.recv() {
        Ok(Ok(local_addr)) => Ok(DirectoryServerHandle { local_addr }),
        Ok(Err(message)) => Err(message),
        Err(err) => Err(format!(
            "error[serve_runtime]: failed to receive server startup status: {err}"
        )),
    }
}

/// Binds a standard TCP listener for handoff into Tokio.
///
/// Inputs:
/// - `host`: bind host.
/// - `port`: bind port.
///
/// Output:
/// - Nonblocking standard TCP listener.
///
/// Transformation:
/// - Performs synchronous bind validation before a background runtime is
///   spawned, so callers receive startup failures directly.
fn bind_std_listener(host: &str, port: u16) -> Result<std_net::TcpListener, String> {
    let listener = std_net::TcpListener::bind(format!("{host}:{port}"))
        .map_err(|err| format!("error[serve_bind]: failed to bind {host}:{port}: {err}"))?;
    listener.set_nonblocking(true).map_err(|err| {
        format!("error[serve_bind]: failed to set {host}:{port} nonblocking: {err}")
    })?;
    Ok(listener)
}

/// Runs the async local browser package HTTP server.
///
/// Inputs:
/// - `args`: parsed serve arguments with a validated package root.
///
/// Output:
/// - `Ok(())` only if the listener loop exits without bind errors.
/// - `Err(String)` when binding fails.
///
/// Transformation:
/// - Binds a Tokio TCP listener, prints the serving URL, and spawns one async
///   task per accepted connection.
async fn serve_web_package_async(args: &ServeArgs) -> Result<(), String> {
    let listener = bind_std_listener(&args.host, args.port)?;
    serve_bound_directory_async(listener, args.web_root.clone(), args.poll_ms, "terlc serve").await
}

/// Serves one bound directory listener through Tokio.
///
/// Inputs:
/// - `listener`: nonblocking standard TCP listener.
/// - `web_root`: directory to serve.
/// - `poll_ms`: reload polling interval.
/// - `log_prefix`: command prefix for diagnostics.
///
/// Output:
/// - `Ok(())` only if the listener loop exits without accept errors.
///
/// Transformation:
/// - Converts a bound standard listener into Tokio, starts the reload watcher,
///   prints serving diagnostics, and spawns one async task per connection.
async fn serve_bound_directory_async(
    listener: std_net::TcpListener,
    web_root: PathBuf,
    poll_ms: u64,
    log_prefix: &str,
) -> Result<(), String> {
    let listener = TcpListener::from_std(listener)
        .map_err(|err| format!("error[serve_bind]: failed to adopt TCP listener: {err}"))?;
    let local_addr = listener
        .local_addr()
        .map(|addr| addr.to_string())
        .unwrap_or_else(|_| "unknown".to_string());
    eprintln!("{log_prefix}: serving {}", web_root.display());
    eprintln!("{log_prefix}: http://{local_addr}");
    eprintln!("{log_prefix}: reload stream {RELOAD_ENDPOINT}");
    eprintln!(
        "{log_prefix}: reload watcher {}",
        ReloadWatchBackend::selected().name()
    );

    let reload_hub = Arc::new(Mutex::new(Vec::new()));
    spawn_reload_watcher(web_root.clone(), poll_ms, Arc::clone(&reload_hub));
    loop {
        match listener.accept().await {
            Ok((stream, _peer_addr)) => {
                let root = web_root.clone();
                let reload_hub = Arc::clone(&reload_hub);
                tokio::spawn(async move {
                    handle_web_connection(stream, &root, reload_hub).await;
                });
            }
            Err(err) => {
                return Err(format!(
                    "error[serve_accept]: failed to accept HTTP connection: {err}"
                ));
            }
        }
    }
}

/// Handles one HTTP request for the browser package server.
///
/// Inputs:
/// - `stream`: accepted TCP stream.
/// - `web_root`: validated package root.
/// - `reload_hub`: shared reload subscriber registry.
///
/// Output:
/// - Future that writes one HTTP response when possible.
///
/// Transformation:
/// - Parses a minimal HTTP request, serves `GET`/`HEAD` files from the package
///   root, and rejects unsupported methods or unsafe paths.
async fn handle_web_connection(mut stream: TcpStream, web_root: &Path, reload_hub: ReloadHub) {
    let mut buffer = [0; 8192];
    let Ok(read) = stream.read(&mut buffer).await else {
        return;
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

    let request_path = target.split('?').next().unwrap_or("/");
    if request_path == RELOAD_ENDPOINT {
        handle_reload_sse(stream, reload_hub).await;
        return;
    }

    if method == "GET" || method == "HEAD" {
        if let Some(response_path) = manifest_static_file_for_request(web_root, request_path) {
            write_static_file_response(&mut stream, method, &response_path).await;
            return;
        }
    }

    if let Some(handler) = manifest_handler_for_request(web_root, method, request_path) {
        match execute_beam_handler(web_root, &handler, method, request_path) {
            Ok(response) => {
                let _ = write_http_response(
                    &mut stream,
                    response.status,
                    http_reason_phrase(response.status),
                    &response.content_type,
                    &response.body,
                    method == "HEAD",
                )
                .await;
            }
            Err(message) => {
                let _ = write_http_response(
                    &mut stream,
                    502,
                    "Bad Gateway",
                    "text/plain; charset=utf-8",
                    message.as_bytes(),
                    method == "HEAD",
                )
                .await;
            }
        }
        return;
    }

    if method != "GET" && method != "HEAD" {
        let _ = write_http_response(
            &mut stream,
            405,
            "Method Not Allowed",
            "text/plain; charset=utf-8",
            b"method not allowed",
            method == "HEAD",
        )
        .await;
        return;
    }

    let Some(file_path) = request_file_path(web_root, request_path) else {
        let _ = write_http_response(
            &mut stream,
            400,
            "Bad Request",
            "text/plain; charset=utf-8",
            b"bad request",
            method == "HEAD",
        )
        .await;
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

    write_static_file_response(&mut stream, method, &response_path).await;
}

/// Writes one static file response for `terlc serve`.
///
/// Inputs:
/// - `stream`: accepted HTTP stream.
/// - `method`: parsed request method.
/// - `response_path`: resolved package file path to read.
///
/// Output:
/// - Future that writes a 200 response for readable files or a stable 404
///   response when the file cannot be read.
///
/// Transformation:
/// - Reads the selected file, injects the local reload client for HTML
///   responses, selects MIME type by extension, and delegates final header/body
///   writing to the shared HTTP response helper.
async fn write_static_file_response(stream: &mut TcpStream, method: &str, response_path: &Path) {
    let bytes = match fs::read(&response_path) {
        Ok(bytes) => bytes,
        Err(_) => {
            let _ = write_http_response(
                stream,
                404,
                "Not Found",
                "text/plain; charset=utf-8",
                b"not found",
                method == "HEAD",
            )
            .await;
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
    let _ = write_http_response(stream, 200, "OK", content_type, &body, method == "HEAD").await;
}

/// Handles one local live-reload SSE request.
///
/// Inputs:
/// - `stream`: accepted TCP stream.
/// - `reload_hub`: shared reload subscriber registry.
///
/// Output:
/// - Future that writes the SSE stream until the client disconnects.
///
/// Transformation:
/// - Registers the connection as a reload subscriber, writes SSE headers, and
///   forwards reload version values sent by the future watcher integration.
async fn handle_reload_sse(mut stream: TcpStream, reload_hub: ReloadHub) {
    let (tx, mut rx) = mpsc::unbounded_channel();
    if let Ok(mut subscribers) = reload_hub.lock() {
        subscribers.push(tx);
    }

    if stream
        .write_all(render_reload_sse_headers().as_bytes())
        .await
        .is_err()
    {
        return;
    }
    let _ = stream.flush().await;

    while let Some(version) = rx.recv().await {
        let event = format!("event: reload\ndata: {version}\n\n");
        if stream.write_all(event.as_bytes()).await.is_err() {
            break;
        }
        if stream.flush().await.is_err() {
            break;
        }
    }
}

/// Renders the local live-reload SSE response headers.
///
/// Inputs:
/// - No dynamic input; reload responses always use the same local development
///   stream contract.
///
/// Output:
/// - Static HTTP response header and initial SSE comment text.
///
/// Transformation:
/// - Centralizes the reload endpoint header contract so it stays aligned with
///   the rest of `terlc serve`: no cache, explicit event-stream content type,
///   no-sniff protection, and a persistent local development connection.
fn render_reload_sse_headers() -> &'static str {
    concat!(
        "HTTP/1.1 200 OK\r\n",
        "Content-Type: text/event-stream\r\n",
        "Cache-Control: no-cache\r\n",
        "X-Content-Type-Options: nosniff\r\n",
        "Connection: keep-alive\r\n",
        "Access-Control-Allow-Origin: *\r\n",
        "\r\n",
        ": connected\n\n"
    )
}

/// Converts a URL request path into a package file path.
///
/// Inputs:
/// - `web_root`: package root.
/// - `request_path`: URL path component.
///
/// Output:
/// - Safe filesystem path under `web_root`, or `None` for unsafe paths.
///
/// Transformation:
/// - Maps `/` to `index.html`, strips a leading slash, and rejects traversal,
///   Windows separators, and NUL bytes.
fn request_file_path(web_root: &Path, request_path: &str) -> Option<PathBuf> {
    let trimmed = request_path.trim_start_matches('/');
    if trimmed.is_empty() {
        return Some(web_root.join("index.html"));
    }
    package_relative_path(web_root, trimmed)
}

/// Converts a manifest-relative path into a safe package file path.
///
/// Inputs:
/// - `web_root`: package root.
/// - `relative`: manifest-relative path text.
///
/// Output:
/// - Safe filesystem path under `web_root`, or `None` for unsafe paths.
///
/// Transformation:
/// - Rejects absolute paths, parent components, prefixes, Windows separators,
///   and NUL bytes before joining accepted normal components.
pub(super) fn package_relative_path(web_root: &Path, relative: &str) -> Option<PathBuf> {
    if relative.contains('\\') || relative.contains('\0') {
        return None;
    }
    let relative_path = Path::new(relative);
    if relative_path.is_absolute() {
        return None;
    }

    let mut output = web_root.to_path_buf();
    for component in relative_path.components() {
        match component {
            Component::Normal(segment) => output.push(segment),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    Some(output)
}

/// Returns a content type for one served package file.
///
/// Inputs:
/// - `path`: response file path.
///
/// Output:
/// - Static content-type string.
///
/// Transformation:
/// - Maps common browser artifact extensions to MIME types and falls back to
///   octet stream for opaque file assets.
fn content_type_for_path(path: &Path) -> &'static str {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("map") => "application/json; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("md") => "text/markdown; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("avif") => "image/avif",
        Some("ico") => "image/x-icon",
        Some("wasm") => "application/wasm",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("ttf") => "font/ttf",
        Some("otf") => "font/otf",
        Some("txt") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// Injects local live-reload wiring into one HTML document.
///
/// Inputs:
/// - `html`: served HTML response text.
///
/// Output:
/// - HTML response text with a local reload script inserted.
///
/// Transformation:
/// - Preserves documents that already reference the reload endpoint, inserts
///   before `</body>` when present, and appends otherwise. The packaged file on
///   disk is never modified.
fn inject_reload_script(html: &str) -> String {
    if html.contains(RELOAD_ENDPOINT) {
        return html.to_string();
    }
    let script = format!(
        "<script>(()=>{{const es=new EventSource('{}');es.addEventListener('reload',()=>location.reload());}})();</script>",
        RELOAD_ENDPOINT
    );
    if let Some(index) = html.rfind("</body>") {
        let mut output = String::with_capacity(html.len() + script.len());
        output.push_str(&html[..index]);
        output.push_str(&script);
        output.push_str(&html[index..]);
        output
    } else {
        let mut output = String::with_capacity(html.len() + script.len());
        output.push_str(html);
        output.push_str(&script);
        output
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
/// - Emits the stable response header block, then writes optional response body
///   bytes for non-HEAD responses.
async fn write_http_response(
    stream: &mut TcpStream,
    status: u16,
    reason: &str,
    content_type: &str,
    body: &[u8],
    head_only: bool,
) -> std::io::Result<()> {
    let headers = render_http_response_headers(status, reason, content_type, body.len());
    stream.write_all(&headers).await?;
    if !head_only {
        stream.write_all(body).await?;
    }
    stream.flush().await
}

/// Renders the stable HTTP response headers used by `terlc serve`.
///
/// Inputs:
/// - `status`: numeric HTTP status.
/// - `reason`: HTTP reason phrase.
/// - `content_type`: response content type.
/// - `content_length`: byte length of the response body.
///
/// Output:
/// - Complete HTTP/1.1 header block ending in the blank-line delimiter.
///
/// Transformation:
/// - Centralizes local-development cache behavior, content length, connection
///   close semantics, and MIME sniffing protection so route and static
///   responses share one testable header contract.
fn render_http_response_headers(
    status: u16,
    reason: &str,
    content_type: &str,
    content_length: usize,
) -> Vec<u8> {
    format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {content_length}\r\nCache-Control: no-cache\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n"
    )
    .into_bytes()
}

#[cfg(test)]
#[path = "serve_test.rs"]
mod serve_test;
