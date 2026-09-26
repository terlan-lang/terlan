//! Finite-coverage limits fail distinctly and never claim exhausted work is covered.

use super::*;

fn true_pattern() -> SyntaxPatternOutput {
    SyntaxPatternOutput {
        kind: SyntaxPatternKind::Atom,
        arity: 0,
        text: Some("true".into()),
        children: Vec::new(),
        fields: Vec::new(),
    }
}

#[test]
fn excessive_region_inventory_reports_the_analysis_budget() {
    let error = subtract_finite_pattern(
        vec![Type::Bool; MAX_REGIONS + 1],
        &true_pattern(),
        &HashMap::new(),
    )
    .unwrap_err();
    assert_eq!(error, FiniteCoverageError::AnalysisBudget);
    assert_eq!(
        error.to_string(),
        "finite pattern coverage exceeds its analysis budget; add a covering pattern"
    );
}

#[test]
fn excessive_pattern_nesting_reports_the_nesting_budget() {
    let mut pattern = true_pattern();
    for _ in 0..=MAX_DEPTH {
        pattern = SyntaxPatternOutput {
            kind: SyntaxPatternKind::Alias,
            arity: 1,
            text: Some("alias".into()),
            children: vec![pattern],
            fields: Vec::new(),
        };
    }
    let error = subtract_finite_pattern(vec![Type::Bool], &pattern, &HashMap::new()).unwrap_err();
    assert_eq!(error, FiniteCoverageError::NestingBudget);
    assert_eq!(
        error.to_string(),
        "finite pattern coverage exceeds its nesting budget"
    );
}

#[test]
fn bounded_refinement_preserves_the_unmatched_boolean() {
    assert_eq!(
        subtract_finite_pattern(vec![Type::Bool], &true_pattern(), &HashMap::new()).unwrap(),
        vec![Type::LiteralAtom("false".into())]
    );
}
