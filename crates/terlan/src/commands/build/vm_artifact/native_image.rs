//! Compiler-owned native image construction independent from JSON artifacts.

use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

#[cfg(any(test, not(feature = "serve-runtime-bin")))]
use crate::compiler::native_ir::native_request_projections;
use crate::compiler::native_ir::{
    emit_native_application_dispatch_object_with_policy,
    emit_native_application_object_with_policy, install_native_request_projection_exports,
    NativeCodegenPolicy, NativeModule, DISPATCH_SYMBOL, IMAGE_ENTRY_SYMBOL,
};
use crate::runtime::native_boundary::adapter_abi::NativeAdapterAbiContract;
use crate::runtime::native_image::{
    descriptor_object_for_native_with_debug, host_tvm_target, inspect_tvm_image, seal_tvm_image,
};
#[cfg(any(test, not(feature = "serve-runtime-bin")))]
use crate::runtime::vm::aot_metadata::NativeRequestProjection;
use crate::terlan_typeck::CoreModule;

use super::super::{write_build_file, BuildOneError};
use super::native_debug::{encode_native_debug, NativeDebugInput};
use super::native_descriptor::native_application_image_descriptor;
use super::native_units::prepare_native_object_units;
use super::{native_cache, output_cleanup};

pub(super) const DIRECT_AOT_BACKEND: &str = "cranelift-0.133.1";
pub(super) const DIRECT_AOT_CACHE_SCHEMA: &str = "terlan-native-codegen-v4";
pub(super) const DIRECT_AOT_CODEGEN_REVISION: &str = env!("TERLAN_NATIVE_CODEGEN_REVISION_SHA256");
pub(super) const DIRECT_AOT_BUILD_POLICY: &str = env!("TERLAN_NATIVE_BUILD_POLICY_SHA256");

/// One compiler-owned native application image independent from transitional
/// per-module artifact envelopes.
#[derive(Clone, Debug)]
pub(super) struct CompiledNativeApplicationImage {
    pub(super) image_name: String,
    pub(super) cache_input_sha256: String,
    pub(super) cached_image_path: PathBuf,
    #[cfg(any(test, not(feature = "serve-runtime-bin")))]
    pub(super) request_projections: Vec<NativeRequestProjection>,
}

/// Live-serve image plus compiler proof metadata that is deliberately not part
/// of the frozen TVM image descriptor format.
#[cfg(any(test, not(feature = "serve-runtime-bin")))]
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
    linker_policy: &'a NativeLinkerPolicy,
    policy: NativeCodegenPolicy,
}

#[derive(Clone, Debug)]
struct NativeLinkerPolicy {
    program: OsString,
    bundled_windows_linker: bool,
    cache_identity: String,
}

static NATIVE_LINKER_POLICY: OnceLock<Result<NativeLinkerPolicy, String>> = OnceLock::new();

struct NativeApplicationCompileRequest<'a> {
    vm_dir: &'a Path,
    native_cache_root: &'a Path,
    image_stem: &'a str,
    cores: &'a [&'a CoreModule],
    debug_inputs: &'a [NativeDebugInput<'a>],
    policy: NativeCodegenPolicy,
    incremental: bool,
    application_identity: Option<&'a str>,
}

pub(super) struct RootedNativeApplicationInput<'a> {
    pub(super) roots: &'a [(String, String, usize)],
    pub(super) debug_inputs: &'a [NativeDebugInput<'a>],
    pub(super) policy: NativeCodegenPolicy,
    pub(super) incremental: bool,
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
    compile_native_application_image_with_identity(NativeApplicationCompileRequest {
        vm_dir,
        native_cache_root,
        image_stem,
        cores,
        debug_inputs,
        policy,
        incremental,
        application_identity: None,
    })
}

fn compile_native_application_image_with_identity(
    request: NativeApplicationCompileRequest<'_>,
) -> Result<Option<CompiledNativeApplicationImage>, BuildOneError> {
    let NativeApplicationCompileRequest {
        vm_dir,
        native_cache_root,
        image_stem,
        cores,
        debug_inputs,
        policy,
        incremental,
        application_identity,
    } = request;
    let mut natives = NativeModule::lower_application(cores).map_err(BuildOneError::Message)?;
    if natives.is_empty() {
        output_cleanup::remove_stale_tvm_images(vm_dir, None)?;
        return Ok(None);
    }
    // `NativeModule::lower_application` returns the canonical function-index
    // order, including synthetic closure and continuation modules. Reordering
    // modules here would invalidate every application-wide direct-call index.
    validate_export_id_uniqueness(&natives)?;
    #[cfg(any(test, not(feature = "serve-runtime-bin")))]
    let request_projections = if policy == NativeCodegenPolicy::Serve {
        install_native_request_projection_exports(&mut natives)
    } else {
        native_request_projections(&natives)
    };
    #[cfg(feature = "serve-runtime-bin")]
    if policy == NativeCodegenPolicy::Serve {
        install_native_request_projection_exports(&mut natives);
    }
    validate_export_id_uniqueness(&natives)?;
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
    let application_identity = application_identity.map_or_else(
        || {
            if natives.len() == 1 {
                natives[0].name.clone()
            } else {
                format!("{package}.application")
            }
        },
        str::to_string,
    );
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
    let target = host_tvm_target().map_err(|error| BuildOneError::Message(error.into()))?;
    let linker_policy = native_linker_policy()?;
    let adapter_cache_identity = NativeAdapterAbiContract::current()
        .cache_identity(&target.triple, &target.calling_convention)
        .map_err(|error| BuildOneError::Message(error.into()))?;
    let fingerprint = natives
        .iter()
        .map(NativeModule::fingerprint_sha256)
        .collect::<Vec<_>>()
        .join("\0");
    let cache_input = format!(
        "{}\0{DIRECT_AOT_BACKEND}\0{DIRECT_AOT_CACHE_SCHEMA}\0{DIRECT_AOT_CODEGEN_REVISION}\0{DIRECT_AOT_BUILD_POLICY}\0tvm-image-format-1\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}",
        env!("CARGO_PKG_VERSION"),
        application_identity,
        policy.cache_identity(),
        target.triple,
        target.architecture,
        target.operating_system,
        target.calling_convention,
        adapter_cache_identity,
        linker_policy.cache_identity,
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
            if std::env::var("TERLAN_NATIVE_CACHE_MISS_POLICY").as_deref() == Ok("error") {
                return Err(BuildOneError::Message(format!(
                    "error[tvm.cache.unexpected_miss]: validation forbids rebuilding native cache entry `{input_sha256}`"
                )));
            }
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
                linker_policy,
                policy,
            })?
        }
    };
    let inspection = inspect_tvm_image(&image, &target.triple)
        .map_err(|error| BuildOneError::Message(error.into()))?;
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
        #[cfg(any(test, not(feature = "serve-runtime-bin")))]
        request_projections,
    }))
}

/// Compiles one application image from explicit executable roots.
///
/// Source/type checking still covers every supplied module. Only the native
/// link closure is pruned, matching ordinary AOT executable semantics and
/// preventing dead library helpers from becoming accidental image ABI roots.
pub(super) fn compile_rooted_native_application_image(
    vm_dir: &Path,
    native_cache_root: &Path,
    image_stem: &str,
    cores: &[&CoreModule],
    input: RootedNativeApplicationInput<'_>,
) -> Result<Option<CompiledNativeApplicationImage>, BuildOneError> {
    let RootedNativeApplicationInput {
        roots,
        debug_inputs,
        policy,
        incremental,
    } = input;
    let mut rooted = cores.iter().map(|core| (*core).clone()).collect::<Vec<_>>();
    crate::compiler::native_ir::resolve_typed_mutable_receiver_calls(&mut rooted)
        .map_err(|error| BuildOneError::Message(error.to_string()))?;
    crate::compiler::native_ir::prune_application_to_function_roots(&mut rooted, roots)
        .map_err(|error| BuildOneError::Message(error.to_string()))?;
    let rooted = rooted.iter().collect::<Vec<_>>();
    let application_identity = roots
        .first()
        .map(|(module, _, _)| module.as_str())
        .ok_or_else(|| {
            BuildOneError::Message(
                "error[native_ir.root]: rooted native application requires an executable root"
                    .to_string(),
            )
        })?;
    compile_native_application_image_with_identity(NativeApplicationCompileRequest {
        vm_dir,
        native_cache_root,
        image_stem,
        cores: &rooted,
        debug_inputs,
        policy,
        incremental,
        application_identity: Some(application_identity),
    })
}

/// Compiles one REPL generation into the shared content-addressed AOT cache.
pub(crate) fn compile_repl_native_image(
    workspace: &Path,
    module_stem: &str,
    source_path: &str,
    source_text: &str,
    syntax: &crate::terlan_syntax::SyntaxModuleOutput,
    core: &CoreModule,
) -> Result<Option<PathBuf>, String> {
    let vm_dir = workspace.join("vm");
    fs::create_dir_all(&vm_dir)
        .map_err(|error| format!("cannot create REPL AOT output directory: {error}"))?;
    let native_cache_root = workspace.join(".terlan").join("native-aot");
    let debug_inputs = [NativeDebugInput {
        source_path,
        source_text,
        core,
        syntax,
    }];
    compile_native_application_image(
        &vm_dir,
        &native_cache_root,
        module_stem,
        &[core],
        &debug_inputs,
        NativeCodegenPolicy::Development,
        true,
    )
    .map(|image| image.map(|image| image.cached_image_path))
    .map_err(build_error_message)
}

/// Compiles one live serve generation into its package-local native cache.
#[cfg(test)]
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

/// Compiles a live-serve image from the complete application closure while
/// retaining Request projection metadata for the route-owning module.
#[cfg(any(test, not(feature = "serve-runtime-bin")))]
pub(super) fn compile_serve_native_application_image_with_metadata(
    web_root: &Path,
    module_stem: &str,
    cores: &[&CoreModule],
    debug_inputs: &[NativeDebugInput<'_>],
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
        cores,
        debug_inputs,
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
    .map_err(|error| BuildOneError::Message(error.into()))?;
    native_cache::publish_file(input.object_path, &object)?;
    let descriptor = native_application_image_descriptor(
        input.application_identity,
        input.package,
        input.natives,
        input.input_sha256,
    )?;
    let descriptor_object =
        descriptor_object_for_native_with_debug(&object, &descriptor, input.debug_metadata)
            .map_err(|error| BuildOneError::Message(error.into()))?;
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
        input.linker_policy,
        input.policy,
    )?;
    let mut image = fs::read(linked_image.path()).map_err(|error| {
        BuildOneError::Message(format!(
            "failed to read linked TVM image `{}`: {error}",
            linked_image.path().display()
        ))
    })?;
    let sealed = seal_tvm_image(&mut image, &descriptor)
        .map_err(|error| BuildOneError::Message(error.into()))?;
    inspect_tvm_image(&image, &sealed.target.triple)
        .map_err(|error| BuildOneError::Message(error.into()))?;
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
    linker_policy: &NativeLinkerPolicy,
    policy: NativeCodegenPolicy,
) -> Result<(), BuildOneError> {
    let mut command = Command::new(&linker_policy.program);
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
        if linker_policy.bundled_windows_linker {
            command.arg("-flavor").arg("link");
        }
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
            Path::new(&linker_policy.program).display()
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

/// Resolves and fingerprints the exact linker admitted by this compiler process.
///
/// A linker override is a code-generation input: accepting an image produced by
/// a different linker would turn a warm cache hit into untracked environment
/// dependence. The binary digest is computed once per compiler process.
fn native_linker_policy() -> Result<&'static NativeLinkerPolicy, BuildOneError> {
    NATIVE_LINKER_POLICY
        .get_or_init(|| {
            let (program, bundled_windows_linker) =
                if let Some(linker) = std::env::var_os("TERLAN_NATIVE_LINKER") {
                    (linker, false)
                } else {
                    let (linker, bundled) = default_native_linker().map_err(build_error_message)?;
                    (linker, bundled)
                };
            let resolved = resolve_linker_program(&program).ok_or_else(|| {
                format!(
                    "error[tvm.cache.linker_identity]: cannot resolve native linker `{}`",
                    Path::new(&program).display()
                )
            })?;
            let bytes = fs::read(&resolved).map_err(|error| {
                format!(
                    "error[tvm.cache.linker_identity]: cannot read native linker `{}`: {error}",
                    resolved.display()
                )
            })?;
            let cache_identity = native_cache::sha256_hex(
                format!(
                    "terlan-native-linker-v1\0{}\0{}",
                    bundled_windows_linker,
                    native_cache::sha256_hex(&bytes)
                )
                .as_bytes(),
            );
            Ok(NativeLinkerPolicy {
                program: resolved.into_os_string(),
                bundled_windows_linker,
                cache_identity,
            })
        })
        .as_ref()
        .map_err(|error| BuildOneError::Message(error.clone()))
}

pub(super) fn native_linker_cache_identity() -> Result<&'static str, BuildOneError> {
    native_linker_policy().map(|policy| policy.cache_identity.as_str())
}

fn resolve_linker_program(program: &OsStr) -> Option<PathBuf> {
    let path = Path::new(program);
    if path.components().count() > 1 {
        return fs::canonicalize(path)
            .ok()
            .filter(|candidate| candidate.is_file());
    }
    let search_path = std::env::var_os("PATH")?;
    for directory in std::env::split_paths(&search_path) {
        let candidate = directory.join(path);
        if let Ok(resolved) = fs::canonicalize(&candidate) {
            if resolved.is_file() {
                return Some(resolved);
            }
        }
        #[cfg(windows)]
        if candidate.extension().is_none() {
            let executable = candidate.with_extension("exe");
            if let Ok(resolved) = fs::canonicalize(&executable) {
                if resolved.is_file() {
                    return Some(resolved);
                }
            }
        }
    }
    None
}

/// Resolves the host linker without relying on ambiguous platform `PATH`
/// entries.
fn default_native_linker() -> Result<(OsString, bool), BuildOneError> {
    #[cfg(target_os = "windows")]
    {
        bundled_rust_lld().map(|linker| (linker, true))
    }
    #[cfg(target_os = "macos")]
    {
        Ok(("cc".into(), false))
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        Ok(("ld".into(), false))
    }
}

/// Finds the COFF-capable linker shipped with the active Rust toolchain.
#[cfg(target_os = "windows")]
fn bundled_rust_lld() -> Result<OsString, BuildOneError> {
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let output = Command::new(&rustc)
        .args(["--print", "target-libdir"])
        .output()
        .map_err(|error| {
            BuildOneError::Message(format!(
                "failed to locate the Rust target library directory with `{}`: {error}",
                Path::new(&rustc).display()
            ))
        })?;
    if !output.status.success() {
        return Err(BuildOneError::Message(format!(
            "`{}` could not locate the Rust target library directory:\n{}",
            Path::new(&rustc).display(),
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    let target_libdir = String::from_utf8(output.stdout)
        .map_err(|_| {
            BuildOneError::Message("Rust target library directory is not valid UTF-8".to_string())
        })?
        .trim()
        .to_string();
    let target_root = Path::new(&target_libdir).parent().ok_or_else(|| {
        BuildOneError::Message(format!(
            "Rust target library directory `{target_libdir}` has no toolchain root"
        ))
    })?;
    let linker = target_root.join("bin").join("rust-lld.exe");
    if !linker.is_file() {
        return Err(BuildOneError::Message(format!(
            "Rust toolchain linker `{}` does not exist",
            linker.display()
        )));
    }
    Ok(linker.into_os_string())
}

fn build_error_message(error: BuildOneError) -> String {
    match error {
        BuildOneError::Message(message) => message,
        BuildOneError::Exit(code) => format!("REPL AOT compilation failed with exit code {code:?}"),
    }
}
