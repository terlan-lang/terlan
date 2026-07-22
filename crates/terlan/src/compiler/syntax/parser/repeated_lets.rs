use super::*;

/// Parses a full canonical Terlan source module.
pub(crate) fn parse_module(input: &str) -> ParseResult<Module> {
    parse_module_with_mode(input, LetBindingMode::Canonical).map(|(module, _)| module)
}

/// Parses source while accepting retired implicit bindings for migration only.
pub(crate) fn parse_module_for_repeated_let_migration(input: &str) -> ParseResult<Module> {
    parse_module_with_mode(input, LetBindingMode::MigrateImplicit).map(|(module, _)| module)
}

/// Finds implicit binding offsets after validating the complete source module.
pub(crate) fn repeated_let_migration_offsets(input: &str) -> ParseResult<Vec<usize>> {
    parse_module_with_mode(input, LetBindingMode::MigrateImplicit).map(|(_, offsets)| offsets)
}

fn parse_module_with_mode(input: &str, mode: LetBindingMode) -> ParseResult<(Module, Vec<usize>)> {
    ensure_syntax_contract_valid().map_err(syntax_contract_parse_error)?;
    let tokens = lex(input).map_err(|errors| {
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
    })?;
    ensure_token_nesting_within_limit(&tokens)?;

    let mut parser = Parser::new(tokens, mode);
    let module = parser.parse_module()?;
    Ok((module, parser.implicit_let_binding_offsets))
}
