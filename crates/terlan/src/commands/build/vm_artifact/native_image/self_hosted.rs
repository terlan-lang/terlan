use super::*;

/// Publishes an admitted self-hosted ABI closure as a sealed VM native image.
///
/// This deliberately shares the ordinary native-image descriptor, linker,
/// sealing, and admission path. The self-hosted compiler replaces only the
/// source-to-NativeIR producer; it does not define a second executable format.
#[cfg(feature = "native-codegen")]
pub(crate) fn publish_self_hosted_application_image(
    output: &Path,
    envelopes: &[crate::self_host_ir::BackendEnvelope],
) -> Result<(), String> {
    let natives = crate::compiler::native_ir::lower_self_hosted_application(envelopes)?;
    publish_self_hosted_native_modules(output, natives)
}

/// Publishes an actor-aware typed self-hosted closure as a sealed VM native image.
#[cfg(feature = "native-codegen")]
pub(crate) fn publish_self_hosted_typed_actor_application_image(
    output: &Path,
    envelopes: &[crate::self_host_ir::BackendEnvelope],
    plans: Vec<crate::self_host_actor_ir::RuntimeActorPlan>,
    variants: crate::self_host_variant_ir::VariantCatalog,
) -> Result<(), String> {
    let natives = crate::compiler::native_ir::lower_self_hosted_typed_actor_application(
        envelopes, plans, variants,
    )?;
    publish_self_hosted_native_modules(output, natives)
}

#[cfg(feature = "native-codegen")]
fn publish_self_hosted_native_modules(
    output: &Path,
    natives: Vec<NativeModule>,
) -> Result<(), String> {
    if natives.is_empty() {
        return Err("error[self-host.native-image.empty]: no modules were provided".to_string());
    }
    validate_export_id_uniqueness(&natives).map_err(|error| error.to_string())?;

    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| {
        format!(
            "error[self-host.native-image.output]: cannot create `{}`: {error}",
            parent.display()
        )
    })?;
    let target = host_tvm_target().map_err(|error| error.to_string())?;
    let linker_policy = native_linker_policy().map_err(|error| error.to_string())?;
    let package = natives[0]
        .name
        .split('.')
        .next()
        .unwrap_or(&natives[0].name)
        .to_string();
    let application_identity = if natives.len() == 1 {
        natives[0].name.clone()
    } else {
        format!("{package}.application")
    };
    let fingerprint = natives
        .iter()
        .map(NativeModule::fingerprint_sha256)
        .collect::<Vec<_>>()
        .join("\0");
    let input_sha256 = native_cache::sha256_hex(
        format!(
            "self-host-abi1\0{}\0{}\0{}\0{}\0{}",
            env!("CARGO_PKG_VERSION"),
            DIRECT_AOT_CODEGEN_REVISION,
            target.triple,
            application_identity,
            fingerprint
        )
        .as_bytes(),
    );
    let stem = output
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or("self-host-image");
    let temporary_stem = format!(
        ".{stem}.self-host-{}.{}",
        std::process::id(),
        &input_sha256[..16]
    );
    let object_suffix = if cfg!(target_os = "windows") {
        "obj"
    } else {
        "o"
    };
    let object_path = parent.join(format!("{temporary_stem}.native.{object_suffix}"));
    let descriptor_path = parent.join(format!("{temporary_stem}.descriptor.{object_suffix}"));
    let linked_path = parent.join(format!("{temporary_stem}.tvm"));

    let result = (|| {
        let object = emit_native_application_object_with_policy(
            &application_identity,
            &natives,
            NativeCodegenPolicy::Development,
        )
        .map_err(|error| error.to_string())?;
        fs::write(&object_path, &object).map_err(|error| {
            format!(
                "error[self-host.native-image.object]: cannot write `{}`: {error}",
                object_path.display()
            )
        })?;
        let descriptor = native_application_image_descriptor(
            &application_identity,
            &package,
            &natives,
            &input_sha256,
        )
        .map_err(|error| error.to_string())?;
        let descriptor_object = descriptor_object_for_native_with_debug(&object, &descriptor, &[])
            .map_err(|error| error.to_string())?;
        fs::write(&descriptor_path, descriptor_object).map_err(|error| {
            format!(
                "error[self-host.native-image.descriptor]: cannot write `{}`: {error}",
                descriptor_path.display()
            )
        })?;
        link_native_image(
            &[],
            &object_path,
            &descriptor_path,
            &linked_path,
            linker_policy,
            NativeCodegenPolicy::Development,
            &input_sha256,
        )
        .map_err(|error| error.to_string())?;
        let mut image = fs::read(&linked_path).map_err(|error| {
            format!(
                "error[self-host.native-image.read]: cannot read `{}`: {error}",
                linked_path.display()
            )
        })?;
        let sealed = seal_tvm_image(&mut image, &descriptor).map_err(|error| error.to_string())?;
        inspect_tvm_image(&image, &sealed.target.triple).map_err(|error| error.to_string())?;
        native_cache::publish_file(output, &image).map_err(|error| error.to_string())
    })();

    let _ = fs::remove_file(object_path);
    let _ = fs::remove_file(descriptor_path);
    let _ = fs::remove_file(linked_path);
    result
}
