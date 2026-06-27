use std::path::{Path, PathBuf};

use comrak::{markdown_to_html, Options};
use cssparser::{Parser, ParserInput};
use html5ever::tendril::StrTendril;
use html5ever::tokenizer::{
    BufferQueue, TagKind, Token, TokenSink, TokenSinkResult, Tokenizer, TokenizerOpts,
};
use html5ever::TokenizerResult;

use crate::terlan_html::header::template_body_source;
use crate::terlan_html::{
    template_tag_from_path, HtmlAttr, HtmlAttrValue, HtmlDiagnostic, HtmlElement, HtmlNode,
    HtmlSlot, HtmlSpan, HtmlTemplate, MarkdownDocument, TERLAN_MARKDOWN_TEMPLATE_SUFFIX,
};

/// Parses either an HTML or Markdown Terlan template.
///
/// Inputs: template `source` and `path`. Output: parsed `HtmlTemplate` or
/// diagnostics. Transformation: dispatches by filename suffix and normalizes
/// both formats into the same HTML node tree.
pub fn parse_template(
    source: impl AsRef<str>,
    path: impl AsRef<Path>,
) -> Result<HtmlTemplate, Vec<HtmlDiagnostic>> {
    let path = path.as_ref();
    let file_name = match path.file_name().and_then(|name| name.to_str()) {
        Some(file_name) => file_name,
        None => {
            return Err(vec![HtmlDiagnostic::new(
                Some(path.to_path_buf()),
                "missing template filename",
            )])
        }
    };

    if file_name.ends_with(TERLAN_MARKDOWN_TEMPLATE_SUFFIX) {
        parse_markdown_template(source, path)
    } else {
        parse_html_template(source, path)
    }
}

/// Parses a `.terl.html` template.
///
/// Inputs: HTML template source and path. Output: named `HtmlTemplate` or
/// diagnostics. Transformation: derives the tag name and tokenizes the source
/// with slot interpolation enabled.
pub fn parse_html_template(
    source: impl AsRef<str>,
    path: impl AsRef<Path>,
) -> Result<HtmlTemplate, Vec<HtmlDiagnostic>> {
    let path = path.as_ref();
    let tag_name = match template_tag_from_path(path) {
        Ok(tag_name) => tag_name,
        Err(diagnostic) => return Err(vec![diagnostic]),
    };

    let body = template_body_source(source.as_ref(), path)?;
    match parse_html_nodes(&body, path) {
        Ok(nodes) => Ok(HtmlTemplate {
            source_path: Some(path.to_path_buf()),
            tag_name: Some(tag_name),
            nodes,
        }),
        Err(diagnostics) => Err(diagnostics),
    }
}

/// Parses a `.terl.md` template.
///
/// Inputs: Markdown template source and path. Output: named `HtmlTemplate` or
/// diagnostics. Transformation: renders Markdown to HTML, then parses the HTML
/// into template nodes with slot interpolation enabled.
pub fn parse_markdown_template(
    source: impl AsRef<str>,
    path: impl AsRef<Path>,
) -> Result<HtmlTemplate, Vec<HtmlDiagnostic>> {
    let path = path.as_ref();
    let tag_name = match template_tag_from_path(path) {
        Ok(tag_name) => tag_name,
        Err(diagnostic) => return Err(vec![diagnostic]),
    };
    let body = template_body_source(source.as_ref(), path)?;
    let rendered_html = markdown_to_html(&body, &Options::default());

    match parse_html_nodes(&rendered_html, path) {
        Ok(nodes) => Ok(HtmlTemplate {
            source_path: Some(path.to_path_buf()),
            tag_name: Some(tag_name),
            nodes,
        }),
        Err(diagnostics) => Err(diagnostics),
    }
}

/// Parses Markdown into a document payload.
///
/// Inputs: Markdown source and path. Output: `MarkdownDocument` or diagnostics.
/// Transformation: preserves raw source, renders HTML, and parses rendered HTML
/// into nodes.
pub fn parse_markdown(
    source: impl AsRef<str>,
    path: impl AsRef<Path>,
) -> Result<MarkdownDocument, Vec<HtmlDiagnostic>> {
    let path = path.as_ref();
    let raw_source = template_body_source(source.as_ref(), path)?;
    let rendered_html = markdown_to_html(&raw_source, &Options::default());
    let nodes = parse_html_nodes(&rendered_html, path)?;

    Ok(MarkdownDocument {
        source_path: Some(path.to_path_buf()),
        raw_source,
        rendered_html,
        nodes,
    })
}

/// Validates rendered HTML output without treating braces as slots.
///
/// Inputs: HTML source and path. Output: `Ok(())` or diagnostics.
/// Transformation: tokenizes HTML with slot parsing disabled.
pub fn validate_html_output(
    source: impl AsRef<str>,
    path: impl AsRef<Path>,
) -> Result<(), Vec<HtmlDiagnostic>> {
    parse_html_nodes_without_slots(source, path).map(|_| ())
}

/// Validates CSS source with the CSS parser.
///
/// Inputs: CSS source and path. Output: `Ok(())` or diagnostics.
/// Transformation: asks `cssparser` to reject error tokens and maps parser
/// locations into Terlan HTML diagnostics.
pub fn validate_css(
    source: impl AsRef<str>,
    path: impl AsRef<Path>,
) -> Result<(), Vec<HtmlDiagnostic>> {
    let path = path.as_ref();
    let mut input = ParserInput::new(source.as_ref());
    let mut parser = Parser::new(&mut input);

    parser.expect_no_error_token().map_err(|error| {
        vec![HtmlDiagnostic::new(
            Some(path.to_path_buf()),
            format!(
                "CSS parse error at {}:{}: {}",
                error.location.line, error.location.column, error.kind
            ),
        )]
    })
}

/// Parses HTML nodes with Terlan slot interpolation enabled.
///
/// Inputs: HTML source and path. Output: node tree or diagnostics.
/// Transformation: delegates to the shared tokenizer path with slot parsing on.
fn parse_html_nodes(
    source: impl AsRef<str>,
    path: impl AsRef<Path>,
) -> Result<Vec<HtmlNode>, Vec<HtmlDiagnostic>> {
    parse_html_nodes_with_slot_parsing(source, path, true)
}

/// Parses HTML nodes with slot interpolation disabled.
///
/// Inputs: HTML source and path. Output: node tree or diagnostics.
/// Transformation: delegates to the shared tokenizer path with slot parsing
/// off for already-rendered output validation.
fn parse_html_nodes_without_slots(
    source: impl AsRef<str>,
    path: impl AsRef<Path>,
) -> Result<Vec<HtmlNode>, Vec<HtmlDiagnostic>> {
    parse_html_nodes_with_slot_parsing(source, path, false)
}

/// Parses HTML nodes through the shared tokenizer pipeline.
///
/// Inputs: source, path, and `parse_slots` flag. Output: node tree or
/// diagnostics. Transformation: feeds html5ever tokens into `TemplateBuilder`
/// and finishes the accumulated tree.
fn parse_html_nodes_with_slot_parsing(
    source: impl AsRef<str>,
    path: impl AsRef<Path>,
    parse_slots: bool,
) -> Result<Vec<HtmlNode>, Vec<HtmlDiagnostic>> {
    let path = path.as_ref();
    let sink = TemplateTokenSink::new(path.to_path_buf(), parse_slots);
    let tokenizer = Tokenizer::new(
        sink,
        TokenizerOpts {
            exact_errors: true,
            ..TokenizerOpts::default()
        },
    );
    let input = BufferQueue::default();
    input.push_back(StrTendril::from(source.as_ref()));

    while let TokenizerResult::Script(_) = tokenizer.feed(&input) {}
    tokenizer.end();

    let builder = tokenizer.sink.into_builder();
    builder.finish()
}

/// html5ever token sink that forwards tokens into a template builder.
///
/// Inputs: tokenizer events. Output: owned `TemplateBuilder` after parsing.
/// Transformation: stores the mutable builder in a `RefCell` because
/// html5ever's sink trait receives shared references.
struct TemplateTokenSink {
    builder: std::cell::RefCell<TemplateBuilder>,
}

impl TemplateTokenSink {
    /// Creates a token sink for one template path.
    ///
    /// Inputs: source path and slot parsing flag. Output: token sink.
    /// Transformation: initializes an empty builder behind interior mutability.
    fn new(path: PathBuf, parse_slots: bool) -> Self {
        Self {
            builder: std::cell::RefCell::new(TemplateBuilder::new(path, parse_slots)),
        }
    }

    /// Extracts the completed builder.
    ///
    /// Inputs: token sink after tokenization. Output: inner `TemplateBuilder`.
    /// Transformation: consumes the sink and unwraps its `RefCell` storage.
    fn into_builder(self) -> TemplateBuilder {
        self.builder.into_inner()
    }
}

impl TokenSink for TemplateTokenSink {
    /// Tokenizer handle emitted for non-tree-building html5ever integration.
    ///
    /// Inputs: html5ever token sink contract. Output: unit handle.
    /// Transformation: declares that this parser does not allocate persistent
    /// tokenizer node handles because `TemplateBuilder` owns the structured
    /// output directly.
    type Handle = ();

    /// Processes one html5ever token.
    ///
    /// Inputs: token and tokenizer line number. Output: continue signal.
    /// Transformation: forwards the token into the mutable template builder.
    fn process_token(&self, token: Token, line_number: u64) -> TokenSinkResult<Self::Handle> {
        self.builder.borrow_mut().process_token(token, line_number);
        TokenSinkResult::Continue
    }
}

/// Incremental builder for parsed template nodes.
///
/// Inputs: html5ever token stream. Output: root node list or diagnostics.
/// Transformation: maintains an element stack, text buffer, diagnostics, and
/// optional slot parsing until `finish`.
struct TemplateBuilder {
    path: PathBuf,
    root: Vec<HtmlNode>,
    stack: Vec<HtmlElement>,
    text_buffer: String,
    text_buffer_line: Option<u64>,
    diagnostics: Vec<HtmlDiagnostic>,
    parse_slots: bool,
}

impl TemplateBuilder {
    /// Creates an empty template builder.
    ///
    /// Inputs: source path and slot parsing flag. Output: initialized builder.
    /// Transformation: seeds empty root/stack/text/diagnostic state.
    fn new(path: PathBuf, parse_slots: bool) -> Self {
        Self {
            path,
            root: Vec::new(),
            stack: Vec::new(),
            text_buffer: String::new(),
            text_buffer_line: None,
            diagnostics: Vec::new(),
            parse_slots,
        }
    }

    /// Applies one tokenizer event to builder state.
    ///
    /// Inputs: token and line number. Output: no direct return value.
    /// Transformation: updates element stack, text buffer, nodes, or diagnostics
    /// according to token kind.
    fn process_token(&mut self, token: Token, line_number: u64) {
        match token {
            Token::CharacterTokens(text) => self.buffer_text(text.to_string(), line_number),
            Token::CommentToken(comment) => {
                self.flush_text_buffer(line_number);
                self.push_node(HtmlNode::Comment(comment.to_string()));
            }
            Token::DoctypeToken(doctype) => {
                self.flush_text_buffer(line_number);
                let name = doctype
                    .name
                    .map(|name| name.to_string())
                    .unwrap_or_default();
                self.push_node(HtmlNode::Doctype(name));
            }
            Token::TagToken(tag) if tag.kind == TagKind::StartTag => {
                self.flush_text_buffer(line_number);
                let mut attrs = Vec::new();
                for attr in tag.attrs {
                    let value = attr.value.to_string();
                    attrs.push(HtmlAttr {
                        name: attr.name.local.to_string(),
                        value: Some(self.parse_attr_value(&value, line_number)),
                    });
                }

                let element = HtmlElement {
                    name: tag.name.to_string(),
                    attrs,
                    children: Vec::new(),
                };

                if tag.self_closing || is_html_void_element(&element.name) {
                    self.push_node(HtmlNode::Element(element));
                } else {
                    self.stack.push(element);
                }
            }
            Token::TagToken(tag) if tag.kind == TagKind::EndTag => {
                self.flush_text_buffer(line_number);
                self.close_element(tag.name.to_string(), line_number);
            }
            Token::TagToken(_) => {}
            Token::NullCharacterToken => {
                self.diagnostics
                    .push(self.diagnostic(line_number, "null character in template"));
            }
            Token::ParseError(message) => {
                self.diagnostics
                    .push(self.diagnostic(line_number, format!("HTML parse error: {message}")));
            }
            Token::EOFToken => self.flush_text_buffer(line_number),
        }
    }

    /// Buffers raw text until a structural token requires flushing.
    ///
    /// Inputs: text fragment and line number. Output: none. Transformation:
    /// appends text and records the first line for later diagnostics.
    fn buffer_text(&mut self, text: String, line_number: u64) {
        if self.text_buffer.is_empty() {
            self.text_buffer_line = Some(line_number);
        }
        self.text_buffer.push_str(&text);
    }

    /// Flushes buffered text into template nodes.
    ///
    /// Inputs: fallback line number. Output: none. Transformation: drains the
    /// text buffer and pushes parsed text/slot nodes.
    fn flush_text_buffer(&mut self, fallback_line_number: u64) {
        if self.text_buffer.is_empty() {
            return;
        }

        let text = std::mem::take(&mut self.text_buffer);
        let line_number = self.text_buffer_line.take().unwrap_or(fallback_line_number);
        self.push_text_nodes(text, line_number);
    }

    /// Pushes text as either raw text or parsed interpolation nodes.
    ///
    /// Inputs: text and line number. Output: none. Transformation: bypasses
    /// slot parsing for raw-text contexts or disabled slot mode.
    fn push_text_nodes(&mut self, text: String, line_number: u64) {
        if !self.parse_slots || self.current_parent_is_raw_text() || !text.contains(['{', '}']) {
            self.push_node(HtmlNode::Text(text));
            return;
        }

        for node in self.parse_text_interpolation(&text, line_number) {
            self.push_node(node);
        }
    }

    /// Parses interpolation slots inside a text node.
    ///
    /// Inputs: text and line number. Output: text/slot nodes. Transformation:
    /// splits `${slot.path}` regions into `HtmlNode::Slot` and records
    /// malformed regions as diagnostics while preserving source text. Legacy
    /// `{slot.path}` regions are still accepted during the syntax migration.
    fn parse_text_interpolation(&mut self, text: &str, line_number: u64) -> Vec<HtmlNode> {
        let mut nodes = Vec::new();
        let mut cursor = 0;

        while let Some(delimiter) = find_next_slot_delimiter(text, cursor) {
            let open = delimiter.open;
            if open > cursor {
                nodes.push(HtmlNode::Text(text[cursor..open].to_owned()));
            }

            let slot_start = open + delimiter.prefix_len;
            let Some(close_offset) = text[slot_start..].find('}') else {
                self.diagnostics
                    .push(self.diagnostic(line_number, "unterminated template interpolation slot"));
                nodes.push(HtmlNode::Text(text[open..].to_owned()));
                return nodes;
            };

            let close = slot_start + close_offset;
            let slot_source = &text[slot_start..close];
            match parse_slot_path(slot_source, Some(span_for(line_number, open, close + 1))) {
                Ok(slot) => nodes.push(HtmlNode::Slot(slot)),
                Err(message) => {
                    self.diagnostics.push(self.diagnostic(line_number, message));
                    nodes.push(HtmlNode::Text(text[open..=close].to_owned()));
                }
            }
            cursor = close + 1;
        }

        if cursor < text.len() {
            if text[cursor..].contains('}') {
                self.diagnostics.push(
                    self.diagnostic(line_number, "unexpected `}` outside template interpolation"),
                );
            }
            nodes.push(HtmlNode::Text(text[cursor..].to_owned()));
        }

        nodes
    }

    /// Parses an HTML attribute value.
    ///
    /// Inputs: attribute value and line number. Output: static text or slot.
    /// Transformation: accepts only whole-value slot interpolation and records a
    /// diagnostic for mixed interpolation.
    fn parse_attr_value(&mut self, value: &str, line_number: u64) -> HtmlAttrValue {
        if !value.contains(['{', '}']) {
            return HtmlAttrValue::Text(value.to_owned());
        }

        if let Some(slot_source) = slot_source_from_whole_attribute_value(value) {
            return match parse_slot_path(slot_source, Some(span_for(line_number, 0, value.len()))) {
                Ok(slot) => HtmlAttrValue::Slot(slot),
                Err(message) => {
                    self.diagnostics.push(self.diagnostic(line_number, message));
                    HtmlAttrValue::Text(value.to_owned())
                }
            };
        }

        self.diagnostics.push(self.diagnostic(
            line_number,
            "attribute interpolation must be a single slot like `${name}`",
        ));
        HtmlAttrValue::Text(value.to_owned())
    }

    /// Returns whether the current element is a raw-text parent.
    ///
    /// Inputs: current element stack. Output: `true` for `script` or `style`.
    /// Transformation: checks the last open element name.
    fn current_parent_is_raw_text(&self) -> bool {
        matches!(
            self.stack.last().map(|element| element.name.as_str()),
            Some("script" | "style")
        )
    }

    /// Closes the current element.
    ///
    /// Inputs: closing tag name and line number. Output: none. Transformation:
    /// validates stack top, emits completed elements, and records mismatches.
    fn close_element(&mut self, name: String, line_number: u64) {
        let Some(element) = self.stack.pop() else {
            self.diagnostics
                .push(self.diagnostic(line_number, format!("unexpected closing tag `</{name}>`")));
            return;
        };

        if element.name != name {
            self.diagnostics.push(self.diagnostic(
                line_number,
                format!(
                    "mismatched closing tag `</{name}>`; expected `</{}>`",
                    element.name
                ),
            ));
            self.stack.push(element);
            return;
        }

        self.push_node(HtmlNode::Element(element));
    }

    /// Pushes a parsed node into the current parent or root.
    ///
    /// Inputs: node. Output: none. Transformation: appends to the current child
    /// list and coalesces adjacent text nodes.
    fn push_node(&mut self, node: HtmlNode) {
        let nodes = if let Some(parent) = self.stack.last_mut() {
            &mut parent.children
        } else {
            &mut self.root
        };

        if let (Some(HtmlNode::Text(existing)), HtmlNode::Text(incoming)) =
            (nodes.last_mut(), &node)
        {
            existing.push_str(incoming);
            return;
        }

        nodes.push(node);
    }

    /// Finalizes the builder into parsed nodes.
    ///
    /// Inputs: completed builder state. Output: root nodes or diagnostics.
    /// Transformation: flushes remaining text, reports unclosed elements, and
    /// returns accumulated diagnostics when present.
    fn finish(mut self) -> Result<Vec<HtmlNode>, Vec<HtmlDiagnostic>> {
        self.flush_text_buffer(0);

        while let Some(element) = self.stack.pop() {
            self.diagnostics.push(HtmlDiagnostic::new(
                Some(self.path.clone()),
                format!("unclosed tag `<{}>`", element.name),
            ));
        }

        if self.diagnostics.is_empty() {
            Ok(self.root)
        } else {
            Err(self.diagnostics)
        }
    }

    /// Creates a path-qualified diagnostic for this template.
    ///
    /// Inputs: line number and message. Output: `HtmlDiagnostic`.
    /// Transformation: prefixes the message with line information and attaches
    /// the builder path.
    fn diagnostic(&self, line_number: u64, message: impl Into<String>) -> HtmlDiagnostic {
        HtmlDiagnostic::new(
            Some(self.path.clone()),
            format!("line {line_number}: {}", message.into()),
        )
    }
}

/// Returns whether an HTML element is a void element.
///
/// Inputs:
/// - `name`: element name from the HTML tokenizer.
///
/// Output:
/// - `true` when the element must not require a closing tag.
///
/// Transformation:
/// - Compares the tokenizer-normalized element name against the HTML void
///   element set so generated output can validate ordinary tags such as
///   `<base>`, `<meta>`, and `<link>`.
fn is_html_void_element(name: &str) -> bool {
    matches!(
        name,
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "source"
            | "track"
            | "wbr"
    )
}

/// Source delimiter for one template interpolation slot.
///
/// Inputs:
/// - Derived from scanning HTML text.
///
/// Output:
/// - Opening byte offset and delimiter prefix length.
///
/// Transformation:
/// - Distinguishes canonical `${...}` interpolation from legacy `{...}`
///   interpolation without changing the downstream slot path representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SlotDelimiter {
    open: usize,
    prefix_len: usize,
}

/// Finds the next template interpolation delimiter in text.
///
/// Inputs:
/// - `text`: HTML text node contents.
/// - `cursor`: byte offset where scanning starts.
///
/// Output:
/// - The next `${` or legacy `{` delimiter, or `None`.
///
/// Transformation:
/// - Performs a byte scan so `${` is treated as one delimiter rather than a
///   literal `$` followed by a legacy brace interpolation.
fn find_next_slot_delimiter(text: &str, cursor: usize) -> Option<SlotDelimiter> {
    let bytes = text.as_bytes();
    let mut index = cursor;
    while index < bytes.len() {
        if bytes[index] == b'$' && bytes.get(index + 1) == Some(&b'{') {
            return Some(SlotDelimiter {
                open: index,
                prefix_len: 2,
            });
        }
        if bytes[index] == b'{' {
            return Some(SlotDelimiter {
                open: index,
                prefix_len: 1,
            });
        }
        index += 1;
    }
    None
}

/// Extracts a whole-attribute interpolation body.
///
/// Inputs:
/// - Raw HTML attribute value.
///
/// Output:
/// - Slot body for `${slot}` or legacy `{slot}` whole-value attributes.
///
/// Transformation:
/// - Accepts only values where the entire attribute is one interpolation slot.
fn slot_source_from_whole_attribute_value(value: &str) -> Option<&str> {
    value
        .strip_prefix("${")
        .and_then(|rest| rest.strip_suffix('}'))
        .or_else(|| {
            value
                .strip_prefix('{')
                .and_then(|rest| rest.strip_suffix('}'))
        })
}

/// Parses an interpolation slot expression.
///
/// Inputs: slot source and optional span. Output: `HtmlSlot` or message.
/// Transformation: preserves the expression text and records dotted path
/// metadata only when every path segment has the simple slot-path shape.
fn parse_slot_path(source: &str, span: Option<HtmlSpan>) -> Result<HtmlSlot, String> {
    let expression = source.trim();
    if expression.is_empty() {
        return Err("template interpolation slot cannot be empty".to_owned());
    }

    let mut path = Vec::new();
    let mut is_path = true;
    for segment in expression.split('.') {
        if !is_valid_slot_segment(segment) {
            is_path = false;
            break;
        }
        path.push(segment.to_owned());
    }
    if !is_path {
        path.clear();
    }

    Ok(HtmlSlot {
        expression: expression.to_owned(),
        path,
        span,
    })
}

/// Builds an interpolation span.
///
/// Inputs: line, start, and end offsets. Output: `HtmlSpan`. Transformation:
/// stores the offsets unchanged.
fn span_for(line: u64, start: usize, end: usize) -> HtmlSpan {
    HtmlSpan { line, start, end }
}

/// Validates one slot path segment.
///
/// Inputs: segment text. Output: validity flag. Transformation: requires an
/// alphabetic or underscore head and alphanumeric/underscore tail.
fn is_valid_slot_segment(segment: &str) -> bool {
    let mut chars = segment.chars();
    let Some(first) = chars.next() else {
        return false;
    };

    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }

    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}
