use std::collections::{HashMap, HashSet};

use crate::terlan_syntax::{
    span::Span, SyntaxDeclarationPayload, SyntaxExprKind, SyntaxExprOutput,
    SyntaxHtmlAttrValueOutput, SyntaxHtmlNodeOutput, SyntaxModuleOutput, SyntaxPatternKind,
    SyntaxPatternOutput,
};

use crate::terlan_hir::ModuleInterface;
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
            if raw_kind == "shape" {
                continue;
            }
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
    expand_syntax_macros_with_interfaces(module, &HashMap::new())
}

/// Expands local and explicitly imported hygienic expression macros.
pub fn expand_syntax_macros_with_interfaces(
    mut module: SyntaxModuleOutput,
    interfaces: &HashMap<String, ModuleInterface>,
) -> (SyntaxModuleOutput, Vec<Diagnostic>) {
    let (mut macros, mut diagnostics) = collect_expression_macros(&module);
    collect_imported_expression_macros(&module, interfaces, &mut macros, &mut diagnostics);
    let mut expansion = MacroExpansion::new(&macros);
    for declaration in &mut module.declarations {
        let is_macro_declaration = matches!(
            declaration.payload,
            SyntaxDeclarationPayload::Function { is_macro: true, .. }
        );
        if !is_macro_declaration {
            expand_declaration_expressions(
                &mut declaration.payload,
                &mut expansion,
                &mut diagnostics,
            );
        }
    }
    diagnostics.extend(collect_syntax_raw_macro_diagnostics(&module));
    if diagnostics.len() > MAX_MACRO_DIAGNOSTICS {
        let span = module
            .declarations
            .first()
            .map_or_else(|| Span::new(0, 0), |declaration| declaration.span.into());
        diagnostics.truncate(MAX_MACRO_DIAGNOSTICS - 1);
        diagnostics.push(macro_diagnostic(
            span,
            format!("macro expansion diagnostic limit {MAX_MACRO_DIAGNOSTICS} exceeded"),
        ));
    }
    (module, diagnostics)
}

fn collect_imported_expression_macros(
    module: &SyntaxModuleOutput,
    interfaces: &HashMap<String, ModuleInterface>,
    macros: &mut HashMap<(String, usize), ExpressionMacro>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::Import {
            module_name,
            items,
            is_selected: true,
            is_type: false,
            ..
        } = &declaration.payload
        else {
            continue;
        };
        let Some(interface) = interfaces.get(module_name) else {
            continue;
        };
        for item in items {
            let candidates = interface
                .expression_macros
                .iter()
                .filter(|((name, _), _)| item.name == "*" || name == &item.name)
                .collect::<Vec<_>>();
            for ((_, arity), signature) in candidates {
                let local_name = item
                    .as_alias
                    .clone()
                    .unwrap_or_else(|| signature.name.clone());
                let key = (local_name.clone(), *arity);
                if macros.contains_key(&key) {
                    diagnostics.push(macro_diagnostic(
                        item.span.into(),
                        format!("ambiguous imported macro `{local_name}/{arity}`"),
                    ));
                    continue;
                }
                macros.insert(
                    key,
                    ExpressionMacro {
                        name: local_name,
                        params: signature
                            .params
                            .iter()
                            .map(|param| param.name.clone())
                            .collect(),
                        template: signature.template.clone(),
                        span: item.span.into(),
                    },
                );
            }
        }
    }
}

const MAX_MACRO_EXPANSION_DEPTH: usize = 64;
const MAX_MACRO_EXPANSION_STEPS: usize = 10_000;
const MAX_MACRO_OUTPUT_NODES: usize = 100_000;
const MAX_MACRO_DIAGNOSTICS: usize = 128;

#[derive(Clone)]
struct ExpressionMacro {
    name: String,
    params: Vec<String>,
    template: SyntaxExprOutput,
    span: Span,
}

fn collect_expression_macros(
    module: &SyntaxModuleOutput,
) -> (HashMap<(String, usize), ExpressionMacro>, Vec<Diagnostic>) {
    let mut macros = HashMap::new();
    let mut diagnostics = Vec::new();
    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::Function {
            name,
            params,
            clauses,
            is_macro: true,
            ..
        } = &declaration.payload
        else {
            continue;
        };
        let span: Span = declaration.span.into();
        if clauses.len() != 1 || clauses[0].guard.is_some() {
            diagnostics.push(macro_diagnostic(
                span,
                format!("macro `{name}` must have exactly one unguarded expression clause"),
            ));
            continue;
        }
        let body = &clauses[0].body;
        if body.kind != SyntaxExprKind::Quote || body.children.len() != 1 {
            diagnostics.push(macro_diagnostic(
                span,
                format!("macro `{name}` must return one `quote` expression"),
            ));
            continue;
        }
        let key = (name.clone(), params.len());
        if macros.contains_key(&key) {
            diagnostics.push(macro_diagnostic(
                span,
                format!("ambiguous macro declaration `{name}/{}`", params.len()),
            ));
            continue;
        }
        macros.insert(
            key,
            ExpressionMacro {
                name: name.clone(),
                params: params.iter().map(|param| param.name.clone()).collect(),
                template: body.children[0].clone(),
                span,
            },
        );
    }
    (macros, diagnostics)
}

struct MacroExpansion<'a> {
    macros: &'a HashMap<(String, usize), ExpressionMacro>,
    steps: usize,
    output_nodes: usize,
    invocation: usize,
}

impl<'a> MacroExpansion<'a> {
    fn new(macros: &'a HashMap<(String, usize), ExpressionMacro>) -> Self {
        Self {
            macros,
            steps: 0,
            output_nodes: 0,
            invocation: 0,
        }
    }

    fn expand_expr(
        &mut self,
        expr: &mut SyntaxExprOutput,
        depth: usize,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if depth > MAX_MACRO_EXPANSION_DEPTH {
            diagnostics.push(macro_diagnostic(
                expr.span.into(),
                format!(
                    "macro expansion exceeded recursion depth limit {MAX_MACRO_EXPANSION_DEPTH}"
                ),
            ));
            return;
        }
        if expr.kind == SyntaxExprKind::Macro {
            let name = expr.text.clone().unwrap_or_default();
            let key = (name.clone(), expr.children.len());
            if let Some(definition) = self.macros.get(&key) {
                self.steps = self.steps.saturating_add(1);
                if self.steps > MAX_MACRO_EXPANSION_STEPS {
                    diagnostics.push(macro_diagnostic(
                        expr.span.into(),
                        format!("macro expansion exceeded step limit {MAX_MACRO_EXPANSION_STEPS}"),
                    ));
                    return;
                }
                let call_span = expr.span;
                let args = expr.children.clone();
                let substitutions = definition
                    .params
                    .iter()
                    .cloned()
                    .zip(args)
                    .collect::<HashMap<_, _>>();
                self.invocation = self.invocation.saturating_add(1);
                let mut expanded = definition.template.clone();
                hygienize_expression(&mut expanded, self.invocation);
                if !splice_unquotes(&mut expanded, &substitutions) {
                    diagnostics.push(macro_diagnostic(
                        definition.span,
                        format!(
                            "macro `{}` contains an `unquote` that is not one of its syntax parameters",
                            definition.name
                        ),
                    ));
                    return;
                }
                expanded.span = call_span;
                self.output_nodes = self
                    .output_nodes
                    .saturating_add(expression_node_count(&expanded));
                if self.output_nodes > MAX_MACRO_OUTPUT_NODES {
                    diagnostics.push(macro_diagnostic(
                        call_span.into(),
                        format!(
                            "macro `{}` output exceeded node limit {MAX_MACRO_OUTPUT_NODES}",
                            definition.name
                        ),
                    ));
                    return;
                }
                *expr = expanded;
                self.expand_expr(expr, depth + 1, diagnostics);
                return;
            }
            if self.macros.keys().any(|(candidate, _)| candidate == &name) {
                let mut arities = self
                    .macros
                    .keys()
                    .filter_map(|(candidate, arity)| (candidate == &name).then_some(*arity))
                    .collect::<Vec<_>>();
                arities.sort_unstable();
                diagnostics.push(macro_diagnostic(
                    expr.span.into(),
                    format!(
                        "wrong arity for macro `{name}`: expected one of {arities:?}, found {}",
                        expr.children.len()
                    ),
                ));
                return;
            }
        }

        for child in &mut expr.children {
            self.expand_expr(child, depth, diagnostics);
        }
        for field in &mut expr.fields {
            self.expand_expr(&mut field.value, depth, diagnostics);
        }
        for clause in expr.clauses.iter_mut().chain(&mut expr.catch_clauses) {
            if let Some(guard) = &mut clause.guard {
                self.expand_expr(guard, depth, diagnostics);
            }
            self.expand_expr(&mut clause.body, depth, diagnostics);
        }
        if let Some(after) = &mut expr.try_after {
            self.expand_expr(&mut after.trigger, depth, diagnostics);
            self.expand_expr(&mut after.body, depth, diagnostics);
        }
        for node in &mut expr.html_nodes {
            expand_html_node(node, self, depth, diagnostics);
        }
    }
}

fn expand_html_node(
    node: &mut SyntaxHtmlNodeOutput,
    expansion: &mut MacroExpansion<'_>,
    depth: usize,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match node {
        SyntaxHtmlNodeOutput::Expr { expr } => expansion.expand_expr(expr, depth, diagnostics),
        SyntaxHtmlNodeOutput::Text { .. } => {}
        SyntaxHtmlNodeOutput::Element { element } => {
            for attr in &mut element.attrs {
                if let Some(SyntaxHtmlAttrValueOutput::Expr { expr }) = &mut attr.value {
                    expansion.expand_expr(expr, depth, diagnostics);
                }
            }
            for child in &mut element.children {
                expand_html_node(child, expansion, depth, diagnostics);
            }
        }
        SyntaxHtmlNodeOutput::NamedSlot { slot } => {
            for child in &mut slot.children {
                expand_html_node(child, expansion, depth, diagnostics);
            }
        }
    }
}

fn splice_unquotes(
    expr: &mut SyntaxExprOutput,
    substitutions: &HashMap<String, SyntaxExprOutput>,
) -> bool {
    if expr.kind == SyntaxExprKind::Unquote {
        let Some(name) = expr.children.first().and_then(|child| {
            (child.kind == SyntaxExprKind::Var)
                .then(|| child.text.as_deref())
                .flatten()
        }) else {
            return false;
        };
        let Some(replacement) = substitutions.get(name) else {
            return false;
        };
        *expr = replacement.clone();
        return true;
    }
    expr.children
        .iter_mut()
        .all(|child| splice_unquotes(child, substitutions))
        && expr
            .fields
            .iter_mut()
            .all(|field| splice_unquotes(&mut field.value, substitutions))
        && expr.clauses.iter_mut().all(|clause| {
            clause
                .guard
                .as_mut()
                .is_none_or(|guard| splice_unquotes(guard, substitutions))
                && splice_unquotes(&mut clause.body, substitutions)
        })
        && expr.catch_clauses.iter_mut().all(|clause| {
            clause
                .guard
                .as_mut()
                .is_none_or(|guard| splice_unquotes(guard, substitutions))
                && splice_unquotes(&mut clause.body, substitutions)
        })
        && expr.try_after.as_mut().is_none_or(|after| {
            splice_unquotes(&mut after.trigger, substitutions)
                && splice_unquotes(&mut after.body, substitutions)
        })
        && expr
            .html_nodes
            .iter_mut()
            .all(|node| splice_unquotes_in_html_node(node, substitutions))
}

fn splice_unquotes_in_html_node(
    node: &mut SyntaxHtmlNodeOutput,
    substitutions: &HashMap<String, SyntaxExprOutput>,
) -> bool {
    match node {
        SyntaxHtmlNodeOutput::Expr { expr } => splice_unquotes(expr, substitutions),
        SyntaxHtmlNodeOutput::Text { .. } => true,
        SyntaxHtmlNodeOutput::Element { element } => {
            element.attrs.iter_mut().all(|attr| match &mut attr.value {
                Some(SyntaxHtmlAttrValueOutput::Expr { expr }) => {
                    splice_unquotes(expr, substitutions)
                }
                _ => true,
            }) && element
                .children
                .iter_mut()
                .all(|child| splice_unquotes_in_html_node(child, substitutions))
        }
        SyntaxHtmlNodeOutput::NamedSlot { slot } => slot
            .children
            .iter_mut()
            .all(|child| splice_unquotes_in_html_node(child, substitutions)),
    }
}

fn hygienize_expression(expr: &mut SyntaxExprOutput, invocation: usize) {
    let mut bound = HashSet::new();
    collect_bound_names(expr, &mut bound);
    let renames = bound
        .into_iter()
        .map(|name| {
            let hygienic = format!("__macro_{invocation}_{name}");
            (name, hygienic)
        })
        .collect::<HashMap<_, _>>();
    rename_hygienic_expr(expr, &renames);
}

fn collect_bound_names(expr: &SyntaxExprOutput, bound: &mut HashSet<String>) {
    if expr.kind == SyntaxExprKind::Unquote {
        return;
    }
    for pattern in &expr.patterns {
        collect_pattern_names(pattern, bound);
    }
    for clause in expr.clauses.iter().chain(&expr.catch_clauses) {
        for pattern in &clause.patterns {
            collect_pattern_names(pattern, bound);
        }
        if let Some(guard) = &clause.guard {
            collect_bound_names(guard, bound);
        }
        collect_bound_names(&clause.body, bound);
    }
    for child in &expr.children {
        collect_bound_names(child, bound);
    }
    for field in &expr.fields {
        collect_bound_names(&field.value, bound);
    }
    if let Some(after) = &expr.try_after {
        collect_bound_names(&after.trigger, bound);
        collect_bound_names(&after.body, bound);
    }
    for node in &expr.html_nodes {
        collect_bound_names_in_html_node(node, bound);
    }
}

fn collect_bound_names_in_html_node(node: &SyntaxHtmlNodeOutput, bound: &mut HashSet<String>) {
    match node {
        SyntaxHtmlNodeOutput::Expr { expr } => collect_bound_names(expr, bound),
        SyntaxHtmlNodeOutput::Text { .. } => {}
        SyntaxHtmlNodeOutput::Element { element } => {
            for attr in &element.attrs {
                if let Some(SyntaxHtmlAttrValueOutput::Expr { expr }) = &attr.value {
                    collect_bound_names(expr, bound);
                }
            }
            for child in &element.children {
                collect_bound_names_in_html_node(child, bound);
            }
        }
        SyntaxHtmlNodeOutput::NamedSlot { slot } => {
            for child in &slot.children {
                collect_bound_names_in_html_node(child, bound);
            }
        }
    }
}

fn collect_pattern_names(pattern: &SyntaxPatternOutput, bound: &mut HashSet<String>) {
    if pattern.kind == SyntaxPatternKind::Var {
        if let Some(name) = &pattern.text {
            bound.insert(name.clone());
        }
    }
    for child in &pattern.children {
        collect_pattern_names(child, bound);
    }
    for field in &pattern.fields {
        collect_pattern_names(&field.value, bound);
    }
}

fn rename_hygienic_expr(expr: &mut SyntaxExprOutput, renames: &HashMap<String, String>) {
    if expr.kind == SyntaxExprKind::Unquote {
        return;
    }
    if expr.kind == SyntaxExprKind::Var {
        if let Some(replacement) = expr.text.as_ref().and_then(|name| renames.get(name)) {
            expr.text = Some(replacement.clone());
        }
    }
    for pattern in &mut expr.patterns {
        rename_hygienic_pattern(pattern, renames);
    }
    for child in &mut expr.children {
        rename_hygienic_expr(child, renames);
    }
    for field in &mut expr.fields {
        rename_hygienic_expr(&mut field.value, renames);
    }
    for clause in expr.clauses.iter_mut().chain(&mut expr.catch_clauses) {
        for pattern in &mut clause.patterns {
            rename_hygienic_pattern(pattern, renames);
        }
        if let Some(guard) = &mut clause.guard {
            rename_hygienic_expr(guard, renames);
        }
        rename_hygienic_expr(&mut clause.body, renames);
    }
    if let Some(after) = &mut expr.try_after {
        rename_hygienic_expr(&mut after.trigger, renames);
        rename_hygienic_expr(&mut after.body, renames);
    }
    for node in &mut expr.html_nodes {
        rename_hygienic_html_node(node, renames);
    }
}

fn rename_hygienic_html_node(node: &mut SyntaxHtmlNodeOutput, renames: &HashMap<String, String>) {
    match node {
        SyntaxHtmlNodeOutput::Expr { expr } => rename_hygienic_expr(expr, renames),
        SyntaxHtmlNodeOutput::Text { .. } => {}
        SyntaxHtmlNodeOutput::Element { element } => {
            for attr in &mut element.attrs {
                if let Some(SyntaxHtmlAttrValueOutput::Expr { expr }) = &mut attr.value {
                    rename_hygienic_expr(expr, renames);
                }
            }
            for child in &mut element.children {
                rename_hygienic_html_node(child, renames);
            }
        }
        SyntaxHtmlNodeOutput::NamedSlot { slot } => {
            for child in &mut slot.children {
                rename_hygienic_html_node(child, renames);
            }
        }
    }
}

fn rename_hygienic_pattern(pattern: &mut SyntaxPatternOutput, renames: &HashMap<String, String>) {
    if pattern.kind == SyntaxPatternKind::Var {
        if let Some(replacement) = pattern.text.as_ref().and_then(|name| renames.get(name)) {
            pattern.text = Some(replacement.clone());
        }
    }
    for child in &mut pattern.children {
        rename_hygienic_pattern(child, renames);
    }
    for field in &mut pattern.fields {
        rename_hygienic_pattern(&mut field.value, renames);
    }
}

fn expression_node_count(expr: &SyntaxExprOutput) -> usize {
    1 + expr
        .children
        .iter()
        .map(expression_node_count)
        .sum::<usize>()
        + expr
            .fields
            .iter()
            .map(|field| expression_node_count(&field.value))
            .sum::<usize>()
        + expr
            .clauses
            .iter()
            .chain(&expr.catch_clauses)
            .map(|clause| {
                expression_node_count(&clause.body)
                    + clause
                        .guard
                        .as_ref()
                        .map_or(0, |guard| expression_node_count(guard))
            })
            .sum::<usize>()
        + expr.try_after.as_ref().map_or(0, |after| {
            expression_node_count(&after.trigger) + expression_node_count(&after.body)
        })
        + expr.html_nodes.iter().map(html_node_count).sum::<usize>()
}

fn html_node_count(node: &SyntaxHtmlNodeOutput) -> usize {
    match node {
        SyntaxHtmlNodeOutput::Expr { expr } => 1 + expression_node_count(expr),
        SyntaxHtmlNodeOutput::Text { .. } => 1,
        SyntaxHtmlNodeOutput::Element { element } => {
            1 + element
                .attrs
                .iter()
                .map(|attr| match &attr.value {
                    Some(SyntaxHtmlAttrValueOutput::Expr { expr }) => expression_node_count(expr),
                    _ => 0,
                })
                .sum::<usize>()
                + element.children.iter().map(html_node_count).sum::<usize>()
        }
        SyntaxHtmlNodeOutput::NamedSlot { slot } => {
            1 + slot.children.iter().map(html_node_count).sum::<usize>()
        }
    }
}

fn expand_declaration_expressions(
    payload: &mut SyntaxDeclarationPayload,
    expansion: &mut MacroExpansion<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut expand = |expr: &mut SyntaxExprOutput| expansion.expand_expr(expr, 0, diagnostics);
    match payload {
        SyntaxDeclarationPayload::Constant { value, .. }
        | SyntaxDeclarationPayload::ConstFunction { body: value, .. } => expand(value),
        SyntaxDeclarationPayload::Type { valued_arms, .. } => {
            for arm in valued_arms {
                expand(&mut arm.value);
            }
        }
        SyntaxDeclarationPayload::Struct { fields, .. } => {
            for field in fields {
                if let Some(default) = &mut field.default {
                    expand(default);
                }
            }
        }
        SyntaxDeclarationPayload::Constructor { clauses, .. } => {
            for clause in clauses {
                for param in &mut clause.params {
                    if let Some(default) = &mut param.default {
                        expand(default);
                    }
                }
                expand(&mut clause.body);
            }
        }
        SyntaxDeclarationPayload::Function {
            params, clauses, ..
        }
        | SyntaxDeclarationPayload::Method {
            params, clauses, ..
        } => {
            for param in params {
                if let Some(default) = &mut param.default {
                    expand(default);
                }
            }
            for clause in clauses {
                if let Some(guard) = &mut clause.guard {
                    expand(guard);
                }
                expand(&mut clause.body);
            }
        }
        SyntaxDeclarationPayload::Trait {
            constants, methods, ..
        } => {
            for constant in constants {
                if let Some(default) = &mut constant.default {
                    expand(default);
                }
            }
            for method in methods {
                for param in &mut method.params {
                    if let Some(default) = &mut param.default {
                        expand(default);
                    }
                }
                if let Some(body) = &mut method.default_body {
                    expand(body);
                }
            }
        }
        SyntaxDeclarationPayload::TraitImpl {
            constants, methods, ..
        } => {
            for constant in constants {
                expand(&mut constant.value);
            }
            for method in methods {
                for param in &mut method.params {
                    if let Some(default) = &mut param.default {
                        expand(default);
                    }
                }
                for clause in &mut method.clauses {
                    if let Some(guard) = &mut clause.guard {
                        expand(guard);
                    }
                    expand(&mut clause.body);
                }
            }
        }
        SyntaxDeclarationPayload::Template { props, .. } => {
            for prop in props {
                if let Some(default) = &mut prop.default {
                    expand(default);
                }
            }
        }
        SyntaxDeclarationPayload::Import { .. }
        | SyntaxDeclarationPayload::Export { .. }
        | SyntaxDeclarationPayload::AnnotationSchema { .. }
        | SyntaxDeclarationPayload::Config { .. }
        | SyntaxDeclarationPayload::Raw { .. } => {}
    }
}

fn macro_diagnostic(span: Span, message: String) -> Diagnostic {
    Diagnostic {
        span,
        message,
        severity: DiagSeverity::Error,
    }
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
