use super::VmMetaTraceConfig;

impl VmMetaTraceConfig {
    pub(crate) const fn calls_only() -> Self {
        Self { returns: false }
    }

    pub(crate) const fn calls_and_returns() -> Self {
        Self { returns: true }
    }
}
