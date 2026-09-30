use super::*;
use crate::channel_plan::{SseEndpointPlan, WebSocketEndpointPlan};

/// Package-owned route declarations with host-admitted callable values.
pub struct Router<C> {
    pub routes: Vec<Route<C>>,
    pub middleware: Vec<C>,
    pub response_middleware: Vec<C>,
    pub fallback: Option<C>,
    pub error: Option<C>,
    pub lifecycle: Option<C>,
    pub overload: Option<(String, usize)>,
}

impl<C> Default for Router<C> {
    fn default() -> Self {
        Self {
            routes: Vec::new(),
            middleware: Vec::new(),
            response_middleware: Vec::new(),
            fallback: None,
            error: None,
            lifecycle: None,
            overload: None,
        }
    }
}

/// One flattened route with its group middleware retained in declaration order.
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
    let mut scopes = vec![(String::new(), Router::default())];
    for entry in entries {
        let group_end = match entry.descriptor_view() {
            DescriptorView::Atom("group_end_entry") => true,
            DescriptorView::Record("Group_end_entry", fields) => fields.is_empty(),
            _ => false,
        };
        if group_end {
            if scopes.len() == 1 {
                return Err(error("unmatched group end"));
            }
            let (prefix, child) = scopes.pop().ok_or_else(|| error("missing group"))?;
            let parent = &mut scopes.last_mut().ok_or_else(|| error("missing parent"))?.1;
            append_group(parent, prefix, child)?;
            continue;
        }
        let (tag, fields) = variant(
            entry,
            &[
                ("Group_start", &["prefix"]),
                ("Route", &["method", "path", "handler"]),
                ("Sse", &["path", "endpoint"]),
                ("Websocket", &["path", "endpoint"]),
                ("Middleware", &["callback"]),
                ("Response_middleware", &["callback"]),
                ("Fallback", &["callback"]),
                ("Err", &["callback"]),
                ("Lifecycle", &["callback"]),
                ("Overload", &["policy", "max_pending"]),
            ],
        )?;
        if let ("Group_start", [prefix]) = (tag, fields.as_slice()) {
            if scopes.len() >= 64 {
                return Err(error("router group nesting exceeds 64"));
            }
            scopes.push((text(*prefix)?, Router::default()));
            continue;
        }
        let scope = &mut scopes.last_mut().ok_or_else(|| error("missing scope"))?.1;
        let target = match (tag, fields.as_slice()) {
            ("Route", [method, path, handler]) => Some((
                text(*method)?,
                text(*path)?,
                RouteTarget::Handler(callback(handler, 1)?),
            )),
            ("Sse", [path, endpoint]) => Some((
                "GET".into(),
                text(*path)?,
                RouteTarget::Sse(sse_endpoint(*endpoint, &mut callback)?),
            )),
            ("Websocket", [path, endpoint]) => Some((
                "GET".into(),
                text(*path)?,
                RouteTarget::WebSocket(websocket_endpoint(*endpoint, &mut callback)?),
            )),
            ("Middleware", [value]) => {
                scope.middleware.push(callback(value, 1)?);
                None
            }
            ("Response_middleware", [value]) => {
                scope.response_middleware.push(callback(value, 2)?);
                None
            }
            ("Fallback", [value]) => {
                install(&mut scope.fallback, callback(value, 1)?, "fallback")?;
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
        if let Some((method, path, target)) = target {
            scope.routes.push(Route {
                method,
                path,
                target,
                middleware: Vec::new(),
                response_middleware: Vec::new(),
            });
        }
    }
    if scopes.len() != 1 {
        return Err(error("unterminated router group"));
    }
    scopes
        .pop()
        .map(|(_, router)| router)
        .ok_or_else(|| error("missing router"))
}

fn install<T>(slot: &mut Option<T>, value: T, name: &str) -> Result<()> {
    if slot.is_some() {
        return Err(error(format!("duplicate {name}")));
    }
    *slot = Some(value);
    Ok(())
}

fn append_group<C: Clone>(parent: &mut Router<C>, prefix: String, child: Router<C>) -> Result<()> {
    if child.lifecycle.is_some() || child.overload.is_some() {
        return Err(error(
            "lifecycle and overload policies require a root router",
        ));
    }
    for mut route in child.routes {
        route.path = prefixed(&prefix, &route.path);
        route.middleware = [child.middleware.clone(), route.middleware].concat();
        route.response_middleware =
            [child.response_middleware.clone(), route.response_middleware].concat();
        parent.routes.push(route);
    }
    if let Some(fallback) = child.fallback {
        for method in ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"] {
            parent.routes.push(Route {
                method: method.into(),
                path: prefixed(&prefix, "*"),
                target: RouteTarget::Handler(fallback.clone()),
                middleware: child.middleware.clone(),
                response_middleware: child.response_middleware.clone(),
            });
        }
    }
    if parent.error.is_none() {
        parent.error = child.error;
    }
    Ok(())
}

fn prefixed(prefix: &str, path: &str) -> String {
    if path == "/" {
        return prefix.trim_end_matches('/').to_owned();
    }
    format!(
        "{}/{}",
        prefix.trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}
