use std::collections::BTreeMap;

use crate::terlan_syntax::{
    SyntaxExprKind, SyntaxExprOutput, SyntaxModuleOutput, SyntaxTemplatePropOutput,
};

use super::render::{
    find_syntax_template_props, render_syntax_static_template_nodes, StaticSyntaxRenderError,
};
use super::render_values::{is_static_template_html_type, StaticTemplateValue};
use super::TEMPLATE_CHILDREN_SLOT;

/// Renders Markdown content through a static page layout template.
///
/// Inputs:
/// - `module`: syntax-output module containing template declarations.
/// - `templates`: parsed external HTML templates keyed by template name.
/// - `layout`: template name from `@page.layout`.
/// - `title`: optional page title from `@page.title`.
/// - `document`: Markdown document whose rendered HTML becomes `children`.
/// - `page_navigation`: trusted documentation fragments when `--docs` is on.
///
/// Output:
/// - Layout-rendered HTML, or a static render error.
///
/// Transformation:
/// - Builds a constrained template value map where `${children}` is Markdown
///   HTML and declared `title` props receive the page title. Documentation
///   navigation and table-of-contents props receive generator-owned HTML only
///   in docs mode.
pub(crate) fn render_syntax_static_markdown_layout(
    module: &SyntaxModuleOutput,
    templates: &BTreeMap<String, crate::terlan_html::HtmlTemplate>,
    layout: &str,
    title: Option<&str>,
    document: &crate::terlan_html::MarkdownDocument,
    page_navigation: Option<&terl_docs::PageNavigation>,
) -> Result<String, StaticSyntaxRenderError> {
    render_syntax_static_html_layout(
        module,
        templates,
        layout,
        title,
        &document.rendered_html,
        page_navigation,
    )
}

/// Renders trusted generated HTML through a declared typed page layout.
pub(crate) fn render_syntax_static_html_layout(
    module: &SyntaxModuleOutput,
    templates: &BTreeMap<String, crate::terlan_html::HtmlTemplate>,
    layout: &str,
    title: Option<&str>,
    children: &str,
    page_navigation: Option<&terl_docs::PageNavigation>,
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
    let values = markdown_layout_values(layout, template_props, title, children, page_navigation)?;

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
/// - `page_navigation`: trusted documentation fragments when available.
///
/// Output:
/// - Static template values for supported layout slots.
///
/// Transformation:
/// - Always supplies `${children}` as HTML, supplies `title` as text, and maps
///   the four generator-owned navigation fragments to explicitly typed HTML
///   props. Other declared props remain unsupported.
fn markdown_layout_values(
    layout: &str,
    template_props: &[SyntaxTemplatePropOutput],
    title: Option<&str>,
    children: &str,
    page_navigation: Option<&terl_docs::PageNavigation>,
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
        let fragment = match prop.name.as_str() {
            "navigation" => page_navigation.map(|page| page.navigation_html.as_str()),
            "breadcrumbs" => page_navigation.map(|page| page.breadcrumbs_html.as_str()),
            "pagination" => page_navigation.map(|page| page.pagination_html.as_str()),
            "toc" => page_navigation.map(|page| page.toc_html.as_str()),
            _ => {
                return Err(StaticSyntaxRenderError::Invalid(format!(
                    "static Markdown layout `{}` declares unsupported required prop `{}`",
                    layout, prop.name
                )))
            }
        };
        if !is_static_template_html_type(&prop.annotation.text) {
            return Err(StaticSyntaxRenderError::Invalid(format!(
                "static Markdown layout `{}` prop `{}` must be Html, found `{}`",
                layout, prop.name, prop.annotation.text
            )));
        }
        values.insert(
            prop.name.clone(),
            StaticTemplateValue::Html(fragment.unwrap_or_default().to_string()),
        );
        continue;
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
    markdown_imports: &BTreeMap<String, crate::terlan_html::MarkdownDocument>,
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
