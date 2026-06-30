use super::*;

/// Stateful parser for raw HTML block syntax.
///
/// Inputs: raw character stream from an `html { ... }` macro block.
/// Output: parsed HTML nodes consumed by the Terlan expression parser.
/// Transformation: tracks character position while converting tags, attrs, named slots, text, and interpolations into parse tree nodes.
#[derive(Debug)]
struct HtmlBlockParser {
    chars: Vec<char>,
    pos: usize,
}

impl HtmlBlockParser {
    /// Parses a raw HTML block.
    ///
    /// Inputs: `raw` contains the source inside an `html { ... }` block.
    /// Output: parsed HTML nodes or a diagnostic when trailing source remains.
    /// Transformation: initializes character state, parses child nodes, and
    /// verifies the full input was consumed.
    fn parse(raw: &str) -> ParseResult<Vec<HtmlNode>> {
        let mut parser = Self {
            chars: raw.chars().collect(),
            pos: 0,
        };
        let nodes = parser.parse_nodes(None, false)?;
        parser.skip_ws();
        if parser.eof() {
            Ok(nodes)
        } else {
            Err(ParseError {
                message: "invalid html source".to_string(),
                span: Span::new(0, raw.len()),
            })
        }
    }

    /// Parses HTML child nodes until an optional stop condition.
    ///
    /// Inputs: optional closing tag and whether a named-slot `}` may stop the
    /// node list.
    /// Output: ordered HTML child nodes.
    /// Transformation: routes tags, named slots, text, and interpolations into
    /// their parse tree node variants.
    fn parse_nodes(
        &mut self,
        stop_tag: Option<&str>,
        stop_slot_block: bool,
    ) -> ParseResult<Vec<HtmlNode>> {
        let mut nodes = Vec::new();
        while !self.eof() {
            self.skip_ws();
            if self.eof() {
                break;
            }

            if stop_slot_block && self.consume_if("}") {
                return Ok(nodes);
            }

            if self.consume_if("</") {
                let name = self.parse_identifier()?;
                self.skip_ws();
                self.expect_char('>')?;

                if let Some(expected) = stop_tag {
                    if name == expected {
                        return Ok(nodes);
                    }
                }
                return Err(ParseError {
                    message: format!("unexpected closing tag </{}>", name),
                    span: Span::new(self.pos, self.pos),
                });
            }

            if self.check_named_slot_start() {
                nodes.push(self.parse_named_slot()?);
                continue;
            }

            if self.consume_if("<") {
                nodes.push(self.parse_html_element()?);
                continue;
            }

            nodes.extend(self.parse_text_nodes(stop_slot_block)?);
        }

        if let Some(name) = stop_tag {
            Err(ParseError {
                message: format!("missing closing tag </{}>", name),
                span: Span::new(self.pos, self.pos),
            })
        } else if stop_slot_block {
            Err(ParseError {
                message: "missing closing brace for named slot".to_string(),
                span: Span::new(self.pos, self.pos),
            })
        } else {
            Ok(nodes)
        }
    }

    /// Parses one HTML element after `<` has been consumed.
    ///
    /// Inputs: cursor at the element name.
    /// Output: `HtmlNode::Element` with attrs and children.
    /// Transformation: consumes a normal or self-closing element and validates
    /// its matching close tag when children are present.
    fn parse_html_element(&mut self) -> ParseResult<HtmlNode> {
        let name = self.parse_identifier()?;
        let attrs = self.parse_attrs()?;

        if self.consume_if("/") {
            self.expect_char('>')?;
            return Ok(HtmlNode::Element(HtmlElement {
                name,
                attrs,
                children: Vec::new(),
            }));
        }

        self.expect_char('>')?;
        let children = self.parse_nodes(Some(&name), false)?;
        Ok(HtmlNode::Element(HtmlElement {
            name,
            attrs,
            children,
        }))
    }

    /// Parses one named slot block.
    ///
    /// Inputs: cursor at `@`.
    /// Output: `HtmlNode::NamedSlot` with parsed children.
    /// Transformation: consumes `@name { ... }` and parses the nested node list.
    fn parse_named_slot(&mut self) -> ParseResult<HtmlNode> {
        self.expect_char('@')?;
        let name = self.parse_identifier()?;
        self.skip_ws();
        self.expect_char('{')?;
        let children = self.parse_nodes(None, true)?;
        Ok(HtmlNode::NamedSlot(HtmlNamedSlot { name, children }))
    }

    /// Parses an element attribute list.
    ///
    /// Inputs: cursor after an element name.
    /// Output: ordered attributes.
    /// Transformation: consumes zero or more attributes until `>`, `/`, or EOF.
    fn parse_attrs(&mut self) -> ParseResult<Vec<HtmlAttr>> {
        let mut attrs = Vec::new();
        loop {
            self.skip_ws();
            if self.eof() || self.check_char('>') || self.check_char('/') {
                break;
            }

            let name = self.parse_identifier()?;
            let value = if self.consume_if("=") {
                self.skip_ws();
                Some(self.parse_attribute_value()?)
            } else {
                None
            };
            attrs.push(HtmlAttr { name, value });
        }
        Ok(attrs)
    }

    /// Parses one HTML attribute value.
    ///
    /// Inputs: cursor at a quoted, braced, or bare attribute value.
    /// Output: text or expression attribute value.
    /// Transformation: converts `{ ... }` values through Terlan expression
    /// parsing and leaves quoted/bare values as text.
    fn parse_attribute_value(&mut self) -> ParseResult<HtmlAttrValue> {
        if self.consume_if("\"") {
            Ok(HtmlAttrValue::Text(self.consume_until('"')))
        } else if self.consume_if("'") {
            Ok(HtmlAttrValue::Text(self.consume_until('\'')))
        } else if self.consume_if("{") {
            let expr_text = self.parse_braced_expression()?;
            Ok(HtmlAttrValue::Expr(parse_terlan_expr(&expr_text)?))
        } else {
            let start = self.pos;
            while !self.eof() && !self.current().is_whitespace() && !self.check_char('>') {
                self.pos += 1;
            }

            if self.pos == start {
                Err(ParseError {
                    message: "expected attribute value".to_string(),
                    span: Span::new(self.pos, self.pos),
                })
            } else {
                Ok(HtmlAttrValue::Text(self.slice(start, self.pos)))
            }
        }
    }

    /// Parses text and interpolation nodes.
    ///
    /// Inputs: whether a named-slot closing brace should stop text parsing.
    /// Output: ordered text and expression nodes.
    /// Transformation: trims text chunks and parses `{ ... }` interpolations as
    /// Terlan expressions.
    fn parse_text_nodes(&mut self, stop_slot_block: bool) -> ParseResult<Vec<HtmlNode>> {
        let mut nodes = Vec::new();
        let mut text = String::new();

        while !self.eof()
            && !self.check_char('<')
            && !self.check_named_slot_start()
            && !(stop_slot_block && self.check_char('}'))
        {
            if self.consume_if("{") {
                let trimmed = text.trim().to_string();
                if !trimmed.is_empty() {
                    nodes.push(HtmlNode::Text(trimmed));
                }
                text.clear();

                let expr = self.parse_braced_expression()?;
                nodes.push(HtmlNode::Expr(parse_terlan_expr(&expr)?));
                continue;
            }

            text.push(self.current());
            self.pos += 1;
        }

        let text = text.trim().to_string();
        if !text.is_empty() {
            nodes.push(HtmlNode::Text(text));
        }

        Ok(nodes)
    }

    /// Parses source text inside a braced interpolation.
    ///
    /// Inputs: cursor just after the opening `{`.
    /// Output: trimmed expression source inside the matching brace.
    /// Transformation: tracks nested braces and quotes so embedded source can be
    /// passed to the Terlan expression parser unchanged.
    fn parse_braced_expression(&mut self) -> ParseResult<String> {
        let start = self.pos;
        let mut depth = 1usize;
        let mut quote = None;
        while !self.eof() {
            let ch = self.current();
            if let Some(current_quote) = quote {
                if ch == '\\' && current_quote == '"' && self.pos + 1 < self.chars.len() {
                    self.pos += 2;
                    continue;
                }

                self.pos += 1;
                if ch == current_quote {
                    quote = None;
                }
                continue;
            }

            if ch == '"' || ch == '\'' {
                quote = Some(ch);
                self.pos += 1;
                continue;
            }

            if ch == '{' {
                depth += 1;
            } else if ch == '}' {
                depth -= 1;
                if depth == 0 {
                    let expression = self.slice(start, self.pos).trim().to_string();
                    self.pos += 1;
                    return Ok(expression);
                }
            }

            self.pos += 1;
        }

        Err(ParseError {
            message: "unterminated interpolated expression".to_string(),
            span: Span::new(start, start),
        })
    }

    /// Parses an HTML identifier.
    ///
    /// Inputs: cursor at the first identifier character.
    /// Output: identifier text.
    /// Transformation: accepts alphanumeric, `_`, `-`, and `:` characters.
    fn parse_identifier(&mut self) -> ParseResult<String> {
        let start = self.pos;
        while !self.eof() {
            let c = self.current();
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == ':' {
                self.pos += 1;
            } else {
                break;
            }
        }
        if start == self.pos {
            Err(ParseError {
                message: "expected html identifier".to_string(),
                span: Span::new(self.pos, self.pos),
            })
        } else {
            Ok(self.slice(start, self.pos))
        }
    }

    /// Reports whether the current cursor starts a named slot.
    ///
    /// Inputs: character parser cursor inside an HTML block.
    /// Output: `true` when the next source shape is `@name {`.
    /// Transformation: performs non-consuming lookahead over a slot name and whitespace.
    fn check_named_slot_start(&self) -> bool {
        if self.eof() || self.current() != '@' {
            return false;
        }

        let mut pos = self.pos + 1;
        if pos >= self.chars.len()
            || !(self.chars[pos].is_ascii_alphabetic() || self.chars[pos] == '_')
        {
            return false;
        }

        while pos < self.chars.len() {
            let ch = self.chars[pos];
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || ch == ':' {
                pos += 1;
            } else {
                break;
            }
        }

        while pos < self.chars.len() && self.chars[pos].is_whitespace() {
            pos += 1;
        }

        pos < self.chars.len() && self.chars[pos] == '{'
    }

    /// Consumes one required character.
    ///
    /// Inputs: `ch` is the required character at the current cursor.
    /// Output: `Ok(())` after consuming the character, or a diagnostic.
    /// Transformation: advances exactly one position when the character matches.
    fn expect_char(&mut self, ch: char) -> ParseResult<()> {
        if self.check_char(ch) {
            self.pos += 1;
            Ok(())
        } else {
            Err(ParseError {
                message: format!("expected '{}'", ch),
                span: Span::new(self.pos, self.pos),
            })
        }
    }

    /// Consumes text until a delimiter character.
    ///
    /// Inputs: `end` is the terminating character to consume when present.
    /// Output: source substring before `end`.
    /// Transformation: advances through characters until the delimiter and consumes it if found.
    fn consume_until(&mut self, end: char) -> String {
        let start = self.pos;
        while !self.eof() && self.current() != end {
            self.pos += 1;
        }
        let value = self.slice(start, self.pos);
        let _ = self.consume_if_char(end);
        value
    }

    /// Optionally consumes one character.
    ///
    /// Inputs: `expected` is the character to consume when present.
    /// Output: `true` when the character was consumed.
    /// Transformation: advances the cursor only on an exact character match.
    fn consume_if_char(&mut self, expected: char) -> bool {
        if self.check_char(expected) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    /// Skips whitespace in the HTML character stream.
    ///
    /// Inputs: current character cursor.
    /// Output: cursor positioned at the next non-whitespace character or EOF.
    /// Transformation: advances through whitespace without producing nodes.
    fn skip_ws(&mut self) {
        while !self.eof() && self.current().is_whitespace() {
            self.pos += 1;
        }
    }

    /// Reports whether the current character matches.
    ///
    /// Inputs: `expected` is compared with the current character.
    /// Output: `true` when not at EOF and the current character matches.
    /// Transformation: performs non-consuming character comparison.
    fn check_char(&self, expected: char) -> bool {
        !self.eof() && self.current() == expected
    }

    /// Reports whether the current cursor starts a string.
    ///
    /// Inputs: `expected` is compared against the character stream.
    /// Output: `true` when all expected characters match at the cursor.
    /// Transformation: performs non-consuming multi-character lookahead.
    fn check_str(&self, expected: &str) -> bool {
        let chars: Vec<char> = expected.chars().collect();
        if self.pos + chars.len() > self.chars.len() {
            return false;
        }
        (0..chars.len()).all(|i| self.chars[self.pos + i] == chars[i])
    }

    /// Optionally consumes a string.
    ///
    /// Inputs: `expected` is the string to consume when present.
    /// Output: `true` when the string was consumed.
    /// Transformation: advances the cursor by the expected character count on exact match.
    fn consume_if(&mut self, expected: &str) -> bool {
        if self.check_str(expected) {
            self.pos += expected.chars().count();
            return true;
        }
        false
    }

    /// Returns the current character.
    ///
    /// Inputs: cursor that must not be at EOF.
    /// Output: character at the cursor.
    /// Transformation: reads without advancing the cursor.
    fn current(&self) -> char {
        self.chars[self.pos]
    }

    /// Reports whether the cursor reached EOF.
    ///
    /// Inputs: current character cursor.
    /// Output: `true` when the cursor is past the character stream.
    /// Transformation: compares the cursor position with the character vector length.
    fn eof(&self) -> bool {
        self.pos >= self.chars.len()
    }

    /// Copies a character slice into a string.
    ///
    /// Inputs: `start` and `end` select a character range.
    /// Output: owned string containing that range.
    /// Transformation: collects stored characters into owned text.
    fn slice(&self, start: usize, end: usize) -> String {
        self.chars[start..end].iter().collect()
    }
}

/// Parses raw HTML nodes for builtin HTML macro lowering.
///
/// Inputs: raw HTML block text.
/// Output: parsed HTML nodes, or one fallback text node if parsing fails.
/// Transformation: treats malformed HTML as literal text to preserve previous
/// parser behavior for raw blocks.
pub(super) fn parse_html_nodes(raw: &str) -> Vec<HtmlNode> {
    HtmlBlockParser::parse(raw).unwrap_or_else(|_| vec![HtmlNode::Text(raw.to_string())])
}

/// Parses a Terlan expression embedded in HTML.
///
/// Inputs: raw expression or HTML-looking shorthand text.
/// Output: parsed expression tree.
/// Transformation: normalizes HTML shorthand forms, lexes the resulting source,
/// and requires full-token consumption.
pub(crate) fn parse_terlan_expr(raw: &str) -> ParseResult<Expr> {
    ensure_syntax_contract_valid().map_err(syntax_contract_parse_error)?;

    let raw = normalize_interpolated_html_expr(raw.to_string());
    let tokens = match lex(raw.as_str()) {
        Ok(tokens) => tokens,
        Err(errors) => {
            let first = errors.into_iter().next().ok_or_else(|| ParseError {
                message: "lexical failure".to_string(),
                span: Span::new(0, 0),
            })?;
            return Err(ParseError {
                message: first.message,
                span: first.span,
            });
        }
    };

    ensure_token_nesting_within_limit(&tokens)?;

    let mut parser = Parser::new(tokens);
    let expr = parser.parse_expr()?;
    if !parser.check(TokenKind::EOF) {
        return Err(ParseError {
            message: "unexpected tokens after expression".to_string(),
            span: parser.current().span(),
        });
    }

    Ok(expr)
}

/// Normalizes shorthand HTML interpolation source.
///
/// Inputs: raw source from an interpolation.
/// Output: Terlan expression source.
/// Transformation: rewrites HTML fragments and `for` shorthands into formal
/// Terlan expression syntax.
fn normalize_interpolated_html_expr(raw: String) -> String {
    let trimmed = raw.trim();
    if let Some(normalized) = normalize_for_html_expr(trimmed) {
        return normalized;
    }
    if starts_with_html_tag(trimmed) {
        return format!("html {{ {} }}", trimmed);
    }

    normalize_case_html_branches(trimmed)
}

/// Reports whether source starts with an HTML tag.
///
/// Inputs: trimmed interpolation source.
/// Output: `true` for opening or closing HTML tag prefixes.
/// Transformation: checks the first two characters without parsing full HTML.
fn starts_with_html_tag(raw: &str) -> bool {
    let mut chars = raw.chars();
    chars.next() == Some('<')
        && chars
            .next()
            .is_some_and(|ch| ch == '/' || ch.is_ascii_alphabetic())
}

/// Normalizes HTML `for` shorthand.
///
/// Inputs: trimmed interpolation source beginning with `for `.
/// Output: list-comprehension expression source when the shape matches.
/// Transformation: extracts the generator header and braced body, then rewrites
/// the body through normal HTML interpolation normalization.
fn normalize_for_html_expr(raw: &str) -> Option<String> {
    let rest = raw.strip_prefix("for ")?;
    let raw_offset = raw.len() - rest.len();
    let chars = raw.chars().collect::<Vec<_>>();
    let body_start = chars
        .iter()
        .enumerate()
        .skip(raw_offset)
        .find_map(|(idx, ch)| (*ch == '{').then_some(idx))?;
    let body_end = find_matching_brace(&chars, body_start)?;
    if chars[body_end + 1..].iter().any(|ch| !ch.is_whitespace()) {
        return None;
    }

    let header = chars[raw_offset..body_start]
        .iter()
        .collect::<String>()
        .trim()
        .to_string();
    let (pattern, source) = header.split_once("<-")?;
    let body = chars[body_start + 1..body_end]
        .iter()
        .collect::<String>()
        .trim()
        .to_string();
    let item = normalize_interpolated_html_expr(body);

    Some(format!(
        "[{} | {} <- {}]",
        item,
        pattern.trim(),
        source.trim()
    ))
}

/// Finds a matching brace in interpolation source.
///
/// Inputs: character slice and index of the opening brace.
/// Output: index of the matching closing brace.
/// Transformation: tracks brace depth while ignoring braces inside quotes.
fn find_matching_brace(chars: &[char], open: usize) -> Option<usize> {
    let mut pos = open;
    let mut depth = 0usize;
    let mut quote = None;

    while pos < chars.len() {
        let ch = chars[pos];
        if let Some(current_quote) = quote {
            pos += 1;
            if ch == current_quote {
                quote = None;
            }
            continue;
        }

        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            pos += 1;
            continue;
        }

        if ch == '{' {
            depth += 1;
        } else if ch == '}' {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(pos);
            }
        }

        pos += 1;
    }

    None
}

/// Normalizes HTML fragments after case arrows.
///
/// Inputs: raw expression source.
/// Output: source with arrow-following HTML fragments wrapped as `html { ... }`.
/// Transformation: scans for `->` and wraps the immediately following balanced
/// HTML fragment when present.
fn normalize_case_html_branches(raw: &str) -> String {
    let chars = raw.chars().collect::<Vec<_>>();
    let mut out = String::new();
    let mut pos = 0usize;

    while pos < chars.len() {
        if pos + 1 < chars.len() && chars[pos] == '-' && chars[pos + 1] == '>' {
            out.push_str("->");
            pos += 2;

            let ws_start = pos;
            while pos < chars.len() && chars[pos].is_whitespace() {
                pos += 1;
            }
            out.extend(chars[ws_start..pos].iter());

            if pos < chars.len() && chars[pos] == '<' {
                if let Some(end) = find_html_fragment_end(&chars, pos) {
                    out.push_str("html { ");
                    out.extend(chars[pos..end].iter());
                    out.push_str(" }");
                    pos = end;
                    continue;
                }
            }

            continue;
        }

        out.push(chars[pos]);
        pos += 1;
    }

    out
}

/// Finds the end of a balanced HTML fragment.
///
/// Inputs: character slice and index of the opening `<`.
/// Output: exclusive end index of the balanced fragment.
/// Transformation: tracks nested element names, self-closing tags, and quoted
/// attribute text.
fn find_html_fragment_end(chars: &[char], start: usize) -> Option<usize> {
    let mut pos = start;
    let mut stack = Vec::<String>::new();

    while pos < chars.len() {
        if chars[pos] != '<' {
            pos += 1;
            continue;
        }

        let closing = pos + 1 < chars.len() && chars[pos + 1] == '/';
        pos += if closing { 2 } else { 1 };
        let name_start = pos;
        while pos < chars.len()
            && (chars[pos].is_ascii_alphanumeric()
                || chars[pos] == '_'
                || chars[pos] == '-'
                || chars[pos] == ':')
        {
            pos += 1;
        }
        if name_start == pos {
            return None;
        }
        let name = chars[name_start..pos].iter().collect::<String>();

        let mut quote = None;
        let mut self_closing = false;
        while pos < chars.len() {
            let ch = chars[pos];
            if let Some(current_quote) = quote {
                pos += 1;
                if ch == current_quote {
                    quote = None;
                }
                continue;
            }

            if ch == '"' || ch == '\'' {
                quote = Some(ch);
                pos += 1;
                continue;
            }

            if ch == '>' {
                self_closing = pos > start && chars[pos.saturating_sub(1)] == '/';
                pos += 1;
                break;
            }

            pos += 1;
        }

        if closing {
            if stack.pop().as_deref() != Some(name.as_str()) {
                return None;
            }
            if stack.is_empty() {
                return Some(pos);
            }
        } else if !self_closing {
            stack.push(name);
        } else if stack.is_empty() {
            return Some(pos);
        }
    }

    None
}
