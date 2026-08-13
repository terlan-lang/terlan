#[cfg(test)]
use super::http::VmHttpOverloadConfig;
pub(crate) use super::native_callable::VmNativeCallableRef as VmHttpCompiledCallableRef;
use super::sse::VmSseEndpointPlan;
use super::websocket::VmWebSocketEndpointPlan;
use super::ReplValue;

#[cfg(test)]
#[path = "http_router/route_concurrency_test.rs"]
#[cfg(test)]
mod route_concurrency_test;

/// HTTP method accepted by the VM router.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmHttpRouteMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
}

/// One exact VM HTTP route.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VmHttpRoute {
    pub(crate) method: VmHttpRouteMethod,
    pub(crate) path: String,
    pub(crate) target: VmHttpRouteTarget,
    pub(crate) middleware: Vec<ReplValue>,
    pub(crate) response_middleware: Vec<ReplValue>,
}

/// Target selected by VM HTTP route dispatch.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum VmHttpRouteTarget {
    Handler(ReplValue),
    SseEndpoint(VmSseEndpointPlan),
    WebSocketEndpoint(VmWebSocketEndpointPlan),
}

/// Result of VM router dispatch.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum VmHttpRouterOutcome {
    Matched(Box<VmHttpRouteDispatch>),
    ShortCircuited(VmHttpRouteShortCircuit),
    NotFound,
}

/// Matched route handler and ordered middleware.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VmHttpRouteDispatch {
    pub(crate) method: VmHttpRouteMethod,
    pub(crate) path: String,
    pub(crate) route_pattern: String,
    pub(crate) route_params: Vec<(String, String)>,
    pub(crate) target: VmHttpRouteTarget,
    pub(crate) middleware: Vec<ReplValue>,
    pub(crate) response_middleware: Vec<ReplValue>,
}

/// Middleware-produced response that terminates dispatch before the handler.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VmHttpRouteShortCircuit {
    pub(crate) middleware: ReplValue,
    pub(crate) response: ReplValue,
    pub(crate) route_params: Vec<(String, String)>,
    pub(crate) response_middleware: Vec<ReplValue>,
}

/// VM-owned continuation over ordered route middleware.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VmHttpMiddlewareContinuation {
    dispatch: VmHttpRouteDispatch,
    next_index: usize,
}

/// One middleware continuation step.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum VmHttpMiddlewareStep {
    Middleware {
        middleware: ReplValue,
        continuation: VmHttpMiddlewareContinuation,
    },
    Handler(VmHttpRouteDispatch),
}

/// Typed result returned by one source middleware invocation.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum VmHttpMiddlewareResult {
    Continue,
    Respond(ReplValue),
}

impl VmHttpMiddlewareResult {
    /// Decodes the closed `std.http.Router.MiddlewareResult` value domain.
    pub(crate) fn from_value(value: ReplValue) -> Result<Self, String> {
        match value {
            ReplValue::Atom(tag) if tag == "continue" => Ok(Self::Continue),
            ReplValue::Record { name, fields } if name == "Continue" && fields.is_empty() => {
                Ok(Self::Continue)
            }
            ReplValue::Tuple(fields) => {
                let [ReplValue::Atom(tag), response] = fields.as_slice() else {
                    return Err(invalid_middleware_result());
                };
                if tag != "respond" || !is_response_descriptor(response) {
                    return Err(invalid_middleware_result());
                }
                Ok(Self::Respond(response.clone()))
            }
            ReplValue::Record { name, fields } if name == "Respond" && fields.len() == 1 => {
                let response = &fields[0].1;
                if !is_response_descriptor(response) {
                    return Err(invalid_middleware_result());
                }
                Ok(Self::Respond(response.clone()))
            }
            _ => Err(invalid_middleware_result()),
        }
    }
}

impl VmHttpMiddlewareContinuation {
    fn new(dispatch: VmHttpRouteDispatch) -> Self {
        Self {
            dispatch,
            next_index: 0,
        }
    }

    pub(crate) fn step(&self) -> VmHttpMiddlewareStep {
        let Some(middleware) = self.dispatch.middleware.get(self.next_index) else {
            return VmHttpMiddlewareStep::Handler(self.dispatch.clone());
        };
        VmHttpMiddlewareStep::Middleware {
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
pub(crate) struct VmHttpRouteAmbiguityDiagnostic {
    pub(crate) method: VmHttpRouteMethod,
    pub(crate) candidate_path: String,
    pub(crate) existing_path: String,
    pub(crate) normalized_shape: String,
    pub(crate) reason: VmHttpRouteAmbiguityReason,
}

/// Why a route registration is ambiguous.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmHttpRouteAmbiguityReason {
    ExactPath,
    ParameterizedShape,
}

impl VmHttpRouteAmbiguityDiagnostic {
    /// Keeps the existing builder API error text stable for callers.
    pub(crate) fn render_text(&self) -> String {
        format!(
            "duplicate VM HTTP route {} {}",
            self.method.as_str(),
            self.candidate_path
        )
    }
}

/// VM-owned HTTP router composition model.
///
/// Inputs:
/// - Exact method/path routes, global middleware, grouped child routers, and
///   fallback/error handlers.
///
/// Output:
/// - Deterministic dispatch outcomes that higher HTTP layers can execute
///   without depending on command-layer serve manifests or host framework state.
///
/// Transformation:
/// - Preserves builder order, rejects ambiguous exact routes, prefixes grouped
///   routes, and lets middleware short-circuit before handler execution.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct VmHttpRouter {
    routes: Vec<VmHttpRoute>,
    middleware: Vec<ReplValue>,
    response_middleware: Vec<ReplValue>,
    fallback: Option<ReplValue>,
    error: Option<ReplValue>,
    #[cfg(test)]
    overload: Option<VmHttpOverloadConfig>,
    lifecycle: Option<ReplValue>,
}

impl VmHttpRouter {
    /// Creates an empty VM router.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Adds global middleware to the router.
    pub(crate) fn use_middleware(mut self, middleware: ReplValue) -> Self {
        self.middleware.push(middleware);
        self
    }

    /// Adds response middleware to the current router scope.
    pub(crate) fn map_response(mut self, middleware: ReplValue) -> Self {
        self.response_middleware.push(middleware);
        self
    }

    /// Adds an exact GET route.
    #[cfg(test)]
    pub(crate) fn get(self, path: impl Into<String>, handler: ReplValue) -> Result<Self, String> {
        self.route(VmHttpRouteMethod::Get, path, handler)
    }

    /// Adds an exact route for the selected method.
    #[cfg(test)]
    pub(crate) fn route(
        self,
        method: VmHttpRouteMethod,
        path: impl Into<String>,
        handler: ReplValue,
    ) -> Result<Self, String> {
        self.route_target(method, path, VmHttpRouteTarget::Handler(handler))
    }

    /// Adds one canonical route target with compiler-flattened scope metadata.
    pub(crate) fn scoped_target(
        mut self,
        method: VmHttpRouteMethod,
        path: impl Into<String>,
        target: VmHttpRouteTarget,
        middleware: Vec<ReplValue>,
        response_middleware: Vec<ReplValue>,
    ) -> Result<Self, String> {
        let path = normalize_router_path(path.into())?;
        validate_route_pattern(&path)?;
        if let Some(diagnostic) = self.route_ambiguity_diagnostic(method, &path)? {
            return Err(diagnostic.render_text());
        }
        self.routes.push(VmHttpRoute {
            method,
            path,
            target,
            middleware,
            response_middleware,
        });
        Ok(self)
    }

    #[cfg(test)]
    fn route_target(
        mut self,
        method: VmHttpRouteMethod,
        path: impl Into<String>,
        target: VmHttpRouteTarget,
    ) -> Result<Self, String> {
        let path = normalize_router_path(path.into())?;
        validate_route_pattern(&path)?;
        if let Some(diagnostic) = self.route_ambiguity_diagnostic(method, &path)? {
            return Err(diagnostic.render_text());
        }
        self.routes.push(VmHttpRoute {
            method,
            path,
            target,
            middleware: Vec::new(),
            response_middleware: Vec::new(),
        });
        Ok(self)
    }

    /// Adds a fallback handler for unmatched requests.
    pub(crate) fn fallback(mut self, handler: ReplValue) -> Self {
        self.fallback = Some(handler);
        self
    }

    /// Adds an error handler used by higher layers after handler failures.
    pub(crate) fn error(mut self, handler: ReplValue) -> Self {
        self.error = Some(handler);
        self
    }

    /// Configures bounded pending HTTP work for this router.
    #[cfg(test)]
    pub(crate) fn overload(mut self, config: VmHttpOverloadConfig) -> Result<Self, String> {
        if self.overload.is_some() {
            return Err("router overload policy is already configured".to_string());
        }
        self.overload = Some(config);
        Ok(self)
    }

    /// Returns the validated source-level overload configuration.
    #[cfg(test)]
    pub(crate) fn overload_config(&self) -> Option<VmHttpOverloadConfig> {
        self.overload
    }

    /// Dispatches one request without executing middleware.
    pub(crate) fn dispatch(
        &self,
        method: VmHttpRouteMethod,
        path: &str,
    ) -> Result<VmHttpRouterOutcome, String> {
        let path = normalize_router_path(path)?;
        if let Some(route) = self
            .routes
            .iter()
            .find(|route| route.method == method && route.path == path)
        {
            return Ok(VmHttpRouterOutcome::Matched(Box::new(
                VmHttpRouteDispatch {
                    method,
                    path,
                    route_pattern: route.path.clone(),
                    route_params: Vec::new(),
                    target: route.target.clone(),
                    middleware: [self.middleware.clone(), route.middleware.clone()].concat(),
                    response_middleware: [
                        self.response_middleware.clone(),
                        route.response_middleware.clone(),
                    ]
                    .concat(),
                },
            )));
        }
        if let Some((route, route_params)) = self
            .routes
            .iter()
            .filter(|route| route.method == method)
            .find_map(|route| match_route_params(&route.path, &path).map(|params| (route, params)))
        {
            return Ok(VmHttpRouterOutcome::Matched(Box::new(
                VmHttpRouteDispatch {
                    method,
                    path,
                    route_pattern: route.path.clone(),
                    route_params,
                    target: route.target.clone(),
                    middleware: [self.middleware.clone(), route.middleware.clone()].concat(),
                    response_middleware: [
                        self.response_middleware.clone(),
                        route.response_middleware.clone(),
                    ]
                    .concat(),
                },
            )));
        }
        Ok(match &self.fallback {
            Some(handler) => VmHttpRouterOutcome::Matched(Box::new(VmHttpRouteDispatch {
                method,
                path,
                route_pattern: "*".to_string(),
                route_params: Vec::new(),
                target: VmHttpRouteTarget::Handler(handler.clone()),
                middleware: self.middleware.clone(),
                response_middleware: self.response_middleware.clone(),
            })),
            None => VmHttpRouterOutcome::NotFound,
        })
    }

    /// Dispatches middleware using the source-level typed result contract.
    pub(crate) fn dispatch_with_typed_middleware(
        &self,
        method: VmHttpRouteMethod,
        path: &str,
        mut invoke: impl FnMut(&ReplValue, &VmHttpMiddlewareContinuation) -> Result<ReplValue, String>,
    ) -> Result<VmHttpRouterOutcome, String> {
        let outcome = self.dispatch(method, path)?;
        let VmHttpRouterOutcome::Matched(dispatch) = outcome else {
            return Ok(outcome);
        };
        let mut continuation = VmHttpMiddlewareContinuation::new(*dispatch);
        loop {
            match continuation.step() {
                VmHttpMiddlewareStep::Middleware {
                    middleware,
                    continuation: next,
                } => match VmHttpMiddlewareResult::from_value(invoke(&middleware, &next)?)? {
                    VmHttpMiddlewareResult::Continue => continuation = next,
                    VmHttpMiddlewareResult::Respond(response) => {
                        return Ok(VmHttpRouterOutcome::ShortCircuited(
                            VmHttpRouteShortCircuit {
                                middleware,
                                response,
                                route_params: next.dispatch.route_params.clone(),
                                response_middleware: next.dispatch.response_middleware.clone(),
                            },
                        ));
                    }
                },
                VmHttpMiddlewareStep::Handler(dispatch) => {
                    return Ok(VmHttpRouterOutcome::Matched(Box::new(dispatch)));
                }
            }
        }
    }

    /// Returns the configured error handler.
    pub(crate) fn error_handler(&self) -> Option<&ReplValue> {
        self.error.as_ref()
    }

    /// Diagnoses whether a candidate route would make dispatch ambiguous.
    pub(crate) fn route_ambiguity_diagnostic(
        &self,
        method: VmHttpRouteMethod,
        path: &str,
    ) -> Result<Option<VmHttpRouteAmbiguityDiagnostic>, String> {
        let candidate_path = normalize_router_path(path)?;
        validate_route_pattern(&candidate_path)?;
        let normalized_shape = route_shape(&candidate_path);
        Ok(self
            .routes
            .iter()
            .filter(|route| route.method == method)
            .find(|route| route_shape(&route.path) == normalized_shape)
            .map(|route| {
                let reason = if route.path == candidate_path {
                    VmHttpRouteAmbiguityReason::ExactPath
                } else {
                    VmHttpRouteAmbiguityReason::ParameterizedShape
                };
                VmHttpRouteAmbiguityDiagnostic {
                    method,
                    candidate_path,
                    existing_path: route.path.clone(),
                    normalized_shape,
                    reason,
                }
            }))
    }
}

fn is_response_descriptor(value: &ReplValue) -> bool {
    matches!(
        value,
        ReplValue::Tuple(fields)
            if matches!(fields.first(), Some(ReplValue::Atom(tag)) if tag == "response")
                || matches!(
                    fields.as_slice(),
                    [ReplValue::Int(0), ReplValue::Int(kind), ..] if (0..=4).contains(kind)
                )
    )
}

/// Validates the result returned by source response middleware.
pub(crate) fn validate_response_middleware_result(value: &ReplValue) -> Result<(), String> {
    if is_response_descriptor(value) {
        Ok(())
    } else {
        Err("error[vm_http_router_response_middleware]: expected Response".to_string())
    }
}

/// Renders the stable diagnostic for malformed middleware result values.
fn invalid_middleware_result() -> String {
    "error[vm_http_router_middleware]: expected Continue or Respond(Response)".to_string()
}

impl VmHttpRouteMethod {
    /// Parses source-builder or HTTP wire spelling into the router method domain.
    pub(crate) fn from_name(method: &str) -> Option<Self> {
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
    pub(crate) fn as_str(self) -> &'static str {
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

fn normalize_router_path(path: impl AsRef<str>) -> Result<String, String> {
    let path = path.as_ref();
    if path == "/" {
        return Ok("/".to_string());
    }
    if !path.starts_with('/') {
        return Err(format!("VM HTTP route path `{path}` must start with `/`"));
    }
    if path.contains("//") || path.contains("..") {
        return Err(format!("VM HTTP route path `{path}` is not safe"));
    }
    Ok(path.trim_end_matches('/').to_string())
}

fn validate_route_pattern(path: &str) -> Result<(), String> {
    for segment in path_segments(path) {
        if let Some(name) = route_param_name(segment) {
            if !valid_route_param_name(name) {
                return Err(format!(
                    "VM HTTP route path `{path}` has invalid parameter `{name}`"
                ));
            }
        }
    }
    Ok(())
}

fn match_route_params(pattern: &str, path: &str) -> Option<Vec<(String, String)>> {
    let pattern_segments = path_segments(pattern);
    let path_segments = path_segments(path);
    if pattern_segments.len() != path_segments.len() {
        return None;
    }

    let mut params = Vec::new();
    for (pattern_segment, path_segment) in pattern_segments.iter().zip(path_segments.iter()) {
        if let Some(name) = route_param_name(pattern_segment) {
            if path_segment.is_empty() {
                return None;
            }
            params.push((name.to_string(), (*path_segment).to_string()));
        } else if pattern_segment != path_segment {
            return None;
        }
    }
    (!params.is_empty()).then_some(params)
}

fn route_shape(path: &str) -> String {
    let mut shape = String::new();
    for segment in path_segments(path) {
        shape.push('/');
        if route_param_name(segment).is_some() {
            shape.push(':');
        } else {
            shape.push_str(segment);
        }
    }
    if shape.is_empty() {
        "/".to_string()
    } else {
        shape
    }
}

fn route_param_name(segment: &str) -> Option<&str> {
    if let Some(name) = segment.strip_prefix(':') {
        return (!name.is_empty()).then_some(name);
    }
    segment
        .strip_prefix('{')
        .and_then(|name| name.strip_suffix('}'))
        .filter(|name| !name.is_empty())
}

fn valid_route_param_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn path_segments(path: &str) -> Vec<&str> {
    if path == "/" {
        Vec::new()
    } else {
        path.trim_matches('/').split('/').collect()
    }
}
