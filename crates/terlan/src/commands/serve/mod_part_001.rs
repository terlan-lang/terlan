use std::fs;
#[cfg(test)]
use std::io::{Read, Write};
use std::net as std_net;
use std::path::{Component, Path, PathBuf};
#[cfg(test)]
use std::pin::Pin;
use std::process::ExitCode;
use std::sync::{mpsc as std_mpsc, Arc, Mutex};
#[cfg(test)]
use std::task::{Context, Poll};
use std::thread;
#[cfg(test)]
use std::time::Instant;

#[cfg(test)]
use std::convert::Infallible;

#[cfg(test)]
use bytes::Bytes;
#[cfg(test)]
use http_body_util::{combinators::BoxBody, BodyExt, Full};
#[cfg(test)]
use hyper::body::Frame;
#[cfg(test)]
use hyper::{Request, Response};

use crate::commands::dev_dependencies;
#[cfg(test)]
use crate::runtime::vm::http::{handle_http1_in_memory_exchange, write_http1_response};
use crate::{CliCommand, CliState};

use crate::terlan_native::http::content_type_for_path;
#[cfg(test)]
use handler::handler_log_identity;
use handler::{
    execute_vm_handler_with_package_root_projected, execute_vm_router_handler_with_package_root,
    execute_vm_router_sse_admission_with_package_root,
    execute_vm_router_static_response_with_package_root,
    execute_vm_router_websocket_admission_with_package_root, http_reason_phrase,
    manifest_route_for_request, sse_router_handler, static_response_header_tuples,
    static_response_router_handler, websocket_router_handler, MatchedWebPackageHandler,
    MatchedWebPackageRoute, VmHttpChannelTransport, VmSseRouterAdmission,
    VmWebSocketRouterAdmission, WebPackageFileResponse, WebPackageSse, WebPackageStaticResponse,
    WebPackageWebSocket,
};
#[cfg(test)]
use handler::{
    manifest_file_response_for_request, manifest_handler_for_request,
    manifest_static_response_for_request,
};
#[cfg(test)]
use handler_cache::handler_cache_test_support::clear_vm_handler_module_cache_for_test;
use handler_cache::{
    cached_vm_handler_for_manifest, cached_vm_handler_runtime_for_manifest,
    cached_vm_handler_runtime_for_request, with_cached_vm_handler_runtime_for_request,
    AotHandlerRuntime,
};
#[cfg(test)]
use logging::{
    log_file_route_result, log_handler_result, log_static_result, log_static_route_result,
    next_request_id, render_dev_error_page,
};
#[cfg(test)]
use manifest::manifest_build_id;
#[cfg(test)]
use manifest::manifest_static_file_for_request;
pub(crate) use manifest::validate_web_package;
#[cfg(test)]
use response::build_http_response;
use response::{
    build_http_response_for_stream, build_http_response_owned_for_stream,
    build_http_text_response_owned_for_stream, inject_reload_script,
};
use tls::{
    acme_http01_challenge, runtime_tls_config_for_serve, AcmeHttp01Challenge, RuntimeTlsConfig,
};
#[cfg(test)]
use watch::ReloadHub;
use watch::{spawn_reload_watcher, ReloadWatchBackend};
#[cfg(test)]
use websocket::manifest_websocket_for_path;
#[cfg(test)]
use websocket::{websocket_hub, websocket_upgrade_response, WebSocketHub};
use websocket::{websocket_upgrade_state, WebSocketUpgradeState};

pub(crate) use args::{parse_serve_args, ServeArgs};

/// Boxed body type used by the Hyper development server.
#[cfg(test)]
type ServeBody = BoxBody<Bytes, Infallible>;

/// Test-only streaming SSE body backed by the standard reload hub channel.
///
/// Inputs:
/// - Receiver registered in the local reload hub.
///
/// Output:
/// - One initial SSE connection frame followed by one frame per reload
///   version.
///
/// Transformation:
/// - Keeps transitional Hyper helper tests independent of host async scheduling by
///   adapting the existing standard channel to the maintained `http_body`
///   trait directly.
#[cfg(test)]
struct ReloadSseBody {
    initial_frame_pending: bool,
    receiver: Mutex<std_mpsc::Receiver<u64>>,
}

#[cfg(test)]
impl http_body::Body for ReloadSseBody {
    type Data = Bytes;
    type Error = Infallible;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let body = self.get_mut();
        if body.initial_frame_pending {
            body.initial_frame_pending = false;
            return Poll::Ready(Some(Ok(Frame::data(Bytes::from_static(
                b": connected\n\n",
            )))));
        }
        let received = body
            .receiver
            .lock()
            .map(|receiver| receiver.try_recv())
            .unwrap_or(Err(std_mpsc::TryRecvError::Disconnected));
        match received {
            Ok(version) => {
                let event = format!("event: reload\ndata: {version}\n\n");
                Poll::Ready(Some(Ok(Frame::data(Bytes::from(event)))))
            }
            Err(std_mpsc::TryRecvError::Empty) => {
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            Err(std_mpsc::TryRecvError::Disconnected) => Poll::Ready(None),
        }
    }
}

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
    if let Err(message) = validate_handler_runtime_contract(&args) {
        eprintln!("{message}");
        return ExitCode::from(1);
    }
    if args.check_only {
        return ExitCode::SUCCESS;
    }
    let dependency_session = match manifest::adjacent_project_root(&args.web_root) {
        Some(project_root) => match dev_dependencies::start_project_dependencies(&project_root) {
            Ok(session) => Some(session),
            Err(message) => {
                eprintln!("{message}");
                return ExitCode::from(1);
            }
        },
        None => None,
    };

    let outcome = match serve_web_package(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(1)
        }
    };
    dev_dependencies::finish_dependency_session(dependency_session, outcome)
}

/// Validates dynamic handler runtime selection for `terlc serve`.
///
/// Inputs:
/// - `args`: parsed serve arguments and selected handler runtime.
///
/// Output:
/// - `Ok(())` when the package can be served under the selected runtime lane.
/// - Stable `error[serve_runtime]` diagnostic when a dynamic handler package
///   lacks source metadata needed by the VM handler runtime.
///
/// Transformation:
/// - Reads only the browser manifest handler surface and keeps static package
///   serving VM-owned by default. Dynamic handlers are accepted only when the
///   generated manifest can resolve them back to source files.
fn validate_handler_runtime_contract(args: &ServeArgs) -> Result<(), String> {
    let requires_runtime = manifest::web_package_requires_handler_runtime(&args.web_root)?;
    if requires_runtime {
        validate_dynamic_handler_sources(&args.web_root)?;
        prewarm_dynamic_handler_sources(&args.web_root)?;
    }
    Ok(())
}

/// Validates dynamic handler rows have source metadata usable by the managed
/// VM lane.
///
/// Inputs:
/// - `web_root`: generated browser package root.
///
/// Output:
/// - `Ok(())` when all dynamic handlers resolve to project source files.
/// - Stable runtime diagnostic when the manifest predates handler runtime
///   metadata or the adjacent project root is absent for source fallback.
///
/// Transformation:
/// - Keeps `terlc serve` from silently accepting dynamic handlers that cannot
///   yet be loaded into the VM runtime.
fn validate_dynamic_handler_sources(web_root: &Path) -> Result<(), String> {
    let manifest = manifest::read_web_manifest(web_root).map_err(|message| {
        format!(
            "error[serve_package]: cannot read browser package manifest `{}`: {message}",
            web_root.join("manifest.json").display()
        )
    })?;
    let project_root = manifest::adjacent_project_root(web_root).ok_or_else(|| {
        "error[serve_runtime]: dynamic handlers require an adjacent project root".to_string()
    })?;
    for handler in &manifest.handlers {
        validate_dynamic_handler_source_path(&project_root, handler)?;
    }
    Ok(())
}

/// Validates one source-backed dynamic handler path.
fn validate_dynamic_handler_source_path(
    project_root: &Path,
    handler: &handler::WebPackageHandler,
) -> Result<(), String> {
    let Some(source) = &handler.source else {
        return Err(format!(
            "error[serve_runtime]: dynamic handler `{}.{}/{}` is missing source metadata",
            handler.module, handler.function, handler.arity
        ));
    };
    let Some(path) = source_path_from_manifest(project_root, &source.path) else {
        return Err(format!(
            "error[serve_runtime]: dynamic handler `{}.{}/{}` has unsafe source path `{}`",
            handler.module, handler.function, handler.arity, source.path
        ));
    };
    if !path.is_file() {
        return Err(format!(
            "error[serve_runtime]: dynamic handler source `{}` does not exist",
            path.display()
        ));
    }
    Ok(())
}

/// Preloads source-backed dynamic handlers into the VM handler cache.
///
/// Inputs:
/// - `web_root`: generated browser package root.
///
/// Output:
/// - `Ok(())` when every dynamic handler source compiles and loads into the
///   VM cache.
/// - Stable serve-runtime diagnostic when manifest or source resolution fails.
///
/// Transformation:
/// - Moves handler compile/load cost from the first matching HTTP request to
///   `terlc serve --check` or server startup, while retaining source metadata
///   invalidation for later edits.
fn prewarm_dynamic_handler_sources(web_root: &Path) -> Result<(), String> {
    let manifest = manifest::read_web_manifest(web_root).map_err(|message| {
        format!(
            "error[serve_package]: cannot read browser package manifest `{}`: {message}",
            web_root.join("manifest.json").display()
        )
    })?;
    let project_root = manifest::adjacent_project_root(web_root).ok_or_else(|| {
        "error[serve_runtime]: dynamic handlers require an adjacent project root".to_string()
    })?;
    for handler in &manifest.handlers {
        cached_vm_handler_for_manifest(web_root, &project_root, handler)?;
    }
    for response in &manifest.static_responses {
        if let Some(handler) = static_response_router_handler(response) {
            cached_vm_handler_for_manifest(web_root, &project_root, &handler)?;
        }
    }
    for websocket in &manifest.websockets {
        if let Some(handler) = websocket_router_handler(websocket) {
            cached_vm_handler_for_manifest(web_root, &project_root, &handler)?;
        }
    }
    for endpoint in &manifest.sse {
        cached_vm_handler_for_manifest(web_root, &project_root, &sse_router_handler(endpoint))?;
    }
    Ok(())
}

/// Executes one dynamic handler through a cached VM module.
///
/// Inputs:
/// - `web_root`: generated browser package root.
/// - `matched`: manifest route handler and decoded route parameters.
/// - `request`: native HTTP request snapshot.
///
/// Output:
/// - Handler response converted for the serve-layer HTTP writer.
/// - Stable runtime diagnostic when source metadata, compilation, loading, or
///   VM execution fails.
///
/// Transformation:
/// - Resolves compiler-generated source metadata relative to the adjacent
///   project root, reuses the cached loaded VM module when possible, and
///   dispatches the matched handler.
#[allow(dead_code)] // Retained for the legacy test adapter during Hyper promotion.
fn execute_dynamic_vm_handler(
    web_root: &Path,
    matched: &MatchedWebPackageHandler,
    request: crate::terlan_native::http::Request,
) -> Result<handler::HandlerResponse, String> {
    let runtime = cached_vm_handler_runtime_for_request(web_root, &matched.handler)?;
    let projection = runtime.vm().request_projection(
        &matched.handler.module,
        &matched.handler.function,
        matched.handler.arity,
    );
    execute_dynamic_vm_handler_with_runtime(runtime.vm(), web_root, matched, request, projection)
}

/// Executes through the exact generation whose projection was used to build
/// the Request boundary value, preventing reload races between proof and use.
fn execute_dynamic_vm_handler_with_runtime(
    vm: &AotHandlerRuntime,
    web_root: &Path,
    matched: &MatchedWebPackageHandler,
    request: crate::terlan_native::http::Request,
    projection: crate::runtime::native::http::RequestFieldProjection,
) -> Result<handler::HandlerResponse, String> {
    let mut output = |_line: &str| {};
    if let Some(response) =
        execute_vm_router_handler_with_package_root(vm, matched, &request, web_root, &mut output)?
    {
        return Ok(response);
    }
    execute_vm_handler_with_package_root_projected(
        vm,
        matched,
        request,
        projection,
        web_root,
        &mut output,
    )
}

/// Executes middleware for a compiler-folded static route through its source router.
fn execute_static_vm_router(
    web_root: &Path,
    response: &WebPackageStaticResponse,
    request: &crate::terlan_native::http::Request,
) -> Result<Option<handler::HandlerResponse>, String> {
    let Some(handler) = static_response_router_handler(response) else {
        return Ok(None);
    };
    let project_root = manifest::adjacent_project_root(web_root).ok_or_else(|| {
        "error[serve_runtime]: static router owner requires an adjacent project root".to_string()
    })?;
    let runtime = cached_vm_handler_runtime_for_manifest(web_root, &project_root, &handler)?;
    let matched = MatchedWebPackageHandler {
        handler,
        params: Vec::new(),
    };
    let mut output = |_line: &str| {};
    execute_vm_router_static_response_with_package_root(
        runtime.vm(),
        &matched,
        response,
        request,
        web_root,
        &mut output,
    )
}

/// Executes source-router middleware before a WebSocket upgrade is accepted.
fn execute_websocket_vm_router(
    web_root: &Path,
    websocket: &WebPackageWebSocket,
    request: &crate::terlan_native::http::Request,
) -> Result<Option<VmWebSocketRouterAdmission>, String> {
    let Some(handler) = websocket_router_handler(websocket) else {
        return Ok(None);
    };
    let project_root = manifest::adjacent_project_root(web_root).ok_or_else(|| {
        "error[serve_runtime]: websocket router owner requires an adjacent project root".to_string()
    })?;
    let runtime = cached_vm_handler_for_manifest(web_root, &project_root, &handler)?;
    let mut output = |_line: &str| {};
    execute_vm_router_websocket_admission_with_package_root(
        runtime,
        websocket,
        request,
        web_root,
        &mut output,
    )
}

/// Executes source-router middleware before an SSE stream is admitted.
fn execute_sse_vm_router(
    web_root: &Path,
    endpoint: &WebPackageSse,
    request: &crate::terlan_native::http::Request,
) -> Result<VmSseRouterAdmission, String> {
    let handler = sse_router_handler(endpoint);
    let project_root = manifest::adjacent_project_root(web_root).ok_or_else(|| {
        "error[serve_runtime]: SSE router owner requires an adjacent project root".to_string()
    })?;
    let runtime = cached_vm_handler_for_manifest(web_root, &project_root, &handler)?;
    let mut output = |_line: &str| {};
    execute_vm_router_sse_admission_with_package_root(
        runtime,
        endpoint,
        request,
        web_root,
        &mut output,
    )
}

/// Resolves a compiler-generated manifest source path under the project root.
///
/// Inputs:
/// - `project_root`: adjacent project directory.
/// - `manifest_path`: package-safe path such as `src/app/Web.terl`.
///
/// Output:
/// - Full source path when the manifest path is relative and safe.
/// - `None` for absolute paths, parent traversal, empty paths, or platform
///   prefixes.
///
/// Transformation:
/// - Applies the same project-relative boundary as serve-time TLS and compose
///   metadata while avoiding canonicalization requirements for generated test
///   fixtures.
fn source_path_from_manifest(project_root: &Path, manifest_path: &str) -> Option<PathBuf> {
    if manifest_path.trim().is_empty() {
        return None;
    }
    let relative = Path::new(manifest_path);
    if relative.is_absolute() {
        return None;
    }
    if relative.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return None;
    }
    Some(project_root.join(relative))
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
/// - Routes plain HTTP directly to the VM stream server. TLS startup may still
///   use the temporary async ACME boundary to obtain certificates, but accepted
///   TLS sockets are served through maintained `rustls` over the same VM HTTP
///   stream adapter as plaintext.
fn serve_web_package(args: &ServeArgs) -> Result<(), String> {
    if manifest::web_package_tls_config(&args.web_root)?.is_none() {
        return serve_web_package_vm_plain(args);
    }
    let tls_config = runtime_tls_config_for_serve(&args.web_root)?;
    serve_web_package_vm_tls(args, tls_config)
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
///   caller, transfers it to the VM-stream plain HTTP server on a background
///   thread, and serves the directory through the same route graph as
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
        let _ = startup_tx.send(Ok(thread_addr));
        if let Err(message) =
            serve_bound_directory_vm_plain(listener, web_root, poll_ms, log_prefix)
        {
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

/// Binds a standard TCP listener for VM stream serving.
///
/// Inputs:
/// - `host`: bind host.
/// - `port`: bind port.
///
/// Output:
/// - Nonblocking standard TCP listener.
///
/// Transformation:
/// - Performs synchronous bind validation before the VM stream accept loop is
///   spawned, so callers receive startup failures directly.
fn bind_std_listener(host: &str, port: u16) -> Result<std_net::TcpListener, String> {
    crate::runtime::vm::protocol_task_executor::bind_protocol_listener(host, port)
}

/// Serves one plain HTTP web package through VM-owned HTTP stream handling.
///
/// Inputs:
/// - `args`: parsed serve arguments for a package without TLS metadata.
///
/// Output:
/// - `Ok(())` only if the listener loop exits without accept errors.
///
/// Transformation:
/// - Binds the existing standard listener path and feeds accepted sockets into
///   the VM HTTP stream adapter, avoiding a separate host async accept loop for the
///   no-TLS production path.
fn serve_web_package_vm_plain(args: &ServeArgs) -> Result<(), String> {
    let listener = bind_std_listener(&args.host, args.port)?;
    serve_bound_directory_vm_plain(listener, args.web_root.clone(), args.poll_ms, "terlc serve")
}

/// Serves one TLS web package through VM HTTP streams over maintained rustls.
///
/// Inputs:
/// - `args`: parsed serve arguments for a package with TLS metadata.
/// - `tls_config`: loaded rustls server configuration.
///
/// Output:
/// - `Ok(())` only if the listener loop exits without accept errors.
///
/// Transformation:
/// - Binds the existing standard listener path and wraps each accepted stream
///   in a blocking `rustls::ServerConnection`, keeping TLS mechanics in
///   maintained rustls while routing HTTP bytes through the VM stream adapter.
fn serve_web_package_vm_tls(
    args: &ServeArgs,
    tls_config: Option<RuntimeTlsConfig>,
) -> Result<(), String> {
    let Some(tls_config) = tls_config else {
        return serve_web_package_vm_plain(args);
    };
    let listener = bind_std_listener(&args.host, args.port)?;
    serve_bound_directory_vm_stream(
        listener,
        args.web_root.clone(),
        args.poll_ms,
        "terlc serve",
        Some(tls_config),
    )
}

/// Serves one bound directory listener through plain VM HTTP streams.
///
/// Inputs:
/// - `listener`: bound standard listener.
/// - `web_root`: directory to serve.
/// - `poll_ms`: reload polling interval.
/// - `log_prefix`: command prefix for diagnostics.
///
/// Output:
/// - `Ok(())` only if the listener loop exits without accept errors.
///
/// Transformation:
/// - Registers the listener with topology-sized owner-thread readiness loops.
///   Each accepted socket remains nonblocking and reactor-local through finite
///   request reads and response writes.
fn serve_bound_directory_vm_plain(
    listener: std_net::TcpListener,
    web_root: PathBuf,
    poll_ms: u64,
    log_prefix: &str,
) -> Result<(), String> {
    serve_bound_directory_vm_stream(listener, web_root, poll_ms, log_prefix, None)
}

/// Serves one bound directory listener through VM HTTP streams.
///
/// Inputs:
/// - `listener`: bound standard listener.
/// - `web_root`: directory to serve.
/// - `poll_ms`: reload polling interval.
/// - `log_prefix`: command prefix for diagnostics.
/// - `tls_config`: optional rustls server configuration.
///
/// Output:
/// - `Ok(())` only if the listener loop exits without accept errors.
///
/// Transformation:
/// - Routes finite plain HTTP through the socket-readiness reactors. TLS remains
///   isolated in a bounded blocking transport executor until maintained rustls
///   is driven directly by readiness state.
fn serve_bound_directory_vm_stream(
    listener: std_net::TcpListener,
    web_root: PathBuf,
    poll_ms: u64,
    log_prefix: &str,
    tls_config: Option<RuntimeTlsConfig>,
) -> Result<(), String> {
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
    spawn_reload_watcher(web_root.clone(), poll_ms, Arc::clone(&reload_hub));
    if let Some(tls_config) = tls_config {
        let _ = tls_config.server_config;
        return Err(
            "error[serve_tls.adapter_missing]: maintained async Hyper TLS adapter is required"
                .to_string(),
        );
    }
    hyper_server::serve(listener, web_root)
}

/// Serves one blocking stream through the VM HTTP/1 adapter.
///
/// Inputs:
/// - `stream`: readable and writable byte stream.
/// - `web_root`: generated browser package root.
///
/// Output:
/// - Success after one HTTP response is written.
///
/// Transformation:
/// - Reads exactly one HTTP/1 request with `httparse` header validation, routes
///   it through the VM stream adapter, and writes the serialized response.
#[cfg(test)]
fn serve_vm_plain_http1_connection<S>(stream: &mut S, web_root: &Path) -> Result<(), String>
where
    S: Read + Write,
{
    let request = read_vm_plain_http1_request(stream)?;
    let exchange = handle_vm_stream_http1_exchange(web_root, &request)?;
    channel_transport::serve_vm_stream_http1_exchange(stream, exchange)
}

/// Reads one complete HTTP/1 request from a blocking stream.
///
/// Inputs:
/// - `stream`: readable byte stream.
///
/// Output:
/// - Raw request bytes containing headers and the declared body.
///
/// Transformation:
/// - Uses `httparse` to detect header completion and content-length, keeping
///   protocol parsing in a maintained crate before VM HTTP validation runs.
#[cfg(test)]
fn read_vm_plain_http1_request<S>(stream: &mut S) -> Result<Vec<u8>, String>
where
    S: Read,
{
    let mut request = Vec::new();
    let mut chunk = [0_u8; 8192];
    loop {
        let read = stream
            .read(&mut chunk)
            .map_err(|err| format!("failed to read VM plain HTTP request: {err}"))?;
        if read == 0 {
            if request.is_empty() {
                return Err("empty VM plain HTTP request".to_string());
            }
            return Ok(request);
        }
        request.extend_from_slice(&chunk[..read]);
        if request.len() > 1024 * 1024 {
            return Err("VM plain HTTP request exceeds 1 MiB".to_string());
        }
        if vm_plain_http1_request_complete(&request)? {
            return Ok(request);
        }
    }
}

/// Returns whether buffered bytes contain one complete HTTP/1 request.
#[cfg(test)]
fn vm_plain_http1_request_complete(bytes: &[u8]) -> Result<bool, String> {
    let mut headers = [httparse::EMPTY_HEADER; 64];
    let mut request = httparse::Request::new(&mut headers);
    let header_length = match request
        .parse(bytes)
        .map_err(|err| format!("invalid VM plain HTTP request: {err}"))?
    {
        httparse::Status::Complete(length) => length,
        httparse::Status::Partial => return Ok(false),
    };
    let content_length = request
        .headers
        .iter()
        .find(|header| header.name.eq_ignore_ascii_case("content-length"))
        .map(|header| {
            std::str::from_utf8(header.value)
                .map_err(|_| "invalid VM plain HTTP content-length header".to_string())?
                .trim()
                .parse::<usize>()
                .map_err(|_| "invalid VM plain HTTP content-length value".to_string())
        })
        .transpose()?
        .unwrap_or(0);
    Ok(bytes.len() >= header_length.saturating_add(content_length))
}
