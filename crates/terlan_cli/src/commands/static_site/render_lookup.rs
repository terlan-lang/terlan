use terlan_syntax::{SyntaxDeclarationPayload, SyntaxModuleOutput, SyntaxTemplatePropOutput};

/// Finds the template declaration associated with an HTML component tag.
///
/// Inputs:
/// - `module`: syntax-output module containing template declarations.
/// - `templates`: parsed templates with optional tag names.
/// - `tag`: HTML tag name to resolve as a component.
///
/// Output:
/// - Template name and prop declarations when the tag is a component.
///
/// Transformation:
/// - Cross-references declarations with parsed templates by tag metadata.
pub(super) fn find_syntax_template_decl_by_tag<'a>(
    module: &'a SyntaxModuleOutput,
    templates: &'a std::collections::BTreeMap<String, terlan_html::HtmlTemplate>,
    tag: &str,
) -> Option<(&'a str, &'a [SyntaxTemplatePropOutput])> {
    module
        .declarations
        .iter()
        .find_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Template { name, props, .. } => {
                let template = templates.get(name)?;
                if template.tag_name.as_deref() == Some(tag) {
                    Some((name.as_str(), props.as_slice()))
                } else {
                    None
                }
            }
            _ => None,
        })
}

/// Looks up the declared type text for a template prop.
///
/// Inputs:
/// - `template_props`: prop declarations to search.
/// - `name`: prop name.
///
/// Output:
/// - Borrowed annotation text when the prop exists.
///
/// Transformation:
/// - Performs a linear name lookup across current template props.
pub(super) fn syntax_static_template_prop_type<'a>(
    template_props: &'a [SyntaxTemplatePropOutput],
    name: &str,
) -> Option<&'a str> {
    template_props
        .iter()
        .find(|prop| prop.name == name)
        .map(|prop| prop.annotation.text.as_str())
}
