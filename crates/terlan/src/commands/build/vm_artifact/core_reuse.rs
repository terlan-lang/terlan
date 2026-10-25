//! Verified checked-CoreIR to native-image reuse before application lowering.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::compiler::native_ir::NativeCodegenPolicy;
use crate::runtime::native_boundary::adapter_abi::NativeAdapterAbiContract;
#[cfg(any(test, not(feature = "serve-runtime-bin")))]
use crate::runtime::native_image::aggregate_projection::NativeAggregateProjection;
use crate::runtime::native_image::{host_tvm_target, inspect_tvm_image};
use crate::terlan_typeck::CoreModule;

use super::super::{write_build_file, BuildOneError};
use super::native_cache;
use super::native_image::{
    native_linker_cache_identity, CompiledNativeApplicationImage, RootedNativeApplicationInput,
    DIRECT_AOT_BACKEND, DIRECT_AOT_BUILD_POLICY, DIRECT_AOT_CACHE_SCHEMA,
    DIRECT_AOT_CODEGEN_REVISION,
};
use super::output_cleanup;

const SCHEMA: &str = "terlan-checked-native-reuse-v1";
const MAX_STAMP_BYTES: u64 = 4 * 1024 * 1024;

/// One replaceable stamp per output identity avoids retaining each source edit.
pub(super) struct CoreReuse {
    identity: String,
    stamp_path: PathBuf,
    native_cache_root: PathBuf,
    image_stem: String,
    target: String,
    application: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Payload {
    schema: String,
    core_identity: String,
    native_identity: String,
    #[cfg(any(test, not(feature = "serve-runtime-bin")))]
    projections: Vec<NativeAggregateProjection>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stamp {
    payload: Payload,
    binding: String,
}

/// Streams serialization into SHA-256 instead of allocating an application JSON.
struct HashWriter(Sha256);

impl Write for HashWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.update(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(super) fn hash_serialized(value: &impl Serialize) -> Result<String, BuildOneError> {
    let mut writer = HashWriter(Sha256::new());
    serde_json::to_writer(&mut writer, value).map_err(|error| {
        BuildOneError::Message(format!("cannot hash native reuse input: {error}"))
    })?;
    Ok(format!("{:x}", writer.0.finalize()))
}

impl CoreReuse {
    /// Cache admission is optional; all failures retain the normal compiler path.
    pub(super) fn for_input(
        native_cache_root: &Path,
        image_stem: &str,
        cores: &[&CoreModule],
        input: &RootedNativeApplicationInput<'_>,
    ) -> Option<Self> {
        if !input.incremental || input.roots.is_empty() || !diagnostic_reuse_allowed() {
            return None;
        }
        let debug = input
            .debug_inputs
            .iter()
            .map(|input| input.cache_identity())
            .collect::<Result<Vec<_>, _>>()
            .ok()?;
        Self::prepare(
            native_cache_root,
            image_stem,
            cores,
            input.roots,
            &debug,
            input.policy,
        )
        .ok()
    }

    /// Called only after the frontend has admitted every module and dependency.
    pub(super) fn prepare(
        native_cache_root: &Path,
        image_stem: &str,
        cores: &[&CoreModule],
        roots: &[(String, String, usize)],
        debug_identities: &[String],
        policy: NativeCodegenPolicy,
    ) -> Result<Self, BuildOneError> {
        let target = host_tvm_target().map_err(|error| BuildOneError::Message(error.into()))?;
        let adapter = NativeAdapterAbiContract::current()
            .cache_identity(&target.triple, &target.calling_convention)
            .map_err(|error| BuildOneError::Message(error.into()))?;
        let mut modules = Vec::with_capacity(cores.len());
        for core in cores {
            // CoreModule deliberately omits its HIR interface from serde. Native
            // selected-import resolution additionally consumes these signatures.
            let signatures =
                crate::terlan_typeck::core_interface::lower_core_functions(&core.interface);
            modules.push(hash_serialized(&(core, signatures, core.interface_text()))?);
        }
        let identity = hash_serialized(&(
            SCHEMA,
            env!("CARGO_PKG_VERSION"),
            env!("TERLAN_CHECKED_FRONTEND_REVISION_SHA256"),
            DIRECT_AOT_BACKEND,
            DIRECT_AOT_CACHE_SCHEMA,
            DIRECT_AOT_CODEGEN_REVISION,
            DIRECT_AOT_BUILD_POLICY,
            policy.cache_identity(),
            (
                &target.triple,
                &target.architecture,
                &target.operating_system,
                &target.calling_convention,
            ),
            adapter,
            native_linker_cache_identity()?,
            image_stem,
            roots,
            modules,
            debug_identities,
        ))?;
        let slot = hash_serialized(&(image_stem, roots))?;
        let application = roots
            .first()
            .ok_or_else(|| {
                BuildOneError::Message("native reuse requires an executable root".into())
            })?
            .0
            .clone();
        Ok(Self {
            identity,
            stamp_path: native_cache_root.join(format!("core-reuse-{slot}.json")),
            native_cache_root: native_cache_root.to_path_buf(),
            image_stem: image_stem.to_string(),
            target: target.triple,
            application,
        })
    }

    fn read_stamp(&self) -> Option<Payload> {
        let metadata = fs::symlink_metadata(&self.stamp_path).ok()?;
        if !metadata.is_file() || metadata.len() > MAX_STAMP_BYTES {
            return None;
        }
        let mut bytes = Vec::new();
        fs::File::open(&self.stamp_path)
            .ok()?
            .take(MAX_STAMP_BYTES + 1)
            .read_to_end(&mut bytes)
            .ok()?;
        if bytes.len() as u64 > MAX_STAMP_BYTES {
            return None;
        }
        let stamp: Stamp = serde_json::from_slice(&bytes).ok()?;
        if stamp.payload.schema != SCHEMA
            || stamp.payload.core_identity != self.identity
            || !native_cache::is_sha256(&stamp.payload.native_identity)
            || hash_serialized(&stamp.payload).ok()? != stamp.binding
        {
            return None;
        }
        Some(stamp.payload)
    }

    /// A stamp is an index only: all native objects and the sealed image must
    /// still pass the ordinary manifest, digest, descriptor, and identity checks.
    pub(super) fn load(
        &self,
        vm_dir: &Path,
    ) -> Result<Option<CompiledNativeApplicationImage>, BuildOneError> {
        let Some(payload) = self.read_stamp() else {
            return Ok(None);
        };
        let directory = self.native_cache_root.join(&payload.native_identity);
        let suffix = if cfg!(target_os = "windows") {
            "obj"
        } else {
            "o"
        };
        let object = format!("{}.native.{suffix}", self.image_stem);
        let descriptor = format!("{}.descriptor.{suffix}", self.image_stem);
        let image_name = format!("{}.tvm", self.image_stem);
        let Some(bytes) = native_cache::load_verified_entry(
            &directory,
            &payload.native_identity,
            &self.target,
            DIRECT_AOT_BACKEND,
            &[&object, &descriptor, &image_name],
            &image_name,
        ) else {
            return Ok(None);
        };
        let Ok(image) = inspect_tvm_image(&bytes, &self.target) else {
            return Ok(None);
        };
        if image.descriptor.identity.build != format!("sha256:{}", payload.native_identity)
            || image.descriptor.identity.module != self.application
        {
            return Ok(None);
        }
        fs::create_dir_all(vm_dir).map_err(|error| {
            BuildOneError::Message(format!(
                "cannot create VM artifact directory `{}`: {error}",
                vm_dir.display()
            ))
        })?;
        write_build_file(&vm_dir.join(&image_name), &bytes, true)
            .map_err(BuildOneError::Message)?;
        output_cleanup::remove_stale_tvm_images(vm_dir, Some(&image_name))?;
        Ok(Some(CompiledNativeApplicationImage {
            cached_image_path: directory.join(&image_name),
            image_name,
            cache_input_sha256: payload.native_identity,
            #[cfg(any(test, not(feature = "serve-runtime-bin")))]
            aggregate_projections: payload.projections,
        }))
    }

    pub(super) fn publish(&self, image: &CompiledNativeApplicationImage) {
        // A missing optional index never invalidates a successfully built image.
        let _ = self.try_publish(image);
    }

    fn try_publish(&self, image: &CompiledNativeApplicationImage) -> Result<(), BuildOneError> {
        let payload = Payload {
            schema: SCHEMA.to_string(),
            core_identity: self.identity.clone(),
            native_identity: image.cache_input_sha256.clone(),
            #[cfg(any(test, not(feature = "serve-runtime-bin")))]
            projections: image.aggregate_projections.clone(),
        };
        let binding = hash_serialized(&payload)?;
        let bytes = serde_json::to_vec(&Stamp { payload, binding }).map_err(|error| {
            BuildOneError::Message(format!("cannot encode native reuse stamp: {error}"))
        })?;
        // Oversized optional metadata disables this fast path, not compilation.
        if bytes.len() as u64 > MAX_STAMP_BYTES {
            return Ok(());
        }
        native_cache::publish_file(&self.stamp_path, &bytes)
    }
}

/// Explicit diagnostic requests require executing the corresponding compiler work.
pub(super) fn diagnostic_reuse_allowed() -> bool {
    [
        "TERLAN_NATIVE_AOT_TRACE",
        "TERLAN_NATIVE_AOT_PROFILE_TRACE",
        "TERLAN_NATIVE_AOT_DUMP_FUNCTION",
    ]
    .iter()
    .all(|name| std::env::var_os(name).is_none())
}

#[cfg(test)]
#[path = "core_reuse_test.rs"]
mod tests;
