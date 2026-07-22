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
///   `crate::terlan_html::HtmlSlot` payload.
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
    pub(super) slot: &'a crate::terlan_html::HtmlSlot,
    pub(super) context: TemplateSlotContext,
    pub(super) attr_name: Option<&'a str>,
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
        TemplateSlotContext::Attribute
            if slot_use.attr_name.is_some()
                && optional_attribute_inner_type(type_text)
                    .as_deref()
                    .is_some_and(|inner_type| {
                        is_optional_attribute_renderable_template_type(
                            slot_use.attr_name.unwrap_or(""),
                            inner_type,
                        )
                    }) =>
        {
            None
        }
        TemplateSlotContext::Attribute
            if slot_use.attr_name.is_some()
                && optional_attribute_inner_type(type_text).is_some() =>
        {
            Some(format!(
                "template `{}` optional attribute `{}` slot `{}` has non-renderable type `{}`{}",
                template_name,
                slot_use.attr_name.unwrap_or(""),
                slot_use.slot.path.join("."),
                type_text,
                template_slot_location_suffix(slot_use.slot)
            ))
        }
        TemplateSlotContext::Attribute
            if slot_use.attr_name.is_some_and(is_url_template_attribute)
                && is_url_attribute_renderable_template_type(type_text) =>
        {
            None
        }
        TemplateSlotContext::Attribute
            if slot_use.attr_name.is_some_and(is_url_template_attribute) =>
        {
            Some(format!(
                "template `{}` URL attribute `{}` slot `{}` has non-renderable type `{}`{}",
                template_name,
                slot_use.attr_name.unwrap_or(""),
                slot_use.slot.path.join("."),
                type_text,
                template_slot_location_suffix(slot_use.slot)
            ))
        }
        TemplateSlotContext::Attribute
            if slot_use
                .attr_name
                .is_some_and(is_boolean_template_attribute)
                && is_bool_template_type(type_text) =>
        {
            None
        }
        TemplateSlotContext::Attribute
            if slot_use
                .attr_name
                .is_some_and(is_boolean_template_attribute) =>
        {
            Some(format!(
                "template `{}` boolean attribute `{}` slot `{}` has non-renderable type `{}`{}",
                template_name,
                slot_use.attr_name.unwrap_or(""),
                slot_use.slot.path.join("."),
                type_text,
                template_slot_location_suffix(slot_use.slot)
            ))
        }
        TemplateSlotContext::Attribute
            if slot_use
                .attr_name
                .is_some_and(is_token_list_template_attribute)
                && is_token_list_attribute_renderable_template_type(type_text) =>
        {
            None
        }
        TemplateSlotContext::Attribute
            if slot_use
                .attr_name
                .is_some_and(is_token_list_template_attribute)
                && is_collection_template_type(type_text) =>
        {
            Some(format!(
                "template `{}` token-list attribute `{}` slot `{}` has non-renderable type `{}`{}",
                template_name,
                slot_use.attr_name.unwrap_or(""),
                slot_use.slot.path.join("."),
                type_text,
                template_slot_location_suffix(slot_use.slot)
            ))
        }
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
pub(super) fn template_slot_location_suffix(slot: &crate::terlan_html::HtmlSlot) -> String {
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

/// Returns whether an attribute carries URL-like content.
///
/// Inputs:
/// - `name`: HTML attribute name.
///
/// Output:
/// - `true` for attributes whose interpolation should satisfy URL rendering
///   rules.
///
/// Transformation:
/// - Normalizes current HTML attribute spelling to lowercase before matching a
///   conservative first set of URL-bearing attributes.
fn is_url_template_attribute(name: &str) -> bool {
    crate::terlan_html::template_attribute_slot_kind(name)
        == crate::terlan_html::TemplateAttributeSlotKind::Url
}

/// Returns whether an attribute carries boolean presence semantics.
///
/// Inputs:
/// - `name`: HTML attribute name.
///
/// Output:
/// - `true` for attributes whose interpolation should satisfy boolean
///   rendering rules.
///
/// Transformation:
/// - Normalizes current HTML attribute spelling to lowercase before matching a
///   conservative first set of boolean HTML attributes.
fn is_boolean_template_attribute(name: &str) -> bool {
    crate::terlan_html::template_attribute_slot_kind(name)
        == crate::terlan_html::TemplateAttributeSlotKind::Boolean
}

/// Returns whether an attribute carries whitespace-separated token-list
/// semantics.
///
/// Inputs:
/// - `name`: HTML attribute name.
///
/// Output:
/// - `true` for attributes whose interpolation may be a scalar token string or
///   a collection of string-like tokens.
///
/// Transformation:
/// - Normalizes current HTML attribute spelling to lowercase before matching a
///   conservative first set of token-list attributes.
fn is_token_list_template_attribute(name: &str) -> bool {
    crate::terlan_html::template_attribute_slot_kind(name)
        == crate::terlan_html::TemplateAttributeSlotKind::TokenList
}

/// Returns whether an optional attribute wrapper can render for a named
/// attribute.
///
/// Inputs:
/// - `attr_name`: HTML attribute name.
/// - `inner_type`: normalized `Option[T]` contained type.
///
/// Output:
/// - `true` when `T` satisfies the same renderability rule the attribute would
///   apply to a non-optional value.
///
/// Transformation:
/// - Reuses URL, boolean, and generic attribute rules so optional attributes do
///   not weaken the target-specific contract.
fn is_optional_attribute_renderable_template_type(attr_name: &str, inner_type: &str) -> bool {
    if is_url_template_attribute(attr_name) {
        is_url_attribute_renderable_template_type(inner_type)
    } else if is_boolean_template_attribute(attr_name) {
        is_bool_template_type(inner_type)
    } else if is_token_list_template_attribute(attr_name) {
        is_token_list_attribute_renderable_template_type(inner_type)
    } else {
        is_attribute_renderable_template_type(inner_type)
    }
}

/// Returns whether a type can render as a token-list attribute value.
///
/// Inputs:
/// - `type_text`: resolved Terlan type text.
///
/// Output:
/// - `true` for string-like scalar token values and collections containing
///   string-like token values.
///
/// Transformation:
/// - Keeps collection token validation stricter than generic attributes so
///   `List[User]` cannot be accidentally stringified into a class list.
fn is_token_list_attribute_renderable_template_type(type_text: &str) -> bool {
    let normalized = normalize_template_type_text(type_text);
    is_token_template_type(&normalized)
        || collection_inner_from_normalized_type(&normalized).is_some_and(is_token_template_type)
}

/// Returns whether a type is any recognized collection spelling.
fn is_collection_template_type(type_text: &str) -> bool {
    let normalized = normalize_template_type_text(type_text);
    collection_inner_from_normalized_type(&normalized).is_some()
}

/// Returns whether a type can render as a URL attribute value.
///
/// Inputs:
/// - `type_text`: resolved Terlan type text.
///
/// Output:
/// - `true` for string-like URL text and explicit `std.net.Uri.Uri` values.
///
/// Transformation:
/// - Keeps URL attributes stricter than generic attributes so numeric and
///   boolean values cannot accidentally become URLs.
fn is_url_attribute_renderable_template_type(type_text: &str) -> bool {
    matches!(
        normalize_template_type_text(type_text).as_str(),
        "Text" | "Binary" | "String" | "Uri" | "std.net.Uri.Uri"
    )
}

/// Returns whether a type denotes a boolean template value.
///
/// Inputs:
/// - `type_text`: resolved Terlan type text.
///
/// Output:
/// - `true` for current Bool spellings.
///
/// Transformation:
/// - Normalizes whitespace and accepts both prelude and fully-qualified core
///   Bool spellings.
fn is_bool_template_type(type_text: &str) -> bool {
    matches!(
        normalize_template_type_text(type_text).as_str(),
        "Bool" | "std.core.Bool.Bool"
    )
}

/// Returns whether a type denotes a string-like token value.
fn is_token_template_type(type_text: &str) -> bool {
    matches!(
        normalize_template_type_text(type_text).as_str(),
        "Text" | "Binary" | "String"
    )
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

/// Extracts the wrapped type from current public `Option[T]` spellings.
///
/// Inputs:
/// - `type_text`: resolved template slot type text.
///
/// Output:
/// - `Some(T)` for `Option[T]` and fully qualified core Option spellings.
/// - `None` for non-option types or malformed generic text.
///
/// Transformation:
/// - Normalizes whitespace and strips one outer Option wrapper without parsing
///   arbitrary type syntax.
fn optional_attribute_inner_type(type_text: &str) -> Option<String> {
    let normalized = normalize_template_type_text(type_text);
    option_inner_from_normalized_type(&normalized).map(ToOwned::to_owned)
}

/// Returns the inner type for a normalized Option spelling.
fn option_inner_from_normalized_type(type_text: &str) -> Option<&str> {
    const SHORT_PREFIX: &str = "Option[";
    const QUALIFIED_PREFIX: &str = "std.core.Option.Option[";

    let inner = type_text
        .strip_prefix(SHORT_PREFIX)
        .or_else(|| type_text.strip_prefix(QUALIFIED_PREFIX))?
        .strip_suffix(']')?;
    if inner.is_empty() {
        None
    } else {
        Some(inner)
    }
}

/// Returns the inner type for normalized collection spellings.
fn collection_inner_from_normalized_type(type_text: &str) -> Option<&str> {
    const PREFIXES: &[&str] = &[
        "List[",
        "std.collections.List.List[",
        "Set[",
        "std.collections.Set.Set[",
    ];

    let inner = PREFIXES
        .iter()
        .find_map(|prefix| type_text.strip_prefix(prefix))?
        .strip_suffix(']')?;
    if inner.is_empty() {
        None
    } else {
        Some(inner)
    }
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
pub(super) fn template_slot_uses(
    nodes: &[crate::terlan_html::HtmlNode],
) -> Vec<TemplateSlotUse<'_>> {
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
    node: &'a crate::terlan_html::HtmlNode,
    slots: &mut Vec<TemplateSlotUse<'a>>,
) {
    match node {
        crate::terlan_html::HtmlNode::Slot(slot) => slots.push(TemplateSlotUse {
            slot,
            context: TemplateSlotContext::Text,
            attr_name: None,
        }),
        crate::terlan_html::HtmlNode::Element(element) => {
            for attr in &element.attrs {
                if let Some(crate::terlan_html::HtmlAttrValue::Slot(slot)) = &attr.value {
                    slots.push(TemplateSlotUse {
                        slot,
                        context: TemplateSlotContext::Attribute,
                        attr_name: Some(attr.name.as_str()),
                    });
                }
            }
            for child in &element.children {
                collect_template_slot_uses(child, slots);
            }
        }
        crate::terlan_html::HtmlNode::Text(_)
        | crate::terlan_html::HtmlNode::Comment(_)
        | crate::terlan_html::HtmlNode::Doctype(_) => {}
    }
}
