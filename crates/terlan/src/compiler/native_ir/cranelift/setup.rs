//! Shared Cranelift object and application inventory construction.

use cranelift_codegen::settings::{self, Configurable};
use cranelift_module::default_libcall_names;
use cranelift_object::{ObjectBuilder, ObjectModule};

use super::super::{NativeCodegenPolicy, NativeFunction, NativeModule};

/// Creates one position-independent host object module under explicit policy.
pub(super) fn object_module(
    application: &str,
    policy: NativeCodegenPolicy,
) -> Result<ObjectModule, String> {
    let mut flags = settings::builder();
    flags
        .set("is_pic", "true")
        .map_err(|error| format!("error[cranelift.flags]: {error}"))?;
    flags
        .set("opt_level", policy.cranelift_opt_level())
        .map_err(|error| format!("error[cranelift.flags]: {error}"))?;
    let isa = cranelift_native::builder()
        .map_err(|error| format!("error[cranelift.host]: {error}"))?
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
