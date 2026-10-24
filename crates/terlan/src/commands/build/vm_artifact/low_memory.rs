//! Bounded-memory native application compilation.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use crate::compiler::native_ir::{ApplicationReachabilityGraph, NativeCodegenPolicy};
use crate::terlan_typeck::{CoreExport, CoreExportKind, CoreImportKind, CoreModule};
use crate::CliState;

use super::super::BuildOneError;
use super::compile::{compile_vm_module, CompiledVmModule};
use super::native_debug::NativeDebugInput;
use super::parallel_compile::{bounded_worker_limit, run_indexed_bounded, ParallelTaskError};
use super::{native_image, std_source};

/// Serialized checked module plus the small data required before body loading.
struct SerializedVmModule {
    source_path: String,
    module: String,
    exports: Vec<CoreExport>,
    has_main: bool,
    imports: Vec<String>,
    core: Vec<u8>,
    interface: String,
    native_debug: NativeDebugInput,
}

/// Builds an application without retaining every unpruned CoreIR tree.
pub(super) fn build_vm_application_artifacts(
    paths: &[PathBuf],
    state: &CliState,
    policy: NativeCodegenPolicy,
    entry_module: Option<&str>,
) -> Result<(), BuildOneError> {
    let mut serialized = match run_indexed_bounded(paths, bounded_worker_limit(), |path| {
        let started = Instant::now();
        let module = compile_vm_module(&path.to_string_lossy(), state)?;
        let module = serialize_module(module)?;
        if state.timings {
            eprintln!(
                "terlc timing: vm.low-memory.frontend: {}: {}ms",
                path.display(),
                started.elapsed().as_millis()
            );
        }
        Ok::<_, BuildOneError>(module)
    }) {
        Ok(modules) => modules,
        Err(ParallelTaskError::Task(error)) => return Err(error),
        Err(ParallelTaskError::WorkerPanicked) => {
            return Err(BuildOneError::Message(
                "error[build.parallel_frontend_panic]: low-memory frontend worker panicked".into(),
            ));
        }
    };
    let mut present = BTreeSet::new();
    let mut pending_std = VecDeque::new();

    for module in &serialized {
        pending_std.extend(module.imports.iter().cloned());
        present.insert(module.module.clone());
    }
    if state.no_emit {
        return Ok(());
    }

    let active_path = paths
        .first()
        .map(PathBuf::as_path)
        .ok_or_else(|| BuildOneError::Message("VM application has no source paths".into()))?;
    let mut imported = Vec::new();
    while !pending_std.is_empty() {
        let mut wave = BTreeMap::new();
        while let Some(module_name) = pending_std.pop_front() {
            if present.contains(&module_name) || wave.contains_key(&module_name) {
                continue;
            }
            if let Some(path) = crate::commands::source_libraries::imported_std_source_path(
                &module_name,
                active_path,
            ) {
                wave.insert(module_name, path);
            }
        }
        if wave.is_empty() {
            break;
        }
        let wave = wave.into_values().collect::<Vec<_>>();
        let compiled = match run_indexed_bounded(&wave, bounded_worker_limit(), |path| {
            let started = Instant::now();
            let mut module = compile_vm_module(&path.to_string_lossy(), state)?;
            std_source::remove_compiler_intrinsic_functions(&mut module.compiled.core);
            let module = serialize_module(module)?;
            if state.timings {
                eprintln!(
                    "terlc timing: vm.low-memory.std-frontend: {}: {}ms",
                    path.display(),
                    started.elapsed().as_millis()
                );
            }
            Ok::<_, BuildOneError>(module)
        }) {
            Ok(modules) => modules,
            Err(ParallelTaskError::Task(error)) => return Err(error),
            Err(ParallelTaskError::WorkerPanicked) => {
                return Err(BuildOneError::Message(
                    "error[build.parallel_frontend_panic]: low-memory std frontend worker panicked"
                        .into(),
                ));
            }
        };
        for module in compiled {
            present.insert(module.module.clone());
            pending_std.extend(module.imports.iter().cloned());
            imported.push(module);
        }
    }
    imported.sort_by(|left, right| left.module.cmp(&right.module));
    serialized.extend(imported);
    crate::terlan_hir::clear_interface_parse_cache();
    trim_allocator();

    let mut graph = ApplicationReachabilityGraph::default();
    for module in &serialized {
        let core = module.decode_core()?;
        graph.add_module_surface(&core);
        drop(core);
        trim_allocator();
    }

    let entry_index = if let Some(entry_module) = entry_module {
        serialized
            .iter()
            .position(|module| module.module == entry_module)
            .ok_or_else(|| {
                BuildOneError::Message(format!(
                    "VM application is missing selected entry module `{entry_module}`"
                ))
            })?
    } else {
        serialized
            .iter()
            .position(|module| module.module.ends_with(".Main"))
            .or_else(|| (!serialized.is_empty()).then_some(0))
            .ok_or_else(|| BuildOneError::Message("VM application has no modules".into()))?
    };
    let entry = &serialized[entry_index];
    let roots = if entry.has_main {
        vec![(entry.module.clone(), "main".to_string(), 0)]
    } else {
        entry
            .exports
            .iter()
            .filter_map(|export| match export.kind {
                CoreExportKind::Function { arity } => {
                    Some((entry.module.clone(), export.name.clone(), arity))
                }
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let image_stem = entry.module.replace('.', "_");

    let graph_started = Instant::now();
    for module in &serialized {
        let core = module.decode_core()?;
        graph.add_module_calls(&core);
        drop(core);
        trim_allocator();
    }
    graph
        .resolve_roots(&roots)
        .map_err(|error| BuildOneError::Message(error.to_string()))?;
    if state.timings {
        eprintln!(
            "terlc timing: vm.low-memory.reachability: {} modules: {}ms",
            serialized.len(),
            graph_started.elapsed().as_millis()
        );
    }

    let prune_started = Instant::now();
    let mut cores = Vec::with_capacity(serialized.len());
    for module in &serialized {
        let mut core = module.decode_core()?;
        graph.prune_module(&mut core);
        cores.push(core);
        trim_allocator();
    }
    if state.timings {
        eprintln!(
            "terlc timing: vm.low-memory.prune: {} modules: {}ms",
            serialized.len(),
            prune_started.elapsed().as_millis()
        );
    }
    let debug_inputs = serialized
        .into_iter()
        .map(|module| module.native_debug)
        .collect::<Vec<_>>();

    let vm_dir = state.out_dir.join("vm");
    fs::create_dir_all(&vm_dir).map_err(|error| {
        BuildOneError::Message(format!("cannot create VM artifact directory: {error}"))
    })?;
    let native_cache_root = state
        .cache_dir
        .clone()
        .unwrap_or_else(|| state.out_dir.join(".terlan"))
        .join("native-aot");
    native_image::compile_owned_rooted_native_application_image(
        &vm_dir,
        &native_cache_root,
        &image_stem,
        cores,
        native_image::RootedNativeApplicationInput {
            roots: &roots,
            debug_inputs: &debug_inputs,
            policy,
            incremental: state.incremental,
        },
    )?;
    Ok(())
}

fn serialize_module(module: CompiledVmModule) -> Result<SerializedVmModule, BuildOneError> {
    let CompiledVmModule {
        source_path,
        compiled,
        native_debug,
        ..
    } = module;
    let core = compiled.core;
    let serialized = SerializedVmModule {
        source_path,
        module: core.module.clone(),
        exports: core.exports.clone(),
        has_main: core
            .functions
            .iter()
            .any(|function| function.name == "main" && function.arity == 0),
        imports: core
            .imports
            .iter()
            .filter(|import| {
                matches!(
                    import.kind,
                    CoreImportKind::Module | CoreImportKind::TypeModule
                )
            })
            .map(|import| import.module.clone())
            .collect(),
        core: serde_json::to_vec(&core).map_err(|error| {
            BuildOneError::Message(format!(
                "error[build.core_spool_encode]: cannot encode `{}`: {error}",
                core.module
            ))
        })?,
        interface: core.interface.to_terlan_interface_text(),
        native_debug,
    };
    trim_allocator();
    Ok(serialized)
}

impl SerializedVmModule {
    fn decode_core(&self) -> Result<CoreModule, BuildOneError> {
        let mut core = crate::support::deserialize_json_with_depth_limit::<CoreModule>(&self.core)
            .map_err(|error| {
                BuildOneError::Message(format!(
                    "error[build.core_spool_decode]: cannot decode `{}` from `{}`: {error}",
                    self.module, self.source_path
                ))
            })?;
        let syntax = crate::terlan_syntax::parse_interface_module_as_syntax_output(&self.interface)
            .map_err(|error| {
                BuildOneError::Message(format!(
                    "error[build.core_spool_interface]: cannot decode interface for `{}` from `{}`: {error:?}",
                    self.module, self.source_path
                ))
            })?;
        core.interface = crate::terlan_hir::syntax_module_output_to_interface(&syntax);
        Ok(core)
    }
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn trim_allocator() {
    unsafe extern "C" {
        fn malloc_trim(pad: usize) -> i32;
    }
    // SAFETY: this process-wide glibc operation receives no Rust pointers.
    unsafe {
        malloc_trim(0);
    }
}

#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
fn trim_allocator() {}
