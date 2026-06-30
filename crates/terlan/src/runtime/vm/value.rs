use std::collections::HashMap;

use crate::terlan_typeck::{CoreExpr, CorePattern};

/// Runtime value produced by the compiler-owned Rust VM evaluator.
///
/// Inputs:
/// - Constructed from supported CoreIR expressions.
///
/// Output:
/// - A backend-neutral value that can be rendered for the public REPL.
///
/// Transformation:
/// - Keeps VM execution independent from BEAM/Erlang runtime values while the
///   evaluator grows toward full CoreIR coverage.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ReplValue {
    Unit,
    Int(i64),
    Float(String),
    String(String),
    Atom(String),
    Bool(bool),
    Type(String),
    Tuple(Vec<ReplValue>),
    List(Vec<ReplValue>),
    Map(Vec<(ReplValue, ReplValue)>),
    Set(Vec<ReplValue>),
    Iterator { items: Vec<ReplValue>, index: usize },
    Closure(ReplClosure),
}

/// Captured anonymous function value for the compiler-owned Rust VM evaluator.
///
/// Inputs:
/// - `params`: CoreIR lambda parameter patterns.
/// - `body`: CoreIR body evaluated when the closure is applied.
/// - `env`: lexical environment captured when the lambda expression evaluated.
///
/// Output:
/// - Stored inside `ReplValue::Closure` until a function-value call applies it.
///
/// Transformation:
/// - Preserves enough lexical state for REPL lambdas without lowering through a
///   target runtime or exposing backend function values.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ReplClosure {
    pub(super) params: Vec<CorePattern>,
    pub(super) body: CoreExpr,
    pub(super) env: HashMap<String, ReplValue>,
}

impl ReplValue {
    /// Renders a VM value with Terlan source-facing spelling.
    ///
    /// Inputs:
    /// - `self`: evaluated backend-neutral value.
    ///
    /// Output:
    /// - Stable text shown in text-mode REPL result events.
    ///
    /// Transformation:
    /// - Converts primitive and aggregate values to Terlan-facing syntax,
    ///   keeping `Unit`, `true`, and `false` distinct from backend atoms.
    pub(crate) fn render(&self) -> String {
        match self {
            Self::Unit => "Unit".to_string(),
            Self::Int(value) => value.to_string(),
            Self::Float(value) => value.clone(),
            Self::String(value) => format!("\"{}\"", escape_string(value)),
            Self::Atom(value) => format!("Atom[\"{}\"]", escape_string(value)),
            Self::Bool(value) => value.to_string(),
            Self::Type(value) => value.clone(),
            Self::Tuple(items) => {
                let rendered = items
                    .iter()
                    .map(Self::render)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{{{rendered}}}")
            }
            Self::List(items) => {
                let rendered = items
                    .iter()
                    .map(Self::render)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("[{rendered}]")
            }
            Self::Map(entries) => {
                let rendered = entries
                    .iter()
                    .map(|(key, value)| format!("{} => {}", key.render(), value.render()))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{{{rendered}}}")
            }
            Self::Set(items) => {
                let rendered = items
                    .iter()
                    .map(Self::render)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("Set({rendered})")
            }
            Self::Iterator { items, index } => {
                format!(
                    "<iterator index={index} remaining={}>",
                    items.len().saturating_sub(*index)
                )
            }
            Self::Closure(_) => "<function>".to_string(),
        }
    }
}

/// Returns whether a name is an implicit target-neutral type value.
///
/// Inputs:
/// - `name`: source variable name encountered in CoreIR.
///
/// Output:
/// - `true` for type names admitted into the implicit prelude.
///
/// Transformation:
/// - Recognizes only compiler-backed type values. Standard-library algebraic
///   types and collections remain ordinary imports.
pub(super) fn is_implicit_type_name(name: &str) -> bool {
    matches!(
        name,
        "Unit" | "Bool" | "Int" | "Float" | "String" | "Atom" | "Type"
    )
}

/// Computes the REPL-facing type value for an evaluated value.
///
/// Inputs:
/// - `value`: already evaluated REPL value.
///
/// Output:
/// - Source-facing type text such as `Int`, `String`, or `List[Int]`.
///
/// Transformation:
/// - Classifies runtime evaluator values without target-runtime reflection.
///   Aggregate types are rendered conservatively while the full language-level
///   type-value model is still being implemented.
pub(crate) fn type_of_value(value: &ReplValue) -> String {
    match value {
        ReplValue::Unit => "Unit".to_string(),
        ReplValue::Int(_) => "Int".to_string(),
        ReplValue::Float(_) => "Float".to_string(),
        ReplValue::String(_) => "String".to_string(),
        ReplValue::Atom(_) => "Atom".to_string(),
        ReplValue::Bool(_) => "Bool".to_string(),
        ReplValue::Type(_) => "Type".to_string(),
        ReplValue::Closure(_) => "Function".to_string(),
        ReplValue::Tuple(items) => {
            let types = items
                .iter()
                .map(type_of_value)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{{{types}}}")
        }
        ReplValue::List(items) => list_type_of_values(items),
        ReplValue::Map(_) => "Map[Dynamic, Dynamic]".to_string(),
        ReplValue::Set(items) => {
            let list_type = list_type_of_values(items);
            let inner = list_type
                .strip_prefix("List[")
                .and_then(|value| value.strip_suffix(']'))
                .unwrap_or("Dynamic");
            format!("Set[{inner}]")
        }
        ReplValue::Iterator { .. } => "Iterator[Dynamic]".to_string(),
    }
}

/// Computes the REPL-facing list type for evaluated list items.
///
/// Inputs:
/// - `items`: evaluated list elements.
///
/// Output:
/// - `List[T]` when all elements share a type, otherwise `List[Dynamic]`.
///
/// Transformation:
/// - Keeps list type rendering predictable without introducing union type
///   rendering into the REPL evaluator prematurely.
fn list_type_of_values(items: &[ReplValue]) -> String {
    let Some(first) = items.first() else {
        return "List[Dynamic]".to_string();
    };
    let first_type = type_of_value(first);
    if items.iter().all(|item| type_of_value(item) == first_type) {
        format!("List[{first_type}]")
    } else {
        "List[Dynamic]".to_string()
    }
}

/// Normalizes a CoreIR string payload into a runtime string value.
///
/// Inputs:
/// - `value`: CoreIR `Binary` payload.
///
/// Output:
/// - Runtime string without source quotes when quotes were preserved.
///
/// Transformation:
/// - Handles both raw and quoted payloads so the evaluator can tolerate the
///   transitional CoreIR string representation.
pub(super) fn normalize_core_string(value: &str) -> String {
    value
        .strip_prefix('"')
        .and_then(|inner| inner.strip_suffix('"'))
        .unwrap_or(value)
        .replace("\\\"", "\"")
        .replace("\\n", "\n")
}

/// Escapes a string for REPL source-style rendering.
///
/// Inputs:
/// - `value`: runtime string.
///
/// Output:
/// - Escaped string payload without surrounding quotes.
///
/// Transformation:
/// - Escapes the small set of characters needed by REPL display tests.
fn escape_string(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}
