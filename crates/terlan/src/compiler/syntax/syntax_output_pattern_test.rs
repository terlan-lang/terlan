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
                    [] -> Atom["empty"]
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

    /// Verifies capture-bearing string patterns have a dedicated syntax-output
    /// kind and ordered capture metadata.
    ///
    /// Inputs:
    /// - A case expression whose pattern uses `${name: Type}` capture syntax.
    ///
    /// Output:
    /// - Test passes when syntax output distinguishes the pattern from exact
    ///   string literals and emits one ordered `string_capture` child.
    ///
    /// Transformation:
    /// - Locks parser-to-syntax-output parity for the first string capture
    ///   slice before typecheck and VM matching are implemented.
    #[test]
    fn syntax_output_marks_string_capture_patterns() {
        let output = parse_module_as_syntax_output(
            r#"
            module string_capture_pattern_output.

            match_path(path: String): Int ->
                case path {
                    "test/${id: Id}.txt" -> 1;
                    _ -> 0
                }.
            "#,
        )
        .expect("syntax output string capture pattern");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[0].payload
        else {
            panic!("expected function declaration");
        };
        let case_expr = &clauses[0].body;
        let pattern = &case_expr.clauses[0].patterns[0];
        assert_eq!(pattern.kind, SyntaxPatternKind::StringPattern);
        assert_eq!(pattern.text.as_deref(), Some("test/${id: Id}.txt"));
        assert_eq!(pattern.children.len(), 1);
        assert_eq!(pattern.children[0].kind, SyntaxPatternKind::StringCapture);
        assert_eq!(pattern.children[0].text.as_deref(), Some("id: Id"));
    }

    /// Verifies template-facing route/path code keeps string capture metadata
    /// and the nominal template construction expression in the same syntax tree.
    ///
    /// Inputs:
    /// - A template declaration with a typed prop.
    /// - A case arm that captures a path segment and passes it into the
    ///   template constructor form.
    ///
    /// Output:
    /// - Test passes when the path arm remains a `StringPattern` with typed
    ///   capture metadata and its body remains a keyed `RecordConstruct`.
    ///
    /// Transformation:
    /// - Locks the current parser-visible template-backed capture surface
    ///   without claiming direct template pattern syntax exists.
    #[test]
    fn syntax_output_keeps_template_backed_string_capture_flow() {
        let output = parse_module_as_syntax_output(
            r#"
            module template_string_capture_output.

            template Page from "./templates/page.terl.html" {
                title: String
            }.

            page_for(path: String): Dynamic ->
                case path {
                    "/pages/${title: String}.html" -> Page { title: title };
                    _ -> Page { title: "missing" }
                }.
            "#,
        )
        .expect("syntax output template-backed string capture");

        let SyntaxDeclarationPayload::Function { clauses, .. } = &output.declarations[1].payload
        else {
            panic!("expected function declaration");
        };
        let case_expr = &clauses[0].body;
        let path_pattern = &case_expr.clauses[0].patterns[0];
        assert_eq!(path_pattern.kind, SyntaxPatternKind::StringPattern);
        assert_eq!(
            path_pattern.text.as_deref(),
            Some("/pages/${title: String}.html")
        );
        assert_eq!(path_pattern.children.len(), 1);
        assert_eq!(
            path_pattern.children[0].kind,
            SyntaxPatternKind::StringCapture
        );
        assert_eq!(
            path_pattern.children[0].text.as_deref(),
            Some("title: String")
        );

        let template_expr = &case_expr.clauses[0].body;
        assert_eq!(template_expr.kind, SyntaxExprKind::RecordConstruct);
        assert_eq!(template_expr.text.as_deref(), Some("Page"));
        assert_eq!(template_expr.fields.len(), 1);
        assert_eq!(template_expr.fields[0].key, "title");
        assert_eq!(template_expr.fields[0].value.kind, SyntaxExprKind::Var);
        assert_eq!(template_expr.fields[0].value.text.as_deref(), Some("title"));
    }
}
