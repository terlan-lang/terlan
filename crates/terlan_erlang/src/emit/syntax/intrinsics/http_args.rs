use super::*;

/// Orders router route-builder arguments for function-style calls.
///
/// Inputs:
/// - `args`: source arguments supplied to a route helper.
/// - `arg_names`: optional names parallel to `args`.
///
/// Output:
/// - Router, route pattern, and handler arguments in canonical order.
/// - `None` when names are unknown or required arguments are missing.
///
/// Transformation:
/// - Accepts positional calls and named `router`, `pattern`, and `handler`
///   arguments so lowering can emit one stable route tuple shape.
pub(super) fn ordered_http_router_route_args<'a>(
    args: &'a [SyntaxExprOutput],
    arg_names: &'a [Option<String>],
) -> Option<(
    &'a SyntaxExprOutput,
    &'a SyntaxExprOutput,
    &'a SyntaxExprOutput,
)> {
    if !arg_names.iter().any(Option::is_some) {
        return Some((args.first()?, args.get(1)?, args.get(2)?));
    }
    let mut router = None;
    let mut pattern = None;
    let mut handler = None;
    for (index, arg) in args.iter().enumerate() {
        match arg_names.get(index).and_then(Option::as_deref) {
            Some("router") => router = Some(arg),
            Some("pattern") => pattern = Some(arg),
            Some("handler") => handler = Some(arg),
            Some(_) => return None,
            None if router.is_none() => router = Some(arg),
            None if pattern.is_none() => pattern = Some(arg),
            None if handler.is_none() => handler = Some(arg),
            None => return None,
        }
    }
    Some((router?, pattern?, handler?))
}

/// Orders router route-builder arguments for receiver-style calls.
///
/// Inputs:
/// - `args`: source arguments after the router receiver.
/// - `arg_names`: optional names parallel to `args`.
///
/// Output:
/// - Route pattern and handler arguments in canonical order.
/// - `None` when names are unknown or required arguments are missing.
///
/// Transformation:
/// - Removes the receiver from the argument model while preserving the same
///   route tuple shape used by function-style lowering.
pub(super) fn ordered_http_router_receiver_route_args<'a>(
    args: &'a [SyntaxExprOutput],
    arg_names: &'a [Option<String>],
) -> Option<(&'a SyntaxExprOutput, &'a SyntaxExprOutput)> {
    if !arg_names.iter().any(Option::is_some) {
        return Some((args.first()?, args.get(1)?));
    }
    let mut pattern = None;
    let mut handler = None;
    for (index, arg) in args.iter().enumerate() {
        match arg_names.get(index).and_then(Option::as_deref) {
            Some("pattern") => pattern = Some(arg),
            Some("handler") => handler = Some(arg),
            Some(_) => return None,
            None if pattern.is_none() => pattern = Some(arg),
            None if handler.is_none() => handler = Some(arg),
            None => return None,
        }
    }
    Some((pattern?, handler?))
}

/// Orders router handler-builder arguments for function-style calls.
///
/// Inputs:
/// - `args`: source arguments supplied to a handler helper.
/// - `arg_names`: optional names parallel to `args`.
/// - `handler_param`: accepted handler parameter name.
///
/// Output:
/// - Router and selected handler or middleware expression.
/// - `None` when names are unknown or required arguments are missing.
///
/// Transformation:
/// - Accepts positional calls and named `router` plus the expected handler
///   parameter so fallback, middleware, and error handlers share one path.
pub(super) fn ordered_http_router_handler_args<'a>(
    args: &'a [SyntaxExprOutput],
    arg_names: &'a [Option<String>],
    handler_param: &str,
) -> Option<(&'a SyntaxExprOutput, &'a SyntaxExprOutput)> {
    if !arg_names.iter().any(Option::is_some) {
        return Some((args.first()?, args.get(1)?));
    }
    let mut router = None;
    let mut handler = None;
    for (index, arg) in args.iter().enumerate() {
        match arg_names.get(index).and_then(Option::as_deref) {
            Some("router") => router = Some(arg),
            Some(name) if name == handler_param => handler = Some(arg),
            Some(_) => return None,
            None if router.is_none() => router = Some(arg),
            None if handler.is_none() => handler = Some(arg),
            None => return None,
        }
    }
    Some((router?, handler?))
}

/// Orders one handler argument for receiver-style router calls.
///
/// Inputs:
/// - `args`: source arguments after the router receiver.
/// - `arg_names`: optional names parallel to `args`.
/// - `handler_param`: accepted handler parameter name.
///
/// Output:
/// - Selected handler or middleware expression.
/// - `None` when named arguments do not include the expected handler name.
///
/// Transformation:
/// - Treats unnamed receiver calls as single-argument calls and named receiver
///   calls as explicit handler selection.
pub(super) fn ordered_http_router_receiver_handler_arg<'a>(
    args: &'a [SyntaxExprOutput],
    arg_names: &'a [Option<String>],
    handler_param: &str,
) -> Option<&'a SyntaxExprOutput> {
    if !arg_names.iter().any(Option::is_some) {
        return args.first();
    }
    args.iter().enumerate().find_map(|(index, arg)| {
        let name = arg_names.get(index).and_then(Option::as_deref)?;
        (name == handler_param).then_some(arg)
    })
}

/// Orders router group arguments for function-style calls.
///
/// Inputs:
/// - `args`: source arguments supplied to `Router.group`.
/// - `arg_names`: optional names parallel to `args`.
///
/// Output:
/// - Router, prefix, and configure callback expressions.
/// - `None` when names are unknown or required arguments are missing.
///
/// Transformation:
/// - Accepts positional calls and named `router`, `prefix`, and `configure`
///   arguments before route-group tuple lowering.
pub(super) fn ordered_http_router_group_args<'a>(
    args: &'a [SyntaxExprOutput],
    arg_names: &'a [Option<String>],
) -> Option<(
    &'a SyntaxExprOutput,
    &'a SyntaxExprOutput,
    &'a SyntaxExprOutput,
)> {
    if !arg_names.iter().any(Option::is_some) {
        return Some((args.first()?, args.get(1)?, args.get(2)?));
    }
    let mut router = None;
    let mut prefix = None;
    let mut configure = None;
    for (index, arg) in args.iter().enumerate() {
        match arg_names.get(index).and_then(Option::as_deref) {
            Some("router") => router = Some(arg),
            Some("prefix") => prefix = Some(arg),
            Some("configure") => configure = Some(arg),
            Some(_) => return None,
            None if router.is_none() => router = Some(arg),
            None if prefix.is_none() => prefix = Some(arg),
            None if configure.is_none() => configure = Some(arg),
            None => return None,
        }
    }
    Some((router?, prefix?, configure?))
}

/// Orders route-group arguments for receiver-style router calls.
///
/// Inputs:
/// - `args`: source arguments after the router receiver.
/// - `arg_names`: optional names parallel to `args`.
///
/// Output:
/// - Prefix and configure callback expressions.
/// - `None` when names are unknown or required arguments are missing.
///
/// Transformation:
/// - Removes the receiver from the argument model while preserving the same
///   group tuple shape used by function-style lowering.
pub(super) fn ordered_http_router_receiver_group_args<'a>(
    args: &'a [SyntaxExprOutput],
    arg_names: &'a [Option<String>],
) -> Option<(&'a SyntaxExprOutput, &'a SyntaxExprOutput)> {
    if !arg_names.iter().any(Option::is_some) {
        return Some((args.first()?, args.get(1)?));
    }
    let mut prefix = None;
    let mut configure = None;
    for (index, arg) in args.iter().enumerate() {
        match arg_names.get(index).and_then(Option::as_deref) {
            Some("prefix") => prefix = Some(arg),
            Some("configure") => configure = Some(arg),
            Some(_) => return None,
            None if prefix.is_none() => prefix = Some(arg),
            None if configure.is_none() => configure = Some(arg),
            None => return None,
        }
    }
    Some((prefix?, configure?))
}
