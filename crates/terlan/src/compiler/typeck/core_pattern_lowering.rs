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
        | CorePattern::String(_)
        | CorePattern::Atom(_) => true,
        CorePattern::Tuple(items) | CorePattern::List(items) => {
            items.iter().all(core_pattern_is_lean_modeled)
        }
        CorePattern::Alias { pattern, .. } => core_pattern_is_lean_modeled(pattern),
        CorePattern::Constructor { args, .. } => args.iter().all(core_pattern_is_lean_modeled),
        CorePattern::Float(_)
        | CorePattern::StringPattern(_)
        | CorePattern::ListCons { .. }
        | CorePattern::Map(_)
        | CorePattern::Record { .. }
        | CorePattern::BinaryLayout { .. } => false,
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
        | SyntaxPatternKind::String
        | SyntaxPatternKind::Atom
        | SyntaxPatternKind::Tuple
        | SyntaxPatternKind::Alias
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
        | SyntaxPatternKind::MapField
        | SyntaxPatternKind::StringPattern
        | SyntaxPatternKind::BinaryLayout
        | SyntaxPatternKind::StringCapture => CoreProofCoverage::ProofModelRequired,
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
        SyntaxPatternKind::String => pattern.text.clone().map(CorePattern::String),
        SyntaxPatternKind::StringPattern => core_string_pattern_from_syntax(pattern),
        SyntaxPatternKind::StringCapture => None,
        SyntaxPatternKind::Tuple => {
            core_patterns_from_syntax_children(pattern).map(CorePattern::Tuple)
        }
        SyntaxPatternKind::Alias => core_alias_pattern_from_syntax(pattern),
        SyntaxPatternKind::List => {
            core_patterns_from_syntax_children(pattern).map(CorePattern::List)
        }
        SyntaxPatternKind::ListCons => core_list_cons_pattern_from_syntax(pattern),
        SyntaxPatternKind::Constructor
            if pattern
                .text
                .as_deref()
                .is_some_and(|name| name.starts_with("$const:")) =>
        {
            pattern.children.first().and_then(core_pattern_from_syntax)
        }
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
        SyntaxPatternKind::BinaryLayout => core_binary_layout_pattern_from_syntax(pattern),
        SyntaxPatternKind::MapField => None,
    }
}

/// Converts a syntax-output alias pattern into typed Core.
///
/// Inputs:
/// - `pattern`: syntax alias node with alias text and one nested child pattern.
///
/// Output:
/// - Core alias pattern when both alias name and nested pattern are valid.
///
/// Transformation:
/// - Preserves the source alias as a binding over the same matched value while
///   recursively lowering the structural child pattern.
fn core_alias_pattern_from_syntax(pattern: &SyntaxPatternOutput) -> Option<CorePattern> {
    if pattern.kind != SyntaxPatternKind::Alias || pattern.children.len() != 1 {
        return None;
    }
    let alias = pattern.text.clone()?;
    let child = core_pattern_from_syntax(&pattern.children[0])?;
    Some(CorePattern::Alias {
        alias,
        pattern: Box::new(child),
    })
}

/// Converts a syntax-output string pattern into typed Core.
///
/// Inputs:
/// - `pattern`: syntax-output string-pattern node with canonical `${...}` text.
///
/// Output:
/// - `Some(CorePattern::StringPattern)` when the canonical text can be split
///   into ordered literal/capture segments.
/// - `None` for malformed or non-string-pattern input.
///
/// Transformation:
/// - Reconstructs a backend-neutral segment payload from syntax-output text,
///   leaving runtime matching and capture conversion to the VM pattern planner.
fn core_string_pattern_from_syntax(pattern: &SyntaxPatternOutput) -> Option<CorePattern> {
    if pattern.kind != SyntaxPatternKind::StringPattern {
        return None;
    }
    let text = pattern.text.as_deref()?;
    core_string_pattern_segments(text).map(CorePattern::StringPattern)
}

/// Splits canonical string-pattern text into CoreIR segments.
///
/// Inputs:
/// - `text`: syntax-output string-pattern text containing `${...}` captures.
///
/// Output:
/// - Ordered CoreIR literal and capture segments.
///
/// Transformation:
/// - Uses the canonical syntax-output spelling only; parser diagnostics own
///   malformed source recovery before this function runs.
fn core_string_pattern_segments(text: &str) -> Option<Vec<CoreStringPatternSegment>> {
    let mut segments = Vec::new();
    let mut rest = text;

    while let Some(start) = rest.find("${") {
        let literal = &rest[..start];
        if !literal.is_empty() {
            segments.push(CoreStringPatternSegment::Literal(literal.to_string()));
        }
        let after_start = &rest[start + 2..];
        let end = after_start.find('}')?;
        let capture = core_string_pattern_capture(&after_start[..end])?;
        segments.push(CoreStringPatternSegment::Capture(capture));
        rest = &after_start[end + 1..];
    }

    if !rest.is_empty() {
        segments.push(CoreStringPatternSegment::Literal(rest.to_string()));
    }
    (!segments.is_empty()).then_some(segments)
}

/// Converts one canonical capture slot into a CoreIR capture payload.
///
/// Inputs:
/// - `slot`: text inside `${...}`.
///
/// Output:
/// - Capture name plus optional type annotation text.
///
/// Transformation:
/// - Splits on the first `:` and trims whitespace introduced by source
///   formatting; parser/typechecker validation owns legality of the slot.
fn core_string_pattern_capture(slot: &str) -> Option<CoreStringPatternCapture> {
    let (name, type_annotation) = match slot.split_once(':') {
        Some((name, annotation)) => (name.trim(), Some(annotation.trim().to_string())),
        None => (slot.trim(), None),
    };
    if name.is_empty() {
        return None;
    }
    Some(CoreStringPatternCapture {
        name: name.to_string(),
        type_annotation,
    })
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
