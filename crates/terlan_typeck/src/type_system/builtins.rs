use crate::{FunctionScheme, Type};

use super::parser::is_type_constructor_atom;

/// Looks up an implicit compiler builtin call.
///
/// Inputs:
/// - `name`: unqualified source-level call name.
/// - `arity`: number of call arguments.
///
/// Output:
/// - Function scheme for supported implicit builtins, otherwise `None`.
///
/// Transformation:
/// - Maps the small always-available builtin surface to typed function schemes.
pub(crate) fn builtin_call(name: &str, arity: usize) -> Option<FunctionScheme> {
    match (name, arity) {
        ("type_of", 1) => Some(FunctionScheme {
            params: vec![Type::Dynamic],
            ret: Type::Named {
                module: None,
                name: "Type".to_string(),
                args: Vec::new(),
            },
            generic_params: Vec::new(),
            bounds: Vec::new(),
        }),
        ("is_type", 2) => Some(FunctionScheme {
            params: vec![
                Type::Dynamic,
                Type::Named {
                    module: None,
                    name: "Type".to_string(),
                    args: Vec::new(),
                },
            ],
            ret: Type::Bool,
            generic_params: Vec::new(),
            bounds: Vec::new(),
        }),
        _ => None,
    }
}

/// Reports whether a call name used to be an implicit legacy builtin.
///
/// Inputs:
/// - `name`: local call name from source.
/// - `arity`: number of source arguments.
///
/// Output:
/// - `true` for legacy Erlang-shaped helper names that are no longer admitted
///   into Terlan's implicit prelude.
///
/// Transformation:
/// - Keeps the minimal implicit prelude closed while producing a clearer
///   diagnostic than a later backend failure for old helper spellings.
pub(crate) fn is_removed_implicit_builtin_call(name: &str, arity: usize) -> bool {
    matches!(
        (name, arity),
        ("integer_to_binary", 1)
            | ("is_integer", 1)
            | ("is_binary", 1)
            | ("is_atom", 1)
            | ("is_boolean", 1)
            | ("is_list", 1)
            | ("is_map", 1)
            | ("is_tuple", 1)
    )
}

/// Reports whether a bare atom payload is a supported literal atom.
///
/// Inputs:
/// - `name`: atom payload without source delimiters.
///
/// Output:
/// - `true` for built-in singleton atoms and valid constructor-style atoms.
///
/// Transformation:
/// - Keeps legacy boolean and nil payloads recognizable while delegating general
///   constructor-atom validation to `is_type_constructor_atom`.
pub(crate) fn is_literal_atom(name: &str) -> bool {
    matches!(name, "ok" | "error" | "true" | "false" | "nil") || is_type_constructor_atom(name)
}

/// Widens literal element types inferred inside list literals.
///
/// Inputs:
/// - `ty`: element type inferred from one list item.
///
/// Output:
/// - A list-compatible element type.
///
/// Transformation:
/// - Converts integer literals to `Int`, boolean atom literals to `Bool`, and
///   other atom literals to `Atom`.
pub(crate) fn widen_list_literal_element_type(ty: Type) -> Type {
    match ty {
        Type::LiteralInt(_) => Type::Int,
        Type::LiteralAtom(atom) => {
            if atom == "true" || atom == "false" {
                Type::Bool
            } else {
                Type::Atom
            }
        }
        other => other,
    }
}
