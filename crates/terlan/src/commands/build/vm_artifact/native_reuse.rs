//! Dependency-free incremental reuse backed only by verified native cache state.

use std::fs;
use std::path::{Path, PathBuf};

use crate::compiler::native_ir::NativeCodegenPolicy;
use crate::runtime::native_boundary::adapter_abi::NativeAdapterAbiContract;
use crate::runtime::native_image::{host_tvm_target, inspect_tvm_image};
use crate::terlan_syntax::SyntaxDeclarationPayload;
use crate::CliState;

use super::super::{write_build_file, BuildOneError};
use super::native_cache::{is_sha256, load_verified_entry, sha256_hex};
use super::native_image::{CompiledNativeApplicationImage, DIRECT_AOT_BACKEND};
use super::output_cleanup;

const REUSE_SCHEMA: &str = "terlan-native-reuse-v2";

#[derive(Clone, Debug, Eq, PartialEq)]
struct NativeReuseStamp {
    source_sha256: String,
    image_name: String,
    input_sha256: String,
    policy: String,
    target: String,
    adapter_abi: String,
}

/// Reuses one dependency-free native image without reading serialized VMIR.
pub(super) fn reuse_dependency_free_native_image(
    path: &str,
    state: &CliState,
    policy: NativeCodegenPolicy,
) -> Result<bool, BuildOneError> {
    if !state.incremental || state.no_emit {
        return Ok(false);
    }
    let Ok(source_text) = crate::support::read_file(path) else {
        return Ok(false);
    };
    let Ok(canonical_source) = fs::canonicalize(path) else {
        return Ok(false);
    };
    let Ok(syntax) = crate::formal_pipeline::parse_source_as_syntax_output(path, &source_text)
    else {
        return Ok(false);
    };
    if syntax
        .declarations
        .iter()
        .any(|declaration| matches!(declaration.payload, SyntaxDeclarationPayload::Import { .. }))
    {
        return Ok(false);
    }
    let module_stem = syntax.module_name.replace('.', "_");
    let vm_dir = state.out_dir.join("vm");
    let cache_root = native_cache_root(state);
    let stamp_path = reuse_stamp_path(&cache_root, &canonical_source);
    let Ok(stamp_text) = fs::read_to_string(&stamp_path) else {
        return Ok(false);
    };
    let Some(stamp) = parse_stamp(&stamp_text) else {
        return Ok(false);
    };
    let target = host_tvm_target().map_err(|error| BuildOneError::Message(error.into()))?;
    let adapter_abi = NativeAdapterAbiContract::current()
        .cache_identity(&target.triple, &target.calling_convention)
        .map_err(|error| BuildOneError::Message(error.into()))?;
    if stamp.source_sha256 != source_identity(&canonical_source, &source_text, policy)
        || stamp.image_name != format!("{module_stem}.tvm")
        || stamp.policy != policy.cache_identity()
        || stamp.target != target.triple
        || stamp.adapter_abi != adapter_abi
    {
        return Ok(false);
    }

    let cache_dir = cache_root.join(&stamp.input_sha256);
    let object_name = native_object_name(&module_stem);
    let descriptor_name = descriptor_object_name(&module_stem);
    let file_names = [
        object_name.as_str(),
        descriptor_name.as_str(),
        stamp.image_name.as_str(),
    ];
    let Some(cached_image) = load_verified_entry(
        &cache_dir,
        &stamp.input_sha256,
        &target.triple,
        DIRECT_AOT_BACKEND,
        &file_names,
        &stamp.image_name,
    ) else {
        return Ok(false);
    };
    let Ok(inspection) = inspect_tvm_image(&cached_image, &target.triple) else {
        return Ok(false);
    };
    if inspection.descriptor.identity.build != format!("sha256:{}", stamp.input_sha256) {
        return Ok(false);
    }
    fs::create_dir_all(&vm_dir).map_err(|error| {
        BuildOneError::Message(format!(
            "cannot create VM artifact directory `{}`: {error}",
            vm_dir.display()
        ))
    })?;
    write_build_file(
        &vm_dir.join(&stamp.image_name),
        &cached_image,
        state.incremental,
    )
    .map_err(BuildOneError::Message)?;
    output_cleanup::remove_stale_tvm_images(&vm_dir, Some(&stamp.image_name))?;
    Ok(true)
}

/// Publishes the source-to-native-cache identity used by warm incremental builds.
pub(super) fn write_native_reuse_stamp(
    source_path: &str,
    source_text: &str,
    state: &CliState,
    image: &CompiledNativeApplicationImage,
    policy: NativeCodegenPolicy,
) -> Result<(), BuildOneError> {
    let canonical_source = fs::canonicalize(source_path).map_err(|error| {
        BuildOneError::Message(format!(
            "cannot canonicalize native reuse source `{source_path}`: {error}"
        ))
    })?;
    image.image_name.strip_suffix(".tvm").ok_or_else(|| {
        BuildOneError::Message(format!(
            "native reuse image `{}` has no .tvm suffix",
            image.image_name
        ))
    })?;
    let target = host_tvm_target().map_err(|error| BuildOneError::Message(error.into()))?;
    let adapter_abi = NativeAdapterAbiContract::current()
        .cache_identity(&target.triple, &target.calling_convention)
        .map_err(|error| BuildOneError::Message(error.into()))?;
    let stamp = render_stamp(&NativeReuseStamp {
        source_sha256: source_identity(&canonical_source, source_text, policy),
        image_name: image.image_name.clone(),
        input_sha256: image.cache_input_sha256.clone(),
        policy: policy.cache_identity().to_string(),
        target: target.triple,
        adapter_abi,
    })
    .map_err(BuildOneError::Message)?;
    let cache_root = native_cache_root(state);
    write_build_file(
        &reuse_stamp_path(&cache_root, &canonical_source),
        stamp.as_bytes(),
        state.incremental,
    )
    .map_err(BuildOneError::Message)
}

/// Returns the compiler-owned native cache root outside published artifacts.
fn native_cache_root(state: &CliState) -> PathBuf {
    state
        .cache_dir
        .clone()
        .unwrap_or_else(|| state.out_dir.join(".terlan"))
        .join("native-aot")
}

/// Returns a source-specific reuse stamp path inside compiler-owned cache state.
fn reuse_stamp_path(cache_root: &Path, source: &Path) -> PathBuf {
    let source_key = sha256_hex(source.to_string_lossy().as_bytes());
    cache_root.join(format!("reuse-{source_key}.stamp"))
}

fn parse_stamp(text: &str) -> Option<NativeReuseStamp> {
    let mut lines = text.lines();
    if lines.next()? != REUSE_SCHEMA {
        return None;
    }
    let source_sha256 = lines.next()?.strip_prefix("source-sha256 ")?.to_string();
    let image_name = lines.next()?.strip_prefix("image ")?.to_string();
    let input_sha256 = lines.next()?.strip_prefix("input-sha256 ")?.to_string();
    let policy = lines.next()?.strip_prefix("policy ")?.to_string();
    let target = lines.next()?.strip_prefix("target ")?.to_string();
    let adapter_abi = lines.next()?.strip_prefix("adapter-abi ")?.to_string();
    let binding_sha256 = lines.next()?.strip_prefix("binding-sha256 ")?;
    if lines.next().is_some()
        || !is_sha256(&source_sha256)
        || !is_sha256(&input_sha256)
        || !is_image_name(&image_name)
        || !is_identity_text(&policy)
        || !is_identity_text(&target)
        || !is_identity_text(&adapter_abi)
        || binding_sha256
            != stamp_binding(
                &source_sha256,
                &image_name,
                &input_sha256,
                &policy,
                &target,
                &adapter_abi,
            )
    {
        return None;
    }
    Some(NativeReuseStamp {
        source_sha256,
        image_name,
        input_sha256,
        policy,
        target,
        adapter_abi,
    })
}

/// Renders one strict source-to-image reuse binding.
fn render_stamp(stamp: &NativeReuseStamp) -> Result<String, String> {
    if !is_sha256(&stamp.source_sha256)
        || !is_sha256(&stamp.input_sha256)
        || !is_image_name(&stamp.image_name)
        || !is_identity_text(&stamp.policy)
        || !is_identity_text(&stamp.target)
        || !is_identity_text(&stamp.adapter_abi)
    {
        return Err("error[tvm.cache.reuse_stamp]: native reuse identity is not canonical".into());
    }
    let binding = stamp_binding(
        &stamp.source_sha256,
        &stamp.image_name,
        &stamp.input_sha256,
        &stamp.policy,
        &stamp.target,
        &stamp.adapter_abi,
    );
    Ok(format!(
        "{REUSE_SCHEMA}\nsource-sha256 {}\nimage {}\ninput-sha256 {}\npolicy {}\ntarget {}\nadapter-abi {}\nbinding-sha256 {binding}\n",
        stamp.source_sha256,
        stamp.image_name,
        stamp.input_sha256,
        stamp.policy,
        stamp.target,
        stamp.adapter_abi,
    ))
}

/// Derives the integrity binding over every reuse-admission field.
fn stamp_binding(
    source_sha256: &str,
    image_name: &str,
    input_sha256: &str,
    policy: &str,
    target: &str,
    adapter_abi: &str,
) -> String {
    sha256_hex(
        format!(
            "{REUSE_SCHEMA}\0{source_sha256}\0{image_name}\0{input_sha256}\0{policy}\0{target}\0{adapter_abi}"
        )
        .as_bytes(),
    )
}

/// Rejects path traversal and non-TVM names in cache index records.
fn is_image_name(value: &str) -> bool {
    !value.is_empty()
        && value.ends_with(".tvm")
        && Path::new(value).components().count() == 1
        && Path::new(value).file_name().and_then(|name| name.to_str()) == Some(value)
}

/// Rejects empty or delimiter-bearing line-protocol values.
fn is_identity_text(value: &str) -> bool {
    !value.is_empty() && !value.contains(['\0', '\n', '\r'])
}

fn source_identity(path: &Path, source_text: &str, policy: NativeCodegenPolicy) -> String {
    sha256_hex(
        format!(
            "{REUSE_SCHEMA}\0{}\0{}\0{}\0{}",
            env!("CARGO_PKG_VERSION"),
            policy.cache_identity(),
            path.display(),
            source_text
        )
        .as_bytes(),
    )
}

fn native_object_name(stem: &str) -> String {
    if cfg!(target_os = "windows") {
        format!("{stem}.native.obj")
    } else {
        format!("{stem}.native.o")
    }
}

fn descriptor_object_name(stem: &str) -> String {
    if cfg!(target_os = "windows") {
        format!("{stem}.descriptor.obj")
    } else {
        format!("{stem}.descriptor.o")
    }
}

#[cfg(test)]
#[path = "native_reuse_test.rs"]
#[cfg(test)]
mod native_reuse_test;
