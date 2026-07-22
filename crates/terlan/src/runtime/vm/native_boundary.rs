//! VM-owned asynchronous external-capability lifecycle support.
//!
//! External adapters execute only through the capability-worker protocol. The
//! former in-process `NativeBoundaryRuntime` compatibility executor was removed
//! at the AOT hard cutover.

#[path = "native_boundary/deadline.rs"]
pub(crate) mod deadline;
