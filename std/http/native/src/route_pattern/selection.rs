//! HTTP route selection and scalar argument admission, without manifest or VM types.

use super::{match_route_pattern, route_segments, typed_route_param_segment};
use terlan_runtime_abi::{BoundaryError, ErrorDomain, NativeValue};

/// A borrowed route and its decoded captures in declaration order.
#[derive(Debug, PartialEq, Eq)]
pub struct SelectedRoute<'a, T> {
    pub route: &'a T,
    pub params: Vec<(String, String)>,
}

/// Selects the most specific matching route for the exact method. HEAD falls
/// back to GET only when no HEAD route matches. Equal scores retain the last
/// declaration, matching source-router dispatch. Hosts own caching and storage.
pub fn select_route<'a, T>(
    routes: &'a [T],
    method: &str,
    path: &str,
    describe: impl Fn(&'a T) -> (&'a str, &'a str),
) -> Option<SelectedRoute<'a, T>> {
    let best = |method: &str| {
        routes
            .iter()
            .filter_map(|route| {
                let (candidate_method, pattern) = describe(route);
                if candidate_method != method {
                    return None;
                }
                match_route_pattern(pattern, path).map(|matched| {
                    (
                        matched.score,
                        SelectedRoute {
                            route,
                            params: matched.params,
                        },
                    )
                })
            })
            .max_by_key(|(score, _)| *score)
            .map(|(_, route)| route)
    };
    best(method).or_else(|| if method == "HEAD" { best("GET") } else { None })
}

/// Materializes one positional route capture according to its manifest type.
///
/// Untyped `:name` captures and wildcard captures remain strings. Typed
/// captures are converted only after route matching has validated their text,
/// so generated handler ABI validation sees the declared scalar type.
pub fn route_param_argument(
    pattern: &str,
    name: &str,
    value: &str,
) -> Result<NativeValue, BoundaryError> {
    let declared_type = route_segments(pattern)
        .into_iter()
        .find_map(|segment| {
            typed_route_param_segment(segment)
                .filter(|(declared_name, _)| *declared_name == name)
                .map(|(_, type_name)| type_name)
        })
        .unwrap_or("String");
    match declared_type {
        "String" => Ok(NativeValue::String(value.to_string())),
        "Int" => value.parse::<i64>().map(NativeValue::Int).map_err(|error| {
            BoundaryError::message(
                ErrorDomain::CommandExecution,
                "materialize HTTP route parameter",
                format!(
                    "error[serve.route_param]: typed route capture `{name}:Int` could not materialize `{value}`: {error}"
                ),
            )
        }),
        "Bool" => match value {
            "true" => Ok(NativeValue::Bool(true)),
            "false" => Ok(NativeValue::Bool(false)),
            _ => Err(BoundaryError::message(
                ErrorDomain::CommandExecution,
                "materialize HTTP route parameter",
                format!(
                    "error[serve.route_param]: typed route capture `{name}:Bool` could not materialize `{value}`"
                ),
            )),
        },
        other => Err(BoundaryError::message(
            ErrorDomain::CommandExecution,
            "materialize HTTP route parameter",
            format!(
                "error[serve.route_param]: unsupported typed route capture `{name}:{other}`"
            ),
        )),
    }
}

#[cfg(test)]
#[path = "selection_test.rs"]
mod tests;
