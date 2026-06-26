use crate::ebnf_lexer::{EbnfLexer, EbnfToken, EbnfTokenKind};
use crate::span::Span;
use serde::{Deserialize, Serialize};

/// EBNF parser diagnostic with source span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EbnfError {
    pub message: String,
    pub span: Span,
}

/// Serializable source span used in EBNF contract artifacts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EbnfSourceSpan {
    pub start: usize,
    pub end: usize,
}

impl From<Span> for EbnfSourceSpan {
    /// Converts the compiler span type into the serializable EBNF span type.
    ///
    /// Inputs:
    /// - `span`: compiler source span.
    ///
    /// Output:
    /// - EBNF source span with the same byte bounds.
    ///
    /// Transformation:
    /// - Copies start and end offsets without normalization.
    fn from(span: Span) -> Self {
        Self {
            start: span.start,
            end: span.end,
        }
    }
}

impl From<EbnfSourceSpan> for Span {
    /// Converts a serializable EBNF span into the compiler span type.
    ///
    /// Inputs:
    /// - `span`: EBNF artifact span.
    ///
    /// Output:
    /// - Compiler source span with the same byte bounds.
    ///
    /// Transformation:
    /// - Copies start and end offsets into `Span::new`.
    fn from(span: EbnfSourceSpan) -> Self {
        Span::new(span.start, span.end)
    }
}

/// Compiled EBNF grammar contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EbnfGrammarContract {
    pub format_version: u32,
    pub entry_rule: Option<String>,
    pub rules: Vec<EbnfGrammarRule>,
}

impl EbnfGrammarContract {
    /// Looks up a rule by name.
    ///
    /// Inputs:
    /// - `name`: EBNF rule name.
    ///
    /// Output:
    /// - Matching grammar rule when present.
    ///
    /// Transformation:
    /// - Performs a linear lookup over the compiled rule list.
    pub fn rule(&self, name: &str) -> Option<&EbnfGrammarRule> {
        self.rules.iter().find(|rule| rule.name == name)
    }
}

/// One named EBNF grammar rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EbnfGrammarRule {
    pub id: String,
    pub name: String,
    pub span: EbnfSourceSpan,
    pub name_span: EbnfSourceSpan,
    pub expr: EbnfGrammarExpr,
}

/// One EBNF expression node with a stable contract id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EbnfGrammarExpr {
    pub id: String,
    pub span: EbnfSourceSpan,
    #[serde(flatten)]
    pub kind: EbnfGrammarExprKind,
}

/// Shape of one EBNF expression node.
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

/// Error emitted while compiling EBNF source into a contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EbnfCompileError {
    Parse(String, Span),
    Serialize(String),
}

/// Result type returned by public EBNF compilation entry points.
pub type EbnfCompileResult<T> = Result<T, EbnfCompileError>;

/// Compiles EBNF source into a grammar contract.
///
/// Inputs:
/// - `input`: full EBNF source text.
///
/// Output:
/// - Grammar contract or compile diagnostic.
///
/// Transformation:
/// - Delegates to the EBNF parser and preserves the parsed contract shape.
pub fn compile_ebnf(input: &str) -> EbnfCompileResult<EbnfGrammarContract> {
    parse_ebnf(input)
}

/// Parses EBNF source into the internal grammar contract before public wrapping.
///
/// Inputs:
/// - `input`: full EBNF source text.
///
/// Output:
/// - Grammar contract or low-level parse diagnostic.
///
/// Transformation:
/// - Lexes EBNF tokens and consumes them with the recursive-descent EBNF parser.
fn parse_ebnf_ast(input: &str) -> EbnfParseResult<EbnfGrammarContract> {
    let tokens = EbnfLexer::new(input).lex()?;
    EbnfParser::new(tokens).parse_grammar()
}

/// Parses EBNF source into a public compile result.
///
/// Inputs:
/// - `input`: full EBNF source text.
///
/// Output:
/// - Grammar contract or public compile diagnostic.
///
/// Transformation:
/// - Converts low-level parser errors into `EbnfCompileError::Parse`.
pub fn parse_ebnf(input: &str) -> EbnfCompileResult<EbnfGrammarContract> {
    parse_ebnf_ast(input).map_err(|error| EbnfCompileError::Parse(error.message, error.span))
}

/// Compiles EBNF source into the canonical contract artifact model.
///
/// Inputs:
/// - `input`: full EBNF source text.
///
/// Output:
/// - Grammar contract or compile diagnostic.
///
/// Transformation:
/// - Alias for `compile_ebnf` retained for callers that name the contract path.
pub fn compile_ebnf_contract(input: &str) -> EbnfCompileResult<EbnfGrammarContract> {
    compile_ebnf(input)
}

/// Compiles EBNF source into pretty JSON.
///
/// Inputs:
/// - `input`: full EBNF source text.
///
/// Output:
/// - Pretty-printed JSON contract or compile/serialization diagnostic.
///
/// Transformation:
/// - Compiles the grammar contract and serializes it with `serde_json`.
pub fn compile_ebnf_to_json(input: &str) -> EbnfCompileResult<String> {
    let output = compile_ebnf(input)?;
    serde_json::to_string_pretty(&output)
        .map_err(|error| EbnfCompileError::Serialize(error.to_string()))
}

/// Compiles EBNF source into pretty JSON through the contract-named entry point.
///
/// Inputs:
/// - `input`: full EBNF source text.
///
/// Output:
/// - Pretty-printed JSON contract or compile/serialization diagnostic.
///
/// Transformation:
/// - Alias for `compile_ebnf_to_json`.
pub fn compile_ebnf_contract_to_json(input: &str) -> EbnfCompileResult<String> {
    compile_ebnf_to_json(input)
}

/// Result type returned by the internal EBNF lexer and parser.
pub type EbnfParseResult<T> = Result<T, EbnfError>;

/// Recursive-descent parser for EBNF token streams.
struct EbnfParser {
    tokens: Vec<EbnfToken>,
    pos: usize,
}

impl EbnfParser {
    /// Creates an EBNF parser.
    ///
    /// Inputs:
    /// - `tokens`: EBNF token stream terminated by EOF.
    ///
    /// Output:
    /// - Parser positioned at the first token.
    ///
    /// Transformation:
    /// - Stores tokens without modification and initializes the cursor.
    fn new(tokens: Vec<EbnfToken>) -> Self {
        Self { tokens, pos: 0 }
    }

    /// Parses a full EBNF grammar contract.
    ///
    /// Inputs:
    /// - Parser cursor at the first rule.
    ///
    /// Output:
    /// - Grammar contract containing all parsed rules.
    ///
    /// Transformation:
    /// - Parses rules until EOF and uses the first rule as the entry rule.
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

    /// Parses one named EBNF rule.
    ///
    /// Inputs:
    /// - Parser cursor at a rule name.
    ///
    /// Output:
    /// - Grammar rule with stable ids and source spans.
    ///
    /// Transformation:
    /// - Consumes `Name ::= Expr .` and assigns nested expression ids.
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

    /// Parses an alternation expression.
    ///
    /// Inputs:
    /// - Parser cursor at the start of an EBNF expression.
    ///
    /// Output:
    /// - Single sequence expression or alternation expression.
    ///
    /// Transformation:
    /// - Parses one or more sequences separated by top-level `|`.
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

    /// Parses a sequence expression.
    ///
    /// Inputs:
    /// - Parser cursor at the start of a sequence.
    ///
    /// Output:
    /// - Single term expression, sequence expression, or empty sequence.
    ///
    /// Transformation:
    /// - Parses terms until a sequence-ending delimiter is reached.
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

    /// Parses one EBNF term expression.
    ///
    /// Inputs:
    /// - Parser cursor at a terminal, nonterminal, class, special, or group.
    ///
    /// Output:
    /// - Parsed term expression including postfix repetition markers.
    ///
    /// Transformation:
    /// - Consumes the primary term and folds trailing `*` or `+` into wrapper
    ///   expression nodes.
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

    /// Reports whether the current token ends an EBNF sequence.
    ///
    /// Inputs:
    /// - Parser cursor at a token inside an expression.
    ///
    /// Output:
    /// - `true` for alternation, rule, group, optional, repetition, or EOF
    ///   terminators.
    ///
    /// Transformation:
    /// - Classifies the current token without advancing.
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

    /// Checks the current EBNF token kind by discriminant.
    ///
    /// Inputs:
    /// - `expected`: token kind variant to compare against.
    ///
    /// Output:
    /// - `true` when the current token has the same variant.
    ///
    /// Transformation:
    /// - Ignores payload values so callers can match variant shape only.
    fn check_kind(&self, expected: &EbnfTokenKind) -> bool {
        std::mem::discriminant(&self.current().kind) == std::mem::discriminant(expected)
    }

    /// Consumes an expected EBNF token kind.
    ///
    /// Inputs:
    /// - `expected`: token kind variant required at the cursor.
    /// - `message`: diagnostic text for mismatches.
    ///
    /// Output:
    /// - Consumed token, or parser diagnostic at the cursor.
    ///
    /// Transformation:
    /// - Checks by token discriminant and advances only on match.
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

    /// Produces a parser diagnostic at the current EBNF token.
    ///
    /// Inputs:
    /// - `message`: diagnostic text.
    ///
    /// Output:
    /// - Always returns `Err(EbnfError)`.
    ///
    /// Transformation:
    /// - Anchors the diagnostic to the current token span.
    fn error_current<T>(&self, message: &str) -> EbnfParseResult<T> {
        Err(EbnfError {
            message: message.to_string(),
            span: self.current().span,
        })
    }

    /// Returns the current EBNF token.
    ///
    /// Inputs:
    /// - Current parser cursor.
    ///
    /// Output:
    /// - Reference to the current token.
    ///
    /// Transformation:
    /// - Indexes the token stream without advancing.
    fn current(&self) -> &EbnfToken {
        &self.tokens[self.pos]
    }

    /// Advances over the current EBNF token.
    ///
    /// Inputs:
    /// - Current parser cursor.
    ///
    /// Output:
    /// - Token that was current before advancing.
    ///
    /// Transformation:
    /// - Clones the token and advances unless the token is EOF.
    fn bump(&mut self) -> EbnfToken {
        let token = self.current().clone();
        if !matches!(token.kind, EbnfTokenKind::Eof) {
            self.pos += 1;
        }
        token
    }
}

/// Combines two source spans.
///
/// Inputs:
/// - `left`: first span.
/// - `right`: second span.
///
/// Output:
/// - Span covering both inputs.
///
/// Transformation:
/// - Uses the minimum start and maximum end offsets.
fn span_union(left: Span, right: Span) -> Span {
    Span::new(left.start.min(right.start), left.end.max(right.end))
}

/// Computes a span covering expression nodes.
///
/// Inputs:
/// - `exprs`: expression list.
///
/// Output:
/// - Span covering all expressions, or an empty zero span for an empty list.
///
/// Transformation:
/// - Folds expression spans with `span_union`.
fn span_from_exprs(exprs: &[EbnfGrammarExpr]) -> Span {
    let Some(first) = exprs.first() else {
        return Span::new(0, 0);
    };
    exprs.iter().skip(1).fold(first.span.into(), |span, expr| {
        span_union(span, expr.span.into())
    })
}

/// Assigns stable ids to an EBNF expression tree.
///
/// Inputs:
/// - `expr`: expression tree to mutate.
/// - `id`: id assigned to the current expression node.
///
/// Output:
/// - No return value.
///
/// Transformation:
/// - Recursively assigns child ids using path-like suffixes that encode
///   sequence, alternation, and wrapper positions.
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

    /// Verifies that basic EBNF rules parse into a contract.
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

    /// Verifies that the canonical Terlan grammar parses as EBNF.
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

    /// Verifies the public parse entry point returns a grammar contract.
    #[test]
    fn parse_ebnf_returns_grammar_contract() {
        let output = parse_ebnf("Program ::= Symbol .\nSymbol ::= \"a\" .").expect("compile ebnf");

        assert_eq!(output.format_version, 1);
        assert_eq!(output.entry_rule, Some("Program".to_string()));
        assert_eq!(output.rules.len(), 2);
    }

    /// Verifies the compile entry point returns rule metadata.
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

    /// Verifies the contract entry point assigns rule and expression spans.
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

    /// Verifies the canonical grammar contract summary remains stable.
    #[test]
    fn canonical_terlan_ebnf_contract_matches_golden_summary() {
        let output = compile_ebnf_contract(include_str!(
            "../../../docs/grammar/TERLAN_SYNTAX_SPEC.ebnf"
        ))
        .expect("compile canonical Terlan EBNF contract");

        let actual = ContractSummary::from_contract(&output);
        let expected = serde_json::from_str::<ContractSummary>(include_str!(
            "../../../docs/grammar/fixtures/contract/terlan_syntax_spec_contract_summary.json"
        ))
        .expect("parse golden contract summary");

        assert_eq!(actual, expected);
    }

    /// Verifies EBNF contracts serialize to JSON.
    #[test]
    fn compiles_ebnf_to_json() {
        let json = compile_ebnf_to_json("Program ::= Symbol .\nSymbol ::= \"a\" .")
            .expect("compile ebnf to json");

        let value = serde_json::from_str::<serde_json::Value>(&json).expect("json output");
        assert_eq!(value["entry_rule"], "Program");
        assert_eq!(value["rules"].as_array().map(|rules| rules.len()), Some(2));
    }

    /// Verifies unterminated comments report a specific parse diagnostic.
    #[test]
    fn reports_unterminated_comment() {
        let error = parse_ebnf("Rule ::= Atom . (*").expect_err("unterminated comment");

        let EbnfCompileError::Parse(message, _) = error else {
            panic!("expected parse error");
        };
        assert_eq!(message, "unterminated EBNF comment");
    }

    /// Verifies missing rule terminators report a specific parse diagnostic.
    #[test]
    fn reports_missing_rule_dot() {
        let error = parse_ebnf("Rule ::= Atom").expect_err("missing dot");

        let EbnfCompileError::Parse(message, _) = error else {
            panic!("expected parse error");
        };
        assert_eq!(message, "expected '.' after EBNF rule");
    }

    /// Stable summary fixture for canonical EBNF contract tests.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct ContractSummary {
        format_version: u32,
        entry_rule: String,
        rule_count: usize,
        key_rules: Vec<RuleSummary>,
    }

    /// Stable per-rule summary fixture for canonical EBNF contract tests.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct RuleSummary {
        name: String,
        id: String,
        expr_id: String,
        kind: String,
    }

    impl ContractSummary {
        /// Builds a stable summary from a full grammar contract.
        ///
        /// Inputs:
        /// - `contract`: compiled grammar contract.
        ///
        /// Output:
        /// - Compact summary containing selected key rules.
        ///
        /// Transformation:
        /// - Extracts deterministic metadata for the rules that protect the
        ///   public syntax contract.
        fn from_contract(contract: &EbnfGrammarContract) -> Self {
            let key_rules = [
                "SyntaxSpec",
                "Declaration",
                "DeclarationCore",
                "Annotation",
                "AnnotationBlock",
                "AnnotationItem",
                "AnnotationEntry",
                "AnnotationValue",
                "AnnotationSchemaDecl",
                "Expr",
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

    /// Returns the stable fixture name for an expression kind.
    ///
    /// Inputs:
    /// - `expr`: grammar expression to classify.
    ///
    /// Output:
    /// - Snake-case kind name used in contract summaries.
    ///
    /// Transformation:
    /// - Maps enum variants to their serialized fixture spelling.
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
