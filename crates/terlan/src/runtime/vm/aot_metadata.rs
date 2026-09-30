//! Compiler-independent metadata admitted beside persisted AOT images.

#[cfg(test)]
#[path = "aot_metadata_test.rs"]
mod tests;

use crate::runtime::native::http::RequestFieldProjection;
#[cfg(test)]
use crate::runtime::vm::native_callable::VmNativeCallableRef;
#[cfg(test)]
use terlan_http_native::channel_plan::{SseEndpointPlan, WebSocketEndpointPlan};

/// One statically resolved callable retained by an AOT router plan.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[cfg(test)]
pub(crate) struct AotRouterCallable {
    pub(crate) module: String,
    pub(crate) function: String,
    pub(crate) arity: usize,
}

/// One method/path route and its statically resolved native callback.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[cfg(test)]
pub(crate) struct AotRouterRoute {
    pub(crate) method: String,
    pub(crate) path: String,
    pub(crate) target: AotRouterRouteTarget,
    pub(crate) middleware: Vec<AotRouterCallable>,
    pub(crate) response_middleware: Vec<AotRouterCallable>,
}

/// Canonical executable target retained by one AOT router route.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[cfg(test)]
pub(crate) enum AotRouterRouteTarget {
    Handler(AotRouterCallable),
    Sse(SseEndpointPlan<VmNativeCallableRef>),
    WebSocket(WebSocketEndpointPlan<VmNativeCallableRef>),
}

/// Closure-free router metadata extracted from checked CoreIR.
#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[cfg(test)]
pub(crate) struct AotRouterPlan {
    pub(crate) module: String,
    pub(crate) routes: Vec<AotRouterRoute>,
    pub(crate) middleware: Vec<AotRouterCallable>,
    pub(crate) response_middleware: Vec<AotRouterCallable>,
    pub(crate) fallback: Option<AotRouterCallable>,
    pub(crate) error: Option<AotRouterCallable>,
}

/// Export-specific opaque Request projection carried beside a compiled image.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub(crate) struct NativeRequestProjection {
    pub(crate) module: String,
    pub(crate) function: String,
    pub(crate) arity: usize,
    pub(crate) fields: RequestFieldProjection,
    #[serde(default)]
    pub(crate) scalar_entry: Option<String>,
    #[serde(default)]
    pub(crate) scalar_field: Option<usize>,
    #[serde(default)]
    pub(crate) suspending: bool,
}
