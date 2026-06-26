/// Token category emitted by the Terlan lexer.
///
/// Inputs:
/// - Raw Terlan source text.
///
/// Output:
/// - Closed token category used by the parser.
///
/// Transformation:
/// - Classifies keywords, identifiers, literals, punctuation, operators,
///   comments, and EOF markers while leaving exact source text on `Token`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    Module,
    Pub,
    Macro,
    Constructor,
    Export,
    Import,
    Type,
    Nominal,
    Opaque,
    Trait,
    Impl,
    Implements,
    Includes,
    For,
    Template,
    Where,
    Extends,
    Struct,
    Case,
    Of,
    End,
    Try,
    Catch,
    After,
    Fun,
    Let,
    If,
    When,
    With,
    And,
    Or,

    Atom,
    Ident,
    Var,

    Int,
    Float,
    String,
    Binary,

    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Hash,
    Comma,
    Dot,
    Ellipsis,
    Colon,
    Semicolon,
    Pipe,
    PipeForward,
    Arrow,
    FatArrow,
    Equals,
    EqEq,
    EqEqEq,
    NotEq,
    NotEqEq,
    Plus,
    Minus,
    Star,
    Slash,
    DivRem,
    Rem,
    Bang,
    Question,
    Dollar,
    At,
    Lt,
    Gt,
    LtEq,
    GtEq,
    LtMinus,

    Comment,
    DocBlockComment,
    DocComment,
    ModuleDocComment,
    EOF,
}

/// Lexed source token with text and byte span.
///
/// Inputs:
/// - Token kind, original token text, and source byte offsets.
///
/// Output:
/// - Token value consumed by parser cursor logic.
///
/// Transformation:
/// - Keeps category and original text together so parser diagnostics and raw
///   block preservation can refer back to exact source content.
#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub text: String,
    pub start: usize,
    pub end: usize,
}

impl Token {
    /// Builds a token.
    ///
    /// Inputs:
    /// - `kind`: token category.
    /// - `text`: source text for the token.
    /// - `start` and `end`: byte span in the original source.
    ///
    /// Output:
    /// - Token value with owned text.
    ///
    /// Transformation:
    /// - Converts text into an owned string while preserving caller-provided
    ///   byte offsets.
    pub fn new(kind: TokenKind, text: impl Into<String>, start: usize, end: usize) -> Self {
        Self {
            kind,
            text: text.into(),
            start,
            end,
        }
    }

    /// Returns this token's source span.
    ///
    /// Inputs:
    /// - `self`: token with start/end byte offsets.
    ///
    /// Output:
    /// - `Span` covering the token in source text.
    ///
    /// Transformation:
    /// - Converts the token's inline offsets into the shared compiler span
    ///   type.
    pub fn span(&self) -> crate::span::Span {
        crate::span::Span::new(self.start, self.end)
    }
}
