#![cfg(test)]

use std::fs;
use std::path::PathBuf;

use crate::compiler::router::AotRouterPlan;
use crate::support::test_fs;
use crate::{ColorChoice, DiagnosticFormat};

/// One compiled native handler fixture and its temporary output root.
pub(in crate::commands::serve) struct CompiledNativeHandlerFixture {
    /// Temporary root that the owning test must remove.
    pub(in crate::commands::serve) root: PathBuf,
    /// Generated native image path admitted by handler scheduler owners.
    pub(in crate::commands::serve) image: PathBuf,
    /// Optional static router plan emitted by the compiler.
    pub(in crate::commands::serve) router: Option<AotRouterPlan>,
}

/// Clears the process-wide native handler cache between focused tests.
pub(in crate::commands::serve) fn clear_vm_handler_module_cache_for_test() {
    super::invalidate_vm_handler_cache();
}

/// Compiles one Terlan module into a reusable native handler fixture.
pub(in crate::commands::serve) fn compile_native_handler_fixture(
    fixture: &str,
    source_path: &str,
    image_name: &str,
    source: &str,
) -> CompiledNativeHandlerFixture {
    let root = test_fs::temp_path("serve", fixture);
    let web_root = root.join("_build/web");
    fs::create_dir_all(&web_root).expect("create native handler output");
    let artifacts = crate::formal_pipeline::compile_syntax_module_through_phases_with_profile(
        source_path,
        source,
        DiagnosticFormat::Text {
            color: ColorChoice::Never,
        },
        None,
        crate::validation::native_policy::NativePolicy::NativeBoundaryOptional,
        crate::validation::target_profile::TargetProfile::Vm,
    )
    .expect("compile native handler source");
    let (core, router) = crate::compiler::router::prepare_aot_router_module(&artifacts.core)
        .expect("prepare native handler module");
    let image = crate::commands::build::vm_artifact::native_image::compile_serve_native_image(
        &web_root, image_name, &core,
    )
    .expect("compile native handler image")
    .expect("handler produces native image");
    CompiledNativeHandlerFixture {
        root,
        image,
        router,
    }
}
