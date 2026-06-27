use super::{format_docs, format_expr, format_pattern, format_type_expr};
use crate::terlan_syntax::parse_tree::{
    ConstructorDecl, ConstructorParam, FunctionClause, FunctionDecl, MethodDecl, Param, Pattern,
    StructDecl, StructFieldDecl, TraitDecl, TraitImplDecl, TypeDecl, TypeExpr, UnsupportedDecl,
};

/// Formats a type declaration.
///
/// Inputs: parsed type declaration. Output: canonical type source text.
/// Transformation: emits visibility, opacity, type parameters, implementation
/// clauses, and union variants in source order.
pub(super) fn format_type_decl(ty: &TypeDecl) -> String {
    let mut out = String::new();
    if ty.is_public {
        out.push_str("pub ");
    }

    if ty.is_opaque {
        out.push_str("opaque ");
    }

    out.push_str("type ");
    out.push_str(&ty.name);

    if !ty.params.is_empty() {
        out.push('[');
        out.push_str(&ty.params.join(", "));
        out.push(']');
    }

    if !ty.implements.is_empty() {
        out.push_str(" implements ");
        out.push_str(
            &ty.implements
                .iter()
                .map(format_type_expr)
                .collect::<Vec<_>>()
                .join(", "),
        );
    }

    if ty.variants.is_empty() {
        out.push('.');
        return out;
    }

    out.push_str(" =\n");
    for (i, variant) in ty.variants.iter().enumerate() {
        if i == 0 {
            out.push_str("      ");
            out.push_str(&format_type_expr(variant));
        } else {
            out.push_str("\n    | ");
            out.push_str(&format_type_expr(variant));
        }
    }
    out.push('.');
    out
}

/// Formats a struct declaration.
///
/// Inputs: parsed struct declaration. Output: canonical struct source text.
/// Transformation: emits visibility, includes/implements clauses, and fields in
/// source order.
pub(super) fn format_struct_decl(decl: &StructDecl) -> String {
    let mut out = String::new();
    if decl.is_public {
        out.push_str("pub ");
    }
    out.push_str("struct ");
    out.push_str(&decl.name);
    if !decl.includes.is_empty() {
        out.push_str(" includes ");
        out.push_str(&decl.includes.join(", "));
    }
    if !decl.implements.is_empty() {
        out.push_str(" implements ");
        out.push_str(
            &decl
                .implements
                .iter()
                .map(format_type_expr)
                .collect::<Vec<_>>()
                .join(", "),
        );
    }
    out.push_str(" {\n");

    for field in &decl.fields {
        let docs = format_docs(&field.docs, 1);
        if !docs.is_empty() {
            out.push_str(&docs);
            out.push('\n');
        }
        out.push_str("    ");
        out.push_str(&format_struct_field(field));
        out.push('\n');
    }
    out.push_str("}.");
    out
}

/// Formats a struct field.
///
/// Inputs: parsed struct field. Output: source field text. Transformation:
/// emits name/type and optional default expression.
fn format_struct_field(field: &StructFieldDecl) -> String {
    let mut out = String::new();
    if field.is_private {
        out.push('#');
    }
    out.push_str(&field.name);
    out.push_str(": ");
    out.push_str(&format_type_expr(&field.annotation));
    if let Some(default) = &field.default {
        out.push_str(" = ");
        out.push_str(&format_expr(default, 0));
    }
    out
}

/// Formats a constructor declaration.
///
/// Inputs: parsed constructor declaration. Output: canonical constructor block.
/// Transformation: emits visibility, type params, clauses, params, return
/// types, and bodies with stable separators.
pub(super) fn format_constructor_decl(decl: &ConstructorDecl) -> String {
    let mut out = String::new();
    if decl.is_public {
        out.push_str("pub ");
    }
    out.push_str("constructor ");
    out.push_str(&decl.name);
    if !decl.params.is_empty() {
        out.push('[');
        out.push_str(&decl.params.join(", "));
        out.push(']');
    }
    out.push_str(" {\n");

    for (index, clause) in decl.clauses.iter().enumerate() {
        out.push_str("    (");
        out.push_str(
            &clause
                .params
                .iter()
                .map(format_constructor_param)
                .collect::<Vec<_>>()
                .join(", "),
        );
        out.push_str("): ");
        out.push_str(&format_type_expr(&clause.return_type));
        out.push_str(" ->\n        ");
        out.push_str(&format_expr(&clause.body, 2));
        if index + 1 < decl.clauses.len() {
            out.push_str(";\n\n");
        } else {
            out.push('\n');
        }
    }

    out.push_str("}.");
    out
}

/// Formats one constructor parameter.
///
/// Inputs: parsed constructor parameter. Output: source parameter text.
/// Transformation: emits varargs marker, name/type, and optional default.
fn format_constructor_param(param: &ConstructorParam) -> String {
    let mut out = String::new();
    if param.is_varargs {
        out.push_str("...");
    }
    out.push_str(&param.name);
    out.push_str(": ");
    out.push_str(&format_type_expr(&param.annotation));
    if let Some(default) = &param.default {
        out.push_str(" = ");
        out.push_str(&format_expr(default, 0));
    }
    out
}

/// Formats a function declaration.
///
/// Inputs: parsed function declaration. Output: canonical function source.
/// Transformation: handles bodyless signatures, single-clause inline bodies,
/// and multi-clause function bodies.
pub(super) fn format_function(function: &FunctionDecl) -> String {
    let mut out = String::new();
    if function.is_public {
        out.push_str("pub ");
        if function.is_macro {
            out.push_str("macro ");
        }
    }

    if function.clauses.is_empty() {
        out.push_str(&format_function_signature(
            &function.name,
            &function.params,
            &function.return_type,
        ));
        out.push('.');
        return out;
    }

    if function.clauses.len() == 1 && single_clause_matches_header(function) {
        out.push_str(&format_function_signature(
            &function.name,
            &function.params,
            &function.return_type,
        ));
        out.push_str(" ->\n    ");
        out.push_str(&format_expr(&function.clauses[0].body, 1));
        out.push('.');
        return out;
    }

    out.push_str(&format_function_signature(
        &function.name,
        &function.params,
        &function.return_type,
    ));
    out.push('.');
    out.push('\n');

    for (i, clause) in function.clauses.iter().enumerate() {
        out.push_str(&format_function_clause(function, clause));
        if i + 1 < function.clauses.len() {
            out.push_str(";\n");
        }
    }

    if !out.ends_with('.') {
        out.push('.');
    }

    out
}

/// Formats a receiver-method declaration.
///
/// Inputs:
/// - `method`: structured method declaration containing receiver, method
///   params, return type, and body clauses.
///
/// Output:
/// - Canonical Terlan receiver-method source text.
///
/// Transformation:
/// - Renders the receiver as `(name: Type)` or `(mut name: Type)` before the
///   method name and formats the first body clause as a declaration body.
///   Multi-clause receiver methods are not currently produced by the parser, so
///   only the first clause is emitted.
pub(super) fn format_method(method: &MethodDecl) -> String {
    let mut out = String::new();
    if method.is_public {
        out.push_str("pub ");
    }
    out.push('(');
    if method.receiver.is_mutable {
        out.push_str("mut ");
    }
    out.push_str(&method.receiver.name);
    out.push_str(": ");
    out.push_str(&format_type_expr(&method.receiver.annotation));
    out.push_str(") ");
    out.push_str(&format_function_signature(
        &method.name,
        &method.params,
        &method.return_type,
    ));
    if let Some(clause) = method.clauses.first() {
        out.push_str(" ->\n    ");
        out.push_str(&format_expr(&clause.body, 1));
    }
    out.push('.');
    out
}

/// Formats a function signature.
///
/// Inputs: function name, params, and return type. Output: `name(params):
/// Type` text. Transformation: formats params in source order and normalizes
/// type expressions through `format_type_expr`.
pub(super) fn format_function_signature(name: &str, params: &[Param], ret: &TypeExpr) -> String {
    let mut out = String::new();
    out.push_str(name);
    out.push('(');
    out.push_str(
        &params
            .iter()
            .map(|param| format!("{}: {}", param.name, format_type_expr(&param.annotation)))
            .collect::<Vec<_>>()
            .join(", "),
    );
    out.push(')');
    out.push_str(": ");
    out.push_str(&format_type_expr(ret));
    out
}

/// Formats a trait declaration.
///
/// Inputs: parsed trait declaration. Output: canonical trait source text.
/// Transformation: emits visibility, params, super traits, method signatures,
/// and default bodies.
pub(super) fn format_trait_decl(trait_decl: &TraitDecl) -> String {
    let mut out = String::new();
    if trait_decl.is_public {
        out.push_str("pub ");
    }
    out.push_str("trait ");
    out.push_str(&trait_decl.name);
    if !trait_decl.params.is_empty() {
        out.push('[');
        out.push_str(&trait_decl.params.join(", "));
        out.push(']');
    }
    if !trait_decl.super_traits.is_empty() {
        out.push_str(" extends ");
        out.push_str(&trait_decl.super_traits.join(", "));
    }
    out.push_str(" {\n");
    for method in &trait_decl.methods {
        let docs = format_docs(&method.docs, 1);
        if !docs.is_empty() {
            out.push_str(&docs);
            out.push('\n');
        }
        out.push_str("    ");
        out.push_str(&method.name);
        if !method.params.is_empty() {
            out.push('(');
            out.push_str(
                &method
                    .params
                    .iter()
                    .map(|param| format!("{}: {}", param.name, param.annotation.text))
                    .collect::<Vec<_>>()
                    .join(", "),
            );
            out.push(')');
        } else {
            out.push_str("()");
        }
        out.push_str(": ");
        out.push_str(&method.return_type.text);
        if let Some(default_body) = &method.default_body {
            out.push_str(" ->\n        ");
            out.push_str(&format_expr(default_body, 2));
        }
        out.push_str(".\n");
    }
    out.push_str("}.");
    out
}

/// Formats an explicit trait conformance declaration.
///
/// Inputs:
/// - `trait_impl`: parsed conformance block with trait reference, target type,
///   visibility, and method declarations.
///
/// Output:
/// - Canonical `impl TraitRef for Type { ... }.` source text.
///
/// Transformation:
/// - Renders each implementation method using the normal function formatter
///   without declaration-site `pub`, because visibility belongs to the impl
///   declaration itself.
pub(super) fn format_trait_impl_decl(trait_impl: &TraitImplDecl) -> String {
    let mut out = String::new();
    if trait_impl.is_public {
        out.push_str("pub ");
    }
    out.push_str("impl ");
    out.push_str(&format_type_expr(&trait_impl.trait_ref));
    out.push_str(" for ");
    out.push_str(&format_type_expr(&trait_impl.for_type));
    out.push_str(" {\n");
    for method in &trait_impl.methods {
        let docs = format_docs(&method.docs, 1);
        if !docs.is_empty() {
            out.push_str(&docs);
            out.push('\n');
        }
        let mut method = method.clone();
        method.is_public = false;
        method.docs.clear();
        for line in format_function(&method).lines() {
            out.push_str("    ");
            out.push_str(line);
            out.push('\n');
        }
    }
    out.push_str("}.");
    out
}

/// Formats a raw/unsupported declaration.
///
/// Inputs: raw declaration payload. Output: raw text with terminating period.
/// Transformation: preserves raw declaration text exactly apart from appending
/// the declaration terminator.
pub(super) fn format_raw_decl(raw: &UnsupportedDecl) -> String {
    format!("{}.", raw.text)
}

/// Formats one multi-clause function clause.
///
/// Inputs: parent function metadata and parsed clause. Output: source clause
/// text. Transformation: uses the parent function name and clause patterns,
/// optional guard, and body.
pub(super) fn format_function_clause(function: &FunctionDecl, clause: &FunctionClause) -> String {
    let mut out = String::new();
    out.push_str(&function.name);
    out.push('(');
    out.push_str(
        &clause
            .patterns
            .iter()
            .map(format_pattern)
            .collect::<Vec<_>>()
            .join(", "),
    );
    out.push(')');

    if let Some(guard) = &clause.guard {
        out.push(' ');
        out.push_str("when");
        out.push(' ');
        out.push_str(&format_expr(guard, 1));
    }

    out.push_str(" ->\n    ");
    out.push_str(&format_expr(&clause.body, 1));

    out
}

/// Returns whether a single function clause duplicates the declaration header.
///
/// Inputs: parsed function declaration. Output: `true` when the first clause
/// patterns are exactly the header parameter names. Transformation: compares
/// clause variables with params to decide compact formatting.
fn single_clause_matches_header(function: &FunctionDecl) -> bool {
    let Some(clause) = function.clauses.first() else {
        return false;
    };

    if clause.patterns.len() != function.params.len() {
        return false;
    }

    clause
        .patterns
        .iter()
        .zip(function.params.iter())
        .all(|(pattern, param)| match pattern {
            Pattern::Var(name) => name == &param.name,
            _ => false,
        })
}
