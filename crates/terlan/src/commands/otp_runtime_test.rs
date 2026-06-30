use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use super::bundled_runtime_dir_from_exe;

/// Verifies bundled OTP runtime discovery follows the release artifact layout.
///
/// Inputs:
/// - Temporary executable path with sibling `experimental/terlan-vm/bin`
///   containing `erl` and `erlc` files.
///
/// Output:
/// - Resolved bundled runtime path.
///
/// Transformation:
/// - Exercises the release archive layout without requiring a real OTP runtime.
#[test]
fn bundled_runtime_dir_from_exe_accepts_sibling_experimental_payload() {
    let root = unique_temp_dir("terlan-otp-runtime-bundled");
    let exe = root.join("terlc");
    let bin = root.join("experimental").join("terlan-vm").join("bin");
    fs::create_dir_all(&bin).unwrap();
    fs::write(&exe, b"terlc").unwrap();
    fs::write(bin.join("erl"), b"erl").unwrap();
    fs::write(bin.join("erlc"), b"erlc").unwrap();

    assert_eq!(
        bundled_runtime_dir_from_exe(&exe),
        Some(root.join("experimental").join("terlan-vm"))
    );

    fs::remove_dir_all(root).unwrap();
}

/// Verifies installed OTP runtime discovery follows the installer layout.
#[test]
fn bundled_runtime_dir_from_exe_accepts_installed_lib_payload() {
    let root = unique_temp_dir("terlan-otp-runtime-installed");
    let bin_dir = root.join("bin");
    let exe = bin_dir.join("terlc");
    let payload = root
        .join("lib")
        .join("terlan")
        .join("experimental")
        .join("terlan-vm");
    let payload_bin = payload.join("bin");
    fs::create_dir_all(&bin_dir).unwrap();
    fs::create_dir_all(&payload_bin).unwrap();
    fs::write(&exe, b"terlc").unwrap();
    fs::write(payload_bin.join("erl"), b"erl").unwrap();
    fs::write(payload_bin.join("erlc"), b"erlc").unwrap();

    assert_eq!(bundled_runtime_dir_from_exe(&exe), Some(payload));

    fs::remove_dir_all(root).unwrap();
}

/// Verifies bundled runtime discovery rejects incomplete payloads.
#[test]
fn bundled_runtime_dir_from_exe_rejects_missing_runtime_binaries() {
    let root = unique_temp_dir("terlan-otp-runtime-missing");
    let exe = root.join("terlc");
    fs::create_dir_all(root.join("experimental").join("terlan-vm").join("bin")).unwrap();
    fs::write(&exe, b"terlc").unwrap();

    assert_eq!(bundled_runtime_dir_from_exe(&exe), None);

    fs::remove_dir_all(root).unwrap();
}

fn unique_temp_dir(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}-{nanos}"))
}
