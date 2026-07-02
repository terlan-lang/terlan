#![allow(dead_code)]

use std::collections::BTreeMap;

use super::process::{VmProcessId, VmProcessState, VmProcessTable};

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
}

impl VmModuleArtifact {
    /// Creates module artifact metadata used for inspection and reload checks.
    pub(crate) fn new(checksum: impl Into<String>, source_map_id: impl Into<String>) -> Self {
        Self {
            checksum: checksum.into(),
            source_map_id: source_map_id.into(),
        }
    }
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
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct VmModuleGeneration {
    module: String,
    generation: VmModuleGenerationId,
    artifact: VmModuleArtifact,
    state: VmModuleGenerationState,
    active_processes: usize,
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
                active.state = if active.active_processes == 0 {
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
            active_processes: 0,
        });

        match previous {
            Some((previous_generation, previous_state)) => VmCodeServerEvent::HotReloaded {
                module,
                previous_generation,
                previous_state,
                active_generation: generation,
            },
            None => VmCodeServerEvent::Published { module, generation },
        }
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
        generation.active_processes = generation.active_processes.saturating_add(1);
        Ok(VmCodeBinding {
            pid,
            module: module.to_string(),
            generation: generation.generation,
        })
    }

    /// Releases a process binding and retires a drained generation when needed.
    pub(crate) fn release_process(
        &mut self,
        binding: &VmCodeBinding,
    ) -> Result<Option<VmCodeServerEvent>, String> {
        let generation = self.generation_mut(&binding.module, binding.generation)?;
        generation.active_processes = generation.active_processes.saturating_sub(1);
        if generation.state == VmModuleGenerationState::Retiring && generation.active_processes == 0
        {
            generation.state = VmModuleGenerationState::Retired;
            return Ok(Some(VmCodeServerEvent::GenerationRetired {
                module: binding.module.clone(),
                generation: binding.generation,
            }));
        }
        Ok(None)
    }

    /// Returns the active generation id for a module.
    pub(crate) fn active_generation(&self, module: &str) -> Result<VmModuleGenerationId, String> {
        Ok(self.active_generation_ref(module)?.generation)
    }

    /// Returns generation rows for runtime inspection.
    pub(crate) fn snapshots(&self) -> Vec<VmModuleGenerationSnapshot> {
        self.modules
            .values()
            .flat_map(|generations| {
                generations
                    .iter()
                    .map(|generation| VmModuleGenerationSnapshot {
                        module: generation.module.clone(),
                        generation: generation.generation,
                        state: generation.state,
                        active_processes: generation.active_processes,
                        checksum: generation.artifact.checksum.clone(),
                        source_map_id: generation.artifact.source_map_id.clone(),
                    })
            })
            .collect()
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

#[cfg(test)]
#[path = "code_server_test.rs"]
mod code_server_test;
