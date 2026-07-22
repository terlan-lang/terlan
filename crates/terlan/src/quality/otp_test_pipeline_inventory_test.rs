use super::*;

/// Verifies required inventory text accepts the checked labels and paths.
///
/// Inputs:
/// - Synthetic inventory text containing required terms and path rows.
///
/// Output:
/// - Test passes when text-only validation reports no diagnostics.
///
/// Transformation:
/// - Protects the inventory contract without reading repository files.
#[test]
fn inventory_text_accepts_required_terms_and_paths() {
    let mut text = REQUIRED_TERMS.join("\n");
    for path in REQUIRED_INVENTORY_PATHS {
        text.push_str("\n| `");
        text.push_str(path);
        text.push_str("` | surface | OTP | ");
        if CLOSED_DEFAULT_RELEASE_ROWS.contains(path) {
            text.push_str("default-release-gate");
        } else {
            text.push_str("reference-only");
        }
        text.push_str(" | replace |");
    }

    assert!(validate_inventory_text(&text).is_empty());
}

/// Verifies closed default release rows cannot remain migration lanes.
///
/// Inputs:
/// - Synthetic inventory text where closed VM/default rows are still marked as
///   migration lanes.
///
/// Output:
/// - Test passes when the stale classification is diagnosed.
///
/// Transformation:
/// - Keeps public run/test/repl command paths and the root Makefile classified
///   as VM-default gates after removed runtime spellings become rejection
///   coverage.
#[test]
fn inventory_text_rejects_closed_default_release_rows_as_migration_lanes() {
    let mut text = REQUIRED_TERMS.join("\n");
    for path in REQUIRED_INVENTORY_PATHS {
        text.push_str("\n| `");
        text.push_str(path);
        text.push_str("` | surface | OTP | migration-lane | replace |");
    }

    let diagnostics = validate_inventory_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("commands/run")),
        "diagnostics should reject stale command migration row: {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("Makefile")),
        "diagnostics should reject stale Makefile migration row: {diagnostics:?}"
    );
}

/// Verifies only checked concrete migration rows can remain migration lanes.
///
/// Inputs:
/// - Synthetic inventory text containing the allowed migration rows.
/// - One additional unexpected migration row.
///
/// Output:
/// - Test passes when the unexpected migration row is diagnosed.
///
/// Transformation:
/// - Locks the 0.0.7 inventory so new OTP/VM migration lanes cannot be
///   introduced without explicitly updating the allowlist.
#[test]
fn inventory_text_rejects_unexpected_migration_lanes() {
    let mut text = REQUIRED_TERMS.join("\n");
    for path in ALLOWED_MIGRATION_ROWS {
        text.push_str("\n| `");
        text.push_str(path);
        text.push_str("` | surface | OTP | migration-lane | replace |");
    }
    text.push_str("\n| `tools/new_otp_gate.py` | surface | OTP | migration-lane | replace |");

    let diagnostics = validate_allowed_migration_rows(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("tools/new_otp_gate.py")),
        "diagnostics should reject unexpected migration lanes: {diagnostics:?}"
    );
}

/// Verifies cleaned VM-default fixture files reject stale VM artifact names.
///
/// Inputs:
/// - Temporary copies of the checked VM-default fixture files.
/// - One fixture containing `beam-thin`.
///
/// Output:
/// - Test passes when the stale fixture text is diagnosed.
///
/// Transformation:
/// - Keeps VM artifact and project-layout tests from drifting back to VM
///   manifest defaults after the 0.0.7 runtime pivot.
#[test]
fn vm_default_fixture_audit_rejects_stale_beam_thin_text() {
    let root = std::env::temp_dir().join(format!(
        "terlan_otp_pipeline_fixture_audit_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    for path in VM_DEFAULT_FIXTURE_FREE_PATHS {
        let full_path = root.join(path);
        fs::create_dir_all(full_path.parent().expect("fixture parent")).expect("create parent");
        fs::write(&full_path, "artifact = \"terlan-vm\"\n").expect("write clean fixture");
    }
    fs::write(
        root.join(VM_DEFAULT_FIXTURE_FREE_PATHS[0]),
        "artifact = \"beam-thin\"\n",
    )
    .expect("write stale fixture");

    let diagnostics =
        validate_no_stale_vm_fixture_beam_artifacts(&root).expect("fixture audit diagnostics");

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("beam-thin")),
        "diagnostics should reject stale VM fixture text: {diagnostics:?}"
    );
    let _ = fs::remove_dir_all(root);
}

/// Verifies build-test files require explicit inventory rows.
///
/// Inputs:
/// - A temporary build-test directory with one Rust test file.
/// - Inventory text containing only the broad build-test directory row.
///
/// Output:
/// - Test passes when the concrete file is diagnosed as missing.
///
/// Transformation:
/// - Protects the 0.0.7 inventory from accepting new build tests that are
///   hidden behind the broad migration-lane directory row.
#[test]
fn build_test_file_row_audit_rejects_unclassified_test_files() {
    let root = std::env::temp_dir().join(format!(
        "terlan_otp_pipeline_build_test_file_audit_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let test_dir = root.join(BUILD_TEST_DIR);
    fs::create_dir_all(&test_dir).expect("create build-test fixture dir");
    fs::write(
        test_dir.join("new_behavior_test.rs"),
        "#[test]\nfn sample() {}\n",
    )
    .expect("write build-test fixture");

    let inventory = format!(
        "{}\n| `{}` | build tests | OTP | migration-lane | replace |",
        REQUIRED_TERMS.join("\n"),
        BUILD_TEST_DIR
    );

    let diagnostics =
        validate_build_test_file_rows(&root, &inventory).expect("build-test row diagnostics");

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("new_behavior_test.rs")),
        "diagnostics should reject unclassified build-test files: {diagnostics:?}"
    );
    let _ = fs::remove_dir_all(root);
}

/// Verifies selected directory surfaces read Rust files for marker scanning.
///
/// Inputs:
/// - A temporary selected-surface directory with one Rust source file and one
///   non-Rust file.
///
/// Output:
/// - Test passes when only the Rust file content is returned.
///
/// Transformation:
/// - Lets the OTP pipeline inventory classify command directories such as
///   `emit_native_metadata` without treating every helper file as a separate
///   inventory row.
#[test]
fn selected_surface_reader_concatenates_rust_files_in_directories() {
    let root = std::env::temp_dir().join(format!(
        "terlan_otp_pipeline_selected_surface_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let surface = "crates/terlan/src/commands/example";
    let dir = root.join(surface);
    fs::create_dir_all(&dir).expect("create selected surface dir");
    fs::write(dir.join("one.rs"), "erlc marker\n").expect("write rust source");
    fs::write(dir.join("note.txt"), "ignored marker\n").expect("write non-rust source");

    let text = read_selected_surface_text(&root, surface).expect("read selected surface");

    assert!(text.contains("erlc marker"));
    assert!(!text.contains("ignored marker"));
    let _ = fs::remove_dir_all(root);
}

/// Verifies missing rows are reported.
///
/// Inputs:
/// - Synthetic inventory text with all terms but no path table rows.
///
/// Output:
/// - Test passes when the missing Makefile row is diagnosed.
///
/// Transformation:
/// - Prevents the gate from accepting prose-only inventories.
#[test]
fn inventory_text_rejects_missing_path_rows() {
    let diagnostics = validate_inventory_text(&REQUIRED_TERMS.join("\n"));

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic
                .contains("missing OTP test/pipeline inventory row `Makefile`")),
        "diagnostics should report missing path row: {diagnostics:?}"
    );
}

/// Verifies placeholder text is rejected from the inventory.
///
/// Inputs:
/// - Synthetic inventory text with required terms, required rows, and one
///   placeholder line.
///
/// Output:
/// - Test passes when placeholder text is diagnosed.
///
/// Transformation:
/// - Keeps the OTP test/pipeline inventory executable rather than
///   future-planned prose.
#[test]
fn inventory_text_rejects_placeholder_wording() {
    let mut text = REQUIRED_TERMS.join("\n");
    for path in REQUIRED_INVENTORY_PATHS {
        text.push_str("\n| `");
        text.push_str(path);
        text.push_str("` | surface | OTP | ");
        if CLOSED_DEFAULT_RELEASE_ROWS.contains(path) {
            text.push_str("default-release-gate");
        } else {
            text.push_str("reference-only");
        }
        text.push_str(" | replace |");
    }
    text.push_str("\nTODO: classify later.");

    let diagnostics = validate_inventory_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("placeholder OTP test/pipeline inventory text")),
        "diagnostics should reject placeholder text: {diagnostics:?}"
    );
}

/// Verifies OTP marker detection includes Vm tools.
///
/// Inputs:
/// - One text sample containing `erlc`.
/// - One text sample without OTP-related words.
///
/// Output:
/// - Test passes when only the Vm-tool sample is marked.
///
/// Transformation:
/// - Keeps selected pipeline scanning focused on explicit OTP-era markers.
#[test]
fn otp_marker_detection_finds_erlang_tooling() {
    assert!(contains_otp_marker("erlc generated.erl"));
    assert!(contains_otp_marker("erl -noshell"));
    assert!(!contains_otp_marker("terlc test fixture.terl"));
    assert!(!contains_otp_marker("terlan-vm runs application.tvm"));
}
