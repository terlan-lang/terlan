use crate::parse_tree::{HtmlAttr, HtmlAttrValue, HtmlNode};

use super::format_expr;

/// Formats an HTML/raw block expression.
///
/// Inputs:
/// - `name`: raw block or macro name.
/// - `nodes`: parsed HTML nodes in source order.
/// - `indent`: current formatter indentation level.
///
/// Output:
/// - Canonical block source text.
///
/// Transformation:
/// - Formats children one per line and closes at the parent indentation level.
pub(super) fn format_html_block(name: &str, nodes: &[HtmlNode], indent: usize) -> String {
    let spacing = "    ".repeat(indent);
    let mut out = format!("{name} {{\n");
    for node in nodes {
        out.push_str(&format_html_node(node, indent + 1));
        out.push('\n');
    }
    out.push_str(&spacing);
    out.push('}');
    out
}

/// Formats one HTML node.
///
/// Inputs:
/// - `node`: parsed HTML node.
/// - `indent`: current formatter indentation level.
///
/// Output:
/// - HTML source fragment.
///
/// Transformation:
/// - Formats text, interpolation, named slots, and elements recursively.
fn format_html_node(node: &HtmlNode, indent: usize) -> String {
    let spacing = "    ".repeat(indent);
    match node {
        HtmlNode::Text(text) => format!("{}{}", spacing, text),
        HtmlNode::Expr(expr) => format!("{}{{{}}}", spacing, format_expr(expr, indent)),
        HtmlNode::NamedSlot(slot) => {
            let mut out = format!("{}@{} {{\n", spacing, slot.name);
            for child in &slot.children {
                out.push_str(&format_html_node(child, indent + 1));
                out.push('\n');
            }
            out.push_str(&spacing);
            out.push('}');
            out
        }
        HtmlNode::Element(element) => {
            let attrs = format_html_attrs(&element.attrs);
            if element.children.is_empty() {
                return format!("{}<{}{} />", spacing, element.name, attrs);
            }

            let mut out = format!("{}<{}{}>\n", spacing, element.name, attrs);
            for child in &element.children {
                out.push_str(&format_html_node(child, indent + 1));
                out.push('\n');
            }
            out.push_str(&spacing);
            out.push_str("</");
            out.push_str(&element.name);
            out.push('>');
            out
        }
    }
}

/// Formats HTML attributes.
///
/// Inputs:
/// - `attrs`: parsed attributes.
///
/// Output:
/// - Sorted attribute source text.
///
/// Transformation:
/// - Sorts by attribute name for deterministic output and formats static or
///   expression values.
fn format_html_attrs(attrs: &[HtmlAttr]) -> String {
    let mut attrs = attrs.iter().collect::<Vec<_>>();
    attrs.sort_by(|left, right| left.name.cmp(&right.name));
    attrs
        .into_iter()
        .map(|attr| match &attr.value {
            None => format!(" {}", attr.name),
            Some(HtmlAttrValue::Text(value)) => format!(" {}=\"{}\"", attr.name, value),
            Some(HtmlAttrValue::Expr(expr)) => {
                format!(" {}={{{}}}", attr.name, format_expr(expr, 0))
            }
        })
        .collect::<Vec<_>>()
        .join("")
}
