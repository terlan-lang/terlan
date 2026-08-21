#[cfg(any(test, not(feature = "serve-runtime-bin")))]
use crate::runtime::vm::aot_metadata::{
    AotRouterCallable, AotRouterPlan, AotRouterRoute, AotRouterRouteTarget,
};
#[cfg(any(test, not(feature = "serve-runtime-bin")))]
use crate::runtime::vm::sse::VmSseCallbackPlan;
#[cfg(any(test, not(feature = "serve-runtime-bin")))]
use crate::runtime::vm::sse::VmSseEndpointPlan;
#[cfg(any(test, not(feature = "serve-runtime-bin")))]
use crate::runtime::vm::websocket::VmWebSocketCallbackPlan;
#[cfg(any(test, not(feature = "serve-runtime-bin")))]
use crate::runtime::vm::websocket::VmWebSocketEndpointPlan;
use crate::terlan_syntax::{SyntaxExprKind, SyntaxExprOutput};
#[cfg(any(test, not(feature = "serve-runtime-bin")))]
use crate::terlan_typeck::{CoreExportKind, CoreExpr, CoreModule, CorePattern};
#[cfg(any(test, not(feature = "serve-runtime-bin")))]
use std::collections::HashMap;

#[cfg(any(test, not(feature = "serve-runtime-bin")))]
const ROUTER_MODULE: &str = "std.http.Router";

/// Extracts static router metadata and removes `router/0` from native execution.
#[cfg(any(test, not(feature = "serve-runtime-bin")))]
pub(crate) fn prepare_aot_router_module(
    core: &CoreModule,
) -> Result<(CoreModule, Option<AotRouterPlan>), String> {
    let Some(router) = core
        .functions
        .iter()
        .find(|function| function.name == "router" && function.arity == 0)
    else {
        return Ok((core.clone(), None));
    };
    let [clause] = router.clauses.as_slice() else {
        return Err("error[native_ir.http_router]: router/0 must have one clause".to_string());
    };
    let body = clause.body.core_expr.as_ref().ok_or_else(|| {
        "error[native_ir.http_router]: router/0 has no executable CoreIR body".to_string()
    })?;
    let mut plan = evaluate_router(core, body, &HashMap::new())?;
    plan.module = core.module.clone();
    let mut executable = core.clone();
    executable
        .functions
        .retain(|function| !(function.name == "router" && function.arity == 0));
    executable.exports.retain(|export| {
        !(export.name == "router" && matches!(export.kind, CoreExportKind::Function { arity: 0 }))
    });
    Ok((executable, Some(plan)))
}

/// Evaluates only the closed router-builder expression domain.
#[cfg(any(test, not(feature = "serve-runtime-bin")))]
fn evaluate_router(
    core: &CoreModule,
    expr: &CoreExpr,
    environment: &HashMap<String, AotRouterPlan>,
) -> Result<AotRouterPlan, String> {
    match expr {
        CoreExpr::Var(name) => environment.get(name).cloned().ok_or_else(|| {
            format!("error[native_ir.http_router]: unknown router binding `{name}`")
        }),
        CoreExpr::Let { bindings, body } => {
            let mut environment = environment.clone();
            for binding in bindings {
                let CorePattern::Var(name) = &binding.pattern else {
                    return Err(
                        "error[native_ir.http_router]: router bindings must use names".to_string(),
                    );
                };
                let value = evaluate_router(core, &binding.value, &environment)?;
                environment.insert(name.clone(), value);
            }
            evaluate_router(core, body, &environment)
        }
        CoreExpr::RemoteCall {
            module,
            function,
            args,
        } if module == ROUTER_MODULE && function == "new" && args.is_empty() => {
            Ok(AotRouterPlan::default())
        }
        CoreExpr::RemoteCall {
            module,
            function,
            args,
        } if matches!(module.as_str(), ROUTER_MODULE | "__receiver__") => {
            apply_router_call(core, function, args, environment)
        }
        _ => Err(format!(
            "error[native_ir.http_router]: router/0 contains unsupported expression `{expr:?}`"
        )),
    }
}

/// Applies one statically known builder operation to an immutable plan.
#[cfg(any(test, not(feature = "serve-runtime-bin")))]
fn apply_router_call(
    core: &CoreModule,
    function: &str,
    args: &[CoreExpr],
    environment: &HashMap<String, AotRouterPlan>,
) -> Result<AotRouterPlan, String> {
    let receiver = args.first().ok_or_else(|| {
        format!("error[native_ir.http_router]: Router.{function} is missing its receiver")
    })?;
    let mut plan = evaluate_router(core, receiver, environment)?;
    match function {
        "get" | "post" | "put" | "patch" | "delete" | "head" | "options" => {
            let [_, path, handler] = args else {
                return Err(router_arity(function, 3, args.len()));
            };
            plan.routes.push(AotRouterRoute {
                method: function.to_ascii_uppercase(),
                path: string_literal(path)?,
                target: AotRouterRouteTarget::Handler(callable(core, handler)?),
                middleware: Vec::new(),
                response_middleware: Vec::new(),
            });
        }
        "sse" => {
            let [_, path, endpoint] = args else {
                return Err(router_arity(function, 3, args.len()));
            };
            plan.routes.push(AotRouterRoute {
                method: "GET".to_string(),
                path: string_literal(path)?,
                target: AotRouterRouteTarget::Sse(sse_endpoint(core, endpoint)?),
                middleware: Vec::new(),
                response_middleware: Vec::new(),
            });
        }
        "websocket" => {
            let [_, path, endpoint] = args else {
                return Err(router_arity(function, 3, args.len()));
            };
            plan.routes.push(AotRouterRoute {
                method: "GET".to_string(),
                path: string_literal(path)?,
                target: AotRouterRouteTarget::WebSocket(websocket_endpoint(core, endpoint)?),
                middleware: Vec::new(),
                response_middleware: Vec::new(),
            });
        }
        "use" => {
            let [_, middleware] = args else {
                return Err(router_arity(function, 2, args.len()));
            };
            plan.middleware.push(callable(core, middleware)?);
        }
        "map_response" => {
            let [_, middleware] = args else {
                return Err(router_arity(function, 2, args.len()));
            };
            plan.response_middleware.push(callable(core, middleware)?);
        }
        "fallback" => {
            let [_, handler] = args else {
                return Err(router_arity(function, 2, args.len()));
            };
            if plan.fallback.replace(callable(core, handler)?).is_some() {
                return Err("error[native_ir.http_router]: duplicate fallback".to_string());
            }
        }
        "error" => {
            let [_, handler] = args else {
                return Err(router_arity(function, 2, args.len()));
            };
            if plan.error.replace(callable(core, handler)?).is_some() {
                return Err("error[native_ir.http_router]: duplicate error handler".to_string());
            }
        }
        "group" => apply_group(core, &mut plan, args, environment)?,
        unsupported => {
            return Err(format!(
                "error[native_ir.http_router]: Router.{unsupported} is not in the synchronous AOT router profile"
            ));
        }
    }
    Ok(plan)
}

/// Applies one statically bounded route group and its scoped middleware.
#[cfg(any(test, not(feature = "serve-runtime-bin")))]
fn apply_group(
    core: &CoreModule,
    plan: &mut AotRouterPlan,
    args: &[CoreExpr],
    environment: &HashMap<String, AotRouterPlan>,
) -> Result<(), String> {
    let [_, prefix, configure] = args else {
        return Err(router_arity("group", 3, args.len()));
    };
    let prefix = string_literal(prefix)?;
    let CoreExpr::Lam { params, body } = configure else {
        return Err(
            "error[native_ir.http_router]: Router.group requires a static lambda".to_string(),
        );
    };
    let [CorePattern::Var(parameter)] = params.as_slice() else {
        return Err(
            "error[native_ir.http_router]: Router.group lambda must accept one named router"
                .to_string(),
        );
    };
    let mut grouped_environment = environment.clone();
    grouped_environment.insert(parameter.clone(), AotRouterPlan::default());
    let child = evaluate_router(core, body, &grouped_environment)?;
    for mut route in child.routes {
        route.path = prefixed_path(&prefix, &route.path);
        route.middleware = [child.middleware.clone(), route.middleware].concat();
        route.response_middleware =
            [child.response_middleware.clone(), route.response_middleware].concat();
        plan.routes.push(route);
    }
    if let Some(fallback) = child.fallback {
        for method in ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"] {
            plan.routes.push(AotRouterRoute {
                method: method.to_string(),
                path: prefixed_path(&prefix, "*"),
                target: AotRouterRouteTarget::Handler(fallback.clone()),
                middleware: child.middleware.clone(),
                response_middleware: child.response_middleware.clone(),
            });
        }
    }
    if plan.error.is_none() {
        plan.error = child.error;
    }
    Ok(())
}

/// Decodes one checked SSE endpoint expression into the canonical VM plan.
#[cfg(any(test, not(feature = "serve-runtime-bin")))]
fn sse_endpoint(core: &CoreModule, expr: &CoreExpr) -> Result<VmSseEndpointPlan, String> {
    if let CoreExpr::RemoteCall {
        module,
        function,
        args,
    } = expr
    {
        if matches!(module.as_str(), "std.http.Sse" | "Sse" | "__receiver__")
            && function == "callbacks"
        {
            let [endpoint, open, event_ready, keep_alive, drain, cancellation] = args.as_slice()
            else {
                return Err(format!(
                    "error[native_ir.http_router]: unsupported SSE callback builder `callbacks/{}`",
                    args.len()
                ));
            };
            let callbacks = VmSseCallbackPlan {
                open: channel_callback(core, open, "SSE", "open", 0)?,
                event_ready: channel_callback(core, event_ready, "SSE", "event-ready", 1)?,
                keep_alive: channel_callback(core, keep_alive, "SSE", "keep-alive", 0)?,
                drain: channel_callback(core, drain, "SSE", "drain", 0)?,
                cancellation: channel_callback(core, cancellation, "SSE", "cancellation", 1)?,
            };
            return sse_endpoint(core, endpoint)?
                .with_callbacks(callbacks)
                .map_err(|error| {
                    format!("error[native_ir.http_router]: invalid SSE callbacks: {error:?}")
                });
        }
    }
    let CoreExpr::RemoteCall {
        module,
        function,
        args,
    } = expr
    else {
        return Err(
            "error[native_ir.http_router]: SSE endpoint must be a static builder call".to_string(),
        );
    };
    if !matches!(module.as_str(), "std.http.Sse" | "Sse") {
        return Err(format!(
            "error[native_ir.http_router]: `{module}.{function}` is not an SSE endpoint builder"
        ));
    }
    let plan = match (function.as_str(), args.as_slice()) {
        ("endpoint", [pending, bytes]) => VmSseEndpointPlan::new(
            positive_usize(pending, "SSE pending event limit")?,
            positive_usize(bytes, "SSE event byte limit")?,
        ),
        ("endpoint_with_keep_alive", [pending, bytes, keep_alive]) => {
            let keep_alive = positive_u64(keep_alive, "SSE keep-alive interval")?;
            VmSseEndpointPlan::new(
                positive_usize(pending, "SSE pending event limit")?,
                positive_usize(bytes, "SSE event byte limit")?,
            )
            .and_then(|plan| plan.with_keep_alive_ms(keep_alive))
        }
        _ => {
            return Err(format!(
                "error[native_ir.http_router]: unsupported SSE endpoint `{function}/{}`",
                args.len()
            ));
        }
    };
    plan.map_err(|error| format!("error[native_ir.http_router]: invalid SSE endpoint: {error:?}"))
}

/// Decodes one checked WebSocket endpoint expression into the canonical VM plan.
#[cfg(any(test, not(feature = "serve-runtime-bin")))]
fn websocket_endpoint(
    core: &CoreModule,
    expr: &CoreExpr,
) -> Result<VmWebSocketEndpointPlan, String> {
    if let CoreExpr::RemoteCall {
        module,
        function,
        args,
    } = expr
    {
        if matches!(
            module.as_str(),
            "std.http.WebSocket" | "WebSocket" | "__receiver__"
        ) && function == "callbacks"
        {
            let [endpoint, open, inbound, writable, close, cancellation] = args.as_slice() else {
                return Err(format!(
                    "error[native_ir.http_router]: unsupported WebSocket callback builder `callbacks/{}`",
                    args.len()
                ));
            };
            let callbacks = VmWebSocketCallbackPlan {
                open: channel_callback(core, open, "WebSocket", "open", 0)?,
                inbound: channel_callback(core, inbound, "WebSocket", "inbound", 1)?,
                writable: channel_callback(core, writable, "WebSocket", "writable", 0)?,
                close: channel_callback(core, close, "WebSocket", "close", 0)?,
                cancellation: channel_callback(core, cancellation, "WebSocket", "cancellation", 1)?,
            };
            return websocket_endpoint(core, endpoint)?
                .with_callbacks(callbacks)
                .map_err(|error| {
                    format!("error[native_ir.http_router]: invalid WebSocket callbacks: {error}")
                });
        }
    }
    let (function, args) = match expr {
        CoreExpr::RemoteCall {
            module,
            function,
            args,
        } if matches!(module.as_str(), "std.http.WebSocket" | "WebSocket") => {
            (function.as_str(), args.as_slice())
        }
        CoreExpr::Call { function, args }
            if matches!(
                function.as_str(),
                "endpoint" | "std.http.WebSocket.endpoint"
            ) =>
        {
            ("endpoint", args.as_slice())
        }
        _ => {
            return Err(
                "error[native_ir.http_router]: WebSocket endpoint must be a static builder call"
                    .to_string(),
            );
        }
    };
    if function != "endpoint" {
        return Err(format!(
            "error[native_ir.http_router]: `{function}` is not a WebSocket endpoint builder"
        ));
    }
    let [pending, bytes] = args else {
        return Err(format!(
            "error[native_ir.http_router]: unsupported WebSocket endpoint `{function}/{}`",
            args.len()
        ));
    };
    VmWebSocketEndpointPlan::new(
        positive_usize(pending, "WebSocket pending frame limit")?,
        positive_usize(bytes, "WebSocket frame byte limit")?,
    )
}

/// Resolves and validates one statically known WebSocket lifecycle callback.
#[cfg(any(test, not(feature = "serve-runtime-bin")))]
fn channel_callback(
    core: &CoreModule,
    expr: &CoreExpr,
    channel: &str,
    event: &str,
    arity: usize,
) -> Result<crate::runtime::vm::native_callable::VmNativeCallableRef, String> {
    let callback = callable(core, expr)?;
    if callback.arity != arity {
        return Err(format!(
            "error[native_ir.http_router]: {channel} {event} callback `{}` must have arity {arity}, found {}",
            callback.function, callback.arity
        ));
    }
    Ok(crate::runtime::vm::native_callable::VmNativeCallableRef {
        module: callback.module,
        function: callback.function,
        arity: callback.arity,
    })
}

/// Decodes one positive target-sized integer from checked router metadata.
#[cfg(any(test, not(feature = "serve-runtime-bin")))]
fn positive_usize(expr: &CoreExpr, label: &str) -> Result<usize, String> {
    let value = positive_u64(expr, label)?;
    usize::try_from(value).map_err(|_| {
        format!("error[native_ir.http_router]: {label} exceeds the target integer range")
    })
}

/// Decodes one positive integer from checked router metadata.
#[cfg(any(test, not(feature = "serve-runtime-bin")))]
fn positive_u64(expr: &CoreExpr, label: &str) -> Result<u64, String> {
    let CoreExpr::Int(value) = expr else {
        return Err(format!(
            "error[native_ir.http_router]: {label} must be a static integer"
        ));
    };
    u64::try_from(*value)
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| format!("error[native_ir.http_router]: {label} must be positive"))
}

/// Resolves one local function value to its exact native entry identity.
#[cfg(any(test, not(feature = "serve-runtime-bin")))]
fn callable(core: &CoreModule, expr: &CoreExpr) -> Result<AotRouterCallable, String> {
    let (module, function, declared_arity) =
        match expr {
            CoreExpr::Var(function) => (core.module.as_str(), function.as_str(), None),
            CoreExpr::RemoteFunRef {
                module,
                function,
                arity,
            } => (module.as_str(), function.as_str(), Some(*arity)),
            _ => return Err(
                "error[native_ir.http_router]: router callbacks must be static function references"
                    .to_string(),
            ),
        };
    if module != core.module {
        return Err(format!(
            "error[native_ir.http_router]: callback `{module}.{function}` is outside the router image"
        ));
    }
    let candidates = core
        .functions
        .iter()
        .filter(|candidate| candidate.name == function)
        .collect::<Vec<_>>();
    let function = match declared_arity {
        Some(arity) => candidates
            .into_iter()
            .find(|candidate| candidate.arity == arity),
        None if candidates.len() == 1 => candidates.into_iter().next(),
        None => None,
    }
    .ok_or_else(|| {
        format!(
            "error[native_ir.http_router]: callback `{module}.{function}` is missing or ambiguous"
        )
    })?;
    Ok(AotRouterCallable {
        module: module.to_string(),
        function: function.name.clone(),
        arity: function.arity,
    })
}

/// Decodes one canonical CoreIR UTF-8 route literal.
#[cfg(any(test, not(feature = "serve-runtime-bin")))]
fn string_literal(expr: &CoreExpr) -> Result<String, String> {
    let CoreExpr::Binary(value) = expr else {
        return Err(
            "error[native_ir.http_router]: route paths must be string literals".to_string(),
        );
    };
    serde_json::from_str(value).map_err(|error| {
        format!("error[native_ir.http_router]: invalid route string literal: {error}")
    })
}

/// Prefixes one group-local path using the public router path convention.
#[cfg(any(test, not(feature = "serve-runtime-bin")))]
fn prefixed_path(prefix: &str, path: &str) -> String {
    let prefix = prefix.trim_end_matches('/');
    if path == "/" {
        return prefix.to_string();
    }
    if path == "*" {
        return format!("{prefix}/*");
    }
    format!("{prefix}/{}", path.trim_start_matches('/'))
}

/// Builds one stable builder arity diagnostic.
#[cfg(any(test, not(feature = "serve-runtime-bin")))]
fn router_arity(function: &str, expected: usize, actual: usize) -> String {
    format!(
        "error[native_ir.http_router]: Router.{function} expects {expected} arguments, found {actual}"
    )
}

#[cfg(test)]
#[path = "router_test.rs"]
#[cfg(test)]
mod router_test;

/// Extracts a router receiver-method name from a call callee.
///
/// Inputs:
/// - `callee`: first child of a syntax call expression.
///
/// Output:
/// - Router builder method name when the callee is a field access.
///
/// Transformation:
/// - Reads `router.get(...)` and `Router.new().get(...)` as source-level
///   receiver calls without requiring the full typechecker.
pub(crate) fn router_receiver_method_name(callee: &SyntaxExprOutput) -> Option<&str> {
    if callee.kind != SyntaxExprKind::FieldAccess {
        return None;
    }
    let method = callee.text.as_deref()?;
    matches!(
        method,
        "get"
            | "post"
            | "put"
            | "patch"
            | "delete"
            | "head"
            | "options"
            | "sse"
            | "websocket"
            | "use"
            | "map_response"
            | "fallback"
            | "error"
            | "overload"
            | "lifecycle"
            | "group"
    )
    .then_some(method)
}
