//! Compiler-owned constructor layouts for managed HTTP values.

use std::sync::Arc;

use crate::runtime::native_image::managed::{
    encode_aggregate_layout, ManagedAggregateDescriptor, ManagedFieldType,
};
use crate::terlan_typeck::{CoreModule, CoreType};

use super::super::constructors::{NativeConstructorLayout, NativeConstructorLayouts};
use super::super::NativeType;
use super::error::ERROR_CONSTRUCTOR;
use super::layout::{http_error_descriptor, imports, response_descriptor, semantic};
use super::security::{security_headers_descriptor, SECURITY_CONSTRUCTOR};
use super::{
    response_constructor, ERROR_MODULE, MIDDLEWARE_RESULT, RESPONSE_HEADERS, RESPONSE_MODULE,
    ROUTER_MODULE,
};

/// One compiler-private response constructor and its physical parameter shape.
type ResponseLayoutSpec = (
    &'static str,
    Arc<ManagedAggregateDescriptor>,
    Vec<NativeType>,
);

/// Installs target-owned constructors that are intentionally absent from source CoreIR.
pub(crate) fn install_http_constructors(
    core: &CoreModule,
    layouts: &mut NativeConstructorLayouts,
) -> Result<(), String> {
    let router = imports(core, ROUTER_MODULE);
    let response = router || imports(core, RESPONSE_MODULE);
    let error = router || imports(core, ERROR_MODULE);
    if !response && !router && !error {
        return Ok(());
    }
    if response {
        install_response_constructors(layouts)?;
        install_security_constructor(layouts)?;
    }
    if router {
        install_middleware_result_constructors(layouts)?;
    }
    if error {
        install_error_constructor(layouts)?;
    }
    Ok(())
}

/// Installs the portable typed HTTP error constructor.
fn install_error_constructor(layouts: &mut NativeConstructorLayouts) -> Result<(), String> {
    let descriptor = http_error_descriptor()?;
    layouts.insert(
        (ERROR_CONSTRUCTOR.to_string(), 3),
        NativeConstructorLayout {
            parameters: vec![NativeType::Atom, NativeType::StringRef, NativeType::Int],
            result: NativeType::ManagedRef(descriptor.managed().semantic_id()),
            encoded_layout: Arc::from(
                encode_aggregate_layout(&descriptor)
                    .map_err(|error| format!("error[native_ir.http_error_abi]: {error}"))?,
            ),
            descriptor,
        },
    );
    Ok(())
}

/// Installs each closed response builder over the shared managed tuple layout.
fn install_response_constructors(layouts: &mut NativeConstructorLayouts) -> Result<(), String> {
    for (name, descriptor, parameters) in response_layout_specs()? {
        let encoded_layout = Arc::<[u8]>::from(
            encode_aggregate_layout(&descriptor)
                .map_err(|error| format!("error[native_ir.http_response_abi]: {error}"))?,
        );
        let result = NativeType::ManagedRef(descriptor.managed().semantic_id());
        let arity = parameters.len();
        layouts.insert(
            (response_constructor(name), arity),
            NativeConstructorLayout {
                parameters,
                result,
                descriptor,
                encoded_layout,
            },
        );
    }
    Ok(())
}

/// Installs the fixed typed HTTP security-policy constructor.
fn install_security_constructor(layouts: &mut NativeConstructorLayouts) -> Result<(), String> {
    let descriptor = security_headers_descriptor()?;
    layouts.insert(
        (SECURITY_CONSTRUCTOR.to_string(), 5),
        NativeConstructorLayout {
            parameters: vec![
                NativeType::Bool,
                NativeType::Int,
                NativeType::Int,
                NativeType::Int,
                NativeType::Bool,
            ],
            result: NativeType::ManagedRef(descriptor.managed().semantic_id()),
            encoded_layout: Arc::from(
                encode_aggregate_layout(&descriptor)
                    .map_err(|error| format!("error[native_ir.http_security_abi]: {error}"))?,
            ),
            descriptor,
        },
    );
    Ok(())
}

/// Installs the two closed variants returned by request middleware callbacks.
fn install_middleware_result_constructors(
    layouts: &mut NativeConstructorLayouts,
) -> Result<(), String> {
    let variants = [
        (
            "Continue",
            Arc::new(
                ManagedAggregateDescriptor::constructor(
                    MIDDLEWARE_RESULT,
                    "Continue",
                    0,
                    2,
                    Vec::new(),
                )
                .map_err(|error| format!("error[native_ir.http_middleware_abi]: {error}"))?,
            ),
            Vec::new(),
        ),
        (
            "Respond",
            Arc::new(
                ManagedAggregateDescriptor::constructor(
                    MIDDLEWARE_RESULT,
                    "Respond",
                    1,
                    2,
                    vec![(
                        Some("response".to_string()),
                        ManagedFieldType::Reference(response_semantic()?),
                    )],
                )
                .map_err(|error| format!("error[native_ir.http_middleware_abi]: {error}"))?,
            ),
            vec![NativeType::ManagedRef(response_semantic()?)],
        ),
    ];
    for (variant, descriptor, parameters) in variants {
        let identity = format!("{ROUTER_MODULE}.{variant}");
        layouts.insert(
            (identity, parameters.len()),
            NativeConstructorLayout {
                parameters,
                result: NativeType::ManagedRef(descriptor.managed().semantic_id()),
                encoded_layout: Arc::from(
                    encode_aggregate_layout(&descriptor).map_err(|error| {
                        format!("error[native_ir.http_middleware_abi]: {error}")
                    })?,
                ),
                descriptor,
            },
        );
    }
    Ok(())
}

/// Resolves the canonical managed response semantic identity.
fn response_semantic() -> Result<crate::runtime::native_image::managed::SemanticTypeId, String> {
    semantic(&CoreType::Named("Response".to_string()).contract_text())
}

/// Builds the closed response constructor inventory admitted in this slice.
fn response_layout_specs() -> Result<Vec<ResponseLayoutSpec>, String> {
    let headers = NativeType::ManagedRef(semantic(RESPONSE_HEADERS)?);
    let common = || {
        vec![
            NativeType::Int,
            NativeType::Int,
            NativeType::StringRef,
            NativeType::Int,
            NativeType::StringRef,
            headers,
        ]
    };
    Ok(vec![
        ("text", response_descriptor()?, common()),
        ("html", response_descriptor()?, common()),
        ("json_text", response_descriptor()?, common()),
        ("redirect", response_descriptor()?, common()),
        ("file", response_descriptor()?, common()),
    ])
}
