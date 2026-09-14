//! Shared child lifetime ownership for VM capabilities and build tools.
//!
//! Linux/macOS own a process group and retain the leader until group cleanup.
//! Nested owners can explicitly remain in an enclosing owner's group; their
//! cleanup then owns only the direct child, leaving whole-group cleanup to the
//! enclosing owner. Default callers still receive isolated process groups.
//! Other platforms currently own only the direct child; this is not a Windows
//! job-object or escaped-session containment implementation.
//!
//! Setting `TERLAN_PROCESS_ACTIVITY_LOG` opts owned launches into bounded JSONL
//! observations. The scope survives command environment clearing; log failures
//! fail the operation without releasing a running child. Arguments and environment
//! values are hashed, not disclosed. These are direct-child observations, not
//! successful build receipts or observations of uninstrumented grandchildren.
//! Nested log destinations are additive: both caller and nested owner retain the
//! same events. Up to eight scopes survive environment clearing; malformed scopes
//! fail before launch. Multi-scope paths must be UTF-8; single paths remain native.

#![forbid(unsafe_code)]

mod child;
mod command;
mod inventory;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub mod pipe;

pub use child::OwnedChild;
pub use command::{
    capture_stdout, capture_stdout_with_launch, run, run_with_launch, CapturedOutput, Failure,
    ProcessControl,
};
