mod html;
#[path = "render/method_rendering.rs"]
mod method_rendering;
mod signatures;
mod support;

use std::collections::HashSet;

use html::render_constructor_clause_signature;
pub(crate) use html::render_syntax_module_docs_html;
use method_rendering::render_syntax_method_decl_docs_markdown;
use signatures::{
    render_constructor_signature, render_function_signature, render_method_signature,
    render_purity_marked_signature, render_raw_shape_signature, render_struct_signature,
    render_syntax_param_signature, render_syntax_trait_method_signature,
    render_trait_impl_signature, render_trait_signature, render_type_signature,
};
use support::declaration_is_compiler_pure;

use crate::terlan_syntax::{
    SyntaxConstructorClauseOutput, SyntaxDeclarationOutput, SyntaxDeclarationPayload,
    SyntaxImplMethodOutput, SyntaxModuleOutput, SyntaxParamOutput, SyntaxStructFieldOutput,
    SyntaxTraitMethodOutput, SyntaxTypeOutput,
};

use serde_json::{json, Value};

#[path = "render/const_expr.rs"]
mod const_expr;
use const_expr::render_const_expr_text;

use crate::terlan_purity::{infer_body_available_pure_callables, CallableIdentity};

struct SyntaxTypeDocumentation<'a> {
    name: &'a str,
    params: &'a [String],
    is_public: bool,
    is_opaque: bool,
    variants: &'a [SyntaxTypeOutput],
    representation: Option<&'a SyntaxTypeOutput>,
    valued_arms: &'a [crate::terlan_syntax::SyntaxValuedUnionArmOutput],
}

struct SyntaxTraitImplDocumentation<'a> {
    trait_ref: &'a SyntaxTypeOutput,
    generic_params: &'a [String],
    for_type: &'a SyntaxTypeOutput,
    is_negative: bool,
    is_public: bool,
    methods: &'a [SyntaxImplMethodOutput],
}

pub(super) struct SyntaxCallableDocumentation<'a> {
    pub(super) name: &'a str,
    pub(super) params: &'a [SyntaxParamOutput],
    pub(super) return_type: &'a SyntaxTypeOutput,
    pub(super) is_public: bool,
    pub(super) is_pure: bool,
}

/// Renders public syntax declarations and their metadata as Markdown.
pub(crate) fn render_syntax_module_docs_markdown(module: &SyntaxModuleOutput) -> String {
    let known_pure = infer_body_available_pure_callables(module);
    let mut out = String::new();
    out.push_str(&format!("# `{}`\n\n", module.module_name));
    push_markdown_doc_block(&mut out, &module.docs);

    let constants = module
        .declarations
        .iter()
        .filter_map(|decl| match &decl.payload {
            SyntaxDeclarationPayload::Constant {
                name,
                annotation,
                value,
                is_public: true,
            } => Some((decl.docs.as_slice(), name, annotation, value)),
            _ => None,
        })
        .collect::<Vec<_>>();
    if !constants.is_empty() {
        out.push_str("## Constants\n\n");
        for (docs, name, annotation, value) in constants {
            out.push_str(&format!("### `{name}`\n\n"));
            push_markdown_doc_block(&mut out, docs);
            out.push_str(&format!(
                "```terlan\npub const {name}: {} = {}.\n```\n\n",
                annotation.text,
                render_const_expr_text(value)
            ));
        }
    }

    let const_functions = module
        .declarations
        .iter()
        .filter_map(|decl| match &decl.payload {
            SyntaxDeclarationPayload::ConstFunction {
                name,
                params,
                return_type,
                is_public: true,
                ..
            } => Some((decl.docs.as_slice(), name, params, return_type)),
            _ => None,
        })
        .collect::<Vec<_>>();
    if !const_functions.is_empty() {
        out.push_str("## Const Functions\n\n");
        for (docs, name, params, return_type) in const_functions {
            out.push_str(&format!("### `{name}`\n\n"));
            push_markdown_doc_block(&mut out, docs);
            out.push_str(&format!(
                "```terlan\npub const {}\n```\n\n",
                render_function_signature(name, params, return_type, false, false)
            ));
        }
    }

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
                representation,
                valued_arms,
                ..
            } if *is_public => Some((
                decl.docs.as_slice(),
                name,
                params,
                is_public,
                is_opaque,
                variants,
                representation,
                valued_arms,
            )),
            _ => None,
        })
        .collect();
    if !types.is_empty() {
        out.push_str("## Types\n\n");
        for (docs, name, params, is_public, is_opaque, variants, representation, valued_arms) in
            types
        {
            render_syntax_type_decl_docs_markdown(
                &mut out,
                docs,
                SyntaxTypeDocumentation {
                    name,
                    params,
                    is_public: *is_public,
                    is_opaque: *is_opaque,
                    variants,
                    representation: representation.as_ref(),
                    valued_arms,
                },
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

    let shapes: Vec<_> = module
        .declarations
        .iter()
        .filter_map(|decl| match &decl.payload {
            SyntaxDeclarationPayload::Raw { raw_kind, text } => {
                let (name, is_public, signature) = render_raw_shape_signature(raw_kind, text)?;
                is_public.then_some((decl.docs.as_slice(), name, signature))
            }
            _ => None,
        })
        .collect();
    if !shapes.is_empty() {
        out.push_str("## Shapes\n\n");
        for (docs, name, signature) in shapes {
            render_syntax_shape_decl_docs_markdown(&mut out, docs, &name, &signature);
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
                ..
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
                generic_params,
                for_type,
                is_negative,
                is_public,
                methods,
                ..
            } if *is_public => Some((
                decl.docs.as_slice(),
                trait_ref,
                generic_params.as_slice(),
                for_type,
                is_negative,
                is_public,
                methods.as_slice(),
            )),
            _ => None,
        })
        .collect();
    if !trait_impls.is_empty() {
        out.push_str("## Trait Implementations\n\n");
        for (docs, trait_ref, generic_params, for_type, is_negative, is_public, methods) in
            trait_impls
        {
            render_syntax_trait_impl_docs_markdown(
                &mut out,
                docs,
                SyntaxTraitImplDocumentation {
                    trait_ref,
                    generic_params,
                    for_type,
                    is_negative: *is_negative,
                    is_public: *is_public,
                    methods,
                },
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
                declaration_is_compiler_pure(decl, &known_pure),
            )),
            _ => None,
        })
        .collect();
    if !functions.is_empty() {
        out.push_str("## Functions\n\n");
        for (docs, name, params, return_type, is_public, is_macro, is_pure) in functions {
            render_syntax_function_decl_docs_markdown(
                &mut out,
                docs,
                SyntaxCallableDocumentation {
                    name,
                    params,
                    return_type,
                    is_public: *is_public,
                    is_pure,
                },
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
                declaration_is_compiler_pure(decl, &known_pure),
            )),
            _ => None,
        })
        .collect();
    if !methods.is_empty() {
        out.push_str("## Receiver Methods\n\n");
        for (docs, receiver, name, params, return_type, is_public, is_pure) in methods {
            render_syntax_method_decl_docs_markdown(
                &mut out,
                docs,
                receiver,
                SyntaxCallableDocumentation {
                    name,
                    params,
                    return_type,
                    is_public: *is_public,
                    is_pure,
                },
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
    let known_pure = infer_body_available_pure_callables(module);
    let declarations = module
        .declarations
        .iter()
        .filter_map(|declaration| render_syntax_declaration_doc_json(declaration, &known_pure))
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
fn render_syntax_declaration_doc_json(
    declaration: &SyntaxDeclarationOutput,
    known_pure: &HashSet<CallableIdentity>,
) -> Option<Value> {
    if let SyntaxDeclarationPayload::Raw { raw_kind, text } = &declaration.payload {
        let (name, is_public, signature) = render_raw_shape_signature(raw_kind, text)?;
        if !is_public {
            return None;
        }
        return Some(json!({
            "kind": "shape",
            "name": name,
            "public": is_public,
            "signature": signature,
            "docs": declaration.docs,
        }));
    }

    let (kind, name, is_public, signature) = match &declaration.payload {
        SyntaxDeclarationPayload::Constant {
            name,
            annotation,
            value,
            is_public,
        } if *is_public => (
            "constant",
            name.as_str(),
            true,
            format!(
                "pub const {}: {} = {}.",
                name,
                annotation.text,
                render_const_expr_text(value)
            ),
        ),
        SyntaxDeclarationPayload::ConstFunction {
            name,
            params,
            return_type,
            is_public,
            ..
        } if *is_public => (
            "const_function",
            name.as_str(),
            true,
            format!(
                "pub const {}({}): {}.",
                name,
                params
                    .iter()
                    .map(|param| format!("{}: {}", param.name, param.annotation.text))
                    .collect::<Vec<_>>()
                    .join(", "),
                return_type.text
            ),
        ),
        SyntaxDeclarationPayload::Type {
            name,
            params,
            is_public,
            is_opaque,
            variants,
            representation,
            valued_arms,
            ..
        } if *is_public => (
            "type",
            name.as_str(),
            *is_public,
            render_type_signature(
                name,
                params,
                *is_public,
                *is_opaque,
                variants,
                representation.as_ref(),
                valued_arms,
            ),
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
            render_purity_marked_signature(
                declaration_is_compiler_pure(declaration, known_pure),
                render_function_signature(name, params, return_type, *is_public, *is_macro),
            ),
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
            render_purity_marked_signature(
                declaration_is_compiler_pure(declaration, known_pure),
                render_method_signature(receiver, name, params, return_type, *is_public),
            ),
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
            generic_params,
            for_type,
            is_negative,
            is_public,
            ..
        } if *is_public => (
            "impl",
            trait_ref.text.as_str(),
            *is_public,
            render_trait_impl_signature(
                trait_ref,
                generic_params,
                for_type,
                *is_negative,
                *is_public,
            ),
        ),
        SyntaxDeclarationPayload::Import { .. }
        | SyntaxDeclarationPayload::Export { .. }
        | SyntaxDeclarationPayload::Constant { .. }
        | SyntaxDeclarationPayload::ConstFunction { .. }
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

/// Renders Markdown documentation for a shape declaration.
///
/// Inputs:
/// - `out`: Markdown output buffer.
/// - `docs`: documentation lines attached to the shape.
/// - `name`: shape name.
/// - `signature`: source-shaped raw shape signature.
///
/// Output:
/// - No return value.
///
/// Transformation:
/// - Emits a public shape API section without requiring semantic expansion.
fn render_syntax_shape_decl_docs_markdown(
    out: &mut String,
    docs: &[String],
    name: &str,
    signature: &str,
) {
    out.push_str(&format!("### `{}`\n\n", name));
    push_markdown_doc_block(out, docs);
    out.push_str("```terlan\n");
    out.push_str(signature);
    out.push_str("\n```\n\n");
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
    declaration: SyntaxTypeDocumentation<'_>,
) {
    let SyntaxTypeDocumentation {
        name,
        params,
        is_public,
        is_opaque,
        variants,
        representation,
        valued_arms,
    } = declaration;
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
    if let Some(representation) = representation {
        out.push_str(": ");
        out.push_str(&representation.text);
        out.push_str(" = ");
        out.push_str(
            &valued_arms
                .iter()
                .map(|arm| format!("{} = {}", arm.name, render_const_expr_text(&arm.value)))
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
    declaration: SyntaxTraitImplDocumentation<'_>,
) {
    let SyntaxTraitImplDocumentation {
        trait_ref,
        generic_params,
        for_type,
        is_negative,
        is_public,
        methods,
    } = declaration;
    let rendered_trait_ref =
        crate::terlan_syntax::render_trait_impl_ref(&trait_ref.text, generic_params);
    let relationship = if is_negative {
        format!("not {}[{}]", trait_ref.text, for_type.text)
    } else {
        format!("{rendered_trait_ref} for {}", for_type.text)
    };
    out.push_str(&format!("### `{relationship}`\n\n"));
    push_markdown_doc_block(out, docs);
    out.push_str("```terlan\n");
    if is_negative {
        out.push_str(&render_trait_impl_signature(
            trait_ref,
            generic_params,
            for_type,
            true,
            is_public,
        ));
        out.push_str("\n```\n\n");
        return;
    }
    out.push_str(if is_public { "pub impl " } else { "impl " });
    out.push_str(&rendered_trait_ref);
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
/// - `is_pure`: whether the function carries marker-only `@pure`.
///
/// Output:
/// - No return value.
///
/// Transformation:
/// - Renders docs and a Terlan function signature fence.
fn render_syntax_function_decl_docs_markdown(
    out: &mut String,
    docs: &[String],
    declaration: SyntaxCallableDocumentation<'_>,
    is_macro: bool,
) {
    let SyntaxCallableDocumentation {
        name,
        params,
        return_type,
        is_public,
        is_pure,
    } = declaration;
    out.push_str(&format!("### `{}/{}`\n\n", name, params.len()));
    push_markdown_doc_block(out, docs);
    out.push_str("```terlan\n");
    out.push_str(&render_purity_marked_signature(
        is_pure,
        render_function_signature(name, params, return_type, is_public, is_macro),
    ));
    out.push_str("\n```\n\n");
}

#[cfg(test)]
#[path = "render_test.rs"]
#[cfg(test)]
mod render_test;
