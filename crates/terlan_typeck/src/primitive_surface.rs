use terlan_syntax::{SyntaxExprKind, SyntaxExprOutput};

use crate::{FunctionScheme, Type};

/// Infers the compiler-backed `Unit` singleton value.
///
/// Inputs:
/// - `name`: source identifier from a variable expression.
///
/// Output:
/// - `Some(Unit)` for the exact source spelling `Unit`.
/// - `None` for all other names, including lowercase `unit`.
///
/// Transformation:
/// - Gives `Unit` value position the same named type as the source return
///   annotation `Unit`, without admitting lowercase `unit` as a built-in value.
pub(super) fn infer_implicit_unit_value(name: &str) -> Option<Type> {
    (name == "Unit").then(|| Type::Named {
        module: None,
        name: "Unit".to_string(),
        args: Vec::new(),
    })
}

/// Infers compiler-backed implicit type names used as values.
///
/// Inputs:
/// - `name`: source identifier from a variable expression.
///
/// Output:
/// - `Some(Type)` when the identifier is one of the target-neutral implicit
///   type values admitted into the current implicit prelude.
/// - `None` for ordinary value names.
///
/// Transformation:
/// - Gives names such as `Int` and `String` the expression type `Type` so calls
///   like `is_type(value, Int)` can be checked without importing std modules.
///   `Unit` is intentionally value-first in expression position because it is
///   both the type name and the unit value.
pub(super) fn infer_implicit_type_value(name: &str) -> Option<Type> {
    is_implicit_type_value_name(name).then(|| Type::Named {
        module: None,
        name: "Type".to_string(),
        args: Vec::new(),
    })
}

/// Checks whether a name is an implicit target-neutral type value.
///
/// Inputs:
/// - `name`: source identifier.
///
/// Output:
/// - `true` for the minimal implicit prelude's type-value names.
///
/// Transformation:
/// - Keeps the implicit type-value set closed and separate from imported
///   standard-library types such as `Option`, `Result`, `List`, `Map`, or `Set`.
///   `Unit` is excluded here so `pub main(): Unit -> Unit.` keeps working as a
///   value expression.
pub(super) fn is_implicit_type_value_name(name: &str) -> bool {
    matches!(name, "Bool" | "Int" | "Float" | "String" | "Atom" | "Type")
}

/// Checks whether a name is an uppercase spelling reserved for diagnostics.
///
/// Inputs:
/// - `name`: source identifier from expression position.
///
/// Output:
/// - `true` for `True` or `False`.
///
/// Transformation:
/// - Keeps the boolean literal rule explicit: lowercase `true` and
///   `false` are built-in values, while uppercase spellings are constructor-like
///   names that must be declared before use.
pub(super) fn is_reserved_uppercase_bool_literal_spelling(name: &str) -> bool {
    matches!(name, "True" | "False")
}

/// Checks whether a name is the rejected lowercase unit spelling.
///
/// Inputs:
/// - `name`: source identifier or atom-like expression text.
///
/// Output:
/// - `true` for the exact source spelling `unit`.
///
/// Transformation:
/// - Enforces the rule that `Unit` is the unit type and value while
///   lowercase `unit` is not a built-in source-level synonym.
pub(super) fn is_reserved_lowercase_unit_spelling(name: &str) -> bool {
    name == "unit"
}

/// Reports whether an atom expression came from canonical `Atom["name"]`.
///
/// Inputs:
/// - `expr`: syntax-output expression node.
///
/// Output:
/// - `true` when the node is an atom with preserved canonical raw source.
///
/// Transformation:
/// - Uses the syntax-output raw field as the phase boundary marker separating
///   explicit language-neutral atom values from bare atom-like spellings.
pub(super) fn is_explicit_atom_literal_expr(expr: &SyntaxExprOutput) -> bool {
    expr.kind == SyntaxExprKind::Atom
        && expr
            .raw
            .as_deref()
            .is_some_and(|raw| raw.trim_start().starts_with("Atom["))
}

/// Returns a function scheme for a primitive receiver method.
///
/// Inputs:
/// - `receiver_type`: inferred receiver type.
/// - `method`: receiver method name.
/// - `arg_count`: number of non-receiver arguments.
///
/// Output:
/// - Function scheme for supported primitive receiver methods.
/// - `None` when receiver type, method, or arity is not supported.
///
/// Transformation:
/// - Encodes the selected primitive receiver-method surface as ordinary
///   parameter and return types for the existing call inference engine.
pub(super) fn primitive_receiver_method_scheme(
    receiver_type: &Type,
    method: &str,
    arg_count: usize,
) -> Option<FunctionScheme> {
    if matches!(receiver_type, Type::Int | Type::LiteralInt(_)) {
        return match (method, arg_count) {
            ("to_string", 0) => Some(FunctionScheme {
                params: Vec::new(),
                ret: Type::Binary,
                generic_params: Vec::new(),
                bounds: Vec::new(),
            }),
            _ => None,
        };
    }

    if matches!(receiver_type, Type::Float) {
        return match (method, arg_count) {
            ("to_string", 0) => Some(FunctionScheme {
                params: Vec::new(),
                ret: Type::Binary,
                generic_params: Vec::new(),
                bounds: Vec::new(),
            }),
            _ => None,
        };
    }

    if !matches!(receiver_type, Type::Binary | Type::Dynamic) {
        return None;
    }

    let binary = Type::Binary;
    match (method, arg_count) {
        ("equal", 1) | ("contains", 1) | ("starts_with", 1) | ("ends_with", 1) => {
            Some(FunctionScheme {
                params: vec![binary],
                ret: Type::Bool,
                generic_params: Vec::new(),
                bounds: Vec::new(),
            })
        }
        ("compare", 1) => Some(FunctionScheme {
            params: vec![binary],
            ret: Type::Union(vec![
                Type::LiteralAtom("lt".to_string()),
                Type::LiteralAtom("eq".to_string()),
                Type::LiteralAtom("gt".to_string()),
            ]),
            generic_params: Vec::new(),
            bounds: Vec::new(),
        }),
        ("append", 1) => Some(FunctionScheme {
            params: vec![binary],
            ret: Type::Binary,
            generic_params: Vec::new(),
            bounds: Vec::new(),
        }),
        ("from_string", 0) => Some(FunctionScheme {
            params: Vec::new(),
            ret: structural_option_type(Type::Binary),
            generic_params: Vec::new(),
            bounds: Vec::new(),
        }),
        ("is_empty", 0) => Some(FunctionScheme {
            params: Vec::new(),
            ret: Type::Bool,
            generic_params: Vec::new(),
            bounds: Vec::new(),
        }),
        ("replace", 2) => Some(FunctionScheme {
            params: vec![binary.clone(), binary],
            ret: Type::Binary,
            generic_params: Vec::new(),
            bounds: Vec::new(),
        }),
        ("split", 1) => Some(FunctionScheme {
            params: vec![binary],
            ret: Type::List(Box::new(Type::Binary)),
            generic_params: Vec::new(),
            bounds: Vec::new(),
        }),
        ("split_once", 1) => Some(FunctionScheme {
            params: vec![binary],
            ret: structural_option_type(Type::Tuple(vec![Type::Binary, Type::Binary])),
            generic_params: Vec::new(),
            bounds: Vec::new(),
        }),
        ("length", 0) | ("byte_size", 0) => Some(FunctionScheme {
            params: Vec::new(),
            ret: Type::Int,
            generic_params: Vec::new(),
            bounds: Vec::new(),
        }),
        ("to_string", 0)
        | ("lowercase", 0)
        | ("uppercase", 0)
        | ("trim", 0)
        | ("trim_start", 0)
        | ("trim_end", 0) => Some(FunctionScheme {
            params: Vec::new(),
            ret: Type::Binary,
            generic_params: Vec::new(),
            bounds: Vec::new(),
        }),
        _ => None,
    }
}

/// Returns source parameter names for primitive receiver methods.
///
/// Inputs:
/// - `receiver_type`: inferred receiver type.
/// - `method`: receiver method name.
/// - `arg_count`: number of non-receiver call arguments.
///
/// Output:
/// - Parameter names for the primitive method when the receiver/method/arity is
///   supported.
/// - `None` when the primitive method is not part of the compiler-owned scalar
///   surface.
///
/// Transformation:
/// - Reuses primitive receiver method support as the gate, then supplies names
///   used by named-argument validation and argument reordering.
pub(super) fn primitive_receiver_method_param_names(
    receiver_type: &Type,
    method: &str,
    arg_count: usize,
) -> Option<Vec<&'static str>> {
    primitive_receiver_method_scheme(receiver_type, method, arg_count)?;
    primitive_receiver_method_arg_names(method, arg_count)
}

/// Returns source argument names for a primitive receiver method shape.
///
/// Inputs:
/// - `method`: receiver method name.
/// - `arg_count`: number of non-receiver call arguments.
///
/// Output:
/// - Parameter names for compiler-owned primitive receiver method arguments.
///
/// Transformation:
/// - Provides the method/arity portion of primitive receiver metadata so type
///   inference and CoreIR lowering can share named-argument ordering rules.
pub(super) fn primitive_receiver_method_arg_names(
    method: &str,
    arg_count: usize,
) -> Option<Vec<&'static str>> {
    match (method, arg_count) {
        ("equal", 1) | ("compare", 1) => Some(vec!["other"]),
        ("append", 1) => Some(vec!["suffix"]),
        ("contains", 1) => Some(vec!["pattern"]),
        ("starts_with", 1) => Some(vec!["prefix"]),
        ("ends_with", 1) => Some(vec!["suffix"]),
        ("replace", 2) => Some(vec!["pattern", "replacement"]),
        ("split", 1) | ("split_once", 1) => Some(vec!["separator"]),
        (_, 0) => Some(Vec::new()),
        _ => None,
    }
}

/// Builds the structural representation of `Option[T]` for inference.
///
/// Inputs:
/// - `inner`: contained value type.
///
/// Output:
/// - Union type equivalent to `Atom["none"] | {Atom["some"], inner}`.
///
/// Transformation:
/// - Expands the public `std.core.Option.Option[T]` alias into its runtime
///   shape so primitive intrinsic return types can unify with APIs that expect
///   an expanded option alias.
pub(super) fn structural_option_type(inner: Type) -> Type {
    Type::Union(vec![
        Type::Tuple(vec![Type::LiteralAtom("some".to_string()), inner]),
        Type::LiteralAtom("none".to_string()),
    ])
}

/// Returns the narrowed type implied by a built-in guard predicate.
///
/// Inputs:
/// - `callee_name`: guard function name from a condition expression.
///
/// Output:
/// - Narrowed type for supported guard predicates.
/// - `None` for ordinary calls that do not narrow source variables.
///
/// Transformation:
/// - Maps guard predicate names onto the internal type representation consumed
///   by branch-local refinement.
pub(super) fn guard_narrow_type(callee_name: &str) -> Option<Type> {
    match callee_name {
        "is_integer" => Some(Type::Int),
        "is_binary" => Some(Type::Binary),
        "is_atom" => Some(Type::Atom),
        "is_boolean" => Some(Type::Bool),
        "is_list" => Some(Type::List(Box::new(Type::Dynamic))),
        "is_map" => Some(Type::Named {
            module: None,
            name: "Map".to_string(),
            args: Vec::new(),
        }),
        "is_tuple" => Some(Type::Tuple(Vec::new())),
        _ => None,
    }
}
