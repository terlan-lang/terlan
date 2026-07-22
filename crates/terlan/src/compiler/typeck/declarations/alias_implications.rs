use super::*;
use crate::terlan_syntax::SyntaxTraitMethodOutput;

/// Checks implication-constrained aliases in callable annotations.
pub(super) fn check_callable_alias_implications(
    params: &[SyntaxParamOutput],
    return_type: &SyntaxTypeOutput,
    generic_params: &[String],
    current_bounds: &[FunctionBound],
    alias_names: &HashSet<String>,
    expr_ctx: &ExprInferContext<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let (mut vars, mut next_var) = generic_type_variables(generic_params);
    let annotations = params
        .iter()
        .map(|param| &param.annotation)
        .chain(std::iter::once(return_type));
    check_alias_annotations(
        annotations,
        &mut vars,
        &mut next_var,
        current_bounds,
        alias_names,
        expr_ctx,
        diagnostics,
    );
}

/// Checks constrained aliases in declaration annotations without bodies.
pub(super) fn check_non_callable_alias_implications(
    declaration: &SyntaxDeclarationOutput,
    alias_names: &HashSet<String>,
    expr_ctx: &ExprInferContext<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if let SyntaxDeclarationPayload::Trait {
        params, methods, ..
    } = &declaration.payload
    {
        check_trait_method_alias_implications(params, methods, alias_names, expr_ctx, diagnostics);
        return;
    }

    let (generic_params, annotations): (&[String], Vec<&SyntaxTypeOutput>) =
        match &declaration.payload {
            SyntaxDeclarationPayload::Type {
                params, variants, ..
            } => (params, variants.iter().collect()),
            SyntaxDeclarationPayload::Struct {
                generic_params,
                fields,
                ..
            } => (
                generic_params,
                fields.iter().map(|field| &field.annotation).collect(),
            ),
            SyntaxDeclarationPayload::Constructor {
                params, clauses, ..
            } => (
                params,
                clauses
                    .iter()
                    .flat_map(|clause| {
                        clause
                            .params
                            .iter()
                            .map(|param| &param.annotation)
                            .chain(std::iter::once(&clause.return_type))
                    })
                    .collect(),
            ),
            SyntaxDeclarationPayload::Template { props, .. } => {
                (&[], props.iter().map(|prop| &prop.annotation).collect())
            }
            _ => return,
        };

    let (mut vars, mut next_var) = generic_type_variables(generic_params);
    let bounds = parse_structural_implication_bounds(generic_params, &vars, alias_names);
    check_alias_annotations(
        annotations,
        &mut vars,
        &mut next_var,
        &bounds,
        alias_names,
        expr_ctx,
        diagnostics,
    );
}

fn check_trait_method_alias_implications(
    trait_params: &[String],
    methods: &[SyntaxTraitMethodOutput],
    alias_names: &HashSet<String>,
    expr_ctx: &ExprInferContext<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for method in methods {
        let generic_params = trait_params
            .iter()
            .chain(method.generic_params.iter())
            .cloned()
            .collect::<Vec<_>>();
        let (mut vars, mut next_var) = generic_type_variables(&generic_params);
        let mut bounds =
            parse_structural_implication_bounds(&method.generic_params, &vars, alias_names);
        bounds.extend(parse_generic_bounds(
            &method.generic_bounds,
            &vars,
            alias_names,
        ));
        let annotations = method
            .params
            .iter()
            .map(|param| &param.annotation)
            .chain(std::iter::once(&method.return_type));
        check_alias_annotations(
            annotations,
            &mut vars,
            &mut next_var,
            &bounds,
            alias_names,
            expr_ctx,
            diagnostics,
        );
    }
}

fn generic_type_variables(generic_params: &[String]) -> (HashMap<String, TypeVarId>, TypeVarId) {
    let mut vars = HashMap::new();
    let mut next_var = 0;
    for param in generic_params {
        vars.insert(normalize_type_param_name(param), next_var);
        next_var += 1;
    }
    (vars, next_var)
}

fn check_alias_annotations<'a>(
    annotations: impl IntoIterator<Item = &'a SyntaxTypeOutput>,
    vars: &mut HashMap<String, TypeVarId>,
    next_var: &mut TypeVarId,
    current_bounds: &[FunctionBound],
    alias_names: &HashSet<String>,
    expr_ctx: &ExprInferContext<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let bounded_ctx = expr_ctx_with_current_bounds(expr_ctx, current_bounds);
    for annotation in annotations {
        let Some(ty) = parse_type_expr(&annotation.text, alias_names, vars, next_var) else {
            continue;
        };
        diagnostics.extend(
            check_type_alias_implication_bounds(&ty, &bounded_ctx)
                .into_iter()
                .map(|message| Diagnostic {
                    span: annotation.span.into(),
                    message,
                    severity: DiagSeverity::Error,
                }),
        );
    }
}
