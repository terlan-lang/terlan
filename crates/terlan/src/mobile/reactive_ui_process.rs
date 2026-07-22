//! Reactive UI process contracts for AngularTS/native shell integration.
#![allow(dead_code)]
//!
//! Inputs:
//! - Compiler-owned declarations describing a UI process state shape, incoming
//!   UI events, emitted effects, and native/async replies.
//!
//! Outputs:
//! - Validated metadata that later AngularTS bindings, native bridge plumbing,
//!   and deterministic process tests can consume.
//!
//! Transformation:
//! - Keeps process-driven UI explicit and typed before runtime wiring exists.

#[cfg(test)]
#[path = "reactive_ui_process_test.rs"]
mod reactive_ui_process_test;
include!("reactive_ui_process_part_001.rs");
include!("reactive_ui_process_part_002.rs");
