use super::*;

impl Default for VmActorRuntime {
    fn default() -> Self {
        Self::with_memory_limits(
            VmMemoryLimits::new(64 * 1024 * 1024, 256 * 1024 * 1024)
                .expect("actor runtime memory limits are valid"),
        )
    }
}

impl VmActorRuntime {
    /// Creates an actor runtime with explicit validated VM memory limits.
    pub(crate) fn with_memory_limits(limits: VmMemoryLimits) -> Self {
        Self::with_runtime_identity(limits, "local", 1)
            .expect("default actor runtime identity is valid")
    }

    /// Creates an actor runtime with explicit memory and reference identity.
    pub(crate) fn with_runtime_identity(
        limits: VmMemoryLimits,
        node_id: impl Into<String>,
        epoch: u64,
    ) -> Result<Self, String> {
        Self::with_runtime_identity_and_scheduler(limits, node_id, epoch, NonZeroU64::MIN)
    }

    /// Creates an actor runtime owned by one fixed scheduler identity.
    pub(crate) fn with_scheduler_owner(owner: NonZeroU64) -> Result<Self, String> {
        Self::with_runtime_identity_and_scheduler(
            VmMemoryLimits::new(64 * 1024 * 1024, 256 * 1024 * 1024)?,
            "local",
            1,
            owner,
        )
    }

    /// Creates an actor runtime with explicit reference and scheduler identity.
    fn with_runtime_identity_and_scheduler(
        limits: VmMemoryLimits,
        node_id: impl Into<String>,
        epoch: u64,
        scheduler_owner: NonZeroU64,
    ) -> Result<Self, String> {
        Ok(Self {
            processes: VmProcessTable::default(),
            aliases: VmProcessAliasTable::default(),
            failures: VmFailureRuntime::default(),
            references: VmReferenceAllocator::new(node_id, epoch)?,
            scheduler: VmScheduler::with_owner(Default::default(), scheduler_owner),
            memory: VmMemoryAccountant::new(limits),
            resources: VmResourceTable::default(),
            #[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
            code_server: VmCodeServer::default(),
            #[cfg(test)]
            dynamic_modules: VmDynamicModuleRegistry::default(),
            timers: VmTimerTable::default(),
            delayed_messages: BTreeMap::new(),
            native_continuations: BTreeMap::new(),
            native_continuations_by_owner: BTreeMap::new(),
            explicit_native_suspensions: BTreeSet::new(),
            #[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
            postgres: VmPostgresRuntime::new(1_024),
            #[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
            postgres_driver: VmPostgresLibpqWorker::default(),
            #[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
            postgres_controls: VecDeque::new(),
            #[cfg(test)]
            call_counts: VmCallCountRegistry::default(),
            #[cfg(test)]
            call_memory: VmCallMemoryRegistry::default(),
            #[cfg(test)]
            call_time: VmCallTimeRegistry::default(),
            local_trace: VmLocalTraceRegistry::default(),
            meta_trace: VmMetaTraceRegistry::default(),
            latest_fatal_diagnostic: None,
            native_image_diagnostics: None,
        })
    }
}
