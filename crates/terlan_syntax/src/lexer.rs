use crate::{
    span::Span,
    token::{Token, TokenKind},
};

#[derive(Debug, Clone)]
pub struct LexError {
    pub message: String,
    pub span: Span,
}

pub fn lex(input: &str) -> Result<Vec<Token>, Vec<LexError>> {
    let chars: Vec<char> = input.chars().collect();
    let mut tokens = Vec::new();
    let mut errors = Vec::new();
    let mut i = 0usize;

    while i < chars.len() {
        let ch = chars[i];

        if ch.is_whitespace() {
            i += 1;
            continue;
        }

        if ch == '/' && i + 2 < chars.len() && chars[i + 1] == '*' && chars[i + 2] == '*' {
            let start = i;
            match parse_block_comment(&chars, i) {
                Some((text, end)) => {
                    if doc_block_contains_nested_doc_start(&text) {
                        errors.push(LexError {
                            message: "nested documentation block comments are not supported"
                                .to_string(),
                            span: Span::new(start, end),
                        });
                        break;
                    }
                    tokens.push(Token::new(
                        TokenKind::DocBlockComment,
                        normalize_doc_block_comment_text(&text),
                        start,
                        end,
                    ));
                    i = end;
                    continue;
                }
                None => {
                    errors.push(LexError {
                        message: "unterminated documentation block comment".to_string(),
                        span: Span::new(i, chars.len()),
                    });
                    break;
                }
            }
        }

        if ch == '/' && i + 1 < chars.len() && chars[i + 1] == '*' {
            match parse_block_comment(&chars, i) {
                Some((text, end)) => {
                    tokens.push(Token::new(TokenKind::Comment, text, i, end));
                    i = end;
                    continue;
                }
                None => {
                    errors.push(LexError {
                        message: "unterminated block comment".to_string(),
                        span: Span::new(i, chars.len()),
                    });
                    break;
                }
            }
        }

        if ch == '/' && i + 2 < chars.len() && chars[i + 1] == '/' && chars[i + 2] == '/' {
            let start = i;
            i += 3;
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            let text = normalize_doc_comment_text(&chars[start + 3..i]);
            tokens.push(Token::new(TokenKind::DocComment, text, start, i));
            continue;
        }

        if ch == '/' && i + 2 < chars.len() && chars[i + 1] == '/' && chars[i + 2] == '!' {
            let start = i;
            i += 3;
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            let text = normalize_doc_comment_text(&chars[start + 3..i]);
            tokens.push(Token::new(TokenKind::ModuleDocComment, text, start, i));
            continue;
        }

        if ch == '/' && i + 1 < chars.len() && chars[i + 1] == '/' {
            let start = i;
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            let text: String = chars[start..i].iter().collect();
            tokens.push(Token::new(TokenKind::Comment, text, start, i));
            continue;
        }

        if ch == '%' {
            let start = i;
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            let text: String = chars[start..i].iter().collect();
            tokens.push(Token::new(TokenKind::Comment, text, start, i));
            continue;
        }

        if ch == '"' {
            match parse_string(&chars, i) {
                Some((text, end)) => {
                    tokens.push(Token::new(TokenKind::String, text, i, end));
                    i = end;
                    continue;
                }
                None => {
                    errors.push(LexError {
                        message: "unterminated string literal".to_string(),
                        span: Span::new(i, chars.len()),
                    });
                    break;
                }
            }
        }

        if ch == '\'' {
            match parse_single_quoted(&chars, i) {
                Some((text, end)) => {
                    tokens.push(Token::new(TokenKind::String, text, i, end));
                    i = end;
                    continue;
                }
                None => {
                    errors.push(LexError {
                        message: "unterminated string literal".to_string(),
                        span: Span::new(i, chars.len()),
                    });
                    break;
                }
            }
        }

        if ch == '<' && i + 1 < chars.len() && chars[i + 1] == '<' {
            if let Some((text, end)) = parse_binary(&chars, i) {
                tokens.push(Token::new(TokenKind::Binary, text, i, end));
                i = end;
                continue;
            }
        }

        if let Some((kind, text, end)) = match_two_char_token(&chars, i) {
            tokens.push(Token::new(kind, text, i, end));
            i = end;
            continue;
        }

        let (kind, text, end) = match ch {
            '(' => (TokenKind::LParen, "(".to_string(), i + 1),
            ')' => (TokenKind::RParen, ")".to_string(), i + 1),
            '[' => (TokenKind::LBracket, "[".to_string(), i + 1),
            ']' => (TokenKind::RBracket, "]".to_string(), i + 1),
            '{' => (TokenKind::LBrace, "{".to_string(), i + 1),
            '}' => (TokenKind::RBrace, "}".to_string(), i + 1),
            '#' => (TokenKind::Hash, "#".to_string(), i + 1),
            ',' => (TokenKind::Comma, ",".to_string(), i + 1),
            '.' => (TokenKind::Dot, ".".to_string(), i + 1),
            ':' => (TokenKind::Colon, ":".to_string(), i + 1),
            ';' => (TokenKind::Semicolon, ";".to_string(), i + 1),
            '|' => (TokenKind::Pipe, "|".to_string(), i + 1),
            '+' => (TokenKind::Plus, "+".to_string(), i + 1),
            '-' => (TokenKind::Minus, "-".to_string(), i + 1),
            '*' => (TokenKind::Star, "*".to_string(), i + 1),
            '/' => (TokenKind::Slash, "/".to_string(), i + 1),
            '!' => (TokenKind::Bang, "!".to_string(), i + 1),
            '?' => (TokenKind::Question, "?".to_string(), i + 1),
            '@' => (TokenKind::At, "@".to_string(), i + 1),
            '<' => (TokenKind::Lt, "<".to_string(), i + 1),
            '>' => (TokenKind::Gt, ">".to_string(), i + 1),
            _ => {
                if ch.is_ascii_digit() {
                    let (kind, text, end) = lex_number_token(&chars, i);
                    tokens.push(Token::new(kind, text, i, end));
                    i = end;
                    continue;
                }

                if is_ident_start(ch) {
                    let start = i;
                    let mut j = i + 1;
                    while j < chars.len() && is_ident_continue(chars[j]) {
                        j += 1;
                    }
                    let text: String = chars[start..j].iter().collect();
                    let kind = match text.as_str() {
                        "module" => TokenKind::Module,
                        "pub" => TokenKind::Pub,
                        "macro" => TokenKind::Macro,
                        "constructor" => TokenKind::Constructor,
                        "export" => TokenKind::Export,
                        "import" => TokenKind::Import,
                        "type" => TokenKind::Type,
                        "nominal" => TokenKind::Nominal,
                        "opaque" => TokenKind::Opaque,
                        "trait" => TokenKind::Trait,
                        "impl" => TokenKind::Impl,
                        "implements" => TokenKind::Implements,
                        "for" => TokenKind::For,
                        "template" => TokenKind::Template,
                        "where" => TokenKind::Where,
                        "extends" => TokenKind::Extends,
                        "derives" => TokenKind::Derives,
                        "struct" => TokenKind::Struct,
                        "case" => TokenKind::Case,
                        "try" => TokenKind::Try,
                        "catch" => TokenKind::Catch,
                        "after" => TokenKind::After,
                        "let" => TokenKind::Let,
                        "if" => TokenKind::If,
                        "when" => TokenKind::When,
                        "with" => TokenKind::With,
                        "and" => TokenKind::And,
                        "or" => TokenKind::Or,
                        "div" => TokenKind::DivRem,
                        "rem" => TokenKind::Rem,
                        _ => {
                            if ch.is_ascii_uppercase() {
                                TokenKind::Var
                            } else {
                                TokenKind::Atom
                            }
                        }
                    };
                    tokens.push(Token::new(kind, text, start, j));
                    i = j;
                    continue;
                }

                errors.push(LexError {
                    message: format!("unrecognized character {:?}", ch),
                    span: Span::new(i, i + 1),
                });
                i += 1;
                continue;
            }
        };

        tokens.push(Token::new(kind, text, i, end));
        i = end;
    }

    tokens.push(Token::new(TokenKind::EOF, "", chars.len(), chars.len()));

    if errors.is_empty() {
        Ok(tokens)
    } else {
        Err(errors)
    }
}

/// Lexes one numeric literal token from a digit-starting source position.
///
/// Inputs:
/// - `chars`: complete source as characters.
/// - `start`: index of the first digit in the numeric literal.
///
/// Output:
/// - Token kind, token text, and exclusive end index for the numeric token.
///
/// Transformation:
/// - Recognizes canonical prefixed integer forms `0b...`, `0x...`, and
///   `0o...` as integer tokens, and otherwise applies the existing decimal
///   integer/float scan.
fn lex_number_token(chars: &[char], start: usize) -> (TokenKind, String, usize) {
    if let Some(end) = prefixed_integer_literal_end(chars, start) {
        return (
            TokenKind::Int,
            chars[start..end].iter().collect::<String>(),
            end,
        );
    }

    let mut end = start + 1;
    let mut has_dot = false;
    while end < chars.len() {
        if chars[end].is_ascii_digit() {
            end += 1;
        } else if chars[end] == '.'
            && !has_dot
            && end + 1 < chars.len()
            && chars[end + 1].is_ascii_digit()
        {
            has_dot = true;
            end += 1;
        } else {
            break;
        }
    }

    let kind = if has_dot {
        TokenKind::Float
    } else {
        TokenKind::Int
    };
    (kind, chars[start..end].iter().collect::<String>(), end)
}

/// Finds the exclusive end of a prefixed integer literal.
///
/// Inputs:
/// - `chars`: complete source as characters.
/// - `start`: index of the leading `0`.
///
/// Output:
/// - `Some(end)` when the source at `start` uses a recognized integer prefix
///   and has at least one body character; otherwise `None`.
///
/// Transformation:
/// - Consumes the prefix and following alphanumeric body as one token so the
///   parser can validate radix-specific digit correctness in one place.
fn prefixed_integer_literal_end(chars: &[char], start: usize) -> Option<usize> {
    if chars.get(start) != Some(&'0') || start + 2 >= chars.len() {
        return None;
    }

    let prefix = chars[start + 1];
    if !matches!(prefix, 'b' | 'x' | 'o') {
        return None;
    }

    let mut end = start + 2;
    while end < chars.len() && chars[end].is_ascii_alphanumeric() {
        end += 1;
    }

    (end > start + 2).then_some(end)
}

fn match_two_char_token(chars: &[char], i: usize) -> Option<(TokenKind, String, usize)> {
    if i + 1 >= chars.len() {
        return None;
    }

    if i + 2 < chars.len() && chars[i] == '.' && chars[i + 1] == '.' && chars[i + 2] == '.' {
        return Some((TokenKind::Ellipsis, "...".to_string(), i + 3));
    }

    match (chars[i], chars[i + 1]) {
        ('-', '>') => Some((TokenKind::Arrow, "->".to_string(), i + 2)),
        ('=', '>') => Some((TokenKind::FatArrow, "=>".to_string(), i + 2)),
        (':', '=') => Some((TokenKind::Equals, ":=".to_string(), i + 2)),
        ('=', ':') if i + 2 < chars.len() && chars[i + 2] == '=' => {
            Some((TokenKind::EqEqEq, "=:=".to_string(), i + 3))
        }
        ('=', '/') if i + 2 < chars.len() && chars[i + 2] == '=' => {
            Some((TokenKind::NotEqEq, "=/=".to_string(), i + 3))
        }
        ('=', '=') => Some((TokenKind::EqEq, "==".to_string(), i + 2)),
        ('!', '=') => Some((TokenKind::NotEq, "!=".to_string(), i + 2)),
        ('/', '=') => Some((TokenKind::NotEq, "/=".to_string(), i + 2)),
        ('<', '-') => Some((TokenKind::LtMinus, "<-".to_string(), i + 2)),
        ('<', '=') => Some((TokenKind::LtEq, "<=".to_string(), i + 2)),
        ('>', '=') => Some((TokenKind::GtEq, ">=".to_string(), i + 2)),
        ('|', '>') => Some((TokenKind::PipeForward, "|>".to_string(), i + 2)),
        ('|', '|') => Some((TokenKind::Or, "||".to_string(), i + 2)),
        ('&', '&') => Some((TokenKind::And, "&&".to_string(), i + 2)),
        ('=', _) => Some((TokenKind::Equals, "=".to_string(), i + 1)),
        _ => None,
    }
}

fn parse_string(chars: &[char], start: usize) -> Option<(String, usize)> {
    let mut i = start + 1;
    while i < chars.len() {
        if chars[i] == '"' {
            let end = i + 1;
            return Some((chars[start..end].iter().collect(), end));
        }
        if chars[i] == '\\' && i + 1 < chars.len() {
            i += 2;
            continue;
        }
        i += 1;
    }
    None
}

fn parse_single_quoted(chars: &[char], start: usize) -> Option<(String, usize)> {
    let mut i = start + 1;
    while i < chars.len() {
        if chars[i] == '\'' {
            let end = i + 1;
            return Some((chars[start..end].iter().collect(), end));
        }
        if chars[i] == '\\' && i + 1 < chars.len() {
            i += 2;
            continue;
        }
        i += 1;
    }
    None
}

fn parse_binary(chars: &[char], start: usize) -> Option<(String, usize)> {
    if chars[start] != '<' || start + 1 >= chars.len() || chars[start + 1] != '<' {
        return None;
    }

    let mut i = start + 2;
    while i + 1 < chars.len() {
        if chars[i] == '>' && chars[i + 1] == '>' {
            return Some((chars[start..i + 2].iter().collect(), i + 2));
        }
        if chars[i] == '"' {
            if let Some((_, end)) = parse_string(chars, i) {
                i = end;
                continue;
            }
            return None;
        }
        i += 1;
    }

    None
}

/// Parses a C-style block comment and returns its exact source text.
///
/// Inputs:
/// - `chars`: complete source as characters.
/// - `start`: index of the opening `/` in `/*`.
///
/// Output:
/// - The complete block comment text and exclusive end offset, or `None` when
///   no terminating `*/` exists.
///
/// Transformation:
/// - Scans forward without nesting so public `/** ... */` docs and ordinary
///   `/* ... */` comments share the same low-level delimiter handling.
fn parse_block_comment(chars: &[char], start: usize) -> Option<(String, usize)> {
    if chars.get(start) != Some(&'/') || chars.get(start + 1) != Some(&'*') {
        return None;
    }

    let mut i = start + 2;
    while i + 1 < chars.len() {
        if chars[i] == '*' && chars[i + 1] == '/' {
            let end = i + 2;
            return Some((chars[start..end].iter().collect(), end));
        }
        i += 1;
    }

    None
}

/// Checks whether a public documentation block contains another doc opener.
///
/// Inputs:
/// - `text`: complete source text for a `/** ... */` block.
///
/// Output:
/// - `true` when the inner text contains another `/**` opener.
///
/// Transformation:
/// - Removes the outer documentation delimiters and searches only the inner
///   body so the opening delimiter itself is not treated as nested syntax.
fn doc_block_contains_nested_doc_start(text: &str) -> bool {
    text.strip_prefix("/**")
        .and_then(|value| value.strip_suffix("*/"))
        .is_some_and(|inner| inner.contains("/**"))
}

fn normalize_doc_comment_text(chars: &[char]) -> String {
    let text = chars.iter().collect::<String>();
    text.strip_prefix(' ').unwrap_or(&text).to_string()
}

/// Normalizes a canonical Terlan documentation block into public doc text.
///
/// Inputs:
/// - `text`: complete source text for a `/** ... */` block.
///
/// Output:
/// - Public documentation text with delimiters removed and leading `*` margins
///   stripped from each line.
///
/// Transformation:
/// - Removes the outer documentation delimiters, trims empty boundary lines,
///   and normalizes common JSDoc/TypeDoc formatting while preserving internal
///   blank lines and tag text for later documentation tooling.
fn normalize_doc_block_comment_text(text: &str) -> String {
    let inner = text
        .strip_prefix("/**")
        .and_then(|value| value.strip_suffix("*/"))
        .unwrap_or(text);
    let mut lines = inner
        .lines()
        .map(|line| {
            let trimmed = line.trim_start();
            if let Some(rest) = trimmed.strip_prefix('*') {
                rest.strip_prefix(' ').unwrap_or(rest).to_string()
            } else {
                trimmed.to_string()
            }
        })
        .collect::<Vec<_>>();

    while lines.first().is_some_and(|line| line.trim().is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|line| line.trim().is_empty()) {
        lines.pop();
    }

    lines.join("\n")
}

fn is_ident_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

fn is_ident_continue(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

#[cfg(test)]
mod tests {
    use super::lex;
    use crate::token::TokenKind;

    #[test]
    fn module_decl_dot_is_a_separator_not_identifier_char() {
        let src = "module mathx.\n";
        let tokens = lex(src).expect("lexer should parse module declaration");
        assert_eq!(tokens[0].text, "module");
        assert_eq!(tokens[1].text, "mathx");
        assert_eq!(tokens[2].text, ".");
        assert_eq!(tokens[3].text, "");
        assert_eq!(tokens[3].kind, TokenKind::EOF);
    }

    #[test]
    fn doc_comments_are_distinct_tokens() {
        let src = "//! Module docs.\n/// Adds one.\nmodule mathx.\n";
        let tokens = lex(src).expect("lexer should parse doc comments");
        assert_eq!(tokens[0].kind, TokenKind::ModuleDocComment);
        assert_eq!(tokens[0].text, "Module docs.");
        assert_eq!(tokens[1].kind, TokenKind::DocComment);
        assert_eq!(tokens[1].text, "Adds one.");
    }

    #[test]
    fn doc_block_comments_are_public_doc_tokens() {
        let src = "/**\n * Adds one.\n *\n * @param x The value.\n * @returns The incremented value.\n */\nmodule mathx.\n";
        let tokens = lex(src).expect("lexer should parse doc block comments");
        assert_eq!(tokens[0].kind, TokenKind::DocBlockComment);
        assert_eq!(
            tokens[0].text,
            "Adds one.\n\n@param x The value.\n@returns The incremented value."
        );
    }

    #[test]
    fn line_and_block_comments_are_not_public_docs() {
        let src = "// implementation note\n/* implementation block */\nmodule mathx.\n";
        let tokens = lex(src).expect("lexer should parse implementation comments");
        assert_eq!(tokens[0].kind, TokenKind::Comment);
        assert_eq!(tokens[1].kind, TokenKind::Comment);
        assert_eq!(tokens[2].kind, TokenKind::Module);
    }

    #[test]
    fn rejects_unterminated_doc_block_comments() {
        let errors = lex("/** missing close").expect_err("unterminated doc block");
        assert_eq!(
            errors[0].message,
            "unterminated documentation block comment"
        );
    }

    #[test]
    fn rejects_nested_doc_block_comments() {
        let errors = lex("/** Outer /** Inner */ Outer */").expect_err("nested doc block");
        assert_eq!(
            errors[0].message,
            "nested documentation block comments are not supported"
        );
    }

    /// Verifies that exact inequality is tokenized as one comparison operator.
    ///
    /// Inputs:
    /// - A source fragment containing `=/=`.
    ///
    /// Output:
    /// - Test passes when the lexer emits `TokenKind::NotEqEq` for the exact
    ///   inequality operator.
    ///
    /// Transformation:
    /// - Runs the lexer over a short comparison expression and inspects the
    ///   middle token, guarding against splitting `=/=` into `=` plus `/=`.
    #[test]
    fn exact_inequality_is_one_token() {
        let tokens = lex("x =/= y").expect("lexer should parse exact inequality");

        assert_eq!(tokens[1].kind, TokenKind::NotEqEq);
        assert_eq!(tokens[1].text, "=/=");
    }

    /// Verifies that canonical inequality is tokenized as one comparison operator.
    ///
    /// Inputs:
    /// - A source fragment containing `!=`.
    ///
    /// Output:
    /// - Test passes when the lexer emits `TokenKind::NotEq` for the canonical
    ///   inequality operator.
    ///
    /// Transformation:
    /// - Runs the lexer over a short comparison expression and inspects the
    ///   middle token, guarding against treating `!` as unary syntax before `=`.
    #[test]
    fn canonical_inequality_is_one_token() {
        let tokens = lex("x != y").expect("lexer should parse canonical inequality");

        assert_eq!(tokens[1].kind, TokenKind::NotEq);
        assert_eq!(tokens[1].text, "!=");
    }

    /// Verifies that symbolic boolean operators tokenize as boolean operators.
    ///
    /// Inputs:
    /// - A source fragment containing `&&` and `||`.
    ///
    /// Output:
    /// - Test passes when `&&` emits `TokenKind::And` and `||` emits
    ///   `TokenKind::Or`.
    ///
    /// Transformation:
    /// - Runs the lexer over a short boolean expression and inspects the
    ///   operator tokens, guarding against treating `||` as a list/type pipe.
    #[test]
    fn symbolic_boolean_operators_are_boolean_tokens() {
        let tokens = lex("a && b || c").expect("lexer should parse symbolic boolean operators");

        assert_eq!(tokens[1].kind, TokenKind::And);
        assert_eq!(tokens[1].text, "&&");
        assert_eq!(tokens[3].kind, TokenKind::Or);
        assert_eq!(tokens[3].text, "||");
    }
}
