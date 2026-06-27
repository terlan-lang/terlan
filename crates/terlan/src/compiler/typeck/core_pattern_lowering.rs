use super::*;

/// Checks whether a Core pattern maps to the current Lean pattern subset.
///
/// Inputs:
/// - `pattern`: typed Core pattern lowered from production syntax.
///
/// Output:
/// - `true` for wildcard, variable, integer, atom, tuple, list, and
///   constructor patterns whose nested patterns are also Lean-modeled.
/// - `false` for typed-but-unmodeled pattern payloads such as float,
///   list-cons, map, and record patterns.
///
/// Transformation:
/// - Recursively inspects structural pattern children without modifying the
///   production CorePattern payload.
pub(crate) fn core_pattern_is_lean_modeled(pattern: &CorePattern) -> bool {
    match pattern {
        CorePattern::Wildcard
        | CorePattern::Var(_)
        | CorePattern::Int(_)
        | CorePattern::Atom(_) => true,
        CorePattern::Tuple(items) | CorePattern::List(items) => {
            items.iter().all(core_pattern_is_lean_modeled)
        }
        CorePattern::Constructor { args, .. } => args.iter().all(core_pattern_is_lean_modeled),
        CorePattern::Float(_)
        | CorePattern::ListCons { .. }
        | CorePattern::Map(_)
        | CorePattern::Record { .. } => false,
    }
}

/// Classifies a syntax-output pattern for Lean proof coverage.
///
/// Inputs:
/// - `pattern`: syntax-output pattern being summarized into CoreIR.
/// - `core_pattern`: typed Core payload produced for `pattern`, when
///   available.
///
/// Output:
/// - Proof coverage label for the current production CoreIR pattern summary.
///
/// Transformation:
/// - Marks Lean-modeled pattern families as covered only when they actually
///   carry typed `CorePattern` payloads whose nested children are also covered;
///   unsupported members of those families remain proof-model-required until
///   Lean models their shape.
pub(crate) fn core_pattern_proof_coverage(
    pattern: &SyntaxPatternOutput,
    core_pattern: Option<&CorePattern>,
) -> CoreProofCoverage {
    match pattern.kind {
        SyntaxPatternKind::Wildcard
        | SyntaxPatternKind::Var
        | SyntaxPatternKind::Int
        | SyntaxPatternKind::Atom
        | SyntaxPatternKind::Tuple
        | SyntaxPatternKind::List
        | SyntaxPatternKind::Constructor
        | SyntaxPatternKind::Ignore
        | SyntaxPatternKind::Placeholder => {
            if core_pattern.is_some_and(core_pattern_is_lean_modeled) {
                CoreProofCoverage::LeanCovered
            } else {
                CoreProofCoverage::ProofModelRequired
            }
        }
        SyntaxPatternKind::Float
        | SyntaxPatternKind::ListCons
        | SyntaxPatternKind::Map
        | SyntaxPatternKind::Record
        | SyntaxPatternKind::MapField => CoreProofCoverage::ProofModelRequired,
    }
}

/// Converts a syntax-output pattern into a typed Core pattern when covered.
///
/// Inputs:
/// - `pattern`: syntax-output pattern summary produced by the parser pipeline.
///
/// Output:
/// - `Some(CorePattern)` for Lean-covered pattern forms.
/// - `None` for source forms that still need a richer CorePattern model.
///
/// Transformation:
/// - Reconstructs typed structural Core pattern nodes from syntax-output kind,
///   text, and child patterns, without using backend lowering or rendered
///   summary text.
pub(crate) fn core_pattern_from_syntax(pattern: &SyntaxPatternOutput) -> Option<CorePattern> {
    match pattern.kind {
        SyntaxPatternKind::Wildcard
        | SyntaxPatternKind::Ignore
        | SyntaxPatternKind::Placeholder => Some(CorePattern::Wildcard),
        SyntaxPatternKind::Var => pattern.text.clone().map(CorePattern::Var),
        SyntaxPatternKind::Int => pattern
            .text
            .as_ref()
            .and_then(|value| value.parse::<i64>().ok())
            .map(CorePattern::Int),
        SyntaxPatternKind::Atom => pattern.text.clone().map(CorePattern::Atom),
        SyntaxPatternKind::Tuple => {
            core_patterns_from_syntax_children(pattern).map(CorePattern::Tuple)
        }
        SyntaxPatternKind::List => {
            core_patterns_from_syntax_children(pattern).map(CorePattern::List)
        }
        SyntaxPatternKind::ListCons => core_list_cons_pattern_from_syntax(pattern),
        SyntaxPatternKind::Constructor => pattern.text.as_ref().and_then(|name| {
            core_patterns_from_syntax_children(pattern).map(|args| CorePattern::Constructor {
                name: name.clone(),
                constructor_identity: None,
                args,
            })
        }),
        SyntaxPatternKind::Float => pattern.text.clone().map(CorePattern::Float),
        SyntaxPatternKind::Map => {
            core_map_pattern_fields_from_syntax(pattern).map(CorePattern::Map)
        }
        SyntaxPatternKind::Record => core_record_pattern_from_syntax(pattern),
        SyntaxPatternKind::MapField => None,
    }
}

/// Converts a syntax-output list-cons pattern into typed Core.
///
/// Inputs:
/// - `pattern`: syntax-output list-cons pattern with head and tail children.
///
/// Output:
/// - `Some(CorePattern::ListCons)` when both head and tail lower to typed Core
///   patterns.
/// - `None` when the shape is not list-cons or either side remains unsupported.
///
/// Transformation:
/// - Preserves the structural cons pattern as a backend-agnostic head/tail Core
///   node without using list rendering syntax.
fn core_list_cons_pattern_from_syntax(pattern: &SyntaxPatternOutput) -> Option<CorePattern> {
    if !matches!(pattern.kind, SyntaxPatternKind::ListCons) || pattern.children.len() != 2 {
        return None;
    }

    Some(CorePattern::ListCons {
        head: Box::new(core_pattern_from_syntax(&pattern.children[0])?),
        tail: Box::new(core_pattern_from_syntax(&pattern.children[1])?),
    })
}

/// Converts syntax-output map-pattern fields into typed Core map fields.
///
/// Inputs:
/// - `pattern`: syntax-output map pattern whose fields should be lowered.
///
/// Output:
/// - `Some(Vec<CoreMapPatternField>)` when every field value lowers to a typed
///   Core pattern.
/// - `None` when the pattern has non-map syntax or any field value remains
///   unsupported.
///
/// Transformation:
/// - Preserves field keys and required/optional matching mode, while
///   recursively lowering field value patterns into backend-agnostic CoreIR.
fn core_map_pattern_fields_from_syntax(
    pattern: &SyntaxPatternOutput,
) -> Option<Vec<CoreMapPatternField>> {
    if !matches!(pattern.kind, SyntaxPatternKind::Map) {
        return None;
    }

    pattern
        .fields
        .iter()
        .map(|field| {
            core_pattern_from_syntax(&field.value).map(|value| CoreMapPatternField {
                key: field.key.clone(),
                required: field.required,
                value,
            })
        })
        .collect()
}

/// Converts a syntax-output record pattern into typed Core.
///
/// Inputs:
/// - `pattern`: syntax-output record pattern with source record name and fields.
///
/// Output:
/// - `Some(CorePattern::Record)` when every field value lowers to a typed Core
///   pattern.
/// - `None` when the shape is not a record, has no name, or any field value is
///   unsupported.
///
/// Transformation:
/// - Preserves record identity and field names as semantic CoreIR data, while
///   recursively lowering field values into typed Core patterns.
fn core_record_pattern_from_syntax(pattern: &SyntaxPatternOutput) -> Option<CorePattern> {
    if !matches!(pattern.kind, SyntaxPatternKind::Record) {
        return None;
    }

    Some(CorePattern::Record {
        name: pattern.text.clone()?,
        fields: core_record_pattern_fields_from_syntax(pattern)?,
    })
}

/// Converts syntax-output record-pattern fields into typed Core record fields.
///
/// Inputs:
/// - `pattern`: syntax-output record pattern whose fields should be lowered.
///
/// Output:
/// - `Some(Vec<CoreRecordPatternField>)` when every field value lowers.
/// - `None` when any field value remains unsupported.
///
/// Transformation:
/// - Preserves field keys and required/optional source mode, while recursively
///   lowering field value patterns into backend-agnostic CoreIR.
fn core_record_pattern_fields_from_syntax(
    pattern: &SyntaxPatternOutput,
) -> Option<Vec<CoreRecordPatternField>> {
    pattern
        .fields
        .iter()
        .map(|field| {
            core_pattern_from_syntax(&field.value).map(|value| CoreRecordPatternField {
                key: field.key.clone(),
                required: field.required,
                value,
            })
        })
        .collect()
}

/// Converts syntax-output pattern children into typed Core pattern children.
///
/// Inputs:
/// - `pattern`: syntax-output parent pattern whose children should be lowered.
///
/// Output:
/// - `Some(Vec<CorePattern>)` when every child is in the covered subset.
/// - `None` when at least one child is not yet representable as a typed Core
///   pattern.
///
/// Transformation:
/// - Recursively lowers children and fails the parent conversion if any child
///   remains unsupported.
fn core_patterns_from_syntax_children(pattern: &SyntaxPatternOutput) -> Option<Vec<CorePattern>> {
    core_patterns_from_syntax_slice(&pattern.children)
}

/// Converts a slice of syntax-output patterns into typed Core patterns.
///
/// Inputs:
/// - `patterns`: syntax-output patterns to lower in order.
///
/// Output:
/// - `Some(Vec<CorePattern>)` when every pattern is in the current typed
///   subset.
/// - `None` when at least one pattern is not yet representable as typed Core.
///
/// Transformation:
/// - Recursively lowers each pattern and fails the entire slice conversion if
///   any element remains unsupported.
pub(crate) fn core_patterns_from_syntax_slice(
    patterns: &[SyntaxPatternOutput],
) -> Option<Vec<CorePattern>> {
    patterns.iter().map(core_pattern_from_syntax).collect()
}

/// Renders a syntax pattern as deterministic CoreIR summary text.
///
/// Inputs:
/// - `pattern`: syntax-output pattern.
///
/// Output:
/// - Stable pattern summary text.
///
/// Transformation:
/// - Combines pattern kind, optional text, arity, and recursive child/field
///   summaries without assigning backend representation.
pub(crate) fn core_pattern_summary_text(pattern: &SyntaxPatternOutput) -> String {
    let mut parts = vec![format!("{:?}", pattern.kind)];
    if let Some(text) = &pattern.text {
        parts.push(format!("text={}", text));
    }
    parts.push(format!("arity={}", pattern.arity));
    if !pattern.children.is_empty() {
        parts.push(format!(
            "children=[{}]",
            pattern
                .children
                .iter()
                .map(core_pattern_summary_text)
                .collect::<Vec<_>>()
                .join(";")
        ));
    }
    if !pattern.fields.is_empty() {
        parts.push(format!(
            "fields=[{}]",
            pattern
                .fields
                .iter()
                .map(|field| format!(
                    "{}:{}={}",
                    field.key,
                    field.required,
                    core_pattern_summary_text(&field.value)
                ))
                .collect::<Vec<_>>()
                .join(";")
        ));
    }
    parts.join(":")
}
