//! Transactional developer reload orchestration for native handler images.

use super::*;
use crate::commands::serve::ServeResult;

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum DeveloperReloadEvent {
    SourceBatch { paths: Vec<String> },
    CompilerInvalidation { modules: Vec<String> },
    Diagnostic { message: String },
    RuntimeActivation { generation: u64 },
    BrowserRefresh { generation: u64 },
    DebuggerSourceMap { generation: u64 },
    EditorDiagnostics { generation: u64, clean: bool },
    TuiStatus { generation: u64, status: String },
}

#[derive(Debug, serde::Serialize)]
struct DeveloperReloadReport {
    schema: &'static str,
    source_event_batches: usize,
    changed_paths: Vec<String>,
    invalidated_modules: Vec<String>,
    cache_reuse: usize,
    compilation_micros: u128,
    activation_micros: u128,
    previous_generation: u64,
    candidate_generation: u64,
    compatibility_decisions: BTreeMap<String, String>,
    retained_runtime_state: bool,
    failed_build_continuity: bool,
    direct_aot: bool,
    events: Vec<DeveloperReloadEvent>,
}

pub(in crate::commands::serve::handler_cache) fn compile_and_publish_source_batch(
    web_root: &Path,
    changed_paths: &[PathBuf],
) -> ServeResult<()> {
    let started = Instant::now();
    let sources = super::super::super::handler_sources::dynamic_handler_source_modules(web_root)?;
    let current = cache()?
        .read()
        .map_err(|_| CACHE_ERROR.to_string())?
        .clone();
    let module_paths = sources
        .iter()
        .map(|(path, module)| (module.clone(), path.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut invalidated = BTreeSet::new();
    let mut source_inputs = BTreeMap::new();
    for (path, module) in &sources {
        let source = fs::read_to_string(path).map_err(|error| {
            format!(
                "error[serve.aot.source]: failed to read `{}`: {error}",
                path.display()
            )
        })?;
        let checksum = format!("source-fnv1a64:{:016x}", fingerprint(source.as_bytes()));
        if current
            .get(path)
            .is_none_or(|entry| entry.checksum != checksum)
        {
            invalidated.insert(module.clone());
        }
        source_inputs.insert(path.clone(), (module.clone(), source, checksum));
    }
    if changed_paths
        .iter()
        .any(|path| !module_paths.values().any(|source| source == path) && is_compiler_input(path))
    {
        invalidated.extend(module_paths.keys().cloned());
    }
    loop {
        let dependent = current.iter().find_map(|(path, entry)| {
            (module_paths.contains_key(&entry.persisted.module)
                && !invalidated.contains(&entry.persisted.module)
                && entry
                    .compatibility
                    .dependencies
                    .iter()
                    .any(|dependency| invalidated.contains(dependency)))
            .then(|| (path.clone(), entry.persisted.module.clone()))
        });
        let Some((_path, module)) = dependent else {
            break;
        };
        invalidated.insert(module);
    }
    let previous_generation = read_active_generation(&active_generation_path(web_root))?
        .map_or(0, |active| active.identity);
    if invalidated.is_empty() {
        return write_reload_report(
            web_root,
            DeveloperReloadReport {
                schema: RELOAD_REPORT_SCHEMA,
                source_event_batches: 1,
                changed_paths: changed_paths
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect(),
                invalidated_modules: Vec::new(),
                cache_reuse: sources.len(),
                compilation_micros: started.elapsed().as_micros(),
                activation_micros: 0,
                previous_generation,
                candidate_generation: previous_generation,
                compatibility_decisions: BTreeMap::new(),
                retained_runtime_state: true,
                failed_build_continuity: true,
                direct_aot: true,
                events: vec![DeveloperReloadEvent::SourceBatch { paths: Vec::new() }],
            },
        );
    }

    let mut candidates = BTreeMap::new();
    let mut decisions = BTreeMap::new();
    for module in &invalidated {
        let path = module_paths.get(module).ok_or_else(|| {
            format!("error[serve.aot.dependency]: dependent module `{module}` has no source")
        })?;
        let (_, source, checksum) = source_inputs.get(path).ok_or_else(|| {
            format!(
                "error[serve.aot.dependency]: missing input `{}`",
                path.display()
            )
        })?;
        let candidate =
            compile_source_candidate(web_root, path, module, source.clone(), checksum.clone())?;
        let decision = current.get(path).map_or(Ok("initial"), |active| {
            validate_compatibility(&active.compatibility, &candidate.compatibility)
        })?;
        decisions.insert(module.clone(), decision.to_string());
        candidates.insert(path.clone(), candidate);
    }
    let compilation_micros = started.elapsed().as_micros();
    let activation_started = Instant::now();
    let persisted = candidates
        .values()
        .map(|entry| entry.persisted.clone())
        .collect::<Vec<_>>();
    let generation = persist_generation_batch(web_root, &persisted)?;
    {
        let mut admitted = cache()?.write().map_err(|_| CACHE_ERROR.to_string())?;
        for (path, candidate) in candidates {
            admitted.insert(path, candidate);
        }
    }
    advance_cache_epoch();
    let paths = changed_paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>();
    let modules = invalidated.into_iter().collect::<Vec<_>>();
    let events = vec![
        DeveloperReloadEvent::SourceBatch {
            paths: paths.clone(),
        },
        DeveloperReloadEvent::CompilerInvalidation {
            modules: modules.clone(),
        },
        DeveloperReloadEvent::RuntimeActivation { generation },
        DeveloperReloadEvent::BrowserRefresh { generation },
        DeveloperReloadEvent::DebuggerSourceMap { generation },
        DeveloperReloadEvent::EditorDiagnostics {
            generation,
            clean: true,
        },
        DeveloperReloadEvent::TuiStatus {
            generation,
            status: "active".into(),
        },
    ];
    write_reload_report(
        web_root,
        DeveloperReloadReport {
            schema: RELOAD_REPORT_SCHEMA,
            source_event_batches: 1,
            changed_paths: paths,
            invalidated_modules: modules,
            cache_reuse: sources.len().saturating_sub(persisted.len()),
            compilation_micros,
            activation_micros: activation_started.elapsed().as_micros(),
            previous_generation,
            candidate_generation: generation,
            compatibility_decisions: decisions,
            retained_runtime_state: true,
            failed_build_continuity: true,
            direct_aot: true,
            events,
        },
    )
}

fn is_compiler_input(path: &Path) -> bool {
    path.extension().and_then(|extension| extension.to_str()) == Some("terl")
        || path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| {
                matches!(name, "terlan.toml" | "terlan.lock" | "package.json")
                    || name.ends_with(".binding.json")
            })
}

fn write_reload_report(web_root: &Path, report: DeveloperReloadReport) -> ServeResult<()> {
    let path = web_root
        .join(".terlan")
        .join("serve-aot")
        .join("watch-mode-hot-reload-report.json");
    let parent = path
        .parent()
        .ok_or_else(|| "error[serve.aot.reload_report]: missing parent".to_string())?;
    fs::create_dir_all(parent).map_err(|error| {
        format!(
            "error[serve.aot.reload_report]: create `{}`: {error}",
            parent.display()
        )
    })?;
    let bytes = serde_json::to_vec_pretty(&report)
        .map_err(|error| format!("error[serve.aot.reload_report]: encode: {error}"))?;
    let temporary = path.with_extension(format!("json.tmp-{}", std::process::id()));
    if temporary.exists() {
        fs::remove_file(&temporary).map_err(|error| {
            format!("error[serve.aot.reload_report]: remove stale temporary: {error}")
        })?;
    }
    write_synced_new_file(&temporary, &bytes)?;
    Ok(fs::rename(&temporary, &path).map_err(|error| {
        format!(
            "error[serve.aot.reload_report]: publish `{}`: {error}",
            path.display()
        )
    })?)
}

pub(in crate::commands::serve::handler_cache) fn record_failed_reload(
    web_root: &Path,
    message: &str,
) -> ServeResult<()> {
    let generation = read_active_generation(&active_generation_path(web_root))?
        .map_or(0, |active| active.identity);
    write_reload_report(
        web_root,
        DeveloperReloadReport {
            schema: RELOAD_REPORT_SCHEMA,
            source_event_batches: 1,
            changed_paths: Vec::new(),
            invalidated_modules: Vec::new(),
            cache_reuse: 0,
            compilation_micros: 0,
            activation_micros: 0,
            previous_generation: generation,
            candidate_generation: generation,
            compatibility_decisions: BTreeMap::new(),
            retained_runtime_state: true,
            failed_build_continuity: true,
            direct_aot: true,
            events: vec![
                DeveloperReloadEvent::Diagnostic {
                    message: message.to_string(),
                },
                DeveloperReloadEvent::EditorDiagnostics {
                    generation,
                    clean: false,
                },
                DeveloperReloadEvent::TuiStatus {
                    generation,
                    status: "candidate_rejected".into(),
                },
            ],
        },
    )
}
