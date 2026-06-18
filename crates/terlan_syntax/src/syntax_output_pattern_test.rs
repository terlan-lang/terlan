use super::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn syntax_output_marks_constructor_pattern_candidates() {
        let output = parse_module_as_syntax_output(
            r#"
            module constructor_patterns.

            unwrap(Result: Result): Int ->
                case Result {
                    Ok(value) -> value;
                    None -> 0
                }.
            "#,
        )
        .expect("syntax output");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload
        else {
            panic!("expected function declaration");
        };
        let body = &clauses[0].body;
        let ok_pattern = &body.clauses[0].patterns[0];
        assert_eq!(ok_pattern.kind, SyntaxPatternKind::Constructor);
        assert_eq!(ok_pattern.text.as_deref(), Some("Ok"));
        assert_eq!(ok_pattern.children.len(), 1);
        assert_eq!(ok_pattern.children[0].kind, SyntaxPatternKind::Var);

        let none_pattern = &body.clauses[1].patterns[0];
        assert_eq!(none_pattern.kind, SyntaxPatternKind::Constructor);
        assert_eq!(none_pattern.text.as_deref(), Some("None"));
        assert!(none_pattern.children.is_empty());
    }

    #[test]
    fn syntax_output_includes_list_cons_expr_and_pattern_trees() {
        let output = parse_module_as_syntax_output(
            r#"
            module list_cons_trees.

            prepend(head: Dynamic, tail: List[Dynamic]): Dynamic ->
                [head | tail].

            pick(input: List[Dynamic]): Dynamic ->
                case input {
                    [head | tail] -> head;
                    [] -> :empty
                }.
            "#,
        )
        .expect("syntax output list cons trees");

        let SyntaxDeclarationPayload::Function {
            clauses: prepend_clauses,
            ..
        } = &output.declarations[0].payload
        else {
            panic!("expected prepend function declaration");
        };
        let prepend = &prepend_clauses[0].body;
        assert_eq!(prepend.kind, SyntaxExprKind::ListCons);
        assert_eq!(prepend.children.len(), 2);
        assert_eq!(prepend.children[0].kind, SyntaxExprKind::Var);
        assert_eq!(prepend.children[0].text.as_deref(), Some("head"));
        assert_eq!(prepend.children[1].kind, SyntaxExprKind::Var);
        assert_eq!(prepend.children[1].text.as_deref(), Some("tail"));

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[1].payload
        else {
            panic!("expected pick function declaration");
        };
        let case_expr = &clauses[0].body;
        let pattern = &case_expr.clauses[0].patterns[0];
        assert_eq!(pattern.kind, SyntaxPatternKind::ListCons);
        assert_eq!(pattern.children.len(), 2);
        assert_eq!(pattern.children[0].kind, SyntaxPatternKind::Var);
        assert_eq!(pattern.children[0].text.as_deref(), Some("head"));
        assert_eq!(pattern.children[1].kind, SyntaxPatternKind::Var);
        assert_eq!(pattern.children[1].text.as_deref(), Some("tail"));
    }
}
