use super::*;

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

fn html_element_output(element: &HtmlElement) -> SyntaxHtmlElementOutput {
    SyntaxHtmlElementOutput {
        name: element.name.clone(),
        attrs: element.attrs.iter().map(html_attr_output).collect(),
        children: element.children.iter().map(html_node_output).collect(),
    }
}

fn html_named_slot_output(slot: &HtmlNamedSlot) -> SyntaxHtmlNamedSlotOutput {
    SyntaxHtmlNamedSlotOutput {
        name: slot.name.clone(),
        children: slot.children.iter().map(html_node_output).collect(),
    }
}

fn html_attr_output(attr: &HtmlAttr) -> SyntaxHtmlAttrOutput {
    SyntaxHtmlAttrOutput {
        name: attr.name.clone(),
        value: attr.value.as_ref().map(html_attr_value_output),
    }
}

fn html_attr_value_output(value: &HtmlAttrValue) -> SyntaxHtmlAttrValueOutput {
    match value {
        HtmlAttrValue::Text(text) => SyntaxHtmlAttrValueOutput::Text { text: text.clone() },
        HtmlAttrValue::Expr(expr) => SyntaxHtmlAttrValueOutput::Expr {
            expr: Box::new(expr_output_with_span(expr, EbnfSourceSpan::default())),
        },
    }
}
