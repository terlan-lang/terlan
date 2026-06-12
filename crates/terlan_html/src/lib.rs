use std::path::{Path, PathBuf};

use comrak::{markdown_to_html, Options};
use cssparser::{Parser, ParserInput};
use html5ever::tendril::StrTendril;
use html5ever::tokenizer::{
    BufferQueue, TagKind, Token, TokenSink, TokenSinkResult, Tokenizer, TokenizerOpts,
};
use html5ever::TokenizerResult;

pub const TERLAN_HTML_TEMPLATE_SUFFIX: &str = ".tl.html";
pub const TERLAN_MARKDOWN_TEMPLATE_SUFFIX: &str = ".tl.md";
pub const TERLAN_TEMPLATE_SUFFIX: &str = TERLAN_HTML_TEMPLATE_SUFFIX;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HtmlTemplate {
    pub source_path: Option<PathBuf>,
    pub tag_name: Option<String>,
    pub nodes: Vec<HtmlNode>,
}

impl HtmlTemplate {
    pub fn new(nodes: Vec<HtmlNode>) -> Self {
        Self {
            source_path: None,
            tag_name: None,
            nodes,
        }
    }

    pub fn from_terlan_template_path(
        path: impl AsRef<Path>,
        nodes: Vec<HtmlNode>,
    ) -> Result<Self, HtmlDiagnostic> {
        let path = path.as_ref();
        Ok(Self {
            source_path: Some(path.to_path_buf()),
            tag_name: Some(template_tag_from_path(path)?),
            nodes,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HtmlNode {
    Text(String),
    Element(HtmlElement),
    Comment(String),
    Doctype(String),
    Slot(HtmlSlot),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HtmlElement {
    pub name: String,
    pub attrs: Vec<HtmlAttr>,
    pub children: Vec<HtmlNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HtmlAttr {
    pub name: String,
    pub value: Option<HtmlAttrValue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HtmlAttrValue {
    Text(String),
    Slot(HtmlSlot),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HtmlSlot {
    pub path: Vec<String>,
    pub span: Option<HtmlSpan>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HtmlSpan {
    pub line: u64,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownDocument {
    pub source_path: Option<PathBuf>,
    pub raw_source: String,
    pub rendered_html: String,
    pub nodes: Vec<HtmlNode>,
}

impl HtmlSlot {
    pub fn dotted(path: impl AsRef<str>) -> Self {
        Self {
            path: path
                .as_ref()
                .split('.')
                .filter(|part| !part.is_empty())
                .map(str::to_owned)
                .collect(),
            span: None,
        }
    }

    pub fn with_span(mut self, span: HtmlSpan) -> Self {
        self.span = Some(span);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HtmlDiagnostic {
    pub path: Option<PathBuf>,
    pub message: String,
}

impl HtmlDiagnostic {
    pub fn new(path: Option<PathBuf>, message: impl Into<String>) -> Self {
        Self {
            path,
            message: message.into(),
        }
    }
}

pub fn is_terlan_template_path(path: impl AsRef<Path>) -> bool {
    path.as_ref()
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| template_suffix(name).is_some())
}

pub fn template_tag_from_path(path: impl AsRef<Path>) -> Result<String, HtmlDiagnostic> {
    let path = path.as_ref();
    let file_name = path
        .file_name()
        .ok_or_else(|| HtmlDiagnostic::new(Some(path.to_path_buf()), "missing template filename"))?
        .to_str()
        .ok_or_else(|| {
            HtmlDiagnostic::new(
                Some(path.to_path_buf()),
                "template filename must be valid UTF-8",
            )
        })?;

    let suffix = template_suffix(file_name).ok_or_else(|| {
        HtmlDiagnostic::new(
            Some(path.to_path_buf()),
            format!(
                "template filename must end with `{TERLAN_HTML_TEMPLATE_SUFFIX}` or `{TERLAN_MARKDOWN_TEMPLATE_SUFFIX}`"
            ),
        )
    })?;
    let stem = file_name.strip_suffix(suffix).expect("known suffix");

    normalize_template_tag(path, stem)
}

fn template_suffix(file_name: &str) -> Option<&'static str> {
    if file_name.ends_with(TERLAN_HTML_TEMPLATE_SUFFIX) {
        Some(TERLAN_HTML_TEMPLATE_SUFFIX)
    } else if file_name.ends_with(TERLAN_MARKDOWN_TEMPLATE_SUFFIX) {
        Some(TERLAN_MARKDOWN_TEMPLATE_SUFFIX)
    } else {
        None
    }
}

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

pub fn parse_html_template(
    source: impl AsRef<str>,
    path: impl AsRef<Path>,
) -> Result<HtmlTemplate, Vec<HtmlDiagnostic>> {
    let path = path.as_ref();
    let tag_name = match template_tag_from_path(path) {
        Ok(tag_name) => tag_name,
        Err(diagnostic) => return Err(vec![diagnostic]),
    };

    match parse_html_nodes(source.as_ref(), path) {
        Ok(nodes) => Ok(HtmlTemplate {
            source_path: Some(path.to_path_buf()),
            tag_name: Some(tag_name),
            nodes,
        }),
        Err(diagnostics) => Err(diagnostics),
    }
}

pub fn parse_markdown_template(
    source: impl AsRef<str>,
    path: impl AsRef<Path>,
) -> Result<HtmlTemplate, Vec<HtmlDiagnostic>> {
    let path = path.as_ref();
    let tag_name = match template_tag_from_path(path) {
        Ok(tag_name) => tag_name,
        Err(diagnostic) => return Err(vec![diagnostic]),
    };
    let rendered_html = markdown_to_html(source.as_ref(), &Options::default());

    match parse_html_nodes(&rendered_html, path) {
        Ok(nodes) => Ok(HtmlTemplate {
            source_path: Some(path.to_path_buf()),
            tag_name: Some(tag_name),
            nodes,
        }),
        Err(diagnostics) => Err(diagnostics),
    }
}

pub fn parse_markdown(
    source: impl AsRef<str>,
    path: impl AsRef<Path>,
) -> Result<MarkdownDocument, Vec<HtmlDiagnostic>> {
    let path = path.as_ref();
    let raw_source = source.as_ref().to_owned();
    let rendered_html = markdown_to_html(&raw_source, &Options::default());
    let nodes = parse_html_nodes(&rendered_html, path)?;

    Ok(MarkdownDocument {
        source_path: Some(path.to_path_buf()),
        raw_source,
        rendered_html,
        nodes,
    })
}

pub fn validate_html_output(
    source: impl AsRef<str>,
    path: impl AsRef<Path>,
) -> Result<(), Vec<HtmlDiagnostic>> {
    parse_html_nodes_without_slots(source, path).map(|_| ())
}

fn parse_html_nodes(
    source: impl AsRef<str>,
    path: impl AsRef<Path>,
) -> Result<Vec<HtmlNode>, Vec<HtmlDiagnostic>> {
    parse_html_nodes_with_slot_parsing(source, path, true)
}

fn parse_html_nodes_without_slots(
    source: impl AsRef<str>,
    path: impl AsRef<Path>,
) -> Result<Vec<HtmlNode>, Vec<HtmlDiagnostic>> {
    parse_html_nodes_with_slot_parsing(source, path, false)
}

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

struct TemplateTokenSink {
    builder: std::cell::RefCell<TemplateBuilder>,
}

impl TemplateTokenSink {
    fn new(path: PathBuf, parse_slots: bool) -> Self {
        Self {
            builder: std::cell::RefCell::new(TemplateBuilder::new(path, parse_slots)),
        }
    }

    fn into_builder(self) -> TemplateBuilder {
        self.builder.into_inner()
    }
}

impl TokenSink for TemplateTokenSink {
    type Handle = ();

    fn process_token(&self, token: Token, line_number: u64) -> TokenSinkResult<Self::Handle> {
        self.builder.borrow_mut().process_token(token, line_number);
        TokenSinkResult::Continue
    }
}

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

                if tag.self_closing {
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

    fn buffer_text(&mut self, text: String, line_number: u64) {
        if self.text_buffer.is_empty() {
            self.text_buffer_line = Some(line_number);
        }
        self.text_buffer.push_str(&text);
    }

    fn flush_text_buffer(&mut self, fallback_line_number: u64) {
        if self.text_buffer.is_empty() {
            return;
        }

        let text = std::mem::take(&mut self.text_buffer);
        let line_number = self.text_buffer_line.take().unwrap_or(fallback_line_number);
        self.push_text_nodes(text, line_number);
    }

    fn push_text_nodes(&mut self, text: String, line_number: u64) {
        if !self.parse_slots || self.current_parent_is_raw_text() || !text.contains(['{', '}']) {
            self.push_node(HtmlNode::Text(text));
            return;
        }

        for node in self.parse_text_interpolation(&text, line_number) {
            self.push_node(node);
        }
    }

    fn parse_text_interpolation(&mut self, text: &str, line_number: u64) -> Vec<HtmlNode> {
        let mut nodes = Vec::new();
        let mut cursor = 0;

        while let Some(open_offset) = text[cursor..].find('{') {
            let open = cursor + open_offset;
            if open > cursor {
                nodes.push(HtmlNode::Text(text[cursor..open].to_owned()));
            }

            let slot_start = open + 1;
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

    fn parse_attr_value(&mut self, value: &str, line_number: u64) -> HtmlAttrValue {
        if !value.contains(['{', '}']) {
            return HtmlAttrValue::Text(value.to_owned());
        }

        if let Some(slot_source) = value
            .strip_prefix('{')
            .and_then(|rest| rest.strip_suffix('}'))
        {
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
            "attribute interpolation must be a single slot like `{name}`",
        ));
        HtmlAttrValue::Text(value.to_owned())
    }

    fn current_parent_is_raw_text(&self) -> bool {
        matches!(
            self.stack.last().map(|element| element.name.as_str()),
            Some("script" | "style")
        )
    }

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

    fn diagnostic(&self, line_number: u64, message: impl Into<String>) -> HtmlDiagnostic {
        HtmlDiagnostic::new(
            Some(self.path.clone()),
            format!("line {line_number}: {}", message.into()),
        )
    }
}

fn parse_slot_path(source: &str, span: Option<HtmlSpan>) -> Result<HtmlSlot, String> {
    if source.is_empty() {
        return Err("template interpolation slot cannot be empty".to_owned());
    }

    let mut path = Vec::new();
    for segment in source.split('.') {
        if !is_valid_slot_segment(segment) {
            return Err(format!("invalid template interpolation slot `{source}`"));
        }
        path.push(segment.to_owned());
    }

    Ok(HtmlSlot { path, span })
}

fn span_for(line: u64, start: usize, end: usize) -> HtmlSpan {
    HtmlSpan { line, start, end }
}

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

fn normalize_template_tag(path: &Path, stem: &str) -> Result<String, HtmlDiagnostic> {
    if stem.is_empty() {
        return Err(HtmlDiagnostic::new(
            Some(path.to_path_buf()),
            "template filename stem cannot be empty",
        ));
    }

    let mut tag = String::with_capacity(stem.len());
    let mut previous_was_dash = false;

    for ch in stem.chars() {
        match ch {
            'a'..='z' | '0'..='9' => {
                tag.push(ch);
                previous_was_dash = false;
            }
            'A'..='Z' => {
                tag.push(ch.to_ascii_lowercase());
                previous_was_dash = false;
            }
            '_' | '-' => {
                if tag.is_empty() || previous_was_dash {
                    return Err(HtmlDiagnostic::new(
                        Some(path.to_path_buf()),
                        "template tag name cannot start with or contain repeated separators",
                    ));
                }
                tag.push('-');
                previous_was_dash = true;
            }
            _ => {
                return Err(HtmlDiagnostic::new(
                    Some(path.to_path_buf()),
                    format!("invalid template filename character `{ch}`"),
                ));
            }
        }
    }

    if tag.ends_with('-') {
        return Err(HtmlDiagnostic::new(
            Some(path.to_path_buf()),
            "template tag name cannot end with a separator",
        ));
    }

    Ok(tag)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_terlan_template_paths() {
        assert!(is_terlan_template_path("templates/user_card.tl.html"));
        assert!(is_terlan_template_path("templates/user_card.tl.md"));
        assert!(!is_terlan_template_path("templates/user_card.html"));
        assert!(!is_terlan_template_path("templates/user_card.md"));
    }

    #[test]
    fn derives_template_tag_from_underscore_filename() {
        let tag = template_tag_from_path("templates/user_card.tl.html").unwrap();
        assert_eq!(tag, "user-card");
    }

    #[test]
    fn derives_template_tag_from_markdown_template_filename() {
        let tag = template_tag_from_path("templates/welcome_content.tl.md").unwrap();
        assert_eq!(tag, "welcome-content");
    }

    #[test]
    fn derives_template_tag_from_kebab_filename() {
        let tag = template_tag_from_path("templates/main-layout.tl.html").unwrap();
        assert_eq!(tag, "main-layout");
    }

    #[test]
    fn rejects_plain_html_as_template_path() {
        let diagnostic = template_tag_from_path("templates/user_card.html").unwrap_err();
        assert!(diagnostic
            .message
            .contains("template filename must end with `.tl.html` or `.tl.md`"));
    }

    #[test]
    fn rejects_invalid_template_filename_characters() {
        let diagnostic = template_tag_from_path("templates/user.card.tl.html").unwrap_err();
        assert!(diagnostic
            .message
            .contains("invalid template filename character"));
    }

    #[test]
    fn builds_template_with_registered_tag() {
        let template =
            HtmlTemplate::from_terlan_template_path("templates/user_card.tl.html", vec![]).unwrap();

        assert_eq!(template.tag_name.as_deref(), Some("user-card"));
    }

    #[test]
    fn parses_static_template_text_and_elements() {
        let template = parse_html_template(
            "<article class=\"card\"><h1>Hello</h1><p>World</p></article>",
            "templates/user_card.tl.html",
        )
        .unwrap();

        assert_eq!(template.tag_name.as_deref(), Some("user-card"));
        assert_eq!(
            template.nodes,
            vec![HtmlNode::Element(HtmlElement {
                name: "article".to_owned(),
                attrs: vec![HtmlAttr {
                    name: "class".to_owned(),
                    value: Some(HtmlAttrValue::Text("card".to_owned())),
                }],
                children: vec![
                    HtmlNode::Element(HtmlElement {
                        name: "h1".to_owned(),
                        attrs: vec![],
                        children: vec![HtmlNode::Text("Hello".to_owned())],
                    }),
                    HtmlNode::Element(HtmlElement {
                        name: "p".to_owned(),
                        attrs: vec![],
                        children: vec![HtmlNode::Text("World".to_owned())],
                    }),
                ],
            })]
        );
    }

    #[test]
    fn parses_template_comments_and_doctype() {
        let template = parse_html_template(
            "<!doctype html><!-- note --><main></main>",
            "templates/page_shell.tl.html",
        )
        .unwrap();

        assert_eq!(
            template.nodes,
            vec![
                HtmlNode::Doctype("html".to_owned()),
                HtmlNode::Comment(" note ".to_owned()),
                HtmlNode::Element(HtmlElement {
                    name: "main".to_owned(),
                    attrs: vec![],
                    children: vec![],
                }),
            ]
        );
    }

    #[test]
    fn parses_markdown_templates_as_named_html_templates() {
        let template = parse_template(
            "# Hello {name}\n\nThis came from **Markdown**.\n",
            "templates/welcome_content.tl.md",
        )
        .unwrap();

        assert_eq!(template.tag_name.as_deref(), Some("welcome-content"));
        assert_eq!(
            template.nodes,
            vec![
                HtmlNode::Element(HtmlElement {
                    name: "h1".to_owned(),
                    attrs: vec![],
                    children: vec![
                        HtmlNode::Text("Hello ".to_owned()),
                        HtmlNode::Slot(HtmlSlot {
                            path: vec!["name".to_owned()],
                            span: Some(HtmlSpan {
                                line: 1,
                                start: 6,
                                end: 12,
                            }),
                        }),
                    ],
                }),
                HtmlNode::Text("\n".to_owned()),
                HtmlNode::Element(HtmlElement {
                    name: "p".to_owned(),
                    attrs: vec![],
                    children: vec![
                        HtmlNode::Text("This came from ".to_owned()),
                        HtmlNode::Element(HtmlElement {
                            name: "strong".to_owned(),
                            attrs: vec![],
                            children: vec![HtmlNode::Text("Markdown".to_owned())],
                        }),
                        HtmlNode::Text(".".to_owned()),
                    ],
                }),
                HtmlNode::Text("\n".to_owned()),
            ]
        );
    }

    #[test]
    fn reports_template_parse_errors_with_path() {
        let diagnostics = parse_html_template(
            "<article><h1>Broken</article>",
            "templates/bad_card.tl.html",
        )
        .unwrap_err();

        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("mismatched closing tag")));
        assert!(diagnostics
            .iter()
            .all(|diagnostic| diagnostic.path.as_deref()
                == Some(Path::new("templates/bad_card.tl.html"))));
    }

    #[test]
    fn parses_text_interpolation_slots() {
        let template = parse_html_template(
            "<p>Hello {user.name}</p>",
            "templates/user_greeting.tl.html",
        )
        .unwrap();

        assert_eq!(
            template.nodes,
            vec![HtmlNode::Element(HtmlElement {
                name: "p".to_owned(),
                attrs: vec![],
                children: vec![
                    HtmlNode::Text("Hello ".to_owned()),
                    HtmlNode::Slot(HtmlSlot {
                        path: vec!["user".to_owned(), "name".to_owned()],
                        span: Some(HtmlSpan {
                            line: 1,
                            start: 6,
                            end: 17,
                        }),
                    }),
                ],
            })]
        );
    }

    #[test]
    fn parses_attribute_interpolation_slots() {
        let template =
            parse_html_template("<a href=\"{url}\">Link</a>", "templates/link_card.tl.html")
                .unwrap();

        assert_eq!(
            template.nodes,
            vec![HtmlNode::Element(HtmlElement {
                name: "a".to_owned(),
                attrs: vec![HtmlAttr {
                    name: "href".to_owned(),
                    value: Some(HtmlAttrValue::Slot(HtmlSlot {
                        path: vec!["url".to_owned()],
                        span: Some(HtmlSpan {
                            line: 1,
                            start: 0,
                            end: 5,
                        }),
                    })),
                }],
                children: vec![HtmlNode::Text("Link".to_owned())],
            })]
        );
    }

    #[test]
    fn rejects_invalid_interpolation_syntax() {
        let diagnostics =
            parse_html_template("<p>Hello {}</p>", "templates/bad_slot.tl.html").unwrap_err();

        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("template interpolation slot cannot be empty")));
    }

    #[test]
    fn does_not_parse_interpolation_inside_script_or_style_text() {
        let template = parse_html_template(
            "<script>let value = {raw};</script><style>.x { color: red; }</style>",
            "templates/raw_text.tl.html",
        )
        .unwrap();

        assert_eq!(
            template.nodes,
            vec![
                HtmlNode::Element(HtmlElement {
                    name: "script".to_owned(),
                    attrs: vec![],
                    children: vec![HtmlNode::Text("let value = {raw};".to_owned())],
                }),
                HtmlNode::Element(HtmlElement {
                    name: "style".to_owned(),
                    attrs: vec![],
                    children: vec![HtmlNode::Text(".x { color: red; }".to_owned())],
                }),
            ]
        );
    }

    #[test]
    fn validates_css_sources() {
        validate_css(
            "body { color: red; }\n.card { display: block; }",
            "styles/page.css",
        )
        .expect("valid css");
    }

    #[test]
    fn reports_css_parse_errors() {
        let diagnostics = validate_css("body { color: '\n'; }", "styles/bad.css").unwrap_err();

        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("CSS parse error")));
        assert!(diagnostics
            .iter()
            .all(|diagnostic| diagnostic.path.as_deref() == Some(Path::new("styles/bad.css"))));
    }

    #[test]
    fn validates_html_output_without_template_slots() {
        validate_html_output("<main>{literal}</main>", "public/page.html").expect("valid html");
    }

    #[test]
    fn reports_html_output_validation_errors() {
        let diagnostics = validate_html_output("<main></section>", "public/bad.html").unwrap_err();

        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("mismatched closing tag")));
        assert!(diagnostics
            .iter()
            .all(|diagnostic| diagnostic.path.as_deref() == Some(Path::new("public/bad.html"))));
    }

    #[test]
    fn renders_markdown_to_valid_html_nodes() {
        let document = parse_markdown("# Hello\n\n- one\n- two\n", "posts/hello.md").unwrap();

        assert_eq!(document.raw_source, "# Hello\n\n- one\n- two\n");
        assert_eq!(
            document.rendered_html,
            "<h1>Hello</h1>\n<ul>\n<li>one</li>\n<li>two</li>\n</ul>\n"
        );
        assert_eq!(
            document.nodes,
            vec![
                HtmlNode::Element(HtmlElement {
                    name: "h1".to_owned(),
                    attrs: vec![],
                    children: vec![HtmlNode::Text("Hello".to_owned())],
                }),
                HtmlNode::Text("\n".to_owned()),
                HtmlNode::Element(HtmlElement {
                    name: "ul".to_owned(),
                    attrs: vec![],
                    children: vec![
                        HtmlNode::Text("\n".to_owned()),
                        HtmlNode::Element(HtmlElement {
                            name: "li".to_owned(),
                            attrs: vec![],
                            children: vec![HtmlNode::Text("one".to_owned())],
                        }),
                        HtmlNode::Text("\n".to_owned()),
                        HtmlNode::Element(HtmlElement {
                            name: "li".to_owned(),
                            attrs: vec![],
                            children: vec![HtmlNode::Text("two".to_owned())],
                        }),
                        HtmlNode::Text("\n".to_owned()),
                    ],
                }),
                HtmlNode::Text("\n".to_owned()),
            ]
        );
    }

    #[test]
    fn validates_markdown_rendered_html_with_path() {
        let document = parse_markdown("[safe](javascript:alert(1))", "posts/safe.md").unwrap();

        assert_eq!(
            document.source_path.as_deref(),
            Some(Path::new("posts/safe.md"))
        );
        assert!(!document.rendered_html.contains("javascript:alert"));
        assert!(document
            .nodes
            .iter()
            .any(|node| matches!(node, HtmlNode::Element(element) if element.name == "p")));
    }

    #[test]
    fn validates_markdown_derived_html_output() {
        let document = parse_markdown(
            "# Links\n\n[good](https://example.com)\n\n[bad](javascript:alert(1))\n",
            "posts/links.md",
        )
        .unwrap();

        assert!(document.rendered_html.contains("<h1>Links</h1>"));
        assert!(document.rendered_html.contains("https://example.com"));
        assert!(!document.rendered_html.contains("javascript:alert"));
        assert!(document.nodes.iter().any(|node| {
            matches!(
                node,
                HtmlNode::Element(HtmlElement { name, .. }) if name == "h1"
            )
        }));
        assert!(document.nodes.iter().any(|node| {
            matches!(
                node,
                HtmlNode::Element(HtmlElement { name, .. }) if name == "p"
            )
        }));
    }
}
