use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

fn temp_dir(name: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("terlan_migrate_{name}_{stamp}"));
    fs::create_dir_all(&path).expect("create temp dir");
    path
}

fn write_source(path: &Path, source: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent");
    }
    fs::write(path, source).expect("write source");
}

/// Verifies dry-run mode reports safe reverse-alias rewrites without changing source.
#[test]
fn pattern_head_migration_dry_run_reports_plan_without_writing() {
    let root = temp_dir("dry_run_reports_plan");
    let source_path = root.join("src").join("app.terl");
    let source = "\
module app.

pub full_name(user = {name, family_name}: User): String ->
    name + family_name.
";
    write_source(&source_path, source);

    let report = run_pattern_head_migration(&root, false).expect("dry-run migration");

    assert_eq!(report.planned_count, 1);
    assert_eq!(report.applied_count, 0);
    assert_eq!(report.safe_rejected_count, 0);
    assert_eq!(report.changed_file_count, 1);
    assert_eq!(report.changes[0].function_name, "full_name");
    assert_eq!(report.changes[0].arity, 1);
    assert_eq!(
        report.changes[0].migration_id,
        "migration.function_head_pattern.invalid_alias_style"
    );
    assert_eq!(
        fs::read_to_string(&source_path).expect("read source"),
        source,
        "dry-run must not write"
    );
}

/// Verifies write mode applies only the safe pattern-first rewrite.
#[test]
fn pattern_head_migration_write_rewrites_safe_reverse_alias() {
    let root = temp_dir("write_rewrites_safe_reverse_alias");
    let source_path = root.join("app.terl");
    write_source(
        &source_path,
        "\
module app.

pub full_name(user = {name, family_name}: User): String ->
    name + family_name.
",
    );

    let report = run_pattern_head_migration(&source_path, true).expect("write migration");

    assert_eq!(report.applied_count, 1);
    assert_eq!(report.planned_count, 0);
    let rewritten = fs::read_to_string(&source_path).expect("read rewritten source");
    assert!(rewritten.contains("pub full_name({name, family_name} = user: User): String ->"));
}

/// Verifies ambiguous reverse-alias candidates are left unchanged with a reason.
#[test]
fn pattern_head_migration_safe_rejects_ambiguous_alias_shape() {
    let root = temp_dir("safe_rejects_ambiguous_alias");
    let source_path = root.join("app.terl");
    let source = "\
module app.

pub full_name(user = name: User): String ->
    name.
";
    write_source(&source_path, source);

    let report = run_pattern_head_migration(&source_path, true).expect("safe reject migration");

    assert_eq!(report.applied_count, 0);
    assert_eq!(report.safe_rejected_count, 1);
    assert!(report.changes[0].reason.contains("manual review"));
    assert_eq!(
        fs::read_to_string(&source_path).expect("read source"),
        source
    );
}

/// Verifies already-migrated pattern-first heads stay idempotent.
#[test]
fn pattern_head_migration_is_idempotent_for_pattern_first_heads() {
    let root = temp_dir("idempotent_pattern_first");
    let source_path = root.join("app.terl");
    let source = "\
module app.

pub full_name({name, family_name} = user: User): String ->
    user.id.to_string() + name + family_name.
";
    write_source(&source_path, source);

    let report = run_pattern_head_migration(&source_path, true).expect("idempotent migration");

    assert_eq!(report.applied_count, 0);
    assert_eq!(report.planned_count, 0);
    assert_eq!(report.safe_rejected_count, 0);
    assert_eq!(
        fs::read_to_string(&source_path).expect("read source"),
        source
    );
}

/// Verifies JSON output exposes stable migration IDs for editor/CI consumers.
#[test]
fn pattern_head_migration_json_report_uses_stable_schema_and_ids() {
    let root = temp_dir("json_report");
    let source_path = root.join("app.terl");
    write_source(
        &source_path,
        "\
module app.

pub full_name(user = {name}: User): String ->
    name.
",
    );

    let report = run_pattern_head_migration(&source_path, false).expect("dry-run migration");
    let json = render_json_report(&report);

    assert!(json.contains("terlan.function-head-pattern-migration-assist-report.v1"));
    assert!(json.contains("migration.function_head_pattern.invalid_alias_style"));
    assert!(json.contains("\"status\":\"planned\""));
}
