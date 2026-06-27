mod html;
mod signatures;

use html::render_constructor_clause_signature;
pub(crate) use html::render_syntax_module_docs_html;
use signatures::{
    render_constructor_signature, render_function_signature, render_method_signature,
    render_struct_signature, render_syntax_param_signature, render_syntax_trait_method_signature,
    render_trait_impl_signature, render_trait_signature, render_type_signature, sanitize_html_text,
};

use crate::terlan_syntax::{
    SyntaxConstructorClauseOutput, SyntaxDeclarationOutput, SyntaxDeclarationPayload,
    SyntaxImplMethodOutput, SyntaxModuleOutput, SyntaxParamOutput, SyntaxStructFieldOutput,
    SyntaxTraitMethodOutput, SyntaxTypeOutput,
};

use serde_json::{json, Value};

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
            } if *is_public => Some((
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
            } if *is_public => Some((decl.docs.as_slice(), name, is_public, fields)),
            _ => None,
        })
        .collect();
    if !structs.is_empty() {
        out.push_str("## Structs\n\n");
        for (docs, name, is_public, fields) in structs {
            render_syntax_struct_decl_docs_markdown(&mut out, docs, name, *is_public, fields);
        }
    }

    let constructors: Vec<_> = module
        .declarations
        .iter()
        .filter_map(|decl| match &decl.payload {
            SyntaxDeclarationPayload::Constructor {
                name,
                params,
                is_public,
                clauses,
            } if *is_public => Some((decl.docs.as_slice(), name, params, is_public, clauses)),
            _ => None,
        })
        .collect();
    if !constructors.is_empty() {
        out.push_str("## Constructors\n\n");
        for (docs, name, params, is_public, clauses) in constructors {
            render_syntax_constructor_decl_docs_markdown(
                &mut out, docs, name, params, *is_public, clauses,
            );
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
            } if *is_public => Some((
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
            } if *is_public => Some((
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
            } if *is_public => Some((
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

    let methods: Vec<_> = module
        .declarations
        .iter()
        .filter_map(|decl| match &decl.payload {
            SyntaxDeclarationPayload::Method {
                receiver,
                name,
                params,
                return_type,
                is_public,
                ..
            } if *is_public => Some((
                decl.docs.as_slice(),
                receiver,
                name,
                params,
                return_type,
                is_public,
            )),
            _ => None,
        })
        .collect();
    if !methods.is_empty() {
        out.push_str("## Receiver Methods\n\n");
        for (docs, receiver, name, params, return_type, is_public) in methods {
            render_syntax_method_decl_docs_markdown(
                &mut out,
                docs,
                receiver,
                name,
                params,
                return_type,
                *is_public,
            );
        }
    }

    out
}

/// Renders syntax-output module documentation as a JSON model.
///
/// Inputs:
/// - `module`: formal syntax-output module containing documentation metadata.
///
/// Output:
/// - Deterministic JSON model for documentation tooling.
///
/// Transformation:
/// - Converts module docs and source declarations into a compact
///   compiler-owned documentation model without depending on a target runtime
///   documentation generator.
pub(crate) fn render_syntax_module_docs_json(module: &SyntaxModuleOutput) -> String {
    let declarations = module
        .declarations
        .iter()
        .filter_map(render_syntax_declaration_doc_json)
        .collect::<Vec<_>>();
    let model = json!({
        "schema": "terlan-doc-module-v1",
        "module": module.module_name,
        "docs": module.docs,
        "declarations": declarations,
    });
    let mut rendered = serde_json::to_string(&model).expect("module docs JSON should serialize");
    rendered.push('\n');
    rendered
}

/// Renders one declaration into the JSON documentation model.
///
/// Inputs:
/// - `declaration`: syntax-output declaration to render.
///
/// Output:
/// - JSON object for renderable declarations.
/// - `None` for imports and exports, which are not public API docs.
///
/// Transformation:
/// - Classifies declaration kind, source-visible name, visibility, signature,
///   and attached docs into stable JSON fields.
fn render_syntax_declaration_doc_json(declaration: &SyntaxDeclarationOutput) -> Option<Value> {
    let (kind, name, is_public, signature) = match &declaration.payload {
        SyntaxDeclarationPayload::Type {
            name,
            params,
            is_public,
            is_opaque,
            variants,
            ..
        } if *is_public => (
            "type",
            name.as_str(),
            *is_public,
            render_type_signature(name, params, *is_public, *is_opaque, variants),
        ),
        SyntaxDeclarationPayload::Struct {
            name,
            is_public,
            fields,
            ..
        } if *is_public => (
            "struct",
            name.as_str(),
            *is_public,
            render_struct_signature(name, *is_public, fields),
        ),
        SyntaxDeclarationPayload::Constructor {
            name,
            params,
            is_public,
            ..
        } if *is_public => (
            "constructor",
            name.as_str(),
            *is_public,
            render_constructor_signature(name, params, *is_public),
        ),
        SyntaxDeclarationPayload::Function {
            name,
            params,
            return_type,
            is_public,
            is_macro,
            ..
        } if *is_public => (
            "function",
            name.as_str(),
            *is_public,
            render_function_signature(name, params, return_type, *is_public, *is_macro),
        ),
        SyntaxDeclarationPayload::Method {
            receiver,
            name,
            params,
            return_type,
            is_public,
            ..
        } if *is_public => (
            "method",
            name.as_str(),
            *is_public,
            render_method_signature(receiver, name, params, return_type, *is_public),
        ),
        SyntaxDeclarationPayload::Trait {
            name,
            params,
            super_traits,
            is_public,
            ..
        } if *is_public => (
            "trait",
            name.as_str(),
            *is_public,
            render_trait_signature(name, params, super_traits, *is_public),
        ),
        SyntaxDeclarationPayload::TraitImpl {
            trait_ref,
            for_type,
            is_public,
            ..
        } if *is_public => (
            "impl",
            trait_ref.text.as_str(),
            *is_public,
            render_trait_impl_signature(trait_ref, for_type, *is_public),
        ),
        SyntaxDeclarationPayload::Import { .. }
        | SyntaxDeclarationPayload::Export { .. }
        | SyntaxDeclarationPayload::Type { .. }
        | SyntaxDeclarationPayload::Struct { .. }
        | SyntaxDeclarationPayload::Constructor { .. }
        | SyntaxDeclarationPayload::Function { .. }
        | SyntaxDeclarationPayload::Method { .. }
        | SyntaxDeclarationPayload::Trait { .. }
        | SyntaxDeclarationPayload::TraitImpl { .. }
        | SyntaxDeclarationPayload::AnnotationSchema { .. }
        | SyntaxDeclarationPayload::Template { .. }
        | SyntaxDeclarationPayload::Config { .. }
        | SyntaxDeclarationPayload::Raw { .. } => return None,
    };

    Some(json!({
        "kind": kind,
        "name": name,
        "public": is_public,
        "signature": signature,
        "docs": declaration.docs,
    }))
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

/// Appends a constructor declaration documentation section.
///
/// Inputs:
/// - `out`: Markdown output buffer.
/// - `docs`: documentation lines for the constructor declaration.
/// - `name`: constructor owner type name.
/// - `params`: constructor type parameter names.
/// - `is_public`: whether the constructor is public.
/// - `clauses`: constructor clause signatures.
///
/// Output:
/// - No return value.
///
/// Transformation:
/// - Renders docs, the constructor header, and public constructor clauses as a
///   Terlan signature fence.
fn render_syntax_constructor_decl_docs_markdown(
    out: &mut String,
    docs: &[String],
    name: &str,
    params: &[String],
    is_public: bool,
    clauses: &[SyntaxConstructorClauseOutput],
) {
    out.push_str(&format!("### `{}`\n\n", name));
    push_markdown_doc_block(out, docs);
    out.push_str("```terlan\n");
    out.push_str(&render_constructor_signature(name, params, is_public));
    if !clauses.is_empty() {
        out.push('\n');
        for clause in clauses {
            out.push_str(&render_constructor_clause_signature(name, clause));
            out.push_str(".\n");
        }
    } else {
        out.push('\n');
    }
    out.push_str("```\n\n");
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

/// Appends a receiver method documentation section.
///
/// Inputs:
/// - `out`: Markdown output buffer.
/// - `docs`: documentation lines for the method.
/// - `receiver`: method receiver parameter.
/// - `name`: method name.
/// - `params`: method call parameters.
/// - `return_type`: return type.
/// - `is_public`: whether the method is public.
///
/// Output:
/// - No return value.
///
/// Transformation:
/// - Renders docs and a Terlan receiver method signature fence.
fn render_syntax_method_decl_docs_markdown(
    out: &mut String,
    docs: &[String],
    receiver: &SyntaxParamOutput,
    name: &str,
    params: &[SyntaxParamOutput],
    return_type: &SyntaxTypeOutput,
    is_public: bool,
) {
    out.push_str(&format!(
        "### `{}.{}({})`\n\n",
        receiver.annotation.text,
        name,
        params.len()
    ));
    push_markdown_doc_block(out, docs);
    out.push_str("```terlan\n");
    out.push_str(&render_method_signature(
        receiver,
        name,
        params,
        return_type,
        is_public,
    ));
    out.push_str("\n```\n\n");
}

#[cfg(test)]
#[path = "render_test.rs"]
mod render_test;
