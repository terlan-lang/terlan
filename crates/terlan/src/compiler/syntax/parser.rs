//! Generated-parser adapters for compiler and formatter consumers.
//!
//! LALRPOP owns Terlan recognition. This module preserves the pre-existing
//! entry-point error type while keeping validation, migration, and AST
//! lowering as explicit phases outside grammar semantic actions.

use super::{
    ebnf::EbnfCompileError,
    lalrpop_syntax::{LalrpopSourceIndex, LalrpopSyntaxNode, LalrpopSyntaxNodeKind},
    lexer::lex,
    parse_tree::{Decl, Expr, Module, Pattern},
    span::Span,
    syntax_contract::{
        ensure_canonical_syntax_contract_valid as ensure_syntax_contract_valid, SyntaxContractError,
    },
    token::{Token, TokenKind},
};

#[cfg(test)]
#[path = "parser_adversarial_test.rs"]
mod parser_adversarial_test;
#[cfg(test)]
#[path = "parser_decl_surface_test.rs"]
mod parser_decl_surface_test;
#[cfg(test)]
#[path = "parser_decl_test.rs"]
mod parser_decl_test;
#[cfg(test)]
#[path = "parser_expr_test.rs"]
mod parser_expr_test;
#[cfg(test)]
#[path = "parser_html_test.rs"]
mod parser_html_test;
#[cfg(test)]
#[path = "parser_pattern_test.rs"]
mod parser_pattern_test;
#[cfg(test)]
#[path = "parser_repeated_let_test.rs"]
mod parser_repeated_let_test;
#[cfg(test)]
#[path = "parser_trait_purity_test.rs"]
mod parser_trait_purity_test;
#[cfg(test)]
#[path = "script_source_test.rs"]
mod script_source_test;
#[cfg(test)]
#[path = "parser_type_params_test.rs"]
mod type_params_test;

/// Parser diagnostic with message and source span.
#[derive(Debug)]
pub struct ParseError {
    pub message: String,
    pub span: Span,
}

/// Result type returned by parser operations.
pub type ParseResult<T> = Result<T, ParseError>;
/// Backwards-compatible parser error alias.
pub type ParserError = ParseError;

/// Parses a full canonical Terlan source module through the generated parser.
pub(crate) fn parse_module(input: &str) -> ParseResult<Module> {
    ensure_syntax_contract_valid().map_err(syntax_contract_parse_error)?;
    if let Some(error) = super::lalrpop_diagnostics::module_preflight(input) {
        return Err(boundary_error(error));
    }
    parse_generated_module(input)
}

/// Parses a headerless executable Terlan script.
///
/// The first-line shebang, when present, is replaced with equal-width spaces
/// before lexing so every subsequent diagnostic and debugger byte span remains
/// aligned with the original file. The caller-provided module identity is
/// path-derived and never appears as source-level ceremony.
pub(crate) fn parse_script(input: &str, module_name: &str) -> ParseResult<Module> {
    parse_script_with_assertion_guards(input, module_name, true)
}

/// Parses script source without lowering inline assertions into VM failures.
///
/// Formatting uses this source-preserving view. Compilation uses
/// [`parse_script`] so the hidden failure guard exists only after formatting's
/// source-level boundary.
pub(crate) fn parse_script_for_format(input: &str, module_name: &str) -> ParseResult<Module> {
    parse_script_with_assertion_guards(input, module_name, false)
}

fn parse_script_with_assertion_guards(
    input: &str,
    module_name: &str,
    lower_assertions: bool,
) -> ParseResult<Module> {
    ensure_syntax_contract_valid().map_err(syntax_contract_parse_error)?;
    let normalized = script_source_without_shebang(input);
    let tokens = lex_tokens(&normalized)?;
    ensure_token_nesting_within_limit(&tokens)?;
    if let Some(module_token) = tokens
        .iter()
        .find(|token| !is_script_trivia(token))
        .filter(|token| token.kind == TokenKind::Module)
    {
        return Err(ParseError {
            message: "`.terls` scripts cannot declare `module`; the compiler derives script identity from the source path"
                .to_string(),
            span: module_token.span(),
        });
    }
    let body_start = script_declaration_boundary(&normalized, &tokens);
    if !tokens.iter().any(|token| {
        token.start >= body_start
            && !matches!(
                token.kind,
                TokenKind::Comment
                    | TokenKind::DocComment
                    | TokenKind::DocBlockComment
                    | TokenKind::ModuleDocComment
                    | TokenKind::EOF
            )
    }) {
        return Err(ParseError {
            message: "`.terls` script is missing its top-level executable expression".to_string(),
            span: Span::new(body_start, body_start),
        });
    }
    let (transformed, header_len, entry_len) = transformed_script_source(&normalized, body_start);
    let mut generated =
        super::lalrpop_boundary::parse_lalrpop_module_syntax_unvalidated(&transformed)
            .map_err(|mut error| {
                error.span = remap_script_span(error.span, body_start, header_len, entry_len);
                super::lalrpop_diagnostics::module_diagnostic(&normalized, error)
            })
            .map_err(boundary_error)?;
    generated.module_name = module_name.to_string();
    remap_script_tree(
        &mut generated.root,
        normalized.len(),
        body_start,
        header_len,
        entry_len,
    );
    super::lalrpop_syntax::validate_lalrpop_expression(&generated.root)
        .map_err(|(message, span)| ParseError { message, span })?;
    let mut module = super::lalrpop_lowering::lower_lalrpop_module(&normalized, &generated)
        .map_err(lowering_error)?;
    module.name = module_name.to_string();
    if lower_assertions {
        if let Some(Decl::Function(main)) = module.declarations.iter_mut().find(
            |declaration| matches!(declaration, Decl::Function(function) if function.name == "main"),
        ) {
            if let Some(clause) = main.clauses.first_mut() {
                clause.body = rewrite_script_assertions(clause.body.clone());
            }
        }
    }
    let main_functions = module
        .declarations
        .iter()
        .filter_map(|declaration| match declaration {
            Decl::Function(function) if function.name == "main" => Some(function),
            _ => None,
        })
        .collect::<Vec<_>>();
    if main_functions.len() != 1 {
        return Err(ParseError {
            message: "`.terls` scripts use top-level execution and cannot define `main`; remove the explicit entrypoint"
                .to_string(),
            span: main_functions
                .first()
                .map_or_else(|| Span::new(0, 0), |function| function.span),
        });
    }
    Ok(module)
}

const SCRIPT_PROBE_HEADER: &str = "module script.Probe.\n";
const SCRIPT_SYNTHETIC_HEADER: &str = concat!(
    "module __terlan_script__.\n",
    "import std.test.Test.{assert, assert_equal, assert_false, assert_not_equal, assert_true, fail}.\n",
    "import std.vm.Process.{exit_reason as __script_exit_reason, fail as __script_fail}.\n",
);
const SCRIPT_SYNTHETIC_ENTRY: &str = "pub main(): Unit ->\n";

fn script_declaration_boundary(source: &str, tokens: &[Token]) -> usize {
    let mut depth = 0usize;
    let mut segment_start = tokens
        .iter()
        .find(|token| token.kind != TokenKind::EOF)
        .map_or(0, |token| token.start);
    for (index, token) in tokens.iter().enumerate() {
        match token.kind {
            TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace => {
                depth = depth.saturating_add(1);
            }
            TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace => {
                depth = depth.saturating_sub(1);
            }
            TokenKind::Dot if depth == 0 => {
                let next = tokens[index + 1..]
                    .iter()
                    .find(|candidate| !is_script_trivia(candidate));
                let is_selector_dot = next.is_some_and(|next| next.kind == TokenKind::LBrace);
                let is_tight_dot = next.is_some_and(|next| {
                    token.end == next.start
                        && matches!(
                            next.kind,
                            TokenKind::Atom
                                | TokenKind::Var
                                | TokenKind::Template
                                | TokenKind::Hash
                        )
                });
                if is_selector_dot || is_tight_dot {
                    continue;
                }
                let candidate = &source[segment_start..token.end];
                let probe = format!("{SCRIPT_PROBE_HEADER}{candidate}");
                if super::lalrpop_boundary::parse_lalrpop_module_syntax(&probe).is_ok() {
                    segment_start = token.end;
                }
            }
            _ => {}
        }
    }
    tokens
        .iter()
        .find(|token| token.start >= segment_start && token.kind != TokenKind::EOF)
        .map_or(segment_start, |token| token.start)
}

fn is_script_trivia(token: &Token) -> bool {
    matches!(
        token.kind,
        TokenKind::Comment
            | TokenKind::DocComment
            | TokenKind::DocBlockComment
            | TokenKind::ModuleDocComment
            | TokenKind::EOF
    )
}

fn transformed_script_source(source: &str, body_start: usize) -> (String, usize, usize) {
    let mut transformed = String::with_capacity(
        SCRIPT_SYNTHETIC_HEADER.len() + source.len() + SCRIPT_SYNTHETIC_ENTRY.len(),
    );
    transformed.push_str(SCRIPT_SYNTHETIC_HEADER);
    transformed.push_str(&source[..body_start]);
    transformed.push_str(SCRIPT_SYNTHETIC_ENTRY);
    transformed.push_str(&source[body_start..]);
    (
        transformed,
        SCRIPT_SYNTHETIC_HEADER.len(),
        SCRIPT_SYNTHETIC_ENTRY.len(),
    )
}

fn remap_script_tree(
    node: &mut LalrpopSyntaxNode,
    source_len: usize,
    body_start: usize,
    header_len: usize,
    entry_len: usize,
) {
    if node.kind == LalrpopSyntaxNodeKind::Module {
        for child in &mut node.children {
            remap_script_tree(child, source_len, body_start, header_len, entry_len);
        }
        node.span = Span::new(0, source_len);
        return;
    }
    if node.kind == LalrpopSyntaxNodeKind::ModuleDeclaration {
        node.span = Span::new(0, 0);
        return;
    }
    let synthetic_entry_start = header_len + body_start;
    if node.kind == LalrpopSyntaxNodeKind::FunctionDeclaration
        && node.span.start == synthetic_entry_start
    {
        node.text = node.text.take().map(|text| format!("{text};script:true"));
        if let Some(body) = node.children.last_mut() {
            rewrite_script_implicit_bindings(body);
        }
        if let Some(return_type) = node.children.first_mut() {
            return_type.span = Span::new(body_start, body_start);
        }
        for child in node.children.iter_mut().skip(1) {
            remap_script_tree(child, source_len, body_start, header_len, entry_len);
        }
        node.span = Span::new(body_start, source_len);
        return;
    }
    for child in &mut node.children {
        remap_script_tree(child, source_len, body_start, header_len, entry_len);
    }
    node.span = remap_script_span(node.span, body_start, header_len, entry_len);
}

fn rewrite_script_implicit_bindings(node: &mut LalrpopSyntaxNode) {
    if node.kind != LalrpopSyntaxNodeKind::Sequence {
        if is_script_implicit_binding(node) {
            *node = script_implicit_let(node.clone(), None);
        }
        return;
    }
    let expressions = std::mem::take(&mut node.children);
    *node = build_script_sequence(expressions);
}

fn build_script_sequence(mut expressions: Vec<LalrpopSyntaxNode>) -> LalrpopSyntaxNode {
    let first = expressions.remove(0);
    if expressions.is_empty() {
        return if is_script_implicit_binding(&first) {
            script_implicit_let(first, None)
        } else {
            first
        };
    }
    let rest = build_script_sequence(expressions);
    if is_script_implicit_binding(&first) {
        return script_implicit_let(first, Some(rest));
    }
    let start = first.span.start;
    let end = rest.span.end;
    let mut children = vec![first];
    if rest.kind == LalrpopSyntaxNodeKind::Sequence {
        children.extend(rest.children);
    } else {
        children.push(rest);
    }
    LalrpopSyntaxNode {
        kind: LalrpopSyntaxNodeKind::Sequence,
        span: Span::new(start, end),
        text: None,
        children,
    }
}

fn is_script_implicit_binding(node: &LalrpopSyntaxNode) -> bool {
    node.kind == LalrpopSyntaxNodeKind::IndexAssign
        && node.children.len() == 2
        && node.children[0].kind == LalrpopSyntaxNodeKind::Binding
}

fn script_implicit_let(
    assignment: LalrpopSyntaxNode,
    body: Option<LalrpopSyntaxNode>,
) -> LalrpopSyntaxNode {
    let [binding, value]: [LalrpopSyntaxNode; 2] = assignment
        .children
        .try_into()
        .expect("implicit script assignment has two children");
    let mut pattern = binding.clone();
    pattern.kind = LalrpopSyntaxNodeKind::Pattern;
    let body = body.unwrap_or(binding);
    let span = Span::new(assignment.span.start, body.span.end);
    LalrpopSyntaxNode {
        kind: LalrpopSyntaxNodeKind::Let,
        span,
        text: None,
        children: vec![pattern, value, body],
    }
}

fn rewrite_script_assertions(expression: Expr) -> Expr {
    match expression {
        Expr::Sequence(expressions) => rewrite_script_assertion_sequence(expressions),
        Expr::Let {
            bindings,
            else_clauses,
            body,
        } => Expr::Let {
            bindings,
            else_clauses,
            body: body.map(|body| Box::new(rewrite_script_assertions(*body))),
        },
        expression if is_script_assertion_call(&expression) => {
            guarded_script_assertion(expression, Expr::Atom("true".to_string()))
        }
        expression => expression,
    }
}

fn rewrite_script_assertion_sequence(mut expressions: Vec<Expr>) -> Expr {
    let first = expressions.remove(0);
    if expressions.is_empty() {
        return rewrite_script_assertions(first);
    }
    let rest = rewrite_script_assertion_sequence(expressions);
    if is_script_assertion_call(&first) {
        guarded_script_assertion(first, rest)
    } else {
        match rest {
            Expr::Sequence(mut tail) => {
                tail.insert(0, first);
                Expr::Sequence(tail)
            }
            rest => Expr::Sequence(vec![first, rest]),
        }
    }
}

fn is_script_assertion_call(expression: &Expr) -> bool {
    matches!(
        expression,
        Expr::Call { callee, remote: None, .. }
            if matches!(callee.as_ref(), Expr::Var(name) if matches!(
                name.as_str(),
                "assert" | "assert_equal" | "assert_false" | "assert_not_equal" | "assert_true" | "fail"
            ))
    )
}

fn guarded_script_assertion(assertion: Expr, success: Expr) -> Expr {
    let reason = script_internal_call("__script_exit_reason", vec![Expr::Int(1)]);
    let failure = Expr::Sequence(vec![
        script_internal_call("__script_fail", vec![reason]),
        success.clone(),
    ]);
    Expr::If {
        clauses: vec![
            super::parse_tree::IfClause {
                condition: assertion,
                body: success,
            },
            super::parse_tree::IfClause {
                condition: Expr::Atom("true".to_string()),
                body: failure,
            },
        ],
    }
}

fn script_internal_call(name: &str, args: Vec<Expr>) -> Expr {
    Expr::Call {
        callee: Box::new(Expr::Var(name.to_string())),
        type_args: Vec::new(),
        arg_names: vec![None; args.len()],
        args,
        remote: None,
        is_fun_value: false,
    }
}

fn remap_script_span(span: Span, body_start: usize, header_len: usize, entry_len: usize) -> Span {
    Span::new(
        remap_script_offset(span.start, body_start, header_len, entry_len),
        remap_script_offset(span.end, body_start, header_len, entry_len),
    )
}

fn remap_script_offset(
    offset: usize,
    body_start: usize,
    header_len: usize,
    entry_len: usize,
) -> usize {
    if offset <= header_len {
        0
    } else if offset <= header_len + body_start {
        offset - header_len
    } else if offset <= header_len + body_start + entry_len {
        body_start
    } else {
        offset - header_len - entry_len
    }
}

fn script_source_without_shebang(input: &str) -> String {
    if !input.starts_with("#!") {
        return input.to_string();
    }
    let end = input.find('\n').unwrap_or(input.len());
    let mut normalized = input.to_string();
    normalized.replace_range(..end, &" ".repeat(end));
    normalized
}

/// Parses a generated interface summary.
///
/// Interface files use the same grammar as source modules but permit bodyless
/// callable/type summaries and `export` declarations. Those differences are
/// represented in lowering rather than by retaining a second parser.
pub(crate) fn parse_interface_module(input: &str) -> ParseResult<Module> {
    ensure_syntax_contract_valid().map_err(syntax_contract_parse_error)?;
    let tokens = lex_tokens(input)?;
    ensure_token_nesting_within_limit(&tokens)?;
    let generated = super::lalrpop_boundary::parse_lalrpop_module_syntax(input)
        .map_err(|error| super::lalrpop_diagnostics::module_diagnostic(input, error))
        .map_err(boundary_error)?;
    let mut module = super::lalrpop_lowering::lower_lalrpop_interface_module(input, &generated)
        .map_err(lowering_error)?;
    let source_index = LalrpopSourceIndex::new(input);
    for declaration in &mut module.declarations {
        if let Decl::Type(type_declaration) = declaration {
            let source = source_index.text(
                input,
                type_declaration.span.start,
                type_declaration.span.end,
            );
            if !type_declaration.is_opaque && !source.contains('=') {
                type_declaration.variants.clear();
            }
        }
    }
    Ok(module)
}

/// Parses one standalone Terlan expression through LALRPOP and explicit AST
/// lowering.
pub(crate) fn parse_terlan_expr(raw: &str) -> ParseResult<Expr> {
    super::html_syntax::parse_terlan_expr(raw)
}

/// Parses one standalone Terlan pattern for compiler-owned expansion passes.
pub(crate) fn parse_terlan_pattern(input: &str) -> ParseResult<Pattern> {
    let generated =
        super::lalrpop_boundary::parse_lalrpop_pattern(input).map_err(boundary_error)?;
    super::lalrpop_lowering::lower_lalrpop_pattern(input, &generated.root).map_err(lowering_error)
}

/// Parses source while accepting retired implicit bindings for migration only.
pub(crate) fn parse_module_for_repeated_let_migration(input: &str) -> ParseResult<Module> {
    let migrated = migrated_repeated_let_source(input)?;
    parse_module(&migrated)
}

/// Finds implicit binding offsets after validating the complete migrated
/// source module.
pub(crate) fn repeated_let_migration_offsets(input: &str) -> ParseResult<Vec<usize>> {
    let offsets = implicit_repeated_let_offsets(input)?;
    let migrated = insert_repeated_lets(input, &offsets);
    parse_module(&migrated)?;
    Ok(offsets)
}

fn parse_generated_module(input: &str) -> ParseResult<Module> {
    let tokens = lex_tokens(input)?;
    ensure_token_nesting_within_limit(&tokens)?;
    let generated = super::lalrpop_boundary::parse_lalrpop_module_syntax(input)
        .map_err(|error| super::lalrpop_diagnostics::module_diagnostic(input, error))
        .map_err(boundary_error)?;
    super::lalrpop_lowering::lower_lalrpop_module(input, &generated).map_err(lowering_error)
}

fn migrated_repeated_let_source(input: &str) -> ParseResult<String> {
    let offsets = implicit_repeated_let_offsets(input)?;
    Ok(insert_repeated_lets(input, &offsets))
}

fn insert_repeated_lets(input: &str, offsets: &[usize]) -> String {
    let mut migrated = input.to_string();
    for offset in offsets.iter().rev() {
        migrated.insert_str(*offset, "let ");
    }
    migrated
}

fn implicit_repeated_let_offsets(input: &str) -> ParseResult<Vec<usize>> {
    let tokens = lex_tokens(input)?;
    let significant = tokens
        .iter()
        .filter(|token| !is_comment(token))
        .collect::<Vec<_>>();
    let mut declaration_has_let = false;
    let mut offsets = Vec::new();
    for (index, token) in significant.iter().enumerate() {
        match token.kind {
            TokenKind::Let => declaration_has_let = true,
            TokenKind::Dot => declaration_has_let = false,
            TokenKind::Semicolon if declaration_has_let => {
                let Some(name) = significant.get(index + 1) else {
                    continue;
                };
                let Some(equal) = significant.get(index + 2) else {
                    continue;
                };
                if matches!(name.kind, TokenKind::Atom | TokenKind::Var)
                    && equal.kind == TokenKind::Equals
                {
                    offsets.push(name.start);
                }
            }
            _ => {}
        }
    }
    Ok(offsets)
}

fn is_comment(token: &&Token) -> bool {
    matches!(
        token.kind,
        TokenKind::Comment
            | TokenKind::DocComment
            | TokenKind::DocBlockComment
            | TokenKind::ModuleDocComment
    )
}

fn lex_tokens(input: &str) -> ParseResult<Vec<Token>> {
    lex(input).map_err(|errors| {
        errors.into_iter().next().map_or(
            ParseError {
                message: "lexical failure".to_string(),
                span: Span::new(0, 0),
            },
            |error| ParseError {
                message: error.message,
                span: error.span,
            },
        )
    })
}

const MAX_SYNTACTIC_NESTING: usize = 16;

fn ensure_token_nesting_within_limit(tokens: &[Token]) -> ParseResult<()> {
    let mut depth = 0usize;
    for token in tokens {
        match token.kind {
            TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace => {
                depth += 1;
                if depth > MAX_SYNTACTIC_NESTING {
                    return Err(ParseError {
                        message: format!(
                            "source nesting depth exceeds maximum of {MAX_SYNTACTIC_NESTING}"
                        ),
                        span: token.span(),
                    });
                }
            }
            TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace => {
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }
    }
    Ok(())
}

fn boundary_error(error: super::lalrpop_boundary::LalrpopBoundaryError) -> ParseError {
    ParseError {
        message: error.message,
        span: error.span,
    }
}

fn lowering_error(error: super::lalrpop_lowering::LalrpopLoweringError) -> ParseError {
    ParseError {
        message: error.message,
        span: error.span,
    }
}

pub(super) fn syntax_contract_parse_error(error: SyntaxContractError) -> ParseError {
    let (message, span) = match error {
        SyntaxContractError::Compile(EbnfCompileError::Parse(message, span)) => (
            format!("canonical syntax contract failed to compile: {message}"),
            span,
        ),
        SyntaxContractError::Compile(EbnfCompileError::Serialize(message)) => (
            format!("canonical syntax contract failed to serialize: {message}"),
            Span::new(0, 0),
        ),
        SyntaxContractError::Validation(diagnostics) => diagnostics.into_iter().next().map_or_else(
            || {
                (
                    "canonical syntax contract failed validation".to_string(),
                    Span::new(0, 0),
                )
            },
            |diagnostic| {
                (
                    format!(
                        "canonical syntax contract failed validation: {}",
                        diagnostic.message
                    ),
                    diagnostic.span,
                )
            },
        ),
    };
    ParseError { message, span }
}
