//! Public direct-native consumer for admitted self-hosted compiler ABI-1.

use crate::self_host_ir::BackendEnvelope;

/// Failure while admitting self-hosted metadata or emitting a native application.
#[derive(Debug)]
pub struct NativeBackendError(terlan_runtime_abi::BoundaryError);

impl std::fmt::Display for NativeBackendError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

impl std::error::Error for NativeBackendError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.source()
    }
}

impl std::ops::Deref for NativeBackendError {
    type Target = str;
    fn deref(&self) -> &str {
        self.0.context()
    }
}

impl From<String> for NativeBackendError {
    fn from(message: String) -> Self {
        Self(terlan_runtime_abi::BoundaryError::message(
            terlan_runtime_abi::ErrorDomain::NativeIrEmission,
            "emit self-hosted native application",
            message,
        ))
    }
}

impl From<&str> for NativeBackendError {
    fn from(message: &str) -> Self {
        message.to_string().into()
    }
}

impl From<NativeBackendError> for String {
    fn from(error: NativeBackendError) -> Self {
        error.to_string()
    }
}

#[cfg(feature = "native-codegen")]
fn validate_envelopes(envelopes: &[BackendEnvelope]) -> Result<(), NativeBackendError> {
    for envelope in envelopes {
        let diagnostics = crate::self_host_ir::validate(envelope);
        if !diagnostics.is_empty() {
            return Err((format!(
                "error[self-host.native-adapter.admission]: module `{}` failed ABI validation: {diagnostics:?}",
                envelope.module.name
            )).into());
        }
    }
    Ok(())
}

#[cfg(feature = "native-codegen")]
fn decode_actor_plan(
    envelope: &BackendEnvelope,
    actor_ir: &str,
) -> Result<crate::self_host_actor_ir::RuntimeActorPlan, NativeBackendError> {
    let document = crate::self_host_actor_ir::decode(actor_ir).map_err(|diagnostics| {
        format!(
            "error[self-host.native-adapter.actor-admission]: module `{}` failed actor IR validation: {diagnostics:?}",
            envelope.module.name
        )
    })?;
    for profile in &document.profiles {
        let function = envelope.module.functions.iter().find(|function| {
            function.module_name == profile.module_name
                && function.name == profile.callable_name
                && function.arity == profile.arity
        });
        let Some(function) = function else {
            return Err((format!(
                "error[self-host.native-adapter.actor-profile-owner]: profile `{}.{}/{}` is outside Core IR module `{}`",
                profile.module_name,
                profile.callable_name,
                profile.arity,
                envelope.module.name
            )).into());
        };
        if profile.runtime_owned != function.native_symbol.is_some() {
            return Err((format!(
                "error[self-host.native-adapter.actor-profile-provenance]: profile `{}.{}/{}` runtime ownership contradicts Core native-symbol provenance",
                profile.module_name, profile.callable_name, profile.arity
            )).into());
        }
    }
    crate::self_host_actor_ir::materialize(document).map_err(|diagnostics| {
        format!(
            "error[self-host.native-adapter.actor-materialization]: module `{}` failed actor IR materialization: {diagnostics:?}",
            envelope.module.name
        )
    }).map_err(NativeBackendError::from)
}

#[cfg(feature = "native-codegen")]
fn decode_typed_application(
    envelopes: &[BackendEnvelope],
    variant_irs: &[String],
    actor_irs: &[String],
) -> Result<
    (
        Vec<crate::self_host_actor_ir::RuntimeActorPlan>,
        crate::self_host_variant_ir::VariantCatalog,
    ),
    NativeBackendError,
> {
    if envelopes.len() != variant_irs.len() || envelopes.len() != actor_irs.len() {
        return Err((format!(
            "error[self-host.native-adapter.typed-application]: {} modules require matching variant and actor sidecars; found {} variant and {} actor sidecars",
            envelopes.len(),
            variant_irs.len(),
            actor_irs.len()
        )).into());
    }
    let catalogs = envelopes
        .iter()
        .zip(variant_irs)
        .map(|(envelope, variant_ir)| {
            crate::self_host_variant_ir::decode_catalog(variant_ir).map_err(|diagnostics| {
                format!(
                    "error[self-host.native-adapter.variant-admission]: module `{}` failed variant IR validation: {diagnostics:?}",
                    envelope.module.name
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let variants = crate::self_host_variant_ir::VariantCatalog::merge(catalogs).map_err(
        |diagnostics| {
            format!(
                "error[self-host.native-adapter.variant-merge]: application variant authority conflicts: {diagnostics:?}"
            )
        },
    )?;
    let plans = envelopes
        .iter()
        .zip(actor_irs)
        .map(|(envelope, actor_ir)| decode_actor_plan(envelope, actor_ir))
        .collect::<Result<Vec<_>, _>>()?;
    Ok((plans, variants))
}

/// Emits one relocatable Cranelift object without returning to the Rust frontend.
#[cfg(feature = "native-codegen")]
pub fn emit_object(envelope: &BackendEnvelope) -> Result<Vec<u8>, NativeBackendError> {
    validate_envelopes(std::slice::from_ref(envelope))?;
    crate::compiler::native_ir::emit_self_hosted_envelope_object(envelope)
        .map_err(NativeBackendError::from)
}

/// Emits one actor-aware object after validating both Core IR and actor metadata.
#[cfg(feature = "native-codegen")]
pub fn emit_actor_object(
    envelope: &BackendEnvelope,
    actor_ir: &str,
) -> Result<Vec<u8>, NativeBackendError> {
    validate_envelopes(std::slice::from_ref(envelope))?;
    let plan = decode_actor_plan(envelope, actor_ir)?;
    crate::compiler::native_ir::emit_self_hosted_actor_envelope_object(envelope, plan)
        .map_err(NativeBackendError::from)
}

/// Emits one actor-aware object after admitting finite-sum and actor sidecars.
#[cfg(feature = "native-codegen")]
pub fn emit_typed_actor_object(
    envelope: &BackendEnvelope,
    variant_ir: &str,
    actor_ir: &str,
) -> Result<Vec<u8>, NativeBackendError> {
    validate_envelopes(std::slice::from_ref(envelope))?;
    let variants = crate::self_host_variant_ir::decode_catalog(variant_ir).map_err(|diagnostics| {
        format!(
            "error[self-host.native-adapter.variant-admission]: module `{}` failed variant IR validation: {diagnostics:?}",
            envelope.module.name
        )
    })?;
    let plan = decode_actor_plan(envelope, actor_ir)?;
    crate::compiler::native_ir::emit_self_hosted_typed_actor_envelope_object(
        envelope, plan, variants,
    )
    .map_err(NativeBackendError::from)
}

/// Emits one application object from an ordered, structurally admitted module closure.
#[cfg(feature = "native-codegen")]
pub fn emit_application(envelopes: &[BackendEnvelope]) -> Result<Vec<u8>, NativeBackendError> {
    validate_envelopes(envelopes)?;
    crate::compiler::native_ir::emit_self_hosted_application_object(envelopes)
        .map_err(NativeBackendError::from)
}

/// Emits one application object after admitting every module's typed sidecars.
#[cfg(feature = "native-codegen")]
pub fn emit_typed_actor_application(
    envelopes: &[BackendEnvelope],
    variant_irs: &[String],
    actor_irs: &[String],
) -> Result<Vec<u8>, NativeBackendError> {
    validate_envelopes(envelopes)?;
    let (plans, variants) = decode_typed_application(envelopes, variant_irs, actor_irs)?;
    crate::compiler::native_ir::emit_self_hosted_typed_actor_application_object(
        envelopes, plans, variants,
    )
    .map_err(NativeBackendError::from)
}

/// Publishes one ordinary sealed TVM image from an ordered ABI-1 module closure.
#[cfg(feature = "native-codegen")]
pub fn publish_application_image(
    envelopes: &[BackendEnvelope],
    output: &std::path::Path,
) -> Result<(), NativeBackendError> {
    validate_envelopes(envelopes)?;
    crate::commands::build::vm_artifact::native_image::publish_self_hosted_application_image(
        output, envelopes,
    )
    .map_err(NativeBackendError::from)
}

/// Publishes one sealed image after admitting every module's typed sidecars.
#[cfg(feature = "native-codegen")]
pub fn publish_typed_actor_application_image(
    envelopes: &[BackendEnvelope],
    variant_irs: &[String],
    actor_irs: &[String],
    output: &std::path::Path,
) -> Result<(), NativeBackendError> {
    validate_envelopes(envelopes)?;
    let (plans, variants) = decode_typed_application(envelopes, variant_irs, actor_irs)?;
    crate::commands::build::vm_artifact::native_image::publish_self_hosted_typed_actor_application_image(
        output,
        envelopes,
        plans,
        variants,
    ).map_err(NativeBackendError::from)
}

/// Reports that this crate was built without the native code-generation feature.
#[cfg(not(feature = "native-codegen"))]
pub fn emit_object(_envelope: &BackendEnvelope) -> Result<Vec<u8>, NativeBackendError> {
    Err(("error[self-host.native-adapter.feature]: native-codegen is disabled".to_string()).into())
}

#[cfg(not(feature = "native-codegen"))]
pub fn emit_actor_object(
    _envelope: &BackendEnvelope,
    _actor_ir: &str,
) -> Result<Vec<u8>, NativeBackendError> {
    Err(("error[self-host.native-adapter.feature]: native-codegen is disabled".to_string()).into())
}

#[cfg(not(feature = "native-codegen"))]
pub fn emit_typed_actor_object(
    _envelope: &BackendEnvelope,
    _variant_ir: &str,
    _actor_ir: &str,
) -> Result<Vec<u8>, NativeBackendError> {
    Err(("error[self-host.native-adapter.feature]: native-codegen is disabled".to_string()).into())
}

#[cfg(not(feature = "native-codegen"))]
pub fn emit_application(_envelopes: &[BackendEnvelope]) -> Result<Vec<u8>, NativeBackendError> {
    Err(("error[self-host.native-adapter.feature]: native-codegen is disabled".to_string()).into())
}

#[cfg(not(feature = "native-codegen"))]
pub fn emit_typed_actor_application(
    _envelopes: &[BackendEnvelope],
    _variant_irs: &[String],
    _actor_irs: &[String],
) -> Result<Vec<u8>, NativeBackendError> {
    Err(("error[self-host.native-adapter.feature]: native-codegen is disabled".to_string()).into())
}

#[cfg(not(feature = "native-codegen"))]
pub fn publish_application_image(
    _envelopes: &[BackendEnvelope],
    _output: &std::path::Path,
) -> Result<(), NativeBackendError> {
    Err(("error[self-host.native-adapter.feature]: native-codegen is disabled".to_string()).into())
}

#[cfg(not(feature = "native-codegen"))]
pub fn publish_typed_actor_application_image(
    _envelopes: &[BackendEnvelope],
    _variant_irs: &[String],
    _actor_irs: &[String],
    _output: &std::path::Path,
) -> Result<(), NativeBackendError> {
    Err(("error[self-host.native-adapter.feature]: native-codegen is disabled".to_string()).into())
}
