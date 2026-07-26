//! Fail-closed Request field-use analysis over typed NativeIR.

use crate::runtime::native::http::RequestFieldProjection;
use crate::runtime::native_image::managed::{
    decode_aggregate_field_projection, scalar_string_projection_rewrite, SemanticTypeId,
};
use crate::terlan_typeck::CoreType;

use super::{NativeExpr, NativeFunction, NativeModule, NativeType};

const SCALAR_REQUEST_INGRESS_PREFIX: &str = "__terlan_http_scalar_ingress_";

/// One export-specific Request projection carried beside a compiled image.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub(crate) struct NativeRequestProjection {
    pub(crate) module: String,
    pub(crate) function: String,
    pub(crate) arity: usize,
    pub(crate) fields: RequestFieldProjection,
    /// Compiler-generated entry accepting the sole observed scalar field.
    #[serde(default)]
    pub(crate) scalar_entry: Option<String>,
    /// Exact Request field accepted by `scalar_entry`.
    #[serde(default)]
    pub(crate) scalar_field: Option<usize>,
    /// Fixed-point proof that this source export can park its actor.
    #[serde(default)]
    pub(crate) suspending: bool,
}

/// Computes Request projections for public exports whose first ordinary
/// parameter is the opaque `std.http.Request` managed value.
pub(crate) fn native_request_projections(modules: &[NativeModule]) -> Vec<NativeRequestProjection> {
    let Some(request_semantic) = request_semantic() else {
        return Vec::new();
    };
    modules
        .iter()
        .flat_map(|module| {
            let (suspending, _) = super::cranelift::native_suspension_profile(module);
            module
                .functions
                .iter()
                .enumerate()
                .filter_map(move |(index, function)| {
                    analyze_function(function, request_semantic).map(|fields| {
                        NativeRequestProjection {
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

/// Installs private-shape HTTP ingress exports after ordinary application
/// lowering, preserving every source-visible function and internal call index.
///
/// Live-serve images currently contain exactly one handler module. Appending
/// the generated entry to that module cannot shift an existing application
/// function index. Multi-module images retain projection metadata but do not
/// receive the scalar ABI until application-wide index rebasing is available.
pub(crate) fn install_native_request_projection_exports(
    modules: &mut [NativeModule],
) -> Vec<NativeRequestProjection> {
    let mut projections = native_request_projections(modules);
    if modules.len() != 1 {
        return projections;
    }
    let module = &mut modules[0];
    let mut generated = Vec::new();
    for projection in &mut projections {
        let Some(field) = sole_scalar_string_field(projection.fields) else {
            continue;
        };
        if projection.arity != 1 {
            continue;
        }
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
        let Some(body) = rewrite_scalar_request_body(&original.body, request_semantic(), field)
        else {
            continue;
        };
        let entry = format!(
            "{SCALAR_REQUEST_INGRESS_PREFIX}{:016x}_{field}",
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
    module.functions.extend(generated);
    projections
}

fn sole_scalar_string_field(projection: RequestFieldProjection) -> Option<usize> {
    let RequestFieldProjection::Fields(fields) = projection else {
        return None;
    };
    if fields.count_ones() != 1 {
        return None;
    }
    let field = fields.trailing_zeros() as usize;
    matches!(
        field,
        RequestFieldProjection::METHOD
            | RequestFieldProjection::PATH
            | RequestFieldProjection::BODY
            | RequestFieldProjection::QUERY_STRING
    )
    .then_some(field)
}

/// Replaces exact field projections from parameter zero with that parameter.
///
/// Returning `None` is fail-closed: aliases, different Request fields, or any
/// raw use of the original Request retain the complete managed ingress.
fn rewrite_scalar_request_body(
    expression: &NativeExpr,
    request_semantic: Option<SemanticTypeId>,
    field: usize,
) -> Option<NativeExpr> {
    if let NativeExpr::ManagedOperation { encoded, args } = expression {
        if let (Some(request_semantic), Some((semantic, projected, scalar_operation))) =
            (request_semantic, scalar_string_projection_rewrite(encoded))
        {
            if semantic == request_semantic
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
            args: rewrite_expressions(args, request_semantic, field)?,
        },
        NativeExpr::MakeClosure { encoded, captures } => NativeExpr::MakeClosure {
            encoded: encoded.clone(),
            captures: rewrite_expressions(captures, request_semantic, field)?,
        },
        NativeExpr::Construct {
            descriptor,
            encoded_layout,
            fields,
        } => NativeExpr::Construct {
            descriptor: descriptor.clone(),
            encoded_layout: encoded_layout.clone(),
            fields: rewrite_expressions(fields, request_semantic, field)?,
        },
        NativeExpr::Call { function, args } => NativeExpr::Call {
            function: *function,
            args: rewrite_expressions(args, request_semantic, field)?,
        },
        NativeExpr::InvokeClosure {
            callee,
            args,
            parameter_types,
            result_type,
        } => NativeExpr::InvokeClosure {
            callee: Box::new(rewrite_scalar_request_body(
                callee,
                request_semantic,
                field,
            )?),
            args: rewrite_expressions(args, request_semantic, field)?,
            parameter_types: parameter_types.clone(),
            result_type: *result_type,
        },
        NativeExpr::TailCall { function, args } => NativeExpr::TailCall {
            function: *function,
            args: rewrite_expressions(args, request_semantic, field)?,
        },
        NativeExpr::CallThen {
            function,
            args,
            callee_continuation_id,
            callee_capture_count,
            continuation_id,
            completion_continuation_id,
            completion_function,
            values,
        } => NativeExpr::CallThen {
            function: *function,
            args: rewrite_expressions(args, request_semantic, field)?,
            callee_continuation_id: *callee_continuation_id,
            callee_capture_count: *callee_capture_count,
            continuation_id: *continuation_id,
            completion_continuation_id: *completion_continuation_id,
            completion_function: *completion_function,
            values: rewrite_expressions(values, request_semantic, field)?,
        },
        NativeExpr::Neg(value) => NativeExpr::Neg(Box::new(rewrite_scalar_request_body(
            value,
            request_semantic,
            field,
        )?)),
        NativeExpr::FloatNeg(value) => NativeExpr::FloatNeg(Box::new(rewrite_scalar_request_body(
            value,
            request_semantic,
            field,
        )?)),
        NativeExpr::FloatFloor(value) => NativeExpr::FloatFloor(Box::new(
            rewrite_scalar_request_body(value, request_semantic, field)?,
        )),
        NativeExpr::FloatCeil(value) => NativeExpr::FloatCeil(Box::new(
            rewrite_scalar_request_body(value, request_semantic, field)?,
        )),
        NativeExpr::IntToFloat(value) => NativeExpr::IntToFloat(Box::new(
            rewrite_scalar_request_body(value, request_semantic, field)?,
        )),
        NativeExpr::Not(value) => NativeExpr::Not(Box::new(rewrite_scalar_request_body(
            value,
            request_semantic,
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
            left: Box::new(rewrite_scalar_request_body(left, request_semantic, field)?),
            right: Box::new(rewrite_scalar_request_body(right, request_semantic, field)?),
        },
        NativeExpr::Let { bindings, body } => NativeExpr::Let {
            bindings: rewrite_expressions(bindings, request_semantic, field)?,
            body: Box::new(rewrite_scalar_request_body(body, request_semantic, field)?),
        },
        NativeExpr::If { clauses } => NativeExpr::If {
            clauses: clauses
                .iter()
                .map(|(condition, body)| {
                    Some((
                        rewrite_scalar_request_body(condition, request_semantic, field)?,
                        rewrite_scalar_request_body(body, request_semantic, field)?,
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
            protected: Box::new(rewrite_scalar_request_body(
                protected,
                request_semantic,
                field,
            )?),
            success: Box::new(rewrite_scalar_request_body(
                success,
                request_semantic,
                field,
            )?),
            failure: Box::new(rewrite_scalar_request_body(
                failure,
                request_semantic,
                field,
            )?),
            cleanup: rewrite_expressions(cleanup, request_semantic, field)?,
        },
        NativeExpr::Suspend {
            operation,
            arguments,
            continuation_id,
            values,
        } => NativeExpr::Suspend {
            operation: *operation,
            arguments: rewrite_expressions(arguments, request_semantic, field)?,
            continuation_id: *continuation_id,
            values: rewrite_expressions(values, request_semantic, field)?,
        },
        other => other.clone(),
    })
}

fn rewrite_expressions(
    expressions: &[NativeExpr],
    request_semantic: Option<SemanticTypeId>,
    field: usize,
) -> Option<Vec<NativeExpr>> {
    expressions
        .iter()
        .map(|expression| rewrite_scalar_request_body(expression, request_semantic, field))
        .collect()
}

fn request_semantic() -> Option<SemanticTypeId> {
    SemanticTypeId::from_canonical(&CoreType::Named("Request".to_string()).contract_text()).ok()
}

fn analyze_function(
    function: &NativeFunction,
    request_semantic: SemanticTypeId,
) -> Option<RequestFieldProjection> {
    if !function.public
        || !function.callable_captures.is_empty()
        || function.params.first() != Some(&NativeType::ManagedRef(request_semantic))
    {
        return None;
    }
    let mut state = Analysis {
        request_semantic,
        fields: RequestFieldProjection::empty(),
        escaped: false,
    };
    let mut origins = vec![Origin::Request];
    origins.extend(function.params.iter().skip(1).map(|_| Origin::Other));
    if state.expr(&function.body, &mut origins) == Origin::Request {
        state.escaped = true;
    }
    Some(if state.escaped {
        RequestFieldProjection::Complete
    } else {
        state.fields
    })
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Origin {
    Other,
    Request,
}

struct Analysis {
    request_semantic: SemanticTypeId,
    fields: RequestFieldProjection,
    escaped: bool,
}

impl Analysis {
    fn expr(&mut self, expr: &NativeExpr, origins: &mut Vec<Origin>) -> Origin {
        match expr {
            NativeExpr::Param(index) => origins.get(*index).copied().unwrap_or_else(|| {
                self.escaped = true;
                Origin::Other
            }),
            NativeExpr::ManagedOperation { encoded, args } => {
                let argument_origins = self.expressions(args, origins);
                if let (Some((semantic, field)), [Origin::Request]) = (
                    decode_aggregate_field_projection(encoded),
                    argument_origins.as_slice(),
                ) {
                    if semantic == self.request_semantic {
                        if (RequestFieldProjection::METHOD..=RequestFieldProjection::COOKIE_JAR)
                            .contains(&field)
                        {
                            self.fields.include(field);
                        } else {
                            self.escaped = true;
                        }
                        return Origin::Other;
                    }
                }
                self.reject_request_use(&argument_origins);
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
                    self.reject_request_use(&[condition]);
                    let body = self.expr(body, origins);
                    self.reject_request_use(&[body]);
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
                self.reject_request_use(&values);
                Origin::Other
            }
            NativeExpr::CallThen { args, values, .. } => {
                let mut uses = self.expressions(args, origins);
                uses.extend(self.expressions(values, origins));
                self.reject_request_use(&uses);
                Origin::Other
            }
            NativeExpr::InvokeClosure { callee, args, .. } => {
                let mut uses = vec![self.expr(callee, origins)];
                uses.extend(self.expressions(args, origins));
                self.reject_request_use(&uses);
                Origin::Other
            }
            NativeExpr::Construct { fields, .. }
            | NativeExpr::MakeClosure {
                captures: fields, ..
            }
            | NativeExpr::Call { args: fields, .. }
            | NativeExpr::TailCall { args: fields, .. }
            | NativeExpr::ContinuationTailCall { args: fields, .. } => {
                let uses = self.expressions(fields, origins);
                self.reject_request_use(&uses);
                Origin::Other
            }
            NativeExpr::Suspend {
                arguments, values, ..
            } => {
                let mut uses = self.expressions(arguments, origins);
                uses.extend(self.expressions(values, origins));
                self.reject_request_use(&uses);
                Origin::Other
            }
            NativeExpr::Neg(value)
            | NativeExpr::FloatNeg(value)
            | NativeExpr::FloatFloor(value)
            | NativeExpr::FloatCeil(value)
            | NativeExpr::IntToFloat(value)
            | NativeExpr::Not(value) => {
                let use_origin = self.expr(value, origins);
                self.reject_request_use(&[use_origin]);
                Origin::Other
            }
            NativeExpr::Binary { left, right, .. } => {
                let uses = [self.expr(left, origins), self.expr(right, origins)];
                self.reject_request_use(&uses);
                Origin::Other
            }
            NativeExpr::Unit
            | NativeExpr::Int(_)
            | NativeExpr::Float(_)
            | NativeExpr::Bool(_)
            | NativeExpr::AtomLiteral(_)
            | NativeExpr::StringLiteral { .. } => Origin::Other,
        }
    }

    fn expressions(&mut self, values: &[NativeExpr], origins: &mut Vec<Origin>) -> Vec<Origin> {
        values
            .iter()
            .map(|value| self.expr(value, origins))
            .collect()
    }

    fn reject_request_use(&mut self, uses: &[Origin]) {
        self.escaped |= uses.contains(&Origin::Request);
    }
}
