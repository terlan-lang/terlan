//! Adapts domain-independent compiler observations to the HTTP ingress contract.

use crate::compiler::native_ir::{AggregateFieldProjection, NativeAggregateProjection};
use crate::runtime::native::http::RequestFieldProjection;
use crate::runtime::native_image::managed::SemanticTypeId;
use crate::runtime::vm::aot_metadata::NativeRequestProjection;
use crate::terlan_typeck::CoreType;

pub(super) fn request_projections(
    projections: Vec<NativeAggregateProjection>,
) -> Vec<NativeRequestProjection> {
    let Ok(request) =
        SemanticTypeId::from_canonical(&CoreType::Named("Request".into()).contract_text())
    else {
        return Vec::new();
    };
    projections
        .into_iter()
        .filter_map(|projection| {
            if projection.semantic != request {
                return None;
            }
            let fields = match projection.fields {
                AggregateFieldProjection::Complete => RequestFieldProjection::Complete,
                AggregateFieldProjection::Fields(fields) => {
                    let mut result = RequestFieldProjection::empty();
                    for field in fields {
                        if !(RequestFieldProjection::METHOD
                            ..=RequestFieldProjection::BODY_FILE_PATH)
                            .contains(&field)
                        {
                            result = RequestFieldProjection::Complete;
                            break;
                        }
                        result.include(field);
                    }
                    result
                }
            };
            let scalar = projection.scalar_field.is_some_and(|field| {
                matches!(
                    field,
                    RequestFieldProjection::METHOD
                        | RequestFieldProjection::PATH
                        | RequestFieldProjection::BODY
                        | RequestFieldProjection::QUERY_STRING
                        | RequestFieldProjection::BODY_FILE_PATH
                ) && fields == RequestFieldProjection::Fields(1 << field)
                    && projection.scalar_entry.is_some()
            });
            Some(NativeRequestProjection {
                module: projection.module,
                function: projection.function,
                arity: projection.arity,
                fields,
                scalar_entry: scalar.then_some(projection.scalar_entry).flatten(),
                scalar_field: scalar.then_some(projection.scalar_field).flatten(),
                suspending: projection.suspending,
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "http_projection_test.rs"]
mod tests;
