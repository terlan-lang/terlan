use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use terlan_typeck::{CoreImport, CoreImportKind};

use super::beam_runner::emit_compiled_module_source_to_workspace;
use crate::validation::target_profile::TargetProfile;
use crate::CliState;

const RELEASE_SUPPORT_CACHE_MARKER: &str = ".complete";
const RELEASE_SUPPORT_CACHE_SCHEMA: &str = "release-support-cache-v1";

/// Embedded source for one release support module used by `terlc test`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ReleaseSupportModule {
    pub(super) path: &'static str,
    pub(super) source: &'static str,
}

/// Emits and compiles release support modules for runtime tests.
///
/// Inputs:
/// - `workspace`: temporary directory for emitted and compiled artifacts.
/// - `state`: borrowed global CLI state used by formal compilation.
/// - `primary_module_name`: module under test, used to avoid recompiling it as
///   support.
/// - `imports`: CoreIR imports visible from the module under test.
/// - `source`: source text for the module under test.
///
/// Output:
/// - `Ok(())` when all selected support `.beam` files are available.
/// - `Err(message)` when a support source is missing or compilation fails.
///
/// Transformation:
/// - Compiles the explicit embedded release support inventory for runtime
///   tests, recursively follows embedded std imports, emits Erlang, and
///   invokes `erlc`. The inventory is intentionally small and avoids missing
///   backend-introduced support calls that do not appear as source imports.
pub(super) fn emit_and_compile_release_support_modules(
    workspace: &Path,
    state: &CliState,
    primary_module_name: &str,
    imports: &[CoreImport],
    source: &str,
) -> Result<(), String> {
    let support_by_name = release_support_modules_by_name();
    let selected = selected_release_support_module_names(
        &support_by_name,
        primary_module_name,
        imports,
        source,
    );
    let cache_dir = release_support_cache_dir(state.cache_dir.as_deref(), &support_by_name)?;
    ensure_release_support_cache(
        &cache_dir,
        state,
        primary_module_name,
        &selected,
        &support_by_name,
    )?;
    copy_cached_release_support_beams(&cache_dir, workspace)?;
    verify_workspace_release_support_beams(workspace, &selected)
}

/// Selects release support module names for a runtime test workspace.
///
/// Inputs:
/// - `support_by_name`: embedded release support source index.
/// - `primary_module_name`: module under test, excluded from support output.
/// - `imports`: CoreIR imports visible from the module under test.
/// - `source`: source text for the module under test.
///
/// Output:
/// - Sorted support module names that should be available to the runtime test.
///
/// Transformation:
/// - Starts with the full embedded release support inventory to preserve the
///   installed test runner's broad std availability, then adds explicit source
///   and CoreIR references while excluding the module under test.
pub(super) fn selected_release_support_module_names(
    support_by_name: &BTreeMap<String, &'static ReleaseSupportModule>,
    primary_module_name: &str,
    imports: &[CoreImport],
    source: &str,
) -> BTreeSet<String> {
    let mut pending = support_by_name.keys().cloned().collect::<BTreeSet<_>>();
    pending.extend(source_release_support_module_names(source, support_by_name));
    pending.extend(direct_release_support_module_names(
        imports,
        support_by_name,
    ));
    pending.remove(primary_module_name);
    pending
}

/// Ensures compiled release support BEAM files exist in a persistent cache.
///
/// Inputs:
/// - `cache_dir`: fingerprinted support cache directory.
/// - `state`: borrowed global CLI state used by formal compilation.
/// - `primary_module_name`: module under test, used to avoid recompiling it as
///   support.
/// - `selected`: support module names requested by this runtime test.
/// - `support_by_name`: embedded release support source index.
///
/// Output:
/// - `Ok(())` when the cache contains a complete set of compiled support BEAM
///   files.
/// - `Err(message)` when cache cleanup, source compilation, or cache finalizing
///   fails.
///
/// Transformation:
/// - Reuses a complete fingerprinted cache when present; otherwise rebuilds the
///   selected support modules into the cache directory and writes a completion
///   marker after successful compilation.
fn ensure_release_support_cache(
    cache_dir: &Path,
    state: &CliState,
    primary_module_name: &str,
    selected: &BTreeSet<String>,
    support_by_name: &BTreeMap<String, &'static ReleaseSupportModule>,
) -> Result<(), String> {
    if cache_is_complete(cache_dir, selected)? {
        return Ok(());
    }

    if cache_dir.exists() {
        fs::remove_dir_all(cache_dir).map_err(|err| {
            format!(
                "failed to remove incomplete release support cache {}: {err}",
                cache_dir.display()
            )
        })?;
    }
    fs::create_dir_all(cache_dir).map_err(|err| {
        format!(
            "failed to create release support cache {}: {err}",
            cache_dir.display()
        )
    })?;

    compile_release_support_modules_into(
        cache_dir,
        state,
        primary_module_name,
        selected,
        support_by_name,
    )?;
    fs::write(
        cache_dir.join(RELEASE_SUPPORT_CACHE_MARKER),
        RELEASE_SUPPORT_CACHE_SCHEMA,
    )
    .map_err(|err| {
        format!(
            "failed to write release support cache marker {}: {err}",
            cache_dir.display()
        )
    })
}

/// Compiles selected support modules into one output directory.
///
/// Inputs:
/// - `output_dir`: directory receiving generated Erlang and BEAM files.
/// - `state`: borrowed global CLI state used by formal compilation.
/// - `primary_module_name`: module under test, excluded from support output.
/// - `selected`: initial support module names to compile.
/// - `support_by_name`: embedded release support source index.
///
/// Output:
/// - `Ok(())` when all selected support modules and their embedded
///   dependencies compile.
/// - `Err(message)` when a support module fails formal compilation or Erlang
///   emission.
///
/// Transformation:
/// - Runs the same formal support compilation path used before caching, while
///   allowing dependency discovery to enqueue additional embedded support
///   modules when the inventory changes.
fn compile_release_support_modules_into(
    output_dir: &Path,
    state: &CliState,
    primary_module_name: &str,
    selected: &BTreeSet<String>,
    support_by_name: &BTreeMap<String, &'static ReleaseSupportModule>,
) -> Result<(), String> {
    let mut pending = selected.clone();
    let mut compiled_names = BTreeSet::new();

    while let Some(module_name) = pending.pop_first() {
        if module_name == primary_module_name || !compiled_names.insert(module_name.clone()) {
            continue;
        }
        let Some(module) = support_by_name.get(&module_name).copied() else {
            continue;
        };
        let compiled = crate::formal_pipeline::compile_syntax_module_through_phases_with_profile(
            module.path,
            module.source,
            state.diagnostic_format,
            state.cache_dir.as_deref(),
            state.native_policy,
            TargetProfile::Erlang,
        )
        .map_err(|exit_code| {
            format!(
                "release support module {} failed with exit code {exit_code:?}",
                module.path
            )
        })?;
        for dependency in
            direct_release_support_module_names(&compiled.core.imports, support_by_name)
        {
            if dependency != primary_module_name && !compiled_names.contains(&dependency) {
                pending.insert(dependency);
            }
        }
        emit_compiled_module_source_to_workspace(
            module.path,
            module.source,
            output_dir,
            &compiled,
            &[],
        )?;
    }
    Ok(())
}

/// Copies compiled support BEAM files from cache into a test workspace.
///
/// Inputs:
/// - `cache_dir`: complete release support cache directory.
/// - `workspace`: temporary BEAM workspace for the active test run.
///
/// Output:
/// - `Ok(())` when at least one cached `.beam` file is copied.
/// - `Err(message)` when cache listing, file copy, or cache completeness checks
///   fail.
///
/// Transformation:
/// - Copies target-ready BEAM artifacts without recompiling embedded support
///   source on every editor test invocation.
fn copy_cached_release_support_beams(cache_dir: &Path, workspace: &Path) -> Result<(), String> {
    let mut copied = 0usize;
    for entry in fs::read_dir(cache_dir).map_err(|err| {
        format!(
            "failed to read release support cache {}: {err}",
            cache_dir.display()
        )
    })? {
        let entry = entry.map_err(|err| {
            format!(
                "failed to read release support cache entry in {}: {err}",
                cache_dir.display()
            )
        })?;
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("beam") {
            continue;
        }
        let target = workspace.join(entry.file_name());
        fs::copy(&path, &target).map_err(|err| {
            format!(
                "failed to copy cached release support {} to {}: {err}",
                path.display(),
                target.display()
            )
        })?;
        copied += 1;
    }
    if copied == 0 {
        Err(format!(
            "release support cache {} is complete but contains no BEAM files",
            cache_dir.display()
        ))
    } else {
        Ok(())
    }
}

/// Verifies copied release support BEAMs are present in the active workspace.
///
/// Inputs:
/// - `workspace`: temporary directory passed to Erlang as the BEAM code path.
/// - `selected`: support module names required by the current test run.
///
/// Output:
/// - `Ok(())` when every selected support module has a copied `.beam` file.
/// - `Err(message)` naming the first missing module otherwise.
///
/// Transformation:
/// - Converts the support module name through the same Erlang output-stem
///   mapping used during compilation and checks the runtime-visible workspace,
///   preventing low-level `{undef, ...}` crashes for missing support modules.
fn verify_workspace_release_support_beams(
    workspace: &Path,
    selected: &BTreeSet<String>,
) -> Result<(), String> {
    for module_name in selected {
        let beam_path = workspace.join(format!(
            "{}.beam",
            crate::support::erlang_output_stem(module_name)
        ));
        if !beam_path.is_file() {
            return Err(format!(
                "release support module `{module_name}` was not copied to runtime workspace {}; missing {}",
                workspace.display(),
                beam_path.display()
            ));
        }
    }
    Ok(())
}

/// Returns whether a release support cache directory is complete.
///
/// Inputs:
/// - `cache_dir`: fingerprinted support cache directory.
///
/// Output:
/// - `selected`: support module names required by the active test run.
///
/// Output:
/// - `Ok(true)` when the completion marker is present and every selected
///   support module has a matching `.beam` file.
/// - `Ok(false)` when the directory is absent, unmarked, empty, or missing any
///   selected module output.
/// - `Err(message)` when listing an existing cache directory fails.
///
/// Transformation:
/// - Treats the marker plus exact selected BEAM presence as the durable
///   cache-completeness contract so older partial caches are rebuilt instead
///   of leaking backend `undef` errors.
fn cache_is_complete(cache_dir: &Path, selected: &BTreeSet<String>) -> Result<bool, String> {
    if !cache_dir.join(RELEASE_SUPPORT_CACHE_MARKER).is_file() {
        return Ok(false);
    }
    let mut has_beam = false;
    for entry in fs::read_dir(cache_dir).map_err(|err| {
        format!(
            "failed to inspect release support cache {}: {err}",
            cache_dir.display()
        )
    })? {
        let entry = entry.map_err(|err| {
            format!(
                "failed to inspect release support cache entry in {}: {err}",
                cache_dir.display()
            )
        })?;
        if entry
            .path()
            .extension()
            .and_then(|extension| extension.to_str())
            == Some("beam")
        {
            has_beam = true;
            break;
        }
    }
    if !has_beam {
        return Ok(false);
    }
    Ok(selected.iter().all(|module_name| {
        cache_dir
            .join(format!(
                "{}.beam",
                crate::support::erlang_output_stem(module_name)
            ))
            .is_file()
    }))
}

/// Builds the persistent release support cache directory.
///
/// Inputs:
/// - `explicit_cache_dir`: optional global `--cache-dir` value.
/// - `support_by_name`: embedded release support source index.
///
/// Output:
/// - Fingerprinted cache path for the current compiler and embedded support
///   source inventory.
///
/// Transformation:
/// - Uses the explicit compiler cache directory when provided; otherwise uses
///   XDG cache, HOME cache, or the system temp directory, then appends a stable
///   support-source fingerprint.
fn release_support_cache_dir(
    explicit_cache_dir: Option<&Path>,
    support_by_name: &BTreeMap<String, &'static ReleaseSupportModule>,
) -> Result<PathBuf, String> {
    let root = explicit_cache_dir
        .map(Path::to_path_buf)
        .unwrap_or_else(default_release_support_cache_root);
    Ok(root
        .join("test-release-support")
        .join(release_support_fingerprint(support_by_name)?))
}

/// Returns the default release support cache root.
///
/// Inputs:
/// - `XDG_CACHE_HOME` and `HOME` environment variables when present.
///
/// Output:
/// - Cache root path for persistent `terlc test` support artifacts.
///
/// Transformation:
/// - Follows XDG cache conventions on Unix-like systems and falls back to the
///   system temp directory when no user cache directory is discoverable.
fn default_release_support_cache_root() -> PathBuf {
    if let Some(path) = std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from) {
        return path.join("terlan");
    }
    if let Some(path) = std::env::var_os("HOME").map(PathBuf::from) {
        return path.join(".cache").join("terlan");
    }
    std::env::temp_dir().join("terlan-cache")
}

/// Computes a stable fingerprint for embedded release support source.
///
/// Inputs:
/// - `support_by_name`: embedded release support source index.
///
/// Output:
/// - Hex fingerprint string used as a cache directory name.
///
/// Transformation:
/// - Hashes cache schema, compiler package version, support module names,
///   source paths, and source text through a deterministic FNV-1a accumulator.
pub(super) fn release_support_fingerprint(
    support_by_name: &BTreeMap<String, &'static ReleaseSupportModule>,
) -> Result<String, String> {
    let mut hash = StableFnv64::new();
    hash.write_str(RELEASE_SUPPORT_CACHE_SCHEMA);
    hash.write_str(env!("CARGO_PKG_VERSION"));
    hash.write_bytes(&current_compiler_executable_bytes()?);
    for (module_name, module) in support_by_name {
        hash.write_str(module_name);
        hash.write_str(module.path);
        hash.write_str(module.source);
    }
    Ok(format!("{:016x}", hash.finish()))
}

/// Reads the current compiler executable for cache invalidation.
///
/// Inputs:
/// - No direct inputs; reads `std::env::current_exe()`.
///
/// Output:
/// - Executable bytes for the running `terlc` process.
/// - `Err(message)` when the executable path or file cannot be read.
///
/// Transformation:
/// - Anchors release-support caches to the actual compiler binary, not just
///   the package version, so local scratch rebuilds invalidate stale BEAM
///   support modules even when `CARGO_PKG_VERSION` has not changed.
fn current_compiler_executable_bytes() -> Result<Vec<u8>, String> {
    let path =
        std::env::current_exe().map_err(|err| format!("failed to locate current terlc: {err}"))?;
    fs::read(&path).map_err(|err| {
        format!(
            "failed to read current terlc executable {} for release support cache fingerprint: {err}",
            path.display()
        )
    })
}

/// Deterministic FNV-1a 64-bit accumulator.
///
/// Inputs:
/// - Strings written in a deliberate order.
///
/// Output:
/// - Stable 64-bit hash value.
///
/// Transformation:
/// - Applies byte-wise FNV-1a with an extra separator byte after each string to
///   avoid accidental concatenation collisions in cache fingerprints.
struct StableFnv64 {
    value: u64,
}

impl StableFnv64 {
    /// Creates a new FNV-1a accumulator.
    ///
    /// Inputs:
    /// - None.
    ///
    /// Output:
    /// - Accumulator initialized with the FNV offset basis.
    ///
    /// Transformation:
    /// - Provides a stable starting state for deterministic fingerprints.
    fn new() -> Self {
        Self {
            value: 0xcbf29ce484222325,
        }
    }

    /// Adds one string plus separator to the accumulator.
    ///
    /// Inputs:
    /// - `text`: string fragment to hash.
    ///
    /// Output:
    /// - Mutates the accumulator in place.
    ///
    /// Transformation:
    /// - Hashes every byte and then a null separator byte with FNV-1a.
    fn write_str(&mut self, text: &str) {
        self.write_bytes(text.as_bytes());
    }

    /// Adds bytes plus separator to the accumulator.
    ///
    /// Inputs:
    /// - `bytes`: byte fragment to hash.
    ///
    /// Output:
    /// - Mutates the accumulator in place.
    ///
    /// Transformation:
    /// - Hashes every byte and then a null separator byte with FNV-1a.
    fn write_bytes(&mut self, bytes: &[u8]) {
        for byte in bytes.iter().copied().chain(std::iter::once(0)) {
            self.value ^= u64::from(byte);
            self.value = self.value.wrapping_mul(0x100000001b3);
        }
    }

    /// Returns the final hash value.
    ///
    /// Inputs:
    /// - `self`: completed accumulator.
    ///
    /// Output:
    /// - Stable 64-bit hash value.
    ///
    /// Transformation:
    /// - Exposes the accumulator value without additional finalization.
    fn finish(self) -> u64 {
        self.value
    }
}

/// Selects embedded support modules referenced by fully-qualified source names.
///
/// Inputs:
/// - `source`: source text for the module under test.
/// - `support_by_name`: embedded support module index keyed by module name.
///
/// Output:
/// - Sorted support module names whose canonical name appears as a
///   fully-qualified call/type prefix in source text.
///
/// Transformation:
/// - Scans for `Module.Name.` tokens in source text so tests using explicit
///   fully-qualified std calls do not need redundant imports just to make the
///   installed runner compile the called module.
pub(super) fn source_release_support_module_names(
    source: &str,
    support_by_name: &BTreeMap<String, &'static ReleaseSupportModule>,
) -> BTreeSet<String> {
    support_by_name
        .keys()
        .filter(|module_name| source.contains(&format!("{module_name}.")))
        .cloned()
        .collect()
}

/// Builds the embedded release support index by module name.
///
/// Inputs:
/// - Static release support module inventory.
///
/// Output:
/// - Map from declared Terlan module name to embedded support source.
///
/// Transformation:
/// - Extracts each `module ... .` declaration from embedded source text so
///   runtime test support can be selected by CoreIR import identity.
pub(super) fn release_support_modules_by_name() -> BTreeMap<String, &'static ReleaseSupportModule> {
    release_support_modules()
        .iter()
        .filter_map(|module| {
            release_support_module_name(module.source).map(|module_name| (module_name, module))
        })
        .collect()
}

/// Extracts a Terlan module name from embedded support source.
///
/// Inputs:
/// - `source`: embedded `.terl` source text.
///
/// Output:
/// - `Some(module_name)` when a top-level `module` declaration is present.
/// - `None` when the source does not contain the expected declaration shape.
///
/// Transformation:
/// - Scans trimmed source lines and strips the `module` keyword plus trailing
///   period without invoking the full parser.
fn release_support_module_name(source: &str) -> Option<String> {
    source.lines().find_map(|line| {
        let trimmed = line.trim();
        trimmed
            .strip_prefix("module ")
            .and_then(|rest| rest.strip_suffix('.'))
            .map(|module_name| module_name.trim().to_string())
    })
}

/// Selects embedded support modules referenced by CoreIR imports.
///
/// Inputs:
/// - `imports`: CoreIR imports from a module being compiled for tests.
/// - `support_by_name`: embedded support module index keyed by module name.
///
/// Output:
/// - Sorted support module names that are both normal module imports and
///   available in the embedded support inventory.
///
/// Transformation:
/// - Filters out asset imports and non-embedded project imports, keeping only
///   std support modules that can be compiled by the installed runner.
pub(super) fn direct_release_support_module_names(
    imports: &[CoreImport],
    support_by_name: &BTreeMap<String, &'static ReleaseSupportModule>,
) -> BTreeSet<String> {
    imports
        .iter()
        .filter(|import| import.kind == CoreImportKind::Module)
        .filter(|import| support_by_name.contains_key(&import.module))
        .map(|import| import.module.clone())
        .collect()
}

/// Returns the embedded release support module list for `terlc test`.
///
/// Inputs:
/// - No runtime input.
///
/// Output:
/// - Static source path/text pairs compiled into the BEAM test workspace.
///
/// Transformation:
/// - Centralizes the current release-matrix support set so future additions are
///   explicit and reviewable, while keeping installed `terlc test` independent
///   of the caller's current working directory.
pub(super) fn release_support_modules() -> &'static [ReleaseSupportModule] {
    &[
        ReleaseSupportModule {
            path: "std/test/test.terl",
            source: include_str!("../../../../../std/test/test.terl"),
        },
        ReleaseSupportModule {
            path: "std/core/atom.terl",
            source: include_str!("../../../../../std/core/atom.terl"),
        },
        ReleaseSupportModule {
            path: "std/core/bool.terl",
            source: include_str!("../../../../../std/core/bool.terl"),
        },
        ReleaseSupportModule {
            path: "std/core/unit.terl",
            source: include_str!("../../../../../std/core/unit.terl"),
        },
        ReleaseSupportModule {
            path: "std/core/ordering.terl",
            source: include_str!("../../../../../std/core/ordering.terl"),
        },
        ReleaseSupportModule {
            path: "std/core/int.terl",
            source: include_str!("../../../../../std/core/int.terl"),
        },
        ReleaseSupportModule {
            path: "std/core/float.terl",
            source: include_str!("../../../../../std/core/float.terl"),
        },
        ReleaseSupportModule {
            path: "std/core/option.terl",
            source: include_str!("../../../../../std/core/option.terl"),
        },
        ReleaseSupportModule {
            path: "std/core/result.terl",
            source: include_str!("../../../../../std/core/result.terl"),
        },
        ReleaseSupportModule {
            path: "std/core/error.terl",
            source: include_str!("../../../../../std/core/error.terl"),
        },
        ReleaseSupportModule {
            path: "std/core/equal.terl",
            source: include_str!("../../../../../std/core/equal.terl"),
        },
        ReleaseSupportModule {
            path: "std/core/functional.terl",
            source: include_str!("../../../../../std/core/functional.terl"),
        },
        ReleaseSupportModule {
            path: "std/http/error.terl",
            source: include_str!("../../../../../std/http/error.terl"),
        },
        ReleaseSupportModule {
            path: "std/core/string.terl",
            source: include_str!("../../../../../std/core/string.terl"),
        },
        ReleaseSupportModule {
            path: "std/core/object.terl",
            source: include_str!("../../../../../std/core/object.terl"),
        },
        ReleaseSupportModule {
            path: "std/data/json.terl",
            source: include_str!("../../../../../std/data/json.terl"),
        },
        ReleaseSupportModule {
            path: "std/io/console.terl",
            source: include_str!("../../../../../std/io/console.terl"),
        },
        ReleaseSupportModule {
            path: "std/io/file.terl",
            source: include_str!("../../../../../std/io/file.terl"),
        },
        ReleaseSupportModule {
            path: "std/collections/iterator.terl",
            source: include_str!("../../../../../std/collections/iterator.terl"),
        },
        ReleaseSupportModule {
            path: "std/collections/index.terl",
            source: include_str!("../../../../../std/collections/index.terl"),
        },
        ReleaseSupportModule {
            path: "std/collections/list.terl",
            source: include_str!("../../../../../std/collections/list.terl"),
        },
        ReleaseSupportModule {
            path: "std/collections/map.terl",
            source: include_str!("../../../../../std/collections/map.terl"),
        },
        ReleaseSupportModule {
            path: "std/collections/set.terl",
            source: include_str!("../../../../../std/collections/set.terl"),
        },
        ReleaseSupportModule {
            path: "std/collections/iterable.terl",
            source: include_str!("../../../../../std/collections/iterable.terl"),
        },
        ReleaseSupportModule {
            path: "std/collections/enumerable.terl",
            source: include_str!("../../../../../std/collections/enumerable.terl"),
        },
        ReleaseSupportModule {
            path: "std/sync/resource.terl",
            source: include_str!("../../../../../std/sync/resource.terl"),
        },
    ]
}
