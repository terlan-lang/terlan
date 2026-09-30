//! Managed HTTP aggregate layouts owned by direct AOT lowering.

use std::sync::Arc;

use crate::runtime::native_image::managed::{
    encode_aggregate_layout, ManagedAggregateDescriptor, ManagedFieldType, SemanticTypeId,
};

use super::{LOOKUP_RESULT, STRING_OPTION};

/// Builds one checked semantic identity used by nested HTTP fields.
pub(super) fn semantic(canonical: &str) -> Result<SemanticTypeId, String> {
    SemanticTypeId::from_canonical(canonical)
        .map_err(|error| format!("error[native_ir.http_type]: {error}"))
}

/// Builds the two active layouts of the managed `Option[String]` union.
pub(super) fn option_string_layouts() -> Result<Vec<Arc<[u8]>>, String> {
    let none = Arc::new(
        ManagedAggregateDescriptor::constructor(STRING_OPTION, "None", 0, 2, Vec::new())
            .map_err(|error| format!("error[native_ir.http_option_layout]: {error}"))?,
    );
    let some = Arc::new(
        ManagedAggregateDescriptor::constructor(
            STRING_OPTION,
            "Some",
            1,
            2,
            vec![(
                Some("value".to_string()),
                ManagedFieldType::Reference(semantic("std.core.String")?),
            )],
        )
        .map_err(|error| format!("error[native_ir.http_option_layout]: {error}"))?,
    );
    Ok(vec![encoded_descriptor(&none)?, encoded_descriptor(&some)?])
}

/// Describes the atomic identity/creation result, not the source Session value.
pub(super) fn lookup_descriptor() -> Result<Arc<ManagedAggregateDescriptor>, String> {
    ManagedAggregateDescriptor::tuple(
        LOOKUP_RESULT,
        vec![
            ManagedFieldType::Reference(semantic("std.core.String")?),
            ManagedFieldType::Bool,
        ],
    )
    .map(Arc::new)
    .map_err(|error| format!("error[native_ir.http_lookup_layout]: {error}"))
}

/// Encodes one target-owned HTTP aggregate descriptor.
pub(super) fn encoded_descriptor(
    descriptor: &Arc<ManagedAggregateDescriptor>,
) -> Result<Arc<[u8]>, String> {
    encode_aggregate_layout(descriptor)
        .map(Arc::from)
        .map_err(|error| format!("error[native_ir.http_layout_abi]: {error}"))
}
