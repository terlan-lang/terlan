use super::let_else::format_function_body_let;
use super::{
    format_docs, format_expr, format_pattern, format_statement_parts, format_type_expr,
    DEFAULT_MAX_LINE_LENGTH,
};
use crate::terlan_syntax::parse_tree::{
    ConstFunctionDecl, ConstantDecl, ConstructorDecl, ConstructorParam, Decl, Expr, FunctionClause,
    FunctionDecl, MethodDecl, Param, Pattern, ShapeDecl, StructDecl, StructFieldDecl, TraitDecl,
    TraitImplDecl, TypeDecl, TypeExpr, UnsupportedDecl,
};
use crate::terlan_syntax::type_name_to_atom_payload;

#[path = "declaration_tail.rs"]
mod declaration_tail;
use declaration_tail::{
    format_function_clause, single_clause_matches_header, single_clause_signature_patterns,
};
pub(super) use declaration_tail::{format_raw_decl, format_shape_decl};

const STRUCTURAL_TYPE_ALIAS_MAX_LINE_LENGTH: usize = 120;

pub(super) fn format_constant_decl(constant: &ConstantDecl) -> String {
    format!(
        "{}const {}: {} = {}.",
        if constant.is_public { "pub " } else { "" },
        constant.name,
        format_type_expr(&constant.annotation),
        format_expr(&constant.value, 0)
    )
}

pub(super) fn format_const_function_decl(function: &ConstFunctionDecl) -> String {
    format!(
        "{}const {}({}): {} -> {}.",
        if function.is_public { "pub " } else { "" },
        function.name,
        function
            .params
            .iter()
            .map(format_param)
            .collect::<Vec<_>>()
            .join(", "),
        format_type_expr(&function.return_type),
        format_expr(&function.body, 0)
    )
}

pub(super) fn declarations_need_blank_line(previous: &Decl, current: &Decl) -> bool {
    match (previous, current) {
        (Decl::Import(left), Decl::Import(right)) => left.is_type != right.is_type,
        _ => true,
    }
}

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

    if let Some(representation) = &ty.representation {
        out.push_str(": ");
        out.push_str(&format_type_expr(representation));
        out.push_str(" = ");
        out.push_str(
            &ty.valued_arms
                .iter()
                .map(|arm| format!("{} = {}", arm.name, format_expr(&arm.value, 0)))
                .collect::<Vec<_>>()
                .join(" | "),
        );
        out.push('.');
        return out;
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

    if is_implicit_atom_type_alias(ty) {
        out.push('.');
        return out;
    }

    let inline_variants = ty
        .variants
        .iter()
        .map(format_type_expr)
        .collect::<Vec<_>>()
        .join(" | ");
    let inline = format!("{out} = {inline_variants}.");
    if inline.chars().count() <= DEFAULT_MAX_LINE_LENGTH {
        return inline;
    }

    if ty.variants.len() == 1 {
        let variant = format_type_expr(&ty.variants[0]);
        let inline = format!("{out} = {variant}.");
        if inline.chars().count() <= STRUCTURAL_TYPE_ALIAS_MAX_LINE_LENGTH {
            return inline;
        }
        if let Some(vertical_variant) = format_vertical_structural_type_expr(&variant) {
            out.push_str(" =\n      ");
            out.push_str(&vertical_variant);
            out.push('.');
            return out;
        }
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

fn format_vertical_structural_type_expr(variant: &str) -> Option<String> {
    let inner = variant.strip_prefix('{')?.strip_suffix('}')?;
    let parts = split_top_level_commas(inner);
    if parts.len() <= 1 {
        return None;
    }

    let mut out = String::from("{\n");
    for (index, part) in parts.iter().enumerate() {
        out.push_str("          ");
        out.push_str(part.trim());
        if index + 1 < parts.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str("      }");
    Some(out)
}

fn split_top_level_commas(text: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut square_depth = 0usize;
    let mut paren_depth = 0usize;
    let mut brace_depth = 0usize;
    let mut in_string = false;
    let mut escape_next = false;

    for ch in text.chars() {
        if in_string {
            current.push(ch);
            if escape_next {
                escape_next = false;
            } else if ch == '\\' {
                escape_next = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        match ch {
            '"' => {
                in_string = true;
                current.push(ch);
            }
            '[' => {
                square_depth += 1;
                current.push(ch);
            }
            ']' => {
                square_depth = square_depth.saturating_sub(1);
                current.push(ch);
            }
            '(' => {
                paren_depth += 1;
                current.push(ch);
            }
            ')' => {
                paren_depth = paren_depth.saturating_sub(1);
                current.push(ch);
            }
            '{' => {
                brace_depth += 1;
                current.push(ch);
            }
            '}' => {
                brace_depth = brace_depth.saturating_sub(1);
                current.push(ch);
            }
            ',' if square_depth == 0 && paren_depth == 0 && brace_depth == 0 => {
                parts.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }

    if !current.trim().is_empty() {
        parts.push(current.trim().to_string());
    }
    parts
}

fn is_implicit_atom_type_alias(ty: &TypeDecl) -> bool {
    if ty.is_opaque || !ty.params.is_empty() || !ty.implements.is_empty() || ty.variants.len() != 1
    {
        return false;
    }
    format_type_expr(&ty.variants[0])
        == format!("Atom[\"{}\"]", type_name_to_atom_payload(&ty.name))
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
    if !decl.generic_params.is_empty() {
        out.push('[');
        out.push_str(&decl.generic_params.join(", "));
        out.push(']');
    }
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
        out.push(',');
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
            &function.generic_params,
            &function.generic_bounds,
            &function.params,
            &function.return_type,
        ));
        out.push('.');
        return out;
    }

    if function.clauses.len() == 1
        && (single_clause_matches_header(function)
            || single_clause_signature_patterns(function).is_some())
    {
        let signature_patterns = single_clause_signature_patterns(function);
        let signature = format_function_signature_with_patterns(
            &function.name,
            &function.generic_params,
            &function.generic_bounds,
            &function.params,
            &function.return_type,
            signature_patterns,
        );
        if let Some(body) = inline_function_body_expr(&function.clauses[0].body) {
            let inline = format!("{out}{signature} -> {body}.");
            if inline.chars().count() <= DEFAULT_MAX_LINE_LENGTH {
                return inline;
            }
        }
        out.push_str(&signature);
        out.push_str(" ->\n    ");
        out.push_str(&format_function_body_expr(&function.clauses[0].body, 1));
        out.push('.');
        return out;
    }

    out.push_str(&format_function_signature(
        &function.name,
        &function.generic_params,
        &function.generic_bounds,
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

fn format_function_body_expr(expr: &Expr, indent: usize) -> String {
    if let Expr::Let {
        bindings,
        else_clauses,
        body,
    } = expr
    {
        return format_function_body_let(bindings, else_clauses, body.as_deref(), indent);
    }

    let rendered = format_expr(expr, indent);
    if rendered
        .lines()
        .all(|line| indent_width(indent) + line.chars().count() <= DEFAULT_MAX_LINE_LENGTH)
    {
        return rendered;
    }

    match expr {
        Expr::Sequence(expressions) => format_statement_parts(
            expressions
                .iter()
                .map(|expr| format_expr(expr, 0))
                .collect::<Vec<_>>(),
            indent,
        ),
        _ => rendered,
    }
}

fn inline_function_body_expr(expr: &Expr) -> Option<String> {
    if !is_inline_function_body_expr(expr) {
        return None;
    }

    let rendered = format_expr(expr, 0);
    if rendered.contains('\n') {
        return None;
    }

    Some(rendered)
}

fn is_inline_function_body_expr(expr: &Expr) -> bool {
    match expr {
        Expr::Int(_) | Expr::Float(_) | Expr::Atom(_) | Expr::AtomLiteral(_) | Expr::Binary(_) => {
            true
        }
        Expr::Var(name) => matches!(name.as_str(), "true" | "false"),
        Expr::Tuple(items) | Expr::List(items) | Expr::FixedArray(items) => {
            items.iter().all(is_inline_function_body_expr)
        }
        Expr::UnaryOp { expr, .. } => is_inline_function_body_expr(expr),
        _ => false,
    }
}

fn indent_width(indent: usize) -> usize {
    indent * 4
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
        &method.generic_params,
        &method.generic_bounds,
        &method.params,
        &method.return_type,
    ));
    if let Some(clause) = method.clauses.first() {
        if let Some(body) = inline_function_body_expr(&clause.body) {
            let inline = format!("{out} -> {body}.");
            if inline.chars().count() <= DEFAULT_MAX_LINE_LENGTH {
                return inline;
            }
        }
        out.push_str(" ->\n    ");
        out.push_str(&format_expr(&clause.body, 1));
    }
    out.push('.');
    out
}

/// Formats a function signature.
///
/// Inputs: function name, generic parameters, generic bounds, params, and
/// return type. Output: `name(params)[Bounds]: Type` text. Transformation:
/// formats callable constraints and params in source order and normalizes type
/// expressions through `format_type_expr`.
pub(super) fn format_function_signature(
    name: &str,
    generic_params: &[String],
    generic_bounds: &[String],
    params: &[Param],
    ret: &TypeExpr,
) -> String {
    format_function_signature_with_patterns(name, generic_params, generic_bounds, params, ret, None)
}

fn format_function_signature_with_patterns(
    name: &str,
    generic_params: &[String],
    generic_bounds: &[String],
    params: &[Param],
    ret: &TypeExpr,
    patterns: Option<&[Pattern]>,
) -> String {
    let mut out = String::new();
    out.push_str(name);
    if !generic_params.is_empty() {
        out.push('[');
        out.push_str(&generic_params.join(", "));
        out.push(']');
    }
    let signature_head = out.clone();
    let rendered_params = params
        .iter()
        .enumerate()
        .map(|(index, param)| {
            format_param_with_pattern(param, patterns.and_then(|items| items.get(index)))
        })
        .collect::<Vec<_>>();
    let return_type = format_type_expr(ret);
    let inline_params = rendered_params.join(", ");
    let rendered_bounds = if generic_bounds.is_empty() {
        String::new()
    } else {
        format!("[{}]", generic_bounds.join(", "))
    };
    let inline = format!("{signature_head}({inline_params}){rendered_bounds}: {return_type}");
    out.push('(');
    if inline.chars().count() + 4 <= DEFAULT_MAX_LINE_LENGTH || rendered_params.is_empty() {
        out.push_str(&inline_params);
        out.push(')');
        out.push_str(&rendered_bounds);
        out.push_str(": ");
        out.push_str(&return_type);
        return out;
    }

    out.push('\n');
    for param in &rendered_params {
        out.push_str("    ");
        out.push_str(param);
        out.push(',');
        out.push('\n');
    }
    out.push(')');
    out.push_str(&rendered_bounds);
    out.push_str(": ");
    out.push_str(&return_type);
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
    for constant in &trait_decl.constants {
        let docs = format_docs(&constant.docs, 1);
        if !docs.is_empty() {
            out.push_str(&docs);
            out.push('\n');
        }
        out.push_str("    const ");
        out.push_str(&constant.name);
        out.push_str(": ");
        out.push_str(&format_type_expr(&constant.annotation));
        if let Some(default) = &constant.default {
            out.push_str(" = ");
            out.push_str(&format_expr(default, 0));
        }
        out.push_str(".\n");
    }
    for method in &trait_decl.methods {
        let docs = format_docs(&method.docs, 1);
        if !docs.is_empty() {
            out.push_str(&docs);
            out.push('\n');
        }
        if method.is_pure {
            out.push_str("    @pure\n");
        }
        out.push_str("    ");
        out.push_str(&method.name);
        if !method.generic_params.is_empty() {
            out.push('[');
            out.push_str(&method.generic_params.join(", "));
            out.push(']');
        }
        if !method.params.is_empty() {
            out.push('(');
            out.push_str(
                &method
                    .params
                    .iter()
                    .map(format_param)
                    .collect::<Vec<_>>()
                    .join(", "),
            );
            out.push(')');
        } else {
            out.push_str("()");
        }
        if !method.generic_bounds.is_empty() {
            out.push('[');
            out.push_str(&method.generic_bounds.join(", "));
            out.push(']');
        }
        out.push_str(": ");
        out.push_str(&format_type_expr(&method.return_type));
        if let Some(default_body) = &method.default_body {
            out.push_str(" ->\n        ");
            out.push_str(&format_expr(default_body, 2));
        }
        out.push_str(".\n");
    }
    out.push_str("}.");
    out
}

fn format_param(param: &Param) -> String {
    format_param_with_pattern(param, None)
}

fn format_param_with_pattern(param: &Param, pattern: Option<&Pattern>) -> String {
    let mut out = String::new();
    if param.is_mutable {
        out.push_str("mut ");
    }
    let annotation = format_type_expr(&param.annotation);
    match pattern {
        Some(Pattern::Var(name)) if name == &param.name => out.push_str(&param.name),
        Some(pattern) => out.push_str(&format_pattern(pattern)),
        None => out.push_str(&param.name),
    }
    if matches!(pattern, Some(Pattern::BinaryLayout { .. })) && annotation == "Dynamic" {
        return out;
    }
    out.push_str(": ");
    out.push_str(&annotation);
    if let Some(default) = &param.default {
        out.push_str(" = ");
        out.push_str(&format_expr(default, 0));
    }
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
    if trait_impl.is_negative {
        out.push_str("not ");
        out.push_str(&format_type_expr(&trait_impl.trait_ref));
        out.push('[');
        out.push_str(&format_type_expr(&trait_impl.for_type));
        out.push_str("].");
        return out;
    }
    out.push_str(&crate::terlan_syntax::render_trait_impl_ref(
        &format_type_expr(&trait_impl.trait_ref),
        &trait_impl.generic_params,
    ));
    out.push_str(" for ");
    out.push_str(&format_type_expr(&trait_impl.for_type));
    out.push_str(" {\n");
    for constant in &trait_impl.constants {
        out.push_str("    const ");
        out.push_str(&constant.name);
        if let Some(annotation) = &constant.annotation {
            out.push_str(": ");
            out.push_str(&format_type_expr(annotation));
        }
        out.push_str(" = ");
        out.push_str(&format_expr(&constant.value, 0));
        out.push_str(".\n");
    }
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
