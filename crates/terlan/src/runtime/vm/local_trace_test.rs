use super::VmLocalTraceConfig;

impl VmLocalTraceConfig {
    pub(crate) const fn calls_only() -> Self {
        Self {
            calls: true,
            returns: false,
            exceptions: false,
        }
    }

    pub(crate) const fn calls_and_returns() -> Self {
        Self {
            calls: true,
            returns: true,
            exceptions: false,
        }
    }

    pub(crate) const fn all() -> Self {
        Self {
            calls: true,
            returns: true,
            exceptions: true,
        }
    }
}
