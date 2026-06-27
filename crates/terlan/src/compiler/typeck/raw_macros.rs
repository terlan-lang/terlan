use crate::terlan_syntax::{
    span::Span, SyntaxDeclarationPayload, SyntaxExprKind, SyntaxExprOutput,
    SyntaxHtmlAttrValueOutput, SyntaxHtmlNodeOutput, SyntaxModuleOutput,
};

use crate::terlan_typeck::{DiagSeverity, Diagnostic};

/// Returns a list of diagnostics for raw declarations that are not yet supported
/// by the formal compiler path.
///
/// Inputs:
/// - `module`: formality-facing syntax module to validate.
///
/// Output:
/// - A list of errors for each unsupported `SyntaxDeclarationPayload::Raw` kind.
///
/// Transformation:
/// - Scans each declaration and emits an error for every remaining raw payload.
///   Canonical config declarations are represented as `Config`, not raw output.
pub fn collect_syntax_unsupported_raw_declaration_diagnostics(
    module: &SyntaxModuleOutput,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for declaration in &module.declarations {
        if let SyntaxDeclarationPayload::Raw { raw_kind, .. } = &declaration.payload {
            diagnostics.push(Diagnostic {
                span: declaration.span.into(),
                message: format!(
                    "unsupported raw declaration kind `{}` in formal compiler path",
                    raw_kind
                ),
                severity: DiagSeverity::Error,
            });
        }
    }

    diagnostics
}

/// Runs the syntax-output macro-expansion phase.
///
/// Inputs:
/// - `module`: compiler-facing syntax output to scan.
///
/// Output:
/// - A tuple containing the expanded syntax-output module and one syntax-check
///   diagnostic per unresolved raw macro.
///
/// Transformation:
/// - Performs explicit expansion of macro-bearing expressions. The current formal
///   phase is explicit-unsupported for raw macros, so this pass currently
///   preserves all nodes and returns diagnostics when raw macros remain.
pub fn expand_syntax_raw_macros(
    module: SyntaxModuleOutput,
) -> (SyntaxModuleOutput, Vec<Diagnostic>) {
    let diagnostics = collect_syntax_raw_macro_diagnostics(&module);
    (module, diagnostics)
}

/// Builds the typechecker diagnostic for an unresolved raw macro expression.
///
/// Inputs:
/// - `name`: source raw-macro name preserved by syntax output.
///
/// Output:
/// - A stable diagnostic message for unresolved raw macro expansion.
///
/// Transformation:
/// - Preserves the historical raw-macro diagnostic prefix while adding
///   feature-specific guidance for compiler-known forms that are planned but
///   not yet lowered.
pub(crate) fn raw_macro_resolution_message(name: &str) -> String {
    let base = format!(
        "raw macro expression `{}` requires macro resolution before type checking",
        name
    );

    if name == "sql" {
        format!(
            "{}; Postgres SQL form lowering is not implemented yet",
            base
        )
    } else {
        base
    }
}

/// Builds the typechecker diagnostic for an unresolved raw macro expression node.
///
/// Inputs:
/// - `expr`: syntax-output raw macro expression preserving the macro name and
///   parsed child expressions.
///
/// Output:
/// - A stable diagnostic message for unresolved raw macro expansion.
///
/// Transformation:
/// - Delegates to the name-only raw-macro diagnostic, then adds compiler-known
///   metadata for typed SQL forms so later parameter binding work has an
///   observable contract before SQL lowering exists.
pub(crate) fn raw_macro_resolution_message_for_expr(expr: &SyntaxExprOutput) -> String {
    let name = expr.text.as_deref().unwrap_or("<unknown>");
    let message = raw_macro_resolution_message(name);

    if name == "sql" {
        let (
            binding_message,
            row_type_message,
            row_type_arity_message,
            parameter_consistency_message,
            cardinality_requirement_message,
            wrapper_readiness_message,
            wrapper_plan_message,
            result_type_message,
            cardinality,
        ) = match crate::terlan_typeck::sql_forms::analyze_sql_form(expr) {
            Ok(Some(analysis)) => (
                format!(
                    "parsed {} SQL parameter expression(s); bound {} SQL parameter placeholder(s)",
                    expr.children.len(),
                    analysis.binding.parameter_count
                ),
                format!("row type argument count: {}", analysis.row_type_arg_count),
                analysis
                    .row_type_arity_message()
                    .unwrap_or_else(|| "row type argument requirement satisfied".to_string()),
                analysis.parameter_count_consistency_message(expr.children.len()),
                analysis
                    .cardinality_requirement_message()
                    .unwrap_or_else(|| "SQL cardinality requirement satisfied".to_string()),
                analysis.wrapper_lowering_readiness_message(expr.children.len()),
                sql_wrapper_plan_status_message(expr),
                format!(
                    "wrapper result type: {}",
                    analysis
                        .result_type
                        .unwrap_or_else(|| "ambiguous".to_string())
                ),
                analysis.cardinality,
            ),
            Ok(None) => (
                "SQL form analysis unavailable".to_string(),
                "row type argument count: unknown".to_string(),
                "row type argument requirement unknown".to_string(),
                "SQL parameter count consistency unknown".to_string(),
                "SQL cardinality requirement unknown".to_string(),
                "SQL wrapper lowering readiness: unknown".to_string(),
                "SQL wrapper plan: unknown".to_string(),
                "wrapper result type: unknown".to_string(),
                crate::terlan_typeck::sql_forms::SqlCardinality::Ambiguous,
            ),
            Err(error) => (
                format!("SQL form analysis error: {}", error.message()),
                "row type argument count: unknown".to_string(),
                "row type argument requirement unknown".to_string(),
                "SQL parameter count consistency unknown".to_string(),
                "SQL cardinality requirement unknown".to_string(),
                "SQL wrapper lowering readiness: unknown".to_string(),
                "SQL wrapper plan: unknown".to_string(),
                "wrapper result type: unknown".to_string(),
                crate::terlan_typeck::sql_forms::SqlCardinality::Ambiguous,
            ),
        };
        format!(
            "{}; {}; {}; {}; {}; {}; {}; {}; {}; inferred SQL cardinality: {}",
            message,
            binding_message,
            row_type_message,
            row_type_arity_message,
            parameter_consistency_message,
            cardinality_requirement_message,
            wrapper_readiness_message,
            wrapper_plan_message,
            result_type_message,
            cardinality.as_diagnostic_label()
        )
    } else {
        message
    }
}

/// Builds a diagnostic fragment for SQL wrapper-plan readiness.
///
/// Inputs:
/// - `expr`: syntax-output raw macro expression preserving SQL-form metadata.
///
/// Output:
/// - Stable wrapper-plan diagnostic fragment.
///
/// Transformation:
/// - Reuses the SQL wrapper-plan builder so the unresolved raw macro
///   diagnostic reports the same payload that later backend wrapper emission
///   will consume.
fn sql_wrapper_plan_status_message(expr: &SyntaxExprOutput) -> String {
    match crate::terlan_typeck::sql_forms::build_sql_wrapper_plan(expr, expr.children.len()) {
        Ok(Some(plan)) => format!(
            "SQL wrapper plan: ready row_type={}, params={}, projection_fields={}, bound_sql_bytes={}, result_type={}, cardinality={}",
            plan.row_type,
            plan.parameter_count,
            plan.projection_fields.as_ref().map_or(0, Vec::len),
            plan.bound_sql.len(),
            plan.result_type,
            plan.cardinality.as_diagnostic_label()
        ),
        Ok(None) => "SQL wrapper plan: unavailable".to_string(),
        Err(error) => format!("SQL wrapper plan: {}", error.message()),
    }
}
/// Collects raw-macro diagnostics for syntax-output modules before full
/// resolution/typechecking.
///
/// Inputs:
/// - `module`: compiler-facing syntax output to scan.
///
/// Output:
/// - A list of syntax-check diagnostics, one per unresolved raw macro.
///
/// Transformation:
/// - Scans declaration expression trees for `SyntaxExprKind::RawMacro` and
///   emits an error diagnostic for each occurrence.
pub fn collect_syntax_raw_macro_diagnostics(module: &SyntaxModuleOutput) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for declaration in &module.declarations {
        match &declaration.payload {
            SyntaxDeclarationPayload::Function { clauses, .. } => {
                for clause in clauses {
                    let clause_span: Span = clause.span.into();
                    collect_raw_macro_diagnostics_in_expr(
                        &clause.body,
                        clause_span,
                        &mut diagnostics,
                    );
                    if let Some(guard) = &clause.guard {
                        collect_raw_macro_diagnostics_in_expr(guard, clause_span, &mut diagnostics);
                    }
                }
            }
            SyntaxDeclarationPayload::Method { clauses, .. } => {
                for clause in clauses {
                    let clause_span: Span = clause.span.into();
                    collect_raw_macro_diagnostics_in_expr(
                        &clause.body,
                        clause_span,
                        &mut diagnostics,
                    );
                    if let Some(guard) = &clause.guard {
                        collect_raw_macro_diagnostics_in_expr(guard, clause_span, &mut diagnostics);
                    }
                }
            }
            SyntaxDeclarationPayload::Constructor { clauses, .. } => {
                for clause in clauses {
                    let clause_span: Span = clause.span.into();
                    collect_raw_macro_diagnostics_in_expr(
                        &clause.body,
                        clause_span,
                        &mut diagnostics,
                    );
                }
            }
            SyntaxDeclarationPayload::Struct { fields, .. } => {
                for field in fields {
                    if let Some(default) = &field.default {
                        let fallback_span: Span = field.span.into();
                        collect_raw_macro_diagnostics_in_expr(
                            default,
                            fallback_span,
                            &mut diagnostics,
                        );
                    }
                }
            }
            SyntaxDeclarationPayload::Trait { methods, .. } => {
                for method in methods {
                    if let Some(default_body) = &method.default_body {
                        let fallback_span: Span = method.span.into();
                        collect_raw_macro_diagnostics_in_expr(
                            default_body,
                            fallback_span,
                            &mut diagnostics,
                        );
                    }
                }
            }
            SyntaxDeclarationPayload::TraitImpl { methods, .. } => {
                for method in methods {
                    for clause in &method.clauses {
                        let clause_span: Span = clause.span.into();
                        collect_raw_macro_diagnostics_in_expr(
                            &clause.body,
                            clause_span,
                            &mut diagnostics,
                        );
                        if let Some(guard) = &clause.guard {
                            collect_raw_macro_diagnostics_in_expr(
                                guard,
                                clause_span,
                                &mut diagnostics,
                            );
                        }
                    }
                }
            }
            SyntaxDeclarationPayload::Template { .. }
            | SyntaxDeclarationPayload::AnnotationSchema { .. }
            | SyntaxDeclarationPayload::Type { .. }
            | SyntaxDeclarationPayload::Import { .. }
            | SyntaxDeclarationPayload::Export { .. }
            | SyntaxDeclarationPayload::Config { .. }
            | SyntaxDeclarationPayload::Raw { .. } => {}
        }
    }
    diagnostics
}

/// Collects unsupported raw-macro diagnostics from an expression tree.
///
/// Inputs:
/// - `expr`: syntax-output expression to inspect.
/// - `fallback_span`: declaration or clause span used when a nested raw-macro
///   expression lacks a better source span.
/// - `diagnostics`: output sink for typechecker diagnostics.
///
/// Output:
/// - No direct value; diagnostics are appended to `diagnostics`.
///
/// Transformation:
/// - Recursively walks expression children, fields, clauses, try/catch blocks,
///   and HTML embedded expressions so raw macro placeholders are rejected
///   before later semantic phases treat them as ordinary expressions.
fn collect_raw_macro_diagnostics_in_expr(
    expr: &SyntaxExprOutput,
    fallback_span: Span,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let expr_span: Span = expr.span.into();
    let fallback_span = if expr_span.start == 0 && expr_span.end == 0 {
        fallback_span
    } else {
        expr_span
    };

    if expr.kind == SyntaxExprKind::RawMacro && raw_macro_requires_resolution_diagnostic(expr) {
        diagnostics.push(Diagnostic {
            span: fallback_span,
            message: raw_macro_resolution_message_for_expr(expr),
            severity: DiagSeverity::Error,
        });
    }

    for child in &expr.children {
        collect_raw_macro_diagnostics_in_expr(child, fallback_span, diagnostics);
    }
    for field in &expr.fields {
        collect_raw_macro_diagnostics_in_expr(&field.value, fallback_span, diagnostics);
    }
    for clause in &expr.clauses {
        collect_raw_macro_diagnostics_in_expr(&clause.body, fallback_span, diagnostics);
        if let Some(guard) = &clause.guard {
            collect_raw_macro_diagnostics_in_expr(guard, fallback_span, diagnostics);
        }
    }
    for clause in &expr.catch_clauses {
        collect_raw_macro_diagnostics_in_expr(&clause.body, fallback_span, diagnostics);
        if let Some(guard) = &clause.guard {
            collect_raw_macro_diagnostics_in_expr(guard, fallback_span, diagnostics);
        }
    }
    if let Some(try_after) = &expr.try_after {
        collect_raw_macro_diagnostics_in_expr(&try_after.trigger, fallback_span, diagnostics);
        collect_raw_macro_diagnostics_in_expr(&try_after.body, fallback_span, diagnostics);
    }
    for node in &expr.html_nodes {
        match node {
            SyntaxHtmlNodeOutput::Expr { expr } => {
                collect_raw_macro_diagnostics_in_expr(expr, fallback_span, diagnostics);
            }
            SyntaxHtmlNodeOutput::Text { .. } => {}
            SyntaxHtmlNodeOutput::Element { element } => {
                for attr in &element.attrs {
                    if let Some(value) = &attr.value {
                        match value {
                            SyntaxHtmlAttrValueOutput::Expr { expr } => {
                                collect_raw_macro_diagnostics_in_expr(
                                    expr,
                                    fallback_span,
                                    diagnostics,
                                );
                            }
                            SyntaxHtmlAttrValueOutput::Text { .. } => {}
                        }
                    }
                }
                for child in &element.children {
                    match child {
                        SyntaxHtmlNodeOutput::Expr { expr } => {
                            collect_raw_macro_diagnostics_in_expr(expr, fallback_span, diagnostics);
                        }
                        SyntaxHtmlNodeOutput::Text { .. } => {}
                        SyntaxHtmlNodeOutput::Element { element } => {
                            for child in &element.children {
                                match child {
                                    SyntaxHtmlNodeOutput::Expr { expr } => {
                                        collect_raw_macro_diagnostics_in_expr(
                                            expr,
                                            fallback_span,
                                            diagnostics,
                                        );
                                    }
                                    SyntaxHtmlNodeOutput::Text { .. } => {}
                                    SyntaxHtmlNodeOutput::Element { .. } => {}
                                    SyntaxHtmlNodeOutput::NamedSlot { slot } => {
                                        for slot_child in &slot.children {
                                            match slot_child {
                                                SyntaxHtmlNodeOutput::Expr { expr } => {
                                                    collect_raw_macro_diagnostics_in_expr(
                                                        expr,
                                                        fallback_span,
                                                        diagnostics,
                                                    );
                                                }
                                                SyntaxHtmlNodeOutput::Text { .. }
                                                | SyntaxHtmlNodeOutput::Element { .. } => {}
                                                SyntaxHtmlNodeOutput::NamedSlot { .. } => {}
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        SyntaxHtmlNodeOutput::NamedSlot { slot } => {
                            for slot_child in &slot.children {
                                match slot_child {
                                    SyntaxHtmlNodeOutput::Expr { expr } => {
                                        collect_raw_macro_diagnostics_in_expr(
                                            expr,
                                            fallback_span,
                                            diagnostics,
                                        );
                                    }
                                    SyntaxHtmlNodeOutput::Text { .. }
                                    | SyntaxHtmlNodeOutput::Element { .. }
                                    | SyntaxHtmlNodeOutput::NamedSlot { .. } => {}
                                }
                            }
                        }
                    }
                }
            }
            SyntaxHtmlNodeOutput::NamedSlot { slot } => {
                for slot_child in &slot.children {
                    match slot_child {
                        SyntaxHtmlNodeOutput::Expr { expr } => {
                            collect_raw_macro_diagnostics_in_expr(expr, fallback_span, diagnostics);
                        }
                        SyntaxHtmlNodeOutput::Text { .. }
                        | SyntaxHtmlNodeOutput::Element { .. }
                        | SyntaxHtmlNodeOutput::NamedSlot { .. } => {}
                    }
                }
            }
        }
    }
}

/// Returns whether a raw macro node should still be rejected before typecheck.
///
/// Inputs:
/// - `expr`: syntax-output expression that may be a raw macro placeholder.
///
/// Output:
/// - `true` for generic raw macros and SQL forms without a ready wrapper plan.
/// - `false` for typed SQL forms that have enough metadata for CoreIR/backend
///   wrapper lowering.
///
/// Transformation:
/// - Uses the SQL wrapper-plan readiness gate as the handoff point between raw
///   macro rejection and compiler-owned SQL lowering. This keeps untyped
///   `sql{...}` blocked while letting ready `sql[Row] { ... }` proceed.
pub(crate) fn raw_macro_requires_resolution_diagnostic(expr: &SyntaxExprOutput) -> bool {
    if expr.text.as_deref() != Some("sql") {
        return true;
    }

    !matches!(
        crate::terlan_typeck::sql_forms::build_sql_wrapper_plan(expr, expr.children.len()),
        Ok(Some(_))
    )
}
