use super::*;

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
module core_v0_accepts_alias_constructor_call.\n\npub type Ok[T] = {:ok, value: T}.\n\npub value(): Dynamic ->\n    Ok(1).\n",
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
module core_v0_rejects_map.\n\npub value(): Map ->\n    #{a := 1}.\n",
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
module core_v0_rejects_map_pattern.\n\npub value(input: Dynamic): Dynamic ->\n    case input {\n        #{a = x} -> x;\n        _ -> input\n    }.\n",
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
module core_v0_rejects_record_pattern.\n\npub struct Point {\n    x: Int\n}.\n\npub value(input: Dynamic): Dynamic ->\n    case input {\n        #Point { x = x } -> x;\n        _ -> input\n    }.\n",
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
module core_v0_rejects_float_pattern.\n\npub value(input: Dynamic): Dynamic ->\n    case input {\n        1.0 -> :float;\n        _ -> :other\n    }.\n",
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
module core_v0_rejects_receive.\n\npub value(): Dynamic ->\n    receive {\n        value -> value;\n    after 0 -> :timeout\n    }.\n",
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
module core_v0_rejects_try.\n\npub value(): Dynamic ->\n    try 1 {\n        value -> value\n    catch\n        reason -> reason\n    after\n        0 -> :done\n    }.\n",
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
module core_v0_rejects_guarded_case.\n\npub value(x: Int): Int ->\n    case x {\n        value when value > 0 -> value;\n        _ -> 0\n    }.\n",
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
module core_v0_rejects_constructor_chain.\n\npub constructor User {\n    (id: Int, name: Binary): Dynamic -> id\n}.\n\npub value(id: Int, name: Binary): Dynamic ->\n    User(id, name) with Admin { id = id, name = name }.\n",
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
module core_v0_rejects_alias_constructor_chain.\n\npub type User = {:user, id: Int, name: Binary}.\n\npub value(id: Int, name: Binary): Dynamic ->\n    User(id, name) with Admin { id = id, name = name }.\n",
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

/// Verifies the `check` command rejects remote calls for CoreIR v0.
///
/// Inputs:
/// - Temporary source file whose function body lowers to a typed remote-call
///   CoreIR expression.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
///   asserts proof-model-required remote-call CoreIR is rejected before a
///   successful result is returned.
#[test]
fn run_check_single_file_rejects_remote_call_for_core_v0_target_profile() {
    let dir = make_temp_dir("check_single_file_core_v0_rejects_remote_call");
    let path = fixture(
        &dir,
        "\
module core_v0_rejects_remote_call.\n\npub value(): Int ->\n    erlang.abs(1).\n",
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

/// Verifies the `check` command rejects remote function references for CoreIR v0.
///
/// Inputs:
/// - Temporary source file whose function body lowers to a typed remote
///   function reference CoreIR expression.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
///   asserts proof-model-required remote function reference CoreIR is
///   rejected before a successful result is returned.
#[test]
fn run_check_single_file_rejects_remote_fun_ref_for_core_v0_target_profile() {
    let dir = make_temp_dir("check_single_file_core_v0_rejects_remote_fun_ref");
    let path = fixture(
        &dir,
        "\
module core_v0_rejects_remote_fun_ref.\n\npub value(): Dynamic ->\n    fun erlang:abs/1.\n",
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

/// Verifies the `check` command rejects record construction for CoreIR v0.
///
/// Inputs:
/// - Temporary source file whose function body lowers to a typed record
///   construction CoreIR expression.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
///   asserts proof-model-required record construction CoreIR is rejected
///   before a successful result is returned.
#[test]
fn run_check_single_file_rejects_record_construct_for_core_v0_target_profile() {
    let dir = make_temp_dir("check_single_file_core_v0_rejects_record_construct");
    let path = fixture(
        &dir,
        "\
module core_v0_rejects_record_construct.\n\npub value(): Dynamic ->\n    #Point { x = 1 }.\n",
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

/// Verifies the `check` command rejects record access for CoreIR v0.
///
/// Inputs:
/// - Temporary source file whose function body lowers to a typed record
///   access CoreIR expression.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
///   asserts proof-model-required record access CoreIR is rejected before a
///   successful result is returned.
#[test]
fn run_check_single_file_rejects_record_access_for_core_v0_target_profile() {
    let dir = make_temp_dir("check_single_file_core_v0_rejects_record_access");
    let path = fixture(
            &dir,
            "\
module core_v0_rejects_record_access.\n\npub struct Point {\n    x: Int\n}.\n\npub value(point: Point): Dynamic ->\n    point#Point.x.\n",
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

/// Verifies the `check` command rejects record updates for CoreIR v0.
///
/// Inputs:
/// - Temporary source file whose function body lowers to a typed record
///   update CoreIR expression.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
///   asserts proof-model-required record update CoreIR is rejected before a
///   successful result is returned.
#[test]
fn run_check_single_file_rejects_record_update_for_core_v0_target_profile() {
    let dir = make_temp_dir("check_single_file_core_v0_rejects_record_update");
    let path = fixture(
            &dir,
            "\
module core_v0_rejects_record_update.\n\npub struct Point {\n    x: Int\n}.\n\npub value(point: Point): Dynamic ->\n    point#Point { x = 1 }.\n",
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

/// Verifies the `check` command rejects template instantiation for CoreIR v0.
///
/// Inputs:
/// - Temporary source file whose function body lowers to a typed
///   template-instantiation CoreIR expression.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
///   asserts proof-model-required template-instantiation CoreIR is rejected
///   before a successful result is returned.
#[test]
fn run_check_single_file_rejects_template_instantiate_for_core_v0_target_profile() {
    let dir = make_temp_dir("check_single_file_core_v0_rejects_template_instantiate");
    let template_dir = dir.join("templates");
    fs::create_dir_all(&template_dir).expect("create template dir");
    fs::write(template_dir.join("user_card.terl.html"), "<p>{name}</p>")
        .expect("write template file");
    let path = fixture(
            &dir,
            "\
module core_v0_rejects_template_instantiate.\n\ntemplate UserCard from \"./templates/user_card.terl.html\" {\n    name: Text\n}.\n\npub value(): Dynamic ->\n    UserCard{ name = \"Ada\" }.\n",
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

/// Verifies BEAM-only std contracts are rejected by non-BEAM target
/// profiles before backend emission.
///
/// Inputs:
/// - A temporary Terlan module importing `std.beam.Process.Process` as a
///   type-only contract and using it in a function signature.
///
/// Output:
/// - Test assertion only; `terlc check --emit-phase-manifest` must fail
///   under `core-v0` after parse, resolve, and typecheck complete, with a
///   stable CoreIR target-profile diagnostic.
///
/// Transformation:
/// - Runs the public command path with `TargetProfile::CoreV0` and confirms
///   BEAM package contracts stay ordinary std imports whose availability is
///   controlled by target-profile validation rather than source grammar.
#[test]
fn run_check_single_file_rejects_beam_process_contract_for_core_v0_target_profile() {
    let dir = make_temp_dir("check_single_file_core_v0_beam_process_contract_rejected");
    let source = dir.join("beam_process_contract.terl");
    fs::write(
        &source,
        "\
module beam_process_contract.\n\
\n\
import type std.beam.Process.Process.\n\
import std.core.Unit.{Unit}.\n\
\n\
pub observe(process: Process[String]): Unit ->\n\
    Unit.\n",
    )
    .expect("write BEAM Process contract source");
    let manifest = dir.join("beam_process_contract.phase-manifest.json");

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
            target_profile: TargetProfile::CoreV0,
            ..CliState::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
    let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
    assert!(manifest_text.contains(r#""name":"parse","status":"ok""#));
    assert!(manifest_text.contains(r#""name":"resolve","status":"ok""#));
    assert!(manifest_text.contains(r#""name":"typecheck","status":"ok""#));
    assert!(manifest_text.contains(r#""name":"core","status":"error""#));
    assert!(manifest_text.contains("BEAM std module std.beam.Process"));
}

/// Verifies BEAM NativeBridge contracts are rejected by non-BEAM target
/// profiles before backend emission.
///
/// Inputs:
/// - A temporary Terlan module importing `std.beam.NativeBridge.NativeBridge`
///   as a type-only contract and using it in a function signature.
///
/// Output:
/// - Test assertion only; `terlc check --emit-phase-manifest` must fail
///   under `core-v0` after parse, resolve, and typecheck complete, with a
///   stable CoreIR target-profile diagnostic.
///
/// Transformation:
/// - Runs the public command path with `TargetProfile::CoreV0` and confirms
///   the BEAM/SafeNative bridge contract is target-profile gated before
///   runtime attachment or backend emission can occur.
#[test]
fn run_check_single_file_rejects_beam_native_bridge_contract_for_core_v0_target_profile() {
    let dir = make_temp_dir("check_single_file_core_v0_beam_native_bridge_contract_rejected");
    let source = dir.join("beam_native_bridge_contract.terl");
    fs::write(
        &source,
        "\
module beam_native_bridge_contract.\n\
\n\
import type std.beam.NativeBridge.NativeBridge.\n\
import std.core.Unit.{Unit}.\n\
\n\
pub observe(bridge: NativeBridge[String]): Unit ->\n\
    Unit.\n",
    )
    .expect("write BEAM NativeBridge contract source");
    let manifest = dir.join("beam_native_bridge_contract.phase-manifest.json");

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
            target_profile: TargetProfile::CoreV0,
            ..CliState::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
    let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
    assert!(manifest_text.contains(r#""name":"parse","status":"ok""#));
    assert!(manifest_text.contains(r#""name":"resolve","status":"ok""#));
    assert!(manifest_text.contains(r#""name":"typecheck","status":"ok""#));
    assert!(manifest_text.contains(r#""name":"core","status":"error""#));
    assert!(manifest_text.contains("BEAM std module std.beam.NativeBridge"));
}
