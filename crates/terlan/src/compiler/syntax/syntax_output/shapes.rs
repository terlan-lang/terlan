use std::collections::{BTreeMap, BTreeSet};

#[path = "shapes/string_patterns.rs"]
mod string_patterns;
use string_patterns::{rewrite_string_pattern_text, string_capture_name};

use super::{
    expr_node, SyntaxClauseOutput, SyntaxDeclarationOutput, SyntaxDeclarationPayload,
    SyntaxExprKind, SyntaxExprOutput, SyntaxFunctionClauseOutput, SyntaxHtmlAttrValueOutput,
    SyntaxHtmlNodeOutput, SyntaxParamOutput, SyntaxPatternKind, SyntaxPatternOutput,
};
use crate::terlan_syntax::{
    ebnf::{EbnfCompileError, EbnfCompileResult},
    parse_tree::Module,
    parser::parse_interface_module,
};

mod binary_layout;
mod declarations;
mod guard_predicates;
mod overlap;
mod runtime_use;

use declarations::{collect_shapes, duplicate_pattern_binding};
use guard_predicates::{collect_guard_predicate_definitions, GuardPredicateDefinitions};
use overlap::{validate_expr_clause_overlap, validate_function_clause_overlap};
use runtime_use::reject_runtime_shape_call;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxShapeImport {
    pub(crate) local_name: String,
    pub(crate) source_module: String,
    pub(crate) source_name: String,
    pub(crate) provider_signatures: Vec<String>,
}

#[derive(Debug, Clone)]
struct ShapePattern {
    params: Vec<String>,
    body: SyntaxPatternOutput,
    guard: Option<SyntaxExprOutput>,
}

#[derive(Debug)]
struct ExpandedPattern {
    pattern: SyntaxPatternOutput,
    guards: Vec<SyntaxExprOutput>,
    shape_origins: BTreeSet<String>,
}

/// Expands local compile-time shape aliases in one syntax-output module.
///
/// Inputs:
/// - `module`: parsed source retaining structured shape declarations.
/// - `declarations`: syntax-output declarations about to enter HIR/typecheck.
///
/// Output:
/// - Success after every local shape call has become an ordinary pattern.
///
/// Transformation:
/// - Parses each shape body with the canonical pattern parser, substitutes
///   call-site patterns for shape parameters, and recursively expands nested
///   aliases. Shape declarations remain in output for interface/docs metadata.
pub(super) fn expand_local_shape_synonyms(
    module: &Module,
    declarations: &mut [SyntaxDeclarationOutput],
) -> EbnfCompileResult<()> {
    let shapes = collect_shapes(module)?;
    if shapes.is_empty() {
        return Ok(());
    }
    let guard_predicates = collect_guard_predicate_definitions(declarations);
    let mut next_hygiene_id = 0;
    for declaration in declarations {
        expand_declaration(
            &mut declaration.payload,
            &shapes,
            &guard_predicates,
            &mut next_hygiene_id,
        )?;
    }
    Ok(())
}

/// Expands interface-backed shape aliases selected by module imports.
///
/// Inputs:
/// - `declarations`: consumer syntax output after local shape expansion.
/// - `imports`: visible local aliases plus the exporting module's complete
///   public shape surface.
///
/// Output:
/// - Success after imported shape calls have become ordinary patterns and
///   guards.
///
/// Transformation:
/// - Parses provider signatures with the canonical interface parser.
/// - Normalizes nested provider shapes before exposing only selected aliases
///   to the consumer.
/// - Rejects recursive provider definitions and ambiguous local aliases.
pub fn expand_shape_imports(
    declarations: &mut [SyntaxDeclarationOutput],
    imports: &[SyntaxShapeImport],
) -> EbnfCompileResult<()> {
    if imports.is_empty() {
        return Ok(());
    }

    let guard_predicates = collect_guard_predicate_definitions(declarations);
    let mut providers = BTreeMap::new();
    let mut imported_shapes = BTreeMap::new();
    let mut origins = BTreeMap::new();
    let mut next_hygiene_id = 0;

    for import in imports {
        let provider_shapes = match providers.get(&import.source_module) {
            Some(shapes) => shapes,
            None => {
                let shapes =
                    collect_interface_shapes(&import.source_module, &import.provider_signatures)?;
                providers.insert(import.source_module.clone(), shapes);
                &providers[&import.source_module]
            }
        };
        let Some(shape) = provider_shapes.get(&import.source_name) else {
            return Err(EbnfCompileError::Serialize(format!(
                "interface `{}` does not export shape `{}`",
                import.source_module, import.source_name
            )));
        };
        let origin = format!("{}.{}", import.source_module, import.source_name);
        if let Some(existing) = origins.get(&import.local_name) {
            if existing != &origin {
                return Err(EbnfCompileError::Serialize(format!(
                    "ambiguous imported shape alias `{}`: `{existing}` and `{origin}`",
                    import.local_name
                )));
            }
            continue;
        }

        let normalized = normalize_imported_shape(
            &import.source_name,
            shape,
            provider_shapes,
            &mut next_hygiene_id,
        )?;
        origins.insert(import.local_name.clone(), origin);
        imported_shapes.insert(import.local_name.clone(), normalized);
    }

    for declaration in declarations {
        expand_declaration(
            &mut declaration.payload,
            &imported_shapes,
            &guard_predicates,
            &mut next_hygiene_id,
        )?;
    }
    Ok(())
}

fn collect_interface_shapes(
    module_name: &str,
    signatures: &[String],
) -> EbnfCompileResult<BTreeMap<String, ShapePattern>> {
    let source = format!("module {module_name}.\n\n{}\n", signatures.join("\n"));
    let module = parse_interface_module(&source)
        .map_err(|error| EbnfCompileError::Parse(error.message, error.span))?;
    collect_shapes(&module)
}

fn normalize_imported_shape(
    name: &str,
    shape: &ShapePattern,
    provider_shapes: &BTreeMap<String, ShapePattern>,
    next_hygiene_id: &mut usize,
) -> EbnfCompileResult<ShapePattern> {
    let mut stack = vec![name.to_string()];
    let expanded = expand_pattern(
        shape.body.clone(),
        provider_shapes,
        &mut stack,
        next_hygiene_id,
    )?;
    Ok(ShapePattern {
        params: shape.params.clone(),
        body: expanded.pattern,
        guard: combine_guards(expanded.guards, shape.guard.clone()),
    })
}

fn expand_declaration(
    declaration: &mut SyntaxDeclarationPayload,
    shapes: &BTreeMap<String, ShapePattern>,
    guard_predicates: &GuardPredicateDefinitions,
    next_hygiene_id: &mut usize,
) -> EbnfCompileResult<()> {
    match declaration {
        SyntaxDeclarationPayload::Constant { value, .. } => {
            expand_expr(value, shapes, guard_predicates, next_hygiene_id)?;
        }
        SyntaxDeclarationPayload::ConstFunction { params, body, .. } => {
            expand_param_defaults(params, shapes, guard_predicates, next_hygiene_id)?;
            expand_expr(body, shapes, guard_predicates, next_hygiene_id)?;
        }
        SyntaxDeclarationPayload::Function {
            params, clauses, ..
        }
        | SyntaxDeclarationPayload::Method {
            params, clauses, ..
        } => {
            expand_param_defaults(params, shapes, guard_predicates, next_hygiene_id)?;
            expand_function_clauses(clauses, shapes, guard_predicates, next_hygiene_id)?;
        }
        SyntaxDeclarationPayload::Constructor { clauses, .. } => {
            for clause in clauses {
                for param in &mut clause.params {
                    if let Some(default) = &mut param.default {
                        expand_expr(default, shapes, guard_predicates, next_hygiene_id)?;
                    }
                }
                expand_expr(&mut clause.body, shapes, guard_predicates, next_hygiene_id)?;
            }
        }
        SyntaxDeclarationPayload::Struct { fields, .. } => {
            for field in fields {
                if let Some(default) = &mut field.default {
                    expand_expr(default, shapes, guard_predicates, next_hygiene_id)?;
                }
            }
        }
        SyntaxDeclarationPayload::Trait { methods, .. } => {
            for method in methods {
                expand_param_defaults(
                    &mut method.params,
                    shapes,
                    guard_predicates,
                    next_hygiene_id,
                )?;
                if let Some(body) = &mut method.default_body {
                    expand_expr(body, shapes, guard_predicates, next_hygiene_id)?;
                }
            }
        }
        SyntaxDeclarationPayload::TraitImpl { methods, .. } => {
            for method in methods {
                expand_param_defaults(
                    &mut method.params,
                    shapes,
                    guard_predicates,
                    next_hygiene_id,
                )?;
                expand_function_clauses(
                    &mut method.clauses,
                    shapes,
                    guard_predicates,
                    next_hygiene_id,
                )?;
            }
        }
        SyntaxDeclarationPayload::Template { props, .. } => {
            for prop in props {
                if let Some(default) = &mut prop.default {
                    expand_expr(default, shapes, guard_predicates, next_hygiene_id)?;
                }
            }
        }
        SyntaxDeclarationPayload::Import { .. }
        | SyntaxDeclarationPayload::Export { .. }
        | SyntaxDeclarationPayload::Type { .. }
        | SyntaxDeclarationPayload::AnnotationSchema { .. }
        | SyntaxDeclarationPayload::Config { .. }
        | SyntaxDeclarationPayload::Raw { .. } => {}
    }
    Ok(())
}

fn expand_param_defaults(
    params: &mut [SyntaxParamOutput],
    shapes: &BTreeMap<String, ShapePattern>,
    guard_predicates: &GuardPredicateDefinitions,
    next_hygiene_id: &mut usize,
) -> EbnfCompileResult<()> {
    for param in params {
        if let Some(default) = &mut param.default {
            expand_expr(default, shapes, guard_predicates, next_hygiene_id)?;
        }
    }
    Ok(())
}

fn expand_function_clauses(
    clauses: &mut [SyntaxFunctionClauseOutput],
    shapes: &BTreeMap<String, ShapePattern>,
    guard_predicates: &GuardPredicateDefinitions,
    next_hygiene_id: &mut usize,
) -> EbnfCompileResult<()> {
    let mut shape_origins = Vec::with_capacity(clauses.len());
    for clause in clauses.iter_mut() {
        let (shape_guards, origins) =
            expand_patterns(&mut clause.patterns, shapes, next_hygiene_id)?;
        if let Some(guard) = &mut clause.guard {
            expand_expr(guard, shapes, guard_predicates, next_hygiene_id)?;
        }
        clause.guard = combine_guards(shape_guards, clause.guard.take());
        clause.has_guard = clause.guard.is_some();
        expand_expr(&mut clause.body, shapes, guard_predicates, next_hygiene_id)?;
        shape_origins.push(origins);
    }
    validate_function_clause_overlap(clauses, &shape_origins, guard_predicates)
}

fn expand_expr(
    expression: &mut SyntaxExprOutput,
    shapes: &BTreeMap<String, ShapePattern>,
    guard_predicates: &GuardPredicateDefinitions,
    next_hygiene_id: &mut usize,
) -> EbnfCompileResult<()> {
    reject_runtime_shape_call(expression, shapes)?;
    if expression.kind == SyntaxExprKind::Let {
        return expand_let_expr(expression, shapes, guard_predicates, next_hygiene_id);
    }
    let (shape_guards, _) = expand_patterns(&mut expression.patterns, shapes, next_hygiene_id)?;
    let accepts_shape_guards = expression.kind == SyntaxExprKind::ListComprehension;
    if !shape_guards.is_empty() && !accepts_shape_guards {
        return Err(EbnfCompileError::Serialize(
            "guarded shape aliases require a clause pattern with a guard channel".to_string(),
        ));
    }
    for child in &mut expression.children {
        expand_expr(child, shapes, guard_predicates, next_hygiene_id)?;
    }
    for field in &mut expression.fields {
        expand_expr(&mut field.value, shapes, guard_predicates, next_hygiene_id)?;
    }
    let mut clause_origins = Vec::with_capacity(expression.clauses.len());
    for clause in &mut expression.clauses {
        clause_origins.push(expand_clause(
            clause,
            shapes,
            guard_predicates,
            next_hygiene_id,
        )?);
    }
    validate_expr_clause_overlap(&expression.clauses, &clause_origins, guard_predicates)?;
    let mut catch_origins = Vec::with_capacity(expression.catch_clauses.len());
    for clause in &mut expression.catch_clauses {
        catch_origins.push(expand_clause(
            clause,
            shapes,
            guard_predicates,
            next_hygiene_id,
        )?);
    }
    validate_expr_clause_overlap(&expression.catch_clauses, &catch_origins, guard_predicates)?;
    if let Some(after) = &mut expression.try_after {
        expand_expr(
            &mut after.trigger,
            shapes,
            guard_predicates,
            next_hygiene_id,
        )?;
        expand_expr(&mut after.body, shapes, guard_predicates, next_hygiene_id)?;
    }
    for node in &mut expression.html_nodes {
        expand_html_node(node, shapes, guard_predicates, next_hygiene_id)?;
    }
    if accepts_shape_guards && !shape_guards.is_empty() {
        append_comprehension_shape_guards(expression, shape_guards)?;
    }
    Ok(())
}

/// Expands shape aliases in ordered let bindings.
///
/// Guarded aliases become ordinary one-clause case assertions. This gives the
/// generated guard the same bound-variable scope, purity checks, CoreIR, and
/// VM failure behavior as every other guarded pattern without adding a
/// shape-specific runtime form.
fn expand_let_expr(
    expression: &mut SyntaxExprOutput,
    shapes: &BTreeMap<String, ShapePattern>,
    guard_predicates: &GuardPredicateDefinitions,
    next_hygiene_id: &mut usize,
) -> EbnfCompileResult<()> {
    if expression.patterns.is_empty() || expression.children.len() != expression.patterns.len() + 1
    {
        return Err(EbnfCompileError::Serialize(
            "invalid let expression while expanding shape aliases".to_string(),
        ));
    }

    if !expression.clauses.is_empty() {
        for child in &mut expression.children {
            expand_expr(child, shapes, guard_predicates, next_hygiene_id)?;
        }
        let mut let_guards = Vec::with_capacity(expression.patterns.len());
        for pattern in &mut expression.patterns {
            let (mut shape_guards, _) =
                expand_patterns(std::slice::from_mut(pattern), shapes, next_hygiene_id)?;
            for guard in &mut shape_guards {
                expand_expr(guard, shapes, guard_predicates, next_hygiene_id)?;
            }
            let_guards.push(combine_guards(shape_guards, None).map(Box::new));
        }
        if let_guards.iter().any(Option::is_some) {
            expression.let_guards = let_guards;
        }
        let mut clause_origins = Vec::with_capacity(expression.clauses.len());
        for clause in &mut expression.clauses {
            clause_origins.push(expand_clause(
                clause,
                shapes,
                guard_predicates,
                next_hygiene_id,
            )?);
        }
        validate_expr_clause_overlap(&expression.clauses, &clause_origins, guard_predicates)?;
        return Ok(());
    }

    let span = expression.span;
    let mut values = std::mem::take(&mut expression.children);
    let mut continuation = values.pop().ok_or_else(|| {
        EbnfCompileError::Serialize(
            "let expression has no result while expanding shape aliases".to_string(),
        )
    })?;
    expand_expr(&mut continuation, shapes, guard_predicates, next_hygiene_id)?;

    let patterns = std::mem::take(&mut expression.patterns);
    let mut bindings = Vec::with_capacity(patterns.len());
    for (mut pattern, mut value) in patterns.into_iter().zip(values) {
        let (mut guards, _) =
            expand_patterns(std::slice::from_mut(&mut pattern), shapes, next_hygiene_id)?;
        expand_expr(&mut value, shapes, guard_predicates, next_hygiene_id)?;
        for guard in &mut guards {
            expand_expr(guard, shapes, guard_predicates, next_hygiene_id)?;
        }
        bindings.push((pattern, value, combine_guards(guards, None)));
    }

    for (pattern, value, guard) in bindings.into_iter().rev() {
        continuation = if let Some(guard) = guard {
            expr_node(
                SyntaxExprKind::Case,
                None,
                None,
                None,
                vec![value],
                Vec::new(),
                Vec::new(),
                vec![SyntaxClauseOutput {
                    patterns: vec![pattern],
                    guard: Some(Box::new(guard)),
                    body: Box::new(continuation),
                }],
                span,
            )
        } else {
            let mut let_expr = expr_node(
                SyntaxExprKind::Let,
                None,
                None,
                None,
                vec![value, continuation],
                vec![pattern],
                Vec::new(),
                Vec::new(),
                span,
            );
            let_expr.arity = 1;
            let_expr
        };
    }

    *expression = continuation;
    Ok(())
}

fn append_comprehension_shape_guards(
    expression: &mut SyntaxExprOutput,
    shape_guards: Vec<SyntaxExprOutput>,
) -> EbnfCompileResult<()> {
    let guard_index = expression.patterns.len() + 1;
    let explicit_guard = match expression.children.len() {
        len if len == guard_index => None,
        len if len == guard_index + 1 => expression.children.pop(),
        _ => {
            return Err(EbnfCompileError::Serialize(
                "invalid list comprehension shape while composing shape guards".to_string(),
            ));
        }
    };
    let guard = combine_guards(shape_guards, explicit_guard).ok_or_else(|| {
        EbnfCompileError::Serialize(
            "guarded shape comprehension produced no guard expression".to_string(),
        )
    })?;
    expression.children.push(guard);
    expression.arity = expression.patterns.len() + 3;
    Ok(())
}

fn expand_clause(
    clause: &mut SyntaxClauseOutput,
    shapes: &BTreeMap<String, ShapePattern>,
    guard_predicates: &GuardPredicateDefinitions,
    next_hygiene_id: &mut usize,
) -> EbnfCompileResult<BTreeSet<String>> {
    let (shape_guards, shape_origins) =
        expand_patterns(&mut clause.patterns, shapes, next_hygiene_id)?;
    if let Some(guard) = &mut clause.guard {
        expand_expr(guard, shapes, guard_predicates, next_hygiene_id)?;
    }
    clause.guard = combine_boxed_guards(shape_guards, clause.guard.take());
    expand_expr(&mut clause.body, shapes, guard_predicates, next_hygiene_id)?;
    Ok(shape_origins)
}

fn expand_html_node(
    node: &mut SyntaxHtmlNodeOutput,
    shapes: &BTreeMap<String, ShapePattern>,
    guard_predicates: &GuardPredicateDefinitions,
    next_hygiene_id: &mut usize,
) -> EbnfCompileResult<()> {
    match node {
        SyntaxHtmlNodeOutput::Text { .. } => Ok(()),
        SyntaxHtmlNodeOutput::Expr { expr } => {
            expand_expr(expr, shapes, guard_predicates, next_hygiene_id)
        }
        SyntaxHtmlNodeOutput::NamedSlot { slot } => {
            for child in &mut slot.children {
                expand_html_node(child, shapes, guard_predicates, next_hygiene_id)?;
            }
            Ok(())
        }
        SyntaxHtmlNodeOutput::Element { element } => {
            for attr in &mut element.attrs {
                if let Some(SyntaxHtmlAttrValueOutput::Expr { expr }) = &mut attr.value {
                    expand_expr(expr, shapes, guard_predicates, next_hygiene_id)?;
                }
            }
            for child in &mut element.children {
                expand_html_node(child, shapes, guard_predicates, next_hygiene_id)?;
            }
            Ok(())
        }
    }
}

fn expand_patterns(
    patterns: &mut [SyntaxPatternOutput],
    shapes: &BTreeMap<String, ShapePattern>,
    next_hygiene_id: &mut usize,
) -> EbnfCompileResult<(Vec<SyntaxExprOutput>, BTreeSet<String>)> {
    let mut guards = Vec::new();
    let mut shape_origins = BTreeSet::new();
    for pattern in patterns {
        let mut stack = Vec::new();
        let expanded = expand_pattern(pattern.clone(), shapes, &mut stack, next_hygiene_id)?;
        *pattern = expanded.pattern;
        guards.extend(expanded.guards);
        shape_origins.extend(expanded.shape_origins);
    }
    Ok((guards, shape_origins))
}

fn expand_pattern(
    mut pattern: SyntaxPatternOutput,
    shapes: &BTreeMap<String, ShapePattern>,
    stack: &mut Vec<String>,
    next_hygiene_id: &mut usize,
) -> EbnfCompileResult<ExpandedPattern> {
    let shape_name = pattern
        .text
        .as_ref()
        .filter(|_| pattern.kind == super::SyntaxPatternKind::Constructor)
        .filter(|name| shapes.contains_key(name.as_str()))
        .cloned();
    if let Some(name) = shape_name {
        if stack.iter().any(|active| active == &name) {
            stack.push(name);
            return Err(EbnfCompileError::Serialize(format!(
                "recursive shape expansion: {}",
                stack.join(" -> ")
            )));
        }
        let shape = &shapes[&name];
        if pattern.children.len() != shape.params.len() {
            return Err(EbnfCompileError::Serialize(format!(
                "shape `{name}` expects {} pattern argument(s), found {}",
                shape.params.len(),
                pattern.children.len()
            )));
        }
        let mut guards = Vec::new();
        let mut shape_origins = BTreeSet::new();
        let mut arguments = Vec::with_capacity(pattern.children.len());
        for child in pattern.children {
            let expanded = expand_pattern(child, shapes, stack, next_hygiene_id)?;
            arguments.push(expanded.pattern);
            guards.extend(expanded.guards);
            shape_origins.extend(expanded.shape_origins);
        }
        let mut substitutions = shape
            .params
            .iter()
            .cloned()
            .zip(arguments)
            .collect::<BTreeMap<_, _>>();
        add_private_binding_substitutions(shape, &mut substitutions, next_hygiene_id);
        stack.push(name.clone());
        let substituted = substitute_pattern(shape.body.clone(), &substitutions, &name)?;
        let expanded = expand_pattern(substituted, shapes, stack, next_hygiene_id);
        stack.pop();
        let expanded = expanded?;
        validate_expanded_shape_bindings(&name, &expanded.pattern)?;
        guards.extend(expanded.guards);
        shape_origins.extend(expanded.shape_origins);
        if let Some(guard) = &shape.guard {
            guards.push(substitute_guard_expr(guard.clone(), &substitutions, &name)?);
        }
        shape_origins.insert(name);
        return Ok(ExpandedPattern {
            pattern: expanded.pattern,
            guards,
            shape_origins,
        });
    }

    let mut guards = Vec::new();
    let mut shape_origins = BTreeSet::new();
    let mut children = Vec::with_capacity(pattern.children.len());
    for child in pattern.children {
        let expanded = expand_pattern(child, shapes, stack, next_hygiene_id)?;
        children.push(expanded.pattern);
        guards.extend(expanded.guards);
        shape_origins.extend(expanded.shape_origins);
    }
    pattern.children = children;
    for field in &mut pattern.fields {
        let expanded = expand_pattern((*field.value).clone(), shapes, stack, next_hygiene_id)?;
        *field.value = expanded.pattern;
        guards.extend(expanded.guards);
        shape_origins.extend(expanded.shape_origins);
    }
    pattern.arity = if pattern.fields.is_empty() {
        pattern.children.len()
    } else {
        pattern.fields.len()
    };
    Ok(ExpandedPattern {
        pattern,
        guards,
        shape_origins,
    })
}

fn validate_expanded_shape_bindings(
    shape_name: &str,
    pattern: &SyntaxPatternOutput,
) -> EbnfCompileResult<()> {
    let mut seen = BTreeSet::new();
    if let Some(binding) = duplicate_pattern_binding(pattern, &mut seen) {
        return Err(EbnfCompileError::Serialize(format!(
            "shape `{shape_name}` expansion binds `{binding}` more than once; overlapping shape arguments are ambiguous"
        )));
    }
    Ok(())
}

fn substitute_guard_expr(
    mut expression: SyntaxExprOutput,
    substitutions: &BTreeMap<String, SyntaxPatternOutput>,
    shape_name: &str,
) -> EbnfCompileResult<SyntaxExprOutput> {
    if expression.kind == SyntaxExprKind::Var {
        if let Some((param, pattern)) = expression
            .text
            .as_ref()
            .and_then(|name| substitutions.get_key_value(name))
        {
            return guard_value_from_pattern(pattern).ok_or_else(|| {
                EbnfCompileError::Serialize(format!(
                    "shape `{shape_name}` guard references parameter `{param}` with a non-value pattern argument"
                ))
            });
        }
    }
    expression.children = expression
        .children
        .into_iter()
        .map(|child| substitute_guard_expr(child, substitutions, shape_name))
        .collect::<EbnfCompileResult<Vec<_>>>()?;
    expression.let_guards = expression
        .let_guards
        .into_iter()
        .map(|guard| {
            guard
                .map(|guard| substitute_guard_expr(*guard, substitutions, shape_name).map(Box::new))
                .transpose()
        })
        .collect::<EbnfCompileResult<Vec<_>>>()?;
    for field in &mut expression.fields {
        *field.value = substitute_guard_expr((*field.value).clone(), substitutions, shape_name)?;
    }
    for clause in &mut expression.clauses {
        substitute_clause_guard_expr(clause, substitutions, shape_name)?;
    }
    for clause in &mut expression.catch_clauses {
        substitute_clause_guard_expr(clause, substitutions, shape_name)?;
    }
    if let Some(after) = &mut expression.try_after {
        *after.trigger =
            substitute_guard_expr((*after.trigger).clone(), substitutions, shape_name)?;
        *after.body = substitute_guard_expr((*after.body).clone(), substitutions, shape_name)?;
    }
    for node in &mut expression.html_nodes {
        substitute_html_guard_expr(node, substitutions, shape_name)?;
    }
    Ok(expression)
}

fn substitute_clause_guard_expr(
    clause: &mut SyntaxClauseOutput,
    substitutions: &BTreeMap<String, SyntaxPatternOutput>,
    shape_name: &str,
) -> EbnfCompileResult<()> {
    if let Some(guard) = &mut clause.guard {
        **guard = substitute_guard_expr((**guard).clone(), substitutions, shape_name)?;
    }
    *clause.body = substitute_guard_expr((*clause.body).clone(), substitutions, shape_name)?;
    Ok(())
}

fn substitute_html_guard_expr(
    node: &mut SyntaxHtmlNodeOutput,
    substitutions: &BTreeMap<String, SyntaxPatternOutput>,
    shape_name: &str,
) -> EbnfCompileResult<()> {
    match node {
        SyntaxHtmlNodeOutput::Text { .. } => Ok(()),
        SyntaxHtmlNodeOutput::Expr { expr } => {
            **expr = substitute_guard_expr((**expr).clone(), substitutions, shape_name)?;
            Ok(())
        }
        SyntaxHtmlNodeOutput::NamedSlot { slot } => {
            for child in &mut slot.children {
                substitute_html_guard_expr(child, substitutions, shape_name)?;
            }
            Ok(())
        }
        SyntaxHtmlNodeOutput::Element { element } => {
            for attr in &mut element.attrs {
                if let Some(SyntaxHtmlAttrValueOutput::Expr { expr }) = &mut attr.value {
                    **expr = substitute_guard_expr((**expr).clone(), substitutions, shape_name)?;
                }
            }
            for child in &mut element.children {
                substitute_html_guard_expr(child, substitutions, shape_name)?;
            }
            Ok(())
        }
    }
}

fn guard_value_from_pattern(pattern: &SyntaxPatternOutput) -> Option<SyntaxExprOutput> {
    let kind = match pattern.kind {
        SyntaxPatternKind::Var => SyntaxExprKind::Var,
        SyntaxPatternKind::Int => SyntaxExprKind::Int,
        SyntaxPatternKind::Float => SyntaxExprKind::Float,
        SyntaxPatternKind::String => SyntaxExprKind::Binary,
        SyntaxPatternKind::Atom => SyntaxExprKind::Atom,
        SyntaxPatternKind::Alias => {
            return pattern.text.as_ref().map(|alias| {
                expr_node(
                    SyntaxExprKind::Var,
                    Some(alias.clone()),
                    None,
                    None,
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    Default::default(),
                )
            });
        }
        _ => return None,
    };
    Some(expr_node(
        kind,
        pattern.text.clone(),
        None,
        None,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Default::default(),
    ))
}

fn combine_guards(
    shape_guards: Vec<SyntaxExprOutput>,
    explicit_guard: Option<SyntaxExprOutput>,
) -> Option<SyntaxExprOutput> {
    shape_guards
        .into_iter()
        .chain(explicit_guard)
        .reduce(and_guard)
}

fn combine_boxed_guards(
    shape_guards: Vec<SyntaxExprOutput>,
    explicit_guard: Option<Box<SyntaxExprOutput>>,
) -> Option<Box<SyntaxExprOutput>> {
    combine_guards(shape_guards, explicit_guard.map(|guard| *guard)).map(Box::new)
}

fn and_guard(left: SyntaxExprOutput, right: SyntaxExprOutput) -> SyntaxExprOutput {
    expr_node(
        SyntaxExprKind::BinaryOp,
        None,
        Some("and".to_string()),
        None,
        vec![left, right],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Default::default(),
    )
}

fn add_private_binding_substitutions(
    shape: &ShapePattern,
    substitutions: &mut BTreeMap<String, SyntaxPatternOutput>,
    next_hygiene_id: &mut usize,
) {
    let mut bindings = BTreeSet::new();
    collect_pattern_bindings(&shape.body, &mut bindings);
    bindings.retain(|binding| !shape.params.iter().any(|param| param == binding));
    if bindings.is_empty() {
        return;
    }

    let hygiene_id = *next_hygiene_id;
    *next_hygiene_id += 1;
    for binding in bindings {
        substitutions.insert(
            binding.clone(),
            SyntaxPatternOutput {
                kind: SyntaxPatternKind::Var,
                arity: 0,
                text: Some(format!("#shape_{hygiene_id}_{binding}")),
                children: Vec::new(),
                fields: Vec::new(),
            },
        );
    }
}

fn collect_pattern_bindings(pattern: &SyntaxPatternOutput, bindings: &mut BTreeSet<String>) {
    if pattern.kind == SyntaxPatternKind::BinaryLayout {
        binary_layout::collect_capture_bindings(pattern, bindings);
        return;
    }
    if let Some(binding) = pattern_binding_name(pattern) {
        bindings.insert(binding.to_string());
    }
    for child in &pattern.children {
        collect_pattern_bindings(child, bindings);
    }
    for field in &pattern.fields {
        collect_pattern_bindings(&field.value, bindings);
    }
}

fn pattern_binding_name(pattern: &SyntaxPatternOutput) -> Option<&str> {
    match pattern.kind {
        SyntaxPatternKind::Var | SyntaxPatternKind::Alias => pattern.text.as_deref(),
        SyntaxPatternKind::StringCapture => pattern.text.as_deref().map(string_capture_name),
        _ => None,
    }
}

fn substitute_pattern(
    mut pattern: SyntaxPatternOutput,
    substitutions: &BTreeMap<String, SyntaxPatternOutput>,
    shape_name: &str,
) -> EbnfCompileResult<SyntaxPatternOutput> {
    if pattern.kind == SyntaxPatternKind::BinaryLayout {
        binary_layout::substitute_captures(&mut pattern, substitutions, shape_name)?;
        return Ok(pattern);
    }
    if pattern.kind == super::SyntaxPatternKind::Var {
        if let Some(replacement) = pattern
            .text
            .as_ref()
            .and_then(|name| substitutions.get(name))
        {
            return Ok(replacement.clone());
        }
    }
    if pattern.kind == SyntaxPatternKind::Alias {
        if let Some(replacement) = pattern
            .text
            .as_ref()
            .and_then(|name| substitutions.get(name))
            .and_then(|replacement| replacement.text.as_ref())
        {
            pattern.text = Some(replacement.clone());
        }
    }
    if pattern.kind == SyntaxPatternKind::StringCapture {
        substitute_string_capture(&mut pattern, substitutions, shape_name)?;
    }
    pattern.children = pattern
        .children
        .into_iter()
        .map(|child| substitute_pattern(child, substitutions, shape_name))
        .collect::<EbnfCompileResult<Vec<_>>>()?;
    for field in &mut pattern.fields {
        *field.value = substitute_pattern((*field.value).clone(), substitutions, shape_name)?;
    }
    if pattern.kind == SyntaxPatternKind::StringPattern {
        pattern.text = Some(rewrite_string_pattern_text(
            pattern.text.as_deref().unwrap_or_default(),
            &pattern.children,
            shape_name,
        )?);
    }
    Ok(pattern)
}

fn substitute_string_capture(
    capture: &mut SyntaxPatternOutput,
    substitutions: &BTreeMap<String, SyntaxPatternOutput>,
    shape_name: &str,
) -> EbnfCompileResult<()> {
    let Some(text) = capture.text.as_deref() else {
        return Ok(());
    };
    let name = string_capture_name(text);
    let Some(replacement) = substitutions.get(name) else {
        return Ok(());
    };
    let replacement_name = match replacement.kind {
        SyntaxPatternKind::Var | SyntaxPatternKind::Alias => replacement.text.as_deref(),
        SyntaxPatternKind::Wildcard => Some("_"),
        _ => None,
    }
    .ok_or_else(|| {
        EbnfCompileError::Serialize(format!(
            "shape `{shape_name}` string capture parameter `{name}` requires a variable, alias, or wildcard pattern argument"
        ))
    })?;
    let annotation = text
        .split_once(':')
        .map(|(_, annotation)| annotation.trim());
    capture.text = Some(match annotation {
        Some(annotation) => format!("{replacement_name}: {annotation}"),
        None => replacement_name.to_string(),
    });
    Ok(())
}
