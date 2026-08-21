//! Compiler-independent metadata admitted beside persisted AOT images.

use crate::runtime::native::http::RequestFieldProjection;
use crate::runtime::vm::sse::VmSseEndpointPlan;
use crate::runtime::vm::websocket::VmWebSocketEndpointPlan;

/// One statically resolved callable retained by an AOT router plan.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub(crate) struct AotRouterCallable {
    pub(crate) module: String,
    pub(crate) function: String,
    pub(crate) arity: usize,
}

/// One method/path route and its statically resolved native callback.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub(crate) struct AotRouterRoute {
    pub(crate) method: String,
    pub(crate) path: String,
    pub(crate) target: AotRouterRouteTarget,
    pub(crate) middleware: Vec<AotRouterCallable>,
    pub(crate) response_middleware: Vec<AotRouterCallable>,
}

/// Canonical executable target retained by one AOT router route.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub(crate) enum AotRouterRouteTarget {
    Handler(AotRouterCallable),
    Sse(VmSseEndpointPlan),
    WebSocket(VmWebSocketEndpointPlan),
}

/// Closure-free router metadata extracted from checked CoreIR.
#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
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
