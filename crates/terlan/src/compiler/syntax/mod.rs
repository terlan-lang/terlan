pub mod ebnf;
mod ebnf_lexer;
pub mod formatter;
mod html_syntax;
pub mod lalrpop_boundary;
#[cfg(test)]
mod lalrpop_boundary_test;
mod lalrpop_diagnostics;
mod lalrpop_lowering;
mod lalrpop_projection;
pub mod lalrpop_syntax;
pub mod lexer;
pub mod native;
mod parse_tree;
mod parser;
mod parser_contract;
mod raw_shape;
pub mod span;
pub(crate) mod sql_regions;
pub mod syntax_contract;
pub mod syntax_output;
pub mod token;
mod trait_impl_ref;

pub use ebnf::*;
pub use formatter::{
    format_interface_source_module, format_script_source, format_source_module,
    format_source_module_migrating_repeated_lets, migrate_repeated_let_source,
};
pub use lexer::*;
pub use native::*;
#[cfg(test)]
pub(crate) use parser::{parse_interface_module, parse_module, parse_terlan_expr};
pub use parser::{ParseResult, ParserError};
pub(crate) use raw_shape::raw_shape_signature;
pub use span::Span;
pub const REPEATED_LET_BINDING_DIAGNOSTIC: &str =
    "subsequent local binding must start with `let`; insert `let` before this binding";
pub const COMMA_GROUPED_LET_BINDING_DIAGNOSTIC: &str =
    "local bindings must be separated by `; let`, not commas";
pub const RAW_ATOM_LITERAL_DIAGNOSTIC: &str =
    "raw `:atom` literal syntax is not supported; use `Atom[\"name\"]` or a named singleton constructor";
pub const NEGATIVE_STRUCTURAL_IMPLICATION_DIAGNOSTIC: &str =
    "negative structural implications are not supported; use negative trait implementations for denied capabilities";
pub const AMBIGUOUS_STRUCTURAL_IMPLICATION_DIAGNOSTIC: &str =
    "ambiguous_implication: structural implication field names must be unique within each shape";
pub const DUPLICATE_RECORD_TYPE_FIELD_DIAGNOSTIC: &str =
    "duplicate_record_type_field: record type field names must be unique within each record";
pub(crate) use sql_regions::sql_opaque_region_end;
pub use syntax_contract::{
    cached_canonical_terlan_syntax_contract, cached_canonical_terlan_syntax_contract_artifact,
    cached_canonical_terlan_syntax_contract_artifact_json,
    cached_canonical_terlan_syntax_contract_identity,
    cached_canonical_terlan_syntax_contract_identity_json, canonical_terlan_syntax_contract,
    check_syntax_contract_artifact_against_current, ensure_canonical_syntax_contract_valid,
    extract_syntax_contract_artifact_fingerprint, syntax_contract_artifact_matches_current,
    syntax_contract_fingerprint, syntax_contract_identity_from_fingerprint,
    syntax_contract_identity_matches_current, validate_ebnf_source, validate_syntax_contract,
    validated_canonical_terlan_syntax_contract, EbnfValidationFinding, EbnfValidationReport,
    SyntaxContractArtifact, SyntaxContractArtifactCheck, SyntaxContractDiagnostic,
    SyntaxContractError, SyntaxContractIdentity, CANONICAL_TERLAN_EBNF,
    SYNTAX_CONTRACT_ARTIFACT_SCHEMA, SYNTAX_CONTRACT_FINGERPRINT_ALGORITHM,
};
pub use syntax_output::{
    expand_shape_imports, parse_expr_as_syntax_output, parse_interface_module_as_syntax_output,
    parse_module_as_syntax_output, parse_script_as_syntax_output, syntax_module_import_identities,
    syntax_module_import_identity, SyntaxClauseOutput, SyntaxConfigEntryOutput,
    SyntaxConfigValueOutput, SyntaxConstructorClauseOutput, SyntaxConstructorParamOutput,
    SyntaxDeclarationOutput, SyntaxDeclarationPayload, SyntaxExportItem, SyntaxExprFieldOutput,
    SyntaxExprKind, SyntaxExprOutput, SyntaxFunctionClauseOutput, SyntaxHtmlAttrOutput,
    SyntaxHtmlAttrValueOutput, SyntaxHtmlElementOutput, SyntaxHtmlNamedSlotOutput,
    SyntaxHtmlNodeOutput, SyntaxImplConstOutput, SyntaxImplMethodOutput, SyntaxImportItem,
    SyntaxImportKind, SyntaxModuleOutput, SyntaxParamOutput, SyntaxPatternFieldOutput,
    SyntaxPatternKind, SyntaxPatternOutput, SyntaxShapeImport, SyntaxSourceKind,
    SyntaxStructFieldOutput, SyntaxTemplatePropOutput, SyntaxTraitConstOutput,
    SyntaxTraitMethodOutput, SyntaxTypeOutput, SyntaxValuedUnionArmOutput,
    SYNTAX_MODULE_OUTPUT_SCHEMA,
};
pub use token::{Token, TokenKind};
pub(crate) use trait_impl_ref::{render_trait_impl_ref, split_trait_impl_ref};

/// Parses, validates through syntax output, and formats a source module once.
pub fn format_validated_source_module(input: &str) -> EbnfCompileResult<String> {
    let module = parser::parse_module(input)
        .map_err(|error| EbnfCompileError::Parse(error.message, error.span))?;
    syntax_output::module_as_validated_syntax_output(&module, SyntaxSourceKind::Module)?;
    Ok(formatter::format_module(&module))
}

/// Parses, validates through syntax output, and formats an interface once.
pub fn format_validated_interface_module(input: &str) -> EbnfCompileResult<String> {
    let module = parser::parse_interface_module(input)
        .map_err(|error| EbnfCompileError::Parse(error.message, error.span))?;
    syntax_output::module_as_validated_syntax_output(&module, SyntaxSourceKind::Interface)?;
    Ok(formatter::format_module(&module))
}

/// Converts a type alias name into its implicit singleton atom payload.
///
/// Inputs:
/// - `name`: source type name such as `InvalidMove` or `HTTPError`.
///
/// Output:
/// - Lower snake-case atom payload such as `invalid_move` or `http_error`.
///
/// Transformation:
/// - Splits camel-case, acronym, and letter-to-digit boundaries without
///   changing existing underscores or runs of digits.
pub(crate) fn type_name_to_atom_payload(name: &str) -> String {
    let chars = name.chars().collect::<Vec<_>>();
    let mut out = String::new();
    for (index, ch) in chars.iter().enumerate() {
        if ch.is_ascii_uppercase() {
            let previous = index.checked_sub(1).and_then(|i| chars.get(i)).copied();
            let next = chars.get(index + 1).copied();
            let starts_new_word = index > 0
                && (previous
                    .is_some_and(|prev| prev.is_ascii_lowercase() || prev.is_ascii_digit())
                    || next.is_some_and(|next| next.is_ascii_lowercase()));
            if starts_new_word {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else if ch.is_ascii_digit() {
            let previous = index.checked_sub(1).and_then(|i| chars.get(i)).copied();
            if previous
                .is_some_and(|previous| previous.is_ascii_alphabetic() && !out.ends_with('_'))
            {
                out.push('_');
            }
            out.push(*ch);
        } else {
            out.push(*ch);
        }
    }
    out
}

/// Decodes the quoted payload accepted by the legacy `:'name'` atom alias.
pub(crate) fn unquote_single_quoted_atom(text: &str) -> Option<String> {
    let inner = text.strip_prefix('\'')?.strip_suffix('\'')?;
    let mut output = String::new();
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(escaped) = chars.next() {
                output.push(escaped);
            }
        } else {
            output.push(ch);
        }
    }
    Some(output)
}

/// Escapes text as a double-quoted Terlan source string literal.
///
/// Inputs:
/// - `value`: unescaped string payload.
///
/// Output:
/// - Double-quoted literal text.
///
/// Transformation:
/// - Escapes backslash, double quote, newline, carriage return, and tab
///   characters using the portable escaping accepted by Terlan source and the
///   backend literal contexts used by Rust, JavaScript, and TypeScript emitters.
pub(crate) fn quoted_string_literal(value: &str) -> String {
    let mut out = String::from("\"");
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}
