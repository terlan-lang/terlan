use std::collections::BTreeSet;
use std::path::PathBuf;

use terlan_syntax::{SyntaxDeclarationPayload, SyntaxModuleOutput};

/// Static route from a URL path to a zero-argument HTML handler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StaticRoute {
    pub(crate) path: String,
    pub(crate) handler: String,
}

/// Parsed static route declarations plus their declaration shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParsedStaticRoutes {
    pub(crate) routes: Vec<StaticRoute>,
    pub(crate) is_block: bool,
}

/// Discovers formal syntax-output static HTML entrypoints.
///
/// Inputs:
/// - `module`: formal syntax output module.
///
/// Output:
/// - Sorted public zero-argument functions returning `Html[Never]`.
///
/// Transformation:
/// - Filters syntax declaration payloads by visibility, arity, and return type.
pub(crate) fn discover_syntax_static_entrypoints(module: &SyntaxModuleOutput) -> Vec<String> {
    let mut entrypoints: Vec<String> = module
        .declarations
        .iter()
        .filter_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Function {
                name,
                params,
                return_type,
                is_public,
                ..
            } if *is_public
                && params.is_empty()
                && is_static_html_return_type(&return_type.text) =>
            {
                Some(name.clone())
            }
            _ => None,
        })
        .collect();
    entrypoints.sort();
    entrypoints
}

/// Discovers formal syntax-output static routes.
///
/// Inputs:
/// - `module`: formal syntax output module.
///
/// Output:
/// - Static routes or a validation error string.
///
/// Transformation:
/// - Scans syntax-output config declarations, parses singular or block route
///   syntax, enforces one route block, and rejects duplicate paths.
pub(crate) fn discover_syntax_static_routes(
    module: &SyntaxModuleOutput,
) -> Result<Vec<StaticRoute>, String> {
    let mut routes = Vec::new();
    let mut route_block_seen = false;
    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::Config { name, text, .. } = &declaration.payload else {
            continue;
        };
        if name != "static" {
            continue;
        }
        let parsed = parse_static_routes_text(text)?;
        if parsed.is_block {
            if route_block_seen {
                return Err("static routes must be declared in a single routes block".to_string());
            }
            route_block_seen = true;
        }
        routes.extend(parsed.routes);
    }
    reject_duplicate_static_routes(&routes)?;
    Ok(routes)
}

/// Parses static route text.
///
/// Inputs:
/// - `raw_text`: source text for a `static route` or `static routes` block.
///
/// Output:
/// - Parsed route declaration shape or an error string.
///
/// Transformation:
/// - Splits route text by whitespace and dispatches singular versus block
///   parsing.
pub(crate) fn parse_static_routes_text(raw_text: &str) -> Result<ParsedStaticRoutes, String> {
    let parts: Vec<&str> = raw_text.split_whitespace().collect();
    if parts.len() >= 4
        && parts[0] == "static"
        && matches!(parts[1], "route" | "routes")
        && parts[2] == "{"
    {
        return parse_static_routes_block_text(raw_text, &parts);
    }

    Ok(ParsedStaticRoutes {
        routes: vec![parse_static_route_parts_text(raw_text, &parts)?],
        is_block: false,
    })
}

/// Parses a singular static route declaration.
///
/// Inputs:
/// - `raw_text`: original route text for error reporting.
/// - `parts`: whitespace-split route tokens.
///
/// Output:
/// - One static route or an error string.
///
/// Transformation:
/// - Validates the expected token shape and normalizes quoted path literals.
fn parse_static_route_parts_text(raw_text: &str, parts: &[&str]) -> Result<StaticRoute, String> {
    if parts.len() != 7
        || parts[0] != "static"
        || parts[1] != "route"
        || parts[3] != "->"
        || parts[5] != "("
        || parts[6] != ")"
    {
        return Err(format!("invalid static route declaration `{}`", raw_text));
    }

    let path = normalize_static_route_path_literal(parts[2]);
    if !path.starts_with('/') {
        return Err(format!("static route path must start with `/`: `{}`", path));
    }

    Ok(StaticRoute {
        path,
        handler: parts[4].to_owned(),
    })
}

/// Parses a static routes block declaration.
///
/// Inputs:
/// - `raw_text`: original block text for error reporting.
/// - `parts`: whitespace-split block tokens.
///
/// Output:
/// - Parsed route block or an error string.
///
/// Transformation:
/// - Walks repeated path/handler entries inside the surrounding braces.
fn parse_static_routes_block_text(
    raw_text: &str,
    parts: &[&str],
) -> Result<ParsedStaticRoutes, String> {
    if parts.last() != Some(&"}") {
        return Err(format!("invalid static routes declaration `{}`", raw_text));
    }

    let mut routes = Vec::new();
    let mut index = 3;
    while index < parts.len() - 1 {
        if index + 5 >= parts.len() {
            return Err(format!("invalid static routes declaration `{}`", raw_text));
        }
        if parts[index + 1] != "->"
            || parts[index + 3] != "("
            || parts[index + 4] != ")"
            || parts[index + 5] != "."
        {
            return Err(format!("invalid static routes declaration `{}`", raw_text));
        }

        let path = normalize_static_route_path_literal(parts[index]);
        if !path.starts_with('/') {
            return Err(format!("static route path must start with `/`: `{}`", path));
        }

        routes.push(StaticRoute {
            path,
            handler: parts[index + 2].to_owned(),
        });
        index += 6;
    }

    Ok(ParsedStaticRoutes {
        routes,
        is_block: true,
    })
}

/// Rejects duplicate static route paths.
///
/// Inputs:
/// - `routes`: parsed static routes.
///
/// Output:
/// - `Ok(())` when all paths are unique, otherwise an error string.
///
/// Transformation:
/// - Tracks seen paths in a set and reports the first duplicate.
fn reject_duplicate_static_routes(routes: &[StaticRoute]) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    for route in routes {
        if !seen.insert(route.path.clone()) {
            return Err(format!("duplicate static route `{}`", route.path));
        }
    }
    Ok(())
}

/// Normalizes a static route path literal.
///
/// Inputs:
/// - `path`: raw path token, possibly single- or double-quoted.
///
/// Output:
/// - Path text without matching surrounding quotes.
///
/// Transformation:
/// - Removes one matching pair of quote delimiters when present.
fn normalize_static_route_path_literal(path: &str) -> String {
    path.strip_prefix('"')
        .and_then(|path| path.strip_suffix('"'))
        .or_else(|| {
            path.strip_prefix('\'')
                .and_then(|path| path.strip_suffix('\''))
        })
        .unwrap_or(path)
        .to_owned()
}

/// Validates formal syntax-output static route handlers.
///
/// Inputs:
/// - `module`: formal syntax output module.
/// - `routes`: static routes to validate.
///
/// Output:
/// - `Ok(())` when every route targets a zero-argument `Html[Never]` handler.
/// - `Err(String)` naming the first invalid handler.
///
/// Transformation:
/// - Resolves route handlers against syntax-output function declarations and
///   checks arity plus return type.
pub(crate) fn validate_syntax_static_route_handlers(
    module: &SyntaxModuleOutput,
    routes: &[StaticRoute],
) -> Result<(), String> {
    for route in routes {
        let Some((params, return_type)) =
            module
                .declarations
                .iter()
                .find_map(|declaration| match &declaration.payload {
                    SyntaxDeclarationPayload::Function {
                        name,
                        params,
                        return_type,
                        ..
                    } if name == &route.handler => Some((params, return_type)),
                    _ => None,
                })
        else {
            return Err(format!(
                "static route `{}` references unknown handler `{}`",
                route.path, route.handler
            ));
        };

        if !params.is_empty() {
            return Err(format!(
                "static route `{}` handler `{}` must not require arguments",
                route.path, route.handler
            ));
        }
        if !is_static_html_return_type(&return_type.text) {
            return Err(format!(
                "static route `{}` handler `{}` must return Html[Never]",
                route.path, route.handler
            ));
        }
    }

    Ok(())
}

/// Converts a route path into a relative output path.
///
/// Inputs:
/// - `route_path`: URL path from a static route declaration.
///
/// Output:
/// - Relative output path ending in `index.html`, or an error string.
///
/// Transformation:
/// - Maps `/` to `index.html`; otherwise maps path segments to directories and
///   rejects empty, current-directory, or parent-directory segments.
pub(crate) fn static_route_output_path(route_path: &str) -> Result<PathBuf, String> {
    if route_path == "/" {
        return Ok(PathBuf::from("index.html"));
    }
    if !route_path.starts_with('/') {
        return Err(format!(
            "static route path must start with `/`: `{}`",
            route_path
        ));
    }

    let mut output = PathBuf::new();
    for segment in route_path.trim_matches('/').split('/') {
        if segment.is_empty() || segment == "." || segment == ".." {
            return Err(format!("invalid static route path `{}`", route_path));
        }
        output.push(segment);
    }
    output.push("index.html");
    Ok(output)
}

/// Returns whether a type annotation is the static HTML route type.
///
/// Inputs:
/// - `type_text`: source spelling of a return type.
///
/// Output:
/// - `true` when the normalized type text is exactly `Html[Never]`.
///
/// Transformation:
/// - Removes whitespace before comparing the type spelling.
pub(crate) fn is_static_html_return_type(type_text: &str) -> bool {
    type_text
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>()
        == "Html[Never]"
}
