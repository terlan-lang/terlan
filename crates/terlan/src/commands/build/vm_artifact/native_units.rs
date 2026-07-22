//! Content-addressed reusable native module object units.

use std::fs;
use std::path::{Path, PathBuf};

use crate::compiler::native_ir::{
    emit_native_module_object_with_policy, native_application_abi_fingerprint, NativeCodegenPolicy,
    NativeModule,
};

use super::super::BuildOneError;
use super::native_cache;
use super::native_image::{DIRECT_AOT_BACKEND, DIRECT_AOT_CACHE_SCHEMA};
use super::parallel_compile::{bounded_worker_limit, run_indexed_bounded, ParallelTaskError};

const NATIVE_UNIT_SCHEMA: &str = "terlan-native-unit-v1";

/// One verified object path for each NativeIR module in canonical order.
pub(super) struct NativeObjectUnits {
    /// Content-addressed relocatable object paths consumed by the final link.
    pub(super) paths: Vec<PathBuf>,
}

/// Loads or builds independently reusable module objects with bounded workers.
///
/// Inputs:
/// - `cache_root`: compiler-private application native cache.
/// - `application`: stable application identity used in object metadata.
/// - `natives`: canonical application NativeIR closure.
/// - `target`: exact target triple used for backend emission.
///
/// Output:
/// - Verified module object paths in NativeIR module order.
///
/// Transformation:
/// - Keys each unit by its implementation and the complete direct-call ABI,
///   verifies atomic cache publications, and compiles misses under the shared
///   host-bound worker ceiling.
pub(super) fn prepare_native_object_units(
    cache_root: &Path,
    application: &str,
    natives: &[NativeModule],
    target: &str,
    policy: NativeCodegenPolicy,
) -> Result<NativeObjectUnits, BuildOneError> {
    let abi = native_application_abi_fingerprint(natives).map_err(BuildOneError::Message)?;
    let units_root = cache_root.join("units");
    fs::create_dir_all(&units_root).map_err(|error| {
        BuildOneError::Message(format!(
            "error[tvm.native_unit.directory]: cannot create `{}`: {error}",
            units_root.display()
        ))
    })?;
    let indexes = (0..natives.len()).collect::<Vec<_>>();
    let result = run_indexed_bounded(&indexes, bounded_worker_limit(), |index| {
        prepare_native_object_unit(
            &units_root,
            application,
            natives,
            *index,
            target,
            &abi,
            policy,
        )
    });
    match result {
        Ok(paths) => Ok(NativeObjectUnits { paths }),
        Err(ParallelTaskError::Task(error)) => Err(error),
        Err(ParallelTaskError::WorkerPanicked) => Err(BuildOneError::Message(
            "error[tvm.native_unit.worker_panic]: native object worker panicked".to_string(),
        )),
    }
}

fn prepare_native_object_unit(
    units_root: &Path,
    application: &str,
    natives: &[NativeModule],
    module_index: usize,
    target: &str,
    abi: &str,
    policy: NativeCodegenPolicy,
) -> Result<PathBuf, BuildOneError> {
    let native = &natives[module_index];
    let input = format!(
        "{}\0{DIRECT_AOT_BACKEND}\0{DIRECT_AOT_CACHE_SCHEMA}\0{NATIVE_UNIT_SCHEMA}\0{}\0{target}\0{abi}\0{}",
        env!("CARGO_PKG_VERSION"),
        policy.cache_identity(),
        native.fingerprint_text()
    );
    let identity = native_cache::sha256_hex(input.as_bytes());
    let directory = units_root.join(&identity);
    fs::create_dir_all(&directory).map_err(|error| {
        BuildOneError::Message(format!(
            "error[tvm.native_unit.directory]: cannot create `{}`: {error}",
            directory.display()
        ))
    })?;
    let object_name = if cfg!(target_os = "windows") {
        "module.obj"
    } else {
        "module.o"
    };
    let object_path = directory.join(object_name);
    let load = || {
        native_cache::load_verified_entry(
            &directory,
            &identity,
            target,
            DIRECT_AOT_BACKEND,
            &[object_name],
            object_name,
        )
    };
    if load().is_some() {
        return Ok(object_path);
    }
    let _lock = native_cache::CacheBuildLock::acquire(&directory)?;
    if load().is_some() {
        return Ok(object_path);
    }
    let object = emit_native_module_object_with_policy(application, natives, module_index, policy)
        .map_err(BuildOneError::Message)?;
    native_cache::publish_file(&object_path, &object)?;
    let manifest = native_cache::cache_manifest_bytes(
        &identity,
        target,
        DIRECT_AOT_BACKEND,
        &[(object_name, object.as_slice())],
    );
    native_cache::publish_file(
        &directory.join(native_cache::CACHE_MANIFEST_NAME),
        &manifest,
    )?;
    Ok(object_path)
}
