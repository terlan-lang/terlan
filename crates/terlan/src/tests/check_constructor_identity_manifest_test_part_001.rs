use super::*;

/// Verifies declared constructor-call manifests carry resolved identity.
///
/// Inputs:
/// - A temporary `.terl` source file declaring public constructor `Ok` and
///   calling `Ok(1)` from a public function.
///
/// Output:
/// - Test assertion only; `terlc check --emit-phase-manifest` must succeed and
///   report one resolved constructor-call identity with no unresolved
///   constructor-call candidates.
///
/// Transformation:
/// - Runs command-level checking through local constructor typechecking,
///   CoreIR lowering, constructor identity annotation, and phase-manifest
///   emission.
#[test]
fn run_check_single_file_accepts_declared_constructor_call_in_core_phase() {
    let dir = make_temp_dir("check_single_file_declared_constructor_call");
    let source = dir.join("constructor_call.terl");
    fs::write(
            &source,
            "module constructor_call.\n\npub constructor Ok {\n    (value: Int): Dynamic -> value\n}.\n\npub value(): Dynamic ->\n    Ok(1).\n",
        )
        .expect("write declared constructor call source");
    let manifest = dir.join("constructor_call.phase-manifest.json");

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
        manifest_json["core_proof_coverage"]["unresolved_constructor_call_candidate"]
            .as_u64()
            .expect("unresolved constructor-call candidate count"),
        0
    );
}

/// Verifies local type-alias constructor-call manifests carry resolved
/// identity.
///
/// Inputs:
/// - A temporary `.terl` source file declaring a single-shape `Ok[T]` type
///   alias and calling `Ok(1)`.
///
/// Output:
/// - Test passes when `terlc check --emit-phase-manifest` succeeds and the
///   CoreIR proof coverage reports one resolved constructor-call identity
///   with no unresolved constructor-call candidates.
///
/// Transformation:
/// - Runs the command-level check path through typechecking, CoreIR
///   type-alias constructor identity annotation, and phase-manifest
///   emission.
#[test]
fn run_check_single_file_accepts_alias_constructor_call_in_core_phase() {
    let dir = make_temp_dir("check_single_file_alias_constructor_call");
    let source = dir.join("alias_constructor_call.terl");
    fs::write(
            &source,
            "module alias_constructor_call.\n\npub type Ok[T] = {Atom[\"ok\"], value: T}.\n\npub value(): Dynamic ->\n    Ok(1).\n",
        )
        .expect("write alias constructor call source");
    let manifest = dir.join("alias_constructor_call.phase-manifest.json");

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
        manifest_json["core_proof_coverage"]["unresolved_constructor_call_candidate"]
            .as_u64()
            .expect("unresolved constructor-call candidate count"),
        0
    );
}

/// Verifies imported constructor-call manifests carry provider-qualified identity.
///
/// Inputs:
/// - A temporary provider `.terli` interface that exports public constructor
///   `Ok`.
/// - A temporary consumer `.terl` module that imports `Ok` and calls it.
///
/// Output:
/// - Test passes when `terlc check --emit-phase-manifest` succeeds and the
///   CoreIR proof coverage reports one resolved constructor-call identity
///   with no unresolved constructor-call candidates.
///
/// Transformation:
/// - Runs the command-level check path through sibling-interface loading,
///   typechecking, CoreIR lowering, and phase-manifest emission so imported
///   constructor identity resolution is pinned outside unit-only lowering.
#[test]
fn run_check_single_file_accepts_imported_constructor_call_in_core_phase() {
    let dir = make_temp_dir("check_single_file_imported_constructor_call");
    let provider = dir.join("provider.terli");
    fs::write(
        &provider,
        "module provider.\n\npub constructor Ok {\n    (value: Int): Dynamic -> value\n}.\n",
    )
    .expect("write provider constructor interface");

    let source = dir.join("imported_constructor_call.terl");
    fs::write(
            &source,
            "module imported_constructor_call.\n\nimport provider.{Ok}.\n\npub value(): Dynamic ->\n    Ok(1).\n",
        )
        .expect("write imported constructor call source");
    let manifest = dir.join("imported_constructor_call.phase-manifest.json");

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
        manifest_json["core_proof_coverage"]["unresolved_constructor_call_candidate"]
            .as_u64()
            .expect("unresolved constructor-call candidate count"),
        0
    );
}

/// Verifies aliased imported constructor-call manifests carry resolved identity.
///
/// Inputs:
/// - A temporary provider `.terli` interface that exports public constructor
///   `Ok`.
/// - A temporary consumer `.terl` module that imports `Ok as Success` and
///   calls `Success`.
///
/// Output:
/// - Test passes when `terlc check --emit-phase-manifest` succeeds and the
///   CoreIR proof coverage reports one resolved constructor-call identity
///   with no unresolved constructor-call candidates.
///
/// Transformation:
/// - Runs the command-level check path through sibling-interface loading,
///   alias-aware typechecking, CoreIR lowering, and phase-manifest emission
///   so aliased imported constructor identity resolution is pinned outside
///   unit-only lowering.
#[test]
fn run_check_single_file_accepts_aliased_imported_constructor_call_in_core_phase() {
    let dir = make_temp_dir("check_single_file_aliased_imported_constructor_call");
    let provider = dir.join("provider.terli");
    fs::write(
        &provider,
        "module provider.\n\npub constructor Ok {\n    (value: Int): Dynamic -> value\n}.\n",
    )
    .expect("write provider constructor interface");

    let source = dir.join("aliased_imported_constructor_call.terl");
    fs::write(
            &source,
            "module aliased_imported_constructor_call.\n\nimport provider.{Ok as Success}.\n\npub value(): Dynamic ->\n    Success(1).\n",
        )
        .expect("write aliased imported constructor call source");
    let manifest = dir.join("aliased_imported_constructor_call.phase-manifest.json");

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
        manifest_json["core_proof_coverage"]["unresolved_constructor_call_candidate"]
            .as_u64()
            .expect("unresolved constructor-call candidate count"),
        0
    );
}

/// Verifies directly imported type-alias constructor-call manifests carry
/// provider-qualified identity.
///
/// Inputs:
/// - A temporary provider `.terli` interface that exports public single-shape
///   type alias `Ok`.
/// - A temporary consumer `.terl` module that imports `Ok` directly and calls
///   `Ok`.
///
/// Output:
/// - Test passes when `terlc check --emit-phase-manifest` succeeds and the
///   CoreIR proof coverage reports one resolved constructor-call identity
///   with no unresolved constructor-call candidates.
///
/// Transformation:
/// - Runs command-level check through sibling-interface loading,
///   direct-import typechecking, CoreIR type-alias constructor identity
///   annotation, and phase-manifest emission.
#[test]
fn run_check_single_file_accepts_direct_imported_alias_constructor_call_in_core_phase() {
    let dir = make_temp_dir("check_single_file_direct_imported_alias_constructor_call");
    let provider = dir.join("provider.terli");
    fs::write(
        &provider,
        "module provider.\n\npub type Ok[T] = {Atom[\"ok\"], value: T}.\n",
    )
    .expect("write provider alias constructor interface");

    let source = dir.join("direct_imported_alias_constructor_call.terl");
    fs::write(
            &source,
            "module direct_imported_alias_constructor_call.\n\nimport provider.{Ok}.\n\npub value(): Dynamic ->\n    Ok(1).\n",
        )
        .expect("write direct imported alias constructor call source");
    let manifest = dir.join("direct_imported_alias_constructor_call.phase-manifest.json");

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
        manifest_json["core_proof_coverage"]["unresolved_constructor_call_candidate"]
            .as_u64()
            .expect("unresolved constructor-call candidate count"),
        0
    );
}

/// Verifies imported type-alias constructor-call manifests carry
/// provider-qualified identity.
///
/// Inputs:
/// - A temporary provider `.terli` interface that exports public single-shape
///   type alias `Ok`.
/// - A temporary consumer `.terl` module that imports `Ok as Success` and
///   calls `Success`.
///
/// Output:
/// - Test passes when `terlc check --emit-phase-manifest` succeeds and the
///   CoreIR proof coverage reports one resolved constructor-call identity
///   with no unresolved constructor-call candidates.
///
/// Transformation:
/// - Runs the command-level check path through sibling-interface loading,
///   alias-aware typechecking, CoreIR type-alias constructor identity
///   annotation, and phase-manifest emission.
#[test]
fn run_check_single_file_accepts_aliased_imported_alias_constructor_call_in_core_phase() {
    let dir = make_temp_dir("check_single_file_aliased_imported_alias_constructor_call");
    let provider = dir.join("provider.terli");
    fs::write(
        &provider,
        "module provider.\n\npub type Ok[T] = {Atom[\"ok\"], value: T}.\n",
    )
    .expect("write provider alias constructor interface");

    let source = dir.join("aliased_imported_alias_constructor_call.terl");
    fs::write(
            &source,
            "module aliased_imported_alias_constructor_call.\n\nimport provider.{Ok as Success}.\n\npub value(): Dynamic ->\n    Success(1).\n",
        )
        .expect("write aliased imported alias constructor call source");
    let manifest = dir.join("aliased_imported_alias_constructor_call.phase-manifest.json");

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
        manifest_json["core_proof_coverage"]["unresolved_constructor_call_candidate"]
            .as_u64()
            .expect("unresolved constructor-call candidate count"),
        0
    );
}

/// Verifies imported constructor-pattern manifests carry provider-qualified identity.
///
/// Inputs:
/// - A temporary provider `.terli` interface that exports public constructor
///   `Some`.
/// - A temporary consumer `.terl` module that imports `Some` and matches it
///   in a `case` expression.
///
/// Output:
/// - Test passes when `terlc check --emit-phase-manifest` succeeds and the
///   CoreIR proof coverage reports one resolved constructor-pattern
///   identity with no unresolved constructor-pattern candidates.
///
/// Transformation:
/// - Runs the command-level check path through sibling-interface loading,
///   typechecking, CoreIR pattern lowering, and phase-manifest emission so
///   imported pattern identity resolution is pinned outside unit-only
///   lowering.
#[test]
fn run_check_single_file_accepts_imported_constructor_pattern_in_core_phase() {
    let dir = make_temp_dir("check_single_file_imported_constructor_pattern");
    let provider = dir.join("provider.terli");
    fs::write(
            &provider,
            "module provider.\n\npub constructor Some {\n    (value: Dynamic): Dynamic -> {Atom[\"some\"], value}\n}.\n",
        )
        .expect("write provider constructor interface");

    let source = dir.join("imported_constructor_pattern.terl");
    fs::write(
            &source,
            "module imported_constructor_pattern.\n\nimport provider.{Some}.\n\npub unwrap(input: Dynamic): Dynamic ->\n    case input {\n        Some(value) -> value\n    }.\n",
        )
        .expect("write imported constructor pattern source");
    let manifest = dir.join("imported_constructor_pattern.phase-manifest.json");

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
        manifest_json["core_proof_coverage"]["resolved_constructor_pattern_identity"]
            .as_u64()
            .expect("resolved constructor-pattern identity count"),
        1
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["unresolved_constructor_pattern_candidate"]
            .as_u64()
            .expect("unresolved constructor-pattern candidate count"),
        0
    );
}

/// Verifies aliased imported constructor-pattern manifests carry resolved identity.
///
/// Inputs:
/// - A temporary provider `.terli` interface that exports public constructor
///   `Some`.
/// - A temporary consumer `.terl` module that imports `Some as Maybe` and
///   matches `Maybe(value)` in a `case` expression.
///
/// Output:
/// - Test passes when `terlc check --emit-phase-manifest` succeeds and the
///   CoreIR proof coverage reports one resolved constructor-pattern
///   identity with no unresolved constructor-pattern candidates.
///
/// Transformation:
/// - Runs the command-level check path through sibling-interface loading,
///   alias-aware typechecking, CoreIR pattern lowering, and
///   phase-manifest emission so aliased imported constructor-pattern
///   identity resolution is pinned outside unit-only lowering.
#[test]
fn run_check_single_file_accepts_aliased_imported_constructor_pattern_in_core_phase() {
    let dir = make_temp_dir("check_single_file_aliased_imported_constructor_pattern");
    let provider = dir.join("provider.terli");
    fs::write(
            &provider,
            "module provider.\n\npub constructor Some {\n    (value: Dynamic): Dynamic -> {Atom[\"some\"], value}\n}.\n",
        )
        .expect("write provider constructor interface");

    let source = dir.join("aliased_imported_constructor_pattern.terl");
    fs::write(
            &source,
            "module aliased_imported_constructor_pattern.\n\nimport provider.{Some as Maybe}.\n\npub unwrap(input: Dynamic): Dynamic ->\n    case input {\n        Maybe(value) -> value\n    }.\n",
        )
        .expect("write aliased imported constructor pattern source");
    let manifest = dir.join("aliased_imported_constructor_pattern.phase-manifest.json");

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
        manifest_json["core_proof_coverage"]["resolved_constructor_pattern_identity"]
            .as_u64()
            .expect("resolved constructor-pattern identity count"),
        1
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["unresolved_constructor_pattern_candidate"]
            .as_u64()
            .expect("unresolved constructor-pattern candidate count"),
        0
    );
}

/// Verifies directly imported type-alias constructor-pattern manifests
/// carry provider-qualified identity.
///
/// Inputs:
/// - A temporary provider `.terli` interface that exports public single-shape
///   type alias `Ok`.
/// - A temporary consumer `.terl` module that imports `Ok` directly and
///   matches `Ok(value)`.
///
/// Output:
/// - Test passes when `terlc check --emit-phase-manifest` succeeds and the
///   CoreIR proof coverage reports one resolved constructor-pattern
///   identity with no unresolved constructor-pattern candidates.
///
/// Transformation:
/// - Runs command-level check through sibling-interface loading,
///   direct-import typechecking, CoreIR type-alias constructor-pattern
///   identity annotation, and phase-manifest emission.
#[test]
fn run_check_single_file_accepts_direct_imported_alias_constructor_pattern_in_core_phase() {
    let dir = make_temp_dir("check_single_file_direct_imported_alias_constructor_pattern");
    let provider = dir.join("provider.terli");
    fs::write(
        &provider,
        "module provider.\n\npub type Ok[T] = {Atom[\"ok\"], value: T}.\n",
    )
    .expect("write provider alias constructor interface");

    let source = dir.join("direct_imported_alias_constructor_pattern.terl");
    fs::write(
            &source,
            "module direct_imported_alias_constructor_pattern.\n\nimport provider.{Ok}.\n\npub unwrap(input: Ok[Int]): Int ->\n    case input {\n        Ok(value) -> value;\n        _ -> 0\n    }.\n",
        )
        .expect("write direct imported alias constructor pattern source");
    let manifest = dir.join("direct_imported_alias_constructor_pattern.phase-manifest.json");

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
        manifest_json["core_proof_coverage"]["resolved_constructor_pattern_identity"]
            .as_u64()
            .expect("resolved constructor-pattern identity count"),
        1
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["unresolved_constructor_pattern_candidate"]
            .as_u64()
            .expect("unresolved constructor-pattern candidate count"),
        0
    );
}

/// Verifies imported type-alias constructor-pattern manifests carry
/// provider-qualified identity.
///
/// Inputs:
/// - A temporary provider `.terli` interface that exports public single-shape
///   type alias `Ok`.
/// - A temporary consumer `.terl` module that imports `Ok as Success` and
///   matches `Success(value)`.
///
/// Output:
/// - Test passes when `terlc check --emit-phase-manifest` succeeds and the
///   CoreIR proof coverage reports one resolved constructor-pattern
///   identity with no unresolved constructor-pattern candidates.
///
/// Transformation:
/// - Runs command-level check through sibling-interface loading,
///   alias-aware typechecking, CoreIR type-alias constructor-pattern
///   identity annotation, and phase-manifest emission.
#[test]
fn run_check_single_file_accepts_aliased_imported_alias_constructor_pattern_in_core_phase() {
    let dir = make_temp_dir("check_single_file_aliased_imported_alias_constructor_pattern");
    let provider = dir.join("provider.terli");
    fs::write(
        &provider,
        "module provider.\n\npub type Ok[T] = {Atom[\"ok\"], value: T}.\n",
    )
    .expect("write provider alias constructor interface");

    let source = dir.join("aliased_imported_alias_constructor_pattern.terl");
    fs::write(
            &source,
            "module aliased_imported_alias_constructor_pattern.\n\nimport provider.{Ok as Success}.\n\npub unwrap(input: Success[Int]): Int ->\n    case input {\n        Success(value) -> value;\n        _ -> 0\n    }.\n",
        )
        .expect("write aliased imported alias constructor pattern source");
    let manifest = dir.join("aliased_imported_alias_constructor_pattern.phase-manifest.json");

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
        manifest_json["core_proof_coverage"]["resolved_constructor_pattern_identity"]
            .as_u64()
            .expect("resolved constructor-pattern identity count"),
        1
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["unresolved_constructor_pattern_candidate"]
            .as_u64()
            .expect("unresolved constructor-pattern candidate count"),
        0
    );
}

/// Verifies constructor-pattern manifests carry identity and freshness debt.
///
/// Inputs:
/// - A temporary single-file Terlan module with a declared constructor and
///   a `case` expression that binds through that constructor pattern.
///
/// Output:
/// - Test passes when `terlc check --emit-phase-manifest` succeeds, records
///   one resolved constructor-pattern identity, and exposes runtime-binding
///   freshness obligations for both the selected case body and pattern.
///
/// Transformation:
/// - Runs the CLI check command through the formal pipeline and validates
///   the manifest fields that future Lean proof export will consume.
#[test]
fn run_check_single_file_accepts_declared_constructor_pattern_in_core_phase() {
    let dir = make_temp_dir("check_single_file_declared_constructor_pattern");
    let source = dir.join("constructor_pattern.terl");
    fs::write(
            &source,
            "module constructor_pattern.\n\npub constructor Some {\n    (value: Dynamic): Dynamic -> {Atom[\"some\"], value}\n}.\n\npub unwrap(input: Dynamic): Dynamic ->\n    case input {\n        Some(value) -> value\n    }.\n",
        )
        .expect("write declared constructor pattern source");
    let manifest = dir.join("constructor_pattern.phase-manifest.json");

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
        manifest_json["core_proof_coverage"]["resolved_constructor_pattern_identity"]
            .as_u64()
            .expect("resolved constructor-pattern identity count"),
        1
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["unresolved_constructor_pattern_candidate"]
            .as_u64()
            .expect("unresolved constructor-pattern candidate count"),
        0
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["checked_preservation_expr_no_runtime_bindings"]
            .as_u64()
            .expect("no-runtime-bindings expression count"),
        2
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["checked_preservation_expr_runtime_bindings_required"]
            .as_u64()
            .expect("runtime-bindings-required expression count"),
        1
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["checked_preservation_pattern_no_runtime_bindings"]
            .as_u64()
            .expect("no-runtime-bindings pattern count"),
        0
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]
            ["checked_preservation_pattern_runtime_bindings_required"]
            .as_u64()
            .expect("runtime-bindings-required pattern count"),
        1
    );
}

/// Verifies local type-alias constructor-pattern manifests carry resolved
/// identity.
///
/// Inputs:
/// - A temporary `.terl` source file declaring a single-shape `Ok[T]` type
///   alias and matching `Ok(value)` in a case expression.
///
/// Output:
/// - Test passes when `terlc check --emit-phase-manifest` succeeds and the
///   CoreIR proof coverage reports one resolved constructor-pattern
///   identity with no unresolved constructor-pattern candidates.
///
/// Transformation:
/// - Runs command-level check through typechecking, CoreIR type-alias
///   constructor-pattern identity annotation, and phase-manifest emission.
#[test]
fn run_check_single_file_accepts_alias_constructor_pattern_in_core_phase() {
    let dir = make_temp_dir("check_single_file_alias_constructor_pattern");
    let source = dir.join("alias_constructor_pattern.terl");
    fs::write(
            &source,
            "module alias_constructor_pattern.\n\npub type Ok[T] = {Atom[\"ok\"], value: T}.\n\npub unwrap(input: Ok[Int]): Int ->\n    case input {\n        Ok(value) -> value;\n        _ -> 0\n    }.\n",
        )
        .expect("write alias constructor pattern source");
    let manifest = dir.join("alias_constructor_pattern.phase-manifest.json");

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
        manifest_json["core_proof_coverage"]["resolved_constructor_pattern_identity"]
            .as_u64()
            .expect("resolved constructor-pattern identity count"),
        1
    );
    assert_eq!(
        manifest_json["core_proof_coverage"]["unresolved_constructor_pattern_candidate"]
            .as_u64()
            .expect("unresolved constructor-pattern candidate count"),
        0
    );
}
