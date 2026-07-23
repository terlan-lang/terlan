//! Explicit development and release policy for native object generation.

/// Native code-generation policy selected by the build command or runtime tool.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NativeCodegenPolicy {
    /// Fast code generation with independently reusable module objects.
    Development,
    /// Speed-optimized generated code with modular units for live serving.
    Serve,
    /// Speed-optimized whole-application code generation and optimized linking.
    Release,
}

impl NativeCodegenPolicy {
    /// Returns the stable cache identity for this policy.
    pub(crate) fn cache_identity(self) -> &'static str {
        match self {
            Self::Development => "development-cranelift-none-modular-link-v1",
            Self::Serve => "serve-cranelift-speed-modular-link-v1",
            Self::Release => "release-cranelift-speed-whole-application-link-v1",
        }
    }

    /// Returns the Cranelift optimization level selected by this policy.
    pub(super) fn cranelift_opt_level(self) -> &'static str {
        match self {
            Self::Development => "none",
            Self::Serve | Self::Release => "speed",
        }
    }

    /// Reports whether independently cacheable module objects are preferred.
    pub(crate) fn uses_incremental_module_units(self) -> bool {
        matches!(self, Self::Development | Self::Serve)
    }

    /// Reports whether the final native linker should optimize the image.
    pub(crate) fn optimizes_link(self) -> bool {
        matches!(self, Self::Release)
    }
}
