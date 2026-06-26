use serde::{Deserialize, Serialize};

use super::{expr_output_with_span, SyntaxExprOutput};
use crate::{
    ebnf::EbnfSourceSpan,
    parse_tree::{HtmlAttr, HtmlAttrValue, HtmlElement, HtmlNamedSlot, HtmlNode},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
/// HTML node represented in syntax output.
///
/// Inputs:
/// - Parsed HTML block node.
///
/// Output:
/// - Tagged text, expression, element, or named-slot node.
///
/// Transformation:
/// - Converts HTML parser structures into syntax-output records.
pub enum SyntaxHtmlNodeOutput {
    Text { text: String },
    Expr { expr: Box<SyntaxExprOutput> },
    Element { element: SyntaxHtmlElementOutput },
    NamedSlot { slot: SyntaxHtmlNamedSlotOutput },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// HTML element represented in syntax output.
///
/// Inputs:
/// - Parsed HTML element.
///
/// Output:
/// - Name, attributes, and children.
///
/// Transformation:
/// - Recursively maps child nodes and attributes.
pub struct SyntaxHtmlElementOutput {
    pub name: String,
    pub attrs: Vec<SyntaxHtmlAttrOutput>,
    pub children: Vec<SyntaxHtmlNodeOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Named HTML slot represented in syntax output.
///
/// Inputs:
/// - Parsed slot name and children.
///
/// Output:
/// - Named-slot record.
///
/// Transformation:
/// - Preserves slot children as syntax-output HTML nodes.
pub struct SyntaxHtmlNamedSlotOutput {
    pub name: String,
    pub children: Vec<SyntaxHtmlNodeOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// HTML attribute represented in syntax output.
///
/// Inputs:
/// - Parsed HTML attribute.
///
/// Output:
/// - Name and optional value.
///
/// Transformation:
/// - Maps static or expression-backed values into tagged output.
pub struct SyntaxHtmlAttrOutput {
    pub name: String,
    pub value: Option<SyntaxHtmlAttrValueOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
/// HTML attribute value represented in syntax output.
///
/// Inputs:
/// - Parsed attribute value.
///
/// Output:
/// - Static text or expression payload.
///
/// Transformation:
/// - Boxes expression values so attribute payload shape remains compact and
///   recursive.
pub enum SyntaxHtmlAttrValueOutput {
    Text { text: String },
    Expr { expr: Box<SyntaxExprOutput> },
}

/// Converts a parsed HTML node into syntax output.
///
/// Inputs:
/// - `node`: parser-owned HTML node.
///
/// Output:
/// - `SyntaxHtmlNodeOutput` consumed by type checking and backend lowering.
///
/// Transformation:
/// - Recursively projects text, embedded expressions, elements, and named
///   slots into the stable syntax-output DTO layer.
pub(super) fn html_node_output(node: &HtmlNode) -> SyntaxHtmlNodeOutput {
    match node {
        HtmlNode::Text(text) => SyntaxHtmlNodeOutput::Text { text: text.clone() },
        HtmlNode::Expr(expr) => SyntaxHtmlNodeOutput::Expr {
            expr: Box::new(expr_output_with_span(expr, EbnfSourceSpan::default())),
        },
        HtmlNode::Element(element) => SyntaxHtmlNodeOutput::Element {
            element: html_element_output(element),
        },
        HtmlNode::NamedSlot(slot) => SyntaxHtmlNodeOutput::NamedSlot {
            slot: html_named_slot_output(slot),
        },
    }
}

/// Converts a parsed HTML element into syntax output.
///
/// Inputs:
/// - `element`: parser-owned HTML element with attributes and children.
///
/// Output:
/// - `SyntaxHtmlElementOutput` with converted attributes and children.
///
/// Transformation:
/// - Clones the element name and recursively converts nested syntax structures.
fn html_element_output(element: &HtmlElement) -> SyntaxHtmlElementOutput {
    SyntaxHtmlElementOutput {
        name: element.name.clone(),
        attrs: element.attrs.iter().map(html_attr_output).collect(),
        children: element.children.iter().map(html_node_output).collect(),
    }
}

/// Converts a parsed named HTML slot into syntax output.
///
/// Inputs:
/// - `slot`: parser-owned named slot.
///
/// Output:
/// - `SyntaxHtmlNamedSlotOutput` with converted slot children.
///
/// Transformation:
/// - Preserves the slot name and recursively projects child nodes.
fn html_named_slot_output(slot: &HtmlNamedSlot) -> SyntaxHtmlNamedSlotOutput {
    SyntaxHtmlNamedSlotOutput {
        name: slot.name.clone(),
        children: slot.children.iter().map(html_node_output).collect(),
    }
}

/// Converts a parsed HTML attribute into syntax output.
///
/// Inputs:
/// - `attr`: parser-owned attribute.
///
/// Output:
/// - `SyntaxHtmlAttrOutput` with optional converted value.
///
/// Transformation:
/// - Preserves the attribute name and converts text or expression values when
///   present.
fn html_attr_output(attr: &HtmlAttr) -> SyntaxHtmlAttrOutput {
    SyntaxHtmlAttrOutput {
        name: attr.name.clone(),
        value: attr.value.as_ref().map(html_attr_value_output),
    }
}

/// Converts a parsed HTML attribute value into syntax output.
///
/// Inputs:
/// - `value`: parser-owned attribute value.
///
/// Output:
/// - `SyntaxHtmlAttrValueOutput` preserving text or embedded expression shape.
///
/// Transformation:
/// - Converts embedded expressions through the normal syntax-output expression
///   path with a default source span until precise HTML expression spans are
///   threaded through the parser.
fn html_attr_value_output(value: &HtmlAttrValue) -> SyntaxHtmlAttrValueOutput {
    match value {
        HtmlAttrValue::Text(text) => SyntaxHtmlAttrValueOutput::Text { text: text.clone() },
        HtmlAttrValue::Expr(expr) => SyntaxHtmlAttrValueOutput::Expr {
            expr: Box::new(expr_output_with_span(expr, EbnfSourceSpan::default())),
        },
    }
}
