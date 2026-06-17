//! CoreIR pattern lowering for CoreIR Erlang emission.
//!
//! Inputs:
//! - CoreIR parameter and destructuring patterns from the formal compiler path.
//!
//! Outputs:
//! - Erlang AST patterns for the currently supported CoreIR subset.
//!
//! Transformations:
//! - Converts backend-neutral CoreIR pattern shapes into BEAM-native pattern
//!   forms while preserving Erlang variable hygiene.

use terlan_typeck::CorePattern;

use super::super::erl::ErlPattern;
use super::super::sanitize_erlang_var;

/// Lowers CoreIR lambda parameter patterns into Erlang patterns.
///
/// Inputs:
/// - `patterns`: CoreIR lambda parameter patterns.
///
/// Output:
/// - Erlang patterns for the currently supported CoreIR pattern subset.
/// - `None` when a parameter uses a pattern outside the backend subset.
///
/// Transformation:
/// - Converts supported Core patterns into backend Erlang patterns without
///   introducing match helpers.
pub(super) fn lower_core_patterns_to_erlang(patterns: &[CorePattern]) -> Option<Vec<ErlPattern>> {
    patterns.iter().map(lower_core_pattern_to_erlang).collect()
}

/// Lowers one CoreIR lambda parameter pattern into an Erlang pattern.
///
/// Inputs:
/// - `pattern`: CoreIR lambda parameter pattern.
///
/// Output:
/// - `Some(ErlPattern)` for direct variable, wildcard, literal, tuple, list,
///   and list-cons patterns.
/// - `None` for pattern forms outside the current CoreIR Erlang subset.
///
/// Transformation:
/// - Preserves direct parameter binding names with Erlang variable hygiene,
///   maps wildcard parameters to `_`, and recursively lowers simple
///   destructuring patterns that Erlang can represent natively.
pub(super) fn lower_core_pattern_to_erlang(pattern: &CorePattern) -> Option<ErlPattern> {
    match pattern {
        CorePattern::Var(name) => Some(ErlPattern::Var(sanitize_erlang_var(name))),
        CorePattern::Wildcard => Some(ErlPattern::Wildcard),
        CorePattern::Int(value) => Some(ErlPattern::Int(*value)),
        CorePattern::Float(value) => Some(ErlPattern::Float(value.clone())),
        CorePattern::Atom(value) => Some(ErlPattern::Atom(value.clone())),
        CorePattern::Tuple(items) => Some(ErlPattern::Tuple(lower_core_patterns_to_erlang(items)?)),
        CorePattern::List(items) => Some(ErlPattern::List(lower_core_patterns_to_erlang(items)?)),
        CorePattern::ListCons { head, tail } => Some(ErlPattern::ListCons(
            Box::new(lower_core_pattern_to_erlang(head)?),
            Box::new(lower_core_pattern_to_erlang(tail)?),
        )),
        _ => None,
    }
}
