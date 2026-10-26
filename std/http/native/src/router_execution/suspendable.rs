//! Suspendable source routing over the host's existing executor and authority.

use super::*;
use crate::source_descriptor::HandlerPipeline;
use std::future::Future;
use terlan_runtime_abi::NativeAdapterError;

/// Invokes source code without supplying an HTTP-specific executor or scheduler.
pub trait AsyncHost {
    type Value: DescriptorValue + Clone;
    type Response;

    fn invoke(
        &self,
        callable: Self::Value,
        args: Vec<Self::Value>,
    ) -> impl Future<Output = Result<Self::Value, String>>;
    fn arity(&self, callable: &Self::Value) -> Option<usize>;
    fn request(&self, params: &[(String, String)]) -> Self::Value;
    fn scalar(value: NativeValue) -> Self::Value;
    fn admit(&self, value: Self::Value) -> Result<Self::Response, String>;
}

/// Executes an async source route with lazy handler admission and bounded recovery.
/// Dropping the future cancels its pending host call without starting later stages.
pub async fn execute_suspendable<H: AsyncHost>(
    router: &Router<H::Value>,
    method: RouteMethod,
    declared_route: &str,
    path: &str,
    host: &H,
) -> Result<H::Response, WebRouteError> {
    let RouterOutcome::Matched(dispatch) = router.dispatch(method, path)? else {
        return Err("error[serve_router]: source router did not match request".into());
    };
    let pipeline = HandlerPipeline {
        middleware: &dispatch.middleware,
        response_middleware: &dispatch.response_middleware,
        recovery: router.error_handler(),
    };
    let request = host.request(&dispatch.route_params);
    let prepare_handler = || {
        if dispatch.method != method || dispatch.route_pattern != declared_route {
            return Err(failure(
                "serve_router",
                "manifest and source router routes disagree".into(),
            ));
        }
        let RouteTarget::Handler(handler) = &dispatch.target else {
            return Err(failure(
                "serve_router",
                "expected source HTTP handler".into(),
            ));
        };
        let args = handler_arguments(request.clone(), &dispatch, host.arity(handler), H::scalar)
            .map_err(|error| failure("serve_router", error.into()))?;
        Ok((handler.clone(), args))
    };
    let mut invoke = |callback, args| async move {
        host.invoke(callback, args)
            .await
            .map_err(|error| failure("http.callback", error))
    };
    let error_value = |error: &NativeAdapterError| H::scalar(error.message().into());
    let response = pipeline
        .execute(request.clone(), prepare_handler, &mut invoke, error_value)
        .await
        .map_err(diagnostic)?;
    match host.admit(response) {
        Ok(response) => Ok(response),
        Err(error) => {
            let recovered = pipeline
                .recover(failure("http.response", error), &mut invoke, &error_value)
                .await
                .map_err(diagnostic)?;
            // Invalid recovery output is terminal, never recursively recovered.
            host.admit(recovered).map_err(WebRouteError::from)
        }
    }
}

fn failure(code: &'static str, message: String) -> NativeAdapterError {
    NativeAdapterError::new(code, message, 0)
}

fn diagnostic(error: NativeAdapterError) -> String {
    format!("error[{}]: {}", error.code(), error.message())
}

/// Unwinds a channel's short-circuit response using the ordinary source callback
/// executor. A failed stage is terminal and is never replayed.
pub async fn finish_suspendable<H: AsyncHost>(
    mut response: H::Value,
    params: &[(String, String)],
    middleware: &[H::Value],
    host: &H,
) -> Result<H::Response, WebRouteError> {
    let request = host.request(params);
    for callback in middleware.iter().rev() {
        response = host
            .invoke(callback.clone(), vec![request.clone(), response])
            .await?;
        validate_response_middleware_result(&response).map_err(String::from)?;
    }
    host.admit(response).map_err(WebRouteError::from)
}

#[cfg(test)]
#[path = "suspendable_test.rs"]
mod tests;
