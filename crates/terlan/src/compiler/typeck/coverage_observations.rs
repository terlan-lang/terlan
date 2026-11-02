//! Read-only observations of the existing finite-pattern checker.
//!
//! Capture is explicitly scoped to semantic export. Ordinary compilation does
//! not allocate observations; diagnostics and exported verdicts use one result.

use super::*;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub(super) struct Analysis {
    pub exhaustive: Option<bool>,
    pub remaining: Vec<Type>,
    pub guarded_clauses: Vec<usize>,
    pub reason: Option<&'static str>,
    pub error: Option<(usize, String)>,
}

pub(super) fn analyze(
    expected: &Type,
    patterns: &[(SyntaxPatternOutput, bool)],
    aliases: &HashMap<String, TypeAlias>,
) -> Analysis {
    let expanded = expand_interface_global_aliases(expected, aliases);
    let mut remaining = as_exhaustive_union_variants(&expanded);
    let mut result = Analysis {
        exhaustive: None,
        remaining: Vec::new(),
        guarded_clauses: patterns
            .iter()
            .enumerate()
            .filter_map(|(i, (_, guarded))| guarded.then_some(i))
            .collect(),
        reason: None,
        error: None,
    };
    if remaining.len() <= 1 && !has_finite_fields(&expanded) {
        result.reason = Some("finite_domain_not_available");
        return result;
    }
    for (index, (pattern, guarded)) in patterns.iter().enumerate() {
        if *guarded {
            continue;
        }
        if matches!(
            pattern.kind,
            SyntaxPatternKind::Wildcard
                | SyntaxPatternKind::Ignore
                | SyntaxPatternKind::Placeholder
                | SyntaxPatternKind::Var
        ) {
            result.exhaustive = Some(true);
            return result;
        }
        remaining = match subtract_finite_pattern(remaining, pattern, aliases) {
            Ok(value) => value,
            Err(message) => {
                result.reason = Some("unsupported_pattern");
                result.error = Some((index, message.to_string()));
                return result;
            }
        };
        if remaining.is_empty() {
            result.exhaustive = Some(true);
            return result;
        }
    }
    result.exhaustive = Some(remaining.is_empty());
    result.remaining = remaining;
    result
}

#[derive(Clone, PartialEq, Eq, Serialize)]
struct Owner {
    module: String,
    name: String,
    arity: usize,
    span_start: usize,
    span_end: usize,
}

#[derive(Clone, PartialEq, Eq, Serialize)]
pub(crate) struct Observation {
    owner: Owner,
    subject_kind: &'static str,
    span_start: usize,
    span_end: usize,
    binding_index: Option<usize>,
    subject_syntax_digest: Option<String>,
    expected_type: String,
    exhaustive: Option<bool>,
    uncovered: Vec<String>,
    guarded_clauses: Vec<usize>,
    reason: Option<&'static str>,
    error: Option<String>,
    consistent: bool,
}

#[derive(Serialize)]
pub(crate) struct Observations {
    model: &'static str,
    pub(crate) observations: Vec<Observation>,
    pub(crate) truncated: bool,
}

impl Default for Observations {
    fn default() -> Self {
        Self {
            model: "finite-pattern-coverage.v1",
            observations: Vec::new(),
            truncated: false,
        }
    }
}

#[derive(Default)]
struct Capture {
    owner: Option<Owner>,
    output: Observations,
}

thread_local! {
    static CAPTURE: RefCell<Option<Capture>> = const { RefCell::new(None) };
}

struct CaptureGuard(Option<Capture>);
impl Drop for CaptureGuard {
    fn drop(&mut self) {
        CAPTURE.with(|cell| {
            cell.replace(self.0.take());
        });
    }
}

pub(crate) fn capture<T>(action: impl FnOnce() -> T) -> (T, Observations) {
    let previous = CAPTURE.with(|cell| cell.replace(Some(Capture::default())));
    let _restore = CaptureGuard(previous);
    let value = action();
    let mut output = CAPTURE.with(|cell| cell.borrow_mut().take().expect("active capture").output);
    // Repeated inference may visit the same expression. Conflicting visits
    // remain explicit alternatives and must never certify one verdict.
    let mut groups: BTreeMap<String, Vec<Observation>> = BTreeMap::new();
    for row in output.observations {
        let key = serde_json::to_string(&(
            &row.owner,
            row.subject_kind,
            row.span_start,
            row.span_end,
            row.binding_index,
            &row.subject_syntax_digest,
        ))
        .expect("serializable observation key");
        let group = groups.entry(key).or_default();
        if !group.contains(&row) {
            group.push(row);
        }
    }
    output.observations = groups
        .into_values()
        .flat_map(|mut rows| {
            let consistent = rows.len() == 1;
            for row in &mut rows {
                row.consistent = consistent;
                if !consistent {
                    row.exhaustive = None;
                    row.reason = Some("ambiguous_inference_context");
                }
            }
            rows.sort_by_cached_key(|row| {
                serde_json::to_string(row).expect("serializable observation")
            });
            rows
        })
        .collect();
    (value, output)
}

pub(super) struct OwnerGuard(Option<Owner>);
impl Drop for OwnerGuard {
    fn drop(&mut self) {
        CAPTURE.with(|cell| {
            if let Some(capture) = cell.borrow_mut().as_mut() {
                capture.owner = self.0.take();
            }
        });
    }
}

pub(super) fn owner(module: &str, name: &str, arity: usize, span: Span) -> OwnerGuard {
    let previous = CAPTURE.with(|cell| {
        cell.borrow_mut().as_mut().and_then(|capture| {
            capture.owner.replace(Owner {
                module: module.to_string(),
                name: name.to_string(),
                arity,
                span_start: span.start,
                span_end: span.end,
            })
        })
    });
    OwnerGuard(previous)
}

pub(super) fn record(
    kind: &'static str,
    span: Span,
    binding_index: Option<usize>,
    expr: Option<&SyntaxExprOutput>,
    expected: &Type,
    analysis: &Analysis,
) {
    CAPTURE.with(|cell| {
        let mut active = cell.borrow_mut();
        let Some(capture) = active.as_mut() else {
            return;
        };
        let Some(owner) = capture.owner.clone() else {
            return;
        };
        if capture.output.observations.len() >= 4096 {
            capture.output.truncated = true;
            return;
        }
        let row = Observation {
            owner,
            subject_kind: kind,
            span_start: span.start,
            span_end: span.end,
            binding_index,
            subject_syntax_digest: expr.map(|expr| {
                Sha256::digest(serde_json::to_vec(expr).expect("serializable syntax"))
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect()
            }),
            expected_type: pretty_type(expected),
            exhaustive: analysis.exhaustive,
            uncovered: analysis.remaining.iter().map(pretty_type).collect(),
            guarded_clauses: analysis.guarded_clauses.clone(),
            reason: analysis.reason,
            error: analysis.error.as_ref().map(|(_, message)| message.clone()),
            consistent: true,
        };
        if !capture.output.observations.contains(&row) {
            capture.output.observations.push(row);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn verdict(covered: bool) -> Analysis {
        Analysis {
            exhaustive: Some(covered),
            remaining: Vec::new(),
            guarded_clauses: Vec::new(),
            reason: None,
            error: None,
        }
    }

    #[test]
    fn conflicting_inference_contexts_never_export_an_exhaustive_verdict() {
        let (_, result) = capture(|| {
            let span = Span { start: 1, end: 2 };
            let _owner = owner("sample", "value", 1, span);
            record("case", span, None, None, &Type::Bool, &verdict(true));
            record("case", span, None, None, &Type::Bool, &verdict(false));
        });
        assert_eq!(result.observations.len(), 2);
        assert!(result.observations.iter().all(|row| !row.consistent
            && row.exhaustive.is_none()
            && row.reason == Some("ambiguous_inference_context")));
    }

    #[test]
    fn nested_capture_restores_outer_owner_even_after_unwinding() {
        let (_, result) = capture(|| {
            let span = Span { start: 1, end: 2 };
            let _owner = owner("sample", "outer", 1, span);
            let aborted = std::panic::catch_unwind(|| {
                capture(|| {
                    let _inner = owner("sample", "inner", 1, span);
                    panic!("interrupted semantic export");
                })
            });
            assert!(aborted.is_err());
            record("case", span, None, None, &Type::Bool, &verdict(true));
        });
        assert_eq!(result.observations.len(), 1);
        assert_eq!(result.observations[0].owner.name, "outer");
    }

    #[test]
    fn capture_limit_is_reported_instead_of_claiming_a_complete_set() {
        let (_, result) = capture(|| {
            let _owner = owner(
                "sample",
                "value",
                1,
                Span {
                    start: 0,
                    end: 10_000,
                },
            );
            for index in 0..4097 {
                record(
                    "case",
                    Span {
                        start: index,
                        end: index + 1,
                    },
                    None,
                    None,
                    &Type::Bool,
                    &verdict(true),
                );
            }
        });
        assert!(result.truncated);
        assert_eq!(result.observations.len(), 4096);
    }
}
