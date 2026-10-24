//! Public direct-native consumer for admitted self-hosted compiler ABI-1.

use crate::self_host_ir::BackendEnvelope;

#[cfg(feature = "native-codegen")]
fn validate_envelopes(envelopes: &[BackendEnvelope]) -> Result<(), String> {
    for envelope in envelopes {
        let diagnostics = crate::self_host_ir::validate(envelope);
        if !diagnostics.is_empty() {
            return Err(format!(
                "error[self-host.native-adapter.admission]: module `{}` failed ABI validation: {diagnostics:?}",
                envelope.module.name
            ));
        }
    }
    Ok(())
}

#[cfg(feature = "native-codegen")]
fn decode_actor_plan(
    envelope: &BackendEnvelope,
    actor_ir: &str,
) -> Result<crate::self_host_actor_ir::RuntimeActorPlan, String> {
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
            return Err(format!(
                "error[self-host.native-adapter.actor-profile-owner]: profile `{}.{}/{}` is outside Core IR module `{}`",
                profile.module_name,
                profile.callable_name,
                profile.arity,
                envelope.module.name
            ));
        };
        if profile.runtime_owned != function.native_symbol.is_some() {
            return Err(format!(
                "error[self-host.native-adapter.actor-profile-provenance]: profile `{}.{}/{}` runtime ownership contradicts Core native-symbol provenance",
                profile.module_name, profile.callable_name, profile.arity
            ));
        }
    }
    crate::self_host_actor_ir::materialize(document).map_err(|diagnostics| {
        format!(
            "error[self-host.native-adapter.actor-materialization]: module `{}` failed actor IR materialization: {diagnostics:?}",
            envelope.module.name
        )
    })
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
    String,
> {
    if envelopes.len() != variant_irs.len() || envelopes.len() != actor_irs.len() {
        return Err(format!(
            "error[self-host.native-adapter.typed-application]: {} modules require matching variant and actor sidecars; found {} variant and {} actor sidecars",
            envelopes.len(),
            variant_irs.len(),
            actor_irs.len()
        ));
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
pub fn emit_object(envelope: &BackendEnvelope) -> Result<Vec<u8>, String> {
    validate_envelopes(std::slice::from_ref(envelope))?;
    crate::compiler::native_ir::emit_self_hosted_envelope_object(envelope)
}

/// Emits one actor-aware object after validating both Core IR and actor metadata.
#[cfg(feature = "native-codegen")]
pub fn emit_actor_object(envelope: &BackendEnvelope, actor_ir: &str) -> Result<Vec<u8>, String> {
    validate_envelopes(std::slice::from_ref(envelope))?;
    let plan = decode_actor_plan(envelope, actor_ir)?;
    crate::compiler::native_ir::emit_self_hosted_actor_envelope_object(envelope, plan)
}

/// Emits one actor-aware object after admitting finite-sum and actor sidecars.
#[cfg(feature = "native-codegen")]
pub fn emit_typed_actor_object(
    envelope: &BackendEnvelope,
    variant_ir: &str,
    actor_ir: &str,
) -> Result<Vec<u8>, String> {
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
}

/// Emits one application object from an ordered, structurally admitted module closure.
#[cfg(feature = "native-codegen")]
pub fn emit_application(envelopes: &[BackendEnvelope]) -> Result<Vec<u8>, String> {
    validate_envelopes(envelopes)?;
    crate::compiler::native_ir::emit_self_hosted_application_object(envelopes)
}

/// Emits one application object after admitting every module's typed sidecars.
#[cfg(feature = "native-codegen")]
pub fn emit_typed_actor_application(
    envelopes: &[BackendEnvelope],
    variant_irs: &[String],
    actor_irs: &[String],
) -> Result<Vec<u8>, String> {
    validate_envelopes(envelopes)?;
    let (plans, variants) = decode_typed_application(envelopes, variant_irs, actor_irs)?;
    crate::compiler::native_ir::emit_self_hosted_typed_actor_application_object(
        envelopes, plans, variants,
    )
}

/// Publishes one ordinary sealed TVM image from an ordered ABI-1 module closure.
#[cfg(feature = "native-codegen")]
pub fn publish_application_image(
    envelopes: &[BackendEnvelope],
    output: &std::path::Path,
) -> Result<(), String> {
    validate_envelopes(envelopes)?;
    crate::commands::build::vm_artifact::native_image::publish_self_hosted_application_image(
        output, envelopes,
    )
}

/// Publishes one sealed image after admitting every module's typed sidecars.
#[cfg(feature = "native-codegen")]
pub fn publish_typed_actor_application_image(
    envelopes: &[BackendEnvelope],
    variant_irs: &[String],
    actor_irs: &[String],
    output: &std::path::Path,
) -> Result<(), String> {
    validate_envelopes(envelopes)?;
    let (plans, variants) = decode_typed_application(envelopes, variant_irs, actor_irs)?;
    crate::commands::build::vm_artifact::native_image::publish_self_hosted_typed_actor_application_image(
        output,
        envelopes,
        plans,
        variants,
    )
}

/// Reports that this crate was built without the native code-generation feature.
#[cfg(not(feature = "native-codegen"))]
pub fn emit_object(_envelope: &BackendEnvelope) -> Result<Vec<u8>, String> {
    Err("error[self-host.native-adapter.feature]: native-codegen is disabled".to_string())
}

#[cfg(not(feature = "native-codegen"))]
pub fn emit_actor_object(_envelope: &BackendEnvelope, _actor_ir: &str) -> Result<Vec<u8>, String> {
    Err("error[self-host.native-adapter.feature]: native-codegen is disabled".to_string())
}

#[cfg(not(feature = "native-codegen"))]
pub fn emit_typed_actor_object(
    _envelope: &BackendEnvelope,
    _variant_ir: &str,
    _actor_ir: &str,
) -> Result<Vec<u8>, String> {
    Err("error[self-host.native-adapter.feature]: native-codegen is disabled".to_string())
}

#[cfg(not(feature = "native-codegen"))]
pub fn emit_application(_envelopes: &[BackendEnvelope]) -> Result<Vec<u8>, String> {
    Err("error[self-host.native-adapter.feature]: native-codegen is disabled".to_string())
}

#[cfg(not(feature = "native-codegen"))]
pub fn emit_typed_actor_application(
    _envelopes: &[BackendEnvelope],
    _variant_irs: &[String],
    _actor_irs: &[String],
) -> Result<Vec<u8>, String> {
    Err("error[self-host.native-adapter.feature]: native-codegen is disabled".to_string())
}

#[cfg(not(feature = "native-codegen"))]
pub fn publish_application_image(
    _envelopes: &[BackendEnvelope],
    _output: &std::path::Path,
) -> Result<(), String> {
    Err("error[self-host.native-adapter.feature]: native-codegen is disabled".to_string())
}

#[cfg(not(feature = "native-codegen"))]
pub fn publish_typed_actor_application_image(
    _envelopes: &[BackendEnvelope],
    _variant_irs: &[String],
    _actor_irs: &[String],
    _output: &std::path::Path,
) -> Result<(), String> {
    Err("error[self-host.native-adapter.feature]: native-codegen is disabled".to_string())
}
