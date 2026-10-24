//! Source execution and image authority for package-owned async channel admission.

use super::*;
use std::sync::Arc;
use terlan_http_native::channel_admission::{self, SourceAdmission};

pub(in crate::commands::serve) async fn execute_suspendable_sse(
    vm: Arc<AotHandlerRuntime>,
    endpoint: &WebPackageSse,
    request: &native_http::Request,
    package_root: &Path,
) -> Result<VmSseRouterAdmission, String> {
    let router = vm.execute_http_router(&endpoint.module, "router", &mut |_| {})?;
    let host = RouterResponseRuntime::new(&vm, &endpoint.module, request, package_root);
    match channel_admission::sse_suspendable(&router, &endpoint.route, request.path(), &host)
        .await?
    {
        SourceAdmission::Respond(response) => Ok(VmSseRouterAdmission::Respond(response)),
        SourceAdmission::Open(plan) => {
            if let Some(response) =
                channel_admission::sse_transport_response(&plan, request.method(), false)
            {
                return Ok(VmSseRouterAdmission::Respond(response));
            }
            let session = native_http::sse_session::SseSession::open(plan);
            sse_invocation::open(vm, endpoint.module.clone(), session)
                .map(|session| VmSseRouterAdmission::Stream(Box::new(session)))
        }
    }
}

pub(in crate::commands::serve) async fn execute_suspendable_websocket(
    vm: Arc<AotHandlerRuntime>,
    endpoint: &WebPackageWebSocket,
    request: &native_http::Request,
    package_root: &Path,
) -> Result<Option<VmWebSocketRouterAdmission>, String> {
    if !vm.has_function(&endpoint.module, "router", 0) {
        return Ok(None);
    }
    let router = vm.execute_http_router(&endpoint.module, "router", &mut |_| {})?;
    let host = RouterResponseRuntime::new(&vm, &endpoint.module, request, package_root);
    match channel_admission::websocket_suspendable(&router, &endpoint.route, request.path(), &host)
        .await?
    {
        SourceAdmission::Respond(response) => {
            Ok(Some(VmWebSocketRouterAdmission::Respond(response)))
        }
        SourceAdmission::Open(plan) => {
            let session = native_http::websocket::session::Session::open(plan);
            websocket_invocation::open_suspendable(vm, endpoint.module.clone(), session)
                .await
                .map(|session| Some(VmWebSocketRouterAdmission::Upgrade(Box::new(session))))
        }
    }
}
