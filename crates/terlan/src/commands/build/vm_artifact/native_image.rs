//! Compiler-owned native image construction independent from JSON artifacts.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::compiler::native_ir::{
    emit_native_application_dispatch_object_with_policy,
    emit_native_application_object_with_policy, native_request_projections, NativeCodegenPolicy,
    NativeModule, NativeRequestProjection, DISPATCH_SYMBOL, IMAGE_ENTRY_SYMBOL,
};
use crate::runtime::native_boundary::adapter_abi::NativeAdapterAbiContract;
use crate::runtime::native_image::{
    descriptor_object_for_native_with_debug, host_tvm_target, inspect_tvm_image, seal_tvm_image,
};
use crate::terlan_typeck::CoreModule;

use super::super::{write_build_file, BuildOneError};
use super::native_debug::{encode_native_debug, NativeDebugInput};
use super::native_descriptor::native_application_image_descriptor;
use super::native_units::prepare_native_object_units;
use super::{native_cache, output_cleanup};

pub(super) const DIRECT_AOT_BACKEND: &str = "cranelift-0.133.1";
pub(super) const DIRECT_AOT_CACHE_SCHEMA: &str = "terlan-native-codegen-v2";

/// One compiler-owned native application image independent from transitional
/// per-module artifact envelopes.
#[derive(Clone, Debug)]
pub(super) struct CompiledNativeApplicationImage {
    pub(super) image_name: String,
    pub(super) cache_input_sha256: String,
    pub(super) cached_image_path: PathBuf,
    pub(super) request_projections: Vec<NativeRequestProjection>,
}

/// Live-serve image plus compiler proof metadata that is deliberately not part
/// of the frozen TVM image descriptor format.
pub(crate) struct CompiledServeNativeImage {
    pub(crate) path: PathBuf,
    pub(crate) request_projections: Vec<NativeRequestProjection>,
}

/// Immutable inputs required to publish one native application image.
struct NativeImageBuildInput<'a> {
    application_identity: &'a str,
    package: &'a str,
    natives: &'a [NativeModule],
    input_sha256: &'a str,
    target_triple: &'a str,
    object_name: &'a str,
    descriptor_object_name: &'a str,
    image_name: &'a str,
    object_path: &'a Path,
    descriptor_object_path: &'a Path,
    debug_metadata: &'a [u8],
    cached_image_path: &'a Path,
    native_dir: &'a Path,
    native_cache_root: &'a Path,
    policy: NativeCodegenPolicy,
}

/// Compiles all supported CoreIR modules into one cached application image.
pub(super) fn compile_native_application_image(
    vm_dir: &Path,
    native_cache_root: &Path,
    image_stem: &str,
    cores: &[&CoreModule],
    debug_inputs: &[NativeDebugInput<'_>],
    policy: NativeCodegenPolicy,
    incremental: bool,
) -> Result<Option<CompiledNativeApplicationImage>, BuildOneError> {
    let mut natives = NativeModule::lower_application(cores).map_err(BuildOneError::Message)?;
    if natives.is_empty() {
        output_cleanup::remove_stale_tvm_images(vm_dir, None)?;
        return Ok(None);
    }
    natives.sort_by(|left, right| left.name.cmp(&right.name));
    validate_export_id_uniqueness(&natives)?;
    let request_projections = native_request_projections(&natives);
    let debug_metadata = if debug_inputs.is_empty() {
        Vec::new()
    } else {
        encode_native_debug(debug_inputs, &natives).map_err(BuildOneError::Message)?
    };

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
    let object_name = if cfg!(target_os = "windows") {
        format!("{image_stem}.native.obj")
    } else {
        format!("{image_stem}.native.o")
    };
    let descriptor_object_name = if cfg!(target_os = "windows") {
        format!("{image_stem}.descriptor.obj")
    } else {
        format!("{image_stem}.descriptor.o")
    };
    let image_name = format!("{image_stem}.tvm");
    let image_path = vm_dir.join(&image_name);
    let target = host_tvm_target().map_err(BuildOneError::Message)?;
    let adapter_cache_identity = NativeAdapterAbiContract::current()
        .cache_identity(&target.triple, &target.calling_convention)
        .map_err(BuildOneError::Message)?;
    let fingerprint = natives
        .iter()
        .map(NativeModule::fingerprint_text)
        .collect::<Vec<_>>()
        .join("\0");
    let cache_input = format!(
        "{}\0{DIRECT_AOT_BACKEND}\0{DIRECT_AOT_CACHE_SCHEMA}\0tvm-image-format-1\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}",
        env!("CARGO_PKG_VERSION"),
        policy.cache_identity(),
        target.triple,
        target.architecture,
        target.operating_system,
        target.calling_convention,
        adapter_cache_identity,
        fingerprint,
        native_cache::sha256_hex(&debug_metadata)
    );
    let input_sha256 = native_cache::sha256_hex(cache_input.as_bytes());
    let native_dir = native_cache_root.join(&input_sha256);
    fs::create_dir_all(&native_dir).map_err(|error| {
        BuildOneError::Message(format!(
            "cannot create pure native cache directory `{}`: {error}",
            native_dir.display()
        ))
    })?;
    let object_path = native_dir.join(&object_name);
    let descriptor_object_path = native_dir.join(&descriptor_object_name);
    let cached_image_path = native_dir.join(&image_name);
    let cache_file_names = [
        object_name.as_str(),
        descriptor_object_name.as_str(),
        image_name.as_str(),
    ];
    let expected_build_identity = format!("sha256:{input_sha256}");
    let load_cached_image = || {
        incremental
            .then(|| {
                native_cache::load_verified_entry(
                    &native_dir,
                    &input_sha256,
                    &target.triple,
                    DIRECT_AOT_BACKEND,
                    &cache_file_names,
                    &image_name,
                )
            })
            .flatten()
            .filter(|image| {
                inspect_tvm_image(image, &target.triple).is_ok_and(|inspection| {
                    inspection.descriptor.identity.build == expected_build_identity
                })
            })
    };
    let image = if let Some(image) = load_cached_image() {
        image
    } else {
        let _cache_build_lock = native_cache::CacheBuildLock::acquire(&native_dir)?;
        if let Some(image) = load_cached_image() {
            image
        } else {
            compile_and_publish_image(NativeImageBuildInput {
                application_identity: &application_identity,
                package: &package,
                natives: &natives,
                input_sha256: &input_sha256,
                target_triple: &target.triple,
                object_name: &object_name,
                descriptor_object_name: &descriptor_object_name,
                image_name: &image_name,
                object_path: &object_path,
                descriptor_object_path: &descriptor_object_path,
                debug_metadata: &debug_metadata,
                cached_image_path: &cached_image_path,
                native_dir: &native_dir,
                native_cache_root,
                policy,
            })?
        }
    };
    let inspection = inspect_tvm_image(&image, &target.triple).map_err(BuildOneError::Message)?;
    if inspection.descriptor.identity.build != expected_build_identity {
        return Err(BuildOneError::Message(format!(
            "error[tvm.cache.identity]: cached image build identity `{}` does not match `{expected_build_identity}`",
            inspection.descriptor.identity.build
        )));
    }
    write_build_file(&image_path, &image, incremental).map_err(BuildOneError::Message)?;
    output_cleanup::remove_stale_tvm_images(vm_dir, Some(&image_name))?;

    Ok(Some(CompiledNativeApplicationImage {
        image_name,
        cache_input_sha256: input_sha256,
        cached_image_path,
        request_projections,
    }))
}

/// Compiles one REPL generation into the shared content-addressed AOT cache.
pub(crate) fn compile_repl_native_image(
    workspace: &Path,
    module_stem: &str,
    core: &CoreModule,
) -> Result<Option<PathBuf>, String> {
    let vm_dir = workspace.join("vm");
    fs::create_dir_all(&vm_dir)
        .map_err(|error| format!("cannot create REPL AOT output directory: {error}"))?;
    let native_cache_root = workspace.join(".terlan").join("native-aot");
    compile_native_application_image(
        &vm_dir,
        &native_cache_root,
        module_stem,
        &[core],
        &[],
        NativeCodegenPolicy::Development,
        true,
    )
    .map(|image| image.map(|image| image.cached_image_path))
    .map_err(build_error_message)
}

/// Compiles one live serve generation into its package-local native cache.
#[allow(dead_code)] // Retained for focused serve-image compilation and tests.
pub(crate) fn compile_serve_native_image(
    web_root: &Path,
    module_stem: &str,
    core: &CoreModule,
) -> Result<Option<PathBuf>, String> {
    let workspace = web_root.join(".terlan").join("serve-aot");
    let vm_dir = workspace.join("vm");
    fs::create_dir_all(&vm_dir)
        .map_err(|error| format!("cannot create serve AOT output directory: {error}"))?;
    let native_cache_root = workspace.join("native-aot");
    compile_native_application_image(
        &vm_dir,
        &native_cache_root,
        module_stem,
        &[core],
        &[],
        NativeCodegenPolicy::Serve,
        true,
    )
    .map(|image| image.map(|image| image.cached_image_path))
    .map_err(build_error_message)
}

/// Compiles one live serve generation and retains export-specific Request
/// projection proofs for admission into the matching runtime generation.
pub(crate) fn compile_serve_native_image_with_metadata(
    web_root: &Path,
    module_stem: &str,
    core: &CoreModule,
) -> Result<Option<CompiledServeNativeImage>, String> {
    let workspace = web_root.join(".terlan").join("serve-aot");
    let vm_dir = workspace.join("vm");
    fs::create_dir_all(&vm_dir)
        .map_err(|error| format!("cannot create serve AOT output directory: {error}"))?;
    let native_cache_root = workspace.join("native-aot");
    compile_native_application_image(
        &vm_dir,
        &native_cache_root,
        module_stem,
        &[core],
        &[],
        NativeCodegenPolicy::Serve,
        true,
    )
    .map(|image| {
        image.map(|image| CompiledServeNativeImage {
            path: image.cached_image_path,
            request_projections: image.request_projections,
        })
    })
    .map_err(build_error_message)
}

/// Compiles one test module into a native image without producing JSON artifacts.
pub(crate) fn compile_test_native_image(
    workspace: &Path,
    native_cache_root: &Path,
    module_stem: &str,
    cores: &[&CoreModule],
    incremental: bool,
) -> Result<Option<PathBuf>, String> {
    let vm_dir = workspace.join("vm");
    fs::create_dir_all(&vm_dir)
        .map_err(|error| format!("cannot create test AOT output directory: {error}"))?;
    compile_native_application_image(
        &vm_dir,
        native_cache_root,
        module_stem,
        cores,
        &[],
        NativeCodegenPolicy::Development,
        incremental,
    )
    .map(|image| image.map(|image| image.cached_image_path))
    .map_err(build_error_message)
}

/// Compiles one complete hot-reload generation into the native image cache.
pub(crate) fn compile_reload_native_image(
    workspace: &Path,
    native_cache_root: &Path,
    generation_stem: &str,
    cores: &[&CoreModule],
) -> Result<Option<PathBuf>, String> {
    let vm_dir = workspace.join("vm");
    fs::create_dir_all(&vm_dir)
        .map_err(|error| format!("cannot create hot-reload AOT output directory: {error}"))?;
    compile_native_application_image(
        &vm_dir,
        native_cache_root,
        generation_stem,
        cores,
        &[],
        NativeCodegenPolicy::Development,
        true,
    )
    .map(|image| image.map(|image| image.cached_image_path))
    .map_err(build_error_message)
}

fn validate_export_id_uniqueness(natives: &[NativeModule]) -> Result<(), BuildOneError> {
    let mut export_ids = HashSet::new();
    for native in natives {
        for function in &native.functions {
            if !export_ids.insert(function.export_id) {
                return Err(BuildOneError::Message(format!(
                    "error[native_ir.export_id_collision]: application export id {} collides at `{}.{}/{}`",
                    function.export_id, native.name, function.name, function.arity
                )));
            }
        }
        for continuation in &native.continuations {
            if !export_ids.insert(continuation.id) {
                return Err(BuildOneError::Message(format!(
                    "error[native_ir.continuation_id_collision]: application continuation id {} collides in `{}`",
                    continuation.id, native.name
                )));
            }
        }
    }
    Ok(())
}

fn compile_and_publish_image(input: NativeImageBuildInput<'_>) -> Result<Vec<u8>, BuildOneError> {
    let module_units = (input.natives.len() > 1 && input.policy.uses_incremental_module_units())
        .then(|| {
            prepare_native_object_units(
                input.native_cache_root,
                input.application_identity,
                input.natives,
                input.target_triple,
                input.policy,
            )
        })
        .transpose()?;
    let object = if module_units.is_some() {
        emit_native_application_dispatch_object_with_policy(
            input.application_identity,
            input.natives,
            input.policy,
        )
    } else {
        emit_native_application_object_with_policy(
            input.application_identity,
            input.natives,
            input.policy,
        )
    }
    .map_err(BuildOneError::Message)?;
    native_cache::publish_file(input.object_path, &object)?;
    let descriptor = native_application_image_descriptor(
        input.application_identity,
        input.package,
        input.natives,
        input.input_sha256,
    )?;
    let descriptor_object =
        descriptor_object_for_native_with_debug(&object, &descriptor, input.debug_metadata)
            .map_err(BuildOneError::Message)?;
    native_cache::publish_file(input.descriptor_object_path, &descriptor_object)?;
    let linked_image = native_cache::TemporaryCacheFile::beside(input.cached_image_path)?;
    let unit_paths = module_units
        .as_ref()
        .map(|units| units.paths.as_slice())
        .unwrap_or_default();
    link_native_image(
        unit_paths,
        input.object_path,
        input.descriptor_object_path,
        linked_image.path(),
        input.policy,
    )?;
    let mut image = fs::read(linked_image.path()).map_err(|error| {
        BuildOneError::Message(format!(
            "failed to read linked TVM image `{}`: {error}",
            linked_image.path().display()
        ))
    })?;
    let sealed = seal_tvm_image(&mut image, &descriptor).map_err(BuildOneError::Message)?;
    inspect_tvm_image(&image, &sealed.target.triple).map_err(BuildOneError::Message)?;
    native_cache::publish_file(input.cached_image_path, &image)?;
    let manifest = native_cache::cache_manifest_bytes(
        input.input_sha256,
        input.target_triple,
        DIRECT_AOT_BACKEND,
        &[
            (input.object_name, object.as_slice()),
            (input.descriptor_object_name, descriptor_object.as_slice()),
            (input.image_name, image.as_slice()),
        ],
    );
    native_cache::publish_file(
        &input.native_dir.join(native_cache::CACHE_MANIFEST_NAME),
        &manifest,
    )?;
    Ok(image)
}

/// Links one Cranelift application object and descriptor object exactly once.
fn link_native_image(
    unit_paths: &[PathBuf],
    object_path: &Path,
    descriptor_object_path: &Path,
    image_path: &Path,
    policy: NativeCodegenPolicy,
) -> Result<(), BuildOneError> {
    let linker = std::env::var_os("TERLAN_NATIVE_LINKER").unwrap_or_else(|| {
        if cfg!(target_os = "windows") {
            "link.exe".into()
        } else if cfg!(target_os = "macos") {
            "cc".into()
        } else {
            "ld".into()
        }
    });
    let mut command = Command::new(&linker);
    if cfg!(target_os = "macos") {
        command
            .arg("-dynamiclib")
            .arg("-Wl,-undefined,dynamic_lookup")
            .arg("-o")
            .arg(image_path);
        if policy.optimizes_link() {
            command.arg("-Wl,-dead_strip");
        }
        command
            .args(unit_paths)
            .arg(object_path)
            .arg(descriptor_object_path);
    } else if cfg!(target_os = "windows") {
        command
            .arg("/DLL")
            .arg(format!("/ENTRY:{IMAGE_ENTRY_SYMBOL}"))
            .arg(format!("/EXPORT:{DISPATCH_SYMBOL}"))
            .arg(format!("/OUT:{}", image_path.display()));
        if policy.optimizes_link() {
            command.arg("/OPT:REF").arg("/OPT:ICF");
        }
        command
            .args(unit_paths)
            .arg(object_path)
            .arg(descriptor_object_path);
    } else {
        command
            .arg("-shared")
            .arg("-e")
            .arg(IMAGE_ENTRY_SYMBOL)
            .arg("-z")
            .arg("noexecstack")
            .arg("-o")
            .arg(image_path);
        if policy.optimizes_link() {
            command.arg("-O1");
        }
        command
            .args(unit_paths)
            .arg(object_path)
            .arg(descriptor_object_path);
    }
    let output = command.output().map_err(|error| {
        BuildOneError::Message(format!(
            "failed to start native linker `{}`: {error}",
            Path::new(&linker).display()
        ))
    })?;
    if !output.status.success() {
        return Err(BuildOneError::Message(format!(
            "native linker failed for `{}`:\n{}",
            image_path.display(),
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    Ok(())
}

fn build_error_message(error: BuildOneError) -> String {
    match error {
        BuildOneError::Message(message) => message,
        BuildOneError::Exit(code) => format!("REPL AOT compilation failed with exit code {code:?}"),
    }
}
