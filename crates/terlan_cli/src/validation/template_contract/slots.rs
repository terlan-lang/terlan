/// Template interpolation use context.
///
/// Inputs:
/// - Produced while walking parsed template nodes.
///
/// Output:
/// - Whether a slot appears as element/text content or as a whole attribute
///   value.
///
/// Transformation:
/// - Keeps type-renderability checks precise without changing the public
///   `terlan_html::HtmlSlot` payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TemplateSlotContext {
    Text,
    Attribute,
}

/// Template slot plus render context.
///
/// Inputs:
/// - Borrowed parsed template slot and its surrounding context.
///
/// Output:
/// - Validation payload used by template contract checks.
///
/// Transformation:
/// - Carries context separately from the parser AST so existing render paths do
///   not need to change while contract validation grows type awareness.
#[derive(Debug, Clone, Copy)]
pub(super) struct TemplateSlotUse<'a> {
    pub(super) slot: &'a terlan_html::HtmlSlot,
    pub(super) context: TemplateSlotContext,
}

/// Returns a renderability diagnostic for one typed template slot.
///
/// Inputs:
/// - `slot_use`: slot plus text/attribute context.
/// - `type_text`: resolved Terlan type text.
/// - `template_name`: template name for diagnostics.
///
/// Output:
/// - Diagnostic message when the type cannot render in that context.
/// - `None` when the type is renderable.
///
/// Transformation:
/// - Applies context-aware `${...}` typechecking rules for templates: scalar
///   values render as text, `Template.Html` renders only as body/text HTML, and
///   complex nominal values must be projected to a renderable field.
pub(super) fn template_slot_renderability_error(
    slot_use: &TemplateSlotUse<'_>,
    type_text: &str,
    template_name: &str,
) -> Option<String> {
    match slot_use.context {
        TemplateSlotContext::Text if is_text_renderable_template_type(type_text) => None,
        TemplateSlotContext::Text => Some(format!(
            "template `{}` slot `{}` has non-renderable type `{}`{}",
            template_name,
            slot_use.slot.path.join("."),
            type_text,
            template_slot_location_suffix(slot_use.slot)
        )),
        TemplateSlotContext::Attribute if is_attribute_renderable_template_type(type_text) => None,
        TemplateSlotContext::Attribute => Some(format!(
            "template `{}` attribute slot `{}` has non-renderable type `{}`{}",
            template_name,
            slot_use.slot.path.join("."),
            type_text,
            template_slot_location_suffix(slot_use.slot)
        )),
    }
}

/// Formats source-location text for a parsed template slot.
///
/// Inputs:
/// - `slot`: parsed template interpolation slot.
///
/// Output:
/// - Empty text when no span is available.
/// - Stable human-readable line/column suffix when the parser supplied a span.
///
/// Transformation:
/// - Converts the parser's zero-based byte offsets into one-based display
///   columns while preserving the line number recorded by the HTML parser.
pub(super) fn template_slot_location_suffix(slot: &terlan_html::HtmlSlot) -> String {
    match slot.span {
        Some(span) => format!(
            " (template line {}, columns {}-{})",
            span.line,
            span.start + 1,
            span.end
        ),
        None => String::new(),
    }
}

/// Returns whether a type can render in text/body template context.
///
/// Inputs:
/// - `type_text`: resolved template slot type text.
///
/// Output:
/// - `true` for scalar text-renderable values and template HTML fragments.
///
/// Transformation:
/// - Normalizes whitespace before comparing current public type spellings.
fn is_text_renderable_template_type(type_text: &str) -> bool {
    is_scalar_renderable_template_type(type_text) || is_template_html_type_text(type_text)
}

/// Returns whether a type can render as an HTML attribute value.
///
/// Inputs:
/// - `type_text`: resolved template slot type text.
///
/// Output:
/// - `true` for scalar attribute-renderable values.
///
/// Transformation:
/// - Excludes `Template.Html` because HTML fragments must not be injected into
///   attribute values.
fn is_attribute_renderable_template_type(type_text: &str) -> bool {
    is_scalar_renderable_template_type(type_text)
}

/// Returns whether a type is a scalar template-renderable value.
///
/// Inputs:
/// - `type_text`: resolved template slot type text.
///
/// Output:
/// - `true` for built-in scalar values that renderer/backends can stringify.
///
/// Transformation:
/// - Accepts current `Text`/`Binary` spellings plus the `String` spelling used
///   by newer stdlib-facing examples.
fn is_scalar_renderable_template_type(type_text: &str) -> bool {
    matches!(
        normalize_template_type_text(type_text).as_str(),
        "Text" | "Binary" | "String" | "Int" | "Float" | "Bool"
    )
}

/// Returns whether a type denotes template HTML.
///
/// Inputs:
/// - `type_text`: resolved template slot type text.
///
/// Output:
/// - `true` for current public/internal HTML fragment spellings.
///
/// Transformation:
/// - Removes whitespace and compares known type spellings without invoking the
///   full typechecker.
fn is_template_html_type_text(type_text: &str) -> bool {
    matches!(
        normalize_template_type_text(type_text).as_str(),
        "Template.Html" | "std.template.Template.Html" | "Html[Never]" | "Html[Dynamic]"
    )
}

/// Normalizes template type text for local renderability checks.
///
/// Inputs:
/// - `type_text`: source-level type text.
///
/// Output:
/// - Text without whitespace.
///
/// Transformation:
/// - Provides stable comparisons for compact and spaced type annotations.
fn normalize_template_type_text(type_text: &str) -> String {
    type_text.chars().filter(|ch| !ch.is_whitespace()).collect()
}

/// Collects every slot reference in parsed template nodes.
///
/// Inputs:
/// - `nodes`: parsed template nodes.
///
/// Output:
/// - Borrowed slot references found in nodes and attributes with context.
///
/// Transformation:
/// - Recursively walks node trees and gathers text slots plus attribute slots.
pub(super) fn template_slot_uses(nodes: &[terlan_html::HtmlNode]) -> Vec<TemplateSlotUse<'_>> {
    let mut slots = Vec::new();
    for node in nodes {
        collect_template_slot_uses(node, &mut slots);
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
fn collect_template_slot_uses<'a>(
    node: &'a terlan_html::HtmlNode,
    slots: &mut Vec<TemplateSlotUse<'a>>,
) {
    match node {
        terlan_html::HtmlNode::Slot(slot) => slots.push(TemplateSlotUse {
            slot,
            context: TemplateSlotContext::Text,
        }),
        terlan_html::HtmlNode::Element(element) => {
            for attr in &element.attrs {
                if let Some(terlan_html::HtmlAttrValue::Slot(slot)) = &attr.value {
                    slots.push(TemplateSlotUse {
                        slot,
                        context: TemplateSlotContext::Attribute,
                    });
                }
            }
            for child in &element.children {
                collect_template_slot_uses(child, slots);
            }
        }
        terlan_html::HtmlNode::Text(_)
        | terlan_html::HtmlNode::Comment(_)
        | terlan_html::HtmlNode::Doctype(_) => {}
    }
}
