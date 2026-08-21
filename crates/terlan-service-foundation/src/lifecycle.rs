use std::fmt;

use serde::{Deserialize, Serialize};

/// Host-independent phase of a service instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecyclePhase {
    Starting,
    Ready,
    Draining,
    Stopped,
    Failed,
}

/// Portable liveness, readiness, and drain state exposed by a service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthState {
    pub live: bool,
    pub ready: bool,
    pub draining: bool,
}

/// Maximum work and time permitted while completing a graceful drain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DrainBounds {
    pub max_in_flight: u32,
    pub max_actors: u32,
    pub max_native_resources: u32,
    pub max_flush_millis: u64,
}

/// Observed work remaining when a graceful drain is finalized.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DrainProgress {
    pub in_flight: u32,
    pub actors: u32,
    pub native_resources: u32,
    pub elapsed_millis: u64,
    pub telemetry_flushed: bool,
}

/// Validates service readiness and graceful-shutdown phase transitions.
#[derive(Debug, Clone)]
pub struct Lifecycle {
    phase: LifecyclePhase,
    bounds: DrainBounds,
}

impl Lifecycle {
    /// Creates a starting lifecycle with validated drain bounds.
    pub fn new(bounds: DrainBounds) -> Result<Self, LifecycleError> {
        if bounds.max_flush_millis == 0 {
            return Err(LifecycleError::InvalidBounds);
        }
        Ok(Self {
            phase: LifecyclePhase::Starting,
            bounds,
        })
    }

    /// Returns the current lifecycle phase.
    pub fn phase(&self) -> LifecyclePhase {
        self.phase
    }

    /// Projects the current lifecycle phase into a portable health state.
    pub fn health(&self) -> HealthState {
        HealthState {
            live: !matches!(self.phase, LifecyclePhase::Stopped | LifecyclePhase::Failed),
            ready: self.phase == LifecyclePhase::Ready,
            draining: self.phase == LifecyclePhase::Draining,
        }
    }

    /// Transitions a starting service to the ready phase.
    pub fn mark_ready(&mut self) -> Result<(), LifecycleError> {
        if self.phase != LifecyclePhase::Starting {
            return Err(LifecycleError::InvalidTransition);
        }
        self.phase = LifecyclePhase::Ready;
        Ok(())
    }

    /// Stops readiness and admission atomically before drain work begins.
    pub fn begin_drain(&mut self) -> Result<(), LifecycleError> {
        if self.phase != LifecyclePhase::Ready {
            return Err(LifecycleError::InvalidTransition);
        }
        self.phase = LifecyclePhase::Draining;
        Ok(())
    }

    /// Reports whether the service may admit new requests.
    pub fn admits_requests(&self) -> bool {
        self.phase == LifecyclePhase::Ready
    }

    /// Completes a drain when all bounded work and telemetry have finished.
    pub fn finish(&mut self, progress: DrainProgress) -> Result<(), LifecycleError> {
        if self.phase != LifecyclePhase::Draining {
            return Err(LifecycleError::InvalidTransition);
        }
        let bounded = progress.in_flight <= self.bounds.max_in_flight
            && progress.actors <= self.bounds.max_actors
            && progress.native_resources <= self.bounds.max_native_resources
            && progress.elapsed_millis <= self.bounds.max_flush_millis
            && progress.in_flight == 0
            && progress.actors == 0
            && progress.native_resources == 0
            && progress.telemetry_flushed;
        self.phase = if bounded {
            LifecyclePhase::Stopped
        } else {
            LifecyclePhase::Failed
        };
        if bounded {
            Ok(())
        } else {
            Err(LifecycleError::DrainIncomplete)
        }
    }
}

/// Stable lifecycle validation and transition failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleError {
    InvalidBounds,
    InvalidTransition,
    DrainIncomplete,
}

impl fmt::Display for LifecycleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid service lifecycle: {self:?}")
    }
}

impl std::error::Error for LifecycleError {}

#[cfg(test)]
#[path = "lifecycle_test.rs"]
mod tests;
