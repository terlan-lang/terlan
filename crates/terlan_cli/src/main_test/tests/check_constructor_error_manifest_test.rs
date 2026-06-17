use super::*;

/// Verifies unknown constructor-call syntax fails before CoreIR lowering.
///
/// Inputs:
/// - A temporary `.terl` source file whose function body calls undeclared
///   constructor-like name `Ok(1)`.
///
/// Output:
/// - Test assertion only; `terlc check --emit-phase-manifest` must fail in the
///   typecheck phase, skip CoreIR, and record the unknown-constructor
///   diagnostic.
///
/// Transformation:
/// - Runs command-level checking through parsing, resolution, typechecking, and
///   phase-manifest emission to prove unresolved constructor sugar cannot reach
///   CoreIR identity annotation.
#[test]
fn run_check_single_file_rejects_unknown_constructor_before_core_phase() {
    let dir = make_temp_dir("check_single_file_unresolved_constructor_candidate");
    let source = dir.join("constructor_candidate.terl");
    fs::write(
        &source,
        "module constructor_candidate.\n\npub value(): Dynamic ->\n    Ok(1).\n",
    )
    .expect("write unresolved constructor candidate source");
    let manifest = dir.join("constructor_candidate.phase-manifest.json");

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
    assert!(manifest_text.contains("unknown constructor Ok / 1"));
}

/// Verifies imported public struct type identity does not permit raw
/// construction outside the defining module.
///
/// Inputs:
/// - A temporary provider `.terli` interface that exports `Point`.
/// - A temporary consumer `.terl` module that imports `Point` as a type and
///   attempts `#Point { ... }` raw construction.
///
/// Output:
/// - Test passes when `terlc check --emit-phase-manifest` fails in the
///   typecheck phase, skips CoreIR, and reports the raw-construction
///   visibility error.
///
/// Transformation:
/// - Runs command-level check through sibling-interface loading, name
///   resolution, typechecking, and phase-manifest emission to pin the
///   constructor-boundary rule on the formal compiler path.
#[test]
fn run_check_single_file_rejects_imported_raw_struct_construction_before_core_phase() {
    let dir = make_temp_dir("check_single_file_imported_raw_struct_construction");
    let provider = dir.join("provider.terli");
    fs::write(
        &provider,
        "module provider.\n\npub struct Point {\n    x: Int\n}.\n",
    )
    .expect("write provider struct interface");

    let source = dir.join("imported_raw_struct_construction.terl");
    fs::write(
            &source,
            "module imported_raw_struct_construction.\n\nimport type provider.Point.\n\npub value(): Dynamic ->\n    #Point { x = 1 }.\n",
        )
        .expect("write imported raw struct construction source");
    let manifest = dir.join("imported_raw_struct_construction.phase-manifest.json");

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
    assert!(manifest_text.contains("cannot raw-construct imported struct provider.Point"));
}

/// Verifies public constructors cannot expose private return types.
///
/// Inputs:
/// - A temporary single-file Terlan module with a private `Secret` struct
///   and public constructor returning `Secret`.
///
/// Output:
/// - Test passes when `terlc check --emit-phase-manifest` fails in the
///   typecheck phase, skips CoreIR, and reports the constructor visibility
///   diagnostic.
///
/// Transformation:
/// - Runs command-level check through parsing, resolution, typechecking,
///   and phase-manifest emission to prove constructor API visibility is
///   enforced before CoreIR/backend emission.
#[test]
fn run_check_single_file_rejects_public_constructor_private_return_before_core_phase() {
    let dir = make_temp_dir("check_single_file_public_constructor_private_return");
    let source = dir.join("public_constructor_private_return.terl");
    fs::write(
            &source,
            "module public_constructor_private_return.\n\nstruct Secret {\n    value: Int\n}.\n\npub constructor Secret {\n    (value: Int): Secret -> value\n}.\n",
        )
        .expect("write public constructor private return source");
    let manifest = dir.join("public_constructor_private_return.phase-manifest.json");

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
    assert!(manifest_text.contains("public constructor Secret exposes private return type Secret"));
}

/// Verifies eligible type-alias constructor calls with wrong arity fail
/// before CoreIR.
///
/// Inputs:
/// - A temporary single-file Terlan module that declares `Ok[T] =
///   {:ok, value: T}` and calls `Ok()` with no payload.
///
/// Output:
/// - Test passes when `terlc check --emit-phase-manifest` fails in the
///   typecheck phase, skips CoreIR, and reports the constructor arity
///   mismatch.
///
/// Transformation:
/// - Runs command-level check through typechecking and confirms eligible
///   type-alias constructors remain semantically resolved enough to report
///   arity errors rather than unresolved constructor metadata.
#[test]
fn run_check_single_file_rejects_alias_constructor_wrong_arity_before_core_phase() {
    let dir = make_temp_dir("check_single_file_alias_constructor_wrong_arity");
    let source = dir.join("alias_constructor_wrong_arity.terl");
    fs::write(
            &source,
            "module alias_constructor_wrong_arity.\n\npub type Ok[T] = {:ok, value: T}.\n\npub value(): Dynamic ->\n    Ok().\n",
        )
        .expect("write alias constructor wrong-arity source");
    let manifest = dir.join("alias_constructor_wrong_arity.phase-manifest.json");

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
    assert!(
        manifest_text.contains("constructor Ok has arity mismatch: expected 1..1 args, found 0")
    );
}

/// Verifies imported eligible type-alias constructor calls with wrong arity
/// fail before CoreIR.
///
/// Inputs:
/// - A temporary provider `.terli` interface that exports `Ok[T] =
///   {:ok, value: T}`.
/// - A temporary consumer `.terl` module that imports `Ok` and calls `Ok()`.
///
/// Output:
/// - Test passes when `terlc check --emit-phase-manifest` fails in the
///   typecheck phase, skips CoreIR, and reports the constructor arity
///   mismatch.
///
/// Transformation:
/// - Runs command-level check through sibling-interface loading and
///   typechecking, confirming imported eligible aliases report arity
///   failures rather than unresolved constructor metadata.
#[test]
fn run_check_single_file_rejects_imported_alias_constructor_wrong_arity_before_core_phase() {
    let dir = make_temp_dir("check_single_file_imported_alias_constructor_wrong_arity");
    let provider = dir.join("result.terli");
    fs::write(
        &provider,
        "module result.\n\npub type Ok[T] = {:ok, value: T}.\n",
    )
    .expect("write provider alias constructor interface");

    let source = dir.join("imported_alias_constructor_wrong_arity.terl");
    fs::write(
            &source,
            "module imported_alias_constructor_wrong_arity.\n\nimport result.{Ok}.\n\npub value(): Dynamic ->\n    Ok().\n",
        )
        .expect("write imported alias constructor wrong-arity source");
    let manifest = dir.join("imported_alias_constructor_wrong_arity.phase-manifest.json");

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
    assert!(
        manifest_text.contains("constructor Ok has arity mismatch: expected 1..1 args, found 0")
    );
}

/// Verifies aliased imported eligible type-alias constructor calls with
/// wrong arity fail before CoreIR and report the source alias name.
///
/// Inputs:
/// - A temporary provider `.terli` interface that exports `Ok[T] =
///   {:ok, value: T}`.
/// - A temporary consumer `.terl` module that imports `Ok as Success` and
///   calls `Success()`.
///
/// Output:
/// - Test passes when `terlc check --emit-phase-manifest` fails in the
///   typecheck phase, skips CoreIR, and reports the constructor arity
///   mismatch for `Success`.
///
/// Transformation:
/// - Runs command-level check through sibling-interface loading,
///   alias-aware import resolution, and typechecking, confirming aliased
///   eligible alias calls report arity failures rather than unresolved
///   constructor metadata.
#[test]
fn run_check_single_file_rejects_aliased_imported_alias_constructor_wrong_arity_before_core_phase()
{
    let dir = make_temp_dir("check_single_file_aliased_imported_alias_constructor_wrong_arity");
    let provider = dir.join("result.terli");
    fs::write(
        &provider,
        "module result.\n\npub type Ok[T] = {:ok, value: T}.\n",
    )
    .expect("write provider alias constructor interface");

    let source = dir.join("aliased_imported_alias_constructor_wrong_arity.terl");
    fs::write(
            &source,
            "module aliased_imported_alias_constructor_wrong_arity.\n\nimport result.{Ok as Success}.\n\npub value(): Dynamic ->\n    Success().\n",
        )
        .expect("write aliased imported alias constructor wrong-arity source");
    let manifest = dir.join("aliased_imported_alias_constructor_wrong_arity.phase-manifest.json");

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
    assert!(manifest_text
        .contains("constructor Success has arity mismatch: expected 1..1 args, found 0"));
}

/// Verifies imported eligible type-alias constructor patterns with wrong
/// arity fail before CoreIR.
///
/// Inputs:
/// - A temporary provider `.terli` interface that exports `Ok[T] =
///   {:ok, value: T}`.
/// - A temporary consumer `.terl` module that imports `Ok` and matches
///   `Ok(value, extra)`.
///
/// Output:
/// - Test passes when `terlc check --emit-phase-manifest` fails in the
///   typecheck phase, skips CoreIR, and reports the constructor-pattern
///   arity mismatch.
///
/// Transformation:
/// - Runs command-level check through sibling-interface loading and
///   typechecking, confirming imported eligible alias patterns report arity
///   failures rather than unresolved constructor-pattern metadata.
#[test]
fn run_check_single_file_rejects_imported_alias_constructor_pattern_wrong_arity_before_core_phase()
{
    let dir = make_temp_dir("check_single_file_imported_alias_constructor_pattern_wrong_arity");
    let provider = dir.join("result.terli");
    fs::write(
        &provider,
        "module result.\n\npub type Ok[T] = {:ok, value: T}.\n",
    )
    .expect("write provider alias constructor interface");

    let source = dir.join("imported_alias_constructor_pattern_wrong_arity.terl");
    fs::write(
            &source,
            "module imported_alias_constructor_pattern_wrong_arity.\n\nimport result.{Ok}.\n\npub unwrap(input: Ok[Int]): Int ->\n    case input {\n        Ok(value, extra) -> value\n    }.\n",
        )
        .expect("write imported alias constructor pattern wrong-arity source");
    let manifest = dir.join("imported_alias_constructor_pattern_wrong_arity.phase-manifest.json");

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
    assert!(
        manifest_text.contains("constructor Ok has arity mismatch: expected 1..1 args, found 2")
    );
}

/// Verifies aliased imported eligible type-alias constructor patterns with
/// wrong arity fail before CoreIR and report the source alias name.
///
/// Inputs:
/// - A temporary provider `.terli` interface that exports `Ok[T] =
///   {:ok, value: T}`.
/// - A temporary consumer `.terl` module that imports `Ok as Success` and
///   matches `Success(value, extra)`.
///
/// Output:
/// - Test passes when `terlc check --emit-phase-manifest` fails in the
///   typecheck phase, skips CoreIR, and reports the constructor-pattern
///   arity mismatch for `Success`.
///
/// Transformation:
/// - Runs command-level check through sibling-interface loading,
///   alias-aware import resolution, and typechecking, confirming aliased
///   eligible alias patterns report arity failures rather than unresolved
///   constructor-pattern metadata.
#[test]
fn run_check_single_file_rejects_aliased_imported_alias_constructor_pattern_wrong_arity_before_core_phase(
) {
    let dir =
        make_temp_dir("check_single_file_aliased_imported_alias_constructor_pattern_wrong_arity");
    let provider = dir.join("result.terli");
    fs::write(
        &provider,
        "module result.\n\npub type Ok[T] = {:ok, value: T}.\n",
    )
    .expect("write provider alias constructor interface");

    let source = dir.join("aliased_imported_alias_constructor_pattern_wrong_arity.terl");
    fs::write(
            &source,
            "module aliased_imported_alias_constructor_pattern_wrong_arity.\n\nimport result.{Ok as Success}.\n\npub unwrap(input: Success[Int]): Int ->\n    case input {\n        Success(value, extra) -> value\n    }.\n",
        )
        .expect("write aliased imported alias constructor pattern wrong-arity source");
    let manifest =
        dir.join("aliased_imported_alias_constructor_pattern_wrong_arity.phase-manifest.json");

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
    assert!(manifest_text
        .contains("constructor Success has arity mismatch: expected 1..1 args, found 2"));
}

/// Verifies eligible type-alias constructor patterns with wrong arity fail
/// before CoreIR.
///
/// Inputs:
/// - A temporary single-file Terlan module that declares `Ok[T] =
///   {:ok, value: T}` and matches `Ok(value, extra)`.
///
/// Output:
/// - Test passes when `terlc check --emit-phase-manifest` fails in the
///   typecheck phase, skips CoreIR, and reports the constructor-pattern
///   arity mismatch.
///
/// Transformation:
/// - Runs command-level check through typechecking and confirms eligible
///   type-alias constructor patterns report arity errors rather than
///   unresolved pattern metadata.
#[test]
fn run_check_single_file_rejects_alias_constructor_pattern_wrong_arity_before_core_phase() {
    let dir = make_temp_dir("check_single_file_alias_constructor_pattern_wrong_arity");
    let source = dir.join("alias_constructor_pattern_wrong_arity.terl");
    fs::write(
            &source,
            "module alias_constructor_pattern_wrong_arity.\n\npub type Ok[T] = {:ok, value: T}.\n\npub unwrap(input: Ok[Int]): Int ->\n    case input {\n        Ok(value, extra) -> value\n    }.\n",
        )
        .expect("write alias constructor pattern wrong-arity source");
    let manifest = dir.join("alias_constructor_pattern_wrong_arity.phase-manifest.json");

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
    assert!(
        manifest_text.contains("constructor Ok has arity mismatch: expected 1..1 args, found 2")
    );
}

/// Verifies eligible type-alias constructor-chain bases with wrong arity
/// fail before CoreIR.
///
/// Inputs:
/// - A temporary single-file Terlan module that declares `User =
///   {:user, id: Int, name: Binary}` and uses `User(id)` as a
///   constructor-chain base.
///
/// Output:
/// - Test passes when `terlc check --emit-phase-manifest` fails in the
///   typecheck phase, skips CoreIR, and reports the constructor arity
///   mismatch.
///
/// Transformation:
/// - Runs command-level check through typechecking and confirms chain-base
///   arity failures remain typecheck diagnostics rather than unresolved
///   constructor-chain metadata.
#[test]
fn run_check_single_file_rejects_alias_constructor_chain_wrong_arity_before_core_phase() {
    let dir = make_temp_dir("check_single_file_alias_constructor_chain_wrong_arity");
    let source = dir.join("alias_constructor_chain_wrong_arity.terl");
    fs::write(
            &source,
            "module alias_constructor_chain_wrong_arity.\n\npub type User = {:user, id: Int, name: Binary}.\n\npub value(id: Int): Dynamic ->\n    User(id) with Wrapped { id = id }.\n",
        )
        .expect("write alias constructor chain wrong-arity source");
    let manifest = dir.join("alias_constructor_chain_wrong_arity.phase-manifest.json");

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
    assert!(
        manifest_text.contains("constructor User has arity mismatch: expected 2..2 args, found 1")
    );
}

/// Verifies directly imported eligible type-alias constructor-chain bases
/// with wrong arity fail before CoreIR.
///
/// Inputs:
/// - A temporary provider `.terli` interface that exports `User =
///   {:user, id: Int, name: Binary}`.
/// - A temporary consumer `.terl` module that imports `User` and uses
///   `User(id)` as a constructor-chain base.
///
/// Output:
/// - Test passes when `terlc check --emit-phase-manifest` fails in the
///   typecheck phase, skips CoreIR, and reports the imported constructor
///   arity mismatch.
///
/// Transformation:
/// - Runs command-level check through sibling-interface loading and
///   typechecking, confirming imported single-shape alias chain-base arity
///   failures remain typecheck diagnostics rather than unresolved
///   constructor-chain metadata.
#[test]
fn run_check_single_file_rejects_imported_alias_constructor_chain_wrong_arity_before_core_phase() {
    let dir = make_temp_dir("check_single_file_imported_alias_constructor_chain_wrong_arity");
    let provider = dir.join("result.terli");
    fs::write(
        &provider,
        "module result.\n\npub type User = {:user, id: Int, name: Binary}.\n",
    )
    .expect("write provider alias constructor-chain interface");

    let source = dir.join("imported_alias_constructor_chain_wrong_arity.terl");
    fs::write(
            &source,
            "module imported_alias_constructor_chain_wrong_arity.\n\nimport result.{User}.\n\npub value(id: Int): Dynamic ->\n    User(id) with Wrapped { id = id }.\n",
        )
        .expect("write imported alias constructor chain wrong-arity source");
    let manifest = dir.join("imported_alias_constructor_chain_wrong_arity.phase-manifest.json");

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
    assert!(
        manifest_text.contains("constructor User has arity mismatch: expected 2..2 args, found 1")
    );
}

/// Verifies aliased imported eligible type-alias constructor-chain bases
/// with wrong arity fail before CoreIR and report the source alias name.
///
/// Inputs:
/// - A temporary provider `.terli` interface that exports `User =
///   {:user, id: Int, name: Binary}`.
/// - A temporary consumer `.terl` module that imports `User as Member` and
///   uses `Member(id)` as a constructor-chain base.
///
/// Output:
/// - Test passes when `terlc check --emit-phase-manifest` fails in the
///   typecheck phase, skips CoreIR, and reports the constructor arity
///   mismatch for `Member`.
///
/// Transformation:
/// - Runs command-level check through sibling-interface loading,
///   alias-aware import resolution, and typechecking, confirming aliased
///   eligible alias chain-base arity failures remain typecheck diagnostics
///   rather than unresolved constructor-chain metadata.
#[test]
fn run_check_single_file_rejects_aliased_imported_alias_constructor_chain_wrong_arity_before_core_phase(
) {
    let dir =
        make_temp_dir("check_single_file_aliased_imported_alias_constructor_chain_wrong_arity");
    let provider = dir.join("result.terli");
    fs::write(
        &provider,
        "module result.\n\npub type User = {:user, id: Int, name: Binary}.\n",
    )
    .expect("write provider alias constructor-chain interface");

    let source = dir.join("aliased_imported_alias_constructor_chain_wrong_arity.terl");
    fs::write(
            &source,
            "module aliased_imported_alias_constructor_chain_wrong_arity.\n\nimport result.{User as Member}.\n\npub value(id: Int): Dynamic ->\n    Member(id) with Wrapped { id = id }.\n",
        )
        .expect("write aliased imported alias constructor chain wrong-arity source");
    let manifest =
        dir.join("aliased_imported_alias_constructor_chain_wrong_arity.phase-manifest.json");

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
    assert!(manifest_text
        .contains("constructor Member has arity mismatch: expected 2..2 args, found 1"));
}

/// Verifies imported list aliases cannot become constructor-chain bases.
///
/// Inputs:
/// - A temporary provider `.terli` interface that exports public list alias
///   `Items`.
/// - A temporary consumer `.terl` module that imports `Items` and attempts to
///   use `Items(values)` as a constructor-chain base.
///
/// Output:
/// - Test passes when `terlc check --emit-phase-manifest` fails in the
///   typecheck phase, skips CoreIR, and reports `unknown constructor
///   Items / 1`.
///
/// Transformation:
/// - Runs command-level check through sibling-interface loading and
///   typechecking, proving non-eligible imported aliases are rejected
///   before CoreIR identity annotation can run.
#[test]
fn run_check_single_file_rejects_imported_list_alias_constructor_chain_before_core_phase() {
    let dir = make_temp_dir("check_single_file_imported_list_alias_constructor_chain");
    let provider = dir.join("items.terli");
    fs::write(&provider, "module items.\n\npub type Items[T] = List[T].\n")
        .expect("write provider list alias interface");

    let source = dir.join("imported_list_alias_constructor_chain.terl");
    fs::write(
            &source,
            "module imported_list_alias_constructor_chain.\n\nimport items.{Items}.\n\npub value(values: List[Int]): Dynamic ->\n    Items(values) with Wrapped { values = values }.\n",
        )
        .expect("write imported list alias constructor-chain source");
    let manifest = dir.join("imported_list_alias_constructor_chain.phase-manifest.json");

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
    assert!(manifest_text.contains("unknown constructor Items / 1"));
}

/// Verifies imported list aliases cannot become constructor calls.
///
/// Inputs:
/// - A temporary provider `.terli` interface that exports public list alias
///   `Items`.
/// - A temporary consumer `.terl` module that imports `Items` and attempts
///   to call `Items(values)`.
///
/// Output:
/// - Test passes when `terlc check --emit-phase-manifest` fails in the
///   typecheck phase, skips CoreIR, and reports `unknown constructor
///   Items / 1`.
///
/// Transformation:
/// - Runs command-level check through sibling-interface loading and
///   typechecking, proving non-eligible imported aliases are rejected
///   before CoreIR constructor-call identity annotation can run.
#[test]
fn run_check_single_file_rejects_imported_list_alias_constructor_call_before_core_phase() {
    let dir = make_temp_dir("check_single_file_imported_list_alias_constructor_call");
    let provider = dir.join("items.terli");
    fs::write(&provider, "module items.\n\npub type Items[T] = List[T].\n")
        .expect("write provider list alias interface");

    let source = dir.join("imported_list_alias_constructor_call.terl");
    fs::write(
            &source,
            "module imported_list_alias_constructor_call.\n\nimport items.{Items}.\n\npub value(values: List[Int]): Items[Int] ->\n    Items(values).\n",
        )
        .expect("write imported list alias constructor-call source");
    let manifest = dir.join("imported_list_alias_constructor_call.phase-manifest.json");

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
    assert!(manifest_text.contains("unknown constructor Items / 1"));
}

/// Verifies aliased imported list aliases cannot become
/// constructor-chain bases.
///
/// Inputs:
/// - A temporary provider `.terli` interface that exports public list alias
///   `Items`.
/// - A temporary consumer `.terl` module that imports `Items as Bag` and
///   attempts to use `Bag(values)` as a constructor-chain base.
///
/// Output:
/// - Test passes when `terlc check --emit-phase-manifest` fails in the
///   typecheck phase, skips CoreIR, and reports `unknown constructor
///   Bag / 1`.
///
/// Transformation:
/// - Runs command-level check through sibling-interface loading,
///   alias-aware import resolution, and typechecking, proving non-eligible
///   imported aliases are rejected before CoreIR identity annotation can
///   run under aliased names.
#[test]
fn run_check_single_file_rejects_aliased_imported_list_alias_constructor_chain_before_core_phase() {
    let dir = make_temp_dir("check_single_file_aliased_imported_list_alias_constructor_chain");
    let provider = dir.join("items.terli");
    fs::write(&provider, "module items.\n\npub type Items[T] = List[T].\n")
        .expect("write provider list alias interface");

    let source = dir.join("aliased_imported_list_alias_constructor_chain.terl");
    fs::write(
            &source,
            "module aliased_imported_list_alias_constructor_chain.\n\nimport items.{Items as Bag}.\n\npub value(values: List[Int]): Dynamic ->\n    Bag(values) with Wrapped { values = values }.\n",
        )
        .expect("write aliased imported list alias constructor-chain source");
    let manifest = dir.join("aliased_imported_list_alias_constructor_chain.phase-manifest.json");

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
    assert!(manifest_text.contains("unknown constructor Bag / 1"));
}

/// Verifies aliased imported list aliases cannot become constructor calls.
///
/// Inputs:
/// - A temporary provider `.terli` interface that exports public list alias
///   `Items`.
/// - A temporary consumer `.terl` module that imports `Items as Bag` and
///   attempts to call `Bag(values)`.
///
/// Output:
/// - Test passes when `terlc check --emit-phase-manifest` fails in the
///   typecheck phase, skips CoreIR, and reports `unknown constructor
///   Bag / 1`.
///
/// Transformation:
/// - Runs command-level check through sibling-interface loading,
///   alias-aware import resolution, and typechecking, proving non-eligible
///   imported aliases are rejected before CoreIR constructor-call identity
///   annotation can run under aliased names.
#[test]
fn run_check_single_file_rejects_aliased_imported_list_alias_constructor_call_before_core_phase() {
    let dir = make_temp_dir("check_single_file_aliased_imported_list_alias_constructor_call");
    let provider = dir.join("items.terli");
    fs::write(&provider, "module items.\n\npub type Items[T] = List[T].\n")
        .expect("write provider list alias interface");

    let source = dir.join("aliased_imported_list_alias_constructor_call.terl");
    fs::write(
            &source,
            "module aliased_imported_list_alias_constructor_call.\n\nimport items.{Items as Bag}.\n\npub value(values: List[Int]): Bag[Int] ->\n    Bag(values).\n",
        )
        .expect("write aliased imported list alias constructor-call source");
    let manifest = dir.join("aliased_imported_list_alias_constructor_call.phase-manifest.json");

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
    assert!(manifest_text.contains("unknown constructor Bag / 1"));
}

/// Verifies imported list aliases cannot become constructor patterns.
///
/// Inputs:
/// - A temporary provider `.terli` interface that exports public list alias
///   `Items`.
/// - A temporary consumer `.terl` module that imports `Items` and attempts
///   to match `Items(values)`.
///
/// Output:
/// - Test passes when `terlc check --emit-phase-manifest` fails in the
///   typecheck phase, skips CoreIR, and reports `unknown constructor
///   pattern Items`.
///
/// Transformation:
/// - Runs command-level check through sibling-interface loading and
///   typechecking, proving non-eligible imported aliases are rejected
///   before CoreIR constructor-pattern identity annotation can run.
#[test]
fn run_check_single_file_rejects_imported_list_alias_constructor_pattern_before_core_phase() {
    let dir = make_temp_dir("check_single_file_imported_list_alias_constructor_pattern");
    let provider = dir.join("items.terli");
    fs::write(&provider, "module items.\n\npub type Items[T] = List[T].\n")
        .expect("write provider list alias interface");

    let source = dir.join("imported_list_alias_constructor_pattern.terl");
    fs::write(
            &source,
            "module imported_list_alias_constructor_pattern.\n\nimport items.{Items}.\n\npub unwrap(input: Items[Int]): List[Int] ->\n    case input {\n        Items(values) -> values\n    }.\n",
        )
        .expect("write imported list alias constructor-pattern source");
    let manifest = dir.join("imported_list_alias_constructor_pattern.phase-manifest.json");

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
    assert!(manifest_text.contains("unknown constructor pattern Items"));
}

/// Verifies aliased imported list aliases cannot become constructor
/// patterns.
///
/// Inputs:
/// - A temporary provider `.terli` interface that exports public list alias
///   `Items`.
/// - A temporary consumer `.terl` module that imports `Items as Bag` and
///   attempts to match `Bag(values)`.
///
/// Output:
/// - Test passes when `terlc check --emit-phase-manifest` fails in the
///   typecheck phase, skips CoreIR, and reports `unknown constructor
///   pattern Bag`.
///
/// Transformation:
/// - Runs command-level check through sibling-interface loading,
///   alias-aware import resolution, and typechecking, proving non-eligible
///   imported aliases are rejected before CoreIR constructor-pattern
///   identity annotation can run under aliased names.
#[test]
fn run_check_single_file_rejects_aliased_imported_list_alias_constructor_pattern_before_core_phase()
{
    let dir = make_temp_dir("check_single_file_aliased_imported_list_alias_constructor_pattern");
    let provider = dir.join("items.terli");
    fs::write(&provider, "module items.\n\npub type Items[T] = List[T].\n")
        .expect("write provider list alias interface");

    let source = dir.join("aliased_imported_list_alias_constructor_pattern.terl");
    fs::write(
            &source,
            "module aliased_imported_list_alias_constructor_pattern.\n\nimport items.{Items as Bag}.\n\npub unwrap(input: Bag[Int]): List[Int] ->\n    case input {\n        Bag(values) -> values\n    }.\n",
        )
        .expect("write aliased imported list alias constructor-pattern source");
    let manifest = dir.join("aliased_imported_list_alias_constructor_pattern.phase-manifest.json");

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
    assert!(manifest_text.contains("unknown constructor pattern Bag"));
}
