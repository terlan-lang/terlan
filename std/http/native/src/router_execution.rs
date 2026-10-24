//! Source-router execution policy over opaque host values and callable authority.

use terlan_runtime_abi::{DescriptorValue, NativeValue};

use crate::routing::{
    validate_response_middleware_result, RouteDispatch, RouteMethod, RouteTarget, Router,
    RouterOutcome,
};

mod suspendable;
pub use suspendable::{execute_suspendable, finish_suspendable, AsyncHost};

/// The host executes code and supplies authorized response resources, not routing policy.
pub trait Host {
    type Value: DescriptorValue + Clone;
    type Response;

    fn invoke(
        &mut self,
        callable: &Self::Value,
        args: Vec<Self::Value>,
    ) -> Result<Self::Value, String>;
    fn arity(&self, callable: &Self::Value) -> Option<usize>;
    fn request(&self, params: &[(String, String)]) -> Self::Value;
    fn scalar(value: NativeValue) -> Self::Value;
    fn admit(&mut self, value: Self::Value) -> Result<Self::Response, String>;
}

/// A folded response still bound to its original source route and middleware.
pub struct PreparedResponse<V> {
    pub method: RouteMethod,
    pub route_pattern: String,
    pub value: V,
}

/// Builds the same typed capture arguments for synchronous and suspendable handlers.
pub fn handler_arguments<V>(
    request: V,
    dispatch: &RouteDispatch<V>,
    arity: Option<usize>,
    scalar: impl Fn(NativeValue) -> V,
) -> Result<Vec<V>, String> {
    let arity = callable_arity(arity)?;
    let mut args = vec![request];
    if arity > 1 {
        for (name, value) in &dispatch.route_params {
            args.push(scalar(
                crate::route_pattern::route_param_argument(&dispatch.route_pattern, name, value)
                    .map_err(String::from)?,
            ));
        }
    }
    if args.len() != arity {
        return Err(format!(
            "error[serve_router]: route {} {} handler expects {arity} argument(s), found {}",
            dispatch.method.as_str(),
            dispatch.path,
            args.len()
        ));
    }
    Ok(args)
}

fn callable_arity(arity: Option<usize>) -> Result<usize, String> {
    arity.ok_or_else(|| "error[serve_router]: matched route target is not callable".into())
}

/// Runs admitted source middleware, route binding, recovery, and response admission.
/// Middleware rejection precedes route-binding checks; recovery never replays a
/// failed response chain. Prepared responses cannot bypass their route identity.
pub fn execute<H: Host>(
    router: &Router<H::Value>,
    method: RouteMethod,
    declared_route: &str,
    path: &str,
    initial_params: &[(String, String)],
    prepared: Option<PreparedResponse<H::Value>>,
    host: &mut H,
) -> Result<H::Response, String> {
    let request = host.request(initial_params);
    let outcome = match router.dispatch_with_typed_middleware(method, path, |callback, _| {
        host.invoke(callback, vec![request.clone()])
    }) {
        Ok(outcome) => outcome,
        Err(error) => {
            let response = recover(
                router,
                error.into(),
                |callable, args| host.invoke(callable, args),
                H::scalar,
            )?;
            return finish(response, &[], &[], host);
        }
    };
    let (response, params, middleware) = match outcome {
        RouterOutcome::ShortCircuited(short) => (
            short.response,
            short.route_params,
            short.response_middleware,
        ),
        RouterOutcome::Matched(dispatch) => {
            if prepared.is_none()
                && (dispatch.method != method || dispatch.route_pattern != declared_route)
            {
                return Err(format!(
                    "error[serve_router]: manifest route `{}` `{declared_route}` does not match materialized route `{}` `{}`",
                    method.as_str(), dispatch.method.as_str(), dispatch.route_pattern
                ));
            }
            let RouteTarget::Handler(handler) = &dispatch.target else {
                return Err(format!(
                    "error[serve_router]: route {} {} did not resolve to a source handler",
                    method.as_str(),
                    dispatch.path
                ));
            };
            let arity = callable_arity(host.arity(handler))?;
            let response = if let Some(prepared) = prepared {
                if dispatch.method != prepared.method
                    || dispatch.route_pattern != prepared.route_pattern
                {
                    return Err(format!(
                        "error[serve_router]: folded static route `{}` `{}` does not match materialized route `{}` `{}`",
                        prepared.method.as_str(), prepared.route_pattern,
                        dispatch.method.as_str(), dispatch.route_pattern
                    ));
                }
                prepared.value
            } else {
                let args = handler_arguments(
                    host.request(&dispatch.route_params),
                    &dispatch,
                    Some(arity),
                    H::scalar,
                )?;
                match host.invoke(handler, args) {
                    Ok(response) => response,
                    Err(error) => recover(
                        router,
                        error,
                        |callable, args| host.invoke(callable, args),
                        H::scalar,
                    )?,
                }
            };
            (
                response,
                dispatch.route_params,
                dispatch.response_middleware,
            )
        }
        RouterOutcome::NotFound => {
            return Err(format!(
                "error[serve_router]: materialized router did not match {} {path}",
                method.as_str()
            ))
        }
    };
    match finish(response, &params, &middleware, host) {
        Ok(response) => Ok(response),
        Err(error) => {
            let recovered = recover(
                router,
                error,
                |callable, args| host.invoke(callable, args),
                H::scalar,
            )?;
            finish(recovered, &params, &[], host)
        }
    }
}

/// Executes recovery once and retains the original failure if recovery also fails.
pub fn recover<V: DescriptorValue + Clone>(
    router: &Router<V>,
    error: String,
    mut invoke: impl FnMut(&V, Vec<V>) -> Result<V, String>,
    scalar: impl Fn(NativeValue) -> V,
) -> Result<V, String> {
    let Some(handler) = router.error_handler() else {
        return Err(error);
    };
    invoke(handler, vec![scalar(NativeValue::String(error.clone()))])
        .map_err(|recovery| format!(
            "error[serve_router_recovery]: router failed with `{error}`; error handler failed with `{recovery}`"
        ))
}

/// Unwinds source response stages, then admits the result through host resource access.
/// Channel short-circuits use this same path without inventing a recovery handler.
pub fn finish<H: Host>(
    mut response: H::Value,
    params: &[(String, String)],
    middleware: &[H::Value],
    host: &mut H,
) -> Result<H::Response, String> {
    let request = host.request(params);
    for callback in middleware.iter().rev() {
        response = host.invoke(callback, vec![request.clone(), response])?;
        validate_response_middleware_result(&response).map_err(String::from)?;
    }
    host.admit(response)
}

#[cfg(test)]
#[path = "router_execution_test.rs"]
mod tests;

#[cfg(test)]
mod test_support;
