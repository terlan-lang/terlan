use super::*;
use crate::channel_plan::{SseEndpointPlan, WebSocketEndpointPlan};
use crate::routing::Fallback;

/// Package-owned route declarations with host-admitted callable values.
pub struct Router<C> {
    pub routes: Vec<Route<C>>,
    pub fallback: Option<Fallback<C>>,
    pub error: Option<C>,
    pub lifecycle: Option<C>,
    pub overload: Option<(String, usize)>,
}

impl<C> Default for Router<C> {
    fn default() -> Self {
        Self {
            routes: Vec::new(),
            fallback: None,
            error: None,
            lifecycle: None,
            overload: None,
        }
    }
}

impl<C: DescriptorValue + Clone> Router<C> {
    /// Admits source-composed declarations into the package's dispatch table.
    pub fn into_routing_table(
        self,
    ) -> std::result::Result<crate::routing::Router<C>, crate::route_pattern::WebRouteError> {
        use crate::routing::{RouteMethod, RouteTarget as Target};

        if self.lifecycle.is_some() || self.overload.is_some() {
            return Err("error[serve.aot.router]: lifecycle and overload admission are not implemented for source routers".into());
        }
        let mut router = crate::routing::Router::new();
        for route in self.routes {
            let method = RouteMethod::from_name(&route.method).ok_or_else(|| {
                format!(
                    "error[serve.aot.router]: unsupported method `{}`",
                    route.method
                )
            })?;
            let target = match route.target {
                RouteTarget::Handler(handler) => Target::Handler(handler),
                RouteTarget::Sse(plan) => Target::SseEndpoint(plan),
                RouteTarget::WebSocket(plan) => Target::WebSocketEndpoint(plan),
            };
            router = router.scoped_target(
                method,
                route.path,
                target,
                route.middleware,
                route.response_middleware,
            )?;
        }
        if let Some(fallback) = self.fallback {
            router = router.fallback_target(fallback);
        }
        if let Some(error) = self.error {
            router = router.error(error);
        }
        Ok(router)
    }
}

/// One flattened route with complete source-composed middleware ordering.
pub struct Route<C> {
    pub method: String,
    pub path: String,
    pub target: RouteTarget<C>,
    pub middleware: Vec<C>,
    pub response_middleware: Vec<C>,
}

/// HTTP or long-lived channel endpoint produced by source code.
pub enum RouteTarget<C> {
    Handler(C),
    Sse(SseEndpointPlan<C>),
    WebSocket(WebSocketEndpointPlan<C>),
}

/// Decodes an executed Router without interpreting compiler IR or function names.
pub fn router<V: DescriptorValue, C: Clone>(
    value: &V,
    mut callback: impl FnMut(&V, usize) -> Result<C>,
) -> Result<Router<C>> {
    let [entries] = record(value, "Router", ["entries"])?;
    let entries = list(entries)?;
    let mut scopes = vec![Router::default()];
    for entry in entries {
        let group_start = match entry.descriptor_view() {
            DescriptorView::Atom("group_start_entry") => true,
            DescriptorView::Record("Group_start_entry", fields) => fields.is_empty(),
            _ => false,
        };
        if group_start {
            if scopes.len() >= 64 {
                return Err(error("router group nesting exceeds 64"));
            }
            scopes.push(Router::default());
            continue;
        }
        let group_end = match entry.descriptor_view() {
            DescriptorView::Atom("group_end_entry") => true,
            DescriptorView::Record("Group_end_entry", fields) => fields.is_empty(),
            _ => false,
        };
        if group_end {
            if scopes.len() == 1 {
                return Err(error("unmatched group end"));
            }
            let child = scopes.pop().ok_or_else(|| error("missing group"))?;
            let parent = scopes.last_mut().ok_or_else(|| error("missing parent"))?;
            append_group(parent, child)?;
            continue;
        }
        let (tag, fields) = variant(
            entry,
            &[
                (
                    "Route",
                    &[
                        "method",
                        "path",
                        "handler",
                        "middleware",
                        "response_middleware",
                    ],
                ),
                (
                    "Sse",
                    &["path", "endpoint", "middleware", "response_middleware"],
                ),
                (
                    "Websocket",
                    &["path", "endpoint", "middleware", "response_middleware"],
                ),
                ("Middleware", &["callback"]),
                ("Response_middleware", &["callback"]),
                (
                    "Fallback",
                    &["callback", "middleware", "response_middleware"],
                ),
                ("Err", &["callback"]),
                ("Lifecycle", &["callback"]),
                ("Overload", &["policy", "max_pending"]),
            ],
        )?;
        let scope = scopes.last_mut().ok_or_else(|| error("missing scope"))?;
        let target = match (tag, fields.as_slice()) {
            ("Route", [method, path, handler, middleware, response_middleware]) => Some((
                text(*method)?,
                text(*path)?,
                RouteTarget::Handler(callback(handler, 1)?),
                *middleware,
                *response_middleware,
            )),
            ("Sse", [path, endpoint, middleware, response_middleware]) => Some((
                "GET".into(),
                text(*path)?,
                RouteTarget::Sse(sse_endpoint(*endpoint, &mut callback)?),
                *middleware,
                *response_middleware,
            )),
            ("Websocket", [path, endpoint, middleware, response_middleware]) => Some((
                "GET".into(),
                text(*path)?,
                RouteTarget::WebSocket(websocket_endpoint(*endpoint, &mut callback)?),
                *middleware,
                *response_middleware,
            )),
            ("Middleware", [value]) => {
                callback(value, 1)?;
                None
            }
            ("Response_middleware", [value]) => {
                callback(value, 2)?;
                None
            }
            ("Fallback", [value, middleware, response_middleware]) => {
                let fallback = Fallback {
                    handler: callback(value, 1)?,
                    middleware: callbacks(*middleware, 1, &mut callback)?,
                    response_middleware: callbacks(*response_middleware, 2, &mut callback)?,
                };
                install(&mut scope.fallback, fallback, "fallback")?;
                None
            }
            ("Err", [value]) => {
                install(&mut scope.error, callback(value, 1)?, "error handler")?;
                None
            }
            ("Lifecycle", [value]) => {
                install(&mut scope.lifecycle, callback(value, 1)?, "lifecycle")?;
                None
            }
            ("Overload", [policy, limit]) => {
                let DescriptorView::Atom(policy @ ("queue" | "reject" | "spill")) =
                    policy.descriptor_view()
                else {
                    return Err(error("unknown overload policy"));
                };
                install(
                    &mut scope.overload,
                    (policy.into(), positive(*limit)?),
                    "overload",
                )?;
                None
            }
            _ => {
                return Err(error(format!(
                    "unknown or malformed router declaration `{tag}`"
                )))
            }
        };
        if let Some((method, path, target, middleware, response_middleware)) = target {
            scope.routes.push(Route {
                method,
                path,
                target,
                middleware: callbacks(middleware, 1, &mut callback)?,
                response_middleware: callbacks(response_middleware, 2, &mut callback)?,
            });
        }
    }
    if scopes.len() != 1 {
        return Err(error("unterminated router group"));
    }
    scopes.pop().ok_or_else(|| error("missing router"))
}

fn callbacks<V: DescriptorValue, C>(
    values: &V,
    arity: usize,
    callback: &mut impl FnMut(&V, usize) -> Result<C>,
) -> Result<Vec<C>> {
    list(values)?
        .iter()
        .map(|value| callback(value, arity))
        .collect()
}

fn install<T>(slot: &mut Option<T>, value: T, name: &str) -> Result<()> {
    if slot.is_some() {
        return Err(error(format!("duplicate {name}")));
    }
    *slot = Some(value);
    Ok(())
}

fn append_group<C>(parent: &mut Router<C>, child: Router<C>) -> Result<()> {
    if child.lifecycle.is_some() || child.overload.is_some() {
        return Err(error(
            "lifecycle and overload policies require a root router",
        ));
    }
    parent.routes.extend(child.routes);
    Ok(())
}
