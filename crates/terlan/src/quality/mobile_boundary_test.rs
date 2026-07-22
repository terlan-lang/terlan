use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

/// Verifies the mobile boundary accepts the intended source layout.
///
/// Inputs:
/// - A fixture with mobile implementation under `src/mobile`.
/// - Only the typechecker bridge hook under `src/compiler`.
///
/// Output:
/// - Success summary from the boundary gate.
///
/// Transformation:
/// - Proves the quality gate allows explicit compiler integration hooks while
///   rejecting mobile implementation ownership under compiler.
#[test]
fn mobile_boundary_accepts_mobile_module_and_typecheck_hook() {
    let root = temp_repo("mobile_boundary_accepts");
    write_fixture(&root);

    let summary = run_mobile_boundary(&root).expect("boundary should pass");

    assert_eq!(summary.mobile_file_count, 2);
    assert_eq!(
        summary.allowed_hook_count,
        ALLOWED_COMPILER_MOBILE_HOOKS.len()
    );
}

/// Verifies mobile implementation files under compiler are rejected.
///
/// Inputs:
/// - A fixture with `compiler/mobile_bridge.rs`.
///
/// Output:
/// - Diagnostic explaining that mobile implementation belongs under
///   `src/mobile`.
///
/// Transformation:
/// - Prevents future mobile slices from reintroducing compiler-owned mobile
///   implementation files.
#[test]
fn mobile_boundary_rejects_compiler_mobile_implementation_file() {
    let root = temp_repo("mobile_boundary_rejects_compiler_file");
    write_fixture(&root);
    write(&root, "crates/terlan/src/compiler/mobile_bridge.rs", "");

    let error = run_mobile_boundary(&root).expect_err("boundary should fail");

    assert!(error.contains("mobile implementation must live under"));
}

/// Verifies mobile build planning cannot import mobile code through compiler.
///
/// Inputs:
/// - A fixture whose build planner imports `crate::compiler::mobile_bridge`.
///
/// Output:
/// - Diagnostic requiring `crate::mobile` boundary imports.
///
/// Transformation:
/// - Locks commands onto the mobile module instead of compiler internals.
#[test]
fn mobile_boundary_rejects_build_planner_compiler_import() {
    let root = temp_repo("mobile_boundary_rejects_compiler_import");
    write_fixture(&root);
    write(
        &root,
        BUILD_MOBILE_SOURCE,
        "use crate::compiler::mobile_bridge::generate_mobile_bridge_metadata;\n",
    );

    let error = run_mobile_boundary(&root).expect_err("boundary should fail");

    assert!(error.contains("must import from `crate::mobile`"));
}

/// Writes a passing mobile-boundary fixture.
fn write_fixture(root: &Path) {
    write(
        root,
        "crates/terlan/src/mobile/mod.rs",
        "pub mod mobile_bridge;\n",
    );
    write(root, "crates/terlan/src/mobile/README.md", "mobile docs\n");
    write(root, "crates/terlan/src/mobile/mobile_bridge.rs", "");
    write(
        root,
        "crates/terlan/src/compiler/mod.rs",
        "pub mod syntax;\n",
    );
    write(
        root,
        "crates/terlan/src/compiler/typeck/mobile_bridge_validation.rs",
        "",
    );
    write(
        root,
        "crates/terlan/src/compiler/typeck/mobile_bridge_validation_test.rs",
        "",
    );
    write(
        root,
        BUILD_MOBILE_SOURCE,
        "use crate::mobile::mobile_bridge::generate_mobile_bridge_metadata;\n",
    );
}

/// Writes one fixture file, creating parents first.
fn write(root: &Path, relative: &str, text: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().expect("fixture path has parent")).expect("create parent");
    fs::write(path, text).expect("write fixture");
}

/// Creates an isolated temporary repository fixture directory.
fn temp_repo(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("{name}_{}_{}", std::process::id(), nanos));
    fs::create_dir_all(&path).expect("create temp repo");
    path
}
