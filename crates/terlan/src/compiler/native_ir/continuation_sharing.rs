//! Linearizes composed calls by sharing continuation bodies as hidden functions.

use std::collections::{hash_map::DefaultHasher, HashMap};
use std::fmt::{self, Write};
use std::hash::{Hash, Hasher};

use super::{NativeContinuation, NativeExpr, NativeFunction, NativeModule};

/// Resolves identity-based continuation calls after application lowering.
///
/// A continuation body is emitted once and ordinary call sites reference that
/// hidden function. This prevents nested suspending calls from recursively
/// cloning complete continuation trees into every caller wrapper.
pub(super) fn materialize_shared_continuations(
    modules: &mut Vec<NativeModule>,
) -> Result<(), String> {
    intern_equivalent_continuations(modules);
    let function_count = modules
        .iter()
        .map(|module| module.functions.len())
        .sum::<usize>();
    let continuations = modules
        .iter()
        .flat_map(|module| module.continuations.iter().cloned())
        .collect::<Vec<_>>();
    if continuations.is_empty() {
        return Ok(());
    }

    let indexes = continuations
        .iter()
        .enumerate()
        .map(|(offset, continuation)| {
            (
                continuation.id,
                (
                    function_count.saturating_add(offset),
                    continuation.params.len(),
                ),
            )
        })
        .collect::<HashMap<_, _>>();
    if indexes.len() != continuations.len() {
        return Err(
            "error[native_ir.continuation_identity]: duplicate continuation identity".to_string(),
        );
    }

    for module in modules.iter_mut() {
        for function in &mut module.functions {
            resolve_calls(&mut function.body, &indexes)?;
        }
        for continuation in &mut module.continuations {
            resolve_calls(&mut continuation.body, &indexes)?;
        }
    }

    let mut functions = Vec::with_capacity(continuations.len());
    for continuation in continuations {
        let mut body = continuation.body;
        resolve_calls(&mut body, &indexes)?;
        functions.push(NativeFunction {
            export_id: super::identity::stable_export_id(
                "$terlan.continuation_bodies",
                &continuation.id.to_string(),
                continuation.params.len(),
            ),
            name: format!("$continuation_{}", continuation.id),
            public: false,
            arity: continuation.params.len(),
            source_module: continuation.source_module,
            source_function: continuation.source_function,
            source_arity: continuation.source_arity,
            callable_captures: Vec::new(),
            params: continuation.params,
            return_type: continuation.return_type,
            body,
        });
    }
    let atoms = modules
        .first()
        .ok_or_else(|| "error[native_ir.continuation_module]: application is empty".to_string())?
        .atoms
        .clone();
    let mut managed_layouts = modules
        .iter()
        .flat_map(|module| module.managed_layouts.iter().cloned())
        .collect::<Vec<_>>();
    managed_layouts.sort_by(|left, right| left.as_ref().cmp(right.as_ref()));
    managed_layouts.dedup_by(|left, right| left.as_ref() == right.as_ref());
    let mut managed_collections = modules
        .iter()
        .flat_map(|module| module.managed_collections.iter().cloned())
        .collect::<Vec<_>>();
    managed_collections.sort_by(|left, right| left.as_ref().cmp(right.as_ref()));
    managed_collections.dedup_by(|left, right| left.as_ref() == right.as_ref());
    modules.push(NativeModule {
        name: "$terlan.continuations".to_string(),
        functions,
        continuations: Vec::new(),
        managed_layouts,
        managed_collections,
        atoms,
    });
    Ok(())
}

/// Collapses structurally identical resume suffixes into a continuation DAG.
///
/// Short-circuit branches may reach the same remaining evaluation context.
/// Lowering initially gives each path a stable identity; interning those
/// equivalent bodies before code generation prevents code size from growing
/// with the number of paths while retaining exact short-circuit semantics.
pub(super) fn intern_equivalent_continuations(modules: &mut [NativeModule]) {
    let mut owners = Vec::new();
    let mut continuations = Vec::new();
    for (owner, module) in modules.iter_mut().enumerate() {
        let owned = std::mem::take(&mut module.continuations);
        owners.extend(std::iter::repeat_n(owner, owned.len()));
        continuations.extend(owned);
    }
    let aliases = intern_continuations(&mut continuations);
    rewrite_application_continuation_ids(modules, &aliases);
    for (owner, mut continuation) in owners.into_iter().zip(continuations) {
        if canonical_id(continuation.id, &aliases) != continuation.id {
            continue;
        }
        rewrite_continuation_ids(&mut continuation.body, &aliases);
        modules[owner].continuations.push(continuation);
    }
}

/// Interns the continuation graph produced while lowering one function.
///
/// This runs before composability profiling so path-count duplication cannot
/// make an otherwise bounded suspending function appear to exceed the
/// continuation admission budget.
pub(super) fn intern_function_continuations(
    body: &mut NativeExpr,
    continuations: &mut Vec<NativeContinuation>,
) {
    let aliases = intern_continuations(continuations);
    rewrite_continuation_ids(body, &aliases);
    for continuation in continuations.iter_mut() {
        rewrite_continuation_ids(&mut continuation.body, &aliases);
    }
    continuations.retain(|continuation| canonical_id(continuation.id, &aliases) == continuation.id);
}

/// Interns an acyclic continuation graph in dependency order.
///
/// The earlier fixed-point implementation rediscovered only one equivalent
/// suffix layer per pass. Deep generated DAGs therefore multiplied complete
/// graph scans. A DFS canonicalizes every referenced suffix before its parent,
/// making the work proportional to the graph plus collision candidates.
fn intern_continuations(continuations: &mut [NativeContinuation]) -> HashMap<u64, u64> {
    let indexes = continuations
        .iter()
        .enumerate()
        .map(|(index, continuation)| (continuation.id, index))
        .collect::<HashMap<_, _>>();
    let mut states = vec![0u8; continuations.len()];
    let mut aliases = HashMap::new();
    let mut canonical = HashMap::<u64, Vec<usize>>::new();
    for index in (0..continuations.len()).rev() {
        intern_continuation(
            index,
            continuations,
            &indexes,
            &mut states,
            &mut aliases,
            &mut canonical,
        );
    }
    aliases
}

fn intern_continuation(
    index: usize,
    continuations: &mut [NativeContinuation],
    indexes: &HashMap<u64, usize>,
    states: &mut [u8],
    aliases: &mut HashMap<u64, u64>,
    canonical: &mut HashMap<u64, Vec<usize>>,
) -> u64 {
    if states[index] == 2 {
        return canonical_id(continuations[index].id, aliases);
    }
    if states[index] == 1 {
        return continuations[index].id;
    }
    states[index] = 1;
    let mut references = Vec::new();
    continuation_references(&continuations[index].body, &mut references);
    references.sort_unstable();
    references.dedup();
    for reference in references {
        if let Some(dependency) = indexes.get(&reference).copied() {
            intern_continuation(
                dependency,
                continuations,
                indexes,
                states,
                aliases,
                canonical,
            );
        }
    }
    rewrite_continuation_ids(&mut continuations[index].body, aliases);
    let fingerprint = continuation_fingerprint(&continuations[index]);
    let candidates = canonical.get(&fingerprint).cloned().unwrap_or_default();
    if let Some(existing) = candidates.into_iter().find(|existing| {
        continuations[*existing].params == continuations[index].params
            && continuations[*existing].return_type == continuations[index].return_type
            && continuations[*existing].body == continuations[index].body
    }) {
        aliases.insert(continuations[index].id, continuations[existing].id);
    } else {
        canonical.entry(fingerprint).or_default().push(index);
    }
    states[index] = 2;
    canonical_id(continuations[index].id, aliases)
}

fn canonical_id(mut id: u64, aliases: &HashMap<u64, u64>) -> u64 {
    while let Some(next) = aliases.get(&id).copied() {
        if next == id {
            break;
        }
        id = next;
    }
    id
}

fn continuation_references(expr: &NativeExpr, references: &mut Vec<u64>) {
    match expr {
        NativeExpr::Construct { fields, .. }
        | NativeExpr::ManagedOperation { args: fields, .. }
        | NativeExpr::MakeClosure {
            captures: fields, ..
        }
        | NativeExpr::Call { args: fields, .. }
        | NativeExpr::TailCall { args: fields, .. } => fields
            .iter()
            .for_each(|field| continuation_references(field, references)),
        NativeExpr::InvokeClosure { callee, args, .. } => {
            continuation_references(callee, references);
            args.iter()
                .for_each(|arg| continuation_references(arg, references));
        }
        NativeExpr::CallThen {
            args,
            callee_continuation_id,
            continuation_id,
            completion_continuation_id,
            values,
            ..
        } => {
            references.push(*callee_continuation_id);
            references.push(*continuation_id);
            references.push(*completion_continuation_id);
            args.iter()
                .chain(values)
                .for_each(|value| continuation_references(value, references));
        }
        NativeExpr::ContinuationTailCall {
            continuation_id,
            args,
        } => {
            references.push(*continuation_id);
            args.iter()
                .for_each(|arg| continuation_references(arg, references));
        }
        NativeExpr::Neg(value)
        | NativeExpr::FloatNeg(value)
        | NativeExpr::FloatFloor(value)
        | NativeExpr::FloatCeil(value)
        | NativeExpr::IntToFloat(value)
        | NativeExpr::Not(value) => continuation_references(value, references),
        NativeExpr::Binary { left, right, .. } => {
            continuation_references(left, references);
            continuation_references(right, references);
        }
        NativeExpr::Let { bindings, body } => {
            bindings
                .iter()
                .for_each(|binding| continuation_references(binding, references));
            continuation_references(body, references);
        }
        NativeExpr::If { clauses } => {
            for (condition, body) in clauses {
                continuation_references(condition, references);
                continuation_references(body, references);
            }
        }
        NativeExpr::Try {
            protected,
            success,
            failure,
            cleanup,
        } => {
            continuation_references(protected, references);
            continuation_references(success, references);
            continuation_references(failure, references);
            cleanup
                .iter()
                .for_each(|value| continuation_references(value, references));
        }
        NativeExpr::Suspend {
            arguments,
            continuation_id,
            values,
            ..
        } => {
            references.push(*continuation_id);
            arguments
                .iter()
                .chain(values)
                .for_each(|value| continuation_references(value, references));
        }
        NativeExpr::Unit
        | NativeExpr::Int(_)
        | NativeExpr::Float(_)
        | NativeExpr::Bool(_)
        | NativeExpr::AtomLiteral(_)
        | NativeExpr::StringLiteral { .. }
        | NativeExpr::Param(_) => {}
    }
}

fn rewrite_application_continuation_ids(modules: &mut [NativeModule], aliases: &HashMap<u64, u64>) {
    for module in modules {
        for function in &mut module.functions {
            rewrite_continuation_ids(&mut function.body, aliases);
        }
        for continuation in &mut module.continuations {
            rewrite_continuation_ids(&mut continuation.body, aliases);
        }
    }
}

fn rewrite_continuation_ids(expr: &mut NativeExpr, aliases: &HashMap<u64, u64>) {
    match expr {
        NativeExpr::Construct { fields, .. }
        | NativeExpr::ManagedOperation { args: fields, .. }
        | NativeExpr::MakeClosure {
            captures: fields, ..
        }
        | NativeExpr::Call { args: fields, .. }
        | NativeExpr::TailCall { args: fields, .. } => {
            fields
                .iter_mut()
                .for_each(|field| rewrite_continuation_ids(field, aliases));
        }
        NativeExpr::InvokeClosure { callee, args, .. } => {
            rewrite_continuation_ids(callee, aliases);
            args.iter_mut()
                .for_each(|arg| rewrite_continuation_ids(arg, aliases));
        }
        NativeExpr::CallThen {
            args,
            callee_continuation_id,
            continuation_id,
            completion_continuation_id,
            completion_function,
            values,
            ..
        } => {
            *callee_continuation_id = canonical_id(*callee_continuation_id, aliases);
            *continuation_id = canonical_id(*continuation_id, aliases);
            *completion_continuation_id = canonical_id(*completion_continuation_id, aliases);
            *completion_function = None;
            args.iter_mut()
                .chain(values)
                .for_each(|value| rewrite_continuation_ids(value, aliases));
        }
        NativeExpr::ContinuationTailCall {
            continuation_id,
            args,
        } => {
            *continuation_id = canonical_id(*continuation_id, aliases);
            args.iter_mut()
                .for_each(|arg| rewrite_continuation_ids(arg, aliases));
        }
        NativeExpr::Neg(value)
        | NativeExpr::FloatNeg(value)
        | NativeExpr::FloatFloor(value)
        | NativeExpr::FloatCeil(value)
        | NativeExpr::IntToFloat(value)
        | NativeExpr::Not(value) => rewrite_continuation_ids(value, aliases),
        NativeExpr::Binary { left, right, .. } => {
            rewrite_continuation_ids(left, aliases);
            rewrite_continuation_ids(right, aliases);
        }
        NativeExpr::Let { bindings, body } => {
            bindings
                .iter_mut()
                .for_each(|binding| rewrite_continuation_ids(binding, aliases));
            rewrite_continuation_ids(body, aliases);
        }
        NativeExpr::If { clauses } => {
            for (condition, body) in clauses {
                rewrite_continuation_ids(condition, aliases);
                rewrite_continuation_ids(body, aliases);
            }
        }
        NativeExpr::Try {
            protected,
            success,
            failure,
            cleanup,
        } => {
            rewrite_continuation_ids(protected, aliases);
            rewrite_continuation_ids(success, aliases);
            rewrite_continuation_ids(failure, aliases);
            cleanup
                .iter_mut()
                .for_each(|value| rewrite_continuation_ids(value, aliases));
        }
        NativeExpr::Suspend {
            arguments,
            continuation_id,
            values,
            ..
        } => {
            *continuation_id = canonical_id(*continuation_id, aliases);
            arguments
                .iter_mut()
                .chain(values)
                .for_each(|value| rewrite_continuation_ids(value, aliases));
        }
        NativeExpr::Unit
        | NativeExpr::Int(_)
        | NativeExpr::Float(_)
        | NativeExpr::Bool(_)
        | NativeExpr::AtomLiteral(_)
        | NativeExpr::StringLiteral { .. }
        | NativeExpr::Param(_) => {}
    }
}

fn continuation_fingerprint(continuation: &NativeContinuation) -> u64 {
    let mut writer = HashWriter(DefaultHasher::new());
    let _ = write!(
        writer,
        "{:?}|{:?}|{:?}",
        continuation.params, continuation.return_type, continuation.body
    );
    writer.0.finish()
}

struct HashWriter(DefaultHasher);

impl Write for HashWriter {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        value.hash(&mut self.0);
        Ok(())
    }
}

fn resolve_calls(
    expr: &mut NativeExpr,
    indexes: &HashMap<u64, (usize, usize)>,
) -> Result<(), String> {
    match expr {
        NativeExpr::ContinuationTailCall {
            continuation_id,
            args,
        } => {
            resolve_sequence(args, indexes)?;
            let (function, arity) = continuation_function(*continuation_id, indexes)?;
            let args = std::mem::take(args);
            if args.len() != arity {
                return Err(format!(
                    "error[native_ir.continuation_call_arity]: continuation {continuation_id} expects {arity} argument(s), found {}",
                    args.len()
                ));
            }
            *expr = NativeExpr::TailCall { function, args };
        }
        NativeExpr::Construct { fields, .. }
        | NativeExpr::ManagedOperation { args: fields, .. }
        | NativeExpr::MakeClosure {
            captures: fields, ..
        }
        | NativeExpr::Call { args: fields, .. }
        | NativeExpr::TailCall { args: fields, .. } => resolve_sequence(fields, indexes)?,
        NativeExpr::InvokeClosure { callee, args, .. } => {
            resolve_calls(callee, indexes)?;
            resolve_sequence(args, indexes)?;
        }
        NativeExpr::CallThen {
            args,
            completion_continuation_id,
            completion_function,
            values,
            ..
        } => {
            resolve_sequence(args, indexes)?;
            resolve_sequence(values, indexes)?;
            *completion_function =
                Some(continuation_function(*completion_continuation_id, indexes)?.0);
        }
        NativeExpr::Neg(value)
        | NativeExpr::FloatNeg(value)
        | NativeExpr::FloatFloor(value)
        | NativeExpr::FloatCeil(value)
        | NativeExpr::IntToFloat(value)
        | NativeExpr::Not(value) => resolve_calls(value, indexes)?,
        NativeExpr::Binary { left, right, .. } => {
            resolve_calls(left, indexes)?;
            resolve_calls(right, indexes)?;
        }
        NativeExpr::Let { bindings, body } => {
            resolve_sequence(bindings, indexes)?;
            resolve_calls(body, indexes)?;
        }
        NativeExpr::If { clauses } => {
            for (condition, body) in clauses {
                resolve_calls(condition, indexes)?;
                resolve_calls(body, indexes)?;
            }
        }
        NativeExpr::Try {
            protected,
            success,
            failure,
            cleanup,
        } => {
            resolve_calls(protected, indexes)?;
            resolve_calls(success, indexes)?;
            resolve_calls(failure, indexes)?;
            resolve_sequence(cleanup, indexes)?;
        }
        NativeExpr::Suspend {
            arguments, values, ..
        } => {
            resolve_sequence(arguments, indexes)?;
            resolve_sequence(values, indexes)?;
        }
        NativeExpr::Unit
        | NativeExpr::Int(_)
        | NativeExpr::Float(_)
        | NativeExpr::Bool(_)
        | NativeExpr::AtomLiteral(_)
        | NativeExpr::StringLiteral { .. }
        | NativeExpr::Param(_) => {}
    }
    Ok(())
}

fn resolve_sequence(
    expressions: &mut [NativeExpr],
    indexes: &HashMap<u64, (usize, usize)>,
) -> Result<(), String> {
    expressions
        .iter_mut()
        .try_for_each(|expression| resolve_calls(expression, indexes))
}

fn continuation_function(
    id: u64,
    indexes: &HashMap<u64, (usize, usize)>,
) -> Result<(usize, usize), String> {
    indexes.get(&id).copied().ok_or_else(|| {
        format!("error[native_ir.continuation_call]: continuation {id} is unavailable")
    })
}
