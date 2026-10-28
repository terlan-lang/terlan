//! Shared dependency discovery over serialized syntax expressions and types.

use super::{
    syntax_module_import_identities, SyntaxDeclarationPayload, SyntaxExprOutput,
    SyntaxFunctionClauseOutput, SyntaxImportKind, SyntaxModuleOutput, SyntaxParamOutput,
};
use std::collections::BTreeSet;

pub(crate) fn collect_type_modules(text: &str, modules: &mut BTreeSet<String>) {
    let mut quoted = false;
    let mut escaped = false;
    let mut token = String::new();
    for ch in text.chars().chain(std::iter::once(' ')) {
        if quoted {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                quoted = false;
            }
            continue;
        }
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '.' {
            token.push(ch);
        } else {
            if let Some((module, name)) = token.rsplit_once('.') {
                if name.starts_with(|ch: char| ch.is_ascii_uppercase()) {
                    modules.insert(module.to_string());
                    // A qualified head may itself name the module's primary type.
                    // Only providers present in the interface catalog are admitted.
                    if module.rsplit('.').next() != Some(name) {
                        modules.insert(token.clone());
                    }
                }
            }
            token.clear();
            quoted = ch == '"';
        }
    }
}

/// Adds fully qualified remote modules referenced without an import alias.
pub(crate) fn collect_remote_modules(module: &SyntaxModuleOutput, modules: &mut BTreeSet<String>) {
    for declaration in &module.declarations {
        match &declaration.payload {
            SyntaxDeclarationPayload::Constant { value, .. } => {
                collect_expr_modules(value, modules)
            }
            SyntaxDeclarationPayload::ConstFunction { params, body, .. } => {
                collect_param_modules(params, modules);
                collect_expr_modules(body, modules);
            }
            SyntaxDeclarationPayload::Type { valued_arms, .. } => valued_arms
                .iter()
                .for_each(|arm| collect_expr_modules(&arm.value, modules)),
            SyntaxDeclarationPayload::Struct { fields, .. } => fields
                .iter()
                .filter_map(|field| field.default.as_ref())
                .for_each(|value| collect_expr_modules(value, modules)),
            SyntaxDeclarationPayload::Constructor { clauses, .. } => {
                for clause in clauses {
                    clause
                        .params
                        .iter()
                        .filter_map(|param| param.default.as_ref())
                        .for_each(|value| collect_expr_modules(value, modules));
                    collect_expr_modules(&clause.body, modules);
                }
            }
            SyntaxDeclarationPayload::Function {
                params, clauses, ..
            }
            | SyntaxDeclarationPayload::Method {
                params, clauses, ..
            } => {
                collect_param_modules(params, modules);
                collect_clause_modules(clauses, modules);
            }
            SyntaxDeclarationPayload::Trait {
                methods, constants, ..
            } => {
                for method in methods {
                    collect_param_modules(&method.params, modules);
                    if let Some(body) = method.default_body.as_ref() {
                        collect_expr_modules(body, modules);
                    }
                }
                constants
                    .iter()
                    .filter_map(|constant| constant.default.as_ref())
                    .for_each(|value| collect_expr_modules(value, modules));
            }
            SyntaxDeclarationPayload::TraitImpl {
                methods, constants, ..
            } => {
                for method in methods {
                    collect_param_modules(&method.params, modules);
                    collect_clause_modules(&method.clauses, modules);
                }
                constants
                    .iter()
                    .for_each(|constant| collect_expr_modules(&constant.value, modules));
            }
            SyntaxDeclarationPayload::Template { props, .. } => props
                .iter()
                .filter_map(|prop| prop.default.as_ref())
                .for_each(|value| collect_expr_modules(value, modules)),
            _ => {}
        }
    }
}

fn collect_param_modules(params: &[SyntaxParamOutput], modules: &mut BTreeSet<String>) {
    for param in params {
        collect_type_modules(&param.annotation.text, modules);
    }
    params
        .iter()
        .filter_map(|param| param.default.as_ref())
        .for_each(|value| collect_expr_modules(value, modules));
}

fn collect_clause_modules(clauses: &[SyntaxFunctionClauseOutput], modules: &mut BTreeSet<String>) {
    for clause in clauses {
        if let Some(guard) = clause.guard.as_ref() {
            collect_expr_modules(guard, modules);
        }
        collect_expr_modules(&clause.body, modules);
    }
}

fn collect_expr_modules(expr: &SyntaxExprOutput, modules: &mut BTreeSet<String>) {
    if let Some(module) = expr.remote.as_ref() {
        modules.insert(module.clone());
    }
    expr.children
        .iter()
        .for_each(|child| collect_expr_modules(child, modules));
    expr.let_guards
        .iter()
        .flatten()
        .for_each(|guard| collect_expr_modules(guard, modules));
    expr.fields
        .iter()
        .for_each(|field| collect_expr_modules(&field.value, modules));
    expr.clauses.iter().for_each(|clause| {
        if let Some(guard) = clause.guard.as_ref() {
            collect_expr_modules(guard, modules);
        }
        collect_expr_modules(&clause.body, modules);
    });
    expr.catch_clauses.iter().for_each(|clause| {
        if let Some(guard) = clause.guard.as_ref() {
            collect_expr_modules(guard, modules);
        }
        collect_expr_modules(&clause.body, modules);
    });
    if let Some(after) = expr.try_after.as_ref() {
        collect_expr_modules(&after.trigger, modules);
        collect_expr_modules(&after.body, modules);
    }
}

/// Source-declared dependency identities, including default imports and remote calls.
pub(crate) fn syntax_dependency_modules(module: &SyntaxModuleOutput) -> BTreeSet<String> {
    let mut required = syntax_module_import_identities(module)
        .into_iter()
        .collect::<BTreeSet<_>>();
    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::Import {
            import_kind: SyntaxImportKind::Module,
            module_name,
            items,
            ..
        } = &declaration.payload
        else {
            continue;
        };
        required.extend(
            items
                .iter()
                .map(|item| format!("{module_name}.{}", item.name)),
        );
    }
    collect_remote_modules(module, &mut required);
    collect_annotation_modules(module, &mut required);
    required
}

/// Qualified annotation references are dependencies even without an import.
fn collect_annotation_modules(module: &SyntaxModuleOutput, modules: &mut BTreeSet<String>) {
    for declaration in &module.declarations {
        let mut texts = Vec::new();
        match &declaration.payload {
            SyntaxDeclarationPayload::Constant { annotation, .. } => {
                texts.push(annotation.text.as_str())
            }
            SyntaxDeclarationPayload::ConstFunction { return_type, .. } => {
                texts.push(return_type.text.as_str())
            }
            SyntaxDeclarationPayload::Type {
                params,
                variants,
                representation,
                implements,
                ..
            } => {
                texts.extend(params.iter().map(String::as_str));
                texts.extend(
                    variants
                        .iter()
                        .chain(representation)
                        .chain(implements)
                        .map(|ty| ty.text.as_str()),
                );
            }
            SyntaxDeclarationPayload::Struct {
                generic_params,
                includes,
                implements,
                fields,
                ..
            } => {
                texts.extend(generic_params.iter().chain(includes).map(String::as_str));
                texts.extend(implements.iter().map(|ty| ty.text.as_str()));
                texts.extend(fields.iter().map(|field| field.annotation.text.as_str()));
            }
            SyntaxDeclarationPayload::Constructor {
                params, clauses, ..
            } => {
                texts.extend(params.iter().map(String::as_str));
                for clause in clauses {
                    texts.push(clause.return_type.text.as_str());
                    texts.extend(
                        clause
                            .params
                            .iter()
                            .map(|param| param.annotation.text.as_str()),
                    );
                }
            }
            SyntaxDeclarationPayload::Function {
                generic_params,
                generic_bounds,
                return_type,
                ..
            }
            | SyntaxDeclarationPayload::Method {
                generic_params,
                generic_bounds,
                return_type,
                ..
            } => {
                texts.extend(
                    generic_params
                        .iter()
                        .chain(generic_bounds)
                        .map(String::as_str),
                );
                texts.push(return_type.text.as_str());
                if let SyntaxDeclarationPayload::Method { receiver, .. } = &declaration.payload {
                    texts.push(receiver.annotation.text.as_str());
                }
            }
            SyntaxDeclarationPayload::Trait {
                params,
                super_traits,
                methods,
                constants,
                ..
            } => {
                texts.extend(params.iter().chain(super_traits).map(String::as_str));
                for method in methods {
                    texts.extend(method.generic_params.iter().map(String::as_str));
                    texts.push(method.return_type.text.as_str());
                }
                texts.extend(
                    constants
                        .iter()
                        .map(|constant| constant.annotation.text.as_str()),
                );
            }
            SyntaxDeclarationPayload::TraitImpl {
                generic_params,
                trait_ref,
                for_type,
                methods,
                constants,
                ..
            } => {
                texts.extend(generic_params.iter().map(String::as_str));
                texts.extend([trait_ref.text.as_str(), for_type.text.as_str()]);
                texts.extend(
                    methods
                        .iter()
                        .map(|method| method.return_type.text.as_str()),
                );
                texts.extend(
                    constants
                        .iter()
                        .filter_map(|constant| constant.annotation.as_ref())
                        .map(|annotation| annotation.text.as_str()),
                );
            }
            SyntaxDeclarationPayload::Template { props, .. } => {
                texts.extend(props.iter().map(|prop| prop.annotation.text.as_str()))
            }
            _ => {}
        }
        for text in texts {
            collect_type_modules(text, modules);
        }
    }
}
