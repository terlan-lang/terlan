use std::collections::BTreeMap;

use terlan_syntax::{
    SyntaxExprKind, SyntaxExprOutput, SyntaxModuleOutput, SyntaxTemplatePropOutput,
};

use super::render::{
    find_syntax_template_props, render_syntax_static_template_nodes, StaticSyntaxRenderError,
};
use super::render_values::StaticTemplateValue;
use super::TEMPLATE_CHILDREN_SLOT;

/// Renders Markdown content through a static page layout template.
///
/// Inputs:
/// - `module`: syntax-output module containing template declarations.
/// - `templates`: parsed external HTML templates keyed by template name.
/// - `layout`: template name from `@page.layout`.
/// - `title`: optional page title from `@page.title`.
/// - `document`: Markdown document whose rendered HTML becomes `children`.
///
/// Output:
/// - Layout-rendered HTML, or a static render error.
///
/// Transformation:
/// - Builds a constrained template value map where `${children}` is Markdown
///   HTML and declared `title` props receive the page title. Other required
///   layout props are rejected until page metadata grows typed prop support.
pub(crate) fn render_syntax_static_markdown_layout(
    module: &SyntaxModuleOutput,
    templates: &BTreeMap<String, terlan_html::HtmlTemplate>,
    layout: &str,
    title: Option<&str>,
    document: &terlan_html::MarkdownDocument,
) -> Result<String, StaticSyntaxRenderError> {
    let template_props = find_syntax_template_props(module, layout).ok_or_else(|| {
        StaticSyntaxRenderError::Invalid(format!("unknown static Markdown layout `{}`", layout))
    })?;
    let template = templates.get(layout).ok_or_else(|| {
        StaticSyntaxRenderError::Invalid(format!(
            "missing parsed static Markdown layout `{}`",
            layout
        ))
    })?;
    let values = markdown_layout_values(layout, template_props, title, &document.rendered_html)?;

    render_syntax_static_template_nodes(
        module,
        templates,
        layout,
        template_props,
        template,
        &values,
    )
}

/// Builds the supported value map for a Markdown page layout.
///
/// Inputs:
/// - `layout`: layout template name for diagnostics.
/// - `template_props`: declared layout props.
/// - `title`: optional `@page.title` value.
/// - `children`: rendered Markdown HTML.
///
/// Output:
/// - Static template values for supported layout slots.
///
/// Transformation:
/// - Always supplies `${children}` as HTML and supplies `title` only when the
///   layout declares it. Any other declared prop is rejected because content
///   page metadata does not yet carry arbitrary typed layout props.
fn markdown_layout_values(
    layout: &str,
    template_props: &[SyntaxTemplatePropOutput],
    title: Option<&str>,
    children: &str,
) -> Result<BTreeMap<String, StaticTemplateValue>, StaticSyntaxRenderError> {
    let mut values = BTreeMap::new();
    values.insert(
        TEMPLATE_CHILDREN_SLOT.to_string(),
        StaticTemplateValue::Html(children.to_string()),
    );

    for prop in template_props {
        if prop.name == "title" {
            values.insert(
                prop.name.clone(),
                StaticTemplateValue::Text(title.unwrap_or_default().to_string()),
            );
            continue;
        }
        return Err(StaticSyntaxRenderError::Invalid(format!(
            "static Markdown layout `{}` declares unsupported required prop `{}`",
            layout, prop.name
        )));
    }

    Ok(values)
}

/// Renders a supported static Markdown field access.
///
/// Inputs:
/// - `markdown_imports`: Markdown documents keyed by import alias.
/// - `expr`: syntax-output field access expression.
///
/// Output:
/// - Rendered Markdown HTML or a render error.
///
/// Transformation:
/// - Validates the access is `alias.html` for an imported Markdown document and
///   rejects non-renderable fields such as `raw`.
pub(super) fn render_syntax_static_markdown_field(
    markdown_imports: &BTreeMap<String, terlan_html::MarkdownDocument>,
    expr: &SyntaxExprOutput,
) -> Result<String, StaticSyntaxRenderError> {
    let field = expr.text.as_deref().ok_or_else(|| {
        StaticSyntaxRenderError::Invalid(
            "static Markdown field access is missing a field name".to_string(),
        )
    })?;
    let value = expr.children.first().ok_or_else(|| {
        StaticSyntaxRenderError::Invalid(
            "static Markdown field access is missing a value".to_string(),
        )
    })?;
    if value.kind != SyntaxExprKind::Var {
        return Err(StaticSyntaxRenderError::Invalid(
            "static Markdown output must reference an imported Markdown alias".to_string(),
        ));
    }
    let alias = value.text.as_deref().ok_or_else(|| {
        StaticSyntaxRenderError::Invalid(
            "static Markdown field access is missing an alias".to_string(),
        )
    })?;
    let document = markdown_imports.get(alias).ok_or_else(|| {
        StaticSyntaxRenderError::Invalid(format!("unknown static Markdown import `{}`", alias))
    })?;

    match field {
        "html" => Ok(document.rendered_html.clone()),
        "raw" => Err(StaticSyntaxRenderError::Invalid(format!(
            "Markdown import `{}.raw` is Binary and cannot be rendered as static Html",
            alias
        ))),
        other => Err(StaticSyntaxRenderError::Invalid(format!(
            "unknown static Markdown field `{}.{}`",
            alias, other
        ))),
    }
}
