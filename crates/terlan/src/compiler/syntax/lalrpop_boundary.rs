//! Stable token and diagnostic boundary for the generated Terlan parser.

use std::fmt;

use lalrpop_util::ParseError as LalrpopParseError;

use super::{
    lalrpop_projection::{
        config_raw_declaration_end, expression_raw_macro_end, function_body_arrow_indices,
        head_constraint_list_end, html_raw_block_end, if_clause_semicolon_indices,
        is_binary_layout_open, is_function_clause_open, is_generic_call_open,
        is_nominal_keyed_open, is_remote_call_colon, is_trivia, lambda_opening_indices,
        lambda_sequence_semicolon_indices, native_raw_declaration_end,
    },
    lalrpop_syntax::{
        normalize_lalrpop_expression, validate_lalrpop_expression, LalrpopExpressionOutput,
        LalrpopFragmentOutput, LalrpopModuleSyntaxOutput, LalrpopSourceIndex,
    },
    lexer::lex,
    span::Span,
    syntax_contract::cached_canonical_terlan_syntax_contract_identity,
    token::{Token, TokenKind},
};

lalrpop_util::lalrpop_mod!(terlan_lalrpop, "/compiler/syntax/terlan_lalrpop.rs");

/// Schema emitted at the lexer/generated-parser boundary.
pub const LALRPOP_SYNTAX_OUTPUT_SCHEMA: &str = "terlan.lalrpop-syntax-output.v1";

/// A single generated-parser token with its original source span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LalrpopSyntaxTokenOutput {
    /// Canonical terminal name used by the generated grammar.
    pub terminal: &'static str,
    /// Exact byte span inherited from the canonical lexer token.
    pub span: Span,
}

/// Versioned output from the generated parser's first stable boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LalrpopSyntaxOutput {
    /// Versioned output schema.
    pub schema: &'static str,
    /// Content identity of the canonical EBNF contract.
    pub grammar_identity: String,
    /// Span-preserving generated-parser token projection.
    pub tokens: Vec<LalrpopSyntaxTokenOutput>,
}

/// Module identity recognized by the first real generated syntax production.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LalrpopModuleHeaderOutput {
    /// Canonical dotted module name.
    pub module_name: String,
    /// Span covering the complete module declaration.
    pub span: Span,
}

impl LalrpopSyntaxOutput {
    /// Constructs the generated parser's versioned token projection.
    pub fn new(
        grammar_identity: &str,
        tokens: Vec<LalrpopSyntaxTokenOutput>,
    ) -> LalrpopSyntaxOutput {
        LalrpopSyntaxOutput {
            schema: LALRPOP_SYNTAX_OUTPUT_SCHEMA,
            grammar_identity: grammar_identity.to_string(),
            tokens,
        }
    }
}

/// Error type accepted by LALRPOP's external-token iterator.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum LalrpopLexicalError {
    /// The canonical lexer rejected the source before parsing.
    #[default]
    InvalidToken,
}

/// Parser terminal preserving the tight-dot distinction encoded by spans.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LalrpopToken {
    /// An ordinary token category from the canonical lexer.
    Canonical(TokenKind),
    /// A grammar-significant word intentionally contextual in the old parser.
    Contextual(LalrpopContextualKeyword),
    /// A dot immediately adjacent to a following module-path segment.
    TightDot,
    /// A colon separating a remote-call module and function name.
    RemoteCallColon,
    /// An arrow separating a callable return type from its expression body.
    FunctionBodyArrow,
    /// An opening parenthesis whose balanced close is followed by `->`.
    LambdaLParen,
    /// One typed identifier parameter inside a lambda head.
    TypedLambdaPattern,
    /// One callable-head constraint list following square generic parameters.
    HeadConstraintList,
    /// A sequence separator at the owning lambda body's delimiter depth.
    LambdaSequenceSemicolon,
    /// A clause separator at the owning `if` block's delimiter depth.
    IfClauseSemicolon,
    /// A semicolon immediately before the surrounding block's close.
    ClosingSemicolon,
    /// A dot whose next significant token opens an import selector.
    SelectorDot,
    /// `file` in asset-import position.
    AssetFile,
    /// `css` in asset-import position.
    AssetCss,
    /// `markdown` in asset-import position.
    AssetMarkdown,
    /// `not` immediately following an implementation declaration keyword.
    NegativeImplNot,
    /// The standalone `_` placeholder used by patterns and higher-kinded slots.
    Placeholder,
    /// Opening brace of a nominal keyed value or pattern.
    NominalKeyedLBrace,
    /// Opening bracket of `Binary[endian] { ... }` syntax.
    BinaryLayoutLBracket,
    /// One balanced raw HTML block, including its braces.
    HtmlRawBlock,
    /// One complete expression-level raw macro, including its balanced payload.
    ExpressionRawMacro,
    /// Opening bracket of type arguments immediately followed by a call.
    GenericCallLBracket,
    /// Opening bracket of a parameterized type immediately following `as`.
    CastTypeLBracket,
    /// Opening parenthesis of a top-level multi-clause function head.
    FunctionClauseLParen,
    /// One balanced retired `native core module` raw declaration.
    NativeRawDeclaration,
    /// One balanced target configuration declaration.
    ConfigRawDeclaration,
    /// A legacy `:name` atom collapsed before grammar routing.
    LegacyAtom,
}

/// Contextual terminals required by the canonical EBNF grammar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LalrpopContextualKeyword {
    Annotation,
    AppliesTo,
    As,
    Bool,
    Config,
    Css,
    Default,
    Else,
    Exists,
    False,
    File,
    Float,
    From,
    Function,
    Html,
    Int,
    Machine,
    Markdown,
    Method,
    Mut,
    Name,
    Not,
    OpaqueType,
    Quote,
    Repeatable,
    Required,
    Shape,
    Static,
    String,
    Target,
    True,
    Type,
    Unquote,
}

impl fmt::Display for LalrpopToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl fmt::Display for LalrpopLexicalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("invalid Terlan token")
    }
}

/// Result produced by the generated parser boundary.
pub type LalrpopBoundaryResult<T> = Result<T, LalrpopBoundaryError>;

/// Stable generated-parser diagnostic mapped to the compiler span contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LalrpopBoundaryError {
    /// Stable user-facing diagnostic text.
    pub message: String,
    /// Source span associated with the parser failure.
    pub span: Span,
}

/// Projects canonical lexer tokens into LALRPOP's external stream format.
fn spanned_tokens(
    tokens: Vec<Token>,
) -> impl Iterator<Item = Result<(usize, LalrpopToken, usize), LalrpopLexicalError>> {
    projected_tokens(tokens, false, false).into_iter()
}

fn expression_spanned_tokens(
    tokens: Vec<Token>,
) -> impl Iterator<Item = Result<(usize, LalrpopToken, usize), LalrpopLexicalError>> {
    projected_tokens(tokens, true, true).into_iter()
}

fn projected_tokens(
    tokens: Vec<Token>,
    classify_lambda_delimiters: bool,
    omit_trivia: bool,
) -> Vec<Result<(usize, LalrpopToken, usize), LalrpopLexicalError>> {
    let mut projected = Vec::with_capacity(tokens.len().saturating_sub(1));
    let function_body_arrows = if classify_lambda_delimiters {
        function_body_arrow_indices(&tokens)
    } else {
        Vec::new()
    };
    let lambda_openings = if classify_lambda_delimiters {
        lambda_opening_indices(&tokens, &function_body_arrows)
    } else {
        Vec::new()
    };
    let lambda_sequence_semicolons = lambda_sequence_semicolon_indices(&tokens, &lambda_openings);
    let typed_lambda_patterns = typed_lambda_pattern_ranges(&tokens, &lambda_openings);
    let if_clause_semicolons = if classify_lambda_delimiters {
        if_clause_semicolon_indices(&tokens)
    } else {
        Vec::new()
    };
    let mut skip_through = None;
    for (index, token) in tokens.iter().enumerate() {
        if skip_through.is_some_and(|end| index <= end) {
            continue;
        }
        if token.kind == TokenKind::EOF || (omit_trivia && is_trivia(&token.kind)) {
            continue;
        }
        let next_significant = || {
            tokens[index + 1..]
                .iter()
                .find(|candidate| !is_trivia(&candidate.kind))
        };
        let previous_significant = || {
            tokens[..index]
                .iter()
                .rev()
                .find(|candidate| !is_trivia(&candidate.kind))
        };
        let expression_raw_end = classify_lambda_delimiters
            .then(|| expression_raw_macro_end(&tokens, index))
            .flatten()
            .filter(|_| {
                previous_significant().is_none_or(|previous| previous.kind != TokenKind::Hash)
            });
        let raw_block_end = classify_lambda_delimiters
            .then(|| html_raw_block_end(&tokens, index))
            .flatten();
        let native_raw_end = classify_lambda_delimiters
            .then(|| native_raw_declaration_end(&tokens, index))
            .flatten();
        let config_raw_end = classify_lambda_delimiters
            .then(|| config_raw_declaration_end(&tokens, index))
            .flatten();
        let typed_lambda_pattern_end = typed_lambda_patterns
            .iter()
            .find_map(|&(start, end)| (start == index).then_some(end));
        let head_constraint_end = classify_lambda_delimiters
            .then(|| head_constraint_list_end(&tokens, index))
            .flatten();
        let legacy_atom_end = (classify_lambda_delimiters
            && token.kind == TokenKind::Colon
            && tokens.get(index + 1).is_some_and(|next| {
                token.end == next.start && matches!(next.kind, TokenKind::Atom | TokenKind::String)
            })
            && previous_significant().is_none_or(|previous| {
                !matches!(
                    previous.kind,
                    TokenKind::Atom
                        | TokenKind::Var
                        | TokenKind::RParen
                        | TokenKind::RBracket
                        | TokenKind::RBrace
                )
            }))
        .then_some(index + 1);
        let parser_token = if native_raw_end.is_some() {
            LalrpopToken::NativeRawDeclaration
        } else if config_raw_end.is_some() {
            LalrpopToken::ConfigRawDeclaration
        } else if typed_lambda_pattern_end.is_some() {
            LalrpopToken::TypedLambdaPattern
        } else if head_constraint_end.is_some() {
            LalrpopToken::HeadConstraintList
        } else if legacy_atom_end.is_some() {
            LalrpopToken::LegacyAtom
        } else if expression_raw_end.is_some() {
            LalrpopToken::ExpressionRawMacro
        } else if raw_block_end.is_some() {
            LalrpopToken::HtmlRawBlock
        } else if function_body_arrows.binary_search(&index).is_ok() {
            LalrpopToken::FunctionBodyArrow
        } else if classify_lambda_delimiters && is_function_clause_open(&tokens, index) {
            LalrpopToken::FunctionClauseLParen
        } else if lambda_sequence_semicolons.binary_search(&index).is_ok() {
            LalrpopToken::LambdaSequenceSemicolon
        } else if if_clause_semicolons.binary_search(&index).is_ok() {
            LalrpopToken::IfClauseSemicolon
        } else if classify_lambda_delimiters
            && token.kind == TokenKind::Semicolon
            && next_significant().is_some_and(|next| next.kind == TokenKind::RBrace)
        {
            LalrpopToken::ClosingSemicolon
        } else if classify_lambda_delimiters && is_nominal_keyed_open(&tokens, index) {
            LalrpopToken::NominalKeyedLBrace
        } else if classify_lambda_delimiters && is_binary_layout_open(&tokens, index) {
            LalrpopToken::BinaryLayoutLBracket
        } else if classify_lambda_delimiters && is_generic_call_open(&tokens, index) {
            LalrpopToken::GenericCallLBracket
        } else if classify_lambda_delimiters && is_cast_type_open(&tokens, index) {
            LalrpopToken::CastTypeLBracket
        } else if classify_lambda_delimiters && is_remote_call_colon(&tokens, index) {
            LalrpopToken::RemoteCallColon
        } else if classify_lambda_delimiters
            && token.kind == TokenKind::Dot
            && next_significant().is_some_and(|next| next.kind == TokenKind::LBrace)
        {
            LalrpopToken::SelectorDot
        } else if classify_lambda_delimiters
            && token.text == "file"
            && next_significant().is_some_and(|next| next.kind == TokenKind::String)
        {
            LalrpopToken::AssetFile
        } else if classify_lambda_delimiters
            && token.text == "css"
            && next_significant().is_some_and(|next| next.kind == TokenKind::String)
        {
            LalrpopToken::AssetCss
        } else if classify_lambda_delimiters
            && token.text == "markdown"
            && next_significant().is_some_and(|next| next.kind == TokenKind::String)
        {
            LalrpopToken::AssetMarkdown
        } else if classify_lambda_delimiters
            && token.text == "not"
            && previous_significant().is_some_and(|previous| previous.kind == TokenKind::Impl)
        {
            LalrpopToken::NegativeImplNot
        } else if token.text == "_" {
            LalrpopToken::Placeholder
        } else if lambda_openings.binary_search(&index).is_ok() {
            LalrpopToken::LambdaLParen
        } else if token.kind == TokenKind::Dot
            && tokens.get(index + 1).is_some_and(|next| {
                token.end == next.start
                    && matches!(
                        next.kind,
                        TokenKind::Atom | TokenKind::Var | TokenKind::Template | TokenKind::Hash
                    )
            })
        {
            LalrpopToken::TightDot
        } else {
            contextual_keyword(&token.text).map_or_else(
                || LalrpopToken::Canonical(token.kind.clone()),
                LalrpopToken::Contextual,
            )
        };
        let collapsed_end = native_raw_end
            .or(config_raw_end)
            .or(typed_lambda_pattern_end)
            .or(head_constraint_end)
            .or(legacy_atom_end)
            .or(expression_raw_end)
            .or(raw_block_end);
        let end = collapsed_end.map_or(token.end, |close| tokens[close].end);
        skip_through = collapsed_end;
        projected.push(Ok((token.start, parser_token, end)));
    }
    projected
}

fn is_cast_type_open(tokens: &[Token], index: usize) -> bool {
    if tokens
        .get(index)
        .is_none_or(|token| token.kind != TokenKind::LBracket)
    {
        return false;
    }
    let significant = tokens[..index]
        .iter()
        .rev()
        .filter(|token| !is_trivia(&token.kind))
        .take(8)
        .collect::<Vec<_>>();
    significant
        .iter()
        .position(|token| token.text == "as")
        .is_some_and(|position| {
            position > 0
                && significant[1..position].iter().all(|token| {
                    matches!(
                        token.kind,
                        TokenKind::Dot | TokenKind::Atom | TokenKind::Var
                    )
                })
        })
}

fn typed_lambda_pattern_ranges(tokens: &[Token], lambda_openings: &[usize]) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    for &opening in lambda_openings {
        let mut parens = 0usize;
        let mut brackets = 0usize;
        let mut braces = 0usize;
        let mut segment_start = opening + 1;
        for index in opening + 1..tokens.len() {
            match tokens[index].kind {
                TokenKind::LParen => parens += 1,
                TokenKind::RParen if parens == 0 && brackets == 0 && braces == 0 => {
                    record_typed_lambda_pattern(tokens, segment_start, index, &mut ranges);
                    break;
                }
                TokenKind::RParen => parens = parens.saturating_sub(1),
                TokenKind::LBracket => brackets += 1,
                TokenKind::RBracket => brackets = brackets.saturating_sub(1),
                TokenKind::LBrace => braces += 1,
                TokenKind::RBrace => braces = braces.saturating_sub(1),
                TokenKind::Comma if parens == 0 && brackets == 0 && braces == 0 => {
                    record_typed_lambda_pattern(tokens, segment_start, index, &mut ranges);
                    segment_start = index + 1;
                }
                TokenKind::EOF => break,
                _ => {}
            }
        }
    }
    ranges
}

fn record_typed_lambda_pattern(
    tokens: &[Token],
    start: usize,
    end: usize,
    ranges: &mut Vec<(usize, usize)>,
) {
    let significant = (start..end)
        .filter(|&index| !is_trivia(&tokens[index].kind))
        .collect::<Vec<_>>();
    let Some((&first, rest)) = significant.split_first() else {
        return;
    };
    if !matches!(tokens[first].kind, TokenKind::Atom | TokenKind::Var) {
        return;
    }
    let mut parens = 0usize;
    let mut brackets = 0usize;
    let mut braces = 0usize;
    let mut typed = false;
    for &index in rest {
        match tokens[index].kind {
            TokenKind::LParen => parens += 1,
            TokenKind::RParen => parens = parens.saturating_sub(1),
            TokenKind::LBracket => brackets += 1,
            TokenKind::RBracket => brackets = brackets.saturating_sub(1),
            TokenKind::LBrace => braces += 1,
            TokenKind::RBrace => braces = braces.saturating_sub(1),
            TokenKind::Colon if parens == 0 && brackets == 0 && braces == 0 => {
                typed = true;
                break;
            }
            _ => {}
        }
    }
    if !typed {
        return;
    }
    if let Some(&last) = significant.last() {
        ranges.push((first, last));
    }
}

fn contextual_keyword(text: &str) -> Option<LalrpopContextualKeyword> {
    Some(match text {
        "annotation" => LalrpopContextualKeyword::Annotation,
        "applies_to" => LalrpopContextualKeyword::AppliesTo,
        "as" => LalrpopContextualKeyword::As,
        "Bool" => LalrpopContextualKeyword::Bool,
        "config" => LalrpopContextualKeyword::Config,
        "css" => LalrpopContextualKeyword::Css,
        "default" => LalrpopContextualKeyword::Default,
        "else" => LalrpopContextualKeyword::Else,
        "exists" => LalrpopContextualKeyword::Exists,
        "false" => LalrpopContextualKeyword::False,
        "file" => LalrpopContextualKeyword::File,
        "Float" => LalrpopContextualKeyword::Float,
        "from" => LalrpopContextualKeyword::From,
        "function" => LalrpopContextualKeyword::Function,
        "html" => LalrpopContextualKeyword::Html,
        "Int" => LalrpopContextualKeyword::Int,
        "machine" => LalrpopContextualKeyword::Machine,
        "markdown" => LalrpopContextualKeyword::Markdown,
        "method" => LalrpopContextualKeyword::Method,
        "mut" => LalrpopContextualKeyword::Mut,
        "Name" => LalrpopContextualKeyword::Name,
        "not" => LalrpopContextualKeyword::Not,
        "opaque_type" => LalrpopContextualKeyword::OpaqueType,
        "quote" => LalrpopContextualKeyword::Quote,
        "repeatable" => LalrpopContextualKeyword::Repeatable,
        "required" => LalrpopContextualKeyword::Required,
        "shape" => LalrpopContextualKeyword::Shape,
        "static" => LalrpopContextualKeyword::Static,
        "String" => LalrpopContextualKeyword::String,
        "target" => LalrpopContextualKeyword::Target,
        "true" => LalrpopContextualKeyword::True,
        "Type" => LalrpopContextualKeyword::Type,
        "unquote" => LalrpopContextualKeyword::Unquote,
        _ => return None,
    })
}

/// Parses the canonical lexer stream through the generated token boundary.
///
/// This boundary deliberately does not call the recursive-descent parser.
/// Grammar productions replace this token projection incrementally; parity
/// gates must remain red until the generated module parser owns the corpus.
pub fn parse_lalrpop_token_output(input: &str) -> LalrpopBoundaryResult<LalrpopSyntaxOutput> {
    let source_index = LalrpopSourceIndex::new(input);
    let tokens = lex(input).map_err(|errors| {
        let first = errors.into_iter().next();
        LalrpopBoundaryError {
            message: first.as_ref().map_or_else(
                || "lexical failure".to_string(),
                |error| error.message.clone(),
            ),
            span: first.map_or_else(|| Span::new(0, 0), |error| error.span),
        }
    })?;
    ensure_generated_nesting_limit(&tokens)?;
    let identity = cached_canonical_terlan_syntax_contract_identity()
        .map_err(|error| LalrpopBoundaryError {
            message: format!("canonical syntax contract is unavailable: {error:?}"),
            span: Span::new(0, 0),
        })?
        .fingerprint;
    terlan_lalrpop::TokenSyntaxOutputParser::new()
        .parse(
            input,
            &source_index,
            identity.as_str(),
            spanned_tokens(tokens),
        )
        .map_err(lalrpop_error)
}

/// Parses and returns the canonical module header through LALRPOP.
///
/// The body is retained as an unclassified token tail while declaration
/// productions migrate. This function therefore proves header ownership only;
/// it is not the full parser parity entrypoint.
pub fn parse_lalrpop_module_header(
    input: &str,
) -> LalrpopBoundaryResult<LalrpopModuleHeaderOutput> {
    let source_index = LalrpopSourceIndex::new(input);
    let tokens = lex(input).map_err(|errors| {
        let first = errors.into_iter().next();
        LalrpopBoundaryError {
            message: first.as_ref().map_or_else(
                || "lexical failure".to_string(),
                |error| error.message.clone(),
            ),
            span: first.map_or_else(|| Span::new(0, 0), |error| error.span),
        }
    })?;
    let identity = cached_canonical_terlan_syntax_contract_identity()
        .map_err(|error| LalrpopBoundaryError {
            message: format!("canonical syntax contract is unavailable: {error:?}"),
            span: Span::new(0, 0),
        })?
        .fingerprint;
    terlan_lalrpop::ModuleHeaderOutputParser::new()
        .parse(
            input,
            &source_index,
            identity.as_str(),
            spanned_tokens(tokens),
        )
        .map_err(lalrpop_error)
}

/// Parses one complete expression through generated precedence productions.
///
/// The generated grammar owns tree shape and spans. Language restrictions that
/// do not affect parsing, such as the indexed-assignment constraint, run as a
/// separate validation phase after construction.
pub fn parse_lalrpop_expression(input: &str) -> LalrpopBoundaryResult<LalrpopExpressionOutput> {
    let source_index = LalrpopSourceIndex::new(input);
    let tokens = lex(input).map_err(|errors| {
        let first = errors.into_iter().next();
        LalrpopBoundaryError {
            message: first.as_ref().map_or_else(
                || "lexical failure".to_string(),
                |error| error.message.clone(),
            ),
            span: first.map_or_else(|| Span::new(0, 0), |error| error.span),
        }
    })?;
    ensure_generated_nesting_limit(&tokens)?;
    let identity = cached_canonical_terlan_syntax_contract_identity()
        .map_err(|error| LalrpopBoundaryError {
            message: format!("canonical syntax contract is unavailable: {error:?}"),
            span: Span::new(0, 0),
        })?
        .fingerprint;
    let mut output = terlan_lalrpop::ExpressionOutputParser::new()
        .parse(
            input,
            &source_index,
            identity.as_str(),
            expression_spanned_tokens(tokens),
        )
        .map_err(lalrpop_error)?;
    output.root = normalize_lalrpop_expression(output.root);
    validate_lalrpop_expression(&output.root)
        .map_err(|(message, span)| LalrpopBoundaryError { message, span })?;
    Ok(output)
}

/// Parses one complete type expression through generated productions.
pub fn parse_lalrpop_type(input: &str) -> LalrpopBoundaryResult<LalrpopFragmentOutput> {
    let source_index = LalrpopSourceIndex::new(input);
    let (tokens, identity) = lex_fragment(input)?;
    terlan_lalrpop::TypeOutputParser::new()
        .parse(
            input,
            &source_index,
            identity.as_str(),
            spanned_tokens(tokens),
        )
        .map_err(lalrpop_error)
}

/// Parses one complete pattern through generated productions.
pub fn parse_lalrpop_pattern(input: &str) -> LalrpopBoundaryResult<LalrpopFragmentOutput> {
    let source_index = LalrpopSourceIndex::new(input);
    let (tokens, identity) = lex_fragment(input)?;
    terlan_lalrpop::PatternOutputParser::new()
        .parse(
            input,
            &source_index,
            identity.as_str(),
            expression_spanned_tokens(tokens),
        )
        .map_err(lalrpop_error)
}

/// Parses a complete module through generated declaration productions.
pub fn parse_lalrpop_module_syntax(
    input: &str,
) -> LalrpopBoundaryResult<LalrpopModuleSyntaxOutput> {
    let output = parse_lalrpop_module_syntax_unvalidated(input)?;
    validate_lalrpop_expression(&output.root)
        .map_err(|(message, span)| LalrpopBoundaryError { message, span })?;
    Ok(output)
}

/// Parses a module without applying post-grammar expression restrictions.
///
/// Script lowering uses this boundary only long enough to rewrite script-only
/// implicit bindings into ordinary `let` nodes, then runs the same validator.
pub(crate) fn parse_lalrpop_module_syntax_unvalidated(
    input: &str,
) -> LalrpopBoundaryResult<LalrpopModuleSyntaxOutput> {
    let source_index = LalrpopSourceIndex::new(input);
    let (tokens, identity) = lex_fragment(input)?;
    let mut output = terlan_lalrpop::ModuleSyntaxOutputParser::new()
        .parse(
            input,
            &source_index,
            identity.as_str(),
            expression_spanned_tokens(tokens),
        )
        .map_err(lalrpop_error)?;
    output.root = normalize_lalrpop_expression(output.root);
    Ok(output)
}

fn lex_fragment(input: &str) -> LalrpopBoundaryResult<(Vec<Token>, String)> {
    let tokens = lex(input).map_err(|errors| {
        let first = errors.into_iter().next();
        LalrpopBoundaryError {
            message: first.as_ref().map_or_else(
                || "lexical failure".to_string(),
                |error| error.message.clone(),
            ),
            span: first.map_or_else(|| Span::new(0, 0), |error| error.span),
        }
    })?;
    ensure_generated_nesting_limit(&tokens)?;
    let identity = cached_canonical_terlan_syntax_contract_identity()
        .map_err(|error| LalrpopBoundaryError {
            message: format!("canonical syntax contract is unavailable: {error:?}"),
            span: Span::new(0, 0),
        })?
        .fingerprint;
    Ok((tokens, identity))
}

fn ensure_generated_nesting_limit(tokens: &[Token]) -> LalrpopBoundaryResult<()> {
    const MAX_SYNTACTIC_NESTING: usize = 16;
    let mut depth = 0usize;
    for token in tokens {
        match token.kind {
            TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace => {
                depth += 1;
                if depth > MAX_SYNTACTIC_NESTING {
                    return Err(LalrpopBoundaryError {
                        message: format!(
                            "source nesting depth exceeds maximum of {MAX_SYNTACTIC_NESTING}"
                        ),
                        span: Span::new(token.start, token.end),
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

fn lalrpop_error(
    error: LalrpopParseError<usize, LalrpopToken, LalrpopLexicalError>,
) -> LalrpopBoundaryError {
    match error {
        LalrpopParseError::InvalidToken { location } => LalrpopBoundaryError {
            message: "invalid token".to_string(),
            span: Span::new(location, location),
        },
        LalrpopParseError::UnrecognizedEof { location, expected } => LalrpopBoundaryError {
            message: expected_message("unexpected end of input", &expected),
            span: Span::new(location, location),
        },
        LalrpopParseError::UnrecognizedToken {
            token: (start, _, end),
            expected,
        } => LalrpopBoundaryError {
            message: expected_message("unexpected token", &expected),
            span: Span::new(start, end),
        },
        LalrpopParseError::ExtraToken {
            token: (start, _, end),
        } => LalrpopBoundaryError {
            message: "unexpected trailing token".to_string(),
            span: Span::new(start, end),
        },
        LalrpopParseError::User { error } => LalrpopBoundaryError {
            message: error.to_string(),
            span: Span::new(0, 0),
        },
    }
}

fn expected_message(prefix: &str, expected: &[String]) -> String {
    if expected.is_empty() {
        prefix.to_string()
    } else {
        let expected = expected
            .iter()
            .map(|terminal| {
                if terminal == "\";\"" {
                    "Semicolon"
                } else {
                    terminal
                }
            })
            .collect::<Vec<_>>();
        format!("{prefix}; expected {}", expected.join(", "))
    }
}
