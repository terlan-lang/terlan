
/// Verifies unresolved local constructor patterns fail before CoreIR.
///
/// Inputs:
/// - A temporary single-file Terlan module that matches `Missing(value)`
///   without declaring or importing `Missing`.
///
/// Output:
/// - Test passes when `terlc check --emit-phase-manifest` fails in the
///   typecheck phase, skips CoreIR, and records the unknown
///   constructor-pattern diagnostic in the emitted phase manifest.
///
/// Transformation:
/// - Runs command-level check through syntax-output parsing, HIR
///   resolution, and typechecking so the formal phase manifest proves
///   unresolved constructor-pattern sugar cannot reach CoreIR lowering.
#[test]
fn run_check_single_file_rejects_local_unknown_constructor_pattern_before_core_phase() {
    let dir = make_temp_dir("check_single_file_local_unknown_constructor_pattern");
    let source = dir.join("local_unknown_constructor_pattern.terl");
    fs::write(
            &source,
            "module local_unknown_constructor_pattern.\n\npub unwrap(input: Dynamic): Dynamic ->\n    case input {\n        Missing(value) -> value\n    }.\n",
        )
        .expect("write local unknown constructor-pattern source");
    let manifest = dir.join("local_unknown_constructor_pattern.phase-manifest.json");

    let cache = dir.join("cache");
    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![
                source.to_string_lossy().into(),
                "--emit-phase-manifest".into(),
                manifest.to_string_lossy().into(),
            ],
        },
        CliState {
            cache_dir: Some(cache),
            ..Default::default()
        },
    );
    assert_eq!(exit, ExitCode::from(1));

    let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
    assert!(manifest_text.contains(r#""name":"typecheck","status":"error""#));
    assert!(manifest_text.contains(r#""name":"core","status":"skipped""#));
    assert!(manifest_text.contains(r#""code":"type_error""#));
    assert!(manifest_text.contains("unknown constructor pattern Missing"));
}

/// Verifies local constructor-chain manifests carry resolved base identity.
///
/// Inputs:
/// - A temporary single-file Terlan module with a declared constructor
///   `User` and a constructor-chain expression that extends its result.
///
/// Output:
/// - Test passes when `terlc check --emit-phase-manifest` succeeds and the
///   CoreIR proof coverage reports one resolved constructor-call identity,
///   one resolved constructor-chain identity, and no unresolved chain
///   candidates.
///
/// Transformation:
/// - Runs the CLI check command through local constructor typechecking,
///   CoreIR constructor-chain lowering, and manifest emission.
#[test]
fn run_check_single_file_accepts_declared_constructor_chain_in_core_phase() {
    let dir = make_temp_dir("check_single_file_declared_constructor_chain");
    let source = dir.join("constructor_chain.terl");
    fs::write(
            &source,
            "module constructor_chain.\n\npub constructor User {\n    (id: Int, name: Binary): Dynamic -> id\n}.\n\npub value(id: Int, name: Binary): Dynamic ->\n    User(id, name) with Admin { id: id, name: name }.\n",
        )
        .expect("write declared constructor chain source");
    let manifest = dir.join("constructor_chain.phase-manifest.json");

    let cache = dir.join("cache");
    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![
                source.to_string_lossy().into(),
                "--emit-phase-manifest".into(),
                manifest.to_string_lossy().into(),
            ],
        },
        CliState {
            cache_dir: Some(cache),
            ..Default::default()
        },
    );
    assert_eq!(exit, ExitCode::SUCCESS);

    let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
    assert!(manifest_text.contains(r#""name":"typecheck","status":"ok""#));
    assert!(manifest_text.contains(r#""name":"core","status":"ok""#));
    let manifest_json: serde_json::Value =
        serde_json::from_str(&manifest_text).expect("parse phase manifest");
    assert_eq!(
        manifest_json["core_proof_coverage"]["resolved_constructor_call_identity"]
            .as_u64()
            .expect("resolved constructor-call identity count"),
        1
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["resolved_constructor_chain_identity"]
            .as_u64()
            .expect("resolved constructor-chain identity count"),
        1
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["unresolved_constructor_chain_candidate"]
            .as_u64()
            .expect("unresolved constructor-chain candidate count"),
        0
    );
}

/// Verifies local type-alias constructor-chain manifests carry resolved
/// base identity.
///
/// Inputs:
/// - A temporary single-file Terlan module with a single-shape `User`
///   type alias and a constructor-chain expression that extends it.
///
/// Output:
/// - Test passes when `terlc check --emit-phase-manifest` succeeds and the
///   CoreIR proof coverage reports one resolved constructor-call identity,
///   one resolved constructor-chain identity, and no unresolved chain
///   candidates.
///
/// Transformation:
/// - Runs the CLI check command through type-alias constructor
///   typechecking, CoreIR constructor-chain identity annotation, and
///   manifest emission.
#[test]
fn run_check_single_file_accepts_alias_constructor_chain_in_core_phase() {
    let dir = make_temp_dir("check_single_file_alias_constructor_chain");
    let source = dir.join("alias_constructor_chain.terl");
    fs::write(
            &source,
            "module alias_constructor_chain.\n\npub type User = {Atom[\"user\"], id: Int, name: Binary}.\n\npub value(id: Int, name: Binary): Dynamic ->\n    User(id, name) with Admin { id: id, name: name }.\n",
        )
        .expect("write alias constructor chain source");
    let manifest = dir.join("alias_constructor_chain.phase-manifest.json");

    let cache = dir.join("cache");
    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![
                source.to_string_lossy().into(),
                "--emit-phase-manifest".into(),
                manifest.to_string_lossy().into(),
            ],
        },
        CliState {
            cache_dir: Some(cache),
            ..Default::default()
        },
    );
    assert_eq!(exit, ExitCode::SUCCESS);

    let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
    assert!(manifest_text.contains(r#""name":"typecheck","status":"ok""#));
    assert!(manifest_text.contains(r#""name":"core","status":"ok""#));
    let manifest_json: serde_json::Value =
        serde_json::from_str(&manifest_text).expect("parse phase manifest");
    assert_eq!(
        manifest_json["core_proof_coverage"]["resolved_constructor_call_identity"]
            .as_u64()
            .expect("resolved constructor-call identity count"),
        1
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["resolved_constructor_chain_identity"]
            .as_u64()
            .expect("resolved constructor-chain identity count"),
        1
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["unresolved_constructor_chain_candidate"]
            .as_u64()
            .expect("unresolved constructor-chain candidate count"),
        0
    );
}

/// Verifies imported constructor-chain manifests carry provider-qualified identity.
///
/// Inputs:
/// - A temporary provider `.terli` interface that exports public constructor
///   `User`.
/// - A temporary consumer `.terl` module that imports `User` and uses it as
///   the base call in a constructor-chain expression.
///
/// Output:
/// - Test passes when `terlc check --emit-phase-manifest` succeeds and the
///   CoreIR proof coverage reports one resolved constructor-call identity,
///   one resolved constructor-chain identity, and no unresolved chain
///   candidates.
///
/// Transformation:
/// - Runs the command-level check path through sibling-interface loading,
///   imported constructor typechecking, CoreIR constructor-chain lowering,
///   and phase-manifest emission.
#[test]
fn run_check_single_file_accepts_imported_constructor_chain_in_core_phase() {
    let dir = make_temp_dir("check_single_file_imported_constructor_chain");
    let provider = dir.join("provider.terli");
    fs::write(
            &provider,
            "module provider.\n\npub constructor User {\n    (id: Int, name: Binary): Dynamic -> id\n}.\n",
        )
        .expect("write provider constructor interface");

    let source = dir.join("imported_constructor_chain.terl");
    fs::write(
            &source,
            "module imported_constructor_chain.\n\nimport provider.{User}.\n\npub value(id: Int, name: Binary): Dynamic ->\n    User(id, name) with Admin { id: id, name: name }.\n",
        )
        .expect("write imported constructor chain source");
    let manifest = dir.join("imported_constructor_chain.phase-manifest.json");

    let cache = dir.join("cache");
    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![
                source.to_string_lossy().into(),
                "--emit-phase-manifest".into(),
                manifest.to_string_lossy().into(),
            ],
        },
        CliState {
            cache_dir: Some(cache),
            ..Default::default()
        },
    );
    assert_eq!(exit, ExitCode::SUCCESS);

    let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
    assert!(manifest_text.contains(r#""name":"typecheck","status":"ok""#));
    assert!(manifest_text.contains(r#""name":"core","status":"ok""#));
    let manifest_json: serde_json::Value =
        serde_json::from_str(&manifest_text).expect("parse phase manifest");
    assert_eq!(
        manifest_json["core_proof_coverage"]["resolved_constructor_call_identity"]
            .as_u64()
            .expect("resolved constructor-call identity count"),
        1
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["resolved_constructor_chain_identity"]
            .as_u64()
            .expect("resolved constructor-chain identity count"),
        1
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["unresolved_constructor_chain_candidate"]
            .as_u64()
            .expect("unresolved constructor-chain candidate count"),
        0
    );
}

/// Verifies aliased imported constructor-chain manifests carry resolved identity.
///
/// Inputs:
/// - A temporary provider `.terli` interface that exports public constructor
///   `User`.
/// - A temporary consumer `.terl` module that imports `User as Member` and
///   uses `Member` as the base call in a constructor-chain expression.
///
/// Output:
/// - Test passes when `terlc check --emit-phase-manifest` succeeds and the
///   CoreIR proof coverage reports one resolved constructor-call identity,
///   one resolved constructor-chain identity, and no unresolved chain
///   candidates.
///
/// Transformation:
/// - Runs the command-level check path through sibling-interface loading,
///   alias-aware constructor typechecking, CoreIR constructor-chain
///   lowering, and phase-manifest emission so aliased imported
///   constructor-chain identity resolution is pinned outside unit-only
///   lowering.
#[test]
fn run_check_single_file_accepts_aliased_imported_constructor_chain_in_core_phase() {
    let dir = make_temp_dir("check_single_file_aliased_imported_constructor_chain");
    let provider = dir.join("provider.terli");
    fs::write(
            &provider,
            "module provider.\n\npub constructor User {\n    (id: Int, name: Binary): Dynamic -> id\n}.\n",
        )
        .expect("write provider constructor interface");

    let source = dir.join("aliased_imported_constructor_chain.terl");
    fs::write(
            &source,
            "module aliased_imported_constructor_chain.\n\nimport provider.{User as Member}.\n\npub value(id: Int, name: Binary): Dynamic ->\n    Member(id, name) with Admin { id: id, name: name }.\n",
        )
        .expect("write aliased imported constructor chain source");
    let manifest = dir.join("aliased_imported_constructor_chain.phase-manifest.json");

    let cache = dir.join("cache");
    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![
                source.to_string_lossy().into(),
                "--emit-phase-manifest".into(),
                manifest.to_string_lossy().into(),
            ],
        },
        CliState {
            cache_dir: Some(cache),
            ..Default::default()
        },
    );
    assert_eq!(exit, ExitCode::SUCCESS);

    let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
    assert!(manifest_text.contains(r#""name":"typecheck","status":"ok""#));
    assert!(manifest_text.contains(r#""name":"core","status":"ok""#));
    let manifest_json: serde_json::Value =
        serde_json::from_str(&manifest_text).expect("parse phase manifest");
    assert_eq!(
        manifest_json["core_proof_coverage"]["resolved_constructor_call_identity"]
            .as_u64()
            .expect("resolved constructor-call identity count"),
        1
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["resolved_constructor_chain_identity"]
            .as_u64()
            .expect("resolved constructor-chain identity count"),
        1
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["unresolved_constructor_chain_candidate"]
            .as_u64()
            .expect("unresolved constructor-chain candidate count"),
        0
    );
}

/// Verifies directly imported type-alias constructor-chain manifests carry
/// provider-qualified identity.
///
/// Inputs:
/// - A temporary provider `.terli` interface that exports public single-shape
///   type alias `User`.
/// - A temporary consumer `.terl` module that imports `User` directly and
///   uses `User` as the base call in a constructor-chain expression.
///
/// Output:
/// - Test passes when `terlc check --emit-phase-manifest` succeeds and the
///   CoreIR proof coverage reports one resolved constructor-call identity,
///   one resolved constructor-chain identity, and no unresolved chain
///   candidates.
///
/// Transformation:
/// - Runs command-level check through sibling-interface loading,
///   direct-import typechecking, CoreIR type-alias constructor-chain
///   identity annotation, and phase-manifest emission.
#[test]
fn run_check_single_file_accepts_direct_imported_alias_constructor_chain_in_core_phase() {
    let dir = make_temp_dir("check_single_file_direct_imported_alias_constructor_chain");
    let provider = dir.join("provider.terli");
    fs::write(
        &provider,
        "module provider.\n\npub type User = {Atom[\"user\"], id: Int, name: Binary}.\n",
    )
    .expect("write provider alias constructor-chain interface");

    let source = dir.join("direct_imported_alias_constructor_chain.terl");
    fs::write(
            &source,
            "module direct_imported_alias_constructor_chain.\n\nimport provider.{User}.\n\npub value(id: Int, name: Binary): Dynamic ->\n    User(id, name) with Admin { id: id, name: name }.\n",
        )
        .expect("write direct imported alias constructor chain source");
    let manifest = dir.join("direct_imported_alias_constructor_chain.phase-manifest.json");

    let cache = dir.join("cache");
    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![
                source.to_string_lossy().into(),
                "--emit-phase-manifest".into(),
                manifest.to_string_lossy().into(),
            ],
        },
        CliState {
            cache_dir: Some(cache),
            ..Default::default()
        },
    );
    assert_eq!(exit, ExitCode::SUCCESS);

    let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
    assert!(manifest_text.contains(r#""name":"typecheck","status":"ok""#));
    assert!(manifest_text.contains(r#""name":"core","status":"ok""#));
    let manifest_json: serde_json::Value =
        serde_json::from_str(&manifest_text).expect("parse phase manifest");
    assert_eq!(
        manifest_json["core_proof_coverage"]["resolved_constructor_call_identity"]
            .as_u64()
            .expect("resolved constructor-call identity count"),
        1
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["resolved_constructor_chain_identity"]
            .as_u64()
            .expect("resolved constructor-chain identity count"),
        1
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["unresolved_constructor_chain_candidate"]
            .as_u64()
            .expect("unresolved constructor-chain candidate count"),
        0
    );
}

/// Verifies imported type-alias constructor-chain manifests carry
/// provider-qualified identity.
///
/// Inputs:
/// - A temporary provider `.terli` interface that exports public single-shape
///   type alias `User`.
/// - A temporary consumer `.terl` module that imports `User as Member` and
///   uses `Member` as the base call in a constructor-chain expression.
///
/// Output:
/// - Test passes when `terlc check --emit-phase-manifest` succeeds and the
///   CoreIR proof coverage reports one resolved constructor-call identity,
///   one resolved constructor-chain identity, and no unresolved chain
///   candidates.
///
/// Transformation:
/// - Runs command-level check through sibling-interface loading,
///   alias-aware typechecking, CoreIR type-alias constructor-chain identity
///   annotation, and phase-manifest emission.
#[test]
fn run_check_single_file_accepts_aliased_imported_alias_constructor_chain_in_core_phase() {
    let dir = make_temp_dir("check_single_file_aliased_imported_alias_constructor_chain");
    let provider = dir.join("provider.terli");
    fs::write(
        &provider,
        "module provider.\n\npub type User = {Atom[\"user\"], id: Int, name: Binary}.\n",
    )
    .expect("write provider alias constructor-chain interface");

    let source = dir.join("aliased_imported_alias_constructor_chain.terl");
    fs::write(
            &source,
            "module aliased_imported_alias_constructor_chain.\n\nimport provider.{User as Member}.\n\npub value(id: Int, name: Binary): Dynamic ->\n    Member(id, name) with Admin { id: id, name: name }.\n",
        )
        .expect("write aliased imported alias constructor chain source");
    let manifest = dir.join("aliased_imported_alias_constructor_chain.phase-manifest.json");

    let cache = dir.join("cache");
    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![
                source.to_string_lossy().into(),
                "--emit-phase-manifest".into(),
                manifest.to_string_lossy().into(),
            ],
        },
        CliState {
            cache_dir: Some(cache),
            ..Default::default()
        },
    );
    assert_eq!(exit, ExitCode::SUCCESS);

    let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
    assert!(manifest_text.contains(r#""name":"typecheck","status":"ok""#));
    assert!(manifest_text.contains(r#""name":"core","status":"ok""#));
    let manifest_json: serde_json::Value =
        serde_json::from_str(&manifest_text).expect("parse phase manifest");
    assert_eq!(
        manifest_json["core_proof_coverage"]["resolved_constructor_call_identity"]
            .as_u64()
            .expect("resolved constructor-call identity count"),
        1
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["resolved_constructor_chain_identity"]
            .as_u64()
            .expect("resolved constructor-chain identity count"),
        1
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["unresolved_constructor_chain_candidate"]
            .as_u64()
            .expect("unresolved constructor-chain candidate count"),
        0
    );
}
