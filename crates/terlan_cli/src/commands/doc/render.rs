use terlan_syntax::{
    SyntaxDeclarationPayload, SyntaxImplMethodOutput, SyntaxModuleOutput, SyntaxParamOutput,
    SyntaxStructFieldOutput, SyntaxTraitMethodOutput, SyntaxTypeOutput,
};

/// Renders syntax-output module documentation as Markdown.
///
/// Inputs:
/// - `module`: formal syntax-output module containing documentation metadata.
///
/// Output:
/// - Markdown documentation for the module.
///
/// Transformation:
/// - Groups declarations by documentation section and renders declaration
///   signatures from syntax-output fields.
pub(crate) fn render_syntax_module_docs_markdown(module: &SyntaxModuleOutput) -> String {
    let mut out = String::new();
    out.push_str(&format!("# `{}`\n\n", module.module_name));
    push_markdown_doc_block(&mut out, &module.docs);

    let types: Vec<_> = module
        .declarations
        .iter()
        .filter_map(|decl| match &decl.payload {
            SyntaxDeclarationPayload::Type {
                name,
                params,
                is_public,
                is_opaque,
                variants,
                ..
            } => Some((
                decl.docs.as_slice(),
                name,
                params,
                is_public,
                is_opaque,
                variants,
            )),
            _ => None,
        })
        .collect();
    if !types.is_empty() {
        out.push_str("## Types\n\n");
        for (docs, name, params, is_public, is_opaque, variants) in types {
            render_syntax_type_decl_docs_markdown(
                &mut out, docs, name, params, *is_public, *is_opaque, variants,
            );
        }
    }

    let structs: Vec<_> = module
        .declarations
        .iter()
        .filter_map(|decl| match &decl.payload {
            SyntaxDeclarationPayload::Struct {
                name,
                is_public,
                fields,
                ..
            } => Some((decl.docs.as_slice(), name, is_public, fields)),
            _ => None,
        })
        .collect();
    if !structs.is_empty() {
        out.push_str("## Structs\n\n");
        for (docs, name, is_public, fields) in structs {
            render_syntax_struct_decl_docs_markdown(&mut out, docs, name, *is_public, fields);
        }
    }

    let trait_decls: Vec<_> = module
        .declarations
        .iter()
        .filter_map(|decl| match &decl.payload {
            SyntaxDeclarationPayload::Trait {
                name,
                params,
                super_traits,
                is_public,
                methods,
            } => Some((
                decl.docs.as_slice(),
                name,
                params,
                super_traits,
                is_public,
                methods,
            )),
            _ => None,
        })
        .collect();
    if !trait_decls.is_empty() {
        out.push_str("## Traits\n\n");
        for (docs, name, params, super_traits, is_public, methods) in trait_decls {
            render_syntax_trait_decl_docs_markdown(
                &mut out,
                docs,
                name,
                params,
                super_traits,
                *is_public,
                methods,
            );
        }
    }

    let trait_impls: Vec<_> = module
        .declarations
        .iter()
        .filter_map(|decl| match &decl.payload {
            SyntaxDeclarationPayload::TraitImpl {
                trait_ref,
                for_type,
                is_public,
                methods,
            } => Some((
                decl.docs.as_slice(),
                trait_ref,
                for_type,
                is_public,
                methods.as_slice(),
            )),
            _ => None,
        })
        .collect();
    if !trait_impls.is_empty() {
        out.push_str("## Trait Implementations\n\n");
        for (docs, trait_ref, for_type, is_public, methods) in trait_impls {
            render_syntax_trait_impl_docs_markdown(
                &mut out, docs, trait_ref, for_type, *is_public, methods,
            );
        }
    }

    let raw_traits: Vec<_> = module
        .declarations
        .iter()
        .filter_map(|decl| match &decl.payload {
            SyntaxDeclarationPayload::Raw { raw_kind, text } if raw_kind == "trait" => {
                Some((decl.docs.as_slice(), raw_kind, text))
            }
            _ => None,
        })
        .collect();
    if !raw_traits.is_empty() {
        out.push_str("## Traits\n\n");
        for (docs, raw_kind, text) in raw_traits {
            render_syntax_raw_decl_docs_markdown(&mut out, docs, raw_kind, text);
        }
    }

    let functions: Vec<_> = module
        .declarations
        .iter()
        .filter_map(|decl| match &decl.payload {
            SyntaxDeclarationPayload::Function {
                name,
                params,
                return_type,
                is_public,
                is_macro,
                ..
            } => Some((
                decl.docs.as_slice(),
                name,
                params,
                return_type,
                is_public,
                is_macro,
            )),
            _ => None,
        })
        .collect();
    if !functions.is_empty() {
        out.push_str("## Functions\n\n");
        for (docs, name, params, return_type, is_public, is_macro) in functions {
            render_syntax_function_decl_docs_markdown(
                &mut out,
                docs,
                name,
                params,
                return_type,
                *is_public,
                *is_macro,
            );
        }
    }

    out
}

/// Renders syntax-output module documentation as a simple HTML document.
///
/// Inputs:
/// - `module`: formal syntax-output module containing documentation metadata.
///
/// Output:
/// - HTML documentation string.
///
/// Transformation:
/// - Renders Markdown first, then embeds it as escaped text inside a minimal
///   HTML shell.
pub(crate) fn render_syntax_module_docs_html(module: &SyntaxModuleOutput) -> String {
    let title = format!("{} documentation", module.module_name);
    let markdown = render_syntax_module_docs_markdown(module);
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<title>{}</title>\n</head>\n<body>\n<main>\n<pre>{}</pre>\n</main>\n</body>\n</html>\n",
        sanitize_html_text(&title),
        sanitize_html_text(&markdown)
    )
}

/// Appends documentation lines to a Markdown output buffer.
///
/// Inputs:
/// - `out`: Markdown output buffer.
/// - `docs`: documentation lines from syntax output.
///
/// Output:
/// - No return value.
///
/// Transformation:
/// - Appends lines with newlines and adds one blank line after non-empty docs.
fn push_markdown_doc_block(out: &mut String, docs: &[String]) {
    for line in docs {
        out.push_str(line);
        out.push('\n');
    }
    if !docs.is_empty() {
        out.push('\n');
    }
}

/// Appends a type declaration documentation section.
///
/// Inputs:
/// - `out`: Markdown output buffer.
/// - `docs`: documentation lines for the type.
/// - `name`: type name.
/// - `params`: type parameter names.
/// - `is_public`: whether the type is public.
/// - `is_opaque`: whether the type is opaque.
/// - `variants`: type expression variants.
///
/// Output:
/// - No return value.
///
/// Transformation:
/// - Renders docs and a Terlan type signature fence.
fn render_syntax_type_decl_docs_markdown(
    out: &mut String,
    docs: &[String],
    name: &str,
    params: &[String],
    is_public: bool,
    is_opaque: bool,
    variants: &[SyntaxTypeOutput],
) {
    out.push_str(&format!("### `{}`\n\n", name));
    push_markdown_doc_block(out, docs);
    out.push_str("```terlan\n");
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
    out.push_str(".\n```\n\n");
}

/// Appends a struct declaration documentation section.
///
/// Inputs:
/// - `out`: Markdown output buffer.
/// - `docs`: documentation lines for the struct.
/// - `name`: struct name.
/// - `is_public`: whether the struct is public.
/// - `fields`: struct field syntax-output data.
///
/// Output:
/// - No return value.
///
/// Transformation:
/// - Renders docs, a Terlan struct signature fence, and field docs when
///   present.
fn render_syntax_struct_decl_docs_markdown(
    out: &mut String,
    docs: &[String],
    name: &str,
    is_public: bool,
    fields: &[SyntaxStructFieldOutput],
) {
    out.push_str(&format!("### `{}`\n\n", name));
    push_markdown_doc_block(out, docs);
    out.push_str("```terlan\n");
    out.push_str(if is_public { "pub " } else { "" });
    out.push_str(&format!("struct {} {{\n", name));
    for field in fields {
        out.push_str(&format!("    {}: {}", field.name, field.annotation.text));
        if field.has_default {
            out.push_str(" = ...");
        }
        out.push_str(",\n");
    }
    out.push_str("}.\n```\n\n");

    if fields.iter().any(|field| !field.docs.is_empty()) {
        out.push_str("#### Fields\n\n");
        for field in fields {
            out.push_str(&format!("- `{}`: `{}`", field.name, field.annotation.text));
            if !field.docs.is_empty() {
                out.push_str(" - ");
                out.push_str(&field.docs.join(" "));
            }
            out.push('\n');
        }
        out.push('\n');
    }
}

/// Appends a trait declaration documentation section.
///
/// Inputs:
/// - `out`: Markdown output buffer.
/// - `docs`: documentation lines for the trait.
/// - `name`: trait name.
/// - `params`: trait type parameters.
/// - `super_traits`: inherited trait names.
/// - `is_public`: whether the trait is public.
/// - `methods`: trait method declarations.
///
/// Output:
/// - No return value.
///
/// Transformation:
/// - Renders docs and a Terlan trait signature fence.
fn render_syntax_trait_decl_docs_markdown(
    out: &mut String,
    docs: &[String],
    name: &str,
    params: &[String],
    super_traits: &[String],
    is_public: bool,
    methods: &[SyntaxTraitMethodOutput],
) {
    out.push_str(&format!("### `{}`\n\n", name));
    push_markdown_doc_block(out, docs);
    out.push_str("```terlan\n");
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
    out.push_str(" {\n");
    out.push_str(
        &methods
            .iter()
            .map(render_syntax_trait_method_signature)
            .collect::<Vec<_>>()
            .join("\n"),
    );
    out.push_str("\n}.\n```\n\n");
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
fn render_syntax_trait_method_signature(method: &SyntaxTraitMethodOutput) -> String {
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

/// Appends an explicit trait implementation documentation section.
///
/// Inputs:
/// - `out`: Markdown output buffer.
/// - `docs`: documentation lines for the implementation declaration.
/// - `trait_ref`: trait reference being implemented.
/// - `for_type`: concrete or generic target type.
/// - `is_public`: whether the conformance is public.
/// - `methods`: implementation method declarations.
///
/// Output:
/// - No return value.
///
/// Transformation:
/// - Renders a source-shaped `impl Trait for Type` block containing method
///   signatures only, because docs should expose API shape rather than method
///   bodies.
fn render_syntax_trait_impl_docs_markdown(
    out: &mut String,
    docs: &[String],
    trait_ref: &SyntaxTypeOutput,
    for_type: &SyntaxTypeOutput,
    is_public: bool,
    methods: &[SyntaxImplMethodOutput],
) {
    out.push_str(&format!(
        "### `{} for {}`\n\n",
        trait_ref.text, for_type.text
    ));
    push_markdown_doc_block(out, docs);
    out.push_str("```terlan\n");
    out.push_str(if is_public { "pub " } else { "" });
    out.push_str("impl ");
    out.push_str(&trait_ref.text);
    out.push_str(" for ");
    out.push_str(&for_type.text);
    out.push_str(" {\n");
    for method in methods {
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
        out.push_str(".\n");
    }
    out.push_str("}.\n```\n\n");
}

/// Appends a raw declaration documentation section.
///
/// Inputs:
/// - `out`: Markdown output buffer.
/// - `docs`: documentation lines for the raw declaration.
/// - `raw_kind`: raw declaration kind.
/// - `text`: raw declaration body text.
///
/// Output:
/// - No return value.
///
/// Transformation:
/// - Renders docs and a Terlan code fence preserving raw body text.
fn render_syntax_raw_decl_docs_markdown(
    out: &mut String,
    docs: &[String],
    raw_kind: &str,
    text: &str,
) {
    out.push_str(&format!("### `{}`\n\n", raw_kind));
    push_markdown_doc_block(out, docs);
    out.push_str("```terlan\n");
    out.push_str(text);
    out.push_str(".\n```\n\n");
}

/// Appends a function declaration documentation section.
///
/// Inputs:
/// - `out`: Markdown output buffer.
/// - `docs`: documentation lines for the function.
/// - `name`: function name.
/// - `params`: syntax-output parameters.
/// - `return_type`: syntax-output return type.
/// - `is_public`: whether the function is public.
/// - `is_macro`: whether the function is a macro.
///
/// Output:
/// - No return value.
///
/// Transformation:
/// - Renders docs and a Terlan function signature fence.
fn render_syntax_function_decl_docs_markdown(
    out: &mut String,
    docs: &[String],
    name: &str,
    params: &[SyntaxParamOutput],
    return_type: &SyntaxTypeOutput,
    is_public: bool,
    is_macro: bool,
) {
    out.push_str(&format!("### `{}/{}`\n\n", name, params.len()));
    push_markdown_doc_block(out, docs);
    out.push_str("```terlan\n");
    out.push_str(if is_public { "pub " } else { "" });
    if is_macro {
        out.push_str("macro ");
    }
    out.push_str(name);
    out.push('(');
    out.push_str(
        &params
            .iter()
            .map(render_syntax_param_signature)
            .collect::<Vec<_>>()
            .join(", "),
    );
    out.push_str(&format!("): {}.\n```\n\n", return_type.text));
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
fn render_syntax_param_signature(param: &SyntaxParamOutput) -> String {
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
/// - Escapes ampersands and angle brackets.
fn sanitize_html_text(input: &str) -> String {
    let mut out = String::new();
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(ch),
        }
    }
    out
}
