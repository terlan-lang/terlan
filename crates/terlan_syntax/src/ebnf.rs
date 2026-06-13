use crate::span::Span;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EbnfError {
    pub message: String,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EbnfSourceSpan {
    pub start: usize,
    pub end: usize,
}

impl From<Span> for EbnfSourceSpan {
    fn from(span: Span) -> Self {
        Self {
            start: span.start,
            end: span.end,
        }
    }
}

impl From<EbnfSourceSpan> for Span {
    fn from(span: EbnfSourceSpan) -> Self {
        Span::new(span.start, span.end)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EbnfGrammarContract {
    pub format_version: u32,
    pub entry_rule: Option<String>,
    pub rules: Vec<EbnfGrammarRule>,
}

impl EbnfGrammarContract {
    pub fn rule(&self, name: &str) -> Option<&EbnfGrammarRule> {
        self.rules.iter().find(|rule| rule.name == name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EbnfGrammarRule {
    pub id: String,
    pub name: String,
    pub span: EbnfSourceSpan,
    pub name_span: EbnfSourceSpan,
    pub expr: EbnfGrammarExpr,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EbnfGrammarExpr {
    pub id: String,
    pub span: EbnfSourceSpan,
    #[serde(flatten)]
    pub kind: EbnfGrammarExprKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EbnfGrammarExprKind {
    Nonterminal { name: String },
    Terminal { value: String },
    CharacterClass { chars: String },
    Special { text: String },
    Sequence { items: Vec<EbnfGrammarExpr> },
    Alternation { items: Vec<EbnfGrammarExpr> },
    Optional { expr: Box<EbnfGrammarExpr> },
    Repetition { expr: Box<EbnfGrammarExpr> },
    Group { expr: Box<EbnfGrammarExpr> },
    OneOrMore { expr: Box<EbnfGrammarExpr> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EbnfCompileError {
    Parse(String, Span),
    Serialize(String),
}

pub type EbnfCompileResult<T> = Result<T, EbnfCompileError>;

pub fn compile_ebnf(input: &str) -> EbnfCompileResult<EbnfGrammarContract> {
    parse_ebnf(input)
}

fn parse_ebnf_ast(input: &str) -> EbnfParseResult<EbnfGrammarContract> {
    let tokens = EbnfLexer::new(input).lex()?;
    EbnfParser::new(tokens).parse_grammar()
}

pub fn parse_ebnf(input: &str) -> EbnfCompileResult<EbnfGrammarContract> {
    parse_ebnf_ast(input).map_err(|error| EbnfCompileError::Parse(error.message, error.span))
}

pub fn compile_ebnf_contract(input: &str) -> EbnfCompileResult<EbnfGrammarContract> {
    compile_ebnf(input)
}

pub fn compile_ebnf_to_json(input: &str) -> EbnfCompileResult<String> {
    let output = compile_ebnf(input)?;
    serde_json::to_string_pretty(&output)
        .map_err(|error| EbnfCompileError::Serialize(error.to_string()))
}

pub fn compile_ebnf_contract_to_json(input: &str) -> EbnfCompileResult<String> {
    compile_ebnf_to_json(input)
}

pub type EbnfParseResult<T> = Result<T, EbnfError>;

#[derive(Debug, Clone, PartialEq, Eq)]
struct EbnfToken {
    kind: EbnfTokenKind,
    span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum EbnfTokenKind {
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

struct EbnfLexer<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> EbnfLexer<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, pos: 0 }
    }

    fn lex(mut self) -> EbnfParseResult<Vec<EbnfToken>> {
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

    fn lex_identifier(&mut self) -> EbnfTokenKind {
        let start = self.pos;
        self.bump_char();
        while matches!(self.current_char(), Some(ch) if is_ebnf_ident_continue(ch)) {
            self.bump_char();
        }
        EbnfTokenKind::Identifier(self.input[start..self.pos].to_string())
    }

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

    fn starts_with(&self, prefix: &str) -> bool {
        self.input[self.pos..].starts_with(prefix)
    }

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

    fn current_char(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }

    fn bump_char(&mut self) -> Option<char> {
        let ch = self.current_char()?;
        self.pos += ch.len_utf8();
        Some(ch)
    }

    fn is_eof(&self) -> bool {
        self.pos >= self.input.len()
    }
}

struct EbnfParser {
    tokens: Vec<EbnfToken>,
    pos: usize,
}

impl EbnfParser {
    fn new(tokens: Vec<EbnfToken>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn parse_grammar(&mut self) -> EbnfParseResult<EbnfGrammarContract> {
        let mut rules = Vec::new();
        while !self.check_kind(&EbnfTokenKind::Eof) {
            rules.push(self.parse_rule_contract()?);
        }

        let entry_rule = rules.first().map(|rule| rule.name.clone());
        Ok(EbnfGrammarContract {
            format_version: 1,
            entry_rule,
            rules,
        })
    }

    fn parse_rule_contract(&mut self) -> EbnfParseResult<EbnfGrammarRule> {
        let name_token = self.current().clone();
        let name = match &name_token.kind {
            EbnfTokenKind::Identifier(name) => {
                let name = name.clone();
                self.bump();
                name
            }
            _ => return self.error_current("expected EBNF rule name"),
        };
        self.expect_kind(
            &EbnfTokenKind::Define,
            "expected '::=' after EBNF rule name",
        )?;
        let mut expr = self.parse_expr_contract()?;
        let dot = self.expect_kind(&EbnfTokenKind::Dot, "expected '.' after EBNF rule")?;
        let rule_id = format!("rule:{name}");
        assign_expr_ids(&mut expr, format!("{rule_id}/expr"));
        Ok(EbnfGrammarRule {
            id: rule_id,
            name,
            span: span_union(name_token.span, dot.span).into(),
            name_span: name_token.span.into(),
            expr,
        })
    }

    fn parse_expr_contract(&mut self) -> EbnfParseResult<EbnfGrammarExpr> {
        let mut alternatives = vec![self.parse_sequence_contract()?];
        while self.check_kind(&EbnfTokenKind::Pipe) {
            self.bump();
            alternatives.push(self.parse_sequence_contract()?);
        }

        if alternatives.len() == 1 {
            Ok(alternatives.remove(0))
        } else {
            let span = span_from_exprs(&alternatives);
            Ok(EbnfGrammarExpr {
                id: String::new(),
                span: span.into(),
                kind: EbnfGrammarExprKind::Alternation {
                    items: alternatives,
                },
            })
        }
    }

    fn parse_sequence_contract(&mut self) -> EbnfParseResult<EbnfGrammarExpr> {
        let mut items = Vec::new();
        while !self.is_sequence_end() {
            items.push(self.parse_term_contract()?);
        }

        if items.is_empty() {
            let current_span = self.current().span;
            Ok(EbnfGrammarExpr {
                id: String::new(),
                span: current_span.into(),
                kind: EbnfGrammarExprKind::Sequence { items },
            })
        } else if items.len() == 1 {
            Ok(items.remove(0))
        } else {
            let span = span_from_exprs(&items);
            Ok(EbnfGrammarExpr {
                id: String::new(),
                span: span.into(),
                kind: EbnfGrammarExprKind::Sequence { items },
            })
        }
    }

    fn parse_term_contract(&mut self) -> EbnfParseResult<EbnfGrammarExpr> {
        let mut expr = match &self.current().kind {
            EbnfTokenKind::Identifier(name) => {
                let span = self.current().span;
                let name = name.clone();
                self.bump();
                EbnfGrammarExpr {
                    id: String::new(),
                    span: span.into(),
                    kind: EbnfGrammarExprKind::Nonterminal { name },
                }
            }
            EbnfTokenKind::Terminal(value) => {
                let span = self.current().span;
                let value = value.clone();
                self.bump();
                EbnfGrammarExpr {
                    id: String::new(),
                    span: span.into(),
                    kind: EbnfGrammarExprKind::Terminal { value },
                }
            }
            EbnfTokenKind::CharacterClass(value) => {
                let span = self.current().span;
                let value = value.clone();
                self.bump();
                EbnfGrammarExpr {
                    id: String::new(),
                    span: span.into(),
                    kind: EbnfGrammarExprKind::CharacterClass { chars: value },
                }
            }
            EbnfTokenKind::Special(value) => {
                let span = self.current().span;
                let value = value.clone();
                self.bump();
                EbnfGrammarExpr {
                    id: String::new(),
                    span: span.into(),
                    kind: EbnfGrammarExprKind::Special { text: value },
                }
            }
            EbnfTokenKind::LBrace => {
                let start = self.bump();
                let inner = self.parse_expr_contract()?;
                let end =
                    self.expect_kind(&EbnfTokenKind::RBrace, "expected '}' after EBNF repetition")?;
                EbnfGrammarExpr {
                    id: String::new(),
                    span: span_union(start.span, end.span).into(),
                    kind: EbnfGrammarExprKind::Repetition {
                        expr: Box::new(inner),
                    },
                }
            }
            EbnfTokenKind::LBracket => {
                let start = self.bump();
                let inner = self.parse_expr_contract()?;
                let end =
                    self.expect_kind(&EbnfTokenKind::RBracket, "expected ']' after EBNF optional")?;
                EbnfGrammarExpr {
                    id: String::new(),
                    span: span_union(start.span, end.span).into(),
                    kind: EbnfGrammarExprKind::Optional {
                        expr: Box::new(inner),
                    },
                }
            }
            EbnfTokenKind::LParen => {
                let start = self.bump();
                let inner = self.parse_expr_contract()?;
                let end =
                    self.expect_kind(&EbnfTokenKind::RParen, "expected ')' after EBNF group")?;
                EbnfGrammarExpr {
                    id: String::new(),
                    span: span_union(start.span, end.span).into(),
                    kind: EbnfGrammarExprKind::Group {
                        expr: Box::new(inner),
                    },
                }
            }
            _ => return self.error_current("expected EBNF expression term"),
        };

        loop {
            expr = match self.current().kind {
                EbnfTokenKind::Star => {
                    let star = self.bump();
                    let span = span_union(expr.span.into(), star.span);
                    EbnfGrammarExpr {
                        id: String::new(),
                        span: span.into(),
                        kind: EbnfGrammarExprKind::Repetition {
                            expr: Box::new(expr),
                        },
                    }
                }
                EbnfTokenKind::Plus => {
                    let plus = self.bump();
                    let span = span_union(expr.span.into(), plus.span);
                    EbnfGrammarExpr {
                        id: String::new(),
                        span: span.into(),
                        kind: EbnfGrammarExprKind::OneOrMore {
                            expr: Box::new(expr),
                        },
                    }
                }
                _ => return Ok(expr),
            };
        }
    }

    fn is_sequence_end(&self) -> bool {
        matches!(
            self.current().kind,
            EbnfTokenKind::Pipe
                | EbnfTokenKind::Dot
                | EbnfTokenKind::RBrace
                | EbnfTokenKind::RBracket
                | EbnfTokenKind::RParen
                | EbnfTokenKind::Eof
        )
    }

    fn check_kind(&self, expected: &EbnfTokenKind) -> bool {
        std::mem::discriminant(&self.current().kind) == std::mem::discriminant(expected)
    }

    fn expect_kind(
        &mut self,
        expected: &EbnfTokenKind,
        message: &str,
    ) -> EbnfParseResult<EbnfToken> {
        if self.check_kind(expected) {
            Ok(self.bump())
        } else {
            self.error_current(message)
        }
    }

    fn error_current<T>(&self, message: &str) -> EbnfParseResult<T> {
        Err(EbnfError {
            message: message.to_string(),
            span: self.current().span,
        })
    }

    fn current(&self) -> &EbnfToken {
        &self.tokens[self.pos]
    }

    fn bump(&mut self) -> EbnfToken {
        let token = self.current().clone();
        if !matches!(token.kind, EbnfTokenKind::Eof) {
            self.pos += 1;
        }
        token
    }
}

fn is_ebnf_ident_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

fn is_ebnf_ident_continue(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

fn span_union(left: Span, right: Span) -> Span {
    Span::new(left.start.min(right.start), left.end.max(right.end))
}

fn span_from_exprs(exprs: &[EbnfGrammarExpr]) -> Span {
    let Some(first) = exprs.first() else {
        return Span::new(0, 0);
    };
    exprs.iter().skip(1).fold(first.span.into(), |span, expr| {
        span_union(span, expr.span.into())
    })
}

fn assign_expr_ids(expr: &mut EbnfGrammarExpr, id: String) {
    expr.id = id.clone();
    match &mut expr.kind {
        EbnfGrammarExprKind::Sequence { items } => {
            for (index, item) in items.iter_mut().enumerate() {
                assign_expr_ids(item, format!("{id}/seq:{index}"));
            }
        }
        EbnfGrammarExprKind::Alternation { items } => {
            for (index, item) in items.iter_mut().enumerate() {
                assign_expr_ids(item, format!("{id}/alt:{index}"));
            }
        }
        EbnfGrammarExprKind::Optional { expr } => {
            assign_expr_ids(expr, format!("{id}/optional"));
        }
        EbnfGrammarExprKind::Repetition { expr } => {
            assign_expr_ids(expr, format!("{id}/repetition"));
        }
        EbnfGrammarExprKind::Group { expr } => {
            assign_expr_ids(expr, format!("{id}/group"));
        }
        EbnfGrammarExprKind::OneOrMore { expr } => {
            assign_expr_ids(expr, format!("{id}/one_or_more"));
        }
        EbnfGrammarExprKind::Nonterminal { .. }
        | EbnfGrammarExprKind::Terminal { .. }
        | EbnfGrammarExprKind::CharacterClass { .. }
        | EbnfGrammarExprKind::Special { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[test]
    fn parses_simple_rules() {
        let grammar = parse_ebnf_ast(
            r#"
            (* comments are skipped *)
            Program ::= { Declaration } .
            Declaration ::= "module" Identifier "." | RawDecl "." .
            Identifier ::= [ "@" ] Letter+ .
            "#,
        )
        .expect("parse ebnf");

        assert_eq!(grammar.rules.len(), 3);
        assert!(grammar.rule("Program").is_some());
        assert!(matches!(
            grammar.rule("Declaration").unwrap().expr.kind,
            EbnfGrammarExprKind::Alternation { .. }
        ));
    }

    #[test]
    fn parses_canonical_terlan_ebnf() {
        let grammar = parse_ebnf_ast(include_str!(
            "../../../docs/grammar/TERLAN_SYNTAX_SPEC.ebnf"
        ))
        .expect("parse canonical Terlan EBNF");

        assert!(grammar.rule("SyntaxSpec").is_some());
        assert!(grammar.rule("Declaration").is_some());
        assert!(grammar.rule("Expr").is_some());
        assert!(grammar.rule("StringChar").is_some());
        assert!(matches!(
            grammar.rule("LowerIdent").unwrap().expr.kind,
            EbnfGrammarExprKind::Sequence { .. }
        ));
        assert!(grammar.rules.len() > 100);
    }

    #[test]
    fn parse_ebnf_returns_grammar_contract() {
        let output = parse_ebnf("Program ::= Symbol .\nSymbol ::= \"a\" .").expect("compile ebnf");

        assert_eq!(output.format_version, 1);
        assert_eq!(output.entry_rule, Some("Program".to_string()));
        assert_eq!(output.rules.len(), 2);
    }

    #[test]
    fn compiles_ebnf_to_grammar_contract() {
        let output =
            compile_ebnf("Program ::= Symbol .\nSymbol ::= \"a\" .").expect("compile ebnf");

        assert_eq!(output.format_version, 1);
        assert_eq!(output.entry_rule, Some("Program".to_string()));
        assert_eq!(output.rules.len(), 2);
        assert_eq!(output.rules[0].name, "Program");
        assert_eq!(output.rules[1].name, "Symbol");
    }

    #[test]
    fn compiles_ebnf_to_spanned_contract() {
        let output = compile_ebnf_contract("Program ::= Symbol .\nSymbol ::= \"a\" .")
            .expect("compile ebnf");

        assert_eq!(output.format_version, 1);
        assert_eq!(output.entry_rule, Some("Program".to_string()));
        assert_eq!(output.rules.len(), 2);
        let program = output.rule("Program").expect("Program rule");
        assert_eq!(program.id, "rule:Program");
        assert_eq!(program.expr.id, "rule:Program/expr");
        assert!(program.span.end > program.span.start);
        assert!(matches!(
            program.expr.kind,
            EbnfGrammarExprKind::Nonterminal { .. }
        ));
    }

    #[test]
    fn canonical_terlan_ebnf_contract_matches_expected_summary() {
        let output = compile_ebnf_contract(include_str!(
            "../../../docs/grammar/TERLAN_SYNTAX_SPEC.ebnf"
        ))
        .expect("compile canonical Terlan EBNF contract");

        let actual = ContractSummary::from_contract(&output);
        let expected = serde_json::from_str::<ContractSummary>(include_str!(
            "../../../docs/grammar/fixtures/contract/terlan_syntax_spec_contract_summary.json"
        ))
        .expect("parse expected contract summary");

        assert_eq!(actual, expected);
    }

    #[test]
    fn compiles_ebnf_to_json() {
        let json = compile_ebnf_to_json("Program ::= Symbol .\nSymbol ::= \"a\" .")
            .expect("compile ebnf to json");

        let value = serde_json::from_str::<serde_json::Value>(&json).expect("json output");
        assert_eq!(value["entry_rule"], "Program");
        assert_eq!(value["rules"].as_array().map(|rules| rules.len()), Some(2));
    }

    #[test]
    fn reports_unterminated_comment() {
        let error = parse_ebnf("Rule ::= Atom . (*").expect_err("unterminated comment");

        let EbnfCompileError::Parse(message, _) = error else {
            panic!("expected parse error");
        };
        assert_eq!(message, "unterminated EBNF comment");
    }

    #[test]
    fn reports_missing_rule_dot() {
        let error = parse_ebnf("Rule ::= Atom").expect_err("missing dot");

        let EbnfCompileError::Parse(message, _) = error else {
            panic!("expected parse error");
        };
        assert_eq!(message, "expected '.' after EBNF rule");
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct ContractSummary {
        format_version: u32,
        entry_rule: String,
        rule_count: usize,
        key_rules: Vec<RuleSummary>,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct RuleSummary {
        name: String,
        id: String,
        expr_id: String,
        kind: String,
    }

    impl ContractSummary {
        fn from_contract(contract: &EbnfGrammarContract) -> Self {
            let key_rules = [
                "SyntaxSpec",
                "Declaration",
                "DeclarationCore",
                "Annotation",
                "Expr",
                "SendExpr",
                "PipeExpr",
                "OrExpr",
                "AndExpr",
                "PostfixExpr",
                "PrimaryExpr",
                "Pattern",
                "ListPattern",
                "CallExpr",
                "ScopedCallExpr",
                "RawMacroExpr",
                "ConfigDecl",
                "MetadataBlock",
                "TypeRef",
            ]
            .into_iter()
            .map(|name| {
                let rule = contract
                    .rule(name)
                    .unwrap_or_else(|| panic!("missing rule {name}"));
                RuleSummary {
                    name: rule.name.clone(),
                    id: rule.id.clone(),
                    expr_id: rule.expr.id.clone(),
                    kind: expr_kind_name(&rule.expr).to_string(),
                }
            })
            .collect();

            Self {
                format_version: contract.format_version,
                entry_rule: contract
                    .entry_rule
                    .clone()
                    .expect("canonical grammar has entry rule"),
                rule_count: contract.rules.len(),
                key_rules,
            }
        }
    }

    fn expr_kind_name(expr: &EbnfGrammarExpr) -> &'static str {
        match &expr.kind {
            EbnfGrammarExprKind::Nonterminal { .. } => "nonterminal",
            EbnfGrammarExprKind::Terminal { .. } => "terminal",
            EbnfGrammarExprKind::CharacterClass { .. } => "character_class",
            EbnfGrammarExprKind::Special { .. } => "special",
            EbnfGrammarExprKind::Sequence { .. } => "sequence",
            EbnfGrammarExprKind::Alternation { .. } => "alternation",
            EbnfGrammarExprKind::Optional { .. } => "optional",
            EbnfGrammarExprKind::Repetition { .. } => "repetition",
            EbnfGrammarExprKind::Group { .. } => "group",
            EbnfGrammarExprKind::OneOrMore { .. } => "one_or_more",
        }
    }
}
