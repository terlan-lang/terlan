use super::*;

#[test]
fn guarded_function_catchall_cannot_certify_finite_coverage() {
    let pattern = SyntaxPatternOutput {
        expanded_from: Vec::new(),
        kind: SyntaxPatternKind::Wildcard,
        arity: 0,
        text: None,
        children: Vec::new(),
        fields: Vec::new(),
    };
    let span = Span { start: 0, end: 1 };
    for (guarded, expected_count) in [(true, 1), (false, 0)] {
        let mut diagnostics = Vec::new();
        check_syntax_function_clause_exhaustiveness(
            "value",
            Some("Bool"),
            1,
            &HashSet::new(),
            &[(vec![pattern.clone()], span, guarded)],
            &HashMap::new(),
            &mut diagnostics,
        );
        assert_eq!(diagnostics.len(), expected_count, "{diagnostics:?}");
        if guarded {
            assert!(diagnostics[0].message.contains("non-exhaustive function"));
        }
    }
}
