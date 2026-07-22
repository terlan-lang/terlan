use super::*;

/// Verifies single-file `check` infers the JS target profile from std.js
/// imports when the CLI target remains at the default VM profile.
///
/// Inputs:
/// - Temporary source file importing the generated `std.js.Promise` type.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path without a target override and asserts
///   target inference selects a compatible JS profile before validation.
#[test]
fn run_check_single_file_infers_js_shared_profile_from_js_import() {
    let dir = make_temp_dir("check_single_file_infers_js_shared_from_js_import");
    let path = fixture(
        &dir,
        "\
module check_infers_js_shared.\n\nimport type std.js.Promise.\n\npub accepts(value: Promise[Int]): Promise[Int] ->\n    value.\n",
    );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState::default(),
    );

    assert_eq!(exit, ExitCode::SUCCESS);
}

/// Verifies single-file `check` reports target-evidence conflicts for explicit
/// non-default profile overrides.
///
/// Inputs:
/// - Temporary source file importing the generated `std.js.Promise` type.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
///   asserts the explicit profile cannot hide JS typed evidence.
#[test]
fn run_check_single_file_rejects_explicit_core_v0_profile_for_js_import() {
    let dir = make_temp_dir("check_single_file_rejects_core_v0_override_for_js_import");
    let path = fixture(
        &dir,
        "\
module check_rejects_core_v0_js_override.\n\nimport type std.js.Promise.\n\npub accepts(value: Promise[Int]): Promise[Int] ->\n    value.\n",
    );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::CoreV0,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies directory `check` infers the JS target profile from parsed module
/// imports before validating module CoreIR.
///
/// Inputs:
/// - Temporary source root containing a module that imports `std.js.Promise`.
///
/// Output:
/// - Test assertion only; temporary files are deleted by the OS temp directory
///   lifecycle.
///
/// Transformation:
/// - Runs directory-mode `check` without a target override and asserts the
///   directory path shares the single-file target inference policy.
#[test]
fn run_check_dir_infers_js_shared_profile_from_js_import() {
    let dir = make_temp_dir("check_dir_infers_js_shared_from_js_import");
    fs::write(
        dir.join("check_dir_infers_js_shared.terl"),
        "\
module check_dir_infers_js_shared.\n\nimport type std.js.Promise.\n\npub accepts(value: Promise[Int]): Promise[Int] ->\n    value.\n",
    )
    .expect("write js directory source");

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![dir.to_string_lossy().into_owned()],
        },
        CliState::default(),
    );

    assert_eq!(exit, ExitCode::SUCCESS);
}

/// Verifies directory `check` rejects explicit target overrides that conflict
/// with parsed module target evidence.
///
/// Inputs:
/// - Temporary source root containing a module that imports `std.js.Promise`.
///
/// Output:
/// - Test assertion only; temporary files are deleted by the OS temp directory
///   lifecycle.
///
/// Transformation:
/// - Runs directory-mode `check` with `TargetProfile::CoreV0` and asserts the
///   explicit profile cannot hide JS typed evidence.
#[test]
fn run_check_dir_rejects_explicit_core_v0_profile_for_js_import() {
    let dir = make_temp_dir("check_dir_rejects_core_v0_override_for_js_import");
    fs::write(
        dir.join("check_dir_rejects_core_v0_js_override.terl"),
        "\
module check_dir_rejects_core_v0_js_override.\n\nimport type std.js.Promise.\n\npub accepts(value: Promise[Int]): Promise[Int] ->\n    value.\n",
    )
    .expect("write js directory source");

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![dir.to_string_lossy().into_owned()],
        },
        CliState {
            target_profile: TargetProfile::CoreV0,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies directory `check` validates lowered CoreIR against the selected
/// target profile.
///
/// Inputs:
/// - Temporary source root containing a module whose body lowers to map CoreIR.
///
/// Output:
/// - Test assertion only; temporary files are deleted by the OS temp directory
///   lifecycle.
///
/// Transformation:
/// - Runs directory-mode `check` with `TargetProfile::CoreV0` and asserts broad
///   CoreIR is rejected instead of bypassing profile validation.
#[test]
fn run_check_dir_rejects_map_for_core_v0_target_profile() {
    let dir = make_temp_dir("check_dir_core_v0_rejects_map");
    fs::write(
        dir.join("check_dir_core_v0_rejects_map.terl"),
        "\
module check_dir_core_v0_rejects_map.\n\npub value(): Map ->\n    {a: 1}.\n",
    )
    .expect("write map directory source");

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![dir.to_string_lossy().into_owned()],
        },
        CliState {
            target_profile: TargetProfile::CoreV0,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command accepts resolved type-alias constructor
/// calls under the portable CoreIR v0 target profile.
///
/// Inputs:
/// - Temporary source file whose function body lowers to a Lean-covered
///   constructor call with identity from an eligible single-shape type
///   alias.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
///   asserts resolved alias constructor calls remain inside the portable
///   CoreIR v0 subset.
#[test]
fn run_check_single_file_accepts_alias_constructor_call_for_core_v0_target_profile() {
    let dir = make_temp_dir("check_single_file_core_v0_accepts_alias_constructor_call");
    let path = fixture(
            &dir,
            "\
module core_v0_accepts_alias_constructor_call.\n\npub type Ok[T] = {Atom[\"ok\"], value: T}.\n\npub value(): Dynamic ->\n    Ok(1).\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::CoreV0,
            ..Default::default()
        },
    );

    assert_eq!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command enforces the selected portable CoreIR v0
/// target profile.
///
/// Inputs:
/// - Temporary source file whose function body lowers to map CoreIR.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
///   asserts broad CoreIR is rejected before a successful result is
///   returned.
#[test]
fn run_check_single_file_rejects_map_for_core_v0_target_profile() {
    let dir = make_temp_dir("check_single_file_core_v0_rejects_map");
    let path = fixture(
        &dir,
        "\
module core_v0_rejects_map.\n\npub value(): Map ->\n    {a: 1}.\n",
    );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::CoreV0,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command rejects map patterns for CoreIR v0.
///
/// Inputs:
/// - Temporary source file whose function body lowers to a typed case
///   expression containing a map pattern.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
///   asserts proof-model-required map-pattern CoreIR is rejected before a
///   successful result is returned.
#[test]
fn run_check_single_file_rejects_map_pattern_for_core_v0_target_profile() {
    let dir = make_temp_dir("check_single_file_core_v0_rejects_map_pattern");
    let path = fixture(
            &dir,
            "\
module core_v0_rejects_map_pattern.\n\npub value(input: Dynamic): Dynamic ->\n    case input {\n        {a: x} -> x;\n        _ -> input\n    }.\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::CoreV0,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command rejects list-cons patterns for CoreIR v0.
///
/// Inputs:
/// - Temporary source file whose function body lowers to a typed case
///   expression containing a list-cons pattern.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
///   asserts proof-model-required list-cons-pattern CoreIR is rejected
///   before a successful result is returned.
#[test]
fn run_check_single_file_rejects_list_cons_pattern_for_core_v0_target_profile() {
    let dir = make_temp_dir("check_single_file_core_v0_rejects_list_cons_pattern");
    let path = fixture(
            &dir,
            "\
module core_v0_rejects_list_cons_pattern.\n\npub value(input: List[Int]): Dynamic ->\n    case input {\n        [head | tail] -> head;\n        _ -> input\n    }.\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::CoreV0,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command rejects record patterns for CoreIR v0.
///
/// Inputs:
/// - Temporary source file whose function body lowers to a typed case
///   expression containing a record pattern.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
///   asserts proof-model-required record-pattern CoreIR is rejected before a
///   successful result is returned.
#[test]
fn run_check_single_file_rejects_record_pattern_for_core_v0_target_profile() {
    let dir = make_temp_dir("check_single_file_core_v0_rejects_record_pattern");
    let path = fixture(
            &dir,
            "\
module core_v0_rejects_record_pattern.\n\npub struct Point {\n    x: Int\n}.\n\npub value(input: Dynamic): Dynamic ->\n    case input {\n        Point { x: x } -> x;\n        _ -> input\n    }.\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::CoreV0,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command rejects float patterns for CoreIR v0.
///
/// Inputs:
/// - Temporary source file whose function body lowers to a typed case
///   expression containing a float pattern.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
///   asserts proof-model-required float-pattern CoreIR is rejected before a
///   successful result is returned.
#[test]
fn run_check_single_file_rejects_float_pattern_for_core_v0_target_profile() {
    let dir = make_temp_dir("check_single_file_core_v0_rejects_float_pattern");
    let path = fixture(
            &dir,
            "\
module core_v0_rejects_float_pattern.\n\npub value(input: Dynamic): Dynamic ->\n    case input {\n        1.0 -> Atom[\"float\"];\n        _ -> Atom[\"other\"]\n    }.\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::CoreV0,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command rejects floats for CoreIR v0.
///
/// Inputs:
/// - Temporary source file whose function body lowers to a typed float
///   literal CoreIR expression.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
///   asserts proof-model-required float CoreIR is rejected before a
///   successful result is returned.
#[test]
fn run_check_single_file_rejects_float_for_core_v0_target_profile() {
    let dir = make_temp_dir("check_single_file_core_v0_rejects_float");
    let path = fixture(
        &dir,
        "\
module core_v0_rejects_float.\n\npub value(): Dynamic ->\n    1.0.\n",
    );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::CoreV0,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command rejects fixed arrays for CoreIR v0.
///
/// Inputs:
/// - Temporary source file whose function body lowers to a typed
///   fixed-array CoreIR expression.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
///   asserts proof-model-required fixed-array CoreIR is rejected before a
///   successful result is returned.
#[test]
fn run_check_single_file_rejects_fixed_array_for_core_v0_target_profile() {
    let dir = make_temp_dir("check_single_file_core_v0_rejects_fixed_array");
    let path = fixture(
        &dir,
        "\
module core_v0_rejects_fixed_array.\n\npub value(): Dynamic ->\n    #[1, 2, 3].\n",
    );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::CoreV0,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command rejects index access for CoreIR v0.
///
/// Inputs:
/// - Temporary source file whose function body lowers to a typed index
///   CoreIR expression.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
///   asserts proof-model-required index CoreIR is rejected before a
///   successful result is returned.
#[test]
fn run_check_single_file_rejects_index_for_core_v0_target_profile() {
    let dir = make_temp_dir("check_single_file_core_v0_rejects_index");
    let path = fixture(
        &dir,
        "\
module core_v0_rejects_index.\n\npub value(values: List[Int]): Dynamic ->\n    values[0].\n",
    );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::CoreV0,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command rejects list comprehensions for CoreIR v0.
///
/// Inputs:
/// - Temporary source file whose function body lowers to a typed
///   list-comprehension CoreIR expression.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
///   asserts proof-model-required list-comprehension CoreIR is rejected
///   before a successful result is returned.
#[test]
fn run_check_single_file_rejects_list_comprehension_for_core_v0_target_profile() {
    let dir = make_temp_dir("check_single_file_core_v0_rejects_list_comprehension");
    let path = fixture(
            &dir,
            "\
module core_v0_rejects_list_comprehension.\n\npub value(values: List[Int]): Dynamic ->\n    [value | value <- values].\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::CoreV0,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command rejects receive expressions for CoreIR v0.
///
/// Inputs:
/// - Temporary source file whose function body lowers to a typed receive {
///   CoreIR expression with a timeout branch.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
///   asserts proof-model-required receive CoreIR is rejected before a
///   successful result is returned.
#[test]
fn run_check_single_file_rejects_receive_for_core_v0_target_profile() {
    let dir = make_temp_dir("check_single_file_core_v0_rejects_receive");
    let path = fixture(
            &dir,
            "\
module core_v0_rejects_receive.\n\npub value(): Dynamic ->\n    receive {\n        value -> value;\n    after 0 -> Atom[\"timeout\"]\n    }.\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::CoreV0,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command rejects try expressions for CoreIR v0.
///
/// Inputs:
/// - Temporary source file whose function body lowers to a typed try CoreIR
///   expression with `of`, `catch`, and `after` branches.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
///   asserts proof-model-required try CoreIR is rejected before a
///   successful result is returned.
#[test]
fn run_check_single_file_rejects_try_for_core_v0_target_profile() {
    let dir = make_temp_dir("check_single_file_core_v0_rejects_try");
    let path = fixture(
            &dir,
            "\
module core_v0_rejects_try.\n\npub value(): Dynamic ->\n    try 1 {\n        value -> value\n    catch\n        reason -> reason\n    after\n        0 -> Atom[\"done\"]\n    }.\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::CoreV0,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command rejects quote expressions for CoreIR v0.
///
/// Inputs:
/// - Temporary source file whose function body parses as a `quote`
///   keyword expression and typechecks as an AST value.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
///   asserts runtime-boundary quote syntax is rejected before a successful
///   backend-ready result is returned.
#[test]
fn run_check_single_file_rejects_quote_for_core_v0_target_profile() {
    let dir = make_temp_dir("check_single_file_core_v0_rejects_quote");
    let path = fixture(
        &dir,
        "\
module core_v0_rejects_quote.\n\npub value(x: Int): Ast[Int] ->\n    quote x.\n",
    );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::CoreV0,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command rejects unquote expressions for CoreIR v0.
///
/// Inputs:
/// - Temporary source file whose function body parses as an `unquote`
///   keyword expression and typechecks to the inner expression type.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
///   asserts runtime-boundary unquote syntax is rejected before a
///   successful backend-ready result is returned.
#[test]
fn run_check_single_file_rejects_unquote_for_core_v0_target_profile() {
    let dir = make_temp_dir("check_single_file_core_v0_rejects_unquote");
    let path = fixture(
        &dir,
        "\
module core_v0_rejects_unquote.\n\npub value(x: Int): Int ->\n    unquote(x).\n",
    );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::CoreV0,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command rejects guarded case clauses for CoreIR v0.
///
/// Inputs:
/// - Temporary source file whose function body lowers to a case expression
///   with a clause guard.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
///   asserts guarded branch semantics stay out of the Lean-covered CoreV0
///   subset until their proof model is explicit.
#[test]
fn run_check_single_file_rejects_guarded_case_for_core_v0_target_profile() {
    let dir = make_temp_dir("check_single_file_core_v0_rejects_guarded_case");
    let path = fixture(
            &dir,
            "\
module core_v0_rejects_guarded_case.\n\npub value(x: Int): Int ->\n    case x {\n        value where value > 0 -> value;\n        _ -> 0\n    }.\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::CoreV0,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command rejects partial case branch bodies for
/// CoreIR v0.
///
/// Inputs:
/// - Temporary source file whose case expression is syntactically valid but
///   has quote expressions as branch bodies.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
///   asserts summary-only branch bodies prevent the enclosing keyword
///   expression from being accepted as backend-ready CoreV0.
#[test]
fn run_check_single_file_rejects_partial_case_branch_for_core_v0_target_profile() {
    let dir = make_temp_dir("check_single_file_core_v0_rejects_partial_case_branch");
    let path = fixture(
            &dir,
            "\
module core_v0_rejects_partial_case_branch.\n\npub value(x: Int): Ast[Int] ->\n    case x {\n        0 -> quote x;\n        _ -> quote x\n    }.\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::CoreV0,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command rejects constructor chains for CoreIR v0.
///
/// Inputs:
/// - Temporary source file whose function body lowers to a constructor-chain
///   CoreIR expression with resolved base constructor identity.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
///   asserts partial constructor-chain CoreIR is rejected before a
///   successful result is returned.
#[test]
fn run_check_single_file_rejects_constructor_chain_for_core_v0_target_profile() {
    let dir = make_temp_dir("check_single_file_core_v0_rejects_constructor_chain");
    let path = fixture(
            &dir,
            "\
module core_v0_rejects_constructor_chain.\n\npub constructor User {\n    (id: Int, name: Binary): Dynamic -> id\n}.\n\npub value(id: Int, name: Binary): Dynamic ->\n    User(id, name) with Admin { id: id, name: name }.\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::CoreV0,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command rejects type-alias constructor chains for
/// CoreIR v0.
///
/// Inputs:
/// - Temporary source file whose function body lowers to a constructor-chain
///   CoreIR expression with resolved base identity from an eligible
///   single-shape type alias.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
///   asserts alias identity evidence does not promote constructor-chain
///   semantics into the portable subset.
#[test]
fn run_check_single_file_rejects_alias_constructor_chain_for_core_v0_target_profile() {
    let dir = make_temp_dir("check_single_file_core_v0_rejects_alias_constructor_chain");
    let path = fixture(
            &dir,
            "\
module core_v0_rejects_alias_constructor_chain.\n\npub type User = {Atom[\"user\"], id: Int, name: Binary}.\n\npub value(id: Int, name: Binary): Dynamic ->\n    User(id, name) with Admin { id: id, name: name }.\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::CoreV0,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}
