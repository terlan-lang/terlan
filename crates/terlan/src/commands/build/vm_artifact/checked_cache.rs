//! Verified compiler-private checked implementation cache.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::commands::artifacts::{
    collect_syntax_dependency_hashes, fingerprint, read_manifest, DependencyManifest,
};
use crate::formal_pipeline::CheckedSyntaxModuleArtifacts;
use crate::terlan_hir::syntax_module_output_to_interface;
use crate::terlan_syntax::syntax_contract_identity_matches_current;
use crate::CliState;

use super::super::{write_build_file, BuildOneError};
use super::native_cache;

const CHECKED_CACHE_SCHEMA: &str = "terlan-checked-implementation-v1";
const CHECKED_CACHE_BACKEND: &str = "terlan-frontend-v1";
const CHECKED_CACHE_FILE: &str = "checked.json";
const CHECKED_CACHE_TARGET: &str = "terlan-vm";
const MAX_CHECKED_CACHE_BYTES: u64 = 64 * 1024 * 1024;

/// Lossless compiler-private implementation payload retained between builds.
#[derive(Debug, Serialize, Deserialize)]
struct CheckedImplementationCache {
    /// Cache schema used to reject incompatible compiler payloads.
    schema: String,
    /// Exact compiler identity that produced the checked representation.
    compiler: String,
    /// Native policy used by target-profile validation.
    native_policy: String,
    /// Cryptographic identity of the implementation source bytes.
    source_sha256: String,
    /// Deterministic interface and dependency manifest for this implementation.
    dependency_manifest: String,
    /// Complete checked syntax contract used by debug and backend stages.
    syntax_output: crate::terlan_syntax::SyntaxModuleOutput,
    /// Complete backend-neutral implementation CoreIR.
    core: crate::terlan_typeck::CoreModule,
}

/// Loads one checked implementation only when every semantic input is current.
///
/// Inputs:
/// - `path`: current source path.
/// - `source`: current implementation bytes as UTF-8 text.
/// - `state`: compiler cache and native-policy configuration.
///
/// Output:
/// - Reconstructed checked artifacts for a verified cache hit.
/// - `None` for a miss, malformed entry, stale dependency, or poisoned file.
///
/// Transformation:
/// - Verifies the content-addressed cache publication, compiler/schema/policy,
///   source SHA-256, syntax contract, adjacent dependency manifest, current
///   imported interface hashes, and module identities before reconstructing
///   the intentionally omitted HIR interface from checked syntax output.
pub(super) fn load_checked_implementation(
    path: &str,
    source: &str,
    state: &CliState,
) -> Option<CheckedSyntaxModuleArtifacts> {
    let cache_dir = state.cache_dir.as_deref()?;
    let source_sha256 = native_cache::sha256_hex(source.as_bytes());
    let (identity, directory) = checked_cache_location(cache_dir, &source_sha256, state);
    let metadata = fs::metadata(directory.join(CHECKED_CACHE_FILE)).ok()?;
    if metadata.len() > MAX_CHECKED_CACHE_BYTES {
        return None;
    }
    let bytes = native_cache::load_verified_entry(
        &directory,
        &identity,
        CHECKED_CACHE_TARGET,
        CHECKED_CACHE_BACKEND,
        &[CHECKED_CACHE_FILE],
        CHECKED_CACHE_FILE,
    )?;
    let mut cached =
        crate::support::deserialize_json_with_depth_limit::<CheckedImplementationCache>(&bytes)
            .ok()?;
    if cached.schema != CHECKED_CACHE_SCHEMA
        || cached.compiler != compiler_identity()
        || cached.native_policy != native_policy_identity(state)
        || cached.source_sha256 != source_sha256
    {
        return None;
    }
    if !syntax_contract_identity_matches_current(&cached.syntax_output.syntax_contract).ok()? {
        return None;
    }
    let manifest = DependencyManifest::decode(&cached.dependency_manifest)?;
    if manifest.module != cached.syntax_output.module_name
        || manifest.module != cached.core.module
        || manifest.source_hash != fingerprint(source.as_bytes())
        || read_manifest(&dependency_manifest_path(cache_dir, &manifest.module))? != manifest
    {
        return None;
    }
    let interface = syntax_module_output_to_interface(&cached.syntax_output);
    if manifest.interface_hash != fingerprint(interface.to_terlan_interface_type_text().as_bytes())
        || manifest.interface_doc_hash
            != fingerprint(interface.to_terlan_interface_doc_text().as_bytes())
    {
        return None;
    }
    let interfaces = crate::formal_pipeline::load_external_interfaces_for_module(
        path,
        Some(cache_dir),
        &cached.syntax_output,
    );
    let dependencies = collect_syntax_dependency_hashes(
        &cached.syntax_output,
        &interfaces,
        Some(Path::new(path)),
        None,
    );
    if dependencies != manifest.dependencies {
        return None;
    }
    cached.core.interface = interface;
    cached.core.source.source_path = Some(path.to_string());
    Some(CheckedSyntaxModuleArtifacts {
        syntax_output: cached.syntax_output,
        interfaces,
        core: cached.core,
    })
}

/// Publishes one complete checked implementation and dependency manifest.
pub(super) fn publish_checked_implementation(
    path: &str,
    source: &str,
    compiled: &CheckedSyntaxModuleArtifacts,
    state: &CliState,
) -> Result<(), BuildOneError> {
    let Some(cache_dir) = state.cache_dir.as_deref() else {
        return Ok(());
    };
    fs::create_dir_all(cache_dir).map_err(|error| {
        BuildOneError::Message(format!(
            "error[build.checked_cache_directory]: cannot create `{}`: {error}",
            cache_dir.display()
        ))
    })?;
    let dependency_manifest = DependencyManifest {
        module: compiled.syntax_output.module_name.clone(),
        syntax_contract_identity: compiled.syntax_output.syntax_contract.clone(),
        source_hash: fingerprint(source.as_bytes()),
        interface_hash: fingerprint(
            compiled
                .core
                .interface
                .to_terlan_interface_type_text()
                .as_bytes(),
        ),
        interface_doc_hash: fingerprint(
            compiled
                .core
                .interface
                .to_terlan_interface_doc_text()
                .as_bytes(),
        ),
        dependencies: collect_syntax_dependency_hashes(
            &compiled.syntax_output,
            &compiled.interfaces,
            Some(Path::new(path)),
            None,
        ),
    };
    write_build_file(
        &dependency_manifest_path(cache_dir, &dependency_manifest.module),
        dependency_manifest.encode().as_bytes(),
        state.incremental,
    )
    .map_err(BuildOneError::Message)?;

    let source_sha256 = native_cache::sha256_hex(source.as_bytes());
    let (identity, directory) = checked_cache_location(cache_dir, &source_sha256, state);
    fs::create_dir_all(&directory).map_err(|error| {
        BuildOneError::Message(format!(
            "error[build.checked_cache_directory]: cannot create `{}`: {error}",
            directory.display()
        ))
    })?;
    let cached = CheckedImplementationCache {
        schema: CHECKED_CACHE_SCHEMA.to_string(),
        compiler: compiler_identity(),
        native_policy: native_policy_identity(state),
        source_sha256,
        dependency_manifest: dependency_manifest.encode(),
        syntax_output: compiled.syntax_output.clone(),
        core: compiled.core.clone(),
    };
    let bytes = serde_json::to_vec(&cached).map_err(|error| {
        BuildOneError::Message(format!(
            "error[build.checked_cache_encode]: cannot encode checked implementation: {error}"
        ))
    })?;
    let _lock = native_cache::CacheBuildLock::acquire(&directory)?;
    native_cache::publish_file(&directory.join(CHECKED_CACHE_FILE), &bytes)?;
    let manifest = native_cache::cache_manifest_bytes(
        &identity,
        CHECKED_CACHE_TARGET,
        CHECKED_CACHE_BACKEND,
        &[(CHECKED_CACHE_FILE, bytes.as_slice())],
    );
    native_cache::publish_file(
        &directory.join(native_cache::CACHE_MANIFEST_NAME),
        &manifest,
    )?;
    Ok(())
}

/// Returns the content identity and directory for one checked source profile.
fn checked_cache_location(
    cache_dir: &Path,
    source_sha256: &str,
    state: &CliState,
) -> (String, PathBuf) {
    let identity = native_cache::sha256_hex(
        format!(
            "{CHECKED_CACHE_SCHEMA}\0{}\0{}\0{source_sha256}",
            compiler_identity(),
            native_policy_identity(state)
        )
        .as_bytes(),
    );
    (identity.clone(), cache_dir.join("checked").join(identity))
}

/// Returns the current compiler identity embedded into checked cache entries.
fn compiler_identity() -> String {
    format!(
        "terlc-{}-{}-{}-{CHECKED_CACHE_SCHEMA}",
        env!("CARGO_PKG_VERSION"),
        env!("TERLAN_CHECKED_FRONTEND_REVISION_SHA256"),
        env!("TERLAN_NATIVE_BUILD_POLICY_SHA256")
    )
}

/// Returns the target validation policy identity used by checked CoreIR.
fn native_policy_identity(state: &CliState) -> String {
    format!("{:?}", state.native_policy)
}

/// Returns the canonical dependency manifest path for one module interface.
fn dependency_manifest_path(cache_dir: &Path, module: &str) -> PathBuf {
    cache_dir.join(format!("{module}.typi.deps"))
}

/// Returns the checked payload path for a source under the current test state.
///
/// Inputs:
/// - `source`: implementation source whose cache entry is inspected.
/// - `state`: test compiler state containing the cache directory and policy.
///
/// Output:
/// - Exact checked payload path, or `None` without a configured cache.
///
/// Transformation:
/// - Reuses production content-addressing so corruption tests target the same
///   entry that normal compilation would load.
#[cfg(test)]
pub(super) fn checked_cache_file_for_test(source: &str, state: &CliState) -> Option<PathBuf> {
    let cache_dir = state.cache_dir.as_deref()?;
    let source_sha256 = native_cache::sha256_hex(source.as_bytes());
    let (_, directory) = checked_cache_location(cache_dir, &source_sha256, state);
    Some(directory.join(CHECKED_CACHE_FILE))
}
