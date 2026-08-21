use super::*;

/// Binds a standard TCP listener for VM stream serving.
pub(super) fn bind_std_listener(host: &str, port: u16) -> Result<std_net::TcpListener, String> {
    crate::runtime::vm::protocol_task_executor::bind_protocol_listener(host, port)
}

/// Serves one plain HTTP web package through VM-owned HTTP stream handling.
pub(super) fn serve_web_package_vm_plain(args: &ServeArgs) -> Result<(), String> {
    let listener = bind_std_listener(&args.host, args.port)?;
    serve_bound_directory_vm_plain(
        listener,
        args.web_root.clone(),
        args.poll_ms,
        args.max_body_bytes,
        "terlc serve",
    )
}

/// Serves one TLS web package through VM HTTP streams over maintained rustls.
pub(super) fn serve_web_package_vm_tls(
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
        args.max_body_bytes,
        "terlc serve",
        Some(tls_config),
    )
}

/// Serves one bound directory listener through plain VM HTTP streams.
pub(super) fn serve_bound_directory_vm_plain(
    listener: std_net::TcpListener,
    web_root: PathBuf,
    poll_ms: u64,
    max_body_bytes: u64,
    log_prefix: &str,
) -> Result<(), String> {
    serve_bound_directory_vm_stream(
        listener,
        web_root,
        poll_ms,
        max_body_bytes,
        log_prefix,
        None,
    )
}

/// Serves one bound directory listener through VM HTTP streams.
pub(super) fn serve_bound_directory_vm_stream(
    listener: std_net::TcpListener,
    web_root: PathBuf,
    poll_ms: u64,
    max_body_bytes: u64,
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
        return hyper_server::serve_tls(
            listener,
            web_root,
            tls_config.server_config,
            max_body_bytes,
        );
    }
    hyper_server::serve(listener, web_root, max_body_bytes)
}
