#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, MutexGuard};

use super::process::{
    VmProcessId, VmProcessLocation, VmProcessSource, VmProcessState, VmProcessTable,
};

/// VM-owned module generation identifier.
///
/// Inputs:
/// - Monotonic runtime allocation.
///
/// Output:
/// - Stable generation id for one published module version.
///
/// Transformation:
/// - Separates Terlan hot-reload identity from BEAM code indexes, host
///   libraries, or file-system artifact names.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct VmModuleGenerationId(u64);

impl VmModuleGenerationId {
    /// Returns the numeric generation id.
    pub(crate) fn as_u64(self) -> u64 {
        self.0
    }
}

/// Descriptor for a published VM module generation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmModuleArtifact {
    pub(crate) checksum: String,
    pub(crate) source_map_id: String,
    exported_functions: BTreeMap<String, BTreeSet<usize>>,
    defined_functions: BTreeMap<String, BTreeSet<usize>>,
}

/// Compiler-verified module artifact that has not entered VM code visibility.
///
/// Transformation:
/// - Keeps the compiler-derived module identity attached to its artifact until
///   explicit publication, preventing compile-before-publish callers from
///   exposing staged code or pairing metadata with the wrong module name.
pub(crate) struct VmStagedModuleArtifact {
    /// Compiler-verified module identity used at publication.
    pub(super) module: String,
    /// Compiler-derived metadata kept invisible until publication.
    pub(super) artifact: VmModuleArtifact,
}

impl VmModuleArtifact {
    /// Creates module artifact metadata used for inspection and reload checks.
    pub(crate) fn new(checksum: impl Into<String>, source_map_id: impl Into<String>) -> Self {
        Self {
            checksum: checksum.into(),
            source_map_id: source_map_id.into(),
            exported_functions: BTreeMap::new(),
            defined_functions: BTreeMap::new(),
        }
    }

    /// Attaches the compiler-verified public function manifest.
    pub(super) fn with_exported_functions(
        mut self,
        exports: impl IntoIterator<Item = (String, usize)>,
    ) -> Self {
        for (function, arity) in exports {
            self.exported_functions
                .entry(function)
                .or_default()
                .insert(arity);
        }
        self
    }

    /// Attaches every compiler-verified function, including private helpers.
    pub(super) fn with_defined_functions(
        mut self,
        functions: impl IntoIterator<Item = (String, usize)>,
    ) -> Self {
        for (function, arity) in functions {
            self.defined_functions
                .entry(function)
                .or_default()
                .insert(arity);
        }
        self
    }

    fn exports_function(&self, function: &str, arity: usize) -> bool {
        self.exported_functions
            .get(function)
            .is_some_and(|arities| arities.contains(&arity))
    }

    fn exports(&self) -> Vec<VmModuleFunction> {
        function_rows(&self.exported_functions)
    }

    fn functions(&self) -> Vec<VmModuleFunction> {
        function_rows(&self.defined_functions)
    }
}

/// One compiler-verified function identity in module metadata.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct VmModuleFunction {
    pub(crate) name: String,
    pub(crate) arity: usize,
}

/// Typed metadata for one active module generation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmModuleInfoSnapshot {
    pub(crate) module: String,
    pub(crate) generation: VmModuleGenerationId,
    pub(crate) checksum: String,
    pub(crate) source_map_id: String,
    pub(crate) exports: Vec<VmModuleFunction>,
    pub(crate) functions: Vec<VmModuleFunction>,
}

/// Runtime lifecycle state for a module generation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmModuleGenerationState {
    Active,
    Retiring,
    Retired,
}

/// Process binding to a specific module generation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmCodeBinding {
    pub(crate) pid: VmProcessId,
    pub(crate) module: String,
    pub(crate) generation: VmModuleGenerationId,
}

/// Read-only code-server row for runtime inspection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmModuleGenerationSnapshot {
    pub(crate) module: String,
    pub(crate) generation: VmModuleGenerationId,
    pub(crate) state: VmModuleGenerationState,
    pub(crate) active_processes: usize,
    pub(crate) checksum: String,
    pub(crate) source_map_id: String,
}

/// Code-server lifecycle event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmCodeServerEvent {
    Published {
        module: String,
        generation: VmModuleGenerationId,
    },
    HotReloaded {
        module: String,
        previous_generation: VmModuleGenerationId,
        previous_state: VmModuleGenerationState,
        active_generation: VmModuleGenerationId,
    },
    GenerationRetired {
        module: String,
        generation: VmModuleGenerationId,
    },
    GenerationPurged {
        module: String,
        generation: VmModuleGenerationId,
    },
}

impl VmCodeServerEvent {
    /// Returns the module that owns this lifecycle event.
    fn module(&self) -> &str {
        match self {
            Self::Published { module, .. }
            | Self::HotReloaded { module, .. }
            | Self::GenerationRetired { module, .. }
            | Self::GenerationPurged { module, .. } => module,
        }
    }
}

/// Read-only code-server event row for runtime inspection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmCodeServerEventSnapshot {
    pub(crate) sequence: u64,
    pub(crate) event: VmCodeServerEvent,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct VmModuleGeneration {
    module: String,
    generation: VmModuleGenerationId,
    artifact: VmModuleArtifact,
    state: VmModuleGenerationState,
    active_processes: BTreeSet<VmProcessId>,
}

impl VmModuleGeneration {
    fn snapshot(&self) -> VmModuleGenerationSnapshot {
        VmModuleGenerationSnapshot {
            module: self.module.clone(),
            generation: self.generation,
            state: self.state,
            active_processes: self.active_processes.len(),
            checksum: self.artifact.checksum.clone(),
            source_map_id: self.artifact.source_map_id.clone(),
        }
    }
}

/// VM-owned module generation registry.
///
/// Inputs:
/// - Published module artifacts and live process generation bindings.
///
/// Output:
/// - Active generation selection, hot-reload state transitions, retired
///   generation draining, and inspection rows.
///
/// Transformation:
/// - Provides Terlan-owned code loading semantics without depending on BEAM
///   code server behavior or VM-specific module replacement rules.
#[derive(Debug, Default)]
pub(crate) struct VmCodeServer {
    next_generation: u64,
    modules: BTreeMap<String, Vec<VmModuleGeneration>>,
    events: Vec<VmCodeServerEventSnapshot>,
}

/// Result of one atomic concurrent publication request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmConcurrentPublishOutcome {
    Published(VmCodeServerEvent),
    Reused {
        module: String,
        generation: VmModuleGenerationId,
    },
}

impl VmConcurrentPublishOutcome {
    pub(crate) fn generation(&self) -> VmModuleGenerationId {
        match self {
            Self::Published(VmCodeServerEvent::Published { generation, .. })
            | Self::Published(VmCodeServerEvent::HotReloaded {
                active_generation: generation,
                ..
            })
            | Self::Reused { generation, .. } => *generation,
            Self::Published(
                VmCodeServerEvent::GenerationRetired { .. }
                | VmCodeServerEvent::GenerationPurged { .. },
            ) => unreachable!("publication cannot produce a retirement or purge event"),
        }
    }
}

/// Thread-safe administrative publication boundary for VM code generations.
///
/// Transformation:
/// - Serializes administrative visibility changes through one VM-owned registry.
/// - Coalesces simultaneous identical artifacts onto the active generation.
/// - Leaves different artifacts to normal generation-retirement semantics.
/// - Exposes no process binding or execution transition API; those operations
///   belong to each shard's lock-free `VmCodeServer`.
#[derive(Clone, Debug, Default)]
pub(crate) struct VmConcurrentCodeServer {
    inner: Arc<Mutex<VmCodeServer>>,
}

impl VmConcurrentCodeServer {
    fn lock(&self) -> Result<MutexGuard<'_, VmCodeServer>, String> {
        self.inner
            .lock()
            .map_err(|_| "VM concurrent code-server lock poisoned".to_string())
    }

    pub(crate) fn publish_if_changed(
        &self,
        module: impl Into<String>,
        artifact: VmModuleArtifact,
    ) -> Result<VmConcurrentPublishOutcome, String> {
        let module = module.into();
        let mut code_server = self.lock()?;
        if let Ok(active) = code_server.active_generation_ref(&module) {
            if active.artifact == artifact {
                return Ok(VmConcurrentPublishOutcome::Reused {
                    module,
                    generation: active.generation,
                });
            }
        }
        Ok(VmConcurrentPublishOutcome::Published(
            code_server.publish(module, artifact),
        ))
    }

    pub(crate) fn purge_retired_generations(
        &self,
        module: &str,
    ) -> Result<Vec<VmCodeServerEvent>, String> {
        self.lock()?.purge_retired_generations(module)
    }

    pub(crate) fn unload_active_generation(
        &self,
        module: &str,
    ) -> Result<VmCodeServerEvent, String> {
        self.lock()?.unload_active_generation(module)
    }

    pub(crate) fn snapshots(&self) -> Result<Vec<VmModuleGenerationSnapshot>, String> {
        Ok(self.lock()?.snapshots())
    }

    pub(crate) fn event_snapshots(&self) -> Result<Vec<VmCodeServerEventSnapshot>, String> {
        Ok(self.lock()?.event_snapshots())
    }
}

impl VmCodeServer {
    /// Publishes a new generation for a module.
    pub(crate) fn publish(
        &mut self,
        module: impl Into<String>,
        artifact: VmModuleArtifact,
    ) -> VmCodeServerEvent {
        let module = module.into();
        self.next_generation = self.next_generation.saturating_add(1);
        let generation = VmModuleGenerationId(self.next_generation);
        let generations = self.modules.entry(module.clone()).or_default();

        let previous = generations
            .iter_mut()
            .find(|candidate| candidate.state == VmModuleGenerationState::Active)
            .map(|active| {
                active.state = if active.active_processes.is_empty() {
                    VmModuleGenerationState::Retired
                } else {
                    VmModuleGenerationState::Retiring
                };
                (active.generation, active.state)
            });

        generations.push(VmModuleGeneration {
            module: module.clone(),
            generation,
            artifact,
            state: VmModuleGenerationState::Active,
            active_processes: BTreeSet::new(),
        });

        let event = match previous {
            Some((previous_generation, previous_state)) => VmCodeServerEvent::HotReloaded {
                module,
                previous_generation,
                previous_state,
                active_generation: generation,
            },
            None => VmCodeServerEvent::Published { module, generation },
        };
        self.record_event(event)
    }

    /// Binds a live process to the current active generation for a module.
    pub(crate) fn bind_process_to_active(
        &mut self,
        processes: &VmProcessTable,
        pid: VmProcessId,
        module: &str,
    ) -> Result<VmCodeBinding, String> {
        ensure_live_process(processes, pid)?;
        let generation = self.active_generation_mut(module)?;
        generation.active_processes.insert(pid);
        Ok(VmCodeBinding {
            pid,
            module: module.to_string(),
            generation: generation.generation,
        })
    }

    /// Atomically moves one process from its retained generation to active code.
    pub(crate) fn switch_process_to_active(
        &mut self,
        processes: &VmProcessTable,
        pid: VmProcessId,
        module: &str,
    ) -> Result<(VmCodeBinding, Option<VmCodeServerEvent>), String> {
        ensure_live_process(processes, pid)?;
        let active_generation = self.active_generation_ref(module)?.generation;
        let previous = self.process_binding(pid, module)?;
        if let Some(previous) = previous {
            if previous.generation == active_generation {
                return Ok((previous, None));
            }
            let retirement = self.release_process(&previous)?;
            let binding = self.bind_process_to_active(processes, pid, module)?;
            return Ok((binding, retirement));
        }
        self.bind_process_to_active(processes, pid, module)
            .map(|binding| (binding, None))
    }

    /// Enters one function while binding the process to its exact module
    /// generation.
    pub(crate) fn enter_process_function(
        &mut self,
        processes: &mut VmProcessTable,
        pid: VmProcessId,
        module: &str,
        function: &str,
        arity: usize,
        entry_instruction_offset: usize,
        return_instruction_offset: usize,
    ) -> Result<VmCodeBinding, String> {
        ensure_live_process(processes, pid)?;
        let existing = self.process_binding(pid, module)?;
        let created_binding = existing.is_none();
        let binding = match existing {
            Some(binding) => binding,
            None => self.bind_process_to_active(processes, pid, module)?,
        };
        let generation = self.generation_ref(module, binding.generation)?;
        if !generation.artifact.exports_function(function, arity) {
            if created_binding {
                self.release_process(&binding)?;
            }
            return Err(format!(
                "module `{module}` generation {} does not export `{function}/{arity}`",
                binding.generation.as_u64()
            ));
        }

        let entered = processes.with_process_control_mutator(pid, |process| {
            process.enter_execution_frame(
                VmProcessSource::new(module, function, arity),
                entry_instruction_offset,
                return_instruction_offset,
            )
        })?;
        if let Err(error) = entered {
            if created_binding {
                self.release_process(&binding)?;
            }
            return Err(error);
        }
        Ok(binding)
    }

    /// Returns from one function and releases a drained module dependency.
    pub(crate) fn return_process_function(
        &mut self,
        processes: &mut VmProcessTable,
        pid: VmProcessId,
    ) -> Result<(VmProcessLocation, Option<VmCodeServerEvent>), String> {
        ensure_live_process(processes, pid)?;
        let module = processes
            .get(pid)
            .expect("live process was validated before function return")
            .current_location()
            .source
            .module
            .clone();
        let binding = self.process_binding(pid, &module)?.ok_or_else(|| {
            format!(
                "process {} has no code binding for current module `{module}`",
                pid.as_u64()
            )
        })?;
        let (returned, module_still_active) =
            processes.with_process_control_mutator(pid, |process| {
                let returned = process.pop_execution_frame()?;
                let module_still_active = process
                    .current_stacktrace()
                    .iter()
                    .any(|location| location.source.module == module);
                Ok::<_, String>((returned, module_still_active))
            })??;
        let event = if module_still_active {
            None
        } else {
            self.release_process(&binding)?
        };
        Ok((returned, event))
    }

    /// Releases a process binding and retires a drained generation when needed.
    pub(crate) fn release_process(
        &mut self,
        binding: &VmCodeBinding,
    ) -> Result<Option<VmCodeServerEvent>, String> {
        let generation = self.generation_mut(&binding.module, binding.generation)?;
        if !generation.active_processes.remove(&binding.pid) {
            return Err(format!(
                "process {} is not bound to generation {} for module `{}`",
                binding.pid.as_u64(),
                binding.generation.as_u64(),
                binding.module
            ));
        }
        if generation.state == VmModuleGenerationState::Retiring
            && generation.active_processes.is_empty()
        {
            generation.state = VmModuleGenerationState::Retired;
            let event = VmCodeServerEvent::GenerationRetired {
                module: binding.module.clone(),
                generation: binding.generation,
            };
            return Ok(Some(self.record_event(event)));
        }
        Ok(None)
    }

    /// Releases every module-generation binding owned by one exiting process.
    pub(crate) fn release_process_bindings(
        &mut self,
        pid: VmProcessId,
    ) -> Result<Vec<VmCodeServerEvent>, String> {
        let bindings = self
            .modules
            .iter()
            .flat_map(|(module, generations)| {
                generations.iter().filter_map(|generation| {
                    generation
                        .active_processes
                        .contains(&pid)
                        .then(|| VmCodeBinding {
                            pid,
                            module: module.clone(),
                            generation: generation.generation,
                        })
                })
            })
            .collect::<Vec<_>>();
        let mut events = Vec::new();
        for binding in bindings {
            if let Some(event) = self.release_process(&binding)? {
                events.push(event);
            }
        }
        Ok(events)
    }

    /// Reclaims artifact metadata for every drained generation of one module.
    pub(crate) fn purge_retired_generations(
        &mut self,
        module: &str,
    ) -> Result<Vec<VmCodeServerEvent>, String> {
        let generations = self
            .modules
            .get_mut(module)
            .ok_or_else(|| format!("module `{module}` has no generations"))?;
        let purged = generations
            .iter()
            .filter(|generation| generation.state == VmModuleGenerationState::Retired)
            .map(|generation| generation.generation)
            .collect::<Vec<_>>();
        generations.retain(|generation| generation.state != VmModuleGenerationState::Retired);

        Ok(purged
            .into_iter()
            .map(|generation| {
                self.record_event(VmCodeServerEvent::GenerationPurged {
                    module: module.to_string(),
                    generation,
                })
            })
            .collect())
    }

    /// Unloads an active module generation when no process still owns it.
    ///
    /// Inputs:
    /// - `module`: module whose active generation should stop resolving.
    ///
    /// Output:
    /// - A retirement event for an unbound active generation.
    /// - A stable error when the module is missing or still process-bound.
    ///
    /// Transformation:
    /// - Validates ownership before changing state, then moves the active
    ///   generation to `Retired` so normal purge and inspection semantics own
    ///   reclamation without BEAM delete/purge behavior.
    pub(crate) fn unload_active_generation(
        &mut self,
        module: &str,
    ) -> Result<VmCodeServerEvent, String> {
        let generation = {
            let active = self.active_generation_mut(module)?;
            if !active.active_processes.is_empty() {
                return Err(format!(
                    "cannot unload active generation {} for module `{module}`: {} process binding(s) remain",
                    active.generation.as_u64(),
                    active.active_processes.len()
                ));
            }
            active.state = VmModuleGenerationState::Retired;
            active.generation
        };
        Ok(self.record_event(VmCodeServerEvent::GenerationRetired {
            module: module.to_string(),
            generation,
        }))
    }

    /// Promotes an existing generation after validating its artifact identity.
    pub(crate) fn promote_generation(
        &mut self,
        module: &str,
        generation: VmModuleGenerationId,
        expected_artifact: &VmModuleArtifact,
    ) -> Result<VmCodeServerEvent, String> {
        let generations = self.modules.get_mut(module).ok_or_else(|| {
            format!(
                "module `{module}` has no generation {}",
                generation.as_u64()
            )
        })?;
        let target_index = generations
            .iter()
            .position(|candidate| candidate.generation == generation)
            .ok_or_else(|| {
                format!(
                    "module `{module}` has no generation {}",
                    generation.as_u64()
                )
            })?;
        let target_artifact = generations[target_index].artifact.clone();
        if target_artifact != *expected_artifact {
            return Err(format!(
                "generation {} for module `{module}` has checksum `{}` and source map `{}`, expected checksum `{}` and source map `{}`",
                generation.as_u64(),
                target_artifact.checksum,
                target_artifact.source_map_id,
                expected_artifact.checksum,
                expected_artifact.source_map_id
            ));
        }

        let previous_index = generations
            .iter()
            .position(|candidate| candidate.state == VmModuleGenerationState::Active)
            .unwrap_or(target_index);
        let previous_generation = generations[previous_index].generation;
        if previous_index != target_index {
            generations[previous_index].state =
                if generations[previous_index].active_processes.is_empty() {
                    VmModuleGenerationState::Retired
                } else {
                    VmModuleGenerationState::Retiring
                };
        }
        let previous_state = generations[previous_index].state;
        generations[target_index].state = VmModuleGenerationState::Active;

        let event = VmCodeServerEvent::HotReloaded {
            module: module.to_string(),
            previous_generation,
            previous_state,
            active_generation: generation,
        };
        Ok(self.record_event(event))
    }

    /// Returns the active generation id for a module.
    pub(crate) fn active_generation(&self, module: &str) -> Result<VmModuleGenerationId, String> {
        Ok(self.active_generation_ref(module)?.generation)
    }

    /// Returns whether a module currently has an active generation.
    pub(crate) fn module_loaded(&self, module: &str) -> bool {
        self.active_generation_ref(module).is_ok()
    }

    /// Returns whether the active generation exports one function signature.
    pub(crate) fn function_exported(&self, module: &str, function: &str, arity: usize) -> bool {
        self.active_generation_ref(module)
            .is_ok_and(|generation| generation.artifact.exports_function(function, arity))
    }

    /// Returns compiler-derived metadata for the active module generation.
    pub(crate) fn active_module_info(&self, module: &str) -> Result<VmModuleInfoSnapshot, String> {
        let generation = self.active_generation_ref(module)?;
        Ok(VmModuleInfoSnapshot {
            module: generation.module.clone(),
            generation: generation.generation,
            checksum: generation.artifact.checksum.clone(),
            source_map_id: generation.artifact.source_map_id.clone(),
            exports: generation.artifact.exports(),
            functions: generation.artifact.functions(),
        })
    }

    /// Returns generation rows for runtime inspection.
    pub(crate) fn snapshots(&self) -> Vec<VmModuleGenerationSnapshot> {
        self.modules
            .values()
            .flatten()
            .map(VmModuleGeneration::snapshot)
            .collect()
    }

    /// Returns generation rows for one module without unrelated runtime state.
    pub(crate) fn snapshots_for_module(&self, module: &str) -> Vec<VmModuleGenerationSnapshot> {
        self.modules
            .get(module)
            .into_iter()
            .flatten()
            .map(VmModuleGeneration::snapshot)
            .collect()
    }

    /// Returns the generation snapshot owned by one validated process binding.
    ///
    /// Transformation:
    /// - Resolves immutable generation identity and verifies the process is
    ///   still an owner before exposing version information to inspectors.
    pub(crate) fn snapshot_for_binding(
        &self,
        binding: &VmCodeBinding,
    ) -> Result<VmModuleGenerationSnapshot, String> {
        let generation = self.generation_ref(&binding.module, binding.generation)?;
        if !generation.active_processes.contains(&binding.pid) {
            return Err(format!(
                "process {} is not bound to generation {} for module `{}`",
                binding.pid.as_u64(),
                binding.generation.as_u64(),
                binding.module
            ));
        }
        Ok(generation.snapshot())
    }

    /// Returns code-server event rows for runtime inspection.
    pub(crate) fn event_snapshots(&self) -> Vec<VmCodeServerEventSnapshot> {
        self.events.clone()
    }

    /// Returns lifecycle events for one module in their global event order.
    pub(crate) fn event_snapshots_for_module(
        &self,
        module: &str,
    ) -> Vec<VmCodeServerEventSnapshot> {
        self.events
            .iter()
            .filter(|snapshot| snapshot.event.module() == module)
            .cloned()
            .collect()
    }

    /// Records one code-server event and returns it to the caller.
    fn record_event(&mut self, event: VmCodeServerEvent) -> VmCodeServerEvent {
        let sequence = self.events.len().saturating_add(1) as u64;
        self.events.push(VmCodeServerEventSnapshot {
            sequence,
            event: event.clone(),
        });
        event
    }

    fn active_generation_ref(&self, module: &str) -> Result<&VmModuleGeneration, String> {
        self.modules
            .get(module)
            .and_then(|generations| {
                generations
                    .iter()
                    .find(|candidate| candidate.state == VmModuleGenerationState::Active)
            })
            .ok_or_else(|| format!("module `{module}` has no active generation"))
    }

    fn active_generation_mut(&mut self, module: &str) -> Result<&mut VmModuleGeneration, String> {
        self.modules
            .get_mut(module)
            .and_then(|generations| {
                generations
                    .iter_mut()
                    .find(|candidate| candidate.state == VmModuleGenerationState::Active)
            })
            .ok_or_else(|| format!("module `{module}` has no active generation"))
    }

    fn generation_ref(
        &self,
        module: &str,
        generation: VmModuleGenerationId,
    ) -> Result<&VmModuleGeneration, String> {
        self.modules
            .get(module)
            .and_then(|generations| {
                generations
                    .iter()
                    .find(|candidate| candidate.generation == generation)
            })
            .ok_or_else(|| {
                format!(
                    "module `{module}` has no generation {}",
                    generation.as_u64()
                )
            })
    }

    fn generation_mut(
        &mut self,
        module: &str,
        generation: VmModuleGenerationId,
    ) -> Result<&mut VmModuleGeneration, String> {
        self.modules
            .get_mut(module)
            .and_then(|generations| {
                generations
                    .iter_mut()
                    .find(|candidate| candidate.generation == generation)
            })
            .ok_or_else(|| {
                format!(
                    "module `{module}` has no generation {}",
                    generation.as_u64()
                )
            })
    }

    fn process_binding(
        &self,
        pid: VmProcessId,
        module: &str,
    ) -> Result<Option<VmCodeBinding>, String> {
        let mut binding = None;
        for generation in self.modules.get(module).into_iter().flatten() {
            if !generation.active_processes.contains(&pid) {
                continue;
            }
            if binding.is_some() {
                return Err(format!(
                    "process {} has multiple code bindings for module `{module}`",
                    pid.as_u64()
                ));
            }
            binding = Some(VmCodeBinding {
                pid,
                module: module.to_string(),
                generation: generation.generation,
            });
        }
        Ok(binding)
    }
}

fn function_rows(functions: &BTreeMap<String, BTreeSet<usize>>) -> Vec<VmModuleFunction> {
    functions
        .iter()
        .flat_map(|(name, arities)| {
            arities.iter().map(|arity| VmModuleFunction {
                name: name.clone(),
                arity: *arity,
            })
        })
        .collect()
}

fn ensure_live_process(processes: &VmProcessTable, pid: VmProcessId) -> Result<(), String> {
    let process = processes
        .get(pid)
        .ok_or_else(|| format!("missing process {}", pid.as_u64()))?;
    if matches!(process.state, VmProcessState::Exited(_)) {
        return Err(format!("process {} has exited", pid.as_u64()));
    }
    Ok(())
}

vm_code_server_test_component! {
    #[path = "code_server_test.rs"]
    mod code_server_test;

    #[path = "code_server_inspection_test.rs"]
    mod code_server_inspection_test;

    #[path = "code_false_dependency_test.rs"]
    mod code_false_dependency_test;

    #[path = "code_parallel_load_beam_suite_parity_test.rs"]
    mod code_parallel_load_beam_suite_parity_test;

    #[path = "multi_load_beam_suite_parity_test.rs"]
    mod multi_load_beam_suite_parity_test;
}
