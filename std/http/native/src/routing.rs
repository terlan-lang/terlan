//! HTTP package routing policy, independent of transport and VM representation.

use crate::channel_plan::{SseEndpointPlan, WebSocketEndpointPlan};
use crate::route_pattern::WebRouteError;
use crate::route_pattern::{match_route_pattern, route_ambiguity_key, validate_route_pattern};
use terlan_runtime_abi::{DescriptorValue, DescriptorView};

mod overload;
pub use overload::{OverloadConfig, OverloadPolicy};

/// HTTP method accepted by the HTTP router.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RouteMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
}

/// One admitted HTTP route.
#[derive(Clone, Debug, PartialEq)]
pub struct Route<C> {
    pub method: RouteMethod,
    pub path: String,
    pub target: RouteTarget<C>,
    pub middleware: Vec<C>,
    pub response_middleware: Vec<C>,
}

/// Target selected by HTTP route dispatch.
#[derive(Clone, Debug, PartialEq)]
pub enum RouteTarget<C> {
    Handler(C),
    SseEndpoint(SseEndpointPlan<C>),
    WebSocketEndpoint(WebSocketEndpointPlan<C>),
}

/// Source-composed fallback handler and its complete callback ordering.
#[derive(Clone, Debug, PartialEq)]
pub struct Fallback<C> {
    pub handler: C,
    pub middleware: Vec<C>,
    pub response_middleware: Vec<C>,
}

/// Result of HTTP router dispatch.
#[derive(Clone, Debug, PartialEq)]
pub enum RouterOutcome<C> {
    Matched(Box<RouteDispatch<C>>),
    ShortCircuited(RouteShortCircuit<C>),
    NotFound,
}

/// Matched route handler and ordered middleware.
#[derive(Clone, Debug, PartialEq)]
pub struct RouteDispatch<C> {
    pub method: RouteMethod,
    pub path: String,
    pub route_pattern: String,
    pub route_params: Vec<(String, String)>,
    pub target: RouteTarget<C>,
    pub middleware: Vec<C>,
    pub response_middleware: Vec<C>,
}

/// Middleware-produced response that terminates dispatch before the handler.
#[derive(Clone, Debug, PartialEq)]
pub struct RouteShortCircuit<C> {
    pub middleware: C,
    pub response: C,
    pub route_params: Vec<(String, String)>,
    pub response_middleware: Vec<C>,
}

/// Package-owned continuation over ordered route middleware.
#[derive(Clone, Debug, PartialEq)]
pub struct MiddlewareContinuation<C> {
    dispatch: RouteDispatch<C>,
    next_index: usize,
}

/// One middleware continuation step.
#[derive(Clone, Debug, PartialEq)]
pub enum MiddlewareStep<C> {
    Middleware {
        middleware: C,
        continuation: MiddlewareContinuation<C>,
    },
    Handler(RouteDispatch<C>),
}

/// Typed result returned by one source middleware invocation.
#[derive(Clone, Debug, PartialEq)]
pub enum MiddlewareResult<C> {
    Continue,
    Respond(C),
}

impl<C: DescriptorValue + Clone> MiddlewareResult<C> {
    /// Decodes the HTTP package's closed middleware result domain.
    pub fn from_value(value: C) -> Result<Self, WebRouteError> {
        crate::source_descriptor::middleware_result(value)
            .map(|response| match response {
                Some(response) => Self::Respond(response),
                None => Self::Continue,
            })
            .map_err(|_| {
                "error[vm_http_router_middleware]: expected Continue or Respond(Response)".into()
            })
    }
}

impl<C: Clone> MiddlewareContinuation<C> {
    fn new(dispatch: RouteDispatch<C>) -> Self {
        Self {
            dispatch,
            next_index: 0,
        }
    }

    pub fn step(&self) -> MiddlewareStep<C> {
        let Some(middleware) = self.dispatch.middleware.get(self.next_index) else {
            return MiddlewareStep::Handler(self.dispatch.clone());
        };
        MiddlewareStep::Middleware {
            middleware: middleware.clone(),
            continuation: Self {
                dispatch: self.dispatch.clone(),
                next_index: self.next_index + 1,
            },
        }
    }
}

/// Structured diagnostic for route registrations that would make dispatch ambiguous.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouteAmbiguityDiagnostic {
    pub method: RouteMethod,
    pub candidate_path: String,
    pub existing_path: String,
    pub normalized_shape: String,
    pub reason: RouteAmbiguityReason,
}

/// Why a route registration is ambiguous.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RouteAmbiguityReason {
    ExactPath,
    ParameterizedShape,
}

impl RouteAmbiguityDiagnostic {
    /// Keeps the existing builder API error text stable for callers.
    pub fn render_text(&self) -> String {
        format!(
            "duplicate VM HTTP route {} {}",
            self.method.as_str(),
            self.candidate_path
        )
    }
}

/// Package-owned HTTP router composition model.
///
/// Inputs:
/// - Method/path routes and fallbacks with complete source-composed callback
///   lists, plus a source-selected error handler.
///
/// Output:
/// - Deterministic dispatch outcomes that higher HTTP layers can execute
///   without depending on command-layer serve manifests or host framework state.
///
/// Transformation:
/// - Admits source-composed routes, rejects ambiguous patterns, and lets
///   middleware short-circuit before handler execution.
#[derive(Clone, Debug, PartialEq)]
pub struct Router<C> {
    routes: Vec<Route<C>>,
    fallback: Option<Fallback<C>>,
    error: Option<C>,
    overload: Option<OverloadConfig>,
}

impl<C> Default for Router<C> {
    fn default() -> Self {
        Self {
            routes: Vec::new(),
            fallback: None,
            error: None,
            overload: None,
        }
    }
}

impl<C: DescriptorValue + Clone> Router<C> {
    /// Creates an empty HTTP router.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds an exact GET route.
    pub fn get(self, path: impl Into<String>, handler: C) -> Result<Self, WebRouteError> {
        self.route(RouteMethod::Get, path, handler)
    }

    /// Adds an exact route for the selected method.
    pub fn route(
        self,
        method: RouteMethod,
        path: impl Into<String>,
        handler: C,
    ) -> Result<Self, WebRouteError> {
        self.route_target(method, path, RouteTarget::Handler(handler))
    }

    /// Adds one canonical route target with complete source-composed callback lists.
    pub fn scoped_target(
        mut self,
        method: RouteMethod,
        path: impl Into<String>,
        target: RouteTarget<C>,
        middleware: Vec<C>,
        response_middleware: Vec<C>,
    ) -> Result<Self, WebRouteError> {
        let path = path.into();
        let path = if path == "*" {
            path
        } else {
            normalize_router_path(path)?
        };
        validate_route_pattern(&path)?;
        if let Some(diagnostic) = self.route_ambiguity_diagnostic(method, &path)? {
            return Err(diagnostic.render_text().into());
        }
        self.routes.push(Route {
            method,
            path,
            target,
            middleware,
            response_middleware,
        });
        Ok(self)
    }

    fn route_target(
        self,
        method: RouteMethod,
        path: impl Into<String>,
        target: RouteTarget<C>,
    ) -> Result<Self, WebRouteError> {
        self.scoped_target(method, path, target, Vec::new(), Vec::new())
    }

    /// Adds a fallback handler for unmatched requests.
    pub fn fallback(self, handler: C) -> Self {
        self.fallback_target(Fallback {
            handler,
            middleware: Vec::new(),
            response_middleware: Vec::new(),
        })
    }

    /// Admits a fallback without synthesizing or reordering source callbacks.
    pub fn fallback_target(mut self, fallback: Fallback<C>) -> Self {
        self.fallback = Some(fallback);
        self
    }

    /// Adds an error handler used by higher layers after handler failures.
    pub fn error(mut self, handler: C) -> Self {
        self.error = Some(handler);
        self
    }

    /// Configures bounded pending HTTP work for this router.
    pub fn overload(mut self, config: OverloadConfig) -> Result<Self, WebRouteError> {
        if self.overload.is_some() {
            return Err("router overload policy is already configured".into());
        }
        self.overload = Some(config);
        Ok(self)
    }

    /// Returns the validated source-level overload configuration.
    pub fn overload_config(&self) -> Option<OverloadConfig> {
        self.overload
    }

    /// Dispatches one request without executing middleware.
    pub fn dispatch(
        &self,
        method: RouteMethod,
        path: &str,
    ) -> Result<RouterOutcome<C>, WebRouteError> {
        let path = normalize_router_path(path)?;
        if let Some((route, matched)) = self
            .routes
            .iter()
            .filter(|route| route.method == method)
            .filter_map(|route| {
                match_route_pattern(&route.path, &path).map(|matched| (route, matched))
            })
            .max_by_key(|(_, matched)| matched.score)
        {
            return Ok(RouterOutcome::Matched(Box::new(RouteDispatch {
                method,
                path,
                route_pattern: route.path.clone(),
                route_params: matched.params,
                target: route.target.clone(),
                middleware: route.middleware.clone(),
                response_middleware: route.response_middleware.clone(),
            })));
        }
        Ok(match &self.fallback {
            Some(fallback) => RouterOutcome::Matched(Box::new(RouteDispatch {
                method,
                path,
                route_pattern: "*".to_string(),
                route_params: Vec::new(),
                target: RouteTarget::Handler(fallback.handler.clone()),
                middleware: fallback.middleware.clone(),
                response_middleware: fallback.response_middleware.clone(),
            })),
            None => RouterOutcome::NotFound,
        })
    }

    /// Dispatches middleware using the source-level typed result contract.
    pub fn dispatch_with_typed_middleware(
        &self,
        method: RouteMethod,
        path: &str,
        mut invoke: impl FnMut(&C, &MiddlewareContinuation<C>) -> Result<C, String>,
    ) -> Result<RouterOutcome<C>, WebRouteError> {
        let outcome = self.dispatch(method, path)?;
        let RouterOutcome::Matched(dispatch) = outcome else {
            return Ok(outcome);
        };
        let mut continuation = MiddlewareContinuation::new(*dispatch);
        loop {
            match continuation.step() {
                MiddlewareStep::Middleware {
                    middleware,
                    continuation: next,
                } => match MiddlewareResult::from_value(invoke(&middleware, &next)?)? {
                    MiddlewareResult::Continue => continuation = next,
                    MiddlewareResult::Respond(response) => {
                        return Ok(RouterOutcome::ShortCircuited(RouteShortCircuit {
                            middleware,
                            response,
                            route_params: next.dispatch.route_params.clone(),
                            response_middleware: next.dispatch.response_middleware.clone(),
                        }));
                    }
                },
                MiddlewareStep::Handler(dispatch) => {
                    return Ok(RouterOutcome::Matched(Box::new(dispatch)));
                }
            }
        }
    }

    /// Returns the configured error handler.
    pub fn error_handler(&self) -> Option<&C> {
        self.error.as_ref()
    }

    /// Diagnoses whether a candidate route would make dispatch ambiguous.
    pub fn route_ambiguity_diagnostic(
        &self,
        method: RouteMethod,
        path: &str,
    ) -> Result<Option<RouteAmbiguityDiagnostic>, WebRouteError> {
        let candidate_path = if path == "*" {
            path.into()
        } else {
            normalize_router_path(path)?
        };
        let normalized_shape = route_ambiguity_key(&candidate_path)?;
        Ok(self
            .routes
            .iter()
            .filter(|route| route.method == method)
            .find(|route| {
                route_ambiguity_key(&route.path).is_ok_and(|shape| shape == normalized_shape)
            })
            .map(|route| {
                let reason = if route.path == candidate_path {
                    RouteAmbiguityReason::ExactPath
                } else {
                    RouteAmbiguityReason::ParameterizedShape
                };
                RouteAmbiguityDiagnostic {
                    method,
                    candidate_path,
                    existing_path: route.path.clone(),
                    normalized_shape,
                    reason,
                }
            }))
    }
}

/// Validates a response callback through the package descriptor contract.
pub fn validate_response_middleware_result<C: DescriptorValue>(
    value: &C,
) -> Result<(), WebRouteError> {
    match value.descriptor_view() {
        DescriptorView::Record("Response", _) => Ok(()),
        _ => Err("error[vm_http_router_response_middleware]: expected Response".into()),
    }
}

impl RouteMethod {
    /// Parses source-builder or HTTP wire spelling into the router method domain.
    pub fn from_name(method: &str) -> Option<Self> {
        match method {
            "GET" | "get" => Some(Self::Get),
            "POST" | "post" => Some(Self::Post),
            "PUT" | "put" => Some(Self::Put),
            "PATCH" | "patch" => Some(Self::Patch),
            "DELETE" | "delete" => Some(Self::Delete),
            "HEAD" | "head" => Some(Self::Head),
            "OPTIONS" | "options" => Some(Self::Options),
            _ => None,
        }
    }

    /// Returns the HTTP method text.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
            Self::Head => "HEAD",
            Self::Options => "OPTIONS",
        }
    }
}

fn normalize_router_path(path: impl AsRef<str>) -> Result<String, WebRouteError> {
    let path = path.as_ref();
    if path == "/" {
        return Ok("/".to_string());
    }
    if !path.starts_with('/') {
        return Err(format!("VM HTTP route path `{path}` must start with `/`").into());
    }
    if path.contains("//") || path.contains("..") {
        return Err(format!("VM HTTP route path `{path}` is not safe").into());
    }
    Ok(path.trim_end_matches('/').to_string())
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod concurrency_test;
