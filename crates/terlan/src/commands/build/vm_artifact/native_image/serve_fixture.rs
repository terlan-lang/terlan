//! Native image compilation for focused serve fixtures.

use super::*;

/// Compiles one live serve generation into its package-local native cache.
#[cfg(test)]
pub(crate) fn compile_serve_native_image(
    web_root: &Path,
    module_stem: &str,
    core: &CoreModule,
) -> Result<Option<PathBuf>, String> {
    compile_serve_native_modules(web_root, module_stem, std::slice::from_ref(&core))
}

/// Compiles several focused serve modules into one reusable test image.
#[cfg(test)]
pub(crate) fn compile_serve_native_modules(
    web_root: &Path,
    module_stem: &str,
    source_cores: &[&CoreModule],
) -> Result<Option<PathBuf>, String> {
    let workspace = web_root.join(".terlan").join("serve-aot");
    let vm_dir = workspace.join("vm");
    fs::create_dir_all(&vm_dir)
        .map_err(|error| format!("cannot create serve AOT output directory: {error}"))?;
    let native_cache_root = workspace.join("native-aot");
    let state = crate::CliState {
        native_policy: crate::validation::native_policy::NativePolicy::NativeBoundaryOptional,
        ..crate::CliState::default()
    };
    let imported = super::super::std_source::compile_imported_std_source_modules(
        source_cores,
        web_root,
        &state,
    )
    .map_err(build_error_message)?;
    let cores = source_cores
        .iter()
        .copied()
        .chain(imported.iter().map(|module| &module.compiled.core))
        .collect::<Vec<_>>();
    let roots = source_cores
        .iter()
        .flat_map(|core| {
            core.functions
                .iter()
                .filter(|function| function.public)
                .map(|function| (core.module.clone(), function.name.clone(), function.arity))
        })
        .collect::<Vec<_>>();
    compile_rooted_native_application_image(
        &vm_dir,
        &native_cache_root,
        module_stem,
        &cores,
        RootedNativeApplicationInput {
            roots: &roots,
            debug_inputs: &[],
            policy: NativeCodegenPolicy::Serve,
            incremental: true,
        },
    )
    .map(|image| image.map(|image| image.cached_image_path))
    .map_err(build_error_message)
}
