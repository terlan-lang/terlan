use super::request_dispatch::*;
use super::response_rendering::*;
use super::*;

mod entry;
mod vm_stream;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) use entry::run;
pub(crate) use entry::run_serve_runtime;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
use vm_stream::{bind_std_listener, serve_bound_directory_vm_plain};
use vm_stream::{serve_web_package_vm_plain, serve_web_package_vm_tls};

thread_local! {
    static SUSPENDABLE_ROUTE_DECISION: RefCell<Option<SuspendableRouteDecision>> =
        const { RefCell::new(None) };
}

pub(super) struct SuspendableRouteDecision {
    epoch: u64,
    web_root: PathBuf,
    method: String,
    path: String,
    suspending: bool,
}

/// Runtime-owned temporary path attached only to file-backed request bodies.
#[derive(Clone, Debug)]
pub(super) struct RequestBodyFilePath(pub(super) String);

/// Boxed body type used by the Hyper development server.
#[cfg(test)]
pub(super) type ServeBody = BoxBody<Bytes, Infallible>;

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
pub(super) struct ReloadSseBody {
    pub(super) initial_frame_pending: bool,
    pub(super) receiver: Mutex<std_mpsc::Receiver<u64>>,
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
pub(super) const RELOAD_ENDPOINT: &str = "/__terlan/reload";

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
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) struct DirectoryServerHandle {
    pub(crate) local_addr: String,
}

pub(super) fn run_parsed(mut args: ServeArgs) -> ExitCode {
    if let Err(message) = validate_web_package(&args.web_root) {
        eprintln!("{message}");
        return ExitCode::from(1);
    }
    if let Err(message) = validate_handler_runtime_contract(&args) {
        eprintln!("{message}");
        return ExitCode::from(1);
    }
    let effective_config = match config::resolve_effective_serve_config(&args) {
        Ok(config) => config,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(1);
        }
    };
    args.host.clone_from(&effective_config.host);
    args.port = effective_config.port;
    args.poll_ms = effective_config.poll_ms;
    args.max_body_bytes = effective_config.max_body_bytes;
    if let Err(message) = config::write_effective_serve_config(&effective_config, &args.web_root) {
        eprintln!("{message}");
        return ExitCode::from(1);
    }
    let telemetry_capacity = usize::try_from(effective_config.queue_capacity)
        .unwrap_or(usize::MAX)
        .min(65_536);
    let mut observability =
        match observability::VmServeObservability::new(&effective_config, telemetry_capacity) {
            Ok(observability) => observability,
            Err(message) => {
                eprintln!("{message}");
                return ExitCode::from(1);
            }
        };
    observability.record(
        observability::VmEventDomain::Process,
        "process.startup",
        observability::VmEventStatus::Started,
    );
    let startup_trace = match std::env::var("TRACEPARENT") {
        Ok(value) => match observability::parse_traceparent(&value) {
            Ok(trace) => Some(trace),
            Err(message) => {
                eprintln!("{message}");
                return ExitCode::from(1);
            }
        },
        Err(std::env::VarError::NotPresent) => None,
        Err(error) => {
            eprintln!("error[vm.observability.traceparent]: cannot read TRACEPARENT: {error}");
            return ExitCode::from(1);
        }
    };
    observability.record_correlated(
        observability::VmEventDomain::Process,
        "config.validated",
        observability::VmEventStatus::Completed,
        observability::VmEventCorrelation {
            trace: startup_trace.as_ref(),
            ..observability::VmEventCorrelation::default()
        },
    );
    observability.record(
        observability::VmEventDomain::Process,
        "process.readiness",
        observability::VmEventStatus::Ready,
    );
    if let Err(message) = observability.flush(&args.web_root) {
        eprintln!("{message}");
        return ExitCode::from(1);
    }
    #[cfg(any(test, not(feature = "serve-runtime-bin")))]
    if std::env::var_os("TERLAN_SERVE_COMPILER_DAEMON").is_some() {
        return handler_cache::run_compiler_daemon(&args.web_root);
    }
    if args.check_only {
        observability.begin_shutdown("check-complete");
        observability.finish_shutdown(false);
        if let Err(message) = observability.flush(&args.web_root) {
            eprintln!("{message}");
            return ExitCode::from(1);
        }
        return ExitCode::SUCCESS;
    }
    #[cfg(any(test, not(feature = "serve-runtime-bin")))]
    match enter_lean_serve_runtime() {
        Ok(None) => {}
        Ok(Some(exit_code)) => return exit_code,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(1);
        }
    }
    #[cfg(all(feature = "serve-runtime-bin", not(test)))]
    return {
        observability.record(
            observability::VmEventDomain::Socket,
            "socket.bind",
            observability::VmEventStatus::Started,
        );
        let outcome = match serve_web_package(&args) {
            Ok(()) => {
                observability.record(
                    observability::VmEventDomain::Socket,
                    "socket.serve",
                    observability::VmEventStatus::Completed,
                );
                ExitCode::SUCCESS
            }
            Err(message) => {
                observability.record(
                    observability::VmEventDomain::Socket,
                    "socket.serve",
                    observability::VmEventStatus::Failed,
                );
                eprintln!("{message}");
                ExitCode::from(1)
            }
        };
        observability.begin_shutdown("runtime-return");
        observability.finish_shutdown(false);
        if let Err(message) = observability.flush(&args.web_root) {
            eprintln!("{message}");
            return ExitCode::from(1);
        }
        outcome
    };
    #[cfg(any(test, not(feature = "serve-runtime-bin")))]
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

    #[cfg(any(test, not(feature = "serve-runtime-bin")))]
    observability.record(
        observability::VmEventDomain::Socket,
        "socket.bind",
        observability::VmEventStatus::Started,
    );
    #[cfg(any(test, not(feature = "serve-runtime-bin")))]
    let outcome = match serve_web_package(&args) {
        Ok(()) => {
            observability.record(
                observability::VmEventDomain::Socket,
                "socket.serve",
                observability::VmEventStatus::Completed,
            );
            ExitCode::SUCCESS
        }
        Err(message) => {
            observability.record(
                observability::VmEventDomain::Socket,
                "socket.serve",
                observability::VmEventStatus::Failed,
            );
            eprintln!("{message}");
            ExitCode::from(1)
        }
    };
    #[cfg(any(test, not(feature = "serve-runtime-bin")))]
    observability.begin_shutdown("runtime-return");
    #[cfg(any(test, not(feature = "serve-runtime-bin")))]
    observability.finish_shutdown(false);
    #[cfg(any(test, not(feature = "serve-runtime-bin")))]
    if let Err(message) = observability.flush(&args.web_root) {
        eprintln!("{message}");
        return ExitCode::from(1);
    }
    #[cfg(any(test, not(feature = "serve-runtime-bin")))]
    dev_dependencies::finish_dependency_session(dependency_session, outcome)
}

/// Replaces the compiler-bearing process image after persisted AOT admission.
///
/// The replacement process loads only persisted generation metadata during
/// steady-state serving. Source changes are compiled by short-lived helpers,
/// preventing compiler stacks and allocator arenas from becoming runtime RSS.
#[cfg(test)]
pub(super) fn enter_lean_serve_runtime() -> Result<Option<ExitCode>, String> {
    Ok(None)
}

#[cfg(all(not(test), not(feature = "serve-runtime-bin")))]
pub(super) fn enter_lean_serve_runtime() -> Result<Option<ExitCode>, String> {
    const RUNTIME_ONLY_ENV: &str = "TERLAN_SERVE_RUNTIME_ONLY";
    if std::env::var_os(RUNTIME_ONLY_ENV).is_some() {
        return Ok(None);
    }
    let compiler = std::env::current_exe()
        .map_err(|error| format!("error[serve.runtime_exec]: current executable: {error}"))?;
    let runtime_name = if cfg!(windows) {
        "terlan-serve-runtime.exe"
    } else {
        "terlan-serve-runtime"
    };
    let runtime = std::env::var_os("TERLAN_SERVE_RUNTIME_BIN")
        .map(PathBuf::from)
        .or_else(|| compiler.parent().map(|parent| parent.join(runtime_name)))
        .filter(|path| path.is_file())
        .unwrap_or_else(|| compiler.clone());
    let mut command = std::process::Command::new(runtime);
    command
        .env(RUNTIME_ONLY_ENV, "1")
        .env("TERLAN_COMPILER", compiler)
        .args(std::env::args_os().skip(1));
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let error = command.exec();
        Err(format!(
            "error[serve.runtime_exec]: replace compiler process: {error}"
        ))
    }
    #[cfg(not(unix))]
    {
        let status = command
            .status()
            .map_err(|error| format!("error[serve.runtime_exec]: start runtime: {error}"))?;
        Ok(Some(
            status
                .code()
                .and_then(|code| u8::try_from(code).ok())
                .map_or_else(|| ExitCode::from(1), ExitCode::from),
        ))
    }
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
pub(super) fn validate_handler_runtime_contract(args: &ServeArgs) -> Result<(), String> {
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
pub(super) fn validate_dynamic_handler_sources(web_root: &Path) -> Result<(), String> {
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
pub(super) fn validate_dynamic_handler_source_path(
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
pub(crate) fn prewarm_dynamic_handler_sources(web_root: &Path) -> Result<(), String> {
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
#[cfg(test)]
pub(super) fn execute_dynamic_vm_handler(
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
pub(super) fn execute_dynamic_vm_handler_with_runtime(
    vm: &AotHandlerRuntime,
    web_root: &Path,
    matched: &MatchedWebPackageHandler,
    request: crate::terlan_native::http::Request,
    projection: crate::runtime::native::http::RequestFieldProjection,
) -> Result<handler::HandlerResponse, String> {
    let mut output = crate::service_foundation::emit_program_output;
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

/// Diverts only compiler-proven suspendable direct handlers into the
/// protocol-owner invocation state machine. Immediate handlers retain their
/// typed response and scalar-ingress fast paths.
pub(super) async fn handle_suspendable_vm_stream_request(
    request: &::http::Request<String>,
    web_root: &Path,
) -> Result<Option<::http::Response<Bytes>>, String> {
    let method = request.method().as_str();
    let request_path = request.uri().path();
    let epoch = handler_cache_epoch();
    if SUSPENDABLE_ROUTE_DECISION.with(|decision| {
        decision.borrow().as_ref().is_some_and(|decision| {
            decision.epoch == epoch
                && decision.web_root == web_root
                && decision.method == method
                && decision.path == request_path
                && !decision.suspending
        })
    }) {
        return Ok(None);
    }
    let request_query = request.uri().query().unwrap_or_default();
    let Some(MatchedWebPackageRoute::Handler(matched)) =
        manifest_route_for_request(web_root, method, request_path)
    else {
        return Ok(None);
    };
    let runtime = cached_vm_handler_runtime_for_request(web_root, &matched.handler)?;
    let suspending = runtime.vm().direct_request_handler_may_suspend(
        &matched.handler.module,
        &matched.handler.function,
        matched.handler.arity,
    );
    SUSPENDABLE_ROUTE_DECISION.with(|decision| {
        *decision.borrow_mut() = Some(SuspendableRouteDecision {
            epoch,
            web_root: web_root.to_path_buf(),
            method: method.to_owned(),
            path: request_path.to_owned(),
            suspending,
        });
    });
    if !suspending {
        return Ok(None);
    }
    if !runtime.vm().direct_router_handler_is_safe(
        method,
        request_path,
        &matched.handler.module,
        &matched.handler.function,
        matched.handler.arity,
    ) {
        return Err(
            "error[serve.aot.async_router]: suspendable handlers with router middleware or recovery are not yet supported"
                .into(),
        );
    }
    let projection = runtime.vm().direct_request_projection(
        &matched.handler.module,
        &matched.handler.function,
        matched.handler.arity,
    );
    let projected_headers =
        if projection.requires(crate::runtime::native::http::RequestFieldProjection::HEADERS) {
            request_header_pairs(request.headers())
        } else {
            Default::default()
        };
    let projected_cookies = if projection
        .requires(crate::runtime::native::http::RequestFieldProjection::COOKIES)
        || projection.requires(crate::runtime::native::http::RequestFieldProjection::COOKIE_JAR)
    {
        request_cookie_pairs(request.headers())
    } else {
        Default::default()
    };
    let native_request = crate::terlan_native::http::Request::from_parts_with_raw_query_metadata(
        if projection.requires(crate::runtime::native::http::RequestFieldProjection::METHOD) {
            method.to_owned()
        } else {
            Default::default()
        },
        if projection.requires(crate::runtime::native::http::RequestFieldProjection::PATH) {
            request_path.to_owned()
        } else {
            Default::default()
        },
        if projection.requires(crate::runtime::native::http::RequestFieldProjection::BODY) {
            request.body().clone()
        } else {
            Default::default()
        },
        crate::terlan_native::http::RequestMetadata {
            params: if projection
                .requires(crate::runtime::native::http::RequestFieldProjection::PARAMS)
            {
                matched.params.clone()
            } else {
                Default::default()
            },
            query_string: if projection
                .requires(crate::runtime::native::http::RequestFieldProjection::QUERY_STRING)
            {
                request_query.to_owned()
            } else {
                Default::default()
            },
            query: if projection
                .requires(crate::runtime::native::http::RequestFieldProjection::QUERY)
            {
                query_pairs(request_query)
            } else {
                Default::default()
            },
            headers: projected_headers,
            cookies: projected_cookies,
        },
    )
    .with_body_file_path(
        if projection.requires(crate::runtime::native::http::RequestFieldProjection::BODY_FILE_PATH)
        {
            request
                .extensions()
                .get::<RequestBodyFilePath>()
                .map(|path| path.0.clone())
                .unwrap_or_default()
        } else {
            String::new()
        },
    );
    let response = execute_suspendable_vm_handler_with_package_root_projected(
        runtime.vm(),
        &matched,
        native_request,
        projection,
        web_root,
    )
    .await?;
    serve_vm_stream_handler_response(response, method == "HEAD").map(Some)
}

/// Returns whether an exact dynamic handler observes only the file-backed body
/// projection. Complete or mixed projections retain the text boundary.
pub(super) fn request_requires_file_body(
    web_root: &Path,
    method: &str,
    path: &str,
) -> Result<bool, String> {
    let Some(MatchedWebPackageRoute::Handler(matched)) =
        manifest_route_for_request(web_root, method, path)
    else {
        return Ok(false);
    };
    let runtime = cached_vm_handler_runtime_for_request(web_root, &matched.handler)?;
    if !runtime.vm().direct_router_handler_is_safe(
        method,
        path,
        &matched.handler.module,
        &matched.handler.function,
        matched.handler.arity,
    ) {
        return Ok(false);
    }
    let projection = runtime.vm().direct_request_projection(
        &matched.handler.module,
        &matched.handler.function,
        matched.handler.arity,
    );
    Ok(matches!(
        projection,
        crate::runtime::native::http::RequestFieldProjection::Fields(_)
    ) && projection
        .requires(crate::runtime::native::http::RequestFieldProjection::BODY_FILE_PATH)
        && !projection.requires(crate::runtime::native::http::RequestFieldProjection::BODY))
}

/// Executes middleware for a compiler-folded static route through its source router.
pub(super) fn execute_static_vm_router(
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
    let mut output = crate::service_foundation::emit_program_output;
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
pub(super) fn execute_websocket_vm_router(
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
    let mut output = crate::service_foundation::emit_program_output;
    execute_vm_router_websocket_admission_with_package_root(
        runtime,
        websocket,
        request,
        web_root,
        &mut output,
    )
}

/// Executes source-router middleware before an SSE stream is admitted.
pub(super) fn execute_sse_vm_router(
    web_root: &Path,
    endpoint: &WebPackageSse,
    request: &crate::terlan_native::http::Request,
) -> Result<VmSseRouterAdmission, String> {
    let handler = sse_router_handler(endpoint);
    let project_root = manifest::adjacent_project_root(web_root).ok_or_else(|| {
        "error[serve_runtime]: SSE router owner requires an adjacent project root".to_string()
    })?;
    let runtime = cached_vm_handler_for_manifest(web_root, &project_root, &handler)?;
    let mut output = crate::service_foundation::emit_program_output;
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
pub(super) fn source_path_from_manifest(
    project_root: &Path,
    manifest_path: &str,
) -> Option<PathBuf> {
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
pub(super) fn serve_web_package(args: &ServeArgs) -> Result<(), String> {
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
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
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
        if let Err(message) = serve_bound_directory_vm_plain(
            listener,
            web_root,
            poll_ms,
            args::DEFAULT_MAX_BODY_BYTES,
            log_prefix,
        ) {
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
