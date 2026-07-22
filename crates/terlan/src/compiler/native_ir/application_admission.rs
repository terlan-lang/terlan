//! Closed-world admission checks for direct-AOT application images.

use std::collections::{HashMap, HashSet};

use crate::terlan_typeck::{CoreExpr, CoreFunction, CoreImportKind, CoreModule};

use super::{
    constructors::NativeConstructorLayouts, native_return_type, native_type,
    scalar_replacement::scalar_replace_fixed_aggregates, NativeExpr, NativeModule, NativeType,
};

/// One function visible while resolving an application call.
#[derive(Clone, Copy)]
struct Provider<'a> {
    /// Module that owns the function.
    module: &'a str,
    /// Checked CoreIR function declaration.
    function: &'a CoreFunction,
}

/// Native call shape used to compare independently imported declarations.
#[derive(Debug, Eq, PartialEq)]
struct FunctionAbi {
    /// Ordered native parameter representations.
    params: Vec<Option<NativeType>>,
    /// Native result representation.
    result: Option<NativeType>,
}

/// Rejects an application whose statically reachable call identities are not
/// closed and unambiguous before NativeIR lowering begins.
pub(super) fn validate_core_application(
    cores: &[CoreModule],
    constructor_layouts: &HashMap<String, NativeConstructorLayouts>,
) -> Result<(), String> {
    reject_duplicate_modules(cores)?;
    for core in cores {
        reject_duplicate_functions(core)?;
        for function in &core.functions {
            if function.arity != function.params.len() {
                return Err(format!(
                    "error[native_ir.function_abi]: `{}.{}/{}` declares {} parameters",
                    core.module,
                    function.name,
                    function.arity,
                    function.params.len()
                ));
            }
            for clause in &function.clauses {
                if let Some(body) = &clause.body.core_expr {
                    let body = scalar_replace_fixed_aggregates(
                        body,
                        constructor_layouts.get(&core.module).ok_or_else(|| {
                            format!(
                                "error[native_ir.application_layout]: module `{}` has no constructor layout inventory",
                                core.module
                            )
                        })?,
                    );
                    validate_expr_calls(&body, core, cores)?;
                }
            }
        }
    }
    Ok(())
}

/// Rejects duplicate module identities that would otherwise be silently
/// collapsed by deterministic application ordering.
fn reject_duplicate_modules(cores: &[CoreModule]) -> Result<(), String> {
    let mut modules = HashSet::new();
    for core in cores {
        if !modules.insert(core.module.as_str()) {
            return Err(duplicate_module_diagnostic(&core.module));
        }
    }
    Ok(())
}

/// Returns the canonical diagnostic for a repeated application module identity.
pub(super) fn duplicate_module_diagnostic(module: &str) -> String {
    format!(
        "error[native_ir.duplicate_module]: application contains duplicate module `{module}`"
    )
}

/// Rejects duplicate local function identities before resolver construction.
fn reject_duplicate_functions(core: &CoreModule) -> Result<(), String> {
    let mut functions = HashSet::new();
    for function in &core.functions {
        if !functions.insert((function.name.as_str(), function.arity)) {
            return Err(format!(
                "error[native_ir.function_identity]: duplicate function `{}.{}/{}`",
                core.module, function.name, function.arity
            ));
        }
    }
    Ok(())
}

/// Walks the executable native subset and validates every ordinary call.
fn validate_expr_calls(
    expr: &CoreExpr,
    caller: &CoreModule,
    cores: &[CoreModule],
) -> Result<(), String> {
    match expr {
        CoreExpr::Call { function, args } => {
            for arg in args {
                validate_expr_calls(arg, caller, cores)?;
            }
            validate_call(function, args.len(), caller, cores)
        }
        CoreExpr::RemoteFunRef {
            module,
            function,
            arity,
        } => validate_call(&format!("{module}.{function}"), *arity, caller, cores),
        CoreExpr::FunctionCall { callee, args } => {
            validate_expr_calls(callee, caller, cores)?;
            validate_expr_sequence(args, caller, cores)
        }
        CoreExpr::Lam { body, .. } => validate_expr_calls(body, caller, cores),
        CoreExpr::ConstructorCall { args, .. }
        | CoreExpr::Intrinsic(crate::terlan_typeck::CoreIntrinsicCall { args, .. }) => {
            validate_expr_sequence(args, caller, cores)
        }
        CoreExpr::RecordConstruct { fields, .. } => {
            for field in fields {
                validate_expr_calls(&field.value, caller, cores)?;
            }
            Ok(())
        }
        CoreExpr::RecordUpdate { base, fields, .. } => {
            validate_expr_calls(base, caller, cores)?;
            for field in fields {
                validate_expr_calls(&field.value, caller, cores)?;
            }
            Ok(())
        }
        CoreExpr::UnaryOp { operand, .. } => validate_expr_calls(operand, caller, cores),
        CoreExpr::FieldAccess { base, .. } | CoreExpr::RecordAccess { base, .. } => {
            validate_expr_calls(base, caller, cores)
        }
        CoreExpr::BinaryOp { left, right, .. } => {
            validate_expr_calls(left, caller, cores)?;
            validate_expr_calls(right, caller, cores)
        }
        CoreExpr::Let { bindings, body } => {
            for binding in bindings {
                validate_expr_calls(&binding.value, caller, cores)?;
            }
            validate_expr_calls(body, caller, cores)
        }
        CoreExpr::If { clauses } => {
            for clause in clauses {
                validate_expr_calls(&clause.condition, caller, cores)?;
                validate_expr_calls(&clause.body, caller, cores)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Validates an ordered expression sequence.
fn validate_expr_sequence(
    expressions: &[CoreExpr],
    caller: &CoreModule,
    cores: &[CoreModule],
) -> Result<(), String> {
    for expression in expressions {
        validate_expr_calls(expression, caller, cores)?;
    }
    Ok(())
}

/// Resolves one call and rejects missing or conflicting providers.
fn validate_call(
    name: &str,
    arity: usize,
    caller: &CoreModule,
    cores: &[CoreModule],
) -> Result<(), String> {
    let providers = call_providers(name, arity, caller, cores);
    match providers.as_slice() {
        [] => Err(format!(
            "error[native_ir.unresolved_call]: `{}.{name}/{arity}` has no function in the native application closure",
            caller.module
        )),
        [_] => Ok(()),
        [first, rest @ ..] => {
            let first_abi = function_abi(first.function);
            let incompatible = rest
                .iter()
                .any(|provider| function_abi(provider.function) != first_abi);
            let mut identities = providers
                .iter()
                .map(|provider| format!("{}.{name}/{arity}", provider.module))
                .collect::<Vec<_>>();
            identities.sort();
            let code = if incompatible {
                "native_ir.import_abi"
            } else {
                "native_ir.ambiguous_import"
            };
            Err(format!(
                "error[{code}]: call `{name}/{arity}` in module `{}` resolves to {}",
                caller.module,
                identities.join(", ")
            ))
        }
    }
}

/// Returns every local or explicitly imported provider for one call identity.
fn call_providers<'a>(
    name: &str,
    arity: usize,
    caller: &'a CoreModule,
    cores: &'a [CoreModule],
) -> Vec<Provider<'a>> {
    if let Some(local) = caller
        .functions
        .iter()
        .find(|function| function.name == name && function.arity == arity)
    {
        return vec![Provider {
            module: &caller.module,
            function: local,
        }];
    }
    cores
        .iter()
        .filter(|core| core.module != caller.module)
        .flat_map(|core| {
            core.functions
                .iter()
                .filter(move |function| {
                    function.public
                        && function.arity == arity
                        && imports_module(caller, &core.module, &function.name)
                        && (function.name == name
                            || format!("{}.{}", core.module, function.name) == name)
                })
                .map(move |function| Provider {
                    module: &core.module,
                    function,
                })
        })
        .collect()
}

/// Reports whether a runtime module import makes a provider visible.
fn imports_module(caller: &CoreModule, module: &str, function: &str) -> bool {
    caller.imports.iter().any(|import| {
        import.kind == CoreImportKind::Module
            && (import.module == module || import.module == format!("{module}.{function}"))
    })
}

/// Projects a checked function declaration onto the direct-AOT ABI.
fn function_abi(function: &CoreFunction) -> FunctionAbi {
    FunctionAbi {
        params: function
            .params
            .iter()
            .map(|param| native_type(param.core_ty.as_ref(), &param.ty))
            .collect(),
        result: native_return_type(function),
    }
}

/// Rejects duplicate or dangling continuation identities in the fully lowered
/// application before an object file can be emitted.
pub(super) fn validate_continuation_graph(modules: &[NativeModule]) -> Result<(), String> {
    let mut owners = HashMap::new();
    for module in modules {
        for continuation in &module.continuations {
            if continuation.id == 0
                || owners
                    .insert(continuation.id, module.name.as_str())
                    .is_some()
            {
                return Err(format!(
                    "error[native_ir.continuation_graph]: continuation identity {} is ambiguous",
                    continuation.id
                ));
            }
        }
    }
    for module in modules {
        for function in &module.functions {
            validate_continuation_references(&function.body, &owners, &module.name)?;
        }
        for continuation in &module.continuations {
            validate_continuation_references(&continuation.body, &owners, &module.name)?;
        }
    }
    Ok(())
}

/// Walks one NativeIR expression and verifies all resume identities.
fn validate_continuation_references(
    expr: &NativeExpr,
    owners: &HashMap<u64, &str>,
    module: &str,
) -> Result<(), String> {
    match expr {
        NativeExpr::Construct { fields, .. }
        | NativeExpr::ManagedOperation { args: fields, .. }
        | NativeExpr::MakeClosure {
            captures: fields, ..
        }
        | NativeExpr::Call { args: fields, .. }
        | NativeExpr::TailCall { args: fields, .. } => {
            validate_native_sequence(fields, owners, module)
        }
        NativeExpr::InvokeClosure { callee, args, .. } => {
            validate_continuation_references(callee, owners, module)?;
            validate_native_sequence(args, owners, module)
        }
        NativeExpr::CallThen {
            args,
            callee_continuation_id,
            continuation_id,
            values,
            resume,
            ..
        } => {
            require_continuation(*callee_continuation_id, owners, module)?;
            require_continuation(*continuation_id, owners, module)?;
            validate_native_sequence(args, owners, module)?;
            validate_native_sequence(values, owners, module)?;
            validate_continuation_references(resume, owners, module)
        }
        NativeExpr::Neg(operand)
        | NativeExpr::FloatNeg(operand)
        | NativeExpr::IntToFloat(operand)
        | NativeExpr::Not(operand) => validate_continuation_references(operand, owners, module),
        NativeExpr::Binary { left, right, .. } => {
            validate_continuation_references(left, owners, module)?;
            validate_continuation_references(right, owners, module)
        }
        NativeExpr::Let { bindings, body } => {
            validate_native_sequence(bindings, owners, module)?;
            validate_continuation_references(body, owners, module)
        }
        NativeExpr::If { clauses } => {
            for (condition, body) in clauses {
                validate_continuation_references(condition, owners, module)?;
                validate_continuation_references(body, owners, module)?;
            }
            Ok(())
        }
        NativeExpr::Try {
            protected,
            success,
            failure,
            cleanup,
        } => {
            validate_continuation_references(protected, owners, module)?;
            validate_continuation_references(success, owners, module)?;
            validate_continuation_references(failure, owners, module)?;
            validate_native_sequence(cleanup, owners, module)
        }
        NativeExpr::Suspend {
            arguments,
            continuation_id,
            values,
            ..
        } => {
            require_continuation(*continuation_id, owners, module)?;
            validate_native_sequence(arguments, owners, module)?;
            validate_native_sequence(values, owners, module)
        }
        NativeExpr::Unit
        | NativeExpr::Int(_)
        | NativeExpr::Float(_)
        | NativeExpr::Bool(_)
        | NativeExpr::StringLiteral { .. }
        | NativeExpr::Param(_) => Ok(()),
    }
}

/// Validates every expression in one NativeIR sequence.
fn validate_native_sequence(
    expressions: &[NativeExpr],
    owners: &HashMap<u64, &str>,
    module: &str,
) -> Result<(), String> {
    for expression in expressions {
        validate_continuation_references(expression, owners, module)?;
    }
    Ok(())
}

/// Requires one resume identity to exist in the closed application graph.
fn require_continuation(
    identity: u64,
    owners: &HashMap<u64, &str>,
    module: &str,
) -> Result<(), String> {
    if owners.contains_key(&identity) {
        Ok(())
    } else {
        Err(format!(
            "error[native_ir.continuation_graph]: module `{module}` references missing continuation {identity}"
        ))
    }
}
