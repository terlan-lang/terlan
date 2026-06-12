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
    For,
    Template,
    Where,
    Extends,
    Derives,
    Struct,
    Receives,
    Case,
    Of,
    End,
    Receive,
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
    At,
    Lt,
    Gt,
    LtEq,
    GtEq,
    LtMinus,

    Comment,
    DocComment,
    ModuleDocComment,
    EOF,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub text: String,
    pub start: usize,
    pub end: usize,
}

impl Token {
    pub fn new(kind: TokenKind, text: impl Into<String>, start: usize, end: usize) -> Self {
        Self {
            kind,
            text: text.into(),
            start,
            end,
        }
    }

    pub fn span(&self) -> crate::span::Span {
        crate::span::Span::new(self.start, self.end)
    }
}
