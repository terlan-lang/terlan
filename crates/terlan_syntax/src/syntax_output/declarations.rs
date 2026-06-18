use super::*;

/// Converts a parsed declaration into the stable syntax-output payload.
///
/// Inputs:
/// - `declaration`: parser AST declaration.
///
/// Output:
/// - Serializable `SyntaxDeclarationPayload` for downstream compiler phases.
///
/// Transformation:
/// - Projects each declaration variant into syntax-output DTOs while preserving
///   names, visibility, spans, docs, typed annotations, and converted bodies.
pub(super) fn declaration_payload(declaration: &Decl) -> SyntaxDeclarationPayload {
    match declaration {
        Decl::Import(decl) => SyntaxDeclarationPayload::Import {
            import_kind: decl.kind.into(),
            module_name: decl.module_name.clone(),
            items: decl
                .items
                .iter()
                .map(|item| SyntaxImportItem {
                    name: item.name.clone(),
                    as_alias: item.as_alias.clone(),
                    span: item.span.into(),
                })
                .collect(),
            is_type: decl.is_type,
            source_path: decl.source_path.clone(),
        },
        Decl::Export(decl) => SyntaxDeclarationPayload::Export {
            items: decl
                .items
                .iter()
                .map(|item| SyntaxExportItem {
                    name: item.name.clone(),
                    arity: item.arity,
                    span: item.span.into(),
                })
                .collect(),
        },
        Decl::Type(decl) => SyntaxDeclarationPayload::Type {
            name: decl.name.clone(),
            params: decl.params.clone(),
            is_public: decl.is_public,
            is_opaque: decl.is_opaque,
            implements: decl.implements.iter().map(type_output).collect(),
            variants: decl.variants.iter().map(type_output).collect(),
        },
        Decl::Struct(decl) => SyntaxDeclarationPayload::Struct {
            name: decl.name.clone(),
            derives: decl.derives.clone(),
            implements: decl.implements.iter().map(type_output).collect(),
            is_public: decl.is_public,
            fields: decl
                .fields
                .iter()
                .map(|field| SyntaxStructFieldOutput {
                    name: field.name.clone(),
                    annotation: type_output(&field.annotation),
                    docs: field.docs.clone(),
                    has_default: field.default.is_some(),
                    default: field
                        .default
                        .as_ref()
                        .map(|expr| expr_output_with_span(expr, field.span.into())),
                    span: field.span.into(),
                })
                .collect(),
        },
        Decl::Constructor(decl) => SyntaxDeclarationPayload::Constructor {
            name: decl.name.clone(),
            params: decl.params.clone(),
            is_public: decl.is_public,
            clauses: decl.clauses.iter().map(constructor_clause_output).collect(),
        },
        Decl::Function(decl) => SyntaxDeclarationPayload::Function {
            name: decl.name.clone(),
            params: decl.params.iter().map(param_output).collect(),
            return_type: type_output(&decl.return_type),
            is_public: decl.is_public,
            is_macro: decl.is_macro,
            generic_bounds: decl.generic_bounds.clone(),
            clauses: decl.clauses.iter().map(function_clause_output).collect(),
        },
        Decl::Method(decl) => SyntaxDeclarationPayload::Method {
            receiver: param_output(&decl.receiver),
            name: decl.name.clone(),
            params: decl.params.iter().map(param_output).collect(),
            return_type: type_output(&decl.return_type),
            is_public: decl.is_public,
            generic_bounds: decl.generic_bounds.clone(),
            clauses: decl.clauses.iter().map(function_clause_output).collect(),
        },
        Decl::Trait(decl) => SyntaxDeclarationPayload::Trait {
            name: decl.name.clone(),
            params: decl.params.clone(),
            super_traits: decl.super_traits.clone(),
            is_public: decl.is_public,
            methods: decl.methods.iter().map(trait_method_output).collect(),
        },
        Decl::TraitImpl(decl) => SyntaxDeclarationPayload::TraitImpl {
            trait_ref: type_output(&decl.trait_ref),
            for_type: type_output(&decl.for_type),
            is_public: decl.is_public,
            methods: decl.methods.iter().map(impl_method_output).collect(),
        },
        Decl::AnnotationSchema(decl) => SyntaxDeclarationPayload::AnnotationSchema {
            path: decl.path.clone(),
            is_public: decl.is_public,
            entries: decl
                .entries
                .iter()
                .map(annotation_schema_entry_output)
                .collect(),
        },
        Decl::Template(decl) => SyntaxDeclarationPayload::Template {
            name: decl.name.clone(),
            source_path: decl.source_path.clone(),
            props: decl
                .props
                .iter()
                .map(|prop| SyntaxTemplatePropOutput {
                    name: prop.name.clone(),
                    annotation: type_output(&prop.annotation),
                    span: prop.span.into(),
                })
                .collect(),
        },
        Decl::Raw(decl) if is_config_declaration_kind(&decl.kind) => {
            SyntaxDeclarationPayload::Config {
                name: decl.kind.clone(),
                target: config_declaration_target(&decl.text),
                text: decl.text.clone(),
                entries: parse_config_entries(&decl.text),
            }
        }
        Decl::Raw(decl) => SyntaxDeclarationPayload::Raw {
            raw_kind: decl.kind.clone(),
            text: decl.text.clone(),
        },
    }
}

/// Extracts documentation comments attached to a parsed declaration.
///
/// Inputs:
/// - `declaration`: parser AST declaration.
///
/// Output:
/// - Documentation lines attached to declarations that support docs.
///
/// Transformation:
/// - Clones declaration-owned doc text and returns an empty list for imports
///   and exports, which currently do not carry source docs.
pub(super) fn declaration_docs(declaration: &Decl) -> Vec<String> {
    match declaration {
        Decl::Type(decl) => decl.docs.clone(),
        Decl::Struct(decl) => decl.docs.clone(),
        Decl::Constructor(decl) => decl.docs.clone(),
        Decl::Function(decl) => decl.docs.clone(),
        Decl::Method(decl) => decl.docs.clone(),
        Decl::Trait(decl) => decl.docs.clone(),
        Decl::TraitImpl(decl) => decl.docs.clone(),
        Decl::AnnotationSchema(decl) => decl.docs.clone(),
        Decl::Template(decl) => decl.docs.clone(),
        Decl::Raw(decl) => decl.docs.clone(),
        Decl::Import(_) | Decl::Export(_) => Vec::new(),
    }
}

/// Converts a parsed type expression into syntax-output form.
///
/// Inputs:
/// - `ty`: parser type expression with source text and span.
///
/// Output:
/// - `SyntaxTypeOutput` retaining text and span.
///
/// Transformation:
/// - Preserves canonical type text for later type parsing and diagnostics.
fn type_output(ty: &TypeExpr) -> SyntaxTypeOutput {
    SyntaxTypeOutput {
        text: ty.text.clone(),
        span: ty.span.into(),
    }
}

/// Converts a parsed function or method parameter into syntax-output form.
///
/// Inputs:
/// - `param`: parser parameter with name, annotation, mutability, and span.
///
/// Output:
/// - `SyntaxParamOutput` consumed by type checking and interface generation.
///
/// Transformation:
/// - Converts the annotation through `type_output` and preserves mutable
///   receiver/argument metadata.
fn param_output(param: &Param) -> SyntaxParamOutput {
    SyntaxParamOutput {
        name: param.name.clone(),
        annotation: type_output(&param.annotation),
        is_mutable: param.is_mutable,
        span: param.span.into(),
    }
}

/// Converts a constructor parameter into syntax-output form.
///
/// Inputs:
/// - `param`: parser constructor parameter with optional default expression.
///
/// Output:
/// - `SyntaxConstructorParamOutput` including default and varargs metadata.
///
/// Transformation:
/// - Converts the annotation and lowers any default expression into syntax
///   output using the parameter span as source context.
fn constructor_param_output(param: &ConstructorParam) -> SyntaxConstructorParamOutput {
    let span: EbnfSourceSpan = param.span.into();
    SyntaxConstructorParamOutput {
        name: param.name.clone(),
        annotation: type_output(&param.annotation),
        has_default: param.default.is_some(),
        default: param
            .default
            .as_ref()
            .map(|expr| expr_output_with_span(expr, span)),
        is_varargs: param.is_varargs,
        span: param.span.into(),
    }
}

/// Converts a constructor clause into syntax-output form.
///
/// Inputs:
/// - `clause`: parser constructor clause.
///
/// Output:
/// - `SyntaxConstructorClauseOutput` containing parameters, return type, body,
///   body text, and span.
///
/// Transformation:
/// - Converts constructor parameters and body expression into the stable
///   syntax-output layer used by type checking and Erlang lowering.
fn constructor_clause_output(clause: &ConstructorClause) -> SyntaxConstructorClauseOutput {
    let span: EbnfSourceSpan = clause.span.into();
    SyntaxConstructorClauseOutput {
        params: clause.params.iter().map(constructor_param_output).collect(),
        return_type: type_output(&clause.return_type),
        body: expr_output_with_span(&clause.body, span),
        body_text: expr_to_output_text(&clause.body),
        span: clause.span.into(),
    }
}

/// Converts a function clause into syntax-output form.
///
/// Inputs:
/// - `clause`: parser function clause with patterns, optional guard, and body.
///
/// Output:
/// - `SyntaxFunctionClauseOutput` consumed by type checking and lowering.
///
/// Transformation:
/// - Converts patterns, guard, and body into syntax-output nodes while
///   preserving whether a guard was present.
fn function_clause_output(clause: &FunctionClause) -> SyntaxFunctionClauseOutput {
    let span: EbnfSourceSpan = clause.span.into();
    SyntaxFunctionClauseOutput {
        patterns: clause.patterns.iter().map(pattern_output).collect(),
        guard: clause
            .guard
            .as_deref()
            .map(|expr| expr_output_with_span(expr, span)),
        body: expr_output_with_span(&clause.body, span),
        has_guard: clause.guard.is_some(),
        span: clause.span.into(),
    }
}

/// Converts a trait method declaration into syntax-output form.
///
/// Inputs:
/// - `method`: parser trait method declaration.
///
/// Output:
/// - `SyntaxTraitMethodOutput` with signature, optional default body, docs, and
///   span.
///
/// Transformation:
/// - Converts parameter and return type annotations and lowers default bodies
///   through normal expression syntax output.
fn trait_method_output(method: &TraitMethodDecl) -> SyntaxTraitMethodOutput {
    let span: EbnfSourceSpan = method.span.into();
    SyntaxTraitMethodOutput {
        name: method.name.clone(),
        params: method.params.iter().map(param_output).collect(),
        return_type: type_output(&method.return_type),
        generic_bounds: method.generic_bounds.clone(),
        default_body: method
            .default_body
            .as_ref()
            .map(|expr| expr_output_with_span(expr, span)),
        is_public: method.is_public,
        docs: method.docs.clone(),
        span: method.span.into(),
    }
}

/// Converts an explicit conformance method into syntax-output form.
///
/// Inputs:
/// - `method`: function declaration parsed inside an `impl` block.
///
/// Output:
/// - Serializable method summary containing signature, clauses, and span.
///
/// Transformation:
/// - Reuses function clause output so impl methods preserve the same body shape
///   as normal declarations until trait conformance lowering resolves them.
fn impl_method_output(method: &FunctionDecl) -> SyntaxImplMethodOutput {
    SyntaxImplMethodOutput {
        name: method.name.clone(),
        params: method.params.iter().map(param_output).collect(),
        return_type: type_output(&method.return_type),
        generic_bounds: method.generic_bounds.clone(),
        clauses: method.clauses.iter().map(function_clause_output).collect(),
        span: method.span.into(),
    }
}
