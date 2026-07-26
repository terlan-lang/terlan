//! Compilation and admission of a source-backed AOT handler generation.

use super::*;
use std::io::{BufRead, BufReader, Write};

const SERVE_RUNTIME_ONLY_ENV: &str = "TERLAN_SERVE_RUNTIME_ONLY";
const COMPILER_DAEMON_ENV: &str = "TERLAN_SERVE_COMPILER_DAEMON";
const COMPILER_DAEMON_PREFIX: &str = "TERLAN_GENERATION:";
const PERSISTED_GENERATION_SCHEMA: &str = "terlan-serve-generation-v3";
const MAX_GENERATION_METADATA_BYTES: u64 = 1024 * 1024;

struct CompilerDaemon {
    web_root: PathBuf,
    child: std::process::Child,
    stdin: std::process::ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
}

static COMPILER_DAEMON: OnceLock<Mutex<Option<CompilerDaemon>>> = OnceLock::new();

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct PersistedServeGeneration {
    schema: String,
    compiler_version: String,
    checksum: String,
    module: String,
    image: PathBuf,
    image_sha256: String,
    image_bytes: u64,
    router: Option<AotRouterPlan>,
    request_projections: Vec<crate::compiler::native_ir::NativeRequestProjection>,
}

pub(super) fn cached_source_entry(
    web_root: &Path,
    source_path: &Path,
    expected_module: &str,
) -> Result<HandlerCacheEntry, String> {
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
        compile_generation_helper(web_root)?;
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
    #[cfg(feature = "serve-runtime-bin")]
    return Err(
        "error[serve.aot.runtime_mode]: compiler-free runtime requires persisted generation metadata"
            .to_string(),
    );
    #[cfg(not(feature = "serve-runtime-bin"))]
    {
        compile_source_generation(web_root, source_path, expected_module, source, checksum)
    }
}

#[cfg(not(feature = "serve-runtime-bin"))]
fn compile_source_generation(
    web_root: &Path,
    source_path: &Path,
    expected_module: &str,
    source: String,
    checksum: String,
) -> Result<HandlerCacheEntry, String> {
    let source_name = source_path.to_string_lossy();
    let artifacts = crate::formal_pipeline::compile_syntax_module_through_phases_with_profile(
        &source_name,
        &source,
        DiagnosticFormat::Text {
            color: ColorChoice::Never,
        },
        None,
        NativePolicy::NativeBoundaryOptional,
        TargetProfile::Vm,
    )
    .map_err(|code| {
        format!(
            "error[serve.aot.compile]: failed to compile `{source_name}` with exit code {code:?}"
        )
    })?;
    if artifacts.core.module != expected_module {
        return Err(format!(
            "error[serve.aot.module]: source declared `{}` but manifest expected `{expected_module}`",
            artifacts.core.module
        ));
    }
    let (core, router) = crate::compiler::router::prepare_aot_router_module(&artifacts.core)?;
    let module_stem = expected_module.replace('.', "_");
    let image = crate::commands::build::vm_artifact::native_image::
        compile_serve_native_image_with_metadata(web_root, &module_stem, &core)?
        .ok_or_else(|| {
            format!(
                "error[serve.aot.image_required]: `{expected_module}` did not produce a native image; runtime CoreIR interpretation has been removed"
            )
        })?;
    let canonical_image = image.path.canonicalize().map_err(|error| {
        format!(
            "error[serve.aot.runtime_generation]: canonicalize image `{}`: {error}",
            image.path.display()
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
    persist_generation(
        web_root,
        PersistedServeGeneration {
            schema: PERSISTED_GENERATION_SCHEMA.to_string(),
            compiler_version: env!("CARGO_PKG_VERSION").to_string(),
            checksum: checksum.clone(),
            module: expected_module.to_string(),
            image: canonical_image,
            image_sha256,
            image_bytes,
            router: router.clone(),
            request_projections: image.request_projections.clone(),
        },
    )?;
    let entry = HandlerCacheEntry {
        checksum,
        runtime: Arc::new(AotHandlerRuntime::load_with_request_projections(
            expected_module.to_string(),
            &image.path,
            router,
            image.request_projections,
        )?),
    };
    cache()?
        .write()
        .map_err(|_| CACHE_ERROR.to_string())?
        .insert(source_path.to_path_buf(), entry.clone());
    Ok(entry)
}

fn persisted_generation_path(web_root: &Path, module: &str) -> PathBuf {
    web_root
        .join(".terlan")
        .join("serve-aot")
        .join("runtime")
        .join(format!("{}.json", module.replace('.', "_")))
}

fn load_persisted_generation(
    web_root: &Path,
    expected_module: &str,
    checksum: &str,
) -> Result<Option<HandlerCacheEntry>, String> {
    let path = persisted_generation_path(web_root, expected_module);
    let metadata = match fs::metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!(
                "error[serve.aot.runtime_generation]: failed to inspect `{}`: {error}",
                path.display()
            ))
        }
    };
    if metadata.len() > MAX_GENERATION_METADATA_BYTES {
        return Err(format!(
            "error[serve.aot.runtime_generation]: `{}` exceeds {} bytes",
            path.display(),
            MAX_GENERATION_METADATA_BYTES
        ));
    }
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) => {
            return Err(format!(
                "error[serve.aot.runtime_generation]: failed to read `{}`: {error}",
                path.display()
            ))
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
    let image = generation.image.canonicalize().map_err(|error| {
        format!(
            "error[serve.aot.runtime_generation]: canonicalize image `{}`: {error}",
            generation.image.display()
        )
    })?;
    let image_root = web_root
        .join(".terlan")
        .join("serve-aot")
        .canonicalize()
        .map_err(|error| {
            format!("error[serve.aot.runtime_generation]: canonicalize image root: {error}")
        })?;
    if !image.starts_with(&image_root) {
        return Err(format!(
            "error[serve.aot.runtime_generation]: image `{}` escapes `{}`",
            image.display(),
            image_root.display()
        ));
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
        ));
    }
    Ok(Some(HandlerCacheEntry {
        checksum: generation.checksum,
        runtime: Arc::new(AotHandlerRuntime::load_with_request_projections(
            generation.module,
            &image,
            generation.router,
            generation.request_projections,
        )?),
    }))
}

#[cfg_attr(feature = "serve-runtime-bin", allow(dead_code))]
fn persist_generation(web_root: &Path, generation: PersistedServeGeneration) -> Result<(), String> {
    let path = persisted_generation_path(web_root, &generation.module);
    let parent = path
        .parent()
        .ok_or_else(|| "error[serve.aot.runtime_generation]: missing parent".to_string())?;
    fs::create_dir_all(parent).map_err(|error| {
        format!(
            "error[serve.aot.runtime_generation]: create `{}`: {error}",
            parent.display()
        )
    })?;
    let bytes = serde_json::to_vec(&generation).map_err(|error| {
        format!("error[serve.aot.runtime_generation]: encode metadata: {error}")
    })?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| format!("error[serve.aot.runtime_generation]: system clock: {error}"))?
        .as_nanos();
    let temporary = path.with_extension(format!("json.tmp-{}-{nonce}", std::process::id()));
    let mut output = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)
        .map_err(|error| {
            format!(
                "error[serve.aot.runtime_generation]: create `{}`: {error}",
                temporary.display()
            )
        })?;
    output.write_all(&bytes).map_err(|error| {
        format!(
            "error[serve.aot.runtime_generation]: write `{}`: {error}",
            temporary.display()
        )
    })?;
    output.sync_all().map_err(|error| {
        format!(
            "error[serve.aot.runtime_generation]: sync `{}`: {error}",
            temporary.display()
        )
    })?;
    drop(output);
    fs::rename(&temporary, &path).map_err(|error| {
        format!(
            "error[serve.aot.runtime_generation]: publish `{}`: {error}",
            path.display()
        )
    })?;
    fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| {
            format!(
                "error[serve.aot.runtime_generation]: sync `{}`: {error}",
                parent.display()
            )
        })
}

fn compile_generation_helper(web_root: &Path) -> Result<(), String> {
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
        daemon
            .stdin
            .write_all(b"compile\n")
            .and_then(|()| daemon.stdin.flush())
            .map_err(|error| format!("error[serve.aot.compiler_helper]: send request: {error}"))?;
    }
    read_compiler_daemon_result(daemon.as_mut().expect("compiler daemon is available"))
}

pub(in crate::commands::serve) fn stage_source_generation(web_root: &Path) -> Result<(), String> {
    compile_generation_helper(web_root)
}

fn spawn_compiler_daemon(web_root: &Path) -> Result<CompilerDaemon, String> {
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

fn read_compiler_daemon_result(daemon: &mut CompilerDaemon) -> Result<(), String> {
    let mut line = String::new();
    loop {
        line.clear();
        if daemon
            .stdout
            .read_line(&mut line)
            .map_err(|error| format!("error[serve.aot.compiler_helper]: read result: {error}"))?
            == 0
        {
            return Err("error[serve.aot.compiler_helper]: compiler daemon exited".to_string());
        }
        let Some(payload) = line.trim_end().strip_prefix(COMPILER_DAEMON_PREFIX) else {
            continue;
        };
        return serde_json::from_str::<Result<(), String>>(payload).map_err(|error| {
            format!("error[serve.aot.compiler_helper]: malformed daemon result: {error}")
        })?;
    }
}

pub(in crate::commands::serve) fn run_compiler_daemon(web_root: &Path) -> std::process::ExitCode {
    if emit_compiler_daemon_result(&Ok(())).is_err() {
        return std::process::ExitCode::from(1);
    }
    for request in std::io::stdin().lock().lines() {
        if request.is_err() {
            return std::process::ExitCode::from(1);
        }
        super::invalidate_vm_handler_cache();
        let result = super::super::prewarm_dynamic_handler_sources(web_root);
        if emit_compiler_daemon_result(&result).is_err() {
            return std::process::ExitCode::from(1);
        }
    }
    std::process::ExitCode::SUCCESS
}

fn emit_compiler_daemon_result(result: &Result<(), String>) -> Result<(), String> {
    let payload = serde_json::to_string(result)
        .map_err(|error| format!("error[serve.aot.compiler_helper]: encode result: {error}"))?;
    let mut stdout = std::io::stdout().lock();
    writeln!(stdout, "{COMPILER_DAEMON_PREFIX}{payload}")
        .and_then(|()| stdout.flush())
        .map_err(|error| format!("error[serve.aot.compiler_helper]: write result: {error}"))
}
