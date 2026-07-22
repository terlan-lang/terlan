use crate::terlan_syntax::{
    SyntaxParamOutput, SyntaxStructFieldOutput, SyntaxTraitMethodOutput, SyntaxTypeOutput,
    SyntaxValuedUnionArmOutput,
};

/// Adds the compiler-owned `@pure` prefix to a rendered signature when needed.
///
/// Inputs:
/// - `is_pure`: whether the source declaration carries marker-only `@pure`.
/// - `signature`: source-shaped declaration signature.
///
/// Output:
/// - Signature text prefixed with `@pure` on a separate line when pure.
///
/// Transformation:
/// - Keeps documentation renderers aligned with summary and LSP display without
///   making every renderer duplicate annotation formatting.
pub(super) fn render_purity_marked_signature(is_pure: bool, signature: String) -> String {
    if is_pure {
        format!("@pure\n{signature}")
    } else {
        signature
    }
}

/// Renders a type declaration signature for documentation JSON.
///
/// Inputs:
/// - `name`: type name.
/// - `params`: type parameter names.
/// - `is_public`: whether the type is public.
/// - `is_opaque`: whether the type uses opaque visibility.
/// - `variants`: rendered type expression variants.
///
/// Output:
/// - Source-shaped type declaration signature.
///
/// Transformation:
/// - Combines visibility, opacity, parameters, and variants into one line.
pub(super) fn render_type_signature(
    name: &str,
    params: &[String],
    is_public: bool,
    is_opaque: bool,
    variants: &[SyntaxTypeOutput],
    representation: Option<&SyntaxTypeOutput>,
    valued_arms: &[SyntaxValuedUnionArmOutput],
) -> String {
    let mut out = String::new();
    out.push_str(if is_public { "pub " } else { "" });
    out.push_str(if is_opaque { "opaque " } else { "type " });
    out.push_str(name);
    if !params.is_empty() {
        out.push('[');
        out.push_str(&params.join(", "));
        out.push(']');
    }
    if let Some(representation) = representation {
        out.push_str(": ");
        out.push_str(&representation.text);
        out.push_str(" = ");
        out.push_str(
            &valued_arms
                .iter()
                .map(|arm| {
                    format!(
                        "{} = {}",
                        arm.name,
                        super::render_const_expr_text(&arm.value)
                    )
                })
                .collect::<Vec<_>>()
                .join(" | "),
        );
    } else if !variants.is_empty() {
        out.push_str(" = ");
        out.push_str(
            &variants
                .iter()
                .map(|variant| variant.text.as_str())
                .collect::<Vec<_>>()
                .join(" | "),
        );
    }
    out.push('.');
    out
}

/// Renders a struct declaration signature for documentation JSON.
///
/// Inputs:
/// - `name`: struct name.
/// - `is_public`: whether the struct is public.
/// - `fields`: struct fields.
///
/// Output:
/// - Compact source-shaped struct signature.
///
/// Transformation:
/// - Joins field declarations into a single-line signature for machine
///   consumers that do not need Markdown formatting.
pub(super) fn render_struct_signature(
    name: &str,
    is_public: bool,
    fields: &[SyntaxStructFieldOutput],
) -> String {
    let fields = fields
        .iter()
        .map(|field| format!("{}: {}", field.name, field.annotation.text))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "{}struct {} {{{}}}.",
        if is_public { "pub " } else { "" },
        name,
        fields
    )
}

/// Renders a constructor declaration signature for documentation JSON.
///
/// Inputs:
/// - `name`: constructor owner type name.
/// - `params`: type parameter names.
/// - `is_public`: whether the constructor declaration is public.
///
/// Output:
/// - Source-shaped constructor declaration header.
///
/// Transformation:
/// - Renders the declaration header because constructor clauses are represented
///   separately in syntax output.
pub(super) fn render_constructor_signature(
    name: &str,
    params: &[String],
    is_public: bool,
) -> String {
    let mut out = String::new();
    out.push_str(if is_public { "pub " } else { "" });
    out.push_str("constructor ");
    out.push_str(name);
    if !params.is_empty() {
        out.push('[');
        out.push_str(&params.join(", "));
        out.push(']');
    }
    out.push('.');
    out
}

/// Renders a function declaration signature for documentation JSON.
///
/// Inputs:
/// - `name`: function name.
/// - `params`: function parameters.
/// - `return_type`: return type.
/// - `is_public`: whether the function is public.
/// - `is_macro`: whether the function is a macro.
///
/// Output:
/// - Source-shaped function signature.
///
/// Transformation:
/// - Joins parameters and return annotation into a declaration signature.
pub(super) fn render_function_signature(
    name: &str,
    params: &[SyntaxParamOutput],
    return_type: &SyntaxTypeOutput,
    is_public: bool,
    is_macro: bool,
) -> String {
    format!(
        "{}{}{}({}): {}.",
        if is_public { "pub " } else { "" },
        if is_macro { "macro " } else { "" },
        name,
        params
            .iter()
            .map(render_syntax_param_signature)
            .collect::<Vec<_>>()
            .join(", "),
        return_type.text
    )
}

/// Renders a receiver method signature for documentation JSON.
///
/// Inputs:
/// - `receiver`: method receiver parameter.
/// - `name`: method name.
/// - `params`: method call parameters.
/// - `return_type`: return type.
/// - `is_public`: whether the method is public.
///
/// Output:
/// - Source-shaped receiver method signature.
///
/// Transformation:
/// - Places the receiver before the method name, matching Terlan source syntax.
pub(super) fn render_method_signature(
    receiver: &SyntaxParamOutput,
    name: &str,
    params: &[SyntaxParamOutput],
    return_type: &SyntaxTypeOutput,
    is_public: bool,
) -> String {
    format!(
        "{}({}) {}({}): {}.",
        if is_public { "pub " } else { "" },
        render_syntax_param_signature(receiver),
        name,
        params
            .iter()
            .map(render_syntax_param_signature)
            .collect::<Vec<_>>()
            .join(", "),
        return_type.text
    )
}

/// Renders a trait declaration signature for documentation JSON.
///
/// Inputs:
/// - `name`: trait name.
/// - `params`: trait type parameters.
/// - `super_traits`: inherited traits.
/// - `is_public`: whether the trait is public.
///
/// Output:
/// - Source-shaped trait declaration header.
///
/// Transformation:
/// - Renders only the trait header for compact JSON documentation.
pub(super) fn render_trait_signature(
    name: &str,
    params: &[String],
    super_traits: &[String],
    is_public: bool,
) -> String {
    let mut out = String::new();
    out.push_str(if is_public { "pub " } else { "" });
    out.push_str("trait ");
    out.push_str(name);
    if !params.is_empty() {
        out.push('[');
        out.push_str(&params.join(", "));
        out.push(']');
    }
    if !super_traits.is_empty() {
        out.push_str(" extends ");
        out.push_str(&super_traits.join(", "));
    }
    out.push('.');
    out
}

/// Renders a trait implementation signature for documentation JSON.
///
/// Inputs:
/// - `trait_ref`: implemented trait.
/// - `for_type`: implementation target type.
/// - `is_public`: whether the implementation is public.
///
/// Output:
/// - Source-shaped implementation header.
///
/// Transformation:
/// - Renders the trait/type pair without implementation method bodies.
pub(super) fn render_trait_impl_signature(
    trait_ref: &SyntaxTypeOutput,
    generic_params: &[String],
    for_type: &SyntaxTypeOutput,
    is_negative: bool,
    is_public: bool,
) -> String {
    if is_negative {
        return format!(
            "{}impl not {}[{}].",
            if is_public { "pub " } else { "" },
            trait_ref.text,
            for_type.text
        );
    }
    format!(
        "{}impl {} for {}.",
        if is_public { "pub " } else { "" },
        crate::terlan_syntax::render_trait_impl_ref(&trait_ref.text, generic_params),
        for_type.text
    )
}

/// Renders one trait method signature for documentation.
///
/// Inputs:
/// - `method`: syntax-output trait method.
///
/// Output:
/// - Indented Terlan method signature text.
///
/// Transformation:
/// - Joins rendered parameter signatures and appends return annotation text.
pub(super) fn render_syntax_trait_method_signature(method: &SyntaxTraitMethodOutput) -> String {
    let mut out = String::new();
    if method.is_pure {
        out.push_str("    @pure\n");
    }
    out.push_str("    ");
    out.push_str(&method.name);
    out.push('(');
    out.push_str(
        &method
            .params
            .iter()
            .map(render_syntax_param_signature)
            .collect::<Vec<_>>()
            .join(", "),
    );
    out.push_str("): ");
    out.push_str(&method.return_type.text);
    out.push('.');
    out
}

/// Renders one typed parameter signature for documentation.
///
/// Inputs:
/// - `param`: syntax-output parameter.
///
/// Output:
/// - `name: Type` parameter signature text.
///
/// Transformation:
/// - Combines parameter name and annotation text.
pub(super) fn render_syntax_param_signature(param: &SyntaxParamOutput) -> String {
    format!("{}: {}", param.name, param.annotation.text)
}

/// Renders a parse-preserved raw shape declaration signature.
///
/// Inputs:
/// - `raw_kind`: raw declaration kind emitted by syntax output.
/// - `text`: original raw declaration text.
///
/// Output:
/// - Shape name, public flag, and source-shaped signature when the raw
///   declaration is a shape.
///
/// Transformation:
/// - Lets documentation render the reserved shape surface without implementing
///   shape expansion or runtime semantics.
pub(super) fn render_raw_shape_signature(
    raw_kind: &str,
    text: &str,
) -> Option<(String, bool, String)> {
    if raw_kind != "shape" {
        return None;
    }

    let trimmed = text.trim();
    let (is_public, after_visibility) =
        if let Some(rest) = trimmed.strip_prefix("pub").and_then(trim_keyword_rest) {
            (true, rest)
        } else {
            (false, trimmed)
        };
    let after_shape = after_visibility
        .strip_prefix("shape")
        .and_then(trim_keyword_rest)?;
    let name = after_shape
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
        .collect::<String>();
    if name.is_empty() {
        return None;
    }

    let signature = trimmed.strip_suffix('.').unwrap_or(trimmed).trim_end();
    Some((name, is_public, format!("{signature}.")))
}

/// Trims whitespace after a recognized keyword token.
///
/// Inputs:
/// - `rest`: source text immediately after the keyword spelling.
///
/// Output:
/// - Remaining source after required whitespace.
///
/// Transformation:
/// - Prevents prefix matches such as `publisher` or `shapeName` from being
///   treated as keyword-bearing declarations.
fn trim_keyword_rest(rest: &str) -> Option<&str> {
    let mut chars = rest.chars();
    let first = chars.next()?;
    if !first.is_whitespace() {
        return None;
    }
    Some(chars.as_str().trim_start())
}
