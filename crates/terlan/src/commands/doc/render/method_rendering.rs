use super::*;

/// Appends a receiver method documentation section.
pub(super) fn render_syntax_method_decl_docs_markdown(
    out: &mut String,
    docs: &[String],
    receiver: &SyntaxParamOutput,
    declaration: SyntaxCallableDocumentation<'_>,
) {
    let SyntaxCallableDocumentation {
        name,
        params,
        return_type,
        is_public,
        is_pure,
    } = declaration;
    out.push_str(&format!(
        "### `{}.{}({})`\n\n",
        receiver.annotation.text,
        name,
        params.len()
    ));
    push_markdown_doc_block(out, docs);
    out.push_str("```terlan\n");
    out.push_str(&render_purity_marked_signature(
        is_pure,
        render_method_signature(receiver, name, params, return_type, is_public),
    ));
    out.push_str("\n```\n\n");
}
