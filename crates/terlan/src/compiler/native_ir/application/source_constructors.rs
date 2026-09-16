//! Source constructors reuse ordinary typed callable and suspension lowering.

use super::super::NativeIrResult;
use super::*;
use crate::terlan_typeck::{
    visit_core_expr_mut, CoreConstructorDecl, CoreLetBinding, CoreParam, CoreProofCoverage,
};

type Providers = HashMap<String, (usize, Vec<CoreConstructorDecl>)>;

/// Routes source constructor invocations through the ordinary typed call pipeline.
pub(super) fn lower(cores: &mut [CoreModule]) -> NativeIrResult<()> {
    let mut providers: Providers = HashMap::new();
    let mut bodies = HashMap::new();
    for (index, core) in cores.iter().enumerate() {
        let implementations = core
            .constructors
            .iter()
            .filter_map(|constructor| constructor.implementation.as_ref())
            .map(|implementation| implementation.function.as_str())
            .collect::<HashSet<_>>();
        for function in core
            .functions
            .iter()
            .filter(|function| implementations.contains(function.name.as_str()))
        {
            bodies.insert((index, function.name.clone()), function.clone());
        }
        for constructor in &core.constructors {
            if constructor.implementation.is_some() {
                providers
                    .entry(format!("{}.{}", core.module, constructor.name))
                    .or_insert_with(|| (index, Vec::new()))
                    .1
                    .push(constructor.clone());
            }
        }
    }
    if providers.is_empty() {
        return Ok(());
    }
    let mut adapters = HashSet::new();
    let mut pending = Vec::new();
    for core in cores.iter_mut() {
        for function in &mut core.functions {
            let initializer = core
                .constructors
                .iter()
                .find(|constructor| {
                    constructor
                        .implementation
                        .as_ref()
                        .is_some_and(|implementation| implementation.function == function.name)
                        && core.types.iter().any(|ty| {
                            ty.name == constructor.name
                                && matches!(ty.core_body, Some(CoreType::Struct { .. }))
                        })
                })
                .map(|constructor| constructor.name.as_str());
            let mut result = Ok(());
            for clause in &mut function.clauses {
                let expressions = clause
                    .guard
                    .iter_mut()
                    .filter_map(|guard| guard.core_expr.as_mut())
                    .chain(clause.body.core_expr.as_mut());
                for expression in expressions {
                    visit_core_expr_mut(expression, &mut |expr| {
                        if result.is_err() {
                            return;
                        }
                        if matches!(expr, CoreExpr::ConstructorCall { constructor, constructor_identity, .. }
                            if initializer == Some(constructor.as_str()) && constructor_identity.as_deref().is_none_or(|identity| identity == constructor || identity == format!("{}.{}", core.module, constructor)))
                        {
                            return;
                        }
                        result = rewrite(
                            expr,
                            &core.module,
                            &providers,
                            &bodies,
                            &mut adapters,
                            &mut pending,
                        );
                    });
                }
            }
            result?;
        }
    }
    for (provider, function) in pending {
        cores[provider].functions.push(function);
    }
    Ok(())
}

fn rewrite(
    expr: &mut CoreExpr,
    module: &str,
    providers: &Providers,
    bodies: &HashMap<(usize, String), CoreFunction>,
    adapters: &mut HashSet<(String, usize)>,
    pending: &mut Vec<(usize, CoreFunction)>,
) -> NativeIrResult<()> {
    let CoreExpr::ConstructorCall {
        constructor,
        constructor_identity,
        type_args,
        args,
    } = expr
    else {
        return Ok(());
    };
    let identity = constructor_identity.as_deref().unwrap_or(constructor);
    let identity = if identity.contains('.') {
        identity.to_string()
    } else {
        format!("{module}.{identity}")
    };
    let Some((provider, declarations)) = providers.get(&identity) else {
        return Ok(());
    };
    let arity = args.len();
    let name = format!("$constructor_call_{}_{}", declarations[0].name, arity);
    let owner = identity.rsplit_once('.').expect("qualified constructor").0;
    if owner != module && !declarations.iter().any(|declaration| declaration.public) {
        return Err(
            format!("error[native_ir.constructor_visibility]: `{identity}` is private").into(),
        );
    }
    if adapters.insert((identity.clone(), arity)) {
        let mut matched = false;
        for declaration in declarations.iter().filter(|declaration| {
            arity >= declaration.min_arity
                && (declaration.vararg.is_some() || arity <= declaration.params.len())
        }) {
            if owner != module && !declaration.public {
                continue;
            }
            let implementation = declaration
                .implementation
                .as_ref()
                .expect("source provider");
            let body = bodies.get(&(*provider, implementation.function.clone())).ok_or_else(|| format!("error[native_ir.constructor_body]: `{identity}` is missing its checked implementation `{}`", implementation.function))?;
            pending.push((*provider, adapter(&name, arity, declaration, body)?));
            matched = true;
        }
        if !matched {
            return Err(format!("error[native_ir.constructor_arity]: `{identity}` has no visible implementation for {arity} arguments").into());
        }
    }
    *expr = CoreExpr::Call {
        function: if owner == module {
            name
        } else {
            format!("{owner}.{name}")
        },
        type_args: std::mem::take(type_args),
        args: std::mem::take(args),
    };
    Ok(())
}

fn adapter(
    name: &str,
    arity: usize,
    declaration: &CoreConstructorDecl,
    body: &CoreFunction,
) -> NativeIrResult<CoreFunction> {
    let implementation = declaration
        .implementation
        .as_ref()
        .expect("source implementation");
    let fixed = arity.min(declaration.params.len());
    let mut parameters = declaration.params[..fixed].to_vec();
    let mut bindings = Vec::new();
    let mut arguments = Vec::new();
    for (index, parameter) in declaration.params.iter().enumerate() {
        if index >= fixed {
            let default = implementation
                .defaults
                .get(index)
                .and_then(Option::as_ref)
                .ok_or_else(|| {
                    format!(
                        "error[native_ir.constructor_default]: `{}` lacks default argument {index}",
                        declaration.name
                    )
                })?;
            bindings.push(CoreLetBinding {
                pattern: CorePattern::Var(parameter.name.clone()),
                value: CoreExpr::Call {
                    function: default.clone(),
                    type_args: body
                        .generic_params
                        .iter()
                        .cloned()
                        .map(CoreType::Named)
                        .collect(),
                    args: arguments.clone(),
                },
            });
        }
        arguments.push(CoreExpr::Var(parameter.name.clone()));
    }
    if let Some(vararg) = &declaration.vararg {
        let mut values = Vec::new();
        for index in declaration.params.len()..arity {
            let parameter = CoreParam {
                name: format!("$constructor_arg_{index}"),
                ..vararg.clone()
            };
            values.push(CoreExpr::Var(parameter.name.clone()));
            parameters.push(parameter);
        }
        arguments.push(CoreExpr::List(values));
    }
    let call = CoreExpr::Call {
        function: implementation.function.clone(),
        type_args: body
            .generic_params
            .iter()
            .cloned()
            .map(CoreType::Named)
            .collect(),
        args: arguments,
    };
    let expression = if bindings.is_empty() {
        call
    } else {
        CoreExpr::Let {
            bindings,
            body: Box::new(call),
        }
    };
    let mut function = body.clone();
    function.name = name.to_string();
    function.public = declaration.public;
    function.arity = arity;
    function.params = parameters;
    let Some(clause) = function.clauses.first_mut() else {
        return Err(format!(
            "error[native_ir.constructor_body]: `{}` has no executable clause",
            implementation.function
        )
        .into());
    };
    clause.patterns = function
        .params
        .iter()
        .map(|parameter| parameter.name.clone())
        .collect();
    clause.core_patterns = function
        .params
        .iter()
        .map(|parameter| Some(CorePattern::Var(parameter.name.clone())))
        .collect();
    clause.pattern_proof_coverage = vec![CoreProofCoverage::RuntimeBoundary; arity];
    clause.pattern_checked_preservation_evidence = vec![None; arity];
    clause.body.checked_preservation_evidence = None;
    clause.body.proof_coverage = CoreProofCoverage::RuntimeBoundary;
    clause.body.children.clear();
    clause.body.text = None;
    clause.body.remote = None;
    clause.body.operator = None;
    clause.body.kind = "native-constructor-adapter".to_string();
    clause.body.arity = arity;
    clause.body.core_expr = Some(expression);
    Ok(function)
}
