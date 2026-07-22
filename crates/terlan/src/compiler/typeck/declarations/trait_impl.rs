use super::purity::trait_method_requires_pure_body;
use super::*;

/// Shared type and effect environment for explicit impl method checking.
pub(super) struct TraitImplCheckContext<'a> {
    pub(super) alias_names: &'a HashSet<String>,
    pub(super) aliases: &'a HashMap<String, TypeAlias>,
    pub(super) imported_type_names: &'a HashMap<String, QualifiedTypeName>,
    pub(super) imported_type_aliases: &'a HashMap<String, TypeAlias>,
    pub(super) local_aliases: &'a HashMap<String, TypeAlias>,
    pub(super) expr_ctx: &'a ExprInferContext<'a>,
    pub(super) effectful_local_calls: &'a HashSet<(String, usize)>,
    pub(super) effectful_imported_calls: &'a ImportedEffectFacts,
}

/// Checks bodies and signatures owned by one explicit trait implementation.
pub(super) fn check_trait_impl_methods(
    trait_ref: &SyntaxTypeOutput,
    generic_params: &[String],
    methods: &[SyntaxImplMethodOutput],
    ctx: &TraitImplCheckContext<'_>,
    trait_inheritance_cache: &mut HashMap<String, Option<HashMap<String, TraitMethodSignature>>>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for method in methods {
        let requires_trait_purity = trait_method_requires_pure_body(
            ctx.expr_ctx,
            &trait_ref.text,
            &method.name,
            trait_inheritance_cache,
        );
        let scheme = function_decl_to_scheme(
            &method
                .params
                .iter()
                .map(|param| param.annotation.text.clone())
                .collect::<Vec<_>>(),
            &method.return_type.text,
            generic_params,
            &method.generic_bounds,
            ctx.alias_names,
            ctx.imported_type_names,
            ctx.imported_type_aliases,
            ctx.local_aliases,
        );

        check_callable_alias_implications(
            &method.params,
            &method.return_type,
            generic_params,
            &scheme.bounds,
            ctx.alias_names,
            ctx.expr_ctx,
            diagnostics,
        );
        check_syntax_param_defaults(
            &method.params,
            &scheme.params,
            ctx.aliases,
            ctx.expr_ctx,
            diagnostics,
        );
        check_syntax_callable_clauses(
            &format!("impl method {}", method.name),
            &method.name,
            &method.params,
            &method.clauses,
            &scheme,
            method.span.into(),
            ctx.alias_names,
            ctx.aliases,
            ctx.expr_ctx,
            diagnostics,
            requires_trait_purity,
            requires_trait_purity,
            ctx.effectful_local_calls,
            ctx.effectful_imported_calls,
        );
    }
}
