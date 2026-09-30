//! Admit executed package descriptors; never interpret router builder syntax.
use super::AotHandlerRuntime;
use crate::runtime::vm::http_router::{VmHttpRouteMethod, VmHttpRouteTarget, VmHttpRouter};
use terlan_http_native::source_descriptor::{self, RouteTarget};
use terlan_runtime_abi::NativeAdapterError;

#[cfg(test)]
#[path = "source_router_test.rs"]
mod tests;

impl AotHandlerRuntime {
    pub(super) fn admit_source_router(mut self) -> Result<Self, String> {
        let export = format!("{}.router", self.module);
        if !self.generation.has_export(&export, 0) {
            return Ok(self);
        }
        // Evaluate once per immutable image generation. The returned closures own
        // their captures; no startup actor or mutable execution shard is retained.
        let value = self.generation.image.spawn_shard()?.call(&export, &[])?;
        let plan = source_descriptor::router(&value, |callback, arity| {
            self.validate_callable(&self.module, callback, arity)
                .map_err(|error| NativeAdapterError::new("http.callback", error.to_string(), 0))?;
            Ok(callback.clone())
        })
        .map_err(|error| format!("error[{}]: {}", error.code(), error.message()))?;
        if plan.lifecycle.is_some() || plan.overload.is_some() {
            return Err("error[serve.aot.router]: lifecycle and overload admission are not implemented for source routers".into());
        }
        let mut router = VmHttpRouter::new();
        for middleware in plan.middleware {
            router = router.use_middleware(middleware);
        }
        for middleware in plan.response_middleware {
            router = router.map_response(middleware);
        }
        for route in plan.routes {
            let method = VmHttpRouteMethod::from_name(&route.method).ok_or_else(|| {
                format!(
                    "error[serve.aot.router]: unsupported method `{}`",
                    route.method
                )
            })?;
            let target = match route.target {
                RouteTarget::Handler(handler) => VmHttpRouteTarget::Handler(handler),
                RouteTarget::Sse(plan) => VmHttpRouteTarget::SseEndpoint(plan),
                RouteTarget::WebSocket(plan) => VmHttpRouteTarget::WebSocketEndpoint(plan),
            };
            router = router.scoped_target(
                method,
                route.path,
                target,
                route.middleware,
                route.response_middleware,
            )?;
        }
        if let Some(fallback) = plan.fallback {
            router = router.fallback(fallback);
        }
        if let Some(error) = plan.error {
            router = router.error(error);
        }
        self.router = Some(router);
        Ok(self)
    }
}
