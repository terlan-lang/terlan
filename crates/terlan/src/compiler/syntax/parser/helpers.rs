use super::*;
pub(crate) use crate::terlan_syntax::unquote_single_quoted_atom;

/// Reports whether a token kind can carry an identifier-like spelling.
///
/// Inputs:
/// - `kind`: token kind to classify.
///
/// Output:
/// - `true` for lower identifiers, generic identifiers, and upper identifiers.
///
/// Transformation:
/// - Centralizes the parser's permissive identifier token set.
pub(super) fn is_identifier_like_token(kind: &TokenKind) -> bool {
    matches!(kind, TokenKind::Atom | TokenKind::Ident | TokenKind::Var)
}

/// Combines ordered list-comprehension filters into one boolean guard.
///
/// Inputs:
/// - `guard`: optional accumulated guard expression from earlier filters.
/// - `filter`: next filter expression in source order.
///
/// Output:
/// - Guard expression equivalent to all filters seen so far.
///
/// Transformation:
/// - Folds comma-separated comprehension filters with `and`, preserving
///   left-to-right source order while reusing the current single-guard parse tree
///   representation.
pub(super) fn combine_comprehension_filter_guard(
    guard: Option<Box<Expr>>,
    filter: Expr,
) -> Box<Expr> {
    match guard {
        Some(previous) => Box::new(Expr::BinaryOp {
            op: BinaryOp::And,
            left: previous,
            right: Box::new(filter),
        }),
        None => Box::new(filter),
    }
}

/// Parses a lexer integer token into its signed integer value.
///
/// Inputs:
/// - `token`: an integer token emitted by the lexer, including decimal or
///   prefixed `0b`, `0x`, and `0o` forms.
///
/// Output:
/// - Parsed `i64` value, or a parse diagnostic anchored to the token span.
///
/// Transformation:
/// - Selects the radix from the token prefix and delegates to Rust integer
///   parsing, preserving one stable diagnostic message for invalid literals.
pub(super) fn parse_int_literal_token(token: &Token) -> ParseResult<i64> {
    parse_int_literal_text(&token.text).ok_or_else(|| ParseError {
        message: "invalid integer literal".to_string(),
        span: token.span(),
    })
}

/// Parses the string token payload used by `Atom["name"]`.
///
/// Inputs:
/// - `token`: lexer token for the quoted string inside the atom literal.
///
/// Output:
/// - The unescaped non-empty atom payload.
///
/// Transformation:
/// - Removes the surrounding quotes and recognizes the small string escape set
///   needed by symbolic atom payloads.
pub(super) fn parse_atom_string_literal_token(token: &Token) -> Option<String> {
    let payload = parse_string_token_payload(&token.text)?;
    if payload.is_empty() {
        None
    } else {
        Some(payload)
    }
}

/// Unquotes a normal Terlan string token.
///
/// Inputs:
/// - `text`: raw lexer token text including double quotes.
///
/// Output:
/// - The unescaped payload when `text` is a valid quoted string token.
///
/// Transformation:
/// - Performs deterministic escape decoding without interpreting source text
///   outside the existing lexer token boundary.
pub(super) fn parse_string_token_payload(text: &str) -> Option<String> {
    let inner = text.strip_prefix('"')?.strip_suffix('"')?;
    let mut output = String::new();
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            output.push(ch);
            continue;
        }
        let escaped = chars.next()?;
        match escaped {
            '"' => output.push('"'),
            '\\' => output.push('\\'),
            'n' => output.push('\n'),
            'r' => output.push('\r'),
            't' => output.push('\t'),
            other => output.push(other),
        }
    }
    Some(output)
}

/// Parses integer literal text using Terlan's supported radix prefixes.
///
/// Inputs:
/// - `text`: raw token text for a decimal, binary, hexadecimal, or octal
///   integer literal.
///
/// Output:
/// - Parsed `i64` value when the text is valid for its radix.
///
/// Transformation:
/// - Strips recognized radix prefixes and applies the matching base; plain
///   text remains decimal.
pub(super) fn parse_int_literal_text(text: &str) -> Option<i64> {
    if let Some(digits) = text.strip_prefix("0b") {
        return i64::from_str_radix(digits, 2).ok();
    }
    if let Some(digits) = text.strip_prefix("0x") {
        return i64::from_str_radix(digits, 16).ok();
    }
    if let Some(digits) = text.strip_prefix("0o") {
        return i64::from_str_radix(digits, 8).ok();
    }
    text.parse::<i64>().ok()
}

/// Reports whether a name starts a raw declaration family.
///
/// Inputs:
/// - `name`: lower-case declaration name candidate.
///
/// Output:
/// - `true` for raw declarations preserved by the parser.
///
/// Transformation:
/// - Keeps non-core declaration families explicit at the parser boundary.
pub(super) fn is_raw_declaration_name(name: &str) -> bool {
    matches!(name, "target" | "native" | "machine" | "static")
}

/// Attaches parsed documentation tokens to a declaration.
///
/// Inputs:
/// - `decl`: declaration parsed after documentation comments.
/// - `docs`: raw documentation tokens in source order.
///
/// Output:
/// - Declaration with its documentation field populated where supported.
///
/// Transformation:
/// - Mutates only declaration variants that carry docs and leaves imports and
///   exports unchanged.
pub(super) fn attach_docs(mut decl: Decl, docs: Vec<String>) -> Decl {
    if docs.is_empty() {
        return decl;
    }

    match &mut decl {
        Decl::Type(type_decl) => type_decl.docs = docs,
        Decl::Struct(struct_decl) => struct_decl.docs = docs,
        Decl::Constructor(constructor_decl) => constructor_decl.docs = docs,
        Decl::Function(function_decl) => function_decl.docs = docs,
        Decl::Method(method_decl) => method_decl.docs = docs,
        Decl::Raw(raw_decl) => raw_decl.docs = docs,
        Decl::Trait(trait_decl) => trait_decl.docs = docs,
        Decl::TraitImpl(trait_impl_decl) => trait_impl_decl.docs = docs,
        Decl::AnnotationSchema(annotation_schema_decl) => annotation_schema_decl.docs = docs,
        Decl::Template(template_decl) => template_decl.docs = docs,
        Decl::Import(_) | Decl::Export(_) => {}
    }

    decl
}

/// Checks whether normalized block documentation declares module docs.
///
/// Inputs:
/// - `text`: normalized public documentation block text.
///
/// Output:
/// - `true` when any trimmed line starts with `@module`.
///
/// Transformation:
/// - Treats the TypeDoc-style `@module` tag as the marker that a block belongs
///   to the module declaration rather than to a following item declaration.
pub(super) fn is_module_doc_block(text: &str) -> bool {
    text.lines()
        .any(|line| line.trim_start().starts_with("@module"))
}

/// Joins raw declaration token parts into readable source text.
///
/// Inputs:
/// - `parts`: token text collected from a raw declaration.
///
/// Output:
/// - Stable raw declaration text.
///
/// Transformation:
/// - Inserts spaces where needed while preserving punctuation adjacency for
///   dots, brackets, commas, and operators.
pub(super) fn join_parts(parts: &[String]) -> String {
    let mut output = String::new();
    let mut first = true;
    for part in parts {
        if first {
            output.push_str(part);
            first = false;
        } else if part == "." {
            output.push('.');
        } else if output.ends_with('.') {
            output.push_str(part);
        } else if part == "," || part == "|" || part == ":" || part == "->" || part == "|>" {
            output.push(' ');
            output.push_str(part);
        } else if output.ends_with('(')
            || output.ends_with('[')
            || output.ends_with('{')
            || part == "["
            || part == "]"
            || part == ","
        {
            output.push_str(part);
        } else {
            output.push(' ');
            output.push_str(part);
        }
    }
    output
}
