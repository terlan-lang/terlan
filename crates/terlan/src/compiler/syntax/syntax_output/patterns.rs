use serde::{Deserialize, Serialize};

use crate::terlan_syntax::parse_tree::{MapField, Pattern};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Serializable pattern node emitted by syntax output.
///
/// Inputs:
/// - Parsed pattern data.
///
/// Outputs:
/// - Backend-neutral pattern payload preserving kind, arity, text, children,
///   and fields.
///
/// Transformation:
/// - Converts parser pattern variants into stable contract data for typecheck
///   and backend lowering.
pub struct SyntaxPatternOutput {
    pub kind: SyntaxPatternKind,
    pub arity: usize,
    pub text: Option<String>,
    pub children: Vec<SyntaxPatternOutput>,
    pub fields: Vec<SyntaxPatternFieldOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Keyed pattern field emitted for map and record patterns.
///
/// Inputs:
/// - Parsed field key, required flag, and nested value pattern.
///
/// Outputs:
/// - Serializable field payload nested under a pattern output node.
///
/// Transformation:
/// - Boxes nested pattern output so recursive field values have stable
///   ownership.
pub struct SyntaxPatternFieldOutput {
    pub key: String,
    pub required: bool,
    pub value: Box<SyntaxPatternOutput>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Pattern kind tag used by serialized syntax output.
///
/// Inputs:
/// - Parser pattern variants and constructor-candidate detection.
///
/// Outputs:
/// - Stable snake-case pattern tags for syntax contracts and downstream phases.
///
/// Transformation:
/// - Decouples serialized pattern identity from parser enum names.
pub enum SyntaxPatternKind {
    Wildcard,
    Var,
    Int,
    Float,
    Atom,
    Tuple,
    List,
    ListCons,
    Constructor,
    Map,
    MapField,
    Ignore,
    Placeholder,
    Record,
}

/// Converts a parse-tree pattern into syntax-output metadata.
///
/// Inputs:
/// - `pattern`: parsed Terlan pattern tree.
///
/// Output:
/// - Serializable pattern output preserving kind, arity, text, child patterns,
///   and map/record fields.
///
/// Transformation:
/// - Rewrites uppercase atom-leading tuples into constructor-pattern nodes so
///   later compiler phases do not need to rediscover constructor candidates
///   from raw tuple shape.
pub(crate) fn pattern_output(pattern: &Pattern) -> SyntaxPatternOutput {
    match pattern {
        Pattern::Wildcard => pattern_leaf(SyntaxPatternKind::Wildcard, None),
        Pattern::Var(name) => pattern_leaf(SyntaxPatternKind::Var, Some(name.clone())),
        Pattern::Int(value) => pattern_leaf(SyntaxPatternKind::Int, Some(value.to_string())),
        Pattern::Float(value) => pattern_leaf(SyntaxPatternKind::Float, Some(value.to_string())),
        Pattern::Atom(name) => pattern_leaf(SyntaxPatternKind::Atom, Some(name.clone())),
        Pattern::Tuple(items) if is_constructor_pattern_tuple(items) => {
            let Pattern::Atom(name) = &items[0] else {
                unreachable!("constructor pattern tuple starts with atom");
            };
            pattern_node(
                SyntaxPatternKind::Constructor,
                Some(name.clone()),
                items.iter().skip(1).map(pattern_output).collect(),
                Vec::new(),
            )
        }
        Pattern::Tuple(items) => pattern_node(
            SyntaxPatternKind::Tuple,
            None,
            items.iter().map(pattern_output).collect(),
            Vec::new(),
        ),
        Pattern::List(items) => pattern_node(
            SyntaxPatternKind::List,
            None,
            items.iter().map(pattern_output).collect(),
            Vec::new(),
        ),
        Pattern::ListCons(head, tail) => pattern_node(
            SyntaxPatternKind::ListCons,
            None,
            vec![pattern_output(head), pattern_output(tail)],
            Vec::new(),
        ),
        Pattern::Map(fields) => pattern_node(
            SyntaxPatternKind::Map,
            None,
            Vec::new(),
            fields.iter().map(pattern_field_output).collect(),
        ),
        Pattern::Record { name, fields } => pattern_node(
            SyntaxPatternKind::Record,
            Some(name.clone()),
            Vec::new(),
            fields.iter().map(pattern_field_output).collect(),
        ),
    }
}

/// Builds a leaf pattern-output node.
///
/// Inputs:
/// - `kind`: output pattern kind.
/// - `text`: optional source-derived payload.
///
/// Output:
/// - Pattern output with no child patterns or fields.
///
/// Transformation:
/// - Delegates to the shared node constructor with empty child and field lists.
pub(crate) fn pattern_leaf(kind: SyntaxPatternKind, text: Option<String>) -> SyntaxPatternOutput {
    pattern_node(kind, text, Vec::new(), Vec::new())
}

/// Builds a pattern-output node and computes its arity.
///
/// Inputs:
/// - `kind`: output pattern kind.
/// - `text`: optional source-derived payload.
/// - `children`: positional child patterns.
/// - `fields`: keyed pattern fields.
///
/// Output:
/// - Pattern output with arity derived from fields when present, otherwise
///   from positional children.
///
/// Transformation:
/// - Centralizes arity computation so constructor, tuple, list, map, and record
///   pattern outputs remain consistent.
fn pattern_node(
    kind: SyntaxPatternKind,
    text: Option<String>,
    children: Vec<SyntaxPatternOutput>,
    fields: Vec<SyntaxPatternFieldOutput>,
) -> SyntaxPatternOutput {
    SyntaxPatternOutput {
        kind,
        arity: if fields.is_empty() {
            children.len()
        } else {
            fields.len()
        },
        text,
        children,
        fields,
    }
}

/// Converts one map or record pattern field into syntax output.
///
/// Inputs:
/// - `field`: parse-tree field pattern.
///
/// Output:
/// - Serializable field output containing the key, required flag, and nested
///   value pattern.
///
/// Transformation:
/// - Recursively converts the field value while preserving field metadata.
fn pattern_field_output(field: &MapField) -> SyntaxPatternFieldOutput {
    SyntaxPatternFieldOutput {
        key: field.key.clone(),
        required: field.required,
        value: Box::new(pattern_output(&field.value)),
    }
}

/// Detects tuple patterns that represent constructor-pattern candidates.
///
/// Inputs:
/// - `items`: tuple pattern items.
///
/// Output:
/// - `true` when the tuple starts with an uppercase atom name.
///
/// Transformation:
/// - Applies the syntactic constructor-candidate rule used by syntax output;
///   semantic validation still decides whether the constructor is declared.
fn is_constructor_pattern_tuple(items: &[Pattern]) -> bool {
    matches!(
        items.first(),
        Some(Pattern::Atom(name)) if name.chars().next().is_some_and(|ch| ch.is_ascii_uppercase())
    )
}
