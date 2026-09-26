//! Static string, function, and WebSocket values in router sources.

use super::*;

pub(super) fn source_string_constants(
    sources: &[WebRouteSourceArtifact],
) -> Result<HashMap<(String, String), String>, String> {
    let mut constants = HashMap::new();
    for source_artifact in sources {
        let source = fs::read_to_string(&source_artifact.source_path).map_err(|err| {
            format!(
                "cannot read source {} for web constant discovery: {err}",
                source_artifact.source_path
            )
        })?;
        let syntax = parse_module_as_syntax_output(&source).map_err(|err| {
            format!(
                "cannot parse source {} for web constant discovery: {err:?}",
                source_artifact.source_path
            )
        })?;
        for declaration in &syntax.declarations {
            let SyntaxDeclarationPayload::Function {
                name,
                params,
                clauses,
                ..
            } = &declaration.payload
            else {
                continue;
            };
            if !params.is_empty() || clauses.len() != 1 {
                continue;
            }
            if let Some(value) = string_literal_from_expr(&clauses[0].body) {
                constants.insert((source_artifact.module.clone(), name.clone()), value);
            }
        }
    }
    Ok(constants)
}

/// Builds a WebSocket artifact from a declarative source module.
///
/// Inputs:
/// - `source_artifact`: route-source module reference.
/// - `syntax`: parsed module syntax.
/// - `source`: source context used for spans.
/// - `constants`: known constant string functions from route-source modules.
///
/// Output:
/// - A WebSocket artifact, no artifact for non-WebSocket modules, or a stable
///   validation error.
///
/// Transformation:
/// - Reads constant `route()` and `protocol()` functions and converts them into
///   package metadata consumed by the runtime server.
pub(super) fn websocket_from_source(
    source_artifact: &WebRouteSourceArtifact,
    syntax: &crate::terlan_syntax::SyntaxModuleOutput,
    source: &WebRouteSourceContext<'_>,
    constants: &HashMap<(String, String), String>,
) -> Result<Option<WebSocketArtifact>, String> {
    if !source_artifact.module.ends_with(".WebSocket") {
        return Ok(None);
    }
    let route_body = zero_arg_function_body(syntax, "route");
    let protocol_body = zero_arg_function_body(syntax, "protocol");
    if route_body.is_none() && protocol_body.is_none() {
        return Ok(None);
    }
    let route_body = route_body.ok_or_else(|| {
        format!(
            "error[web_router]: websocket module `{}` must define route(): String",
            source_artifact.module
        )
    })?;
    let protocol_body = protocol_body.ok_or_else(|| {
        format!(
            "error[web_router]: websocket module `{}` must define protocol(): String",
            source_artifact.module
        )
    })?;
    let route = constant_string_from_expr(route_body, constants).ok_or_else(|| {
        format!(
            "error[web_router]: websocket `{}` route() must return a constant string",
            source_artifact.module
        )
    })?;
    let protocol = constant_string_from_expr(protocol_body, constants).ok_or_else(|| {
        format!(
            "error[web_router]: websocket `{}` protocol() must return a constant string",
            source_artifact.module
        )
    })?;
    validate_route_pattern(&route).map_err(|message| {
        message
            .to_string()
            .replacen("error[serve_package]", "error[web_router]", 1)
    })?;
    if protocol.trim().is_empty() {
        return Err(format!(
            "error[web_router]: websocket `{}` protocol() cannot be empty",
            source_artifact.module
        ));
    }
    Ok(Some(WebSocketArtifact {
        module: source_artifact.module.clone(),
        route,
        protocol,
        source: Some(source_span_for_expr(source, route_body)),
    }))
}

/// Finds the body of a single-clause zero-argument function.
///
/// Inputs:
/// - `syntax`: parsed module syntax.
/// - `target`: function name to locate.
///
/// Output:
/// - Body expression when a matching simple function exists.
///
/// Transformation:
/// - Filters declarations to the route-metadata function shape.
pub(super) fn zero_arg_function_body<'a>(
    syntax: &'a crate::terlan_syntax::SyntaxModuleOutput,
    target: &str,
) -> Option<&'a SyntaxExprOutput> {
    syntax.declarations.iter().find_map(|declaration| {
        let SyntaxDeclarationPayload::Function {
            name,
            params,
            clauses,
            ..
        } = &declaration.payload
        else {
            return None;
        };
        if name == target && params.is_empty() && clauses.len() == 1 {
            Some(&clauses[0].body)
        } else {
            None
        }
    })
}

/// Resolves a constant string expression used by route metadata.
///
/// Inputs:
/// - `expr`: expression that should evaluate to a route/protocol string.
/// - `constants`: cross-module constant string function map.
///
/// Output:
/// - Constant string value when it can be determined statically.
///
/// Transformation:
/// - Accepts direct string literals and remote calls to previously discovered
///   zero-argument string constants.
pub(super) fn constant_string_from_expr(
    expr: &SyntaxExprOutput,
    constants: &HashMap<(String, String), String>,
) -> Option<String> {
    if let Some(value) = string_literal_from_expr(expr) {
        return Some(value);
    }
    if expr.kind != SyntaxExprKind::Call || expr.children.len() != 1 {
        return None;
    }
    let function = expr.children.first()?.text.as_deref()?;
    let remote = expr.remote.as_deref()?;
    remote_constant_string(remote, function, constants)
}

/// Extracts a string literal from a route metadata expression.
///
/// Inputs:
/// - `expr`: syntax expression to inspect.
///
/// Output:
/// - Literal string value when the expression is a supported string literal.
///
/// Transformation:
/// - Delegates to the shared router route-literal parser.
pub(super) fn string_literal_from_expr(expr: &SyntaxExprOutput) -> Option<String> {
    router_route_literal(expr)
}

/// Resolves a remote zero-argument constant string function.
///
/// Inputs:
/// - `remote`: remote module segment from the call.
/// - `function`: called function name.
/// - `constants`: discovered module/function constant string table.
///
/// Output:
/// - Constant string when exactly one matching remote function exists.
///
/// Transformation:
/// - Supports exact module matches and unambiguous suffix matches for local
///   route-source modules.
pub(super) fn remote_constant_string(
    remote: &str,
    function: &str,
    constants: &HashMap<(String, String), String>,
) -> Option<String> {
    if let Some(value) = constants.get(&(remote.to_string(), function.to_string())) {
        return Some(value.clone());
    }
    let suffix = format!(".{remote}");
    let mut matches = constants
        .iter()
        .filter(|((module, name), _)| name == function && module.ends_with(&suffix))
        .map(|(_, value)| value.clone());
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}
