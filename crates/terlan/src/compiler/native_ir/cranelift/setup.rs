//! Shared Cranelift object and application inventory construction.

use cranelift_codegen::ir::{FuncRef, Function, GlobalValue, GlobalValueData};
use cranelift_codegen::isa;
use cranelift_codegen::settings::{self, Configurable};
use cranelift_module::{default_libcall_names, DataId, FuncId, Module};
use cranelift_object::{ObjectBuilder, ObjectModule};
use target_lexicon::{Architecture, OperatingSystem};

use super::super::{NativeCodegenPolicy, NativeFunction, NativeModule};

/// Creates one position-independent host object module under explicit policy.
pub(super) fn object_module(
    application: &str,
    policy: NativeCodegenPolicy,
) -> Result<ObjectModule, String> {
    let isa =
        cranelift_native::builder().map_err(|error| format!("error[cranelift.host]: {error}"))?;
    object_module_for_isa(application, policy, isa)
}

/// Creates one position-independent object module for an explicit target ISA.
pub(super) fn object_module_for_isa(
    application: &str,
    policy: NativeCodegenPolicy,
    isa: isa::Builder,
) -> Result<ObjectModule, String> {
    let mut flags = settings::builder();
    let windows_aarch64 = matches!(isa.triple().architecture, Architecture::Aarch64(_))
        && isa.triple().operating_system == OperatingSystem::Windows;
    flags
        .set("is_pic", if windows_aarch64 { "false" } else { "true" })
        .map_err(|error| format!("error[cranelift.flags]: {error}"))?;
    flags
        .set("opt_level", policy.cranelift_opt_level())
        .map_err(|error| format!("error[cranelift.flags]: {error}"))?;
    let isa = isa
        .finish(settings::Flags::new(flags))
        .map_err(|error| format!("error[cranelift.isa]: {error}"))?;
    let builder = ObjectBuilder::new(
        isa,
        application.as_bytes().to_vec(),
        default_libcall_names(),
    )
    .map_err(|error| format!("error[cranelift.object]: {error}"))?;
    Ok(ObjectModule::new(builder))
}

/// Declares an image-local function call whose target is linked into the same native image.
///
/// Cranelift otherwise treats imported functions as arbitrarily distant. On AArch64 that
/// requires a GOT relocation which COFF does not support. Terlan's native linker owns every
/// application object, so these calls can use the platform's ordinary linker-resolved branch
/// relocation and any linker-provided range extension.
pub(super) fn declare_image_func_in_func(
    module: &mut ObjectModule,
    function_id: FuncId,
    function: &mut Function,
) -> FuncRef {
    let function_ref = module.declare_func_in_func(function_id, function);
    function.dfg.ext_funcs[function_ref].colocated = true;
    function_ref
}

/// Declares image-local data using relocations supported by every object format.
///
/// Cranelift's AArch64 COFF backend does not implement either its PIC GOT
/// relocations or the page-relative relocation used for nearby data. Windows
/// AArch64 objects therefore use the supported absolute 64-bit relocation for
/// these immutable image tables. PIC targets ignore this distance metadata.
pub(super) fn declare_image_data_in_func(
    module: &mut ObjectModule,
    data_id: DataId,
    function: &mut Function,
) -> GlobalValue {
    let global_value = module.declare_data_in_func(data_id, function);
    if let GlobalValueData::Symbol { colocated, .. } = &mut function.global_values[global_value] {
        *colocated = false;
    }
    global_value
}

/// Flattens module metadata into the canonical application function order.
pub(super) fn flattened_application(application: &str, natives: &[NativeModule]) -> NativeModule {
    NativeModule {
        name: application.to_string(),
        functions: natives
            .iter()
            .flat_map(|native| native.functions.iter().cloned())
            .collect(),
        continuations: Vec::new(),
        managed_layouts: natives
            .iter()
            .flat_map(|native| native.managed_layouts.iter().cloned())
            .collect(),
        managed_collections: natives
            .iter()
            .flat_map(|native| native.managed_collections.iter().cloned())
            .collect(),
        atoms: natives
            .iter()
            .flat_map(|native| native.atoms.iter().cloned())
            .collect(),
    }
}

/// Returns every native function with its owning module in canonical order.
pub(super) fn application_functions(
    natives: &[NativeModule],
) -> Vec<(&NativeModule, &NativeFunction)> {
    natives
        .iter()
        .flat_map(|native| {
            native
                .functions
                .iter()
                .map(move |function| (native, function))
        })
        .collect()
}
