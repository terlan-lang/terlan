//! Capability-worker executable boundary tests.

use std::ffi::OsString;
use std::io::Cursor;

use super::run;
use crate::terlan_native_boundary::capability_sandbox::LINUX_BWRAP_PROFILE;

/// Rejects the retired application-image positional argument.
#[test]
fn worker_rejects_application_image_arguments() {
    let error = run(
        vec![OsString::from("application.tvm")],
        Cursor::new(Vec::<u8>::new()),
        Vec::<u8>::new(),
    )
    .expect_err("application images are not capability-worker arguments");

    assert_eq!(
        error.to_string(),
        "error[capability_worker.args]: unsupported argument `application.tvm`"
    );
}

/// Rejects direct startup that bypasses the mandatory operating-system sandbox.
#[test]
fn worker_rejects_unsandboxed_startup() {
    let error = run(Vec::new(), Cursor::new(Vec::<u8>::new()), Vec::new())
        .expect_err("unsandboxed worker must fail");

    assert_eq!(
        error.to_string(),
        "error[capability_worker.sandbox]: a sandbox profile is required"
    );
}

/// Rejects startup that does not identify an explicit worker-only profile.
#[test]
fn worker_rejects_implicit_execution_profile() {
    let error = run(
        vec![
            OsString::from("--sandbox-profile"),
            OsString::from(LINUX_BWRAP_PROFILE),
        ],
        Cursor::new(Vec::<u8>::new()),
        Vec::<u8>::new(),
    )
    .expect_err("implicit execution profile must fail");

    assert!(error.to_string().contains("execution profile is required"));
}
