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
}
