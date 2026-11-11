//! Type-directed closure of mutable receiver calls.

use std::collections::HashMap;

use crate::terlan_typeck::{core_type_from_text, CoreExpr, CoreModule, CorePattern, CoreType};

#[derive(Clone)]
struct NominalType {
    params: Vec<String>,
    body: CoreType,
}

type NominalTypes = HashMap<String, NominalType>;

#[derive(Clone, PartialEq, Eq)]
pub(in crate::compiler::native_ir) struct ReceiverTarget {
    module: String,
    function: String,
    receiver: CoreType,
    result: Option<CoreType>,
    public: bool,
    generic_params: Vec<String>,
    trait_fallback: bool,
    scope_preferred: bool,
    mutable: bool,
    command: bool,
}

/// Resolves mutable receiver syntax to one exact application callable.
///
/// Typechecking has already selected a receiver-method signature, but legacy
/// CoreIR stores only the source method name. Application linking can expose
/// several owners of names such as `put/3`; this pass reconstructs the exact
/// owner from the canonical checked receiver type before native admission.
pub(crate) fn resolve_typed_mutable_receiver_calls(
    cores: &mut [CoreModule],
) -> super::super::NativeIrResult<()> {
    let targets = receiver_targets(cores);
    let types = nominal_types(cores);
    let qualified_results = function_results(cores);
    for core in cores {
        let type_modules = core
            .imports
            .iter()
            .filter(|import| import.kind == crate::terlan_typeck::CoreImportKind::TypeModule)
            .map(|import| import.module.as_str())
            .collect::<std::collections::HashSet<_>>();
        let mut scoped_targets = targets.clone();
        for candidates in scoped_targets.values_mut() {
            for target in candidates {
                target.scope_preferred =
                    target.module == core.module || type_modules.contains(target.module.as_str());
            }
        }
        let mut results = qualified_results.clone();
        for function in &core.functions {
            if let Some(result) = function
                .core_return_type
                .clone()
                .or_else(|| core_type_from_text(&function.return_type))
            {
                results.insert((function.name.clone(), function.arity), result);
            }
        }
        let module = core.module.clone();
        for function in &mut core.functions {
            let variables = function
                .params
                .iter()
                .filter_map(|parameter| {
                    parameter
                        .core_ty
                        .clone()
                        .map(|ty| (parameter.name.clone(), ty))
                })
                .collect::<HashMap<_, _>>();
            for clause in &mut function.clauses {
                if let Some(guard) = clause
                    .guard
                    .as_mut()
                    .and_then(|summary| summary.core_expr.as_mut())
                {
                    resolve_expr(
                        guard,
                        &module,
                        &variables,
                        &results,
                        &scoped_targets,
                        &types,
                    )?;
                }
                if let Some(body) = clause.body.core_expr.as_mut() {
                    resolve_expr(body, &module, &variables, &results, &scoped_targets, &types)?;
                }
            }
        }
    }
    Ok(())
}

fn nominal_types(cores: &[CoreModule]) -> NominalTypes {
    let mut types = HashMap::new();
    for core in cores {
        for declaration in &core.types {
            let Some(body) = &declaration.core_body else {
                continue;
            };
            let ty = NominalType {
                params: declaration.params.clone(),
                body: body.clone(),
            };
            for name in [
                declaration.name.clone(),
                format!("{}.{}", core.module, declaration.name),
                crate::terlan_hir::public_type_path(&core.module, &declaration.name),
            ] {
                types.insert(name, ty.clone());
            }
        }
    }
    types
}

/// Checked receiver and trait methods grouped by source method name and arity.
pub(in crate::compiler::native_ir) type ReceiverTargets =
    HashMap<(String, usize), Vec<ReceiverTarget>>;

/// Collects declared methods without interpreting standard-library module names.
pub(in crate::compiler::native_ir) fn receiver_targets(cores: &[CoreModule]) -> ReceiverTargets {
    let mut targets = HashMap::<(String, usize), Vec<ReceiverTarget>>::new();
    let nominal_parameters = super::super::generic_specialization::nominal_parameter_catalog(cores);
    for core in cores {
        for function in &core.functions {
            let method = if function.receiver_method {
                &function.name
            } else if let Some(identity) = &function.trait_method {
                &identity.method
            } else {
                continue;
            };
            let Some(receiver) = function.params.first().and_then(|parameter| {
                parameter
                    .core_ty
                    .clone()
                    .or_else(|| core_type_from_text(&parameter.ty))
            }) else {
                continue;
            };
            let target = ReceiverTarget {
                module: core.module.clone(),
                function: function.name.clone(),
                receiver,
                result: function
                    .core_return_type
                    .clone()
                    .or_else(|| core_type_from_text(&function.return_type)),
                public: function.public,
                trait_fallback: !function.receiver_method,
                scope_preferred: false,
                mutable: function.receiver_mutable,
                command: function.receiver_command,
                generic_params:
                    super::super::generic_specialization::generic_parameters_with_nominals(
                        function,
                        &nominal_parameters,
                    ),
            };
            let candidates = targets.entry((method.clone(), function.arity)).or_default();
            if !candidates.contains(&target) {
                candidates.push(target);
            }
        }
    }
    targets
}

fn binding_receiver(
    binding: &crate::terlan_typeck::CoreLetBinding,
    locals: &HashMap<String, CoreType>,
    module: &str,
    targets: &ReceiverTargets,
) -> Option<(String, bool)> {
    // Sequence temporaries cannot collide with source identifiers.
    let (owner, function, receiver, arity) = match &binding.value {
        CoreExpr::RemoteCall {
            module: owner,
            function,
            args,
            ..
        } => (owner.as_str(), function.as_str(), args.first()?, args.len()),
        CoreExpr::Call { function, args, .. } => {
            let (owner, function) = function.rsplit_once('.').unwrap_or((module, function));
            (owner, function, args.first()?, args.len())
        }
        CoreExpr::MutableReceiverCall {
            receiver,
            method,
            args,
            ..
        } => (
            "__receiver__",
            method.as_str(),
            receiver.as_ref(),
            args.len() + 1,
        ),
        _ => return None,
    };
    let CoreExpr::Var(name) = receiver else {
        return None;
    };
    let receiver_type = locals.get(name)?;
    if let Some(target) = receiver_target(function, arity, receiver_type, module, targets) {
        if owner != "__receiver__" && owner != target.module {
            return None;
        }
        let statement =
            matches!(&binding.pattern, CorePattern::Var(name) if name.starts_with("$seq"));
        return (target.mutable && (statement || target.command))
            .then(|| (name.clone(), target.command));
    }
    if owner != "__receiver__" {
        return None;
    }
    super::super::collection_intrinsic_specialization::receiver_intrinsics::typed_receiver_intrinsic(
        receiver_type,
        function,
        arity,
    )
    .filter(|(intrinsic, _)| intrinsic_receiver_command(intrinsic))
    .map(|_| (name.clone(), true))
}

fn intrinsic_receiver_command(intrinsic: &crate::terlan_typeck::CorePrimitiveIntrinsic) -> bool {
    use crate::terlan_typeck::CorePrimitiveIntrinsic as Primitive;

    matches!(
        intrinsic,
        Primitive::ListPush
            | Primitive::ListClear
            | Primitive::MapPut
            | Primitive::MapRemove
            | Primitive::MapClear
            | Primitive::SetAdd
            | Primitive::SetRemove
            | Primitive::SetClear
    )
}

fn function_results(cores: &[CoreModule]) -> HashMap<(String, usize), CoreType> {
    cores
        .iter()
        .flat_map(|core| {
            core.functions.iter().filter_map(move |function| {
                function
                    .core_return_type
                    .clone()
                    .or_else(|| core_type_from_text(&function.return_type))
                    .map(|result| {
                        (
                            (format!("{}.{}", core.module, function.name), function.arity),
                            result,
                        )
                    })
            })
        })
        .collect()
}

fn resolve_expr(
    expr: &mut CoreExpr,
    module: &str,
    variables: &HashMap<String, CoreType>,
    functions: &HashMap<(String, usize), CoreType>,
    targets: &HashMap<(String, usize), Vec<ReceiverTarget>>,
    types: &NominalTypes,
) -> super::super::NativeIrResult<()> {
    match expr {
        CoreExpr::MutableReceiverCall {
            receiver,
            method,
            args,
            ..
        } => {
            resolve_expr(receiver, module, variables, functions, targets, types)?;
            for argument in args.iter_mut() {
                resolve_expr(argument, module, variables, functions, targets, types)?;
            }
            let Some(receiver_type) =
                infer_core_type(receiver, variables, functions, targets, types, module)
            else {
                return Ok(());
            };
            if let Some(target) =
                receiver_target(method, args.len() + 1, &receiver_type, module, targets)
            {
                let receiver =
                    std::mem::replace(receiver.as_mut(), CoreExpr::Atom("Unit".to_string()));
                let mut call_args = vec![receiver];
                call_args.append(args);
                *expr = CoreExpr::Call {
                    type_args: Vec::new(),
                    function: if target.module == module {
                        target.function.clone()
                    } else {
                        format!("{}.{}", target.module, target.function)
                    },
                    args: call_args,
                };
                return Ok(());
            }
            if let Some((intrinsic, return_type)) = super::super::collection_intrinsic_specialization::receiver_intrinsics::typed_receiver_intrinsic(
                &receiver_type,
                method,
                args.len() + 1,
            ) {
                let receiver =
                    std::mem::replace(receiver.as_mut(), CoreExpr::Atom("Unit".to_string()));
                let mut call_args = vec![receiver];
                call_args.append(args);
                *expr = CoreExpr::Intrinsic(crate::terlan_typeck::CoreIntrinsicCall {
                    id: crate::terlan_typeck::CoreIntrinsicId::Primitive(intrinsic),
                    args: call_args,
                    return_type,
                    effects: crate::terlan_typeck::core_intrinsic_lowering::core_pure_effect_set(),
                    span: crate::terlan_syntax::span::Span::new(0, 0),
                });
                return Ok(());
            }
        }
        CoreExpr::RemoteCall {
            type_args,
            module: receiver_module,
            function,
            args,
        } if receiver_module == "__receiver__" => {
            for argument in args.iter_mut() {
                resolve_expr(argument, module, variables, functions, targets, types)?;
            }
            let receiver_type = args.first().and_then(|receiver| {
                infer_core_type(receiver, variables, functions, targets, types, module)
            });
            let target = receiver_type.as_ref().and_then(|receiver_type| {
                receiver_target(function, args.len(), receiver_type, module, targets)
            });
            if let Some(target) = target {
                *expr = CoreExpr::Call {
                    type_args: std::mem::take(type_args),
                    function: callable_identity(target, module),
                    args: std::mem::take(args),
                };
                return Ok(());
            }
            if let Some((intrinsic, return_type)) = receiver_type.as_ref().and_then(|receiver| {
                super::super::collection_intrinsic_specialization::receiver_intrinsics::typed_receiver_intrinsic(
                    receiver,
                    function,
                    args.len(),
                )
            }) {
                *expr = CoreExpr::Intrinsic(crate::terlan_typeck::CoreIntrinsicCall {
                    id: crate::terlan_typeck::CoreIntrinsicId::Primitive(intrinsic),
                    args: std::mem::take(args),
                    return_type,
                    effects: crate::terlan_typeck::core_intrinsic_lowering::core_pure_effect_set(),
                    span: crate::terlan_syntax::span::Span::new(0, 0),
                });
                return Ok(());
            }
        }
        CoreExpr::Let { bindings, body } => {
            let mut locals = variables.clone();
            let mut rewritten = Vec::with_capacity(bindings.len());
            for mut binding in std::mem::take(bindings) {
                let receiver = binding_receiver(&binding, &locals, module, targets);
                resolve_expr(
                    &mut binding.value,
                    module,
                    &locals,
                    functions,
                    targets,
                    types,
                )?;
                if let Some((name, command)) = receiver {
                    let receiver_continuation = matches!(
                        &binding.pattern,
                        CorePattern::Var(binding_name) if binding_name == &name
                    );
                    // The internal receiver result is not another public command
                    // invocation when this pass runs again after source linking.
                    let value = CoreExpr::Cast {
                        expr: Box::new(std::mem::replace(
                            &mut binding.value,
                            CoreExpr::Atom("Unit".into()),
                        )),
                        target_type: locals.get(&name).expect("checked receiver type").clone(),
                    };
                    rewritten.push(crate::terlan_typeck::CoreLetBinding {
                        pattern: CorePattern::Var(name),
                        value,
                    });
                    if !command || receiver_continuation {
                        continue;
                    }
                }
                if let CorePattern::Var(name) = &binding.pattern {
                    if let Some(ty) =
                        infer_core_type(&binding.value, &locals, functions, targets, types, module)
                    {
                        locals.insert(name.clone(), ty);
                    }
                }
                rewritten.push(binding);
            }
            *bindings = rewritten;
            resolve_expr(body, module, &locals, functions, targets, types)?;
        }
        CoreExpr::Call { function, args, .. } => {
            for argument in args.iter_mut() {
                resolve_expr(argument, module, variables, functions, targets, types)?;
            }
            if !function.contains('.') && !functions.contains_key(&(function.clone(), args.len())) {
                let target = args
                    .first()
                    .and_then(|receiver| {
                        infer_core_type(receiver, variables, functions, targets, types, module)
                    })
                    .and_then(|receiver_type| {
                        receiver_target(function, args.len(), &receiver_type, module, targets)
                    });
                if let Some(target) = target {
                    *function = callable_identity(target, module);
                }
            }
        }
        CoreExpr::RemoteCall { args, .. }
        | CoreExpr::ConstructorCall { args, .. }
        | CoreExpr::Intrinsic(crate::terlan_typeck::CoreIntrinsicCall { args, .. }) => {
            for argument in args.iter_mut() {
                resolve_expr(argument, module, variables, functions, targets, types)?;
            }
        }
        CoreExpr::FunctionCall { callee, args } => {
            resolve_expr(callee, module, variables, functions, targets, types)?;
            for argument in args {
                resolve_expr(argument, module, variables, functions, targets, types)?;
            }
        }
        CoreExpr::Tuple(items) | CoreExpr::List(items) | CoreExpr::Array(items) => {
            for item in items {
                resolve_expr(item, module, variables, functions, targets, types)?;
            }
        }
        CoreExpr::ListCons { head, tail }
        | CoreExpr::BinaryOp {
            left: head,
            right: tail,
            ..
        } => {
            resolve_expr(head, module, variables, functions, targets, types)?;
            resolve_expr(tail, module, variables, functions, targets, types)?;
        }
        CoreExpr::Map(fields) => {
            for field in fields {
                resolve_expr(
                    &mut field.value,
                    module,
                    variables,
                    functions,
                    targets,
                    types,
                )?;
            }
        }
        CoreExpr::RecordConstruct { fields, .. } | CoreExpr::TemplateInstantiate { fields, .. } => {
            for field in fields {
                resolve_expr(
                    &mut field.value,
                    module,
                    variables,
                    functions,
                    targets,
                    types,
                )?;
            }
        }
        CoreExpr::RecordUpdate { base, fields, .. } => {
            resolve_expr(base, module, variables, functions, targets, types)?;
            for field in fields {
                resolve_expr(
                    &mut field.value,
                    module,
                    variables,
                    functions,
                    targets,
                    types,
                )?;
            }
        }
        CoreExpr::FieldAccess { base, .. }
        | CoreExpr::RecordAccess { base, .. }
        | CoreExpr::UnaryOp { operand: base, .. }
        | CoreExpr::Cast { expr: base, .. } => {
            resolve_expr(base, module, variables, functions, targets, types)?;
        }
        CoreExpr::Lam {
            params,
            parameter_types,
            body,
        } => {
            let locals = super::super::generic_specialization::lambda_type_scope(
                params,
                parameter_types,
                variables,
            );
            resolve_expr(body, module, &locals, functions, targets, types)?;
        }
        CoreExpr::If { clauses } => {
            for clause in clauses {
                resolve_expr(
                    &mut clause.condition,
                    module,
                    variables,
                    functions,
                    targets,
                    types,
                )?;
                resolve_expr(
                    &mut clause.body,
                    module,
                    variables,
                    functions,
                    targets,
                    types,
                )?;
            }
        }
        CoreExpr::Case { scrutinee, clauses } => {
            resolve_expr(scrutinee, module, variables, functions, targets, types)?;
            let scrutinee_type =
                infer_core_type(scrutinee, variables, functions, targets, types, module);
            for clause in clauses {
                let mut locals = variables.clone();
                if let Some(scrutinee_type) = &scrutinee_type {
                    super::super::generic_specialization::bind_pattern_types(
                        &clause.pattern,
                        scrutinee_type,
                        &mut locals,
                    );
                }
                if let Some(guard) = &mut clause.guard {
                    resolve_expr(guard, module, &locals, functions, targets, types)?;
                }
                resolve_expr(&mut clause.body, module, &locals, functions, targets, types)?;
            }
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            resolve_expr(body, module, variables, functions, targets, types)?;
            for clause in of_clauses.iter_mut().chain(catch_clauses) {
                if let Some(guard) = &mut clause.guard {
                    resolve_expr(guard, module, variables, functions, targets, types)?;
                }
                resolve_expr(
                    &mut clause.body,
                    module,
                    variables,
                    functions,
                    targets,
                    types,
                )?;
            }
            if let Some(after) = after_clause {
                resolve_expr(
                    &mut after.trigger,
                    module,
                    variables,
                    functions,
                    targets,
                    types,
                )?;
                resolve_expr(
                    &mut after.body,
                    module,
                    variables,
                    functions,
                    targets,
                    types,
                )?;
            }
        }
        CoreExpr::ConstructorChain { args, record, .. } => {
            for argument in args {
                resolve_expr(argument, module, variables, functions, targets, types)?;
            }
            resolve_expr(record, module, variables, functions, targets, types)?;
        }
        CoreExpr::ListComprehension {
            expr,
            generators,
            guards,
            ..
        } => {
            resolve_expr(expr, module, variables, functions, targets, types)?;
            for generator in generators {
                resolve_expr(
                    &mut generator.source,
                    module,
                    variables,
                    functions,
                    targets,
                    types,
                )?;
            }
            for guard in guards {
                resolve_expr(guard, module, variables, functions, targets, types)?;
            }
        }
        CoreExpr::SqlQuery { parameters, .. } => {
            for parameter in parameters {
                resolve_expr(parameter, module, variables, functions, targets, types)?;
            }
        }
        CoreExpr::Index { base, index } => {
            resolve_expr(base, module, variables, functions, targets, types)?;
            resolve_expr(index, module, variables, functions, targets, types)?;
        }
        CoreExpr::Atom(_)
        | CoreExpr::Int(_)
        | CoreExpr::Float(_)
        | CoreExpr::Binary(_)
        | CoreExpr::Var(_)
        | CoreExpr::RemoteFunRef { .. } => {}
    }
    Ok(())
}

/// Uses the same visibility, generic matching and ambiguity rules during
/// contextual callback inference as during application receiver resolution.
pub(in crate::compiler::native_ir) fn receiver_callable(
    targets: &ReceiverTargets,
    method: &str,
    arity: usize,
    receiver: &CoreType,
    caller: &str,
) -> Option<String> {
    let target = receiver_target(method, arity, receiver, caller, targets)?;
    Some(format!("{}.{}", target.module, target.function))
}

fn receiver_target<'a>(
    method: &str,
    arity: usize,
    receiver_type: &CoreType,
    caller_module: &str,
    targets: &'a HashMap<(String, usize), Vec<ReceiverTarget>>,
) -> Option<&'a ReceiverTarget> {
    let identity = (method.to_string(), arity);
    let matches = targets
        .get(&identity)
        .into_iter()
        .flatten()
        .filter(|target| {
            receiver_matches(target, receiver_type)
                && (target.public || target.module == caller_module)
        });
    if std::env::var_os("TERLAN_NATIVE_AOT_TRACE").is_some() {
        eprintln!(
            "[native-aot receiver] module={caller_module} method={method}/{arity} receiver={} candidates={:?}",
            receiver_type.contract_text(),
            targets
                .get(&identity)
                .into_iter()
                .flatten()
                .map(|target| (
                    target.module.as_str(),
                    target.function.as_str(),
                    target.receiver.contract_text(),
                    target
                        .result
                        .as_ref()
                        .map(crate::terlan_typeck::CoreType::contract_text),
                    target.generic_params.as_slice(),
                    target.trait_fallback,
                    target.public,
                ))
                .collect::<Vec<_>>(),
        );
    }
    // Declared receiver methods take precedence over trait fallback, matching
    // frontend dispatch. Ambiguous candidates within either tier stay unresolved.
    for trait_fallback in [false, true] {
        let tier = matches
            .clone()
            .filter(|target| target.trait_fallback == trait_fallback)
            .collect::<Vec<_>>();
        let preferred = tier
            .iter()
            .copied()
            .filter(|target| target.scope_preferred)
            .collect::<Vec<_>>();
        let selected = if trait_fallback && !preferred.is_empty() {
            preferred
        } else {
            tier
        };
        if !selected.is_empty() {
            return match selected.as_slice() {
                [target] => Some(*target),
                _ => None,
            };
        }
    }
    None
}

fn callable_identity(target: &ReceiverTarget, caller_module: &str) -> String {
    if target.module == caller_module {
        target.function.clone()
    } else {
        format!("{}.{}", target.module, target.function)
    }
}

/// Instantiates only declared generic parameters, then checks the full receiver identity.
fn receiver_matches(target: &ReceiverTarget, actual: &CoreType) -> bool {
    use super::super::generic_specialization::{substitute, unify};

    if receiver_types_match(&target.receiver, actual) {
        return true;
    }
    let mut substitution = HashMap::new();
    unify(
        &target.receiver,
        actual,
        &target.generic_params,
        &mut substitution,
    )
    .is_ok()
        && receiver_types_match(
            &substitute(&target.receiver, &target.generic_params, &substitution),
            actual,
        )
}

/// Compares receiver types after nominal qualification and opaque-type
/// expansion have exposed equivalent short and structural names.
pub(in crate::compiler::native_ir::application) fn receiver_types_match(
    expected: &CoreType,
    actual: &CoreType,
) -> bool {
    if expected == actual {
        return true;
    }
    if let (
        CoreType::Apply {
            constructor: expected_constructor,
            args: expected_args,
        },
        CoreType::Apply {
            constructor: actual_constructor,
            args: actual_args,
        },
    ) = (expected, actual)
    {
        return expected_args.len() == actual_args.len()
            && receiver_nominal_names_match(expected_constructor, actual_constructor)
            && expected_args
                .iter()
                .zip(actual_args)
                .all(|(expected, actual)| receiver_types_match(expected, actual));
    }
    let (Some(expected), Some(actual)) = (nominal_name(expected), nominal_name(actual)) else {
        return false;
    };
    receiver_nominal_names_match(expected, actual)
}

fn receiver_nominal_names_match(expected: &str, actual: &str) -> bool {
    let expected = canonical_receiver_name(expected);
    let actual = canonical_receiver_name(actual);
    if expected == actual {
        return true;
    }
    // `Map` and `Set` are the canonical ABI spellings for Terlan's built-in
    // collections. They are identities, not unresolved short names. Treating
    // them as leaf-name aliases makes an imported type such as
    // `std.js.Map.Map[K, V]` collide with `std.collections.Map[K, V]` during
    // receiver dispatch.
    if matches!(expected, "Map" | "Set") || matches!(actual, "Map" | "Set") {
        return false;
    }
    (!expected.contains('.') || !actual.contains('.'))
        && expected.rsplit('.').next() == actual.rsplit('.').next()
}

fn canonical_receiver_name(name: &str) -> &str {
    let Some((owner, leaf)) = name.rsplit_once('.') else {
        return name;
    };
    if owner.rsplit('.').next() == Some(leaf) {
        owner
    } else {
        name
    }
}

/// Returns the declared name carried by a nominal or expanded structural type.
fn nominal_name(ty: &CoreType) -> Option<&str> {
    match ty {
        CoreType::Named(name) | CoreType::Struct { name, .. } => Some(name.as_str()),
        _ => None,
    }
}

fn infer_core_type(
    expr: &CoreExpr,
    variables: &HashMap<String, CoreType>,
    functions: &HashMap<(String, usize), CoreType>,
    targets: &ReceiverTargets,
    types: &NominalTypes,
    module: &str,
) -> Option<CoreType> {
    match expr {
        CoreExpr::Var(name) if matches!(name.as_str(), "true" | "false") => Some(CoreType::Bool),
        CoreExpr::Var(name) => variables.get(name).cloned(),
        CoreExpr::Atom(value) if matches!(value.as_str(), "true" | "false") => Some(CoreType::Bool),
        CoreExpr::Atom(_) => Some(CoreType::Atom),
        CoreExpr::Int(_) => Some(CoreType::Int),
        CoreExpr::Float(_) => Some(CoreType::Float),
        CoreExpr::Binary(_) => Some(CoreType::String),
        CoreExpr::Intrinsic(call) => Some(call.return_type.clone()),
        CoreExpr::RecordConstruct { name, .. } => Some(CoreType::Named(name.clone())),
        CoreExpr::RecordUpdate { base, .. } => {
            infer_core_type(base, variables, functions, targets, types, module)
        }
        CoreExpr::FieldAccess { base, field } | CoreExpr::RecordAccess { base, field, .. } => {
            let base = infer_core_type(base, variables, functions, targets, types, module)?;
            nominal_field_type(&base, field, types)
        }
        CoreExpr::Call { function, args, .. } => {
            specialized_call_result(function, args, variables, functions, targets, types, module)
                .or_else(|| functions.get(&(function.clone(), args.len())).cloned())
        }
        CoreExpr::RemoteCall {
            module: owner,
            function,
            args,
            ..
        } => {
            let qualified = format!("{owner}.{function}");
            specialized_call_result(
                &qualified, args, variables, functions, targets, types, module,
            )
            .or_else(|| functions.get(&(qualified, args.len())).cloned())
        }
        CoreExpr::Cast { target_type, .. } => Some(target_type.clone()),
        CoreExpr::List(items) => items
            .first()
            .and_then(|item| infer_core_type(item, variables, functions, targets, types, module))
            .map(|item| CoreType::List(Box::new(item))),
        CoreExpr::ListCons { tail, .. } => {
            infer_core_type(tail, variables, functions, targets, types, module)
        }
        CoreExpr::Tuple(items) => Some(CoreType::Tuple(
            items
                .iter()
                .map(|item| {
                    infer_core_type(item, variables, functions, targets, types, module)
                        .map(crate::terlan_typeck::CoreTupleTypeElem::Type)
                })
                .collect::<Option<Vec<_>>>()?,
        )),
        CoreExpr::Let { bindings, body } => {
            let mut locals = variables.clone();
            for binding in bindings {
                let CorePattern::Var(name) = &binding.pattern else {
                    return None;
                };
                let ty =
                    infer_core_type(&binding.value, &locals, functions, targets, types, module)?;
                locals.insert(name.clone(), ty);
            }
            infer_core_type(body, &locals, functions, targets, types, module)
        }
        CoreExpr::If { clauses } => common_branch_type(clauses.iter().map(|clause| {
            infer_core_type(&clause.body, variables, functions, targets, types, module)
        })),
        CoreExpr::Case { clauses, .. } => common_branch_type(clauses.iter().map(|clause| {
            infer_core_type(&clause.body, variables, functions, targets, types, module)
        })),
        _ => None,
    }
}

fn nominal_field_type(base: &CoreType, field: &str, types: &NominalTypes) -> Option<CoreType> {
    match base {
        CoreType::Struct { fields, .. } => fields
            .iter()
            .find(|candidate| candidate.name == field)
            .map(|candidate| candidate.ty.clone()),
        CoreType::Map(fields) => fields
            .iter()
            .find(|candidate| candidate.key == field)
            .map(|candidate| candidate.value.clone()),
        CoreType::Tuple(elements) => elements.iter().find_map(|element| match element {
            crate::terlan_typeck::CoreTupleTypeElem::Field { name, ty } if name == field => {
                Some(ty.clone())
            }
            _ => None,
        }),
        CoreType::Named(name) => nominal_field_type(&types.get(name)?.body, field, types),
        CoreType::Apply { constructor, args } => {
            let declaration = types.get(constructor)?;
            let substitution = declaration
                .params
                .iter()
                .cloned()
                .zip(args.iter().cloned())
                .collect::<HashMap<_, _>>();
            let body = super::super::generic_specialization::substitute(
                &declaration.body,
                &declaration.params,
                &substitution,
            );
            nominal_field_type(&body, field, types)
        }
        _ => None,
    }
}

/// Specializes a receiver method's result from its concrete receiver.
fn specialized_call_result(
    function: &str,
    args: &[CoreExpr],
    variables: &HashMap<String, CoreType>,
    functions: &HashMap<(String, usize), CoreType>,
    targets: &ReceiverTargets,
    types: &NominalTypes,
    module: &str,
) -> Option<CoreType> {
    let receiver = infer_core_type(args.first()?, variables, functions, targets, types, module)?;
    let method = function.rsplit('.').next()?;
    let target = receiver_target(method, args.len(), &receiver, module, targets)?;
    if callable_identity(target, module) != function {
        return None;
    }
    let result = target.result.as_ref()?;
    if target.generic_params.is_empty() {
        return Some(result.clone());
    }
    let mut substitution = HashMap::new();
    super::super::generic_specialization::unify(
        &target.receiver,
        &receiver,
        &target.generic_params,
        &mut substitution,
    )
    .ok()?;
    Some(super::super::generic_specialization::substitute(
        result,
        &target.generic_params,
        &substitution,
    ))
}

fn common_branch_type(types: impl Iterator<Item = Option<CoreType>>) -> Option<CoreType> {
    let mut types = types;
    let first = types.next()??;
    types
        .all(|candidate| candidate.as_ref() == Some(&first))
        .then_some(first)
}

#[cfg(test)]
#[path = "mutable_receivers_test.rs"]
mod tests;
