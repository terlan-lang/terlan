use terlan_syntax::{
    span::Span, SyntaxDeclarationPayload, SyntaxExprKind, SyntaxExprOutput,
    SyntaxHtmlAttrValueOutput, SyntaxHtmlNodeOutput, SyntaxModuleOutput,
};

use crate::{DiagSeverity, Diagnostic};

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

    if expr.kind == SyntaxExprKind::RawMacro {
        let name = expr.text.as_deref().unwrap_or("<unknown>");
        diagnostics.push(Diagnostic {
            span: fallback_span,
            message: format!(
                "raw macro expression `{}` requires macro resolution before type checking",
                name
            ),
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
