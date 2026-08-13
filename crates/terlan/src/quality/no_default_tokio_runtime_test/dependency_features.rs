use std::fs;
use std::path::PathBuf;

use super::super::*;
use super::support::*;

/// Verifies serve HTTP Tokio usage stays explicit migration debt.
///
/// Inputs:
/// - Serve runtime file that mentions Tokio-backed HTTP machinery.
/// - Current serve TLS file that mentions Tokio for ACME issuance.
/// - Current serve test file that mentions Tokio test fixtures.
/// - One invalid serve runtime row classified as a maintained client boundary.
/// - Serve watch, WebSocket, and retired WebSocket test files that try to use
///   old Tokio classifications.
///
/// Output:
/// - Valid migration-debt and test-harness rows produce no diagnostics.
/// - Invalid maintained-boundary classification is rejected.
/// - Watch and WebSocket migration-debt classifications are rejected.
/// - Retired WebSocket test-harness classification is rejected.
///
/// Transformation:
/// - Locks the 0.0.7 rule that `terlc serve` may keep its temporary Tokio path
///   only in the ACME issuance boundary until that moves behind VM-owned
///   worker support, without reopening already cleaned serve submodules.
#[test]
pub(super) fn tokio_inventory_keeps_serve_http_websocket_as_migration_debt_or_tests() {
    let root = make_quality_temp_dir("tokio_serve_http_websocket_migration_debt");
    write_file(
        &root,
        SERVE_TLS_MIGRATION_PATH,
        concat!(
            "let runtime = tokio::runtime::Builder::new_current_thread()\n",
            "    .enable_all()\n",
            "    .build()?;\n",
            "runtime.block_on(issue_acme_certificate_cache(plan));\n",
        ),
    );
    write_file(
        &root,
        "crates/terlan/src/commands/serve/serve_test.rs",
        "tokio::spawn(async {});\n",
    );
    let valid_rows = vec![
        TokioInventoryRow {
            path: PathBuf::from(SERVE_TLS_MIGRATION_PATH),
            classification: "migration-debt".to_string(),
            owner: "http".to_string(),
            notes: "serve ACME migration debt".to_string(),
        },
        TokioInventoryRow {
            path: PathBuf::from("crates/terlan/src/commands/serve/serve_test.rs"),
            classification: "test-harness".to_string(),
            owner: "http-tests".to_string(),
            notes: "serve test harness".to_string(),
        },
    ];
    let references = collect_tokio_reference_files(&root).expect("collect references");

    let valid_diagnostics = validate_tokio_inventory(&root, &valid_rows, &references);

    assert!(
        valid_diagnostics.is_empty(),
        "unexpected valid serve diagnostics: {valid_diagnostics:?}"
    );

    let invalid_rows = vec![TokioInventoryRow {
        path: PathBuf::from(SERVE_TLS_MIGRATION_PATH),
        classification: "maintained-client-boundary".to_string(),
        owner: "http".to_string(),
        notes: "bad maintained runtime lane".to_string(),
    }];
    let invalid_diagnostics = validate_tokio_inventory(&root, &invalid_rows, &references);

    assert!(
        invalid_diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("classification `maintained-client-boundary`")),
        "expected serve maintained-boundary diagnostic: {invalid_diagnostics:?}"
    );

    write_file(
        &root,
        "crates/terlan/src/commands/serve/mod.rs",
        "tokio::runtime::Builder::new_current_thread();\n",
    );
    write_file(
        &root,
        "crates/terlan/src/commands/serve/watch.rs",
        "tokio::sync::mpsc::channel::<()>(1); tokio::time::sleep;\n",
    );
    write_file(
        &root,
        "crates/terlan/src/commands/serve/websocket.rs",
        "tokio_tungstenite::WebSocketStream; tokio::select! {}\n",
    );
    write_file(
        &root,
        "crates/terlan/src/commands/serve/websocket_test.rs",
        "tokio::sync::mpsc::channel::<()>(1);\n",
    );
    let references = collect_tokio_reference_files(&root).expect("collect invalid references");
    let invalid_submodule_rows = vec![
        TokioInventoryRow {
            path: PathBuf::from("crates/terlan/src/commands/serve/mod.rs"),
            classification: "migration-debt".to_string(),
            owner: "http".to_string(),
            notes: "serve entrypoint migration debt".to_string(),
        },
        TokioInventoryRow {
            path: PathBuf::from("crates/terlan/src/commands/serve/watch.rs"),
            classification: "migration-debt".to_string(),
            owner: "http".to_string(),
            notes: "serve watch migration debt".to_string(),
        },
        TokioInventoryRow {
            path: PathBuf::from("crates/terlan/src/commands/serve/websocket.rs"),
            classification: "migration-debt".to_string(),
            owner: "http".to_string(),
            notes: "serve websocket migration debt".to_string(),
        },
        TokioInventoryRow {
            path: PathBuf::from("crates/terlan/src/commands/serve/websocket_test.rs"),
            classification: "test-harness".to_string(),
            owner: "http-tests".to_string(),
            notes: "retired websocket test harness".to_string(),
        },
    ];
    let invalid_submodule_diagnostics =
        validate_tokio_inventory(&root, &invalid_submodule_rows, &references);

    assert!(
        invalid_submodule_diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("commands/serve/mod.rs")),
        "expected serve entrypoint migration-debt diagnostic: {invalid_submodule_diagnostics:?}"
    );
    assert!(
        invalid_submodule_diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("commands/serve/watch.rs")),
        "expected serve watch migration-debt diagnostic: {invalid_submodule_diagnostics:?}"
    );
    assert!(
        invalid_submodule_diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("commands/serve/websocket.rs")),
        "expected serve websocket migration-debt diagnostic: {invalid_submodule_diagnostics:?}"
    );
    assert!(
        invalid_submodule_diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("commands/serve/websocket_test.rs")),
        "expected serve websocket test-harness diagnostic: {invalid_submodule_diagnostics:?}"
    );
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies direct Tokio-family dependencies are parsed from Cargo manifests.
///
/// Inputs:
/// - A Cargo manifest with Tokio dependencies and a Tokio-enabled Hyper utility
///   feature.
///
/// Output:
/// - Sorted direct dependency entries.
///
/// Transformation:
/// - Locks the dependency ratchet used while the default runtime migrates away
///   from Tokio.
#[test]
pub(super) fn tokio_direct_dependency_parser_finds_names_and_tokio_features() {
    let entries = parse_direct_tokio_dependency_entries(
        r#"
[package]
name = "terlan"

[dependencies]
hyper-util = { version = "0.1", features = ["client-legacy", "tokio"] }
tokio = { version = "1", features = ["rt-multi-thread"] }
tokio-postgres = "0.7"
serde = "1"

[dev-dependencies]
tokio-test = "0.4"
"#,
    );

    assert_eq!(
        entries,
        vec![
            "hyper-util[feature:tokio]".to_string(),
            "tokio".to_string(),
            "tokio-postgres".to_string()
        ]
    );
}

/// Verifies the current direct Tokio removal target set is explicit.
///
/// Inputs:
/// - A Cargo manifest shaped like the current runtime dependency lane.
///
/// Output:
/// - The exact direct Tokio-family dependencies that remain as migration
///   debt.
///
/// Transformation:
/// - Makes dependency removal a deliberate ratchet: the set may shrink only
///   when the checked removal plan and this test are updated together.
#[test]
pub(super) fn tokio_direct_dependency_parser_locks_current_removal_target_set() {
    let entries = parse_direct_tokio_dependency_entries(
        r#"
[package]
name = "terlan"

[dependencies]
tokio = { version = "1", features = ["rt"], optional = true }
tokio-postgres = { version = "0.7", features = ["with-serde_json-1"], optional = true }
serde = "1"
"#,
    );

    assert_eq!(
        entries,
        ALLOWED_DIRECT_TOKIO_DEPENDENCIES
            .iter()
            .map(|dependency| dependency.to_string())
            .collect::<Vec<_>>()
    );
}

/// Verifies removed direct Tokio features are detected.
///
/// Inputs:
/// - A Cargo manifest whose direct Tokio dependency includes features already
///   removed from the 0.0.7 default-runtime lane.
///
/// Output:
/// - Removed feature names.
///
/// Transformation:
/// - Keeps broad Tokio capabilities such as sockets and channels from
///   re-entering after the shipped feature set has been narrowed.
#[test]
pub(super) fn tokio_direct_dependency_parser_detects_removed_features() {
    let entries = parse_removed_direct_tokio_features(
        r#"
[package]
name = "terlan"

[dependencies]
tokio = { version = "1", features = ["io-std", "io-util", "macros", "net", "rt-multi-thread", "sync", "time"] }
serde = "1"
"#,
    );

    assert_eq!(
        entries,
        vec![
            "io-std".to_string(),
            "io-util".to_string(),
            "macros".to_string(),
            "net".to_string(),
            "rt-multi-thread".to_string(),
            "sync".to_string(),
            "time".to_string()
        ]
    );
}

/// Verifies unknown direct Tokio features are rejected separately.
///
/// Inputs:
/// - A Cargo manifest whose direct Tokio dependency contains the remaining
///   allowed runtime feature plus a new unapproved feature.
///
/// Output:
/// - The unapproved feature name.
///
/// Transformation:
/// - Prevents the default Tokio feature set from growing without an explicit
///   roadmap-backed decision.
#[test]
pub(super) fn tokio_direct_dependency_parser_detects_unexpected_features() {
    let entries = parse_unexpected_direct_tokio_features(
        r#"
[package]
name = "terlan"

[dependencies]
tokio = { version = "1", features = ["rt", "signal"] }
serde = "1"
"#,
    );

    assert_eq!(entries, vec!["signal".to_string()]);
}

/// Verifies only default runtime dependencies enter the removal target set.
///
/// Inputs:
/// - A Cargo manifest with default, dev, build, and target-specific Tokio
///   dependencies.
///
/// Output:
/// - Only Tokio entries from `[dependencies]`.
///
/// Transformation:
/// - Prevents test harnesses, build tooling, and platform-specific fixtures
///   from being counted as default Terlan runtime dependencies while the Tokio
///   removal gate still scans their source references separately.
#[test]
pub(super) fn tokio_direct_dependency_parser_ignores_non_default_dependency_sections() {
    let entries = parse_direct_tokio_dependency_entries(
        r#"
[package]
name = "terlan"

[build-dependencies]
tokio-build = "1"

[target.'cfg(unix)'.dependencies]
tokio-uds = "0.2"

[dependencies]
tokio = { version = "1", features = ["rt-multi-thread"] }
serde = "1"
tokio-postgres = { version = "0.7", optional = true }
tower-lsp = { version = "0.20", optional = true, features = ["tokio"] }

[dev-dependencies]
tokio-test = "0.4"
hyper-util = { version = "0.1", features = ["tokio"] }
"#,
    );

    assert_eq!(entries, vec!["tokio".to_string()]);
}

/// Verifies optional editor/runtime dependencies are not counted as defaults.
///
/// Inputs:
/// - A Cargo manifest with optional Tokio and Tokio-enabled dependencies under
///   `[dependencies]`.
///
/// Output:
/// - No direct default Tokio dependency entries.
///
/// Transformation:
/// - Lets editor tooling and future adapters move behind explicit features
///   without being treated as shipped default runtime dependencies.
#[test]
pub(super) fn tokio_direct_dependency_parser_ignores_optional_dependencies() {
    let entries = parse_direct_tokio_dependency_entries(
        r#"
[package]
name = "terlan"

[dependencies]
tokio = { version = "1", optional = true, features = ["io-std", "rt"] }
tower-lsp = { version = "0.20", optional = true, features = ["tokio"] }
pg = { package = "tokio-postgres", version = "0.7", optional = true }
serde = "1"
"#,
    );

    assert!(
        entries.is_empty(),
        "expected no default entries: {entries:?}"
    );
}

/// Verifies default-enabled optional dependencies still count as defaults.
///
/// Inputs:
/// - A Cargo manifest where the default feature enables optional Tokio-family
///   dependencies directly and through a nested feature.
///
/// Output:
/// - Direct default Tokio dependency entries.
///
/// Transformation:
/// - Prevents optional dependency syntax from hiding runtime dependencies that
///   are actually enabled by default features.
#[test]
pub(super) fn tokio_direct_dependency_parser_counts_default_enabled_optional_dependencies() {
    let entries = parse_direct_tokio_dependency_entries(
        r#"
[package]
name = "terlan"

[features]
default = ["editor-lsp", "dep:tokio-postgres"]
editor-lsp = ["dep:tokio", "tower-lsp"]

[dependencies]
tokio = { version = "1", optional = true, features = ["io-std", "rt"] }
tokio-postgres = { version = "0.7", optional = true }
tower-lsp = { version = "0.20", optional = true, features = ["tokio"] }
serde = "1"
"#,
    );

    assert_eq!(
        entries,
        vec![
            "tokio".to_string(),
            "tokio-postgres".to_string(),
            "tower-lsp[feature:tokio]".to_string()
        ]
    );
}

/// Verifies Cargo dependency aliases cannot hide direct Tokio-family packages.
///
/// Inputs:
/// - A Cargo manifest where Tokio-backed packages are imported through
///   non-Tokio dependency aliases.
///
/// Output:
/// - The actual package names, not the local aliases.
///
/// Transformation:
/// - Prevents aliasing from bypassing the default runtime dependency removal
///   inventory.
#[test]
pub(super) fn tokio_direct_dependency_parser_detects_renamed_tokio_packages() {
    let entries = parse_direct_tokio_dependency_entries(
        r#"
[package]
name = "terlan"

[dependencies]
pg = { package = "tokio-postgres", version = "0.7" }
tls = { package = "tokio-rustls", version = "0.26" }
http_io = { package = "hyper-util", version = "0.1", features = ["tokio"] }
serde = "1"
"#,
    );

    assert_eq!(
        entries,
        vec![
            "hyper-util[feature:tokio]".to_string(),
            "tokio-postgres".to_string(),
            "tokio-rustls".to_string()
        ]
    );
}

/// Verifies TOML table dependency syntax is parsed by the Tokio gate.
///
/// Inputs:
/// - A Cargo manifest using `[dependencies.<alias>]` tables instead of inline
///   dependency objects.
///
/// Output:
/// - Direct Tokio-family package names and Tokio-enabled feature entries.
///
/// Transformation:
/// - Keeps the dependency-removal gate backed by TOML structure instead of
///   line-oriented manifest parsing.
#[test]
pub(super) fn tokio_direct_dependency_parser_accepts_table_dependency_syntax() {
    let entries = parse_direct_tokio_dependency_entries(
        r#"
[package]
name = "terlan"

[dependencies]
serde = "1"

[dependencies.pg]
package = "tokio-postgres"
version = "0.7"

[dependencies.http_io]
package = "hyper-util"
version = "0.1"
features = ["client-legacy", "tokio"]

[dev-dependencies]
tokio-test = "0.4"
"#,
    );

    assert_eq!(
        entries,
        vec![
            "hyper-util[feature:tokio]".to_string(),
            "tokio-postgres".to_string()
        ]
    );
}

/// Verifies removed runtime dependencies are parsed from default dependencies.
///
/// Inputs:
/// - A Cargo manifest with removed async-client dependencies in default,
///   dev, and target-specific sections.
///
/// Output:
/// - Only removed dependencies from package default `[dependencies]`.
///
/// Transformation:
/// - Keeps previously removed runtime crates from re-entering the shipped
///   runtime dependency graph while allowing test/tooling fixtures to be
///   handled by their own gates.
#[test]
pub(super) fn tokio_removed_runtime_dependency_parser_finds_only_default_dependencies() {
    let entries = parse_removed_runtime_dependency_entries(
        r#"
[package]
name = "terlan"

[dependencies]
futures-util = "0.3"
tokio-tungstenite = "0.28"
tungstenite = "0.28"

[dev-dependencies]
futures-util = "0.3"

[target.'cfg(unix)'.dependencies]
tokio-tungstenite = "0.28"
"#,
    );

    assert_eq!(
        entries,
        vec!["futures-util".to_string(), "tokio-tungstenite".to_string()]
    );
}

/// Verifies Cargo dependency aliases cannot hide removed runtime packages.
///
/// Inputs:
/// - A Cargo manifest where removed runtime dependencies are imported through
///   non-removed dependency aliases.
///
/// Output:
/// - The actual removed package names.
///
/// Transformation:
/// - Keeps removed runtime crates from re-entering under friendlier local
///   aliases after the Tokio/WebSocket removal lane has retired them.
#[test]
pub(super) fn tokio_removed_runtime_dependency_parser_detects_renamed_removed_packages() {
    let entries = parse_removed_runtime_dependency_entries(
        r#"
[package]
name = "terlan"

[dependencies]
ws = { package = "tokio-tungstenite", version = "0.28" }
futures = { package = "futures-util", version = "0.3" }
tungstenite = "0.28"
"#,
    );

    assert_eq!(
        entries,
        vec!["futures-util".to_string(), "tokio-tungstenite".to_string()]
    );
}

/// Verifies unexpected direct Tokio dependencies fail the inventory gate.
///
/// Inputs:
/// - Direct Tokio dependency entries containing a new, unapproved Tokio crate.
/// - A valid Cargo manifest inventory row.
///
/// Output:
/// - Diagnostic naming the unexpected dependency.
///
/// Transformation:
/// - Prevents new Tokio dependencies from being added while old runtime debt is
///   being retired.
#[test]
pub(super) fn tokio_direct_dependency_inventory_rejects_unexpected_entries() {
    let inventory = vec![TokioInventoryRow {
        path: PathBuf::from(CARGO_MANIFEST_PATH),
        classification: "migration-debt".to_string(),
        owner: "cargo".to_string(),
        notes: "Removal plan: tokio-stream -> remove before default VM runtime".to_string(),
    }];
    let direct_dependencies = vec!["tokio-stream".to_string()];

    let diagnostics = validate_direct_tokio_dependencies(&inventory, &direct_dependencies);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("tokio-stream")),
        "expected unexpected direct dependency diagnostic: {diagnostics:?}"
    );
}

/// Verifies removed Postgres driver debt cannot re-enter the default lane.
///
/// Inputs:
/// - Direct Tokio dependency entries containing `tokio-postgres`.
/// - A migration-debt Cargo manifest row that still names only current debt.
///
/// Output:
/// - Diagnostic naming `tokio-postgres` as unexpected.
///
/// Transformation:
/// - Locks the 0.0.7 ratchet that moved Postgres execution into the VM worker.
#[test]
pub(super) fn tokio_direct_dependency_inventory_rejects_reintroduced_tokio_postgres() {
    let inventory = vec![TokioInventoryRow {
        path: PathBuf::from(CARGO_MANIFEST_PATH),
        classification: "migration-debt".to_string(),
        owner: "cargo".to_string(),
        notes: "Removal plan: tokio -> replace with VM scheduler".to_string(),
    }];
    let direct_dependencies = vec!["tokio".to_string(), "tokio-postgres".to_string()];

    let diagnostics = validate_direct_tokio_dependencies(&inventory, &direct_dependencies);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic
                .contains("unexpected direct Tokio dependency `tokio-postgres`")),
        "expected reintroduced tokio-postgres diagnostic: {diagnostics:?}"
    );
}

/// Verifies direct Tokio dependencies require a Cargo manifest inventory row.
///
/// Inputs:
/// - Direct Tokio dependency entries and no inventory row for `Cargo.toml`.
///
/// Output:
/// - Diagnostic requiring the manifest row.
///
/// Transformation:
/// - Keeps the direct dependency list tied to the checked Tokio inventory.
#[test]
pub(super) fn tokio_direct_dependency_inventory_requires_cargo_manifest_row() {
    let diagnostics = validate_direct_tokio_dependencies(&[], &["tokio".to_string()]);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("require an inventory row")),
        "expected inventory-row diagnostic: {diagnostics:?}"
    );
}

/// Verifies direct Tokio dependency inventory rows must be actionable.
///
/// Inputs:
/// - Direct Tokio dependency entries.
/// - A Cargo manifest inventory row that marks migration debt but omits the
///   dependency name and removal/replacement language.
///
/// Output:
/// - Diagnostics requiring an explicit dependency-specific removal plan.
///
/// Transformation:
/// - Prevents the Tokio inventory from becoming a stale count-only checklist.
#[test]
pub(super) fn tokio_direct_dependency_inventory_requires_removal_plan_notes() {
    let inventory = vec![TokioInventoryRow {
        path: PathBuf::from(CARGO_MANIFEST_PATH),
        classification: "migration-debt".to_string(),
        owner: "cargo".to_string(),
        notes: "runtime debt".to_string(),
    }];
    let direct_dependencies = vec!["tokio".to_string()];

    let diagnostics = validate_direct_tokio_dependencies(&inventory, &direct_dependencies);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.contains("must describe removal or replacement") }),
        "expected removal-plan diagnostic: {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("must name `tokio`")),
        "expected dependency-name diagnostic: {diagnostics:?}"
    );
}

/// Verifies direct Tokio removal plans cannot keep deleted dependencies.
///
/// Inputs:
/// - Current direct Tokio dependency entries.
/// - A Cargo manifest inventory row whose structured removal plan still names
///   a dependency that is no longer direct.
///
/// Output:
/// - Diagnostic naming the stale planned dependency.
///
/// Transformation:
/// - Keeps the direct-dependency ratchet honest after a dependency is removed
///   from `Cargo.toml`.
#[test]
pub(super) fn tokio_direct_dependency_inventory_rejects_stale_removal_plan_targets() {
    let inventory = vec![TokioInventoryRow {
        path: PathBuf::from(CARGO_MANIFEST_PATH),
        classification: "migration-debt".to_string(),
        owner: "cargo".to_string(),
        notes: concat!(
            "Removal plan: tokio -> replace with VM scheduler; ",
            "tokio-tungstenite -> removed"
        )
        .to_string(),
    }];
    let direct_dependencies = vec!["tokio".to_string()];

    let diagnostics = validate_direct_tokio_dependencies(&inventory, &direct_dependencies);

    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.contains("stale direct Tokio dependency `tokio-tungstenite`")
        }),
        "expected stale removal-plan diagnostic: {diagnostics:?}"
    );
}

/// Verifies the full gate accepts matching inventory and scanned files.
///
/// Inputs:
/// - Temporary inventory and one classified LSP source file.
///
/// Output:
/// - Success summary with one row and one scanned reference.
///
/// Transformation:
/// - Exercises the disk-backed gate used by `make no-default-tokio-runtime-check`.
#[test]
pub(super) fn no_default_tokio_runtime_gate_accepts_matching_inventory() {
    let root = make_quality_temp_dir("tokio_matching_inventory");
    write_file(
        &root,
        CARGO_MANIFEST_PATH,
        "[package]\nname = \"terlan\"\n\n[dependencies]\nserde = \"1\"\n",
    );
    write_file(
        &root,
        "crates/terlan/src/lsp/server.rs",
        "tokio::runtime::Runtime::new();\n",
    );
    write_file(
        &root,
        TOKIO_INVENTORY_PATH,
        &format!(
            "{INVENTORY_HEADER_ROW}crates/terlan/src/lsp/server.rs\teditor-tooling\tlsp\ttooling only\n"
        ),
    );

    let summary = run_no_default_tokio_runtime(&root).expect("run gate");

    assert_eq!(summary.inventory_row_count, 1);
    assert_eq!(summary.scanned_reference_count, 1);
    assert_eq!(summary.direct_tokio_dependency_count, 0);
    assert_eq!(summary.direct_tokio_dependencies, Vec::<String>::new());
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies malformed Cargo manifests fail the default Tokio runtime gate.
///
/// Inputs:
/// - A temporary `Cargo.toml` with invalid TOML syntax.
///
/// Output:
/// - Stable invalid-manifest diagnostic.
///
/// Transformation:
/// - Prevents dependency inventory parsing from treating malformed manifests
///   as empty dependency sets.
#[test]
pub(super) fn no_default_tokio_runtime_gate_rejects_malformed_cargo_manifest() {
    let root = make_quality_temp_dir("tokio_malformed_manifest");
    write_file(
        &root,
        CARGO_MANIFEST_PATH,
        "[package]\nname = \"terlan\"\n\n[dependencies]\ntokio = { version = \"1\"\n",
    );
    write_file(&root, TOKIO_INVENTORY_PATH, INVENTORY_HEADER_ROW);

    let diagnostic = run_no_default_tokio_runtime(&root).expect_err("malformed manifest");

    assert!(
        diagnostic.contains("invalid Cargo manifest"),
        "expected invalid-manifest diagnostic: {diagnostic}"
    );
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies the full gate rejects direct Tokio dependency names.
///
/// Inputs:
/// - Temporary Cargo manifest with one direct Tokio dependency.
/// - Matching migration-debt inventory row for the manifest.
///
/// Output:
/// - Failure diagnostic carrying the direct dependency name.
///
/// Transformation:
/// - Makes the removal gate actionable by proving diagnostics identify direct
///   Tokio dependencies that re-enter the default dependency lane.
#[test]
pub(super) fn no_default_tokio_runtime_gate_rejects_direct_dependency_names() {
    let root = make_quality_temp_dir("tokio_direct_dependency_summary");
    write_file(
        &root,
        CARGO_MANIFEST_PATH,
        "[package]\nname = \"terlan\"\n\n[dependencies]\ntokio = \"1\"\n",
    );
    write_file(
        &root,
        TOKIO_INVENTORY_PATH,
        &format!(
            "{INVENTORY_HEADER_ROW}{CARGO_MANIFEST_PATH}\tmigration-debt\tcargo\tRemoval plan: tokio -> replace with VM scheduler and VM sockets\n"
        ),
    );

    let diagnostic = run_no_default_tokio_runtime(&root).expect_err("direct tokio rejected");

    assert!(
        diagnostic.contains("unexpected direct Tokio dependency `tokio`"),
        "expected direct tokio diagnostic: {diagnostic}"
    );
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies removed runtime dependencies fail the full Tokio gate.
///
/// Inputs:
/// - Temporary Cargo manifest that reintroduces a removed async-client helper.
/// - Matching inventory row for the Tokio-bearing dependency plan.
///
/// Output:
/// - Diagnostic naming the removed dependency.
///
/// Transformation:
/// - Protects the 0.0.7 dependency-removal ratchet from accepting
///   non-Tokio-named async helpers that were removed with the old Tokio
///   WebSocket client.
#[test]
pub(super) fn no_default_tokio_runtime_gate_rejects_removed_runtime_dependencies() {
    let root = make_quality_temp_dir("tokio_removed_runtime_dependency");
    write_file(
        &root,
        CARGO_MANIFEST_PATH,
        concat!(
            "[package]\nname = \"terlan\"\n\n",
            "[dependencies]\n",
            "tokio = \"1\"\n",
            "futures-util = \"0.3\"\n"
        ),
    );
    write_file(
        &root,
        TOKIO_INVENTORY_PATH,
        &format!(
            "{INVENTORY_HEADER_ROW}{CARGO_MANIFEST_PATH}\tmigration-debt\tcargo\tRemoval plan: tokio -> replace with VM scheduler and VM sockets\n"
        ),
    );

    let diagnostic = run_no_default_tokio_runtime(&root).expect_err("removed dependency");

    assert!(
        diagnostic.contains("removed runtime dependency `futures-util` must stay absent"),
        "expected removed-dependency diagnostic: {diagnostic}"
    );
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies removed direct Tokio features fail the full gate.
///
/// Inputs:
/// - Temporary Cargo manifest that reintroduces a removed Tokio feature.
/// - Matching inventory row for the Tokio dependency plan.
///
/// Output:
/// - Diagnostic naming the removed feature.
///
/// Transformation:
/// - Protects the direct Tokio feature ratchet independently from the direct
///   package-name ratchet.
#[test]
pub(super) fn no_default_tokio_runtime_gate_rejects_removed_tokio_features() {
    let root = make_quality_temp_dir("tokio_removed_feature");
    write_file(
        &root,
        CARGO_MANIFEST_PATH,
        concat!(
            "[package]\nname = \"terlan\"\n\n",
            "[dependencies]\n",
            "tokio = { version = \"1\", features = [\"io-std\", \"net\"] }\n"
        ),
    );
    write_file(
        &root,
        TOKIO_INVENTORY_PATH,
        &format!(
            "{INVENTORY_HEADER_ROW}{CARGO_MANIFEST_PATH}\tmigration-debt\tcargo\tRemoval plan: tokio -> replace with VM scheduler and VM sockets\n"
        ),
    );

    let diagnostic = run_no_default_tokio_runtime(&root).expect_err("removed feature");

    assert!(
        diagnostic.contains("removed Tokio feature `net` must stay absent"),
        "expected removed-feature diagnostic: {diagnostic}"
    );
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies unexpected direct Tokio features fail the full gate.
///
/// Inputs:
/// - Temporary Cargo manifest that introduces a Tokio feature not in the
///   current allowlist and not in the removed-feature ratchet.
/// - Matching inventory row for the Tokio dependency plan.
///
/// Output:
/// - Diagnostic naming the unexpected feature.
///
/// Transformation:
/// - Keeps the default Tokio feature surface exact while migration debt still
///   exists.
#[test]
pub(super) fn no_default_tokio_runtime_gate_rejects_unexpected_tokio_features() {
    let root = make_quality_temp_dir("tokio_unexpected_feature");
    write_file(
        &root,
        CARGO_MANIFEST_PATH,
        concat!(
            "[package]\nname = \"terlan\"\n\n",
            "[dependencies]\n",
            "tokio = { version = \"1\", features = [\"rt\", \"signal\"] }\n"
        ),
    );
    write_file(
        &root,
        TOKIO_INVENTORY_PATH,
        &format!(
            "{INVENTORY_HEADER_ROW}{CARGO_MANIFEST_PATH}\tmigration-debt\tcargo\tRemoval plan: tokio -> replace with VM scheduler and VM sockets\n"
        ),
    );

    let diagnostic = run_no_default_tokio_runtime(&root).expect_err("unexpected feature");

    assert!(
        diagnostic.contains("unexpected Tokio feature `signal` must not enter"),
        "expected unexpected-feature diagnostic: {diagnostic}"
    );
    fs::remove_dir_all(root).expect("remove fixture");
}
