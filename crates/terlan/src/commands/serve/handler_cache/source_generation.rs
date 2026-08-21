//! Compilation and admission of a source-backed AOT handler generation.

use super::*;
use crate::commands::serve::ServeResult;
use std::collections::BTreeMap;
#[cfg(any(test, not(feature = "serve-runtime-bin")))]
use std::collections::BTreeSet;
use std::io::{BufRead, BufReader, Write};
#[cfg(any(test, not(feature = "serve-runtime-bin")))]
use std::time::Instant;

#[cfg(any(test, not(feature = "serve-runtime-bin")))]
use sha2::{Digest, Sha256};

const SERVE_RUNTIME_ONLY_ENV: &str = "TERLAN_SERVE_RUNTIME_ONLY";
const COMPILER_DAEMON_ENV: &str = "TERLAN_SERVE_COMPILER_DAEMON";
const COMPILER_DAEMON_PREFIX: &str = "TERLAN_GENERATION:";
const PERSISTED_GENERATION_SCHEMA: &str = "terlan-serve-generation-v4";
const ACTIVE_GENERATION_SCHEMA: &str = "terlan-serve-active-generation-v1";
#[cfg(any(test, not(feature = "serve-runtime-bin")))]
const RELOAD_REPORT_SCHEMA: &str = "terlan-aot-developer-hot-reload-v1";
const MAX_GENERATION_METADATA_BYTES: u64 = 1024 * 1024;

#[cfg(any(test, not(feature = "serve-runtime-bin")))]
#[path = "source_generation/developer_reload.rs"]
pub(super) mod developer_reload;

struct CompilerDaemon {
    web_root: PathBuf,
    child: std::process::Child,
    stdin: std::process::ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
}

static COMPILER_DAEMON: OnceLock<Mutex<Option<CompilerDaemon>>> = OnceLock::new();

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PersistedServeGeneration {
    schema: String,
    compiler_version: String,
    checksum: String,
    module: String,
    image: PathBuf,
    image_sha256: String,
    image_bytes: u64,
    router: Option<AotRouterPlan>,
    request_projections: Vec<crate::runtime::vm::aot_metadata::NativeRequestProjection>,
    compatibility: ServeGenerationCompatibility,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ServeGenerationCompatibility {
    public_abi_sha256: String,
    process_state_sha256: String,
    resource_contract_sha256: String,
    dependencies: Vec<String>,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct ActiveServeGeneration {
    schema: String,
    identity: u64,
    modules: BTreeMap<String, PathBuf>,
}

pub(super) fn cached_source_entry(
    web_root: &Path,
    source_path: &Path,
    expected_module: &str,
) -> ServeResult<HandlerCacheEntry> {
    if let Some(entry) = cache()?
        .read()
        .map_err(|_| CACHE_ERROR.to_string())?
        .get(source_path)
        .cloned()
    {
        return Ok(entry);
    }
    let source = fs::read_to_string(source_path).map_err(|error| {
        format!(
            "error[serve.aot.source]: failed to read `{}`: {error}",
            source_path.display()
        )
    })?;
    let checksum = format!("source-fnv1a64:{:016x}", fingerprint(source.as_bytes()));
    if let Some(entry) = cache()?
        .read()
        .map_err(|_| CACHE_ERROR.to_string())?
        .get(source_path)
        .filter(|entry| entry.checksum == checksum)
        .cloned()
    {
        return Ok(entry);
    }
    if let Some(entry) = load_persisted_generation(web_root, expected_module, &checksum)? {
        cache()?
            .write()
            .map_err(|_| CACHE_ERROR.to_string())?
            .insert(source_path.to_path_buf(), entry.clone());
        return Ok(entry);
    }
    if std::env::var_os(SERVE_RUNTIME_ONLY_ENV).is_some() {
        compile_generation_helper(web_root, std::slice::from_ref(&source_path.to_path_buf()))?;
        let entry = load_persisted_generation(web_root, expected_module, &checksum)?
            .ok_or_else(|| {
                format!(
                    "error[serve.aot.runtime_generation]: compiler helper did not publish `{expected_module}`"
                )
            })?;
        cache()?
            .write()
            .map_err(|_| CACHE_ERROR.to_string())?
            .insert(source_path.to_path_buf(), entry.clone());
        return Ok(entry);
    }
    #[cfg(all(feature = "serve-runtime-bin", not(test)))]
    return Err(
        "error[serve.aot.runtime_mode]: compiler-free runtime requires persisted generation metadata"
            .into(),
    );
    #[cfg(any(test, not(feature = "serve-runtime-bin")))]
    {
        compile_source_generation(web_root, source_path, expected_module, source, checksum)
    }
}

#[cfg(any(test, not(feature = "serve-runtime-bin")))]
fn compile_source_generation(
    web_root: &Path,
    source_path: &Path,
    expected_module: &str,
    source: String,
    checksum: String,
) -> ServeResult<HandlerCacheEntry> {
    let entry = compile_source_candidate(web_root, source_path, expected_module, source, checksum)?;
    persist_generation_batch(web_root, std::slice::from_ref(&entry.persisted))?;
    cache()?
        .write()
        .map_err(|_| CACHE_ERROR.to_string())?
        .insert(source_path.to_path_buf(), entry.clone());
    advance_cache_epoch();
    Ok(entry)
}

#[cfg(any(test, not(feature = "serve-runtime-bin")))]
fn compile_source_candidate(
    web_root: &Path,
    source_path: &Path,
    expected_module: &str,
    source: String,
    checksum: String,
) -> ServeResult<HandlerCacheEntry> {
    let syntax = crate::formal_pipeline::parse_source_as_syntax_output(
        &source_path.to_string_lossy(),
        &source,
    )
    .map_err(|error| {
        format!(
            "error[serve.aot.compile]: failed to parse `{}`: {error:?}",
            source_path.display()
        )
    })?;
    if syntax.module_name != expected_module {
        return Err(format!(
            "error[serve.aot.module]: source declared `{}` but manifest expected `{expected_module}`",
            syntax.module_name
        ).into());
    }
    let compiled = crate::commands::build::vm_artifact::compile_serve_application(
        web_root,
        source_path,
        expected_module,
    )?;
    let compatibility = compatibility_for_core(&compiled.core)?;
    let router = compiled.router;
    let image = compiled.image;
    let canonical_image = image.path.canonicalize().map_err(|error| {
        format!(
            "error[serve.aot.runtime_generation]: canonicalize image `{}`: {error}",
            image.path.display()
        )
    })?;
    let image_root = web_root
        .join(".terlan")
        .join("serve-aot")
        .canonicalize()
        .map_err(|error| {
            format!("error[serve.aot.runtime_generation]: canonicalize image root: {error}")
        })?;
    let portable_image = canonical_image
        .strip_prefix(&image_root)
        .map(Path::to_path_buf)
        .map_err(|_| {
            format!(
                "error[serve.aot.runtime_generation]: image `{}` escapes `{}`",
                canonical_image.display(),
                image_root.display()
            )
        })?;
    let image_sha256 = crate::support::sha256sum_file(&image.path)?;
    let image_bytes = image
        .path
        .metadata()
        .map_err(|error| {
            format!(
                "error[serve.aot.runtime_generation]: inspect image `{}`: {error}",
                image.path.display()
            )
        })?
        .len();
    let persisted = PersistedServeGeneration {
        schema: PERSISTED_GENERATION_SCHEMA.to_string(),
        compiler_version: env!("CARGO_PKG_VERSION").to_string(),
        checksum: checksum.clone(),
        module: expected_module.to_string(),
        image: portable_image,
        image_sha256,
        image_bytes,
        router: router.clone(),
        request_projections: image.request_projections.clone(),
        compatibility: compatibility.clone(),
    };
    let entry = HandlerCacheEntry {
        checksum,
        runtime: Arc::new(AotHandlerRuntime::load_with_request_projections(
            expected_module.to_string(),
            &image.path,
            router,
            image.request_projections,
            http_session_service_for(web_root)?,
        )?),
        compatibility,
        persisted,
    };
    Ok(entry)
}

#[cfg(any(test, not(feature = "serve-runtime-bin")))]
fn compatibility_for_core(
    core: &crate::terlan_typeck::CoreModule,
) -> ServeResult<ServeGenerationCompatibility> {
    let dependencies = core
        .imports
        .iter()
        .map(|import| import.module.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    Ok(ServeGenerationCompatibility {
        public_abi_sha256: hash_contract(core.interface_text().as_bytes()),
        process_state_sha256: hash_contract(
            &serde_json::to_vec(&(core.types.as_slice(), core.constructors.as_slice())).map_err(
                |error| format!("error[serve.aot.compatibility]: encode state: {error}"),
            )?,
        ),
        resource_contract_sha256: hash_contract(&serde_json::to_vec(&dependencies).map_err(
            |error| format!("error[serve.aot.compatibility]: encode resources: {error}"),
        )?),
        dependencies,
    })
}

#[cfg(any(test, not(feature = "serve-runtime-bin")))]
fn hash_contract(bytes: &[u8]) -> String {
    let mut result = String::from("sha256:");
    for byte in Sha256::digest(bytes) {
        use std::fmt::Write as _;
        write!(&mut result, "{byte:02x}").expect("writing to String cannot fail");
    }
    result
}

fn persisted_generation_path(web_root: &Path, module: &str) -> PathBuf {
    web_root
        .join(".terlan")
        .join("serve-aot")
        .join("runtime")
        .join(format!("{}.json", module.replace('.', "_")))
}

fn active_generation_path(web_root: &Path) -> PathBuf {
    web_root
        .join(".terlan")
        .join("serve-aot")
        .join("runtime")
        .join("active.json")
}

fn load_persisted_generation(
    web_root: &Path,
    expected_module: &str,
    checksum: &str,
) -> ServeResult<Option<HandlerCacheEntry>> {
    let path = active_generation_metadata_path(web_root, expected_module)?
        .unwrap_or_else(|| persisted_generation_path(web_root, expected_module));
    let metadata = match fs::metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!(
                "error[serve.aot.runtime_generation]: failed to inspect `{}`: {error}",
                path.display()
            )
            .into())
        }
    };
    if metadata.len() > MAX_GENERATION_METADATA_BYTES {
        return Err(format!(
            "error[serve.aot.runtime_generation]: `{}` exceeds {} bytes",
            path.display(),
            MAX_GENERATION_METADATA_BYTES
        )
        .into());
    }
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) => {
            return Err(format!(
                "error[serve.aot.runtime_generation]: failed to read `{}`: {error}",
                path.display()
            )
            .into())
        }
    };
    let generation =
        serde_json::from_slice::<PersistedServeGeneration>(&bytes).map_err(|error| {
            format!(
                "error[serve.aot.runtime_generation]: invalid `{}`: {error}",
                path.display()
            )
        })?;
    if generation.schema != PERSISTED_GENERATION_SCHEMA
        || generation.compiler_version != env!("CARGO_PKG_VERSION")
        || generation.checksum != checksum
        || generation.module != expected_module
    {
        return Ok(None);
    }
    let image_root = web_root
        .join(".terlan")
        .join("serve-aot")
        .canonicalize()
        .map_err(|error| {
            format!("error[serve.aot.runtime_generation]: canonicalize image root: {error}")
        })?;
    let stored_image = if generation.image.is_absolute() {
        generation.image.clone()
    } else {
        image_root.join(&generation.image)
    };
    let image = stored_image.canonicalize().map_err(|error| {
        format!(
            "error[serve.aot.runtime_generation]: canonicalize image `{}`: {error}",
            stored_image.display()
        )
    })?;
    if !image.starts_with(&image_root) {
        return Err(format!(
            "error[serve.aot.runtime_generation]: image `{}` escapes `{}`",
            image.display(),
            image_root.display()
        )
        .into());
    }
    let image_metadata = image.metadata().map_err(|error| {
        format!(
            "error[serve.aot.runtime_generation]: inspect image `{}`: {error}",
            image.display()
        )
    })?;
    if !image_metadata.is_file()
        || image_metadata.len() != generation.image_bytes
        || crate::support::sha256sum_file(&image)? != generation.image_sha256
    {
        return Err(format!(
            "error[serve.aot.runtime_generation]: image integrity mismatch for `{}`",
            image.display()
        )
        .into());
    }
    Ok(Some(HandlerCacheEntry {
        checksum: generation.checksum.clone(),
        runtime: Arc::new(AotHandlerRuntime::load_with_request_projections(
            generation.module.clone(),
            &image,
            generation.router.clone(),
            generation.request_projections.clone(),
            http_session_service_for(web_root)?,
        )?),
        compatibility: generation.compatibility.clone(),
        #[cfg(any(test, not(feature = "serve-runtime-bin")))]
        persisted: generation,
    }))
}

pub(super) fn active_generation_metadata_path(
    web_root: &Path,
    module: &str,
) -> ServeResult<Option<PathBuf>> {
    let active_path = active_generation_path(web_root);
    let bytes = match fs::read(&active_path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!(
                "error[serve.aot.runtime_generation]: read `{}`: {error}",
                active_path.display()
            )
            .into())
        }
    };
    if bytes.len() as u64 > MAX_GENERATION_METADATA_BYTES {
        return Err("error[serve.aot.runtime_generation]: active generation is oversized".into());
    }
    let active = serde_json::from_slice::<ActiveServeGeneration>(&bytes).map_err(|error| {
        format!(
            "error[serve.aot.runtime_generation]: invalid `{}`: {error}",
            active_path.display()
        )
    })?;
    if active.schema != ACTIVE_GENERATION_SCHEMA {
        return Err(format!(
            "error[serve.aot.runtime_generation]: unsupported active schema `{}`",
            active.schema
        )
        .into());
    }
    let Some(relative) = active.modules.get(module) else {
        return Ok(None);
    };
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err("error[serve.aot.runtime_generation]: unsafe active metadata path".into());
    }
    Ok(Some(
        active_path
            .parent()
            .expect("active generation has runtime parent")
            .join(relative),
    ))
}
#[cfg(any(test, not(feature = "serve-runtime-bin")))]
fn persist_generation_batch(
    web_root: &Path,
    generations: &[PersistedServeGeneration],
) -> ServeResult<u64> {
    if generations.is_empty() {
        return Err("error[serve.aot.runtime_generation]: empty candidate generation".into());
    }
    let active_path = active_generation_path(web_root);
    let runtime_dir = active_path
        .parent()
        .ok_or_else(|| "error[serve.aot.runtime_generation]: missing runtime parent".to_string())?;
    fs::create_dir_all(runtime_dir).map_err(|error| {
        format!(
            "error[serve.aot.runtime_generation]: create `{}`: {error}",
            runtime_dir.display()
        )
    })?;
    let previous = read_active_generation(&active_path)?;
    let identity = previous
        .as_ref()
        .map_or(1, |active| active.identity.saturating_add(1));
    let generation_bytes = serde_json::to_vec(generations).map_err(|error| {
        format!("error[serve.aot.runtime_generation]: encode generation identity: {error}")
    })?;
    let generation_digest = Sha256::digest(&generation_bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let generation_name = format!("{identity}-{generation_digest}");
    let generation_dir = runtime_dir.join("generations").join(&generation_name);
    fs::create_dir_all(&generation_dir).map_err(|error| {
        format!(
            "error[serve.aot.runtime_generation]: create `{}`: {error}",
            generation_dir.display()
        )
    })?;
    let mut modules = previous.map_or_else(BTreeMap::new, |active| active.modules);
    for generation in generations {
        let file_name = format!("{}.json", generation.module.replace('.', "_"));
        let path = generation_dir.join(&file_name);
        let bytes = serde_json::to_vec(generation).map_err(|error| {
            format!("error[serve.aot.runtime_generation]: encode metadata: {error}")
        })?;
        write_synced_new_file(&path, &bytes)?;
        modules.insert(
            generation.module.clone(),
            PathBuf::from("generations")
                .join(&generation_name)
                .join(file_name),
        );
    }
    let active = ActiveServeGeneration {
        schema: ACTIVE_GENERATION_SCHEMA.to_string(),
        identity,
        modules,
    };
    let bytes = serde_json::to_vec(&active).map_err(|error| {
        format!("error[serve.aot.runtime_generation]: encode active metadata: {error}")
    })?;
    let temporary = active_path.with_extension(format!(
        "json.tmp-{}-{generation_digest}",
        std::process::id()
    ));
    write_synced_new_file(&temporary, &bytes)?;
    fs::rename(&temporary, &active_path).map_err(|error| {
        format!(
            "error[serve.aot.runtime_generation]: publish `{}`: {error}",
            active_path.display()
        )
    })?;
    fs::File::open(runtime_dir)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| {
            format!(
                "error[serve.aot.runtime_generation]: sync `{}`: {error}",
                runtime_dir.display()
            )
        })?;
    Ok(identity)
}

#[cfg(any(test, not(feature = "serve-runtime-bin")))]
fn read_active_generation(path: &Path) -> ServeResult<Option<ActiveServeGeneration>> {
    match fs::read(path) {
        Ok(bytes) => Ok(serde_json::from_slice::<ActiveServeGeneration>(&bytes)
            .map(Some)
            .map_err(|error| {
                format!(
                    "error[serve.aot.runtime_generation]: invalid `{}`: {error}",
                    path.display()
                )
            })?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!(
            "error[serve.aot.runtime_generation]: read `{}`: {error}",
            path.display()
        )
        .into()),
    }
}

#[cfg(any(test, not(feature = "serve-runtime-bin")))]
fn write_synced_new_file(path: &Path, bytes: &[u8]) -> ServeResult<()> {
    let mut output = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|error| {
            format!(
                "error[serve.aot.runtime_generation]: create `{}`: {error}",
                path.display()
            )
        })?;
    output.write_all(bytes).map_err(|error| {
        format!(
            "error[serve.aot.runtime_generation]: write `{}`: {error}",
            path.display()
        )
    })?;
    output.sync_all().map_err(|error| {
        format!(
            "error[serve.aot.runtime_generation]: sync `{}`: {error}",
            path.display()
        )
    })?;
    Ok(())
}

fn validate_compatibility(
    active: &ServeGenerationCompatibility,
    candidate: &ServeGenerationCompatibility,
) -> ServeResult<&'static str> {
    let restart = std::env::var_os("TERLAN_SERVE_RESTART_INCOMPATIBLE").is_some();
    if active.process_state_sha256 != candidate.process_state_sha256 && !restart {
        return Err("error[serve.aot.incompatible_state]: process-state shape changed; restart explicitly with TERLAN_SERVE_RESTART_INCOMPATIBLE=1".into());
    }
    if active.resource_contract_sha256 != candidate.resource_contract_sha256 && !restart {
        return Err("error[serve.aot.incompatible_resource]: capability or native-resource contract changed; restart explicitly with TERLAN_SERVE_RESTART_INCOMPATIBLE=1".into());
    }
    if active.public_abi_sha256 != candidate.public_abi_sha256 && !restart {
        return Err("error[serve.aot.incompatible_abi]: exported ABI changed; restart explicitly with TERLAN_SERVE_RESTART_INCOMPATIBLE=1".into());
    }
    Ok(if restart {
        "explicit_restart"
    } else {
        "compatible"
    })
}

fn compile_generation_helper(web_root: &Path, changed_paths: &[PathBuf]) -> ServeResult<()> {
    let daemon = COMPILER_DAEMON.get_or_init(|| Mutex::new(None));
    let mut daemon = daemon
        .lock()
        .map_err(|_| "error[serve.aot.compiler_helper]: daemon lock poisoned".to_string())?;
    if daemon.as_mut().is_some_and(|daemon| {
        daemon.web_root != web_root || daemon.child.try_wait().ok().flatten().is_some()
    }) {
        *daemon = None;
    }
    if daemon.is_none() {
        *daemon = Some(spawn_compiler_daemon(web_root)?);
    } else {
        let daemon = daemon.as_mut().expect("checked compiler daemon");
        let request = serde_json::to_vec(changed_paths).map_err(|error| {
            format!("error[serve.aot.compiler_helper]: encode request: {error}")
        })?;
        daemon
            .stdin
            .write_all(&request)
            .and_then(|()| daemon.stdin.write_all(b"\n"))
            .and_then(|()| daemon.stdin.flush())
            .map_err(|error| format!("error[serve.aot.compiler_helper]: send request: {error}"))?;
    }
    read_compiler_daemon_result(daemon.as_mut().expect("compiler daemon is available"))
}

pub(in crate::commands::serve) fn stage_source_generation(
    web_root: &Path,
    changed_paths: &[PathBuf],
) -> ServeResult<()> {
    compile_generation_helper(web_root, changed_paths)?;
    activate_persisted_generation(web_root)
}

pub(super) fn activate_persisted_generation(web_root: &Path) -> ServeResult<()> {
    let sources = super::super::handler_sources::dynamic_handler_source_modules(web_root)?;
    let current = cache()?
        .read()
        .map_err(|_| CACHE_ERROR.to_string())?
        .clone();
    let mut candidates = Vec::with_capacity(sources.len());
    for (path, module) in sources {
        let source = fs::read_to_string(&path).map_err(|error| {
            format!(
                "error[serve.aot.source]: failed to read `{}`: {error}",
                path.display()
            )
        })?;
        let checksum = format!("source-fnv1a64:{:016x}", fingerprint(source.as_bytes()));
        let candidate =
            load_persisted_generation(web_root, &module, &checksum)?.ok_or_else(|| {
                format!("error[serve.aot.partial_generation]: active generation omits `{module}`")
            })?;
        if let Some(active) = current.get(&path) {
            validate_compatibility(&active.compatibility, &candidate.compatibility)?;
        }
        candidates.push((path, candidate));
    }
    {
        let mut admitted = cache()?.write().map_err(|_| CACHE_ERROR.to_string())?;
        for (path, candidate) in candidates {
            admitted.insert(path, candidate);
        }
    }
    advance_cache_epoch();
    Ok(())
}

fn spawn_compiler_daemon(web_root: &Path) -> ServeResult<CompilerDaemon> {
    let current = std::env::current_exe().map_err(|error| {
        format!("error[serve.aot.compiler_helper]: resolve current executable: {error}")
    })?;
    let executable = std::env::var_os("TERLAN_COMPILER")
        .map(PathBuf::from)
        .or_else(|| {
            current
                .file_stem()
                .is_some_and(|name| name == "terlc")
                .then(|| current.clone())
        })
        .or_else(|| {
            let name = if cfg!(windows) { "terlc.exe" } else { "terlc" };
            current.parent().map(|parent| parent.join(name))
        })
        .filter(|path| path.is_file())
        .ok_or_else(|| {
            "error[serve.aot.compiler_helper]: set TERLAN_COMPILER to the `terlc` executable"
                .to_string()
        })?;
    let mut child = std::process::Command::new(executable)
        .env_remove(SERVE_RUNTIME_ONLY_ENV)
        .env(COMPILER_DAEMON_ENV, "1")
        .arg("serve")
        .arg(web_root)
        .arg("--check")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| {
            format!("error[serve.aot.compiler_helper]: start compiler helper: {error}")
        })?;
    let stdin = child.stdin.take().ok_or_else(|| {
        "error[serve.aot.compiler_helper]: compiler daemon stdin unavailable".to_string()
    })?;
    let stdout = child.stdout.take().ok_or_else(|| {
        "error[serve.aot.compiler_helper]: compiler daemon stdout unavailable".to_string()
    })?;
    Ok(CompilerDaemon {
        web_root: web_root.to_path_buf(),
        child,
        stdin,
        stdout: BufReader::new(stdout),
    })
}

fn read_compiler_daemon_result(daemon: &mut CompilerDaemon) -> ServeResult<()> {
    let mut line = String::new();
    loop {
        line.clear();
        if daemon
            .stdout
            .read_line(&mut line)
            .map_err(|error| format!("error[serve.aot.compiler_helper]: read result: {error}"))?
            == 0
        {
            return Err("error[serve.aot.compiler_helper]: compiler daemon exited".into());
        }
        let Some(payload) = line.trim_end().strip_prefix(COMPILER_DAEMON_PREFIX) else {
            continue;
        };
        return Ok(
            serde_json::from_str::<Result<(), String>>(payload).map_err(|error| {
                format!("error[serve.aot.compiler_helper]: malformed daemon result: {error}")
            })??,
        );
    }
}
#[cfg(any(test, not(feature = "serve-runtime-bin")))]
pub(in crate::commands::serve) fn run_compiler_daemon(web_root: &Path) -> std::process::ExitCode {
    if emit_compiler_daemon_result(&Ok(())).is_err() {
        return std::process::ExitCode::from(1);
    }
    for request in std::io::stdin().lock().lines() {
        let result = request
            .map_err(|error| format!("error[serve.aot.compiler_helper]: read request: {error}"))
            .and_then(|request| {
                serde_json::from_str::<Vec<PathBuf>>(&request).map_err(|error| {
                    format!("error[serve.aot.compiler_helper]: malformed request: {error}")
                })
            })
            .and_then(|paths| {
                developer_reload::compile_and_publish_source_batch(web_root, &paths)
                    .map_err(String::from)
            });
        if let Err(error) = &result {
            if let Err(report_error) = developer_reload::record_failed_reload(web_root, error) {
                eprintln!("{report_error}");
            }
        }
        if emit_compiler_daemon_result(&result).is_err() {
            return std::process::ExitCode::from(1);
        }
    }
    std::process::ExitCode::SUCCESS
}
#[cfg(any(test, not(feature = "serve-runtime-bin")))]
fn emit_compiler_daemon_result(result: &Result<(), String>) -> ServeResult<()> {
    let payload = serde_json::to_string(result)
        .map_err(|error| format!("error[serve.aot.compiler_helper]: encode result: {error}"))?;
    let mut stdout = std::io::stdout().lock();
    Ok(writeln!(stdout, "{COMPILER_DAEMON_PREFIX}{payload}")
        .and_then(|()| stdout.flush())
        .map_err(|error| format!("error[serve.aot.compiler_helper]: write result: {error}"))?)
}
