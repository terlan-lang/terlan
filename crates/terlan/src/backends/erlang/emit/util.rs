//! Shared Erlang backend lowering utilities.

use super::*;

mod type_specs;
pub(in crate::backends::erlang::emit) use type_specs::*;

/// Returns whether a name is one of Terlan's boolean literals.
///
/// Inputs: `name` is source identifier text. Output: `true` for `true` or
/// `false`. Transformation: performs an exact literal-name match.
pub(in crate::backends::erlang::emit) fn is_bool_literal_name(name: &str) -> bool {
    matches!(name, "true" | "false")
}

#[derive(Debug, Clone)]
/// Lowered template payload prepared for Erlang HTML/template emission.
///
/// Inputs: parsed HTML nodes and template prop types. Output: shared payload
/// consumed by template emitter paths. Transformation: stores parsed template
/// data without changing node or prop semantics.
pub(super) struct LowerTemplate {
    pub(super) nodes: Vec<crate::terlan_html::HtmlNode>,
    pub(super) prop_order: Vec<String>,
    pub(super) props: BTreeMap<String, LowerTemplateProp>,
}

#[derive(Debug, Clone)]
/// Lowered template property metadata prepared for Erlang emission.
///
/// Inputs: syntax-output template property type plus optional default
/// expression. Output: shared property payload consumed by slot lowering.
/// Transformation: stores escaping type metadata together with any expression
/// used when an instantiation omits the property.
pub(super) struct LowerTemplateProp {
    pub(super) type_text: String,
    pub(super) default: Option<SyntaxExprOutput>,
}

/// Builds the Erlang wrapper name for a constructor-like shape.
///
/// Inputs: source constructor `name`, fixed arity, and varargs flag. Output:
/// Erlang-safe constructor function name. Transformation: lowercases the type
/// name and encodes arity/varargs in the generated name.
pub(in crate::backends::erlang::emit) fn constructor_function_name(
    name: &str,
    fixed_arity: usize,
    varargs: bool,
) -> String {
    if varargs {
        format!(
            "typer_ctor_{}_varargs_{}",
            to_erlang_type_name(name),
            fixed_arity
        )
    } else {
        format!("typer_ctor_{}_{}", to_erlang_type_name(name), fixed_arity)
    }
}

/// Lowers a raw declaration block into backend Erlang text.
///
/// Inputs: raw declaration `kind` and body `text`. Output: Erlang source text.
/// Transformation: emits NIF stubs for `native` declarations and comment
/// placeholders for other raw declaration kinds.
pub(in crate::backends::erlang::emit) fn lower_raw_decl_text(kind: &str, text: &str) -> String {
    match kind {
        "native" => {
            let signatures = extract_native_function_signatures(text);
            if signatures.is_empty() {
                return format!("% terlan native: {}\n\n", text);
            }

            let native_module =
                extract_native_module_name(text).unwrap_or_else(|| "terlan_native".to_string());
            let mut names = BTreeSet::new();
            for signature in &signatures {
                names.insert((signature.name.clone(), signature.arity));
            }

            let mut out = String::new();
            out.push_str(&format!(
                "-on_load(load/0).\n-export([load/0, {}]).\n\n",
                names
                    .iter()
                    .map(|(name, arity)| format!("{}/{}", name, arity))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            out.push_str(&format!(
                "load() ->\n    erlang:load_nif(filename:join([code:priv_dir(?MODULE), \"{}.so\"]), 0).\n\n",
                native_module
            ));
            for (name, arity) in names {
                let args = (0..arity)
                    .map(|idx| format!("A{}", idx + 1))
                    .collect::<Vec<_>>()
                    .join(", ");
                out.push_str(&format!(
                    "{}({}) ->\n    erlang:nif_error(nif_not_loaded).\n\n",
                    name, args
                ));
            }
            out
        }
        _ => format!("% terlan {}: {}\n\n", kind, text),
    }
}

/// Extracts a simple template type name.
///
/// Inputs: `type_text` is a template type annotation. Output: the type text
/// when it is a simple uppercase identifier. Transformation: validates the
/// identifier shape without resolving it.
pub(in crate::backends::erlang::emit) fn simple_template_type_name(
    type_text: &str,
) -> Option<&str> {
    let mut chars = type_text.chars();
    let first = chars.next()?;
    if !first.is_ascii_uppercase() {
        return None;
    }
    if chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_') {
        Some(type_text)
    } else {
        None
    }
}

/// Returns whether a template type annotation denotes HTML output.
///
/// Inputs: `type_text` is a type annotation. Output: HTML-type flag.
/// Transformation: trims whitespace and checks `Html` or `Html[...]` forms.
pub(in crate::backends::erlang::emit) fn is_template_html_type(type_text: &str) -> bool {
    let trimmed = type_text.trim();
    trimmed == "Html" || trimmed.starts_with("Html[")
}

/// Extracts static HTML attribute text from supported Erlang string forms.
///
/// Inputs: `value` is rendered attribute text. Output: unwrapped text when the
/// input is a quoted string or Erlang binary string. Transformation: strips
/// known wrappers and otherwise preserves the value.
pub(in crate::backends::erlang::emit) fn static_html_attr_binary_text(value: &str) -> String {
    if let Some(inner) = value
        .strip_prefix('"')
        .and_then(|text| text.strip_suffix('"'))
    {
        return inner.to_string();
    }
    if let Some(inner) = value
        .strip_prefix("<<\"")
        .and_then(|text| text.strip_suffix("\">>"))
    {
        return inner.to_string();
    }
    value.to_string()
}

/// Renders bytes as an Erlang binary literal.
///
/// Inputs: `bytes` is raw UTF-8 or binary content. Output: Erlang binary
/// literal text. Transformation: joins bytes as decimal byte values inside
/// `<<...>>`.
pub(in crate::backends::erlang::emit) fn erlang_binary_bytes(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return "<<>>".to_string();
    }

    format!(
        "<<{}>>",
        bytes
            .iter()
            .map(u8::to_string)
            .collect::<Vec<_>>()
            .join(",")
    )
}

/// Builds an Erlang binary expression for static HTML text.
///
/// Inputs: `text` is static HTML. Output: `ErlExpr::Binary`. Transformation:
/// escapes Erlang binary string characters and wraps them in `<<"...">>`.
pub(in crate::backends::erlang::emit) fn html_binary(text: &str) -> ErlExpr {
    ErlExpr::Binary(format!("<<\"{}\">>", escape_erlang_binary_string(text)))
}

/// Escapes text for inclusion in an Erlang binary string literal.
///
/// Inputs: `text` is unescaped source text. Output: escaped literal payload.
/// Transformation: escapes backslash, quote, newline, carriage return, and tab.
pub(in crate::backends::erlang::emit) fn escape_erlang_binary_string(text: &str) -> String {
    text.chars()
        .flat_map(|ch| match ch {
            '\\' => "\\\\".chars().collect::<Vec<_>>(),
            '"' => "\\\"".chars().collect::<Vec<_>>(),
            '\n' => "\\n".chars().collect::<Vec<_>>(),
            '\r' => "\\r".chars().collect::<Vec<_>>(),
            '\t' => "\\t".chars().collect::<Vec<_>>(),
            other => vec![other],
        })
        .collect()
}

/// Escapes text for an HTML attribute value.
///
/// Inputs: `text` is raw attribute text. Output: HTML-escaped text.
/// Transformation: replaces ampersand, quote, less-than, and greater-than.
pub(in crate::backends::erlang::emit) fn escape_html_attr(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Normalizes trait type text for generated wrapper names and lookups.
///
/// Inputs: `text` is trait type text. Output: whitespace-normalized text.
/// Transformation: collapses whitespace-separated tokens with single spaces.
pub(in crate::backends::erlang::emit) fn normalize_trait_type_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Builds the default trait-method dictionary wrapper name.
///
/// Inputs: trait and method names. Output: Erlang-safe wrapper function name.
/// Transformation: sanitizes both names and appends the dictionary suffix.
pub(in crate::backends::erlang::emit) fn trait_method_wrapper_name(
    trait_name: &str,
    method_name: &str,
) -> String {
    format!(
        "typer_trait_{}_{}_dict",
        sanitize_erlang_fn_name(trait_name),
        sanitize_erlang_fn_name(method_name),
    )
}

/// Builds a typed trait-method dictionary wrapper name.
///
/// Inputs:
/// - `trait_name`: source trait name.
/// - `method_name`: trait method name.
/// - `type_arg`: concrete implementation type argument.
///
/// Output:
/// - Erlang-safe wrapper function name that is distinct per trait/method/type.
///
/// Transformation:
/// - Sanitizes every source segment and appends the type identity before the
///   dictionary suffix so multiple impls for the same method can coexist.
pub(in crate::backends::erlang::emit) fn typed_trait_method_wrapper_name(
    trait_name: &str,
    method_name: &str,
    type_arg: &str,
) -> String {
    format!(
        "typer_trait_{}_{}_{}_dict",
        sanitize_erlang_fn_name(trait_name),
        sanitize_erlang_fn_name(method_name),
        sanitize_erlang_fn_name(type_arg),
    )
}

/// Sanitizes source text for use as an Erlang function atom.
///
/// Inputs: `input` is arbitrary source-derived text. Output: lowercase
/// Erlang-safe function name. Transformation: replaces non-identifier
/// characters with underscores and prefixes digit-leading names.
pub(in crate::backends::erlang::emit) fn sanitize_erlang_fn_name(input: &str) -> String {
    let mut out = String::new();
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        "_".to_string()
    } else if out.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
        format!("_{}", out)
    } else {
        out
    }
}

/// Sanitizes source text for use as an Erlang variable.
///
/// Inputs: `name` is source-derived variable text. Output: Erlang-safe
/// variable name. Transformation: preserves underscores, replaces invalid
/// characters, and ensures non-special variables start uppercase or underscore.
pub(in crate::backends::erlang::emit) fn sanitize_erlang_var(name: &str) -> String {
    let mut out = String::new();
    for (idx, ch) in name.chars().enumerate() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            if idx == 0 && ch.is_ascii_lowercase() {
                out.push(ch.to_ascii_uppercase());
            } else {
                out.push(ch);
            }
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        "_arg".to_string()
    } else if out == "_" || out.starts_with('_') {
        out
    } else if out.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
        format!("_{}", out)
    } else if out.chars().next().is_some_and(|ch| ch.is_ascii_lowercase()) {
        let mut chars = out.chars();
        let first = chars.next().unwrap().to_ascii_uppercase();
        format!("{}{}", first, chars.collect::<String>())
    } else {
        out
    }
}

/// Builds an Erlang trait dictionary marker expression.
///
/// Inputs: trait and method names. Output: Erlang map expression carrying
/// trait metadata. Transformation: sanitizes names into atom values stored
/// under stable internal keys.
pub(in crate::backends::erlang::emit) fn trait_dictionary_expr(
    trait_name: &str,
    method_name: &str,
) -> ErlExpr {
    ErlExpr::Map(vec![
        ErlMapField {
            key: "\"__typer_trait\"".to_string(),
            value: ErlExpr::Atom(sanitize_erlang_fn_name(trait_name)),
            required: false,
        },
        ErlMapField {
            key: "\"__typer_method\"".to_string(),
            value: ErlExpr::Atom(sanitize_erlang_fn_name(method_name)),
            required: false,
        },
    ])
}

/// Maps a syntax-output binary operator token to the Erlang operator model.
///
/// Inputs: optional operator token. Output: `ErlBinaryOp`. Transformation:
/// defaults missing operators to match/equality lowering and maps Terlan
/// aliases such as `&&`, `||`, and `!=`.
pub(in crate::backends::erlang::emit) fn lower_syntax_binary_op(
    operator: Option<&str>,
) -> ErlBinaryOp {
    match operator.unwrap_or("=") {
        "+" => ErlBinaryOp::Add,
        "-" => ErlBinaryOp::Sub,
        "*" => ErlBinaryOp::Mul,
        "/" => ErlBinaryOp::Div,
        "=" => ErlBinaryOp::Eq,
        "==" => ErlBinaryOp::EqEq,
        "=:=" => ErlBinaryOp::EqEqEq,
        "!=" | "/=" => ErlBinaryOp::NotEq,
        "=/=" => ErlBinaryOp::NotEqEq,
        ">=" => ErlBinaryOp::GtEq,
        "<" => ErlBinaryOp::Lt,
        ">" => ErlBinaryOp::Gt,
        "<=" => ErlBinaryOp::LtEq,
        "div" => ErlBinaryOp::DivRem,
        "rem" => ErlBinaryOp::Rem,
        "and" | "&&" => ErlBinaryOp::And,
        "or" | "||" => ErlBinaryOp::Or,
        "|>" => ErlBinaryOp::PipeForward,
        "!" => ErlBinaryOp::Send,
        _ => ErlBinaryOp::Eq,
    }
}

/// Maps a syntax-output unary operator token to the Erlang operator model.
///
/// Inputs: optional operator token. Output: `ErlUnaryOp`. Transformation:
/// maps negation and logical-not aliases, defaulting unknown tokens to `not`.
pub(in crate::backends::erlang::emit) fn lower_syntax_unary_op(
    operator: Option<&str>,
) -> ErlUnaryOp {
    match operator.unwrap_or("") {
        "-" => ErlUnaryOp::Neg,
        "not" | "!" => ErlUnaryOp::Not,
        _ => ErlUnaryOp::Not,
    }
}
