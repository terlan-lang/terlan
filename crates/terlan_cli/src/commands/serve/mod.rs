use std::convert::Infallible;
use std::fs;
use std::net as std_net;
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;
use std::sync::{mpsc as std_mpsc, Arc, Mutex};
use std::thread;
use std::time::Instant;

use bytes::Bytes;
use http_body_util::{combinators::BoxBody, BodyExt, Channel, Full};
use hyper::body::Frame;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_rustls::TlsAcceptor;

use crate::{CliCommand, CliState};

mod args;
pub(crate) mod compose_check;
mod handler;
mod logging;
mod manifest;
mod response;
mod tls;
mod watch;
mod websocket;

use handler::{
    execute_beam_error_handler, execute_beam_handler, handler_log_identity, http_reason_phrase,
    manifest_error_handler, manifest_file_response_for_request, manifest_handler_for_request,
    manifest_static_response_for_request, static_response_header_tuples, WebPackageFileResponse,
};
use logging::{
    log_file_route_result, log_handler_result, log_static_result, log_static_route_result,
    next_request_id, render_dev_error_page,
};
pub(crate) use manifest::validate_web_package;
use manifest::{manifest_build_id, manifest_static_file_for_request};
use response::{build_http_response, inject_reload_script};
use terlan_safenative::http::content_type_for_path;
use tls::{acme_http01_challenge, runtime_tls_config, AcmeHttp01Challenge, RuntimeTlsConfig};
use watch::{spawn_reload_watcher, ReloadHub, ReloadWatchBackend};
use websocket::{
    is_websocket_upgrade, manifest_websocket_for_request, serve_websocket_upgrade, websocket_hub,
    websocket_upgrade_response, WebSocketHub,
};

pub(crate) use args::{parse_serve_args, ServeArgs};

/// Boxed body type used by the Hyper development server.
type ServeBody = BoxBody<Bytes, Infallible>;

/// Local live-reload endpoint reserved by `terlc serve`.
const RELOAD_ENDPOINT: &str = "/__terlan/reload";

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
    if let Some(project_root) = manifest::adjacent_project_root(&args.web_root) {
        if let Err(message) = compose_check::start_project_compose_dependencies(&project_root) {
            eprintln!("{message}");
            return ExitCode::from(1);
        }
    }

    match serve_web_package(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(1)
        }
    }
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
///   async listener loop to `serve_web_package_async` after the TLS runtime
///   boundary accepts the current package configuration.
fn serve_web_package(args: &ServeArgs) -> Result<(), String> {
    let tls_config = runtime_tls_config(&args.web_root)?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|err| format!("error[serve_runtime]: failed to start Tokio runtime: {err}"))?;
    runtime.block_on(serve_web_package_async(args, tls_config))
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
            .enable_all()
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
            listener, web_root, poll_ms, log_prefix, None,
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
async fn serve_web_package_async(
    args: &ServeArgs,
    tls_config: Option<RuntimeTlsConfig>,
) -> Result<(), String> {
    let listener = bind_std_listener(&args.host, args.port)?;
    serve_bound_directory_async(
        listener,
        args.web_root.clone(),
        args.poll_ms,
        "terlc serve",
        tls_config,
    )
    .await
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
    tls_config: Option<RuntimeTlsConfig>,
) -> Result<(), String> {
    let listener = TcpListener::from_std(listener)
        .map_err(|err| format!("error[serve_bind]: failed to adopt TCP listener: {err}"))?;
    let local_addr = listener
        .local_addr()
        .map(|addr| addr.to_string())
        .unwrap_or_else(|_| "unknown".to_string());
    eprintln!("{log_prefix}: serving {}", web_root.display());
    let scheme = if tls_config.is_some() {
        "https"
    } else {
        "http"
    };
    eprintln!("{log_prefix}: {scheme}://{local_addr}");
    eprintln!("{log_prefix}: reload stream {RELOAD_ENDPOINT}");
    eprintln!(
        "{log_prefix}: reload watcher {}",
        ReloadWatchBackend::selected().name()
    );

    let reload_hub = Arc::new(Mutex::new(Vec::new()));
    let websocket_hub = websocket_hub();
    spawn_reload_watcher(web_root.clone(), poll_ms, Arc::clone(&reload_hub));
    let tls_acceptor = tls_config.map(|config| TlsAcceptor::from(config.server_config));
    loop {
        match listener.accept().await {
            Ok((stream, _peer_addr)) => {
                let root = web_root.clone();
                let reload_hub = Arc::clone(&reload_hub);
                let websocket_hub = Arc::clone(&websocket_hub);
                let tls_acceptor = tls_acceptor.clone();
                tokio::spawn(async move {
                    if let Some(tls_acceptor) = tls_acceptor {
                        match tls_acceptor.accept(stream).await {
                            Ok(tls_stream) => {
                                serve_connection(
                                    tls_stream,
                                    root,
                                    reload_hub,
                                    websocket_hub,
                                    "HTTPS",
                                )
                                .await;
                            }
                            Err(err) => {
                                eprintln!("error[serve_tls]: TLS handshake failed: {err}");
                            }
                        }
                    } else {
                        serve_connection(stream, root, reload_hub, websocket_hub, "HTTP").await;
                    }
                });
            }
            Err(err) => {
                return Err(format!(
                    "error[serve_accept]: failed to accept {scheme} connection: {err}"
                ));
            }
        }
    }
}

/// Serves one accepted HTTP or HTTPS stream.
///
/// Inputs:
/// - `stream`: accepted socket or rustls stream.
/// - `root`: package root for request routing.
/// - `reload_hub`: shared reload subscribers.
/// - `websocket_hub`: shared WebSocket room state.
/// - `protocol`: diagnostic protocol label.
///
/// Output:
/// - None; connection diagnostics are written to stderr.
///
/// Transformation:
/// - Adapts the stream into Hyper's Tokio IO wrapper and delegates all request
///   behavior to the shared `handle_hyper_request` function.
async fn serve_connection<S>(
    stream: S,
    root: PathBuf,
    reload_hub: ReloadHub,
    websocket_hub: WebSocketHub,
    protocol: &str,
) where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let io = TokioIo::new(stream);
    let service = service_fn(move |request| {
        let root = root.clone();
        let reload_hub = Arc::clone(&reload_hub);
        let websocket_hub = Arc::clone(&websocket_hub);
        async move {
            Ok::<_, Infallible>(
                handle_hyper_request(request, root, reload_hub, websocket_hub).await,
            )
        }
    });
    let connection = http1::Builder::new().serve_connection(io, service);
    if let Err(err) = connection.with_upgrades().await {
        eprintln!("error[serve_http]: {protocol} connection failed: {err}");
    }
}

/// Handles one Hyper request for the browser package server.
///
/// Inputs:
/// - `request`: Hyper request accepted by the local HTTP service.
/// - `web_root`: validated package root owned by the connection task.
/// - `reload_hub`: shared reload subscriber registry.
/// - `websocket_hub`: shared WebSocket room state.
///
/// Output:
/// - Hyper response carrying the selected route body.
///
/// Transformation:
/// - Reads method, URI, headers, and body through Hyper/http types, then
///   preserves the existing Terlan route-manifest and BEAM handler bridge
///   behavior above the protocol layer.
async fn handle_hyper_request<B>(
    mut request: Request<B>,
    web_root: PathBuf,
    reload_hub: ReloadHub,
    websocket_hub: WebSocketHub,
) -> Response<ServeBody>
where
    B: hyper::body::Body<Data = Bytes> + Send + 'static,
    B::Error: std::fmt::Display,
{
    let request_id = next_request_id();
    let build_id = manifest_build_id(&web_root);
    let method = request.method().as_str().to_string();
    let request_path = request.uri().path().to_string();
    let request_query = request.uri().query().unwrap_or("").to_string();
    if let Some(websocket) = manifest_websocket_for_request(&web_root, &method, &request_path) {
        if !is_websocket_upgrade(request.headers()) {
            return serve_response(
                426,
                "Upgrade Required",
                "text/plain; charset=utf-8",
                &[("upgrade".to_string(), "websocket".to_string())],
                b"websocket upgrade required",
                false,
            );
        }
        let upgrade = hyper::upgrade::on(&mut request);
        let response = websocket_upgrade_response(&request);
        tokio::spawn(serve_websocket_upgrade(
            upgrade,
            websocket_hub,
            websocket,
            request_query,
        ));
        return response;
    }

    let (parts, body) = request.into_parts();
    let request_headers = request_header_pairs(&parts.headers);
    let cookie_header = parts
        .headers
        .get(http::header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_string();

    if request_path == RELOAD_ENDPOINT {
        return reload_sse_response(reload_hub);
    }

    if method == "GET" || method == "HEAD" {
        match acme_http01_challenge(&web_root, &request_path) {
            Ok(AcmeHttp01Challenge::Found(body)) => {
                return serve_response(
                    200,
                    "OK",
                    "text/plain; charset=utf-8",
                    &[],
                    body.as_bytes(),
                    method == "HEAD",
                );
            }
            Ok(AcmeHttp01Challenge::Missing) => {
                return serve_response(
                    404,
                    "Not Found",
                    "text/plain; charset=utf-8",
                    &[],
                    b"not found",
                    method == "HEAD",
                );
            }
            Ok(AcmeHttp01Challenge::Invalid(message)) => {
                return serve_response(
                    400,
                    "Bad Request",
                    "text/plain; charset=utf-8",
                    &[],
                    message.as_bytes(),
                    method == "HEAD",
                );
            }
            Ok(AcmeHttp01Challenge::NotMatched) => {}
            Err(message) => {
                return serve_response(
                    500,
                    "Internal Server Error",
                    "text/plain; charset=utf-8",
                    &[],
                    message.as_bytes(),
                    method == "HEAD",
                );
            }
        }
    }

    let request_body = match body.collect().await {
        Ok(collected) => String::from_utf8_lossy(&collected.to_bytes()).to_string(),
        Err(err) => {
            return serve_response(
                400,
                "Bad Request",
                "text/plain; charset=utf-8",
                &[],
                format!("bad request body: {err}").as_bytes(),
                method == "HEAD",
            );
        }
    };

    if let Some(response) = manifest_static_response_for_request(&web_root, &method, &request_path)
    {
        let started = Instant::now();
        let status = response.status;
        let headers = static_response_header_tuples(&response.headers).unwrap_or_else(|message| {
            eprintln!("{message}");
            Vec::new()
        });
        let output = serve_response(
            response.status,
            http_reason_phrase(response.status),
            &response.content_type,
            &headers,
            response.body.as_bytes(),
            method == "HEAD",
        );
        log_static_route_result(
            request_id,
            &build_id,
            &method,
            &request_path,
            &response.method,
            &response.route,
            response.source.as_ref(),
            status,
            started.elapsed().as_millis(),
        );
        return output;
    }

    if let Some(handler) = manifest_handler_for_request(&web_root, &method, &request_path) {
        let identity = handler_log_identity(&handler);
        let started = Instant::now();
        match execute_beam_handler(
            &web_root,
            &handler,
            &method,
            &request_path,
            &request_query,
            &request_headers,
            &cookie_header,
            &request_body,
        ) {
            Ok(response) => {
                let output = serve_response(
                    response.status,
                    http_reason_phrase(response.status),
                    &response.content_type,
                    &response.headers,
                    &response.body,
                    method == "HEAD",
                );
                log_handler_result(
                    request_id,
                    &build_id,
                    &method,
                    &request_path,
                    &identity,
                    response.status,
                    started.elapsed().as_millis(),
                );
                return output;
            }
            Err(message) => {
                if let Some(error_handler) = manifest_error_handler(&web_root) {
                    match execute_beam_error_handler(&web_root, &error_handler, &message) {
                        Ok(response) => {
                            let output = serve_response(
                                response.status,
                                http_reason_phrase(response.status),
                                &response.content_type,
                                &response.headers,
                                &response.body,
                                method == "HEAD",
                            );
                            log_handler_result(
                                request_id,
                                &build_id,
                                &method,
                                &request_path,
                                &identity,
                                response.status,
                                started.elapsed().as_millis(),
                            );
                            return output;
                        }
                        Err(error_handler_message) => {
                            eprintln!("{error_handler_message}");
                        }
                    }
                }
                let body = render_dev_error_page(
                    request_id,
                    &build_id,
                    &method,
                    &request_path,
                    &identity,
                    &message,
                )
                .into_bytes();
                let output = serve_response(
                    502,
                    "Bad Gateway",
                    "text/html; charset=utf-8",
                    &[],
                    &body,
                    method == "HEAD",
                );
                log_handler_result(
                    request_id,
                    &build_id,
                    &method,
                    &request_path,
                    &identity,
                    502,
                    started.elapsed().as_millis(),
                );
                return output;
            }
        }
    }

    if method == "GET" || method == "HEAD" {
        if let Some(response_path) = manifest_static_file_for_request(&web_root, &request_path) {
            let started = Instant::now();
            let (status, output) = static_file_response(&method, &response_path);
            log_static_result(
                request_id,
                &build_id,
                &method,
                &request_path,
                &response_path,
                status,
                started.elapsed().as_millis(),
            );
            return output;
        }
    }

    if let Some((response, response_path)) =
        manifest_file_response_for_request(&web_root, &method, &request_path)
    {
        let started = Instant::now();
        let (status, output) = manifest_file_response(&method, &response_path, &response);
        log_file_route_result(
            request_id,
            &build_id,
            &method,
            &request_path,
            &response.method,
            &response.route,
            &response_path,
            response.source.as_ref(),
            status,
            started.elapsed().as_millis(),
        );
        return output;
    }

    if method != "GET" && method != "HEAD" {
        return serve_response(
            405,
            "Method Not Allowed",
            "text/plain; charset=utf-8",
            &[],
            b"method not allowed",
            method == "HEAD",
        );
    }

    let Some(file_path) = request_file_path(&web_root, &request_path) else {
        return serve_response(
            400,
            "Bad Request",
            "text/plain; charset=utf-8",
            &[],
            b"bad request",
            method == "HEAD",
        );
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

    let started = Instant::now();
    let (status, output) = static_file_response(&method, &response_path);
    log_static_result(
        request_id,
        &build_id,
        &method,
        &request_path,
        &response_path,
        status,
        started.elapsed().as_millis(),
    );
    output
}

/// Extracts source-visible request header pairs from Hyper metadata.
///
/// Inputs:
/// - `headers`: Hyper/http request header map.
///
/// Output:
/// - Header name/value pairs with lowercase header names and UTF-8-lossy
///   values.
///
/// Transformation:
/// - Converts the protocol-owned header map into the temporary BEAM handler
///   request-map shape without exposing Hyper types to generated handler code.
fn request_header_pairs(headers: &http::HeaderMap) -> Vec<(String, String)> {
    headers
        .iter()
        .map(|(name, value)| {
            (
                name.as_str().to_ascii_lowercase(),
                String::from_utf8_lossy(value.as_bytes()).into_owned(),
            )
        })
        .collect()
}

/// Builds one static file response for `terlc serve`.
///
/// Inputs:
/// - `method`: parsed request method.
/// - `response_path`: resolved package file path to read.
///
/// Output:
/// - Emitted status code for request logging.
/// - Hyper response for the selected file or a stable 404.
///
/// Transformation:
/// - Reads the selected file, injects the local reload client for HTML
///   responses, selects MIME type by extension, and builds a typed HTTP
///   response for Hyper.
fn static_file_response(method: &str, response_path: &Path) -> (u16, Response<ServeBody>) {
    let bytes = match fs::read(&response_path) {
        Ok(bytes) => bytes,
        Err(_) => {
            return (
                404,
                serve_response(
                    404,
                    "Not Found",
                    "text/plain; charset=utf-8",
                    &[],
                    b"not found",
                    method == "HEAD",
                ),
            );
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
    (
        200,
        serve_response(200, "OK", &content_type, &[], &body, method == "HEAD"),
    )
}

/// Builds one manifest file-route response for `terlc serve`.
///
/// Inputs:
/// - `method`: parsed request method.
/// - `response_path`: resolved package file path to read.
/// - `response`: manifest file response metadata.
///
/// Output:
/// - Emitted status code for request logging.
/// - Hyper response for the configured file or a stable 404.
///
/// Transformation:
/// - Reads the selected file, uses explicit manifest content type when
///   supplied or infers it by path, and builds a typed HTTP response without
///   modifying the file bytes.
fn manifest_file_response(
    method: &str,
    response_path: &Path,
    response: &WebPackageFileResponse,
) -> (u16, Response<ServeBody>) {
    let bytes = match fs::read(response_path) {
        Ok(bytes) => bytes,
        Err(_) => {
            return (
                404,
                serve_response(
                    404,
                    "Not Found",
                    "text/plain; charset=utf-8",
                    &[],
                    b"not found",
                    method == "HEAD",
                ),
            );
        }
    };
    let inferred_content_type;
    let content_type = match response.content_type.as_deref() {
        Some(content_type) => content_type,
        None => {
            inferred_content_type = content_type_for_path(response_path);
            inferred_content_type.as_str()
        }
    };
    (
        response.status,
        serve_response(
            response.status,
            http_reason_phrase(response.status),
            content_type,
            &[],
            &bytes,
            method == "HEAD",
        ),
    )
}

/// Builds one local live-reload SSE response.
///
/// Inputs:
/// - `reload_hub`: shared reload subscriber registry.
///
/// Output:
/// - Streaming Hyper response for local reload events.
///
/// Transformation:
/// - Registers the connection as a reload subscriber, emits the initial SSE
///   comment, and forwards reload version values as streamed frames.
fn reload_sse_response(reload_hub: ReloadHub) -> Response<ServeBody> {
    let (tx, rx) = mpsc::unbounded_channel();
    if let Ok(mut subscribers) = reload_hub.lock() {
        subscribers.push(tx);
    }

    let (mut sender, body) = Channel::new(8);
    tokio::spawn(async move {
        let _ = sender
            .send(Frame::data(Bytes::from_static(b": connected\n\n")))
            .await;
        let mut rx = rx;
        while let Some(version) = rx.recv().await {
            let event = format!("event: reload\ndata: {version}\n\n");
            if sender.send(Frame::data(Bytes::from(event))).await.is_err() {
                break;
            }
        }
    });
    let body = body.boxed();

    http::Response::builder()
        .status(200)
        .header(http::header::CONTENT_TYPE, "text/event-stream")
        .header(http::header::CACHE_CONTROL, "no-cache")
        .header("x-content-type-options", "nosniff")
        .header(http::header::CONNECTION, "keep-alive")
        .header(http::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .body(body)
        .unwrap_or_else(|err| internal_error_response(format!("reload response failed: {err}")))
}

/// Builds one Hyper response from validated response metadata.
///
/// Inputs:
/// - `status`: numeric response status.
/// - `reason`: response reason phrase retained for fallback diagnostics.
/// - `content_type`: response content type.
/// - `extra_headers`: validated handler or manifest headers.
/// - `body`: response body bytes.
/// - `head_only`: whether to omit emitted body bytes.
///
/// Output:
/// - Hyper response with a boxed body.
///
/// Transformation:
/// - Builds a Rust `http::Response<Vec<u8>>` through the shared response
///   helper, then converts its body into Hyper's boxed body type.
fn serve_response(
    status: u16,
    reason: &str,
    content_type: &str,
    extra_headers: &[(String, String)],
    body: &[u8],
    head_only: bool,
) -> Response<ServeBody> {
    match build_http_response(status, content_type, extra_headers, body, head_only) {
        Ok(response) => response.map(boxed_body),
        Err(message) => internal_error_response(format!(
            "response build failed for {status} {reason}: {message}"
        )),
    }
}

/// Wraps bytes in the Hyper body type used by `terlc serve`.
///
/// Inputs:
/// - `body`: response bytes selected by route handling.
///
/// Output:
/// - Boxed Hyper body.
///
/// Transformation:
/// - Converts concrete bytes into a single-frame body accepted by Hyper.
fn boxed_body(body: Vec<u8>) -> ServeBody {
    Full::new(Bytes::from(body)).boxed()
}

/// Builds a generic internal error response for protocol-boundary failures.
///
/// Inputs:
/// - `message`: diagnostic text for local development response body.
///
/// Output:
/// - Hyper response with status 500.
///
/// Transformation:
/// - Avoids panics in the Hyper service by turning unexpected response build
///   failures into ordinary local development responses.
fn internal_error_response(message: String) -> Response<ServeBody> {
    http::Response::builder()
        .status(500)
        .header(http::header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(boxed_body(message.into_bytes()))
        .unwrap_or_else(|_| Response::new(boxed_body(b"internal server error".to_vec())))
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

#[cfg(test)]
#[path = "serve_test.rs"]
mod serve_test;
