//! Opt-in fine-grained timing for the backend-neutral formal pipeline.

use std::time::Instant;

/// Environment switch for compiler-development phase profiling.
const PHASE_TIMINGS_ENVIRONMENT: &str = "TERLAN_COMPILER_PHASE_TIMINGS";

/// Low-overhead phase clock enabled only for explicit compiler profiling.
pub(super) struct FormalPipelineTimings<'a> {
    enabled: bool,
    path: &'a str,
    started: Instant,
    last: Instant,
}

impl<'a> FormalPipelineTimings<'a> {
    /// Creates a source-specific phase clock.
    pub(super) fn new(path: &'a str) -> Self {
        let now = Instant::now();
        Self {
            enabled: std::env::var_os(PHASE_TIMINGS_ENVIRONMENT).as_deref()
                == Some(std::ffi::OsStr::new("1")),
            path,
            started: now,
            last: now,
        }
    }

    /// Records one completed phase without changing compiler behavior.
    pub(super) fn mark(&mut self, phase: &str) {
        if !self.enabled {
            return;
        }
        let now = Instant::now();
        eprintln!(
            "terlc timing: frontend.{phase}: {}: +{}ms total={}ms",
            self.path,
            now.duration_since(self.last).as_millis(),
            now.duration_since(self.started).as_millis()
        );
        self.last = now;
    }
}
