//! Anchored Markdown headings that accept focus during fragment navigation.

use std::fmt::{self, Write};

use comrak::html::{format_document_with_formatter, format_node_default, ChildRendering, Context};
use comrak::nodes::{Node, NodeValue};
use comrak::options::Plugins;
use comrak::{parse_document, Arena, Options};

/// Preserves Comrak's unique anchors and link rendering while making headings focusable.
pub(super) fn render(source: &str) -> String {
    let mut options = Options::default();
    options.extension.header_id_prefix = Some(String::new());
    options.extension.header_id_prefix_in_href = true;
    let arena = Arena::new();
    let document = parse_document(&arena, source, &options);
    let mut output = String::new();
    format_document_with_formatter(
        document,
        &options,
        &mut output,
        &Plugins::default(),
        heading,
        (),
    )
    .expect("formatting Markdown into a String cannot fail");
    output
}

fn heading(
    context: &mut Context<()>,
    node: Node<'_>,
    entering: bool,
) -> Result<ChildRendering, fmt::Error> {
    if let NodeValue::Heading(heading) = node.data().value {
        if entering {
            let id = context.anchorizer.anchorize(&node.collect_text());
            context.cr()?;
            write!(
                context,
                "<h{} id=\"{}\" tabindex=\"-1\">",
                heading.level, id
            )?;
            context.current_anchorized_id = Some(id);
            return Ok(ChildRendering::HTML);
        }
    }
    format_node_default(context, node, entering)
}
