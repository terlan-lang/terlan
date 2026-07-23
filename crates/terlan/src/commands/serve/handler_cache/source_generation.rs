//! Compilation and admission of a source-backed AOT handler generation.

use super::*;

pub(super) fn cached_source_entry(
    web_root: &Path,
    source_path: &Path,
    expected_module: &str,
) -> Result<HandlerCacheEntry, String> {
    if let Some(entry) = cache()?
        .read()
        .map_err(|_| CACHE_ERROR.to_string())?
        .get(source_path)
        .cloned()
    {
        return Ok(entry);
    }
    let source = fs::read_to_string(source_path).map_err(|error| {
        format!(
            "error[serve.aot.source]: failed to read `{}`: {error}",
            source_path.display()
        )
    })?;
    let checksum = format!("source-fnv1a64:{:016x}", fingerprint(source.as_bytes()));
    if let Some(entry) = cache()?
        .read()
        .map_err(|_| CACHE_ERROR.to_string())?
        .get(source_path)
        .filter(|entry| entry.checksum == checksum)
        .cloned()
    {
        return Ok(entry);
    }

    let source_name = source_path.to_string_lossy();
    let artifacts = crate::formal_pipeline::compile_syntax_module_through_phases_with_profile(
        &source_name,
        &source,
        DiagnosticFormat::Text {
            color: ColorChoice::Never,
        },
        None,
        NativePolicy::NativeBoundaryOptional,
        TargetProfile::Vm,
    )
    .map_err(|code| {
        format!(
            "error[serve.aot.compile]: failed to compile `{source_name}` with exit code {code:?}"
        )
    })?;
    if artifacts.core.module != expected_module {
        return Err(format!(
            "error[serve.aot.module]: source declared `{}` but manifest expected `{expected_module}`",
            artifacts.core.module
        ));
    }
    let (core, router) = crate::compiler::router::prepare_aot_router_module(&artifacts.core)?;
    let module_stem = expected_module.replace('.', "_");
    let image = crate::commands::build::vm_artifact::native_image::
        compile_serve_native_image_with_metadata(web_root, &module_stem, &core)?
        .ok_or_else(|| {
            format!(
                "error[serve.aot.image_required]: `{expected_module}` did not produce a native image; runtime CoreIR interpretation has been removed"
            )
        })?;
    let entry = HandlerCacheEntry {
        checksum,
        runtime: Arc::new(AotHandlerRuntime::load_with_request_projections(
            expected_module.to_string(),
            &image.path,
            router,
            image.request_projections,
        )?),
    };
    cache()?
        .write()
        .map_err(|_| CACHE_ERROR.to_string())?
        .insert(source_path.to_path_buf(), entry.clone());
    Ok(entry)
}
