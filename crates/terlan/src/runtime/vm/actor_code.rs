//! Shard-local image generation ownership for actor execution.

use super::super::code_server::{
    VmCodeBinding, VmCodeServerEvent, VmModuleArtifact, VmModuleGenerationSnapshot,
};
use super::{VmActorRuntime, VmProcessId};

impl VmActorRuntime {
    /// Publishes one image generation into this actor runtime only.
    pub(crate) fn publish_image_generation(
        &mut self,
        module: impl Into<String>,
        artifact: VmModuleArtifact,
    ) -> VmCodeServerEvent {
        self.code_server.publish(module, artifact)
    }

    /// Binds or moves one live local actor to this shard's active generation.
    pub(crate) fn switch_actor_to_active_image(
        &mut self,
        actor: VmProcessId,
        module: &str,
    ) -> Result<(VmCodeBinding, Option<VmCodeServerEvent>), String> {
        self.code_server
            .switch_process_to_active(&self.processes, actor, module)
    }

    /// Returns deterministic image-generation rows owned by this shard.
    pub(crate) fn image_generation_snapshots(&self) -> Vec<VmModuleGenerationSnapshot> {
        self.code_server.snapshots()
    }
}
