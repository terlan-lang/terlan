//! Managed HTTP aggregate layouts owned by direct AOT lowering.

use std::sync::Arc;

use crate::runtime::native_image::managed::{
    encode_aggregate_layout, ManagedAggregateDescriptor, ManagedFieldType, SemanticTypeId,
};
use crate::terlan_typeck::{CoreModule, CoreType};

use super::{
    COOKIE_JAR, COOKIE_MUTATIONS, REQUEST_STRING_MAP, RESPONSE_HEADER, RESPONSE_HEADERS,
    STRING_OPTION,
};

/// Builds one checked semantic identity used by nested HTTP fields.
pub(super) fn semantic(canonical: &str) -> Result<SemanticTypeId, String> {
    SemanticTypeId::from_canonical(canonical)
        .map_err(|error| format!("error[native_ir.http_type]: {error}"))
}

/// Reports whether one checked module imports an HTTP value contract.
pub(super) fn imports(core: &CoreModule, module: &str) -> bool {
    core.imports.iter().any(|import| import.module == module)
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
                None,
                ManagedFieldType::Reference(semantic("std.core.String")?),
            )],
        )
        .map_err(|error| format!("error[native_ir.http_option_layout]: {error}"))?,
    );
    Ok(vec![encoded_descriptor(&none)?, encoded_descriptor(&some)?])
}

/// Builds the fixed request tuple descriptor accepted by the HTTP boundary.
pub(super) fn request_descriptor() -> Result<Arc<ManagedAggregateDescriptor>, String> {
    let string = ManagedFieldType::Reference(semantic("std.core.String")?);
    let map = ManagedFieldType::Reference(semantic(REQUEST_STRING_MAP)?);
    ManagedAggregateDescriptor::tuple(
        &CoreType::Named("Request".to_string()).contract_text(),
        vec![
            ManagedFieldType::Int,
            string,
            string,
            map,
            string,
            string,
            map,
            map,
            map,
            ManagedFieldType::Reference(semantic(COOKIE_JAR)?),
        ],
    )
    .map(Arc::new)
    .map_err(|error| format!("error[native_ir.http_request_layout]: {error}"))
}

/// Builds the request-scoped cookie jar descriptor with persistent mutations.
pub(super) fn cookie_jar_descriptor() -> Result<Arc<ManagedAggregateDescriptor>, String> {
    ManagedAggregateDescriptor::tuple(
        COOKIE_JAR,
        vec![
            ManagedFieldType::Reference(semantic(REQUEST_STRING_MAP)?),
            ManagedFieldType::Reference(semantic(COOKIE_MUTATIONS)?),
        ],
    )
    .map(Arc::new)
    .map_err(|error| format!("error[native_ir.http_cookie_jar_layout]: {error}"))
}

/// Builds the opaque managed session identity and pending-cookie descriptor.
pub(super) fn session_descriptor() -> Result<Arc<ManagedAggregateDescriptor>, String> {
    let string = ManagedFieldType::Reference(semantic("std.core.String")?);
    ManagedAggregateDescriptor::tuple("Named(Session)", vec![string, string])
        .map(Arc::new)
        .map_err(|error| format!("error[native_ir.http_session_layout]: {error}"))
}

/// Builds the uniform response tuple descriptor emitted by native handlers.
pub(super) fn response_descriptor() -> Result<Arc<ManagedAggregateDescriptor>, String> {
    ManagedAggregateDescriptor::tuple(
        &CoreType::Named("Response".to_string()).contract_text(),
        vec![
            ManagedFieldType::Int,
            ManagedFieldType::Int,
            ManagedFieldType::Reference(semantic("std.core.String")?),
            ManagedFieldType::Int,
            ManagedFieldType::Reference(semantic("std.core.String")?),
            ManagedFieldType::Reference(semantic(RESPONSE_HEADERS)?),
        ],
    )
    .map(Arc::new)
    .map_err(|error| format!("error[native_ir.http_response_layout]: {error}"))
}

/// Builds one immutable response-header pair descriptor.
pub(super) fn response_header_descriptor() -> Result<Arc<ManagedAggregateDescriptor>, String> {
    let string = ManagedFieldType::Reference(semantic("std.core.String")?);
    ManagedAggregateDescriptor::tuple(RESPONSE_HEADER, vec![string, string])
        .map(Arc::new)
        .map_err(|error| format!("error[native_ir.http_response_header_layout]: {error}"))
}

/// Builds the typed HTTP error record accepted by router recovery callbacks.
pub(super) fn http_error_descriptor() -> Result<Arc<ManagedAggregateDescriptor>, String> {
    ManagedAggregateDescriptor::record(
        &CoreType::Named("HttpError".to_string()).contract_text(),
        vec![
            ("code".to_string(), ManagedFieldType::Atom),
            (
                "message".to_string(),
                ManagedFieldType::Reference(semantic("std.core.String")?),
            ),
            ("status".to_string(), ManagedFieldType::Int),
        ],
    )
    .map(Arc::new)
    .map_err(|error| format!("error[native_ir.http_error_layout]: {error}"))
}

/// Encodes one target-owned HTTP aggregate descriptor.
pub(super) fn encoded_descriptor(
    descriptor: &Arc<ManagedAggregateDescriptor>,
) -> Result<Arc<[u8]>, String> {
    encode_aggregate_layout(descriptor)
        .map(Arc::from)
        .map_err(|error| format!("error[native_ir.http_layout_abi]: {error}"))
}
