mod alias_implications;
mod purity;
mod trait_impl;

use self::alias_implications::{
    check_callable_alias_implications, check_non_callable_alias_implications,
};
use self::purity::{
    collect_effectful_imported_calls, collect_effectful_local_calls, effectful_call_facts,
    receiver_method_requires_pure_body, ImportedEffectFacts,
};
use self::trait_impl::{check_trait_impl_methods, TraitImplCheckContext};
use super::expression::{
    check_clause_guard_purity, check_pure_expression_effects_with_call_facts,
    refine_by_syntax_guard,
};
use super::*;

#[path = "declarations/exhaustiveness.rs"]
mod exhaustiveness;
use crate::terlan_syntax::SyntaxConstructorClauseOutput;
use exhaustiveness::check_syntax_function_clause_exhaustiveness;

/// Checks function-like declarations in a syntax-output module.
///
/// Inputs:
/// - `module`: syntax-output module containing functions, receiver methods,
///   and explicit trait implementation methods.
/// - `function_signatures`: local function signature candidates keyed by name
///   and arity.
/// - `constructor_signatures`: explicit constructor schemes keyed by target
///   type name.
/// - `alias_names`, `aliases`, and imported/local alias maps: visible type
///   information used to parse method signatures and check patterns.
/// - `expr_ctx`: shared expression-inference context for callable bodies.
///
/// Output:
/// - One diagnostic per invalid callable declaration body, missing signature,
///   pattern error, trait-bound failure, or return-type mismatch.
///
/// Transformation:
/// - Dispatches each function-like declaration to the common clause checker,
///   adapting receiver methods into receiver-first callable clauses without
///   changing the syntax output model.
pub(super) fn check_syntax_module_functions(
    module: &SyntaxModuleOutput,
    function_signatures: &HashMap<(String, usize), Vec<FunctionScheme>>,
    constructor_signatures: &HashMap<String, Vec<ConstructorScheme>>,
    alias_names: &HashSet<String>,
    aliases: &HashMap<String, TypeAlias>,
    imported_type_names: &HashMap<String, QualifiedTypeName>,
    imported_type_aliases: &HashMap<String, TypeAlias>,
    local_aliases: &HashMap<String, TypeAlias>,
    expr_ctx: &ExprInferContext,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let effectful_imported_calls = collect_effectful_imported_calls(expr_ctx);
    let effectful_local_calls = collect_effectful_local_calls(
        module,
        function_signatures,
        aliases,
        expr_ctx.templates,
        &effectful_imported_calls,
    );
    let mut trait_inheritance_cache = HashMap::new();
    for declaration in &module.declarations {
        if is_compiler_intrinsic_declaration(declaration) {
            continue;
        }

        check_non_callable_alias_implications(declaration, alias_names, expr_ctx, &mut diagnostics);

        match &declaration.payload {
            SyntaxDeclarationPayload::Function {
                name,
                generic_params,
                params,
                return_type,
                clauses,
                generic_bounds,
                ..
            } => {
                let key = (name.clone(), params.len());
                let schemes = match function_signatures.get(&key) {
                    Some(schemes) => schemes,
                    None => {
                        diagnostics.push(Diagnostic {
                            span: declaration.span.into(),
                            message: format!(
                                "missing type signature for function {} / {}",
                                name,
                                params.len()
                            ),
                            severity: DiagSeverity::Error,
                        });
                        continue;
                    }
                };
                let scheme = function_decl_to_scheme(
                    &params
                        .iter()
                        .map(|param| param.annotation.text.clone())
                        .collect::<Vec<_>>(),
                    &return_type.text,
                    generic_params,
                    generic_bounds,
                    alias_names,
                    imported_type_names,
                    imported_type_aliases,
                    local_aliases,
                );
                let scheme = schemes
                    .iter()
                    .find(|candidate| {
                        candidate.params == scheme.params
                            && candidate.ret == scheme.ret
                            && candidate.bounds.len() == scheme.bounds.len()
                    })
                    .cloned()
                    .unwrap_or(scheme);

                check_callable_alias_implications(
                    params,
                    return_type,
                    generic_params,
                    &scheme.bounds,
                    alias_names,
                    expr_ctx,
                    &mut diagnostics,
                );

                check_syntax_param_defaults(
                    params,
                    &scheme.params,
                    aliases,
                    expr_ctx,
                    &mut diagnostics,
                );
                check_syntax_callable_clauses(
                    &format!("function {}", name),
                    name,
                    params,
                    clauses,
                    &scheme,
                    declaration.span.into(),
                    alias_names,
                    aliases,
                    expr_ctx,
                    &mut diagnostics,
                    declaration_has_marker_annotation(declaration, &["pure"]),
                    false,
                    &effectful_local_calls,
                    &effectful_imported_calls,
                );
            }
            SyntaxDeclarationPayload::TraitImpl {
                trait_ref,
                generic_params,
                methods,
                ..
            } => {
                check_trait_impl_methods(
                    trait_ref,
                    generic_params,
                    methods,
                    &TraitImplCheckContext {
                        alias_names,
                        aliases,
                        imported_type_names,
                        imported_type_aliases,
                        local_aliases,
                        expr_ctx,
                        effectful_local_calls: &effectful_local_calls,
                        effectful_imported_calls: &effectful_imported_calls,
                    },
                    &mut trait_inheritance_cache,
                    &mut diagnostics,
                );
            }
            SyntaxDeclarationPayload::Method {
                receiver,
                name,
                generic_params,
                params,
                return_type,
                clauses,
                generic_bounds,
                ..
            } => {
                let requires_trait_purity = receiver_method_requires_pure_body(
                    module,
                    expr_ctx,
                    &receiver.annotation.text,
                    name,
                    &mut trait_inheritance_cache,
                );
                let mut receiver_first_params = Vec::with_capacity(params.len() + 1);
                receiver_first_params.push(receiver.clone());
                receiver_first_params.extend(params.iter().cloned());
                let scheme = function_decl_to_scheme(
                    &receiver_first_params
                        .iter()
                        .map(|param| param.annotation.text.clone())
                        .collect::<Vec<_>>(),
                    mutable_receiver_internal_return_type(receiver, return_type),
                    generic_params,
                    generic_bounds,
                    alias_names,
                    imported_type_names,
                    imported_type_aliases,
                    local_aliases,
                );

                check_callable_alias_implications(
                    &receiver_first_params,
                    return_type,
                    generic_params,
                    &scheme.bounds,
                    alias_names,
                    expr_ctx,
                    &mut diagnostics,
                );

                check_syntax_param_defaults(
                    &receiver_first_params,
                    &scheme.params,
                    aliases,
                    expr_ctx,
                    &mut diagnostics,
                );
                check_syntax_callable_clauses(
                    &format!("receiver method {}", name),
                    name,
                    &receiver_first_params,
                    &receiver_method_clauses_with_bindings(receiver, params, clauses),
                    &scheme,
                    declaration.span.into(),
                    alias_names,
                    aliases,
                    expr_ctx,
                    &mut diagnostics,
                    declaration_has_marker_annotation(declaration, &["pure"])
                        || requires_trait_purity,
                    requires_trait_purity,
                    &effectful_local_calls,
                    &effectful_imported_calls,
                );
            }
            SyntaxDeclarationPayload::Trait { methods, .. } => {
                for method in methods {
                    let scheme = function_decl_to_scheme(
                        &method
                            .params
                            .iter()
                            .map(|param| param.annotation.text.clone())
                            .collect::<Vec<_>>(),
                        &method.return_type.text,
                        &Vec::new(),
                        &method.generic_bounds,
                        alias_names,
                        imported_type_names,
                        imported_type_aliases,
                        local_aliases,
                    );
                    check_syntax_param_defaults(
                        &method.params,
                        &scheme.params,
                        aliases,
                        expr_ctx,
                        &mut diagnostics,
                    );
                    if method.is_pure {
                        if let Some(default_body) = &method.default_body {
                            let facts = effectful_call_facts(
                                &effectful_local_calls,
                                &effectful_imported_calls,
                            );
                            let mut errors = Vec::new();
                            check_pure_expression_effects_with_call_facts(
                                default_body,
                                &format!(
                                    "trait default method {} required pure by its contract",
                                    method.name
                                ),
                                expr_ctx.templates,
                                &facts,
                                &mut errors,
                            );
                            diagnostics.extend(errors.into_iter().map(|error| {
                                expression_error_to_diagnostic(error, method.span.into())
                            }));
                        }
                    }
                }
            }
            SyntaxDeclarationPayload::Constructor { name, clauses, .. } => {
                let Some(schemes) = constructor_signatures.get(name) else {
                    diagnostics.push(Diagnostic {
                        span: declaration.span.into(),
                        message: format!("missing type signature for constructor {}", name),
                        severity: DiagSeverity::Error,
                    });
                    continue;
                };
                check_syntax_constructor_clauses(
                    name,
                    clauses,
                    schemes,
                    aliases,
                    expr_ctx,
                    &mut diagnostics,
                );
            }
            _ => {}
        }
    }
    diagnostics
}

/// Checks default parameter expressions for callable declarations.
///
/// Inputs:
/// - `params`: syntax-output parameters carrying optional defaults.
/// - `expected_types`: parsed parameter types in declaration order.
/// - `aliases`: visible type aliases used when comparing inferred defaults.
/// - `expr_ctx`: expression inference context for primitive and literal rules.
/// - `diagnostics`: output diagnostic sink.
///
/// Output:
/// - No direct return value.
///
/// Transformation:
/// - Rejects non-constant defaults for the 0.0.5 language slice, infers each
///   constant default expression with an empty local scope, and unifies the
///   inferred type with the declared parameter type.
fn check_syntax_param_defaults(
    params: &[SyntaxParamOutput],
    expected_types: &[Type],
    aliases: &HashMap<String, TypeAlias>,
    expr_ctx: &ExprInferContext,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for (param, expected_type) in params.iter().zip(expected_types.iter()) {
        let Some(default_expr) = &param.default else {
            continue;
        };
        check_syntax_default_expr_against_type(
            &param.name,
            default_expr,
            expected_type,
            aliases,
            expr_ctx,
            diagnostics,
        );
    }
}

/// Checks explicit constructor clause bodies.
///
/// Inputs:
/// - `constructor_name`: target type named by the constructor declaration.
/// - `clauses`: syntax-output constructor clauses.
/// - `schemes`: parsed constructor schemes aligned with `clauses`.
/// - `aliases`: visible aliases for expected/inferred comparison.
/// - `expr_ctx`: module-wide expression inference context.
/// - `diagnostics`: output diagnostic buffer.
///
/// Output:
/// - No direct return value.
///
/// Transformation:
/// - Builds local bindings from constructor parameters, marks the constructor
///   target as active for internal struct initialization, infers each body, and
///   unifies it with the clause return type.
fn check_syntax_constructor_clauses(
    constructor_name: &str,
    clauses: &[SyntaxConstructorClauseOutput],
    schemes: &[ConstructorScheme],
    aliases: &HashMap<String, TypeAlias>,
    expr_ctx: &ExprInferContext,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for (clause, scheme) in clauses.iter().zip(schemes.iter()) {
        let span = clause.span.into();
        let mut subst = HashMap::new();
        let mut locals = HashMap::new();
        let mut fixed_param_index = 0usize;

        for param in &clause.params {
            let ty = if param.is_varargs {
                scheme
                    .vararg
                    .as_ref()
                    .map(|item| Type::List(Box::new(item.clone())))
                    .unwrap_or(Type::Dynamic)
            } else {
                let ty = scheme
                    .fixed_params
                    .get(fixed_param_index)
                    .cloned()
                    .unwrap_or(Type::Dynamic);
                fixed_param_index += 1;
                ty
            };
            locals.insert(param.name.clone(), ty);
        }

        let constructor_expr_ctx = expr_ctx_with_current_constructor(expr_ctx, constructor_name);
        let mut local_errors = Vec::new();
        let expected_return = expand_type_aliases(&scheme.ret, aliases);
        let inferred = infer_syntax_expr_with_expected(
            &clause.body,
            &expected_return,
            &locals,
            &constructor_expr_ctx,
            &mut subst,
            &mut local_errors,
        )
        .unwrap_or_else(|| {
            infer_syntax_expr(
                &clause.body,
                &locals,
                &constructor_expr_ctx,
                &mut subst,
                &mut local_errors,
            )
        });

        for error in local_errors {
            diagnostics.push(expression_error_to_diagnostic(error, span));
        }

        let inferred_expanded = expand_type_aliases(&inferred, aliases);
        if let Err(message) = unify(&expected_return, &inferred_expanded, &mut subst) {
            diagnostics.push(Diagnostic {
                span,
                message: format!("constructor `{}` body {}", constructor_name, message),
                severity: DiagSeverity::Error,
            });
        }
    }
}

/// Checks default parameter expressions for constructor declarations.
///
/// Inputs:
/// - `module`: syntax-output module containing constructor declarations.
/// - `constructor_signatures`: parsed constructor schemes by constructor name.
/// - `aliases`: visible type aliases used when comparing inferred defaults.
/// - `expr_ctx`: expression inference context for primitive and literal rules.
///
/// Output:
/// - Diagnostics for dynamic or type-mismatched constructor defaults.
///
/// Transformation:
/// - Aligns constructor clauses with their collected schemes, then delegates
///   each non-vararg default expression to the same constant/type checker used
///   for ordinary callable defaults.
pub(super) fn check_syntax_constructor_param_defaults(
    module: &SyntaxModuleOutput,
    constructor_signatures: &HashMap<String, Vec<ConstructorScheme>>,
    aliases: &HashMap<String, TypeAlias>,
    expr_ctx: &ExprInferContext,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::Constructor { name, clauses, .. } = &declaration.payload
        else {
            continue;
        };
        let Some(schemes) = constructor_signatures.get(name) else {
            continue;
        };

        for (clause, scheme) in clauses.iter().zip(schemes.iter()) {
            for (param, expected_type) in clause
                .params
                .iter()
                .filter(|param| !param.is_varargs)
                .zip(scheme.fixed_params.iter())
            {
                let Some(default_expr) = &param.default else {
                    continue;
                };
                check_syntax_constructor_default_expr(
                    param,
                    default_expr,
                    expected_type,
                    aliases,
                    expr_ctx,
                    &mut diagnostics,
                );
            }
        }
    }

    diagnostics
}

/// Checks one constructor default expression.
///
/// Inputs:
/// - `param`: constructor parameter carrying the diagnostic name.
/// - `default_expr`: default expression attached to the parameter.
/// - `expected_type`: parsed constructor parameter type.
/// - `aliases`, `expr_ctx`, and `diagnostics`: shared typechecking context.
///
/// Output:
/// - No direct return value.
///
/// Transformation:
/// - Delegates to the shared default-expression checker while adapting the
///   constructor parameter DTO to a name/default pair.
fn check_syntax_constructor_default_expr(
    param: &SyntaxConstructorParamOutput,
    default_expr: &SyntaxExprOutput,
    expected_type: &Type,
    aliases: &HashMap<String, TypeAlias>,
    expr_ctx: &ExprInferContext,
    diagnostics: &mut Vec<Diagnostic>,
) {
    check_syntax_default_expr_against_type(
        &param.name,
        default_expr,
        expected_type,
        aliases,
        expr_ctx,
        diagnostics,
    );
}

/// Checks one default expression against one expected parameter type.
///
/// Inputs:
/// - `param_name`: source parameter name for diagnostics.
/// - `default_expr`: syntax-output default expression.
/// - `expected_type`: parsed expected parameter type.
/// - `aliases`: visible aliases for comparison.
/// - `expr_ctx`: expression inference context.
/// - `diagnostics`: output diagnostic sink.
///
/// Output:
/// - No direct return value.
///
/// Transformation:
/// - Enforces the 0.0.5 compile-time-constant restriction, infers the default
///   expression with no local bindings, and unifies it with the expected type.
fn check_syntax_default_expr_against_type(
    param_name: &str,
    default_expr: &SyntaxExprOutput,
    expected_type: &Type,
    aliases: &HashMap<String, TypeAlias>,
    expr_ctx: &ExprInferContext,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let span: Span = default_expr.span.into();
    if !is_compile_time_default_expr(default_expr) {
        diagnostics.push(Diagnostic {
            span,
            message: format!(
                "default value for parameter `{}` must be a compile-time constant",
                param_name
            ),
            severity: DiagSeverity::Error,
        });
        return;
    }

    let mut subst = HashMap::new();
    let mut local_errors = Vec::new();
    let locals = HashMap::new();
    let inferred = infer_syntax_expr(
        default_expr,
        &locals,
        expr_ctx,
        &mut subst,
        &mut local_errors,
    );
    for error in local_errors {
        diagnostics.push(expression_error_to_diagnostic(error, span));
    }

    let expected_expanded = expand_type_aliases(expected_type, aliases);
    let inferred_expanded = expand_type_aliases(&inferred, aliases);
    if let Err(message) = unify(&expected_expanded, &inferred_expanded, &mut subst) {
        diagnostics.push(Diagnostic {
            span,
            message: format!("default value for parameter `{}` {}", param_name, message),
            severity: DiagSeverity::Error,
        });
    }
}

/// Returns whether an expression is allowed as a 0.0.5 parameter default.
///
/// Inputs:
/// - `expr`: syntax-output expression attached to a default parameter.
///
/// Output:
/// - `true` for literal constants and literal aggregate expressions.
/// - `false` for calls, variables, control flow, casts, and other expressions
///   whose evaluation order or purity is not yet specified for defaults.
///
/// Transformation:
/// - Recursively validates only closed literal shapes; no inference state is
///   mutated by this predicate.
fn is_compile_time_default_expr(expr: &SyntaxExprOutput) -> bool {
    crate::value_lifecycle::expression_is_const_safe(expr)
}

/// Returns whether a declaration is implemented by a compiler intrinsic.
///
/// Inputs:
/// - `declaration`: syntax-output declaration carrying parsed annotations.
///
/// Output:
/// - `true` when the declaration has `@compiler.intrinsic`.
/// - `false` for ordinary source declarations.
///
/// Transformation:
/// - Reads only the annotation path and ignores annotation payload text so the
///   checker can trust the explicit compiler-provided implementation marker
///   without coupling type checking to backend intrinsic registry parsing.
fn is_compiler_intrinsic_declaration(declaration: &SyntaxDeclarationOutput) -> bool {
    declaration_has_marker_annotation(declaration, &["compiler", "intrinsic"])
}

/// Returns whether a declaration carries one exact marker annotation path.
///
/// Inputs:
/// - `declaration`: syntax-output declaration carrying parsed annotations.
/// - `path`: compiler-known marker annotation path to test.
///
/// Output:
/// - `true` when the declaration has an annotation with exactly `path`.
///
/// Transformation:
/// - Centralizes annotation-path matching for compiler metadata consumed by
///   typechecking so individual checks do not duplicate vector/string logic.
fn declaration_has_marker_annotation(declaration: &SyntaxDeclarationOutput, path: &[&str]) -> bool {
    declaration.annotations.iter().any(|annotation| {
        annotation.path.len() == path.len()
            && annotation
                .path
                .iter()
                .map(String::as_str)
                .zip(path.iter().copied())
                .all(|(actual, expected)| actual == expected)
    })
}

/// Synthesizes callable patterns for receiver-method body checking.
///
/// Inputs:
/// - `receiver`: receiver parameter declared before the method name.
/// - `params`: ordinary method parameters.
/// - `clauses`: syntax-output method clauses produced by the parser.
///
/// Output:
/// - Owned clause list whose patterns bind the receiver followed by each method
///   parameter, preserving each original body, guard, and span.
///
/// Transformation:
/// - Converts the current single-expression receiver-method declaration shape
///   into the receiver-first function clause shape shared by typechecking and
///   CoreIR lowering. It does not alter syntax output.
pub(super) fn receiver_method_clauses_with_bindings(
    receiver: &SyntaxParamOutput,
    params: &[SyntaxParamOutput],
    clauses: &[SyntaxFunctionClauseOutput],
) -> Vec<SyntaxFunctionClauseOutput> {
    clauses
        .iter()
        .map(|clause| {
            let patterns = std::iter::once(receiver)
                .chain(params.iter())
                .map(|param| SyntaxPatternOutput {
                    kind: SyntaxPatternKind::Var,
                    arity: 1,
                    text: Some(param.name.clone()),
                    children: Vec::new(),
                    fields: Vec::new(),
                })
                .collect();
            SyntaxFunctionClauseOutput {
                patterns,
                guard: clause.guard.clone(),
                body: clause.body.clone(),
                has_guard: clause.has_guard,
                span: clause.span,
            }
        })
        .collect()
}

/// Selects the internal body-check return type for receiver methods.
///
/// Inputs:
/// - `receiver`: source receiver parameter, including the contextual `mut`
///   marker.
/// - `return_type`: source-visible method return type annotation.
///
/// Output:
/// - Receiver type text for command-style mutable receiver methods declared as
///   returning `Unit`; otherwise the original source return type.
///
/// Transformation:
/// - Models the first P0.2c executable ABI slice: command-style mutable receiver
///   methods expose `Unit` at the source level but must produce the updated
///   receiver value internally so backend lowering has a concrete value to
///   rebind.
fn mutable_receiver_internal_return_type<'a>(
    receiver: &'a SyntaxParamOutput,
    return_type: &'a SyntaxTypeOutput,
) -> &'a str {
    if receiver.is_mutable && is_unit_type_text(&return_type.text) {
        &receiver.annotation.text
    } else {
        &return_type.text
    }
}

/// Checks whether a type annotation names Terlan `Unit`.
///
/// Inputs:
/// - `text`: source type annotation text.
///
/// Output:
/// - `true` when `text` is the compact source spelling `Unit`.
///
/// Transformation:
/// - Normalizes whitespace using the same type-text comparison helper used by
///   trait/receiver validation; this intentionally does not treat arbitrary
///   aliases as `Unit`.
pub(super) fn is_unit_type_text(text: &str) -> bool {
    trait_type_text_equal(text, "Unit")
}

/// Checks syntax-output callable clauses against a declared function scheme.
///
/// Inputs:
/// - `callable_label`: diagnostic label such as `function add` or
///   `impl method to_string`.
/// - `callable_name`: bare function/method name used for trait-bound and
///   exhaustiveness diagnostics.
/// - `params`: declared callable parameters.
/// - `clauses`: parsed callable clauses with patterns, guards, and bodies.
/// - `scheme`: parsed parameter and return types for the callable.
/// - `fallback_span`: span used for diagnostics that are not tied to one
///   clause.
/// - `alias_names`: local/imported names that may appear in patterns.
/// - `aliases`: visible type aliases used for expected/inferred comparison.
/// - `expr_ctx`: expression inference context.
/// - `diagnostics`: output diagnostic buffer.
/// - `requires_pure_body`: whether the callable carries `@pure`.
/// - `effectful_local_calls`: same-module functions with direct structural
///   effects.
/// - `imported_effects`: selected and qualified imported functions lacking an
///   explicit purity proof.
///
/// Output:
/// - No direct return value.
///
/// Transformation:
/// - Instantiates the callable scheme per clause, checks pattern bindings
///   against parameter types, infers the clause body, and unifies it with the
///   declared return type. The same path is used for normal functions and
///   explicit trait impl methods so adapter bodies cannot bypass typechecking.
fn check_syntax_callable_clauses(
    callable_label: &str,
    callable_name: &str,
    params: &[SyntaxParamOutput],
    clauses: &[SyntaxFunctionClauseOutput],
    scheme: &FunctionScheme,
    fallback_span: Span,
    alias_names: &HashSet<String>,
    aliases: &HashMap<String, TypeAlias>,
    expr_ctx: &ExprInferContext,
    diagnostics: &mut Vec<Diagnostic>,
    requires_pure_body: bool,
    requires_trait_purity: bool,
    effectful_local_calls: &HashSet<(String, usize)>,
    imported_effects: &ImportedEffectFacts,
) {
    let mut clause_patterns: Vec<(Vec<SyntaxPatternOutput>, Span)> = Vec::new();

    if clauses.is_empty() {
        diagnostics.push(Diagnostic {
            span: fallback_span,
            message: format!("{} has no clauses", callable_label),
            severity: DiagSeverity::Error,
        });
        return;
    }

    for clause in clauses {
        let span = clause.span.into();
        if clause.patterns.len() != params.len() {
            diagnostics.push(Diagnostic {
                span,
                message: format!(
                    "{} has arity mismatch: expected {}, found {}",
                    callable_label,
                    params.len(),
                    clause.patterns.len()
                ),
                severity: DiagSeverity::Error,
            });
            continue;
        }

        let instantiated = instantiate_function_scheme(scheme);
        let mut subst = HashMap::new();
        let mut locals: HashMap<String, Type> = HashMap::new();
        for (pattern, param_type) in clause.patterns.iter().zip(instantiated.params.iter()) {
            if let Err(message) = check_syntax_pattern(
                pattern,
                &expand_type_aliases(param_type, aliases),
                aliases,
                Some(expr_ctx),
                &mut locals,
                &mut subst,
            ) {
                diagnostics.push(Diagnostic {
                    span,
                    message,
                    severity: DiagSeverity::Error,
                });
            }
        }

        let function_values = locals
            .iter()
            .filter(|(_, value_type)| {
                matches!(
                    expand_type_aliases(value_type, aliases),
                    Type::Function { .. }
                )
            })
            .map(|(name, _)| name.clone())
            .collect::<HashSet<_>>();
        let mut facts = effectful_call_facts(effectful_local_calls, imported_effects);
        facts.function_values = Some(&function_values);
        let mut local_expr_ctx = expr_ctx_with_current_bounds(expr_ctx, &instantiated.bounds);
        local_expr_ctx.effectful_calls = facts;
        let mut local_errors = Vec::new();
        if let Some(guard) = clause.guard.as_ref() {
            refine_by_syntax_guard(guard, &mut locals, aliases, &mut subst);
            check_clause_guard_purity(
                guard,
                "function guard",
                &locals,
                &local_expr_ctx,
                &subst,
                &mut local_errors,
            );
            let guard_type = infer_syntax_expr(
                guard,
                &locals,
                &local_expr_ctx,
                &mut subst,
                &mut local_errors,
            );
            if let Err(message) = unify(&Type::Bool, &guard_type, &mut subst) {
                diagnostics.push(Diagnostic {
                    span,
                    message: format!("function guard {message}"),
                    severity: DiagSeverity::Error,
                });
            }
        }
        let bounds_error = if let Err(message) =
            check_function_bounds(&instantiated, Some(callable_name), &local_expr_ctx, &subst)
        {
            diagnostics.push(Diagnostic {
                span,
                message,
                severity: DiagSeverity::Error,
            });
            true
        } else {
            false
        };
        if requires_pure_body {
            let contract = if requires_trait_purity {
                format!("{callable_label} required pure by its trait contract")
            } else {
                format!("{callable_label} annotated @pure")
            };
            check_pure_expression_effects_with_call_facts(
                &clause.body,
                &contract,
                local_expr_ctx.templates,
                &facts,
                &mut local_errors,
            );
        }

        let expected_return = expand_type_aliases(&instantiated.ret, aliases);
        let inferred = if bounds_error {
            Type::Dynamic
        } else {
            infer_syntax_expr_with_expected(
                &clause.body,
                &expected_return,
                &locals,
                &local_expr_ctx,
                &mut subst,
                &mut local_errors,
            )
            .unwrap_or_else(|| {
                infer_syntax_expr(
                    &clause.body,
                    &locals,
                    &local_expr_ctx,
                    &mut subst,
                    &mut local_errors,
                )
            })
        };

        for error in local_errors {
            diagnostics.push(expression_error_to_diagnostic(error, span));
        }

        let expected_expanded = expand_type_aliases(&instantiated.ret, aliases);
        let inferred_expanded = expand_type_aliases(&inferred, aliases);

        if let Err(message) = unify(&expected_expanded, &inferred_expanded, &mut subst) {
            let revealed_inferred = reveal_opaque_aliases(&inferred_expanded, aliases);
            if unify(&expected_expanded, &revealed_inferred, &mut subst).is_ok() {
                clause_patterns.push((clause.patterns.clone(), span));
                continue;
            }
            if expected_syntax_opaque_constructor_return_matches(
                &clause.body,
                &expected_expanded,
                &locals,
                expr_ctx,
                &mut subst,
            ) {
                clause_patterns.push((clause.patterns.clone(), span));
                continue;
            }
            diagnostics.push(Diagnostic {
                span,
                message,
                severity: DiagSeverity::Error,
            });
        }

        clause_patterns.push((clause.patterns.clone(), span));
    }

    check_syntax_function_clause_exhaustiveness(
        callable_name,
        params.first().map(|param| param.annotation.text.as_str()),
        params.len(),
        alias_names,
        &clause_patterns,
        aliases,
        diagnostics,
    );
}
