use super::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn syntax_output_includes_structured_html_nodes() {
        let output = parse_module_as_syntax_output(
            r#"
            module html_tree.

            view(Title: Text): Html[:none] ->
                html {
                    <section class={["hero", "compact"]}>
                        <h1>{Title}</h1>
                    </section>
                }.
            "#,
        )
        .expect("syntax output");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload
        else {
            panic!("expected function declaration");
        };
        let body = &clauses[0].body;

        assert_eq!(body.kind, SyntaxExprKind::HtmlBlock);
        assert_eq!(body.html_nodes.len(), 1);
        let SyntaxHtmlNodeOutput::Element { element } = &body.html_nodes[0] else {
            panic!("expected root html element");
        };
        assert_eq!(element.name, "section");
        assert_eq!(element.attrs.len(), 1);
        assert_eq!(element.attrs[0].name, "class");
        match element.attrs[0].value.as_ref().expect("class value") {
            SyntaxHtmlAttrValueOutput::Expr { expr } => assert_eq!(expr.kind, SyntaxExprKind::List),
            other => panic!("unexpected html attr value: {other:?}"),
        }
        let SyntaxHtmlNodeOutput::Element { element: heading } = &element.children[0] else {
            panic!("expected heading child");
        };
        assert_eq!(heading.name, "h1");
        let SyntaxHtmlNodeOutput::Expr { expr } = &heading.children[0] else {
            panic!("expected heading interpolation");
        };
        assert_eq!(expr.kind, SyntaxExprKind::Var);
        assert_eq!(expr.text.as_deref(), Some("Title"));
    }

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
