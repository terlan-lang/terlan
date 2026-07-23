//! Fail-closed Request field-use analysis over typed NativeIR.

use crate::runtime::native::http::RequestFieldProjection;
use crate::runtime::native_image::managed::{decode_aggregate_field_projection, SemanticTypeId};
use crate::terlan_typeck::CoreType;

use super::{NativeExpr, NativeFunction, NativeModule, NativeType};

/// One export-specific Request projection carried beside a compiled image.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NativeRequestProjection {
    pub(crate) module: String,
    pub(crate) function: String,
    pub(crate) arity: usize,
    pub(crate) fields: RequestFieldProjection,
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
            module.functions.iter().filter_map(move |function| {
                analyze_function(function, request_semantic).map(|fields| NativeRequestProjection {
                    module: module.name.clone(),
                    function: function.name.clone(),
                    arity: function.arity,
                    fields,
                })
            })
        })
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
            NativeExpr::CallThen {
                args,
                values,
                resume,
                ..
            } => {
                let mut uses = self.expressions(args, origins);
                uses.extend(self.expressions(values, origins));
                uses.push(self.expr(resume, origins));
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
            | NativeExpr::TailCall { args: fields, .. } => {
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
