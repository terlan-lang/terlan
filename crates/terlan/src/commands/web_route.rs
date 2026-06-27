use std::path::Path;

/// Returns whether text is an ASCII Terlan identifier.
///
/// Inputs:
/// - `value`: candidate identifier text.
///
/// Output:
/// - `true` when the text starts with an ASCII letter or underscore and
///   continues with ASCII letters, digits, or underscores.
///
/// Transformation:
/// - Applies the manifest-level identifier subset needed for route targets
///   and route parameter names without invoking the full source parser.
pub(crate) fn is_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

/// Returns whether text is a route capture binding name.
///
/// Inputs:
/// - `value`: candidate route capture name from `:name` or `{name:Type}`.
///
/// Output:
/// - `true` when the text starts with an ASCII lowercase letter and continues
///   with ASCII letters, digits, or underscores.
///
/// Transformation:
/// - Applies the source-level binding subset used by route handlers. Capture
///   names become handler parameter names, so wildcard `*` is the only
///   non-binding capture name accepted by the router contract.
fn is_route_param_name(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_lowercase() {
        return false;
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

/// Validates one dynamic route pattern.
///
/// Inputs:
/// - `route`: absolute route pattern from source or the web manifest.
///
/// Output:
/// - `Ok(())` when the pattern belongs to the 0.0.5 router surface.
/// - Stable `error[serve_package]` diagnostic otherwise.
///
/// Transformation:
/// - Accepts exact segments, `:param` segments, typed `{param:Type}` segments,
///   a final `*` wildcard, and the canonical global fallback route `*`, while
///   rejecting traversal, empty segments, malformed params, unsupported typed
///   route params, and non-final wildcards.
pub(crate) fn validate_route_pattern(route: &str) -> Result<(), String> {
    if route == "*" {
        return Ok(());
    }
    if route == "/" {
        return Ok(());
    }
    for component in Path::new(route.trim_start_matches('/')).components() {
        if matches!(
            component,
            std::path::Component::ParentDir
                | std::path::Component::RootDir
                | std::path::Component::Prefix(_)
        ) {
            return Err(format!(
                "error[serve_package]: unsafe handler route `{route}`"
            ));
        }
    }
    let segments = route.trim_matches('/').split('/').collect::<Vec<_>>();
    for (index, segment) in segments.iter().enumerate() {
        if segment.is_empty() {
            return Err(format!(
                "error[serve_package]: handler route `{route}` contains an empty segment"
            ));
        }
        if *segment == "*" {
            if index + 1 != segments.len() {
                return Err(format!(
                    "error[serve_package]: wildcard in handler route `{route}` must be the final segment"
                ));
            }
            continue;
        }
        if let Some(name) = segment.strip_prefix(':') {
            if name.is_empty() || !is_route_param_name(name) {
                return Err(format!(
                    "error[serve_package]: invalid route parameter `:{name}` in `{route}`"
                ));
            }
        } else if let Some((name, type_name)) = typed_route_param_segment(segment) {
            if !is_route_param_name(name) || !is_identifier(type_name) {
                return Err(format!(
                    "error[serve_package]: invalid typed route parameter `{segment}` in `{route}`"
                ));
            }
            if !is_supported_route_param_type(type_name) {
                return Err(format!(
                    "error[serve_package]: unsupported route parameter type `{type_name}` in `{route}`"
                ));
            }
        } else if segment.starts_with('{') || segment.ends_with('}') {
            return Err(format!(
                "error[serve_package]: invalid typed route parameter `{segment}` in `{route}`"
            ));
        } else if segment.contains('*') || segment.contains(':') {
            return Err(format!(
                "error[serve_package]: invalid handler route segment `{segment}` in `{route}`"
            ));
        }
    }
    Ok(())
}

/// Returns whether a route parameter type can be decoded by the 0.0.5 server.
///
/// Inputs:
/// - `type_name`: type name from a typed route parameter.
///
/// Output:
/// - `true` for route parameter types with implemented local serve decoding.
///
/// Transformation:
/// - Keeps route pattern validation aligned with the BEAM handler bridge so
///   manifests cannot claim typed route params that `terlc serve` cannot
///   convert.
pub(crate) fn is_supported_route_param_type(type_name: &str) -> bool {
    matches!(type_name, "String" | "Int" | "Bool")
}

/// Builds an ambiguity key for one route pattern.
///
/// Inputs:
/// - `route`: manifest or source route pattern.
///
/// Output:
/// - Normalized pattern where parameter names are erased.
///
/// Transformation:
/// - Preserves exact and wildcard segments, replacing every `:name` or
///   `{name:Type}` with a parameter placeholder so same-shape parameter routes
///   collide during validation.
pub(crate) fn route_ambiguity_key(route: &str) -> Result<String, String> {
    validate_route_pattern(route)?;
    if route == "*" || route == "/*" {
        return Ok("*".to_string());
    }
    if route == "/" {
        return Ok("/".to_string());
    }
    let normalized = route_segments(route)
        .into_iter()
        .map(|segment| {
            if segment.starts_with(':') || typed_route_param_segment(segment).is_some() {
                ":"
            } else {
                segment
            }
        })
        .collect::<Vec<_>>()
        .join("/");
    Ok(format!("/{normalized}"))
}

/// Returns the ordered capture names declared by one route pattern.
///
/// Inputs:
/// - `route`: manifest or source route pattern.
///
/// Output:
/// - Ordered route capture names.
/// - Stable route validation diagnostic when the route is malformed.
///
/// Transformation:
/// - Reuses the canonical route validator, extracts `:name` and `{name:Type}`
///   captures in path order, and treats slash-wildcard routes as a final `*`
///   capture. The canonical `*` fallback route intentionally has no capture
///   because it is used as a catch-all handler, not a path-value handler.
pub(crate) fn route_param_names(route: &str) -> Result<Vec<String>, String> {
    Ok(route_param_types(route)?
        .into_iter()
        .map(|(name, _type_name)| name)
        .collect())
}

/// Returns the ordered capture names and types declared by one route pattern.
///
/// Inputs:
/// - `route`: manifest or source route pattern.
///
/// Output:
/// - Ordered `(name, type)` route captures.
/// - Stable route validation diagnostic when the route is malformed.
///
/// Transformation:
/// - Reuses the canonical route validator. Colon params and slash wildcards
///   default to `String`; typed brace params preserve their declared type so
///   build-time handler validation and serve-time argument decoding share the
///   same route contract.
pub(crate) fn route_param_types(route: &str) -> Result<Vec<(String, String)>, String> {
    validate_route_pattern(route)?;
    if route == "*" {
        return Ok(Vec::new());
    }
    Ok(route_segments(route)
        .into_iter()
        .filter_map(|segment| {
            if segment == "*" {
                Some(("*".to_string(), "String".to_string()))
            } else if let Some(name) = segment.strip_prefix(':') {
                Some((name.to_string(), "String".to_string()))
            } else {
                typed_route_param_segment(segment)
                    .map(|(name, type_name)| (name.to_string(), type_name.to_string()))
            }
        })
        .collect())
}

/// Splits a URL path or route pattern into non-empty path segments.
///
/// Inputs:
/// - `path`: absolute URL path or route pattern.
///
/// Output:
/// - Trimmed path segments with no leading slash.
///
/// Transformation:
/// - Treats `/` and empty strings as no segments. Query strings must already
///   be removed by the caller.
pub(crate) fn route_segments(path: &str) -> Vec<&str> {
    path.trim_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect()
}

/// Parses one typed route parameter segment.
///
/// Inputs:
/// - `segment`: route path segment such as `{id:Int}`.
///
/// Output:
/// - Parameter name and type text when the segment is exactly typed-param
///   shaped.
/// - `None` when the segment is not a typed route parameter.
///
/// Transformation:
/// - Removes surrounding braces, splits once at `:`, and rejects empty or
///   nested pieces before caller-level identifier validation.
pub(crate) fn typed_route_param_segment(segment: &str) -> Option<(&str, &str)> {
    let inner = segment.strip_prefix('{')?.strip_suffix('}')?;
    if inner.contains('{') || inner.contains('}') {
        return None;
    }
    let (name, type_name) = inner.split_once(':')?;
    if name.is_empty() || type_name.is_empty() || type_name.contains(':') {
        return None;
    }
    Some((name, type_name))
}

#[cfg(test)]
#[path = "web_route_test.rs"]
mod web_route_test;
