//! Fail-closed aggregate field-use analysis over typed NativeIR.

use crate::runtime::native_image::managed::{
    decode_aggregate_field_projection, scalar_string_projection_rewrite, SemanticTypeId,
};
use std::collections::BTreeSet;

use super::{NativeExpr, NativeFunction, NativeModule, NativeType};

pub(crate) use crate::runtime::native_image::aggregate_projection::{
    AggregateFieldProjection, NativeAggregateProjection,
};

const SCALAR_AGGREGATE_INGRESS_PREFIX: &str = "__terlan_aggregate_scalar_ingress_";

/// Computes aggregate projections for public exports whose first ordinary
/// parameter is the managed aggregate value.
pub(crate) fn native_aggregate_projections(
    modules: &[NativeModule],
) -> Vec<NativeAggregateProjection> {
    let suspending = application_suspension_profile(modules);
    let functions: Vec<_> = modules
        .iter()
        .flat_map(|module| &module.functions)
        .collect();
    let functions = functions.as_slice();
    modules
        .iter()
        .zip(suspending)
        .flat_map(|(module, suspending)| {
            module
                .functions
                .iter()
                .enumerate()
                .filter_map(move |(index, function)| {
                    let NativeType::ManagedRef(aggregate_semantic) = *function.params.first()?
                    else {
                        return None;
                    };
                    analyze_function(function, aggregate_semantic, functions).map(|fields| {
                        NativeAggregateProjection {
                            semantic: aggregate_semantic,
                            module: module.name.clone(),
                            function: function.name.clone(),
                            arity: function.arity,
                            fields,
                            scalar_entry: None,
                            scalar_field: None,
                            suspending: suspending.get(index).copied().unwrap_or(true),
                        }
                    })
                })
        })
        .collect()
}

/// Computes suspension over the application-global function index space used
/// by NativeIR calls, then partitions the proof back into module order.
fn application_suspension_profile(modules: &[NativeModule]) -> Vec<Vec<bool>> {
    let function_count = modules
        .iter()
        .map(|module| module.functions.len())
        .sum::<usize>();
    let mut suspending = vec![false; function_count];
    for _ in 0..=function_count {
        let next = modules
            .iter()
            .flat_map(|module| module.functions.iter())
            .map(|function| super::suspension::is_suspending(&function.body, &suspending))
            .collect::<Vec<_>>();
        if next == suspending {
            break;
        }
        suspending = next;
    }
    let mut offset = 0_usize;
    modules
        .iter()
        .map(|module| {
            let end = offset + module.functions.len();
            let profile = suspending[offset..end].to_vec();
            offset = end;
            profile
        })
        .collect()
}

/// Installs private-shape aggregate ingress exports after ordinary application
/// lowering, preserving every source-visible function and internal call index.
///
/// Single-module images can append a specialization without rebasing calls. Appending
/// the generated entry to that module cannot shift an existing application
/// function index. Multi-module images retain projection metadata but do not
/// receive the scalar ABI until application-wide index rebasing is available.
pub(crate) fn install_native_aggregate_projection_exports(
    modules: &mut [NativeModule],
) -> Vec<NativeAggregateProjection> {
    let mut projections = native_aggregate_projections(modules);
    if modules.len() != 1 {
        return projections;
    }
    let mut generated = Vec::new();
    for projection in &mut projections {
        let Some(field) =
            sole_scalar_string_field(&projection.fields, projection.semantic, modules)
        else {
            continue;
        };
        if projection.arity != 1 {
            continue;
        }
        let module = &modules[0];
        let Some(original) = module
            .functions
            .iter()
            .find(|function| {
                function.public
                    && function.name == projection.function
                    && function.arity == projection.arity
            })
            .cloned()
        else {
            continue;
        };
        let Some(body) = rewrite_scalar_aggregate_body(&original.body, projection.semantic, field)
        else {
            continue;
        };
        let entry = format!(
            "{SCALAR_AGGREGATE_INGRESS_PREFIX}{:016x}_{field}",
            original.export_id
        );
        if module
            .functions
            .iter()
            .chain(generated.iter())
            .any(|function: &NativeFunction| function.name == entry && function.arity == 1)
        {
            continue;
        }
        generated.push(NativeFunction {
            export_id: super::stable_export_id(&module.name, &entry, 1),
            name: entry.clone(),
            public: true,
            arity: 1,
            source_module: original.source_module.clone(),
            source_function: original.source_function.clone(),
            source_arity: original.source_arity,
            callable_captures: Vec::new(),
            params: vec![NativeType::StringRef],
            return_type: original.return_type,
            body,
        });
        projection.scalar_entry = Some(entry);
        projection.scalar_field = Some(field);
    }
    modules[0].functions.extend(generated);
    projections
}

fn sole_scalar_string_field(
    projection: &AggregateFieldProjection,
    semantic: SemanticTypeId,
    modules: &[NativeModule],
) -> Option<usize> {
    use crate::runtime::native_image::managed::{
        decode_aggregate_layout, managed_string_semantic_id, ManagedFieldType,
    };

    let AggregateFieldProjection::Fields(fields) = projection else {
        return None;
    };
    if fields.len() != 1 {
        return None;
    }
    let field = *fields.first()?;
    let string = managed_string_semantic_id();
    let mut found = false;
    for encoded in modules.iter().flat_map(|module| &module.managed_layouts) {
        let layout = decode_aggregate_layout(encoded).ok()?;
        if layout.managed().semantic_id() == semantic {
            found = true;
            if layout.fields().get(field)?.field_type() != ManagedFieldType::Reference(string) {
                return None;
            }
        }
    }
    found.then_some(field)
}

/// Replaces exact field projections from parameter zero with that parameter.
///
/// Returning `None` is fail-closed: aliases, different aggregate fields, or any
/// raw use of the original aggregate retain the complete managed ingress.
fn rewrite_scalar_aggregate_body(
    expression: &NativeExpr,
    aggregate_semantic: SemanticTypeId,
    field: usize,
) -> Option<NativeExpr> {
    if let NativeExpr::ManagedOperation { encoded, args } = expression {
        if let Some((semantic, projected, scalar_operation)) =
            scalar_string_projection_rewrite(encoded)
        {
            if semantic == aggregate_semantic
                && projected == field
                && args.as_slice() == [NativeExpr::Param(0)]
            {
                return Some(match scalar_operation {
                    None => NativeExpr::Param(0),
                    Some(encoded) => NativeExpr::ManagedOperation {
                        encoded: encoded.into(),
                        args: vec![NativeExpr::Param(0)],
                    },
                });
            }
        }
    }
    Some(match expression {
        NativeExpr::Param(0) => return None,
        NativeExpr::ManagedOperation { encoded, args } => NativeExpr::ManagedOperation {
            encoded: encoded.clone(),
            args: rewrite_expressions(args, aggregate_semantic, field)?,
        },
        NativeExpr::MakeClosure { encoded, captures } => NativeExpr::MakeClosure {
            encoded: encoded.clone(),
            captures: rewrite_expressions(captures, aggregate_semantic, field)?,
        },
        NativeExpr::Construct {
            descriptor,
            encoded_layout,
            fields,
        } => NativeExpr::Construct {
            descriptor: descriptor.clone(),
            encoded_layout: encoded_layout.clone(),
            fields: rewrite_expressions(fields, aggregate_semantic, field)?,
        },
        NativeExpr::Call { function, args } => NativeExpr::Call {
            function: *function,
            args: rewrite_expressions(args, aggregate_semantic, field)?,
        },
        NativeExpr::InvokeClosure {
            callee,
            args,
            parameter_types,
            result_type,
        } => NativeExpr::InvokeClosure {
            callee: Box::new(rewrite_scalar_aggregate_body(
                callee,
                aggregate_semantic,
                field,
            )?),
            args: rewrite_expressions(args, aggregate_semantic, field)?,
            parameter_types: parameter_types.clone(),
            result_type: *result_type,
        },
        NativeExpr::TailCall {
            function,
            args,
            yield_continuation_id,
        } => NativeExpr::TailCall {
            function: *function,
            args: rewrite_expressions(args, aggregate_semantic, field)?,
            yield_continuation_id: *yield_continuation_id,
        },
        NativeExpr::CallThen {
            function,
            args,
            resumes,
            completion_continuation_id,
            completion_function,
            values,
        } => NativeExpr::CallThen {
            function: *function,
            args: rewrite_expressions(args, aggregate_semantic, field)?,
            resumes: resumes.clone(),
            completion_continuation_id: *completion_continuation_id,
            completion_function: *completion_function,
            values: rewrite_expressions(values, aggregate_semantic, field)?,
        },
        NativeExpr::Neg(value) => NativeExpr::Neg(Box::new(rewrite_scalar_aggregate_body(
            value,
            aggregate_semantic,
            field,
        )?)),
        NativeExpr::FloatNeg(value) => NativeExpr::FloatNeg(Box::new(
            rewrite_scalar_aggregate_body(value, aggregate_semantic, field)?,
        )),
        NativeExpr::FloatFloor(value) => NativeExpr::FloatFloor(Box::new(
            rewrite_scalar_aggregate_body(value, aggregate_semantic, field)?,
        )),
        NativeExpr::FloatCeil(value) => NativeExpr::FloatCeil(Box::new(
            rewrite_scalar_aggregate_body(value, aggregate_semantic, field)?,
        )),
        NativeExpr::IntToFloat(value) => NativeExpr::IntToFloat(Box::new(
            rewrite_scalar_aggregate_body(value, aggregate_semantic, field)?,
        )),
        NativeExpr::Not(value) => NativeExpr::Not(Box::new(rewrite_scalar_aggregate_body(
            value,
            aggregate_semantic,
            field,
        )?)),
        NativeExpr::Binary {
            operator,
            operand_type,
            left,
            right,
        } => NativeExpr::Binary {
            operator: *operator,
            operand_type: *operand_type,
            left: Box::new(rewrite_scalar_aggregate_body(
                left,
                aggregate_semantic,
                field,
            )?),
            right: Box::new(rewrite_scalar_aggregate_body(
                right,
                aggregate_semantic,
                field,
            )?),
        },
        NativeExpr::Let { bindings, body } => NativeExpr::Let {
            bindings: rewrite_expressions(bindings, aggregate_semantic, field)?,
            body: Box::new(rewrite_scalar_aggregate_body(
                body,
                aggregate_semantic,
                field,
            )?),
        },
        NativeExpr::If { clauses } => NativeExpr::If {
            clauses: clauses
                .iter()
                .map(|(condition, body)| {
                    Some((
                        rewrite_scalar_aggregate_body(condition, aggregate_semantic, field)?,
                        rewrite_scalar_aggregate_body(body, aggregate_semantic, field)?,
                    ))
                })
                .collect::<Option<Vec<_>>>()?,
        },
        NativeExpr::Try {
            protected,
            success,
            failure,
            cleanup,
        } => NativeExpr::Try {
            protected: Box::new(rewrite_scalar_aggregate_body(
                protected,
                aggregate_semantic,
                field,
            )?),
            success: Box::new(rewrite_scalar_aggregate_body(
                success,
                aggregate_semantic,
                field,
            )?),
            failure: Box::new(rewrite_scalar_aggregate_body(
                failure,
                aggregate_semantic,
                field,
            )?),
            cleanup: rewrite_expressions(cleanup, aggregate_semantic, field)?,
        },
        NativeExpr::Suspend {
            operation,
            arguments,
            continuation_id,
            values,
        } => NativeExpr::Suspend {
            operation: *operation,
            arguments: rewrite_expressions(arguments, aggregate_semantic, field)?,
            continuation_id: *continuation_id,
            values: rewrite_expressions(values, aggregate_semantic, field)?,
        },
        other => other.clone(),
    })
}

fn rewrite_expressions(
    expressions: &[NativeExpr],
    aggregate_semantic: SemanticTypeId,
    field: usize,
) -> Option<Vec<NativeExpr>> {
    expressions
        .iter()
        .map(|expression| rewrite_scalar_aggregate_body(expression, aggregate_semantic, field))
        .collect()
}

fn analyze_function(
    function: &NativeFunction,
    aggregate_semantic: SemanticTypeId,
    functions: &[&NativeFunction],
) -> Option<AggregateFieldProjection> {
    if !function.public
        || !function.callable_captures.is_empty()
        || function.params.first() != Some(&NativeType::ManagedRef(aggregate_semantic))
    {
        return None;
    }
    let mut state = Analysis {
        aggregate_semantic,
        fields: BTreeSet::new(),
        escaped: false,
        functions,
        calls: BTreeSet::new(),
        call_budget: 128,
    };
    let mut origins = vec![Origin::Aggregate];
    origins.extend(function.params.iter().skip(1).map(|_| Origin::Other));
    if state.expr(&function.body, &mut origins) == Origin::Aggregate {
        state.escaped = true;
    }
    Some(if state.escaped {
        AggregateFieldProjection::Complete
    } else {
        AggregateFieldProjection::Fields(state.fields)
    })
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Origin {
    Other,
    Aggregate,
}

struct Analysis<'a> {
    aggregate_semantic: SemanticTypeId,
    fields: BTreeSet<usize>,
    escaped: bool,
    functions: &'a [&'a NativeFunction],
    calls: BTreeSet<usize>,
    call_budget: usize,
}

impl Analysis<'_> {
    /// Follow ordinary source accessors without recognizing library names.
    /// Unknown targets, recursion and exhausted analysis budgets fail closed.
    fn call(&mut self, target: usize, args: &[NativeExpr], origins: &mut Vec<Origin>) -> Origin {
        let mut arguments = self.expressions(args, origins);
        if !arguments.contains(&Origin::Aggregate) {
            return Origin::Other;
        }
        let Some(function) = self.functions.get(target).copied() else {
            self.escaped = true;
            return Origin::Other;
        };
        if self.call_budget == 0
            || !function.callable_captures.is_empty()
            || function.params.len() != arguments.len()
            || function.params.iter().zip(&arguments).any(|(ty, origin)| {
                *origin == Origin::Aggregate
                    && *ty != NativeType::ManagedRef(self.aggregate_semantic)
            })
            || !self.calls.insert(target)
        {
            self.escaped = true;
            return Origin::Other;
        }
        self.call_budget -= 1;
        let result = self.expr(&function.body, &mut arguments);
        self.calls.remove(&target);
        result
    }

    fn expr(&mut self, expr: &NativeExpr, origins: &mut Vec<Origin>) -> Origin {
        match expr {
            NativeExpr::Param(index) => origins.get(*index).copied().unwrap_or_else(|| {
                self.escaped = true;
                Origin::Other
            }),
            NativeExpr::ManagedOperation { encoded, args } => {
                let argument_origins = self.expressions(args, origins);
                if let (Some((semantic, field)), [Origin::Aggregate]) = (
                    decode_aggregate_field_projection(encoded),
                    argument_origins.as_slice(),
                ) {
                    if semantic == self.aggregate_semantic {
                        self.fields.insert(field);
                        return Origin::Other;
                    }
                }
                self.reject_aggregate_use(&argument_origins);
                Origin::Other
            }
            NativeExpr::Let { bindings, body } => {
                let original_len = origins.len();
                for binding in bindings {
                    let origin = self.expr(binding, origins);
                    origins.push(origin);
                }
                let result = self.expr(body, origins);
                origins.truncate(original_len);
                result
            }
            NativeExpr::If { clauses } => {
                for (condition, body) in clauses {
                    let condition = self.expr(condition, origins);
                    self.reject_aggregate_use(&[condition]);
                    let body = self.expr(body, origins);
                    self.reject_aggregate_use(&[body]);
                }
                Origin::Other
            }
            NativeExpr::Try {
                protected,
                success,
                failure,
                cleanup,
            } => {
                let mut values = vec![
                    self.expr(protected, origins),
                    self.expr(success, origins),
                    self.expr(failure, origins),
                ];
                values.extend(self.expressions(cleanup, origins));
                self.reject_aggregate_use(&values);
                Origin::Other
            }
            NativeExpr::CallThen { args, values, .. } => {
                let mut uses = self.expressions(args, origins);
                uses.extend(self.expressions(values, origins));
                self.reject_aggregate_use(&uses);
                Origin::Other
            }
            NativeExpr::InvokeClosure { callee, args, .. } => {
                let mut uses = vec![self.expr(callee, origins)];
                uses.extend(self.expressions(args, origins));
                self.reject_aggregate_use(&uses);
                Origin::Other
            }
            NativeExpr::InvokeClosureThen {
                callee,
                args,
                values,
                ..
            } => {
                let mut uses = vec![self.expr(callee, origins)];
                uses.extend(self.expressions(args, origins));
                uses.extend(self.expressions(values, origins));
                self.reject_aggregate_use(&uses);
                Origin::Other
            }
            NativeExpr::Call { function, args } | NativeExpr::TailCall { function, args, .. } => {
                self.call(*function, args, origins)
            }
            NativeExpr::Construct { fields, .. }
            | NativeExpr::MakeClosure {
                captures: fields, ..
            }
            | NativeExpr::ContinuationTailCall { args: fields, .. } => {
                let uses = self.expressions(fields, origins);
                self.reject_aggregate_use(&uses);
                Origin::Other
            }
            NativeExpr::Suspend {
                arguments, values, ..
            } => {
                let mut uses = self.expressions(arguments, origins);
                uses.extend(self.expressions(values, origins));
                self.reject_aggregate_use(&uses);
                Origin::Other
            }
            NativeExpr::Neg(value)
            | NativeExpr::FloatNeg(value)
            | NativeExpr::FloatFloor(value)
            | NativeExpr::FloatCeil(value)
            | NativeExpr::IntToFloat(value)
            | NativeExpr::Not(value) => {
                let use_origin = self.expr(value, origins);
                self.reject_aggregate_use(&[use_origin]);
                Origin::Other
            }
            NativeExpr::Binary { left, right, .. } => {
                let uses = [self.expr(left, origins), self.expr(right, origins)];
                self.reject_aggregate_use(&uses);
                Origin::Other
            }
            NativeExpr::Unit
            | NativeExpr::Int(_)
            | NativeExpr::Float(_)
            | NativeExpr::Bool(_)
            | NativeExpr::AtomLiteral(_)
            | NativeExpr::ManagedLiteral { .. } => Origin::Other,
        }
    }

    fn expressions(&mut self, values: &[NativeExpr], origins: &mut Vec<Origin>) -> Vec<Origin> {
        values
            .iter()
            .map(|value| self.expr(value, origins))
            .collect()
    }

    fn reject_aggregate_use(&mut self, uses: &[Origin]) {
        self.escaped |= uses.contains(&Origin::Aggregate);
    }
}
