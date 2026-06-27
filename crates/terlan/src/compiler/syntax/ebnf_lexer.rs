use crate::terlan_syntax::ebnf::{EbnfError, EbnfParseResult};
use crate::terlan_syntax::span::Span;

/// One token emitted by the EBNF lexer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EbnfToken {
    pub(crate) kind: EbnfTokenKind,
    pub(crate) span: Span,
}

/// Token kinds recognized in EBNF source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EbnfTokenKind {
    Identifier(String),
    Terminal(String),
    CharacterClass(String),
    Special(String),
    Define,
    Dot,
    Pipe,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    LParen,
    RParen,
    Star,
    Plus,
    Eof,
}

/// Lexer for the small EBNF grammar language.
pub(crate) struct EbnfLexer<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> EbnfLexer<'a> {
    /// Creates an EBNF lexer.
    ///
    /// Inputs:
    /// - `input`: EBNF source text.
    ///
    /// Output:
    /// - Lexer positioned at the start of the input.
    ///
    /// Transformation:
    /// - Stores the borrowed input and initializes the byte cursor.
    pub(crate) fn new(input: &'a str) -> Self {
        Self { input, pos: 0 }
    }

    /// Lexes the full EBNF source into tokens.
    ///
    /// Inputs:
    /// - `self`: lexer over an EBNF input string.
    ///
    /// Output:
    /// - Token stream terminated with EOF, or a lexer diagnostic.
    ///
    /// Transformation:
    /// - Skips whitespace/comments and emits structural, terminal, class,
    ///   special-sequence, and identifier tokens with spans.
    pub(crate) fn lex(mut self) -> EbnfParseResult<Vec<EbnfToken>> {
        let mut tokens = Vec::new();
        while !self.is_eof() {
            self.skip_ws_and_comments()?;
            if self.is_eof() {
                break;
            }

            let start = self.pos;
            let kind = match self.current_char().unwrap() {
                ':' if self.starts_with("::=") => {
                    self.pos += 3;
                    EbnfTokenKind::Define
                }
                '.' => {
                    self.bump_char();
                    EbnfTokenKind::Dot
                }
                '|' => {
                    self.bump_char();
                    EbnfTokenKind::Pipe
                }
                '{' => {
                    self.bump_char();
                    EbnfTokenKind::LBrace
                }
                '}' => {
                    self.bump_char();
                    EbnfTokenKind::RBrace
                }
                '[' if self.is_character_class_start() => self.lex_character_class()?,
                '[' => {
                    self.bump_char();
                    EbnfTokenKind::LBracket
                }
                ']' => {
                    self.bump_char();
                    EbnfTokenKind::RBracket
                }
                '(' => {
                    self.bump_char();
                    EbnfTokenKind::LParen
                }
                ')' => {
                    self.bump_char();
                    EbnfTokenKind::RParen
                }
                '*' => {
                    self.bump_char();
                    EbnfTokenKind::Star
                }
                '+' => {
                    self.bump_char();
                    EbnfTokenKind::Plus
                }
                '?' => self.lex_special()?,
                '"' => self.lex_terminal()?,
                ch if is_ebnf_ident_start(ch) => self.lex_identifier(),
                ch => {
                    return Err(EbnfError {
                        message: format!("unexpected EBNF character '{ch}'"),
                        span: Span::new(start, start + ch.len_utf8()),
                    })
                }
            };
            tokens.push(EbnfToken {
                kind,
                span: Span::new(start, self.pos),
            });
        }

        tokens.push(EbnfToken {
            kind: EbnfTokenKind::Eof,
            span: Span::new(self.pos, self.pos),
        });
        Ok(tokens)
    }

    /// Skips whitespace and nested EBNF comments.
    ///
    /// Inputs:
    /// - Mutable lexer cursor.
    ///
    /// Output:
    /// - `Ok(())` after all trivia has been consumed.
    ///
    /// Transformation:
    /// - Advances over whitespace and balanced `(* ... *)` comment blocks.
    fn skip_ws_and_comments(&mut self) -> EbnfParseResult<()> {
        loop {
            while matches!(self.current_char(), Some(ch) if ch.is_whitespace()) {
                self.bump_char();
            }

            if !self.starts_with("(*") {
                return Ok(());
            }

            let start = self.pos;
            self.pos += 2;
            let mut depth = 1usize;
            while depth > 0 {
                if self.is_eof() {
                    return Err(EbnfError {
                        message: "unterminated EBNF comment".into(),
                        span: Span::new(start, self.pos),
                    });
                }
                if self.starts_with("(*") {
                    self.pos += 2;
                    depth += 1;
                } else if self.starts_with("*)") {
                    self.pos += 2;
                    depth -= 1;
                } else {
                    self.bump_char();
                }
            }
        }
    }

    /// Lexes an EBNF identifier.
    ///
    /// Inputs:
    /// - Lexer cursor at an identifier-start character.
    ///
    /// Output:
    /// - Identifier token kind.
    ///
    /// Transformation:
    /// - Consumes identifier-start and identifier-continue characters.
    fn lex_identifier(&mut self) -> EbnfTokenKind {
        let start = self.pos;
        self.bump_char();
        while matches!(self.current_char(), Some(ch) if is_ebnf_ident_continue(ch)) {
            self.bump_char();
        }
        EbnfTokenKind::Identifier(self.input[start..self.pos].to_string())
    }

    /// Lexes a double-quoted EBNF terminal.
    ///
    /// Inputs:
    /// - Lexer cursor at the opening quote.
    ///
    /// Output:
    /// - Terminal token kind, or an unterminated terminal diagnostic.
    ///
    /// Transformation:
    /// - Removes quotes and decodes the supported escape sequences.
    fn lex_terminal(&mut self) -> EbnfParseResult<EbnfTokenKind> {
        let start = self.pos;
        self.bump_char();
        let mut value = String::new();
        while let Some(ch) = self.current_char() {
            match ch {
                '"' => {
                    self.bump_char();
                    return Ok(EbnfTokenKind::Terminal(value));
                }
                '\\' => {
                    self.bump_char();
                    let Some(escaped) = self.current_char() else {
                        return Err(EbnfError {
                            message: "unterminated escape in EBNF terminal".into(),
                            span: Span::new(start, self.pos),
                        });
                    };
                    value.push(match escaped {
                        'n' => '\n',
                        'r' => '\r',
                        't' => '\t',
                        '"' => '"',
                        '\\' => '\\',
                        other => other,
                    });
                    self.bump_char();
                }
                other => {
                    value.push(other);
                    self.bump_char();
                }
            }
        }

        Err(EbnfError {
            message: "unterminated EBNF terminal".into(),
            span: Span::new(start, self.pos),
        })
    }

    /// Lexes an EBNF character class.
    ///
    /// Inputs:
    /// - Lexer cursor at the opening bracket of a character class.
    ///
    /// Output:
    /// - Character-class token kind, or an unterminated class diagnostic.
    ///
    /// Transformation:
    /// - Captures the raw class payload between brackets.
    fn lex_character_class(&mut self) -> EbnfParseResult<EbnfTokenKind> {
        let start = self.pos;
        self.bump_char();
        let content_start = self.pos;
        while let Some(ch) = self.current_char() {
            if ch == ']' {
                let value = self.input[content_start..self.pos].to_string();
                self.bump_char();
                return Ok(EbnfTokenKind::CharacterClass(value));
            }
            self.bump_char();
        }

        Err(EbnfError {
            message: "unterminated EBNF character class".into(),
            span: Span::new(start, self.pos),
        })
    }

    /// Lexes an EBNF special sequence.
    ///
    /// Inputs:
    /// - Lexer cursor at the opening `?`.
    ///
    /// Output:
    /// - Special-sequence token kind, or an unterminated sequence diagnostic.
    ///
    /// Transformation:
    /// - Captures trimmed text between `?` delimiters.
    fn lex_special(&mut self) -> EbnfParseResult<EbnfTokenKind> {
        let start = self.pos;
        self.bump_char();
        let content_start = self.pos;
        while let Some(ch) = self.current_char() {
            if ch == '?' {
                let value = self.input[content_start..self.pos].trim().to_string();
                self.bump_char();
                return Ok(EbnfTokenKind::Special(value));
            }
            self.bump_char();
        }

        Err(EbnfError {
            message: "unterminated EBNF special sequence".into(),
            span: Span::new(start, self.pos),
        })
    }

    /// Checks whether the remaining input starts with a prefix.
    ///
    /// Inputs:
    /// - `prefix`: text to compare at the current cursor.
    ///
    /// Output:
    /// - `true` when the input suffix starts with `prefix`.
    ///
    /// Transformation:
    /// - Reads the input slice without advancing.
    fn starts_with(&self, prefix: &str) -> bool {
        self.input[self.pos..].starts_with(prefix)
    }

    /// Reports whether the current bracket begins a character class.
    ///
    /// Inputs:
    /// - Lexer cursor at `[`.
    ///
    /// Output:
    /// - `true` when the bracketed payload looks like a simple class range.
    ///
    /// Transformation:
    /// - Peeks to the closing bracket and validates the payload shape.
    fn is_character_class_start(&self) -> bool {
        let Some(close_offset) = self.input[self.pos + 1..].find(']') else {
            return false;
        };
        let inner = &self.input[self.pos + 1..self.pos + 1 + close_offset];
        !inner.is_empty()
            && inner.contains('-')
            && inner
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
    }

    /// Returns the current character.
    ///
    /// Inputs:
    /// - Current lexer cursor.
    ///
    /// Output:
    /// - Next Unicode scalar value, or `None` at EOF.
    ///
    /// Transformation:
    /// - Reads from the current byte offset without advancing.
    fn current_char(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }

    /// Advances over the current character.
    ///
    /// Inputs:
    /// - Current lexer cursor.
    ///
    /// Output:
    /// - Character consumed, or `None` at EOF.
    ///
    /// Transformation:
    /// - Increments the byte cursor by the UTF-8 width of the character.
    fn bump_char(&mut self) -> Option<char> {
        let ch = self.current_char()?;
        self.pos += ch.len_utf8();
        Some(ch)
    }

    /// Reports whether the lexer cursor is at end of input.
    ///
    /// Inputs:
    /// - Current lexer cursor.
    ///
    /// Output:
    /// - `true` when the cursor is at or past the input length.
    ///
    /// Transformation:
    /// - Compares byte cursor with input byte length.
    fn is_eof(&self) -> bool {
        self.pos >= self.input.len()
    }
}

/// Checks whether a character can start an EBNF identifier.
///
/// Inputs:
/// - `ch`: character to classify.
///
/// Output:
/// - `true` when the character is alphabetic or underscore.
///
/// Transformation:
/// - Applies the EBNF parser's conservative ASCII identifier-start rule.
fn is_ebnf_ident_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

/// Checks whether a character can continue an EBNF identifier.
///
/// Inputs:
/// - `ch`: character to classify.
///
/// Output:
/// - `true` when the character is alphanumeric or underscore.
///
/// Transformation:
/// - Applies the EBNF parser's conservative ASCII identifier-continue rule.
fn is_ebnf_ident_continue(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}
