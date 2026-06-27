use crate::terlan_html::escape_html_text;
use crate::terlan_syntax::{
    SyntaxParamOutput, SyntaxStructFieldOutput, SyntaxTraitMethodOutput, SyntaxTypeOutput,
};

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
    if !variants.is_empty() {
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
    for_type: &SyntaxTypeOutput,
    is_public: bool,
) -> String {
    format!(
        "{}impl {} for {}.",
        if is_public { "pub " } else { "" },
        trait_ref.text,
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

/// Escapes text before embedding it into generated documentation HTML.
///
/// Inputs:
/// - `input`: raw text.
///
/// Output:
/// - HTML-safe text.
///
/// Transformation:
/// - Delegates text escaping to `terlan_html` so generated documentation HTML
///   shares the compiler HTML escaping boundary.
pub(super) fn sanitize_html_text(input: &str) -> String {
    escape_html_text(input)
}
