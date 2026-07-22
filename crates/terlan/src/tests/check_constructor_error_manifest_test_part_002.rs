
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
            "module aliased_imported_list_alias_constructor_chain.\n\nimport items.{Items as Bag}.\n\npub value(values: List[Int]): Dynamic ->\n    Bag(values) with Wrapped { values: values }.\n",
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
