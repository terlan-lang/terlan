/// Renders Terlan documentation lines as an Erlang documentation attribute.
///
/// Input is a sequence of already extracted doc strings. Output is a possibly
/// empty native documentation attribute. The transformation joins source doc
/// lines with newlines and emits `-moduledoc` or `-doc` so BEAM builds can
/// carry EEP-48/HexDocs-readable documentation instead of loose comments.
pub(super) fn render_doc_attribute(attribute: &str, docs: &[String]) -> String {
    if docs.is_empty() {
        return String::new();
    }

    format!(
        "-{} \"{}\".\n\n",
        attribute,
        escape_erlang_string_literal(&docs.join("\n"))
    )
}

/// Escapes text for use in an Erlang string literal.
///
/// Input is raw documentation text. Output is text safe to place between
/// Erlang double quotes. The transformation preserves newlines as `\n` escapes
/// and escapes backslashes, quotes, carriage returns, and tabs.
fn escape_erlang_string_literal(text: &str) -> String {
    text.chars()
        .flat_map(|ch| match ch {
            '\\' => "\\\\".chars().collect::<Vec<_>>(),
            '"' => "\\\"".chars().collect::<Vec<_>>(),
            '\n' => "\\n".chars().collect::<Vec<_>>(),
            '\r' => "\\r".chars().collect::<Vec<_>>(),
            '\t' => "\\t".chars().collect::<Vec<_>>(),
            other => vec![other],
        })
        .collect()
}
