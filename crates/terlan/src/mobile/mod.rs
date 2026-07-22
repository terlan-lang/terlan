//! Mobile delivery planning and runtime bridge metadata.
//!
//! Inputs:
//! - Compiler public contracts, mobile target selections, and source metadata.
//!
//! Outputs:
//! - Mobile shell plans, route metadata, bridge metadata, widget metadata, and
//!   native capability metadata.
//!
//! Transformation:
//! - Keeps mobile packaging and shell concerns out of the compiler module while
//!   preserving one crate-level architecture.

pub(crate) mod mobile_android_shell;
pub(crate) mod mobile_angular_bridge;
pub(crate) mod mobile_bridge;
pub(crate) mod mobile_bridge_inspection;
pub(crate) mod mobile_capability;
pub(crate) mod mobile_debug_identity;
pub(crate) mod mobile_ios_shell;
pub(crate) mod mobile_native_error;
pub(crate) mod mobile_route;
pub(crate) mod mobile_shell_parity;
pub(crate) mod mobile_widget;
pub(crate) mod reactive_ui_process;
