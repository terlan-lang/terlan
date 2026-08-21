mod args;
#[cfg(test)]
mod channel_transport;
mod config;
mod handler;
mod handler_cache;
mod handler_sources;
mod hyper_server;
#[cfg(test)]
mod logging;
mod manifest;
mod observability;
mod response;
mod tls;
pub(crate) mod tls_contract;
mod watch;
mod websocket;

use std::cell::RefCell;
use std::fs;
use std::net as std_net;
use std::path::{Component, Path, PathBuf};
#[cfg(test)]
use std::pin::Pin;
use std::process::ExitCode;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex};
#[cfg(test)]
use std::task::{Context, Poll};
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
use std::thread;
#[cfg(test)]
use std::time::Instant;

use bytes::Bytes;
#[cfg(test)]
use http_body_util::{combinators::BoxBody, BodyExt, Full};
#[cfg(test)]
use hyper::body::Frame;
#[cfg(test)]
use hyper::{Request, Response};
#[cfg(test)]
use std::convert::Infallible;

#[cfg(any(test, not(feature = "serve-runtime-bin")))]
use crate::commands::dev_dependencies;
#[cfg(test)]
use crate::runtime::vm::http::{handle_http1_in_memory_exchange, write_http1_response};
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
use crate::{CliCommand, CliState};

/// Typed failure returned by VM serve configuration and lifecycle helpers.
pub(super) struct ServeError(terlan_runtime_abi::BoundaryError);

impl ServeError {
    fn message(rendered: impl Into<String>) -> Self {
        Self(terlan_runtime_abi::BoundaryError::message(
            terlan_runtime_abi::ErrorDomain::CommandExecution,
            "run VM serve operation",
            rendered,
        ))
    }
}

impl std::fmt::Debug for ServeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

impl std::fmt::Display for ServeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

impl std::error::Error for ServeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        std::error::Error::source(&self.0)
    }
}

impl std::ops::Deref for ServeError {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.0.context()
    }
}

impl From<String> for ServeError {
    fn from(rendered: String) -> Self {
        Self::message(rendered)
    }
}

impl From<&str> for ServeError {
    fn from(rendered: &str) -> Self {
        rendered.to_owned().into()
    }
}

impl From<terlan_runtime_abi::BoundaryError> for ServeError {
    fn from(error: terlan_runtime_abi::BoundaryError) -> Self {
        Self(error)
    }
}

impl From<ServeError> for String {
    fn from(error: ServeError) -> Self {
        error.to_string()
    }
}

pub(super) type ServeResult<T> = Result<T, ServeError>;

use crate::terlan_native::http::content_type_for_path;
#[cfg(test)]
use handler::handler_log_identity;
use handler::{
    execute_suspendable_vm_handler_with_package_root_projected,
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
    cached_vm_handler_runtime_for_request, handler_cache_epoch,
    with_cached_vm_handler_runtime_for_request, AotHandlerRuntime,
};
#[cfg(test)]
use logging::{
    log_file_route_result, log_handler_result, log_static_result, log_static_route_result,
    next_request_id, render_dev_error_page, RouteLogEvent,
};
use manifest::manifest_build_id;
#[cfg(test)]
use manifest::manifest_static_file_for_request;
pub(crate) use manifest::validate_web_package;
#[cfg(test)]
use response::build_http_response;
use response::{
    build_http_response_for_stream, build_http_response_owned_for_stream,
    build_http_shared_response_owned_for_stream, build_http_text_response_owned_for_stream,
    inject_reload_script,
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

#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) use args::parse_serve_args;
pub(crate) use args::ServeArgs;

mod request_dispatch;
mod response_rendering;
mod server_lifecycle;

use request_dispatch::handle_vm_stream_request;
#[cfg(test)]
use request_dispatch::{handle_vm_stream_http1_exchange, VmStreamHttp1Exchange};
use response_rendering::package_relative_path;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) use server_lifecycle::prewarm_dynamic_handler_sources;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) use server_lifecycle::run;
pub(crate) use server_lifecycle::run_serve_runtime;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) use server_lifecycle::spawn_directory_server;
#[cfg(test)]
use server_lifecycle::ServeBody;
use server_lifecycle::{source_path_from_manifest, RELOAD_ENDPOINT};

#[cfg(test)]
#[path = "serve_test.rs"]
#[cfg(test)]
mod serve_test;
