//! Compiler-owned native image construction independent from JSON artifacts.

#[path = "native_image/linker.rs"]
mod linker;
use linker::*;

use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

#[cfg(target_os = "windows")]
use crate::runtime::native_boundary::dispatch::capture_tool_command;
use crate::runtime::native_boundary::dispatch::capture_tool_command_with_launch;

#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
use crate::compiler::native_ir::native_aggregate_projections;
use crate::compiler::native_ir::{
    emit_native_application_dispatch_object_with_policy,
    emit_native_application_object_with_policy, install_native_aggregate_projection_exports,
    NativeCodegenPolicy, NativeModule, DISPATCH_SYMBOL, IMAGE_ENTRY_SYMBOL,
};
use crate::runtime::native_boundary::adapter_abi::NativeAdapterAbiContract;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
use crate::runtime::native_image::aggregate_projection::NativeAggregateProjection;
use crate::runtime::native_image::{
    descriptor_object_for_native_with_debug, host_tvm_target, inspect_tvm_image, seal_tvm_image,
};
use crate::terlan_typeck::CoreModule;

use super::super::{write_build_file, BuildOneError};
use super::build_activity::{Activity, Operation};
use super::native_debug::{encode_native_debug, NativeDebugInput};
use super::native_descriptor::native_application_image_descriptor;
use super::native_units::prepare_native_object_units;
use super::{native_cache, output_cleanup};

pub(super) const DIRECT_AOT_BACKEND: &str = "cranelift-0.134.4";
pub(super) const DIRECT_AOT_CACHE_SCHEMA: &str = "terlan-native-codegen-v5";
pub(super) const DIRECT_AOT_CODEGEN_REVISION: &str = env!("TERLAN_NATIVE_CODEGEN_REVISION_SHA256");
pub(super) const DIRECT_AOT_BUILD_POLICY: &str = env!("TERLAN_NATIVE_BUILD_POLICY_SHA256");

/// One compiler-owned native application image independent from transitional
/// per-module artifact envelopes.
#[derive(Clone, Debug)]
pub(super) struct CompiledNativeApplicationImage {
    pub(super) image_name: String,
    pub(super) cache_input_sha256: String,
    pub(super) cached_image_path: PathBuf,
    #[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
    pub(super) aggregate_projections: Vec<NativeAggregateProjection>,
}

/// Live-serve image plus compiler proof metadata that is deliberately not part
/// of the frozen TVM image descriptor format.
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) struct CompiledServeNativeImage {
    pub(crate) path: PathBuf,
    #[cfg(any(test, not(feature = "serve-runtime-bin")))]
    pub(crate) aggregate_projections: Vec<NativeAggregateProjection>,
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
    cores: NativeApplicationCores<'a>,
    debug_inputs: &'a [NativeDebugInput],
    policy: NativeCodegenPolicy,
    incremental: bool,
    application_identity: Option<&'a str>,
}

enum NativeApplicationCores<'a> {
    Borrowed(&'a [&'a CoreModule]),
    Owned(Vec<CoreModule>),
}

pub(super) struct RootedNativeApplicationInput<'a> {
    pub(super) roots: &'a [(String, String, usize)],
    pub(super) debug_inputs: &'a [NativeDebugInput],
    pub(super) policy: NativeCodegenPolicy,
    pub(super) incremental: bool,
}

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

/// Compiles all supported CoreIR modules into one cached application image.
pub(super) fn compile_native_application_image(
    vm_dir: &Path,
    native_cache_root: &Path,
    image_stem: &str,
    cores: &[&CoreModule],
    debug_inputs: &[NativeDebugInput],
    policy: NativeCodegenPolicy,
    incremental: bool,
) -> Result<Option<CompiledNativeApplicationImage>, BuildOneError> {
    compile_native_application_image_with_identity(NativeApplicationCompileRequest {
        vm_dir,
        native_cache_root,
        image_stem,
        cores: NativeApplicationCores::Borrowed(cores),
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
    let mut natives = match cores {
        NativeApplicationCores::Borrowed(cores) => NativeModule::lower_application(cores),
        NativeApplicationCores::Owned(cores) => NativeModule::lower_owned_application(cores),
    }
    .map_err(BuildOneError::Message)?;
    if natives.is_empty() {
        output_cleanup::remove_stale_tvm_images(vm_dir, None)?;
        return Ok(None);
    }
    // `NativeModule::lower_application` returns the canonical function-index
    // order, including synthetic closure and continuation modules. Reordering
    // modules here would invalidate every application-wide direct-call index.
    validate_export_id_uniqueness(&natives)?;
    #[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
    let aggregate_projections = if policy == NativeCodegenPolicy::Serve {
        install_native_aggregate_projection_exports(&mut natives)
    } else {
        native_aggregate_projections(&natives)
    };
    #[cfg(feature = "serve-runtime-bin")]
    if policy == NativeCodegenPolicy::Serve {
        install_native_aggregate_projection_exports(&mut natives);
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
        #[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
        aggregate_projections,
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
    let reuse =
        super::core_reuse::CoreReuse::for_input(native_cache_root, image_stem, cores, &input);
    if let Some(reuse) = &reuse {
        if let Some(image) = reuse.load(vm_dir)? {
            return Ok(Some(image));
        }
    }
    let trace_timings = std::env::var_os("TERLAN_NATIVE_AOT_TIMINGS").is_some();
    let started = Instant::now();
    let mut previous = started;
    let mut mark = |phase: &str| {
        if trace_timings {
            let now = Instant::now();
            eprintln!(
                "terlc native-aot: rooted.{phase}: +{}ms total={}ms",
                now.duration_since(previous).as_millis(),
                now.duration_since(started).as_millis(),
            );
            previous = now;
        }
    };
    let RootedNativeApplicationInput {
        roots,
        debug_inputs,
        policy,
        incremental,
    } = input;
    let mut rooted = crate::compiler::native_ir::rooted_application_to_function_roots(cores, roots)
        .map_err(|error| BuildOneError::Message(error.to_string()))?;
    mark("borrowed-root-pruning");
    crate::compiler::native_ir::resolve_typed_mutable_receiver_calls(&mut rooted)
        .map_err(|error| BuildOneError::Message(error.to_string()))?;
    mark("mutable-receivers");
    crate::compiler::native_ir::prune_application_to_function_roots(&mut rooted, roots)
        .map_err(|error| BuildOneError::Message(error.to_string()))?;
    mark("root-pruning");
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
    let image = compile_native_application_image_with_identity(NativeApplicationCompileRequest {
        vm_dir,
        native_cache_root,
        image_stem,
        cores: NativeApplicationCores::Borrowed(&rooted),
        debug_inputs,
        policy,
        incremental,
        application_identity: Some(application_identity),
    })?;
    if let (Some(reuse), Some(image)) = (&reuse, &image) {
        reuse.publish(image);
    }
    Ok(image)
}

/// Compiles an owned CoreIR closure after pruning it in place to executable roots.
///
/// Compiler-sized package builds use this path so rooted lowering never keeps
/// both the checked Core closure and a near-identical rooted clone alive.
pub(super) fn compile_owned_rooted_native_application_image(
    vm_dir: &Path,
    native_cache_root: &Path,
    image_stem: &str,
    mut cores: Vec<CoreModule>,
    input: RootedNativeApplicationInput<'_>,
) -> Result<Option<CompiledNativeApplicationImage>, BuildOneError> {
    let reuse = super::core_reuse::CoreReuse::for_input(
        native_cache_root,
        image_stem,
        &cores.iter().collect::<Vec<_>>(),
        &input,
    );
    if let Some(reuse) = &reuse {
        if let Some(image) = reuse.load(vm_dir)? {
            return Ok(Some(image));
        }
    }
    let RootedNativeApplicationInput {
        roots,
        debug_inputs,
        policy,
        incremental,
    } = input;
    crate::compiler::native_ir::prune_application_to_function_roots(&mut cores, roots)
        .map_err(|error| BuildOneError::Message(error.to_string()))?;
    crate::compiler::native_ir::resolve_typed_mutable_receiver_calls(&mut cores)
        .map_err(|error| BuildOneError::Message(error.to_string()))?;
    crate::compiler::native_ir::prune_application_to_function_roots(&mut cores, roots)
        .map_err(|error| BuildOneError::Message(error.to_string()))?;
    let application_identity = roots
        .first()
        .map(|(module, _, _)| module.as_str())
        .ok_or_else(|| {
            BuildOneError::Message(
                "error[native_ir.root]: rooted native application requires an executable root"
                    .to_string(),
            )
        })?;
    let image = compile_native_application_image_with_identity(NativeApplicationCompileRequest {
        vm_dir,
        native_cache_root,
        image_stem,
        cores: NativeApplicationCores::Owned(cores),
        debug_inputs,
        policy,
        incremental,
        application_identity: Some(application_identity),
    })?;
    if let (Some(reuse), Some(image)) = (&reuse, &image) {
        reuse.publish(image);
    }
    Ok(image)
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
    let debug_inputs = [NativeDebugInput::capture(
        source_path,
        source_text,
        core,
        syntax,
    )];
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

#[cfg(test)]
mod serve_fixture;
#[cfg(test)]
pub(crate) use serve_fixture::compile_serve_native_image;

/// Compiles a live-serve image from the complete application closure while
/// retaining generic aggregate observations for package ingress admission.
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(super) fn compile_serve_native_application_image_with_metadata(
    web_root: &Path,
    module_stem: &str,
    cores: &[&CoreModule],
    roots: &[(String, String, usize)],
    debug_inputs: &[NativeDebugInput],
) -> Result<Option<CompiledServeNativeImage>, String> {
    let workspace = web_root.join(".terlan").join("serve-aot");
    let vm_dir = workspace.join("vm");
    fs::create_dir_all(&vm_dir)
        .map_err(|error| format!("cannot create serve AOT output directory: {error}"))?;
    let native_cache_root = workspace.join("native-aot");
    compile_rooted_native_application_image(
        &vm_dir,
        &native_cache_root,
        module_stem,
        cores,
        RootedNativeApplicationInput {
            roots,
            debug_inputs,
            policy: NativeCodegenPolicy::Serve,
            incremental: true,
        },
    )
    .map(|image| {
        image.map(|image| CompiledServeNativeImage {
            path: image.cached_image_path,
            #[cfg(any(test, not(feature = "serve-runtime-bin")))]
            aggregate_projections: image.aggregate_projections,
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
    roots: &[(String, String, usize)],
    debug_inputs: &[NativeDebugInput],
    incremental: bool,
) -> Result<Option<PathBuf>, String> {
    let vm_dir = workspace.join("vm");
    fs::create_dir_all(&vm_dir)
        .map_err(|error| format!("cannot create test AOT output directory: {error}"))?;
    compile_rooted_native_application_image(
        &vm_dir,
        native_cache_root,
        module_stem,
        cores,
        RootedNativeApplicationInput {
            roots,
            debug_inputs,
            policy: NativeCodegenPolicy::Development,
            incremental,
        },
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
    let activity = Activity::begin(Operation::ApplicationObject, input.input_sha256)?;
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
    .map_err(|error| BuildOneError::Message(error.into()));
    activity.finish(
        object.is_ok(),
        object.as_ref().ok().map(|bytes| bytes.len() as u64),
    )?;
    let object = object?;
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
    let unit_links = module_units
        .as_ref()
        .map(|units| units.paths.as_slice())
        .unwrap_or_default()
        .iter()
        .enumerate()
        .map(|(index, source)| {
            native_cache::TemporaryCacheFile::linked_beside(
                source,
                &input.native_dir.join(format!("link-input-{index}.o")),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    // A surviving linker must not depend on the parent compiler's lock alone.
    // These private names survive parent termination even if canonical units
    // are subsequently retired. They never alias mutable staging payloads.
    let unit_paths = unit_links
        .iter()
        .map(|link| link.path().to_path_buf())
        .collect::<Vec<_>>();
    link_native_image(
        &unit_paths,
        input.object_path,
        input.descriptor_object_path,
        linked_image.path(),
        input.linker_policy,
        input.policy,
        input.input_sha256,
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

pub(super) fn native_linker_cache_identity() -> Result<&'static str, BuildOneError> {
    native_linker_policy().map(|policy| policy.cache_identity.as_str())
}

/// Reports host AOT tool identity without parsing sources or compiling an image.
pub(in crate::commands::build) fn print_toolchain_identity() -> Result<(), BuildOneError> {
    let target = host_tvm_target().map_err(|error| BuildOneError::Message(error.into()))?;
    let adapter = NativeAdapterAbiContract::current()
        .cache_identity(&target.triple, &target.calling_convention)
        .map_err(|error| BuildOneError::Message(error.into()))?;
    let linker = native_linker_cache_identity()?;
    println!(
        "{}",
        serde_json::json!({
            "schema": "terlan.native-toolchain-identity.v1",
            "compiler_version": env!("CARGO_PKG_VERSION"),
            "backend": DIRECT_AOT_BACKEND,
            "cache_schema": DIRECT_AOT_CACHE_SCHEMA,
            "codegen_revision": DIRECT_AOT_CODEGEN_REVISION,
            "build_policy": DIRECT_AOT_BUILD_POLICY,
            "target": target.triple,
            "architecture": target.architecture,
            "operating_system": target.operating_system,
            "calling_convention": target.calling_convention,
            "adapter_abi": adapter,
            "linker_identity": linker,
        })
    );
    Ok(())
}

fn build_error_message(error: BuildOneError) -> String {
    match error {
        BuildOneError::Message(message) => message,
        BuildOneError::Exit(code) => format!("REPL AOT compilation failed with exit code {code:?}"),
    }
}
