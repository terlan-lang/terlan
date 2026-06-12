use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use terlan_syntax::{span::Span, SyntaxDeclarationPayload, SyntaxModuleOutput};
use terlan_typeck::{type_check_syntax_module_output, DiagSeverity, Diagnostic};

/// Runs syntax-output typechecking plus static template contract checks.
///
/// Inputs:
/// - `module`: formal syntax-output module to validate.
/// - `resolved`: resolved HIR module used by regular typechecking.
/// - `source_path`: source path used to resolve external template files.
///
/// Output:
/// - Combined regular typecheck diagnostics and template contract diagnostics.
///
/// Transformation:
/// - Runs `terlan_typeck` first, then appends diagnostics for template files,
///   slots, component tags, component props, and struct field paths.
pub(crate) fn type_check_syntax_module_output_with_templates(
    module: &SyntaxModuleOutput,
    resolved: &terlan_hir::ResolvedModule,
    source_path: &Path,
) -> Vec<Diagnostic> {
    let mut diagnostics = type_check_syntax_module_output(module, resolved);
    diagnostics.extend(check_template_declarations_syntax_output(
        module,
        source_path,
    ));
    diagnostics
}

/// Checks template declarations in one syntax-output module.
///
/// Inputs:
/// - `module`: syntax-output module containing template declarations.
/// - `source_path`: source path used to derive the template base directory.
///
/// Output:
/// - Template-specific diagnostics.
///
/// Transformation:
/// - Normalizes template declarations and struct fields before validating
///   external template files.
fn check_template_declarations_syntax_output(
    module: &SyntaxModuleOutput,
    source_path: &Path,
) -> Vec<Diagnostic> {
    let collected =
        crate::commands::artifacts::collect_syntax_template_frontend_inputs(module, source_path);
    let mut diagnostics = collected
        .errors
        .into_iter()
        .map(|error| Diagnostic {
            span: error.span,
            message: error.message,
            severity: DiagSeverity::Error,
        })
        .collect::<Vec<_>>();
    diagnostics.extend(check_template_declarations_from_parts(
        collected.inputs,
        syntax_template_struct_fields(module),
    ));
    diagnostics
}

/// Template declaration shape used by the validator.
///
/// Inputs:
/// - Created from `SyntaxDeclarationPayload::Template`.
///
/// Output:
/// - Template name, source path, props, and diagnostic span.
///
/// Transformation:
/// - Keeps only fields required by template contract checks.
#[derive(Debug, Clone)]
struct TemplateCheckDecl {
    name: String,
    source_path: String,
    resolved_path: String,
    props: Vec<TemplateCheckProp>,
    span: Span,
}

/// Template prop shape used by the validator.
///
/// Inputs:
/// - Created from syntax-output template prop declarations.
///
/// Output:
/// - Prop name, annotation text, and diagnostic span.
///
/// Transformation:
/// - Flattens prop annotation output to text for local type comparisons.
#[derive(Debug, Clone)]
struct TemplateCheckProp {
    name: String,
    annotation: String,
    span: Span,
}

/// Validates external template files and component relationships.
///
/// Inputs:
/// - `base_dir`: base directory for relative template paths.
/// - `templates`: normalized template declarations.
/// - `struct_fields`: known struct fields keyed by struct name.
///
/// Output:
/// - Diagnostics for unreadable templates, parse failures, duplicate tags,
///   slot misuse, component misuse, and field-path misuse.
///
/// Transformation:
/// - Reads and parses template files, indexes component tags, and checks each
///   parsed template against its declared contract.
fn check_template_declarations_from_parts(
    templates: Vec<crate::commands::artifacts::SyntaxTemplateFrontendInput>,
    struct_fields: HashMap<String, HashMap<String, String>>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut checked_templates: Vec<CheckedTemplate> = Vec::new();
    let mut template_indexes_by_tag: HashMap<String, usize> = HashMap::new();
    let mut duplicate_tags = BTreeSet::new();

    for input in templates {
        let template = TemplateCheckDecl {
            name: input.name,
            source_path: input.source_path,
            resolved_path: input.resolved_path.display().to_string(),
            props: input
                .props
                .into_iter()
                .map(|prop| TemplateCheckProp {
                    name: prop.name,
                    annotation: prop.annotation.text,
                    span: prop.span.into(),
                })
                .collect(),
            span: input.span,
        };
        let tag_name = input
            .parsed
            .tag_name
            .clone()
            .unwrap_or_else(|| template.name.clone());
        if let Some(previous_index) = template_indexes_by_tag.get(&tag_name) {
            let previous = &checked_templates[*previous_index].template;
            duplicate_tags.insert(tag_name.clone());
            diagnostics.push(Diagnostic {
                span: template.span,
                message: format!(
                    "duplicate template tag `{}` derived from `{}` ({}) and `{}` ({})",
                    tag_name,
                    previous.source_path,
                    previous.resolved_path,
                    template.source_path,
                    template.resolved_path
                ),
                severity: DiagSeverity::Error,
            });
        } else {
            template_indexes_by_tag.insert(tag_name.clone(), checked_templates.len());
        }
        checked_templates.push(CheckedTemplate {
            template,
            parsed: input.parsed,
        });
    }

    let templates_by_tag = template_indexes_by_tag
        .into_iter()
        .map(|(tag, index)| (tag, &checked_templates[index].template))
        .collect::<HashMap<_, _>>();

    for checked in &checked_templates {
        diagnostics.extend(check_template_slots(
            &checked.template,
            &checked.parsed,
            &struct_fields,
        ));
        diagnostics.extend(check_template_component_tags(
            &checked.template,
            &checked.parsed,
            &templates_by_tag,
            &duplicate_tags,
            &struct_fields,
        ));
    }

    diagnostics
}

/// Parsed template paired with its source declaration.
///
/// Inputs:
/// - Created after an external template file is parsed.
///
/// Output:
/// - Borrowed declaration plus owned parsed template.
///
/// Transformation:
/// - Keeps declaration metadata available while checking parsed nodes.
struct CheckedTemplate {
    template: TemplateCheckDecl,
    parsed: terlan_html::HtmlTemplate,
}

/// Checks prop declarations and slot references for one template.
///
/// Inputs:
/// - `template`: normalized template declaration.
/// - `parsed`: parsed external template.
/// - `struct_fields`: known struct fields keyed by struct name.
///
/// Output:
/// - Diagnostics for reserved props, duplicate props, undeclared slots,
///   invalid `children` usage, and bad field paths.
///
/// Transformation:
/// - Builds prop maps and validates every slot found in template nodes.
fn check_template_slots(
    template: &TemplateCheckDecl,
    parsed: &terlan_html::HtmlTemplate,
    struct_fields: &HashMap<String, HashMap<String, String>>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut prop_names = BTreeSet::new();
    let mut prop_types = HashMap::new();
    for prop in &template.props {
        if prop.name == crate::commands::static_site::TEMPLATE_CHILDREN_SLOT {
            diagnostics.push(Diagnostic {
                span: prop.span,
                message: format!(
                    "template `{}` declares reserved prop `{}`",
                    template.name,
                    crate::commands::static_site::TEMPLATE_CHILDREN_SLOT
                ),
                severity: DiagSeverity::Error,
            });
        }
        if !prop_names.insert(prop.name.clone()) {
            diagnostics.push(Diagnostic {
                span: prop.span,
                message: format!(
                    "duplicate prop `{}` in template `{}`",
                    prop.name, template.name
                ),
                severity: DiagSeverity::Error,
            });
        }
        prop_types.insert(prop.name.clone(), prop.annotation.clone());
    }

    for slot in template_slots(&parsed.nodes) {
        let Some(root) = slot.path.first() else {
            continue;
        };
        if root == crate::commands::static_site::TEMPLATE_CHILDREN_SLOT {
            if slot.path.len() != 1 {
                diagnostics.push(Diagnostic {
                    span: template.span,
                    message: format!(
                        "template `{}` uses invalid children slot `{}`",
                        template.name,
                        slot.path.join(".")
                    ),
                    severity: DiagSeverity::Error,
                });
            }
            continue;
        }
        if !prop_names.contains(root) {
            diagnostics.push(Diagnostic {
                span: template.span,
                message: format!(
                    "template `{}` uses undeclared slot `{}`",
                    template.name,
                    slot.path.join(".")
                ),
                severity: DiagSeverity::Error,
            });
            continue;
        }
        diagnostics.extend(check_template_slot_field_path(
            template,
            slot,
            &prop_types,
            struct_fields,
        ));
    }

    diagnostics
}

/// Checks component tags used by one template.
///
/// Inputs:
/// - `template`: normalized template declaration.
/// - `parsed`: parsed external template.
/// - `templates_by_tag`: component declarations indexed by tag name.
/// - `duplicate_tags`: ambiguous component tag names.
/// - `struct_fields`: known struct fields keyed by struct name.
///
/// Output:
/// - Diagnostics for component tag and prop contract failures.
///
/// Transformation:
/// - Builds the parent prop type map and recursively checks component nodes.
fn check_template_component_tags(
    template: &TemplateCheckDecl,
    parsed: &terlan_html::HtmlTemplate,
    templates_by_tag: &HashMap<String, &TemplateCheckDecl>,
    duplicate_tags: &BTreeSet<String>,
    struct_fields: &HashMap<String, HashMap<String, String>>,
) -> Vec<Diagnostic> {
    let prop_types = template
        .props
        .iter()
        .map(|prop| (prop.name.clone(), prop.annotation.clone()))
        .collect::<HashMap<_, _>>();
    let mut diagnostics = Vec::new();
    check_template_component_nodes(
        template,
        &parsed.nodes,
        templates_by_tag,
        duplicate_tags,
        &prop_types,
        struct_fields,
        &mut diagnostics,
    );
    diagnostics
}

/// Recursively checks component elements in template nodes.
///
/// Inputs:
/// - `template`: normalized template declaration.
/// - `nodes`: parsed template nodes.
/// - `templates_by_tag`: component declarations indexed by tag name.
/// - `duplicate_tags`: ambiguous component tag names.
/// - `prop_types`: parent template prop type map.
/// - `struct_fields`: known struct fields keyed by struct name.
/// - `diagnostics`: output diagnostics buffer.
///
/// Output:
/// - No return value.
///
/// Transformation:
/// - Walks parsed element nodes depth-first and validates component-looking
///   tags.
fn check_template_component_nodes(
    template: &TemplateCheckDecl,
    nodes: &[terlan_html::HtmlNode],
    templates_by_tag: &HashMap<String, &TemplateCheckDecl>,
    duplicate_tags: &BTreeSet<String>,
    prop_types: &HashMap<String, String>,
    struct_fields: &HashMap<String, HashMap<String, String>>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for node in nodes {
        let terlan_html::HtmlNode::Element(element) = node else {
            continue;
        };

        if element.name.contains('-') {
            check_template_component_element(
                template,
                element,
                templates_by_tag,
                duplicate_tags,
                prop_types,
                struct_fields,
                diagnostics,
            );
        }

        check_template_component_nodes(
            template,
            &element.children,
            templates_by_tag,
            duplicate_tags,
            prop_types,
            struct_fields,
            diagnostics,
        );
    }
}

/// Checks one component element against its declaration.
///
/// Inputs:
/// - `template`: parent template declaration.
/// - `element`: parsed component element.
/// - `templates_by_tag`: component declarations indexed by tag.
/// - `duplicate_tags`: ambiguous component tag names.
/// - `prop_types`: parent template prop type map.
/// - `struct_fields`: known struct fields keyed by struct name.
/// - `diagnostics`: output diagnostics buffer.
///
/// Output:
/// - No return value.
///
/// Transformation:
/// - Validates component existence, required props, unknown props, static-text
///   prop compatibility, slot prop compatibility, and missing values.
fn check_template_component_element(
    template: &TemplateCheckDecl,
    element: &terlan_html::HtmlElement,
    templates_by_tag: &HashMap<String, &TemplateCheckDecl>,
    duplicate_tags: &BTreeSet<String>,
    prop_types: &HashMap<String, String>,
    struct_fields: &HashMap<String, HashMap<String, String>>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if duplicate_tags.contains(&element.name) {
        return;
    }

    let Some(component) = templates_by_tag.get(&element.name) else {
        diagnostics.push(Diagnostic {
            span: template.span,
            message: format!(
                "template `{}` references unknown component `<{}>`",
                template.name, element.name
            ),
            severity: DiagSeverity::Error,
        });
        return;
    };

    let component_props = component
        .props
        .iter()
        .map(|prop| (prop.name.clone(), prop.annotation.clone()))
        .collect::<HashMap<_, _>>();
    let attr_names = element
        .attrs
        .iter()
        .map(|attr| attr.name.clone())
        .collect::<BTreeSet<_>>();

    for prop in &component.props {
        if !attr_names.contains(&prop.name) {
            diagnostics.push(Diagnostic {
                span: template.span,
                message: format!(
                    "template `{}` component `<{}>` is missing required prop `{}`",
                    template.name, element.name, prop.name
                ),
                severity: DiagSeverity::Error,
            });
        }
    }

    for attr in &element.attrs {
        let Some(expected_type) = component_props.get(&attr.name) else {
            diagnostics.push(Diagnostic {
                span: template.span,
                message: format!(
                    "template `{}` component `<{}>` has unknown prop `{}`",
                    template.name, element.name, attr.name
                ),
                severity: DiagSeverity::Error,
            });
            continue;
        };

        match &attr.value {
            Some(terlan_html::HtmlAttrValue::Text(_)) => {
                if expected_type != "Text" && expected_type != "Binary" {
                    diagnostics.push(Diagnostic {
                        span: template.span,
                        message: format!(
                            "template `{}` component `<{}>` prop `{}` expects `{}`, but got static text",
                            template.name, element.name, attr.name, expected_type
                        ),
                        severity: DiagSeverity::Error,
                    });
                }
            }
            Some(terlan_html::HtmlAttrValue::Slot(slot)) => {
                let Some(actual_type) = template_slot_type_text(slot, prop_types, struct_fields)
                else {
                    continue;
                };
                if &actual_type != expected_type {
                    diagnostics.push(Diagnostic {
                        span: template.span,
                        message: format!(
                            "template `{}` component `<{}>` prop `{}` expects `{}`, but got `{}`",
                            template.name, element.name, attr.name, expected_type, actual_type
                        ),
                        severity: DiagSeverity::Error,
                    });
                }
            }
            None => {
                diagnostics.push(Diagnostic {
                    span: template.span,
                    message: format!(
                        "template `{}` component `<{}>` prop `{}` requires a value",
                        template.name, element.name, attr.name
                    ),
                    severity: DiagSeverity::Error,
                });
            }
        }
    }
}

/// Checks a dotted slot path against known struct fields.
///
/// Inputs:
/// - `template`: template declaration used for diagnostic spans.
/// - `slot`: parsed slot path.
/// - `prop_types`: template prop type map.
/// - `struct_fields`: known struct fields keyed by struct name.
///
/// Output:
/// - Diagnostics for invalid struct field references.
///
/// Transformation:
/// - Walks the slot path from the root prop through struct field maps until the
///   path ends or a field is missing.
fn check_template_slot_field_path(
    template: &TemplateCheckDecl,
    slot: &terlan_html::HtmlSlot,
    prop_types: &HashMap<String, String>,
    struct_fields: &HashMap<String, HashMap<String, String>>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    if slot.path.len() < 2 {
        return diagnostics;
    }

    let Some(root) = slot.path.first() else {
        return diagnostics;
    };
    let Some(mut current_type) = prop_types.get(root).map(String::as_str) else {
        return diagnostics;
    };

    for field in slot.path.iter().skip(1) {
        let Some(type_name) = simple_template_type_name(current_type) else {
            break;
        };
        let Some(fields) = struct_fields.get(type_name) else {
            break;
        };
        let Some(next_type) = fields.get(field) else {
            diagnostics.push(Diagnostic {
                span: template.span,
                message: format!(
                    "template `{}` uses invalid field path `{}`: struct `{}` has no field `{}`",
                    template.name,
                    slot.path.join("."),
                    type_name,
                    field
                ),
                severity: DiagSeverity::Error,
            });
            break;
        };
        current_type = next_type;
    }

    diagnostics
}

/// Resolves the type text referenced by a slot path.
///
/// Inputs:
/// - `slot`: parsed slot path.
/// - `prop_types`: template prop type map.
/// - `struct_fields`: known struct fields keyed by struct name.
///
/// Output:
/// - Type text for the final slot path segment when resolvable.
///
/// Transformation:
/// - Handles reserved `children` as HTML, then walks struct fields for dotted
///   paths.
fn template_slot_type_text(
    slot: &terlan_html::HtmlSlot,
    prop_types: &HashMap<String, String>,
    struct_fields: &HashMap<String, HashMap<String, String>>,
) -> Option<String> {
    let root = slot.path.first()?;
    if root == crate::commands::static_site::TEMPLATE_CHILDREN_SLOT {
        return Some("Html[Never]".to_string());
    }
    let mut current_type = prop_types.get(root)?.clone();
    for field in slot.path.iter().skip(1) {
        let type_name = simple_template_type_name(&current_type)?;
        let fields = struct_fields.get(type_name)?;
        current_type = fields.get(field)?.clone();
    }
    Some(current_type)
}

/// Collects struct field type maps from syntax output.
///
/// Inputs:
/// - `module`: syntax-output module to scan.
///
/// Output:
/// - Map from struct name to field-name/type-text pairs.
///
/// Transformation:
/// - Filters declarations to structs and records each field annotation text.
fn syntax_template_struct_fields(
    module: &SyntaxModuleOutput,
) -> HashMap<String, HashMap<String, String>> {
    module
        .declarations
        .iter()
        .filter_map(|declaration| {
            let SyntaxDeclarationPayload::Struct { name, fields, .. } = &declaration.payload else {
                return None;
            };
            Some((
                name.clone(),
                fields
                    .iter()
                    .map(|field| (field.name.clone(), field.annotation.text.clone()))
                    .collect::<HashMap<_, _>>(),
            ))
        })
        .collect()
}

/// Extracts a simple nominal type name from type text.
///
/// Inputs:
/// - `type_text`: type annotation text.
///
/// Output:
/// - Type name when the text is a single uppercase identifier.
///
/// Transformation:
/// - Rejects lowercase, parameterized, qualified, or compound type text.
fn simple_template_type_name(type_text: &str) -> Option<&str> {
    let mut chars = type_text.chars();
    let first = chars.next()?;
    if !first.is_ascii_uppercase() {
        return None;
    }
    if chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_') {
        Some(type_text)
    } else {
        None
    }
}

/// Collects every slot reference in parsed template nodes.
///
/// Inputs:
/// - `nodes`: parsed template nodes.
///
/// Output:
/// - Borrowed slot references found in nodes and attributes.
///
/// Transformation:
/// - Recursively walks node trees and gathers text slots plus attribute slots.
fn template_slots(nodes: &[terlan_html::HtmlNode]) -> Vec<&terlan_html::HtmlSlot> {
    let mut slots = Vec::new();
    for node in nodes {
        collect_template_slots(node, &mut slots);
    }
    slots
}

/// Recursively appends slot references from one parsed template node.
///
/// Inputs:
/// - `node`: parsed template node to inspect.
/// - `slots`: output buffer for borrowed slot references.
///
/// Output:
/// - No return value.
///
/// Transformation:
/// - Adds direct slots, attribute slots, and slots nested in child elements.
fn collect_template_slots<'a>(
    node: &'a terlan_html::HtmlNode,
    slots: &mut Vec<&'a terlan_html::HtmlSlot>,
) {
    match node {
        terlan_html::HtmlNode::Slot(slot) => slots.push(slot),
        terlan_html::HtmlNode::Element(element) => {
            for attr in &element.attrs {
                if let Some(terlan_html::HtmlAttrValue::Slot(slot)) = &attr.value {
                    slots.push(slot);
                }
            }
            for child in &element.children {
                collect_template_slots(child, slots);
            }
        }
        terlan_html::HtmlNode::Text(_)
        | terlan_html::HtmlNode::Comment(_)
        | terlan_html::HtmlNode::Doctype(_) => {}
    }
}
