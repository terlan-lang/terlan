use super::*;

/// Verifies the `check` command accepts Lean-covered programs under the
/// portable CoreIR v0 target profile.
///
/// Inputs:
/// - Temporary source file whose function body lowers to a Lean-covered
///   arithmetic CoreIR expression.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
///   asserts the accepted portable subset still exits successfully.
#[test]
fn run_check_single_file_accepts_subtraction_for_core_v0_target_profile() {
    let dir = make_temp_dir("check_single_file_core_v0_accepts_subtraction");
    let path = fixture(
            &dir,
            "\
module core_v0_accepts_subtraction.\n\npub value(left: Int, right: Int): Int ->\n    left - right.\n",
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

/// Verifies the `check` command accepts the frozen A0 fixture shape under
/// the A0 Erlang target profile.
///
/// Inputs:
/// - Temporary source file matching the frozen A0 arithmetic fixture.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::A0Erlang` and
///   asserts the documented A0 baseline exits successfully.
#[test]
fn run_check_single_file_accepts_mathx_for_a0_erlang_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_erlang_accepts_mathx");
    let path = fixture(
        &dir,
        "\
module a0_erlang_accepts_mathx.\n\npub add(x: Int): Int ->\n    x + 1.\n",
    );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A0Erlang,
            ..Default::default()
        },
    );

    assert_eq!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command rejects a source feature outside the frozen
/// A0 artifact matrix.
///
/// Inputs:
/// - Temporary source file with a binary/string literal body.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::A0Erlang` and
///   asserts excluded syntax fails before a successful result is returned.
#[test]
fn run_check_single_file_rejects_binary_for_a0_erlang_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_erlang_rejects_binary");
    let path = fixture(
        &dir,
        "\
module a0_erlang_rejects_binary.\n\npub value(): Binary ->\n    \"hello\".\n",
    );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A0Erlang,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command accepts the named A0.1 successor arithmetic
/// and comparison subset.
///
/// Inputs:
/// - Temporary source file with `Int` parameters, arithmetic operators, and
///   a comparison return.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::A01Erlang`
///   and asserts the documented successor matrix exits successfully.
#[test]
fn run_check_single_file_accepts_arithmetic_for_a0_1_erlang_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_1_erlang_accepts_arithmetic");
    let path = fixture(
            &dir,
            "\
module a0_1_erlang_accepts_arithmetic.\n\npub bigger(x: Int, y: Int): Bool ->\n    x * 2 - 1 > y.\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A01Erlang,
            ..Default::default()
        },
    );

    assert_eq!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command accepts the named A0.2 successor boolean
/// expression subset.
///
/// Inputs:
/// - Temporary source file with `Bool` return annotation, boolean literal,
///   boolean operators, and comparison expressions.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::A02Erlang`
///   and asserts the documented successor matrix exits successfully.
#[test]
fn run_check_single_file_accepts_bool_ops_for_a0_2_erlang_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_2_erlang_accepts_bool_ops");
    let path = fixture(
            &dir,
            "\
module a0_2_erlang_accepts_bool_ops.\n\npub both(x: Int, y: Int): Bool ->\n    true and x > 0 or y > 0.\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A02Erlang,
            ..Default::default()
        },
    );

    assert_eq!(exit, ExitCode::SUCCESS);
}

/// Verifies the A0.1 profile does not silently widen when A0.2 boolean
/// expressions are introduced.
///
/// Inputs:
/// - Temporary source file using `and`, which belongs to the named A0.2
///   successor matrix rather than the A0.1 matrix.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::A01Erlang`
///   and asserts the earlier successor profile still rejects the new
///   feature.
#[test]
fn run_check_single_file_keeps_bool_ops_out_of_a0_1_erlang_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_1_erlang_rejects_bool_ops");
    let path = fixture(
        &dir,
        "\
module a0_1_erlang_rejects_bool_ops.\n\npub both(x: Int, y: Int): Bool ->\n    x > 0 and y > 0.\n",
    );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A01Erlang,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command accepts the named A0.3 successor
/// conditional expression subset.
///
/// Inputs:
/// - Temporary source file with an `if` expression whose conditions and
///   branch bodies stay inside the A0.2 expression subset.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::A03Erlang`
///   and asserts the documented successor matrix exits successfully.
#[test]
fn run_check_single_file_accepts_if_expr_for_a0_3_erlang_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_3_erlang_accepts_if_expr");
    let path = fixture(
            &dir,
            "\
module a0_3_erlang_accepts_if_expr.\n\npub choose(x: Int): Int ->\n    if { x > 0 -> x; true -> 0 }.\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A03Erlang,
            ..Default::default()
        },
    );

    assert_eq!(exit, ExitCode::SUCCESS);
}

/// Verifies the A0.2 profile does not silently widen when A0.3 conditional
/// expressions are introduced.
///
/// Inputs:
/// - Temporary source file using `if`, which belongs to the named A0.3
///   successor matrix rather than the A0.2 matrix.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::A02Erlang`
///   and asserts the earlier successor profile still rejects the new
///   feature.
#[test]
fn run_check_single_file_keeps_if_expr_out_of_a0_2_erlang_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_2_erlang_rejects_if_expr");
    let path = fixture(
            &dir,
            "\
module a0_2_erlang_rejects_if_expr.\n\npub choose(x: Int): Int ->\n    if { x > 0 -> x; true -> 0 }.\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A02Erlang,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command accepts the named A0.4 successor case
/// expression subset.
///
/// Inputs:
/// - Temporary source file with a `case` expression whose scrutinee,
///   patterns, and branch bodies stay inside the A0.4 subset.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::A04Erlang`
///   and asserts the documented successor matrix exits successfully.
#[test]
fn run_check_single_file_accepts_case_expr_for_a0_4_erlang_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_4_erlang_accepts_case_expr");
    let path = fixture(
            &dir,
            "\
module a0_4_erlang_accepts_case_expr.\n\npub choose(x: Int): Int ->\n    case x { 0 -> 0; n -> n }.\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A04Erlang,
            ..Default::default()
        },
    );

    assert_eq!(exit, ExitCode::SUCCESS);
}

/// Verifies the A0.3 profile does not silently widen when A0.4 case
/// expressions are introduced.
///
/// Inputs:
/// - Temporary source file using `case`, which belongs to the named A0.4
///   successor matrix rather than the A0.3 matrix.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::A03Erlang`
///   and asserts the earlier successor profile still rejects the new
///   feature.
#[test]
fn run_check_single_file_keeps_case_expr_out_of_a0_3_erlang_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_3_erlang_rejects_case_expr");
    let path = fixture(
            &dir,
            "\
module a0_3_erlang_rejects_case_expr.\n\npub choose(x: Int): Int ->\n    case x { 0 -> 0; n -> n }.\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A03Erlang,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command accepts the named A0.5 successor raw atom
/// literal subset.
///
/// Inputs:
/// - Temporary source file with a raw atom expression body and raw atom
///   literal case pattern.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::A05Erlang`
///   and asserts the documented successor matrix exits successfully.
#[test]
fn run_check_single_file_accepts_raw_atoms_for_a0_5_erlang_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_5_erlang_accepts_raw_atoms");
    let path = fixture(
            &dir,
            "\
module a0_5_erlang_accepts_raw_atoms.\n\npub none(): Dynamic ->\n    :none.\n\npub is_none(x: Dynamic): Bool ->\n    case x { :none -> true; _ -> false }.\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A05Erlang,
            ..Default::default()
        },
    );

    assert_eq!(exit, ExitCode::SUCCESS);
}

/// Verifies the A0.4 profile does not silently widen when A0.5 raw atom
/// literals are introduced.
///
/// Inputs:
/// - Temporary source file using raw atom literals, which belong to the
///   named A0.5 successor matrix rather than the A0.4 matrix.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::A04Erlang`
///   and asserts the earlier successor profile still rejects the new
///   feature.
#[test]
fn run_check_single_file_keeps_raw_atoms_out_of_a0_4_erlang_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_4_erlang_rejects_raw_atoms");
    let path = fixture(
            &dir,
            "\
module a0_4_erlang_rejects_raw_atoms.\n\npub none(): Dynamic ->\n    :none.\n\npub is_none(x: Dynamic): Bool ->\n    case x { :none -> true; _ -> false }.\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A04Erlang,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command accepts the named A0.6 successor tuple
/// expression and pattern subset.
///
/// Inputs:
/// - Temporary source file with tuple construction and tuple case pattern
///   matching.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::A06Erlang`
///   and asserts the documented successor matrix exits successfully.
#[test]
fn run_check_single_file_accepts_tuples_for_a0_6_erlang_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_6_erlang_accepts_tuples");
    let path = fixture(
            &dir,
            "\
module a0_6_erlang_accepts_tuples.\n\npub pair(x: Int): Dynamic ->\n    {x, :none}.\n\npub first(value: Dynamic): Int ->\n    case value { {n, :none} -> n; _ -> 0 }.\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A06Erlang,
            ..Default::default()
        },
    );

    assert_eq!(exit, ExitCode::SUCCESS);
}

/// Verifies the A0.5 profile does not silently widen when A0.6 tuple forms
/// are introduced.
///
/// Inputs:
/// - Temporary source file using tuple construction and tuple patterns,
///   which belong to the named A0.6 successor matrix rather than A0.5.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::A05Erlang`
///   and asserts the earlier successor profile still rejects the new
///   feature.
#[test]
fn run_check_single_file_keeps_tuples_out_of_a0_5_erlang_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_5_erlang_rejects_tuples");
    let path = fixture(
            &dir,
            "\
module a0_5_erlang_rejects_tuples.\n\npub pair(x: Int): Dynamic ->\n    {x, :none}.\n\npub first(value: Dynamic): Int ->\n    case value { {n, :none} -> n; _ -> 0 }.\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A05Erlang,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command accepts the named A0.7 successor list
/// expression and fixed-list pattern subset.
///
/// Inputs:
/// - Temporary source file with list construction and fixed-list case
///   pattern matching.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::A07Erlang`
///   and asserts the documented successor matrix exits successfully.
#[test]
fn run_check_single_file_accepts_lists_for_a0_7_erlang_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_7_erlang_accepts_lists");
    let path = fixture(
            &dir,
            "\
module a0_7_erlang_accepts_lists.\n\npub values(): Dynamic ->\n    [1, 2, 3].\n\npub first(value: Dynamic): Int ->\n    case value { [n, _] -> n; _ -> 0 }.\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A07Erlang,
            ..Default::default()
        },
    );

    assert_eq!(exit, ExitCode::SUCCESS);
}

/// Verifies the A0.6 profile does not silently widen when A0.7 list forms
/// are introduced.
///
/// Inputs:
/// - Temporary source file using list construction and fixed-list patterns,
///   which belong to the named A0.7 successor matrix rather than A0.6.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::A06Erlang`
///   and asserts the earlier successor profile still rejects the new
///   feature.
#[test]
fn run_check_single_file_keeps_lists_out_of_a0_6_erlang_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_6_erlang_rejects_lists");
    let path = fixture(
            &dir,
            "\
module a0_6_erlang_rejects_lists.\n\npub values(): Dynamic ->\n    [1, 2, 3].\n\npub first(value: Dynamic): Int ->\n    case value { [n, _] -> n; _ -> 0 }.\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A06Erlang,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command accepts binary/string literal expressions
/// under the named A0.8 Erlang successor target profile.
///
/// Inputs:
/// - Temporary source file with a `Binary` return annotation and string
///   literal expression body.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::A08Erlang`
///   and asserts the documented successor matrix exits successfully.
#[test]
fn run_check_single_file_accepts_binary_for_a0_8_erlang_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_8_erlang_accepts_binary");
    let path = fixture(
        &dir,
        "\
module a0_8_erlang_accepts_binary.\n\npub value(): Binary ->\n    \"hello\".\n",
    );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A08Erlang,
            ..Default::default()
        },
    );

    assert_eq!(exit, ExitCode::SUCCESS);
}

/// Verifies the A0.7 profile does not silently widen when A0.8 binary
/// literal expressions are introduced.
///
/// Inputs:
/// - Temporary source file using a string literal expression, which belongs
///   to the named A0.8 successor matrix rather than A0.7.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::A07Erlang`
///   and asserts the earlier successor profile still rejects the new
///   feature.
#[test]
fn run_check_single_file_keeps_binary_out_of_a0_7_erlang_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_7_erlang_rejects_binary");
    let path = fixture(
        &dir,
        "\
module a0_7_erlang_rejects_binary.\n\npub value(): Binary ->\n    \"hello\".\n",
    );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A07Erlang,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command accepts expression-side list cons under the
/// named A0.9 Erlang successor target profile.
///
/// Inputs:
/// - Temporary source file with a list cons expression body.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::A09Erlang`
///   and asserts the documented successor matrix exits successfully.
#[test]
fn run_check_single_file_accepts_list_cons_for_a0_9_erlang_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_9_erlang_accepts_list_cons");
    let path = fixture(
            &dir,
            "\
module a0_9_erlang_accepts_list_cons.\n\npub prepend(head: Int, tail: List[Int]): List[Int] ->\n    [head | tail].\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A09Erlang,
            ..Default::default()
        },
    );

    assert_eq!(exit, ExitCode::SUCCESS);
}

/// Verifies the A0.8 profile does not silently widen when A0.9 list cons
/// expressions are introduced.
///
/// Inputs:
/// - Temporary source file using expression-side list cons, which belongs to
///   the named A0.9 successor matrix rather than A0.8.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::A08Erlang`
///   and asserts the earlier successor profile still rejects the new
///   feature.
#[test]
fn run_check_single_file_keeps_list_cons_out_of_a0_8_erlang_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_8_erlang_rejects_list_cons");
    let path = fixture(
            &dir,
            "\
module a0_8_erlang_rejects_list_cons.\n\npub prepend(head: Int, tail: List[Int]): List[Int] ->\n    [head | tail].\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A08Erlang,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command accepts lowercase local named calls under
/// the named A0.10 Erlang successor target profile.
///
/// Inputs:
/// - Temporary source file with a private local function and public caller.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::A010Erlang`
///   and asserts the documented successor matrix exits successfully.
#[test]
fn run_check_single_file_accepts_named_call_for_a0_10_erlang_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_10_erlang_accepts_named_call");
    let path = fixture(
            &dir,
            "\
module a0_10_erlang_accepts_named_call.\n\nidentity(x: Int): Int ->\n    x.\n\npub call_it(): Int ->\n    identity(1).\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A010Erlang,
            ..Default::default()
        },
    );

    assert_eq!(exit, ExitCode::SUCCESS);
}

/// Verifies the A0.9 profile does not silently widen when A0.10 local
/// named-call expressions are introduced.
///
/// Inputs:
/// - Temporary source file using a lowercase local named call, which belongs
///   to the named A0.10 successor matrix rather than A0.9.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::A09Erlang`
///   and asserts the earlier successor profile still rejects the new
///   feature.
#[test]
fn run_check_single_file_keeps_named_call_out_of_a0_9_erlang_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_9_erlang_rejects_named_call");
    let path = fixture(
            &dir,
            "\
module a0_9_erlang_rejects_named_call.\n\nidentity(x: Int): Int ->\n    x.\n\npub call_it(): Int ->\n    identity(1).\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A09Erlang,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command accepts unary negation under the named
/// A0.11 Erlang successor target profile.
///
/// Inputs:
/// - Temporary source file with a unary negation expression body.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::A011Erlang`
///   and asserts the documented successor matrix exits successfully.
#[test]
fn run_check_single_file_accepts_unary_neg_for_a0_11_erlang_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_11_erlang_accepts_unary_neg");
    let path = fixture(
        &dir,
        "\
module a0_11_erlang_accepts_unary_neg.\n\npub negate(value: Int): Int ->\n    -value.\n",
    );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A011Erlang,
            ..Default::default()
        },
    );

    assert_eq!(exit, ExitCode::SUCCESS);
}

/// Verifies the A0.10 profile does not silently widen when A0.11 unary
/// negation expressions are introduced.
///
/// Inputs:
/// - Temporary source file using unary negation, which belongs to the named
///   A0.11 successor matrix rather than A0.10.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::A010Erlang`
///   and asserts the earlier successor profile still rejects the new
///   feature.
#[test]
fn run_check_single_file_keeps_unary_neg_out_of_a0_10_erlang_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_10_erlang_rejects_unary_neg");
    let path = fixture(
        &dir,
        "\
module a0_10_erlang_rejects_unary_neg.\n\npub negate(value: Int): Int ->\n    -value.\n",
    );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A010Erlang,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command accepts resolved constructor calls under
/// the named A0.12 Erlang successor target profile.
///
/// Inputs:
/// - Temporary source file with an explicit constructor declaration and a
///   matching constructor call expression.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::A012Erlang`
///   and asserts the documented successor matrix exits successfully.
#[test]
fn run_check_single_file_accepts_constructor_call_for_a0_12_erlang_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_12_erlang_accepts_constructor_call");
    let path = fixture(
            &dir,
            "\
module a0_12_erlang_accepts_constructor_call.\n\npub constructor Ok {\n    (Value: Int): Dynamic ->\n        Value\n}.\n\npub make(): Dynamic ->\n    Ok(1).\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A012Erlang,
            ..Default::default()
        },
    );

    assert_eq!(exit, ExitCode::SUCCESS);
}

/// Verifies the A0.11 profile does not silently widen when A0.12
/// constructor-call expressions are introduced.
///
/// Inputs:
/// - Temporary source file using a resolved constructor call, which belongs
///   to the named A0.12 successor matrix rather than A0.11.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::A011Erlang`
///   and asserts the earlier successor profile still rejects the new
///   feature.
#[test]
fn run_check_single_file_keeps_constructor_call_out_of_a0_11_erlang_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_11_erlang_rejects_constructor_call");
    let path = fixture(
            &dir,
            "\
module a0_11_erlang_rejects_constructor_call.\n\npub constructor Ok {\n    (Value: Int): Dynamic ->\n        Value\n}.\n\npub make(): Dynamic ->\n    Ok(1).\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A011Erlang,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command accepts resolved constructor patterns under
/// the named A0.13 Erlang successor target profile.
///
/// Inputs:
/// - Temporary source file with an explicit constructor declaration and a
///   matching constructor pattern in a case expression.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::A013Erlang`
///   and asserts the documented successor matrix exits successfully.
#[test]
fn run_check_single_file_accepts_constructor_pattern_for_a0_13_erlang_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_13_erlang_accepts_constructor_pattern");
    let path = fixture(
            &dir,
            "\
module a0_13_erlang_accepts_constructor_pattern.\n\npub constructor Some {\n    (value: Dynamic): Dynamic ->\n        {:some, value}\n}.\n\npub unwrap(input: Dynamic): Dynamic ->\n    case input {\n        Some(value) -> value\n    }.\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A013Erlang,
            ..Default::default()
        },
    );

    assert_eq!(exit, ExitCode::SUCCESS);
}

/// Verifies the A0.12 profile does not silently widen when A0.13
/// constructor-pattern forms are introduced.
///
/// Inputs:
/// - Temporary source file using a resolved constructor pattern, which
///   belongs to the named A0.13 successor matrix rather than A0.12.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::A012Erlang`
///   and asserts the earlier successor profile still rejects the new
///   feature.
#[test]
fn run_check_single_file_keeps_constructor_pattern_out_of_a0_12_erlang_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_12_erlang_rejects_constructor_pattern");
    let path = fixture(
            &dir,
            "\
module a0_12_erlang_rejects_constructor_pattern.\n\npub constructor Some {\n    (value: Dynamic): Dynamic ->\n        {:some, value}\n}.\n\npub unwrap(input: Dynamic): Dynamic ->\n    case input {\n        Some(value) -> value\n    }.\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A012Erlang,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command accepts anonymous function values under the
/// named A0.14 Erlang successor target profile.
///
/// Inputs:
/// - Temporary source file with a `(x) -> x` expression body.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::A014Erlang`
///   and asserts the documented successor matrix exits successfully.
#[test]
fn run_check_single_file_accepts_lambda_for_a0_14_erlang_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_14_erlang_accepts_lambda");
    let path = fixture(
        &dir,
        "\
module a0_14_erlang_accepts_lambda.\n\npub id_fun(): Term ->\n    (x) -> x.\n",
    );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A014Erlang,
            ..Default::default()
        },
    );

    assert_eq!(exit, ExitCode::SUCCESS);
}

/// Verifies the A0.13 profile does not silently widen when A0.14 lambda
/// expressions are introduced.
///
/// Inputs:
/// - Temporary source file using an anonymous function value.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::A013Erlang`
///   and asserts the earlier successor profile still rejects the new
///   feature.
#[test]
fn run_check_single_file_keeps_lambda_out_of_a0_13_erlang_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_13_erlang_rejects_lambda");
    let path = fixture(
        &dir,
        "\
module a0_13_erlang_rejects_lambda.\n\npub id_fun(): Term ->\n    (x) -> x.\n",
    );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A013Erlang,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command accepts constructor extension under the
/// named A0.15 Erlang successor target profile.
///
/// Inputs:
/// - Temporary source file with `User(id, name) with Admin { ... }`.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::A015Erlang`
///   and asserts the documented successor matrix exits successfully.
#[test]
fn run_check_single_file_accepts_constructor_extension_for_a0_15_erlang_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_15_erlang_accepts_constructor_extension");
    let path = fixture(
            &dir,
            "\
module a0_15_erlang_accepts_constructor_extension.\n\npub constructor User {\n    (id: Int, name: Binary): Dynamic -> id\n}.\n\npub build(id: Int, name: Binary): Dynamic ->\n    User(id, name) with Admin { id = id, name = name }.\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A015Erlang,
            ..Default::default()
        },
    );

    assert_eq!(exit, ExitCode::SUCCESS);
}

/// Verifies the A0.14 profile does not silently widen when A0.15
/// constructor extension expressions are introduced.
///
/// Inputs:
/// - Temporary source file using `User(id, name) with Admin { ... }`.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::A014Erlang`
///   and asserts the earlier successor profile still rejects the new
///   feature.
#[test]
fn run_check_single_file_keeps_constructor_extension_out_of_a0_14_erlang_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_14_erlang_rejects_constructor_extension");
    let path = fixture(
            &dir,
            "\
module a0_14_erlang_rejects_constructor_extension.\n\npub constructor User {\n    (id: Int, name: Binary): Dynamic -> id\n}.\n\npub build(id: Int, name: Binary): Dynamic ->\n    User(id, name) with Admin { id = id, name = name }.\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A014Erlang,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command accepts function-value invocation under the
/// named A0.16 Erlang successor target profile.
///
/// Inputs:
/// - Temporary source file using dedicated `f.(value)` syntax.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::A016Erlang`
///   and asserts the documented successor matrix exits successfully.
#[test]
fn run_check_single_file_accepts_fun_call_for_a0_16_erlang_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_16_erlang_accepts_fun_call");
    let path = fixture(
            &dir,
            "\
module a0_16_erlang_accepts_fun_call.\n\npub apply(value: Int, f: (Int) -> Int): Int ->\n    f.(value).\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A016Erlang,
            ..Default::default()
        },
    );

    assert_eq!(exit, ExitCode::SUCCESS);
}

/// Verifies the A0.15 profile does not silently widen when A0.16
/// function-value invocation syntax is introduced.
///
/// Inputs:
/// - Temporary source file using dedicated `f.(value)` syntax.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::A015Erlang`
///   and asserts the earlier successor profile rejects the new expression
///   kind.
#[test]
fn run_check_single_file_keeps_fun_call_out_of_a0_15_erlang_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_15_erlang_rejects_fun_call");
    let path = fixture(
            &dir,
            "\
module a0_15_erlang_rejects_fun_call.\n\npub apply(value: Int, f: (Int) -> Int): Int ->\n    f.(value).\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A015Erlang,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command accepts struct field access under the named
/// A0.17 Erlang successor target profile.
///
/// Inputs:
/// - Temporary source file with a public struct and `point.x`.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::A017Erlang`
///   and asserts the documented successor matrix exits successfully.
#[test]
fn run_check_single_file_accepts_field_access_for_a0_17_erlang_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_17_erlang_accepts_field_access");
    let path = fixture(
            &dir,
            "\
module a0_17_erlang_accepts_field_access.\n\npub struct Point {\n    x: Int\n}.\n\npub read(point: Point): Int ->\n    point.x.\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A017Erlang,
            ..Default::default()
        },
    );

    assert_eq!(exit, ExitCode::SUCCESS);
}

/// Verifies the A0.16 profile does not silently widen when A0.17 struct
/// field access is introduced.
///
/// Inputs:
/// - Temporary source file with a public struct and `point.x`.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::A016Erlang`
///   and asserts the earlier successor profile rejects the new expression
///   shape.
#[test]
fn run_check_single_file_keeps_field_access_out_of_a0_16_erlang_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_16_erlang_rejects_field_access");
    let path = fixture(
            &dir,
            "\
module a0_16_erlang_rejects_field_access.\n\npub struct Point {\n    x: Int\n}.\n\npub read(point: Point): Int ->\n    point.x.\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A016Erlang,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command accepts local let bindings under the named
/// A0.18 Erlang successor target profile.
///
/// Inputs:
/// - Temporary source file with a `let y = ...; z = ...; body` expression.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::A018Erlang`
///   and asserts the documented successor matrix exits successfully.
#[test]
fn run_check_single_file_accepts_let_expr_for_a0_18_erlang_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_18_erlang_accepts_let_expr");
    let path = fixture(
            &dir,
            "\
module a0_18_erlang_accepts_let_expr.\n\npub calc(x: Int): Int ->\n    let y = x + 1; z = y * 2; z + y.\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A018Erlang,
            ..Default::default()
        },
    );

    assert_eq!(exit, ExitCode::SUCCESS);
}

/// Verifies the A0.17 profile does not silently widen when A0.18 local let
/// bindings are introduced.
///
/// Inputs:
/// - Temporary source file with a `let y = ...; z = ...; body` expression.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::A017Erlang`
///   and asserts the earlier successor profile rejects the new expression
///   shape.
#[test]
fn run_check_single_file_keeps_let_expr_out_of_a0_17_erlang_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_17_erlang_rejects_let_expr");
    let path = fixture(
            &dir,
            "\
module a0_17_erlang_rejects_let_expr.\n\npub calc(x: Int): Int ->\n    let y = x + 1; z = y * 2; z + y.\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A017Erlang,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command accepts index access under the named A0.19
/// Erlang successor target profile.
///
/// Inputs:
/// - Temporary source file with a `values[0]` expression.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::A019Erlang`
///   and asserts the documented successor matrix exits successfully.
#[test]
fn run_check_single_file_accepts_index_access_for_a0_19_erlang_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_19_erlang_accepts_index_access");
    let path = fixture(
            &dir,
            "\
module a0_19_erlang_accepts_index_access.\n\npub first(values: Dynamic): Dynamic ->\n    values[0].\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A019Erlang,
            ..Default::default()
        },
    );

    assert_eq!(exit, ExitCode::SUCCESS);
}

/// Verifies the A0.18 profile does not silently widen when A0.19 index
/// access is introduced.
///
/// Inputs:
/// - Temporary source file with a `values[0]` expression.
///
/// Output:
/// Verifies the native vector selection-sort fixture checks successfully.
///
/// Inputs:
/// - The checked-in `tests/fixtures/native/vector_selection_sort.terl`
///   algorithm probe.
///
/// Output:
/// - Failed check command exit code under the Erlang profile.
///
/// Transformation:
/// - Runs the public check command with the Erlang profile and verifies native
///   std modules remain target-gated until a native backend profile owns their
///   execution contract.
#[test]
fn run_check_single_file_rejects_native_vector_selection_sort_for_erlang_profile() {
    let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/native/vector_selection_sort.terl");
    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![fixture_path.to_string_lossy().into_owned()],
        },
        CliState {
            target_profile: TargetProfile::Erlang,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command accepts qualified and scoped calls under
/// the named A0.20 Erlang successor target profile.
///
/// Inputs:
/// - Temporary source file with lowercase module-path and uppercase
///   scoped-call expressions.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::A020Erlang`
///   and asserts the documented successor matrix exits successfully.
#[test]
fn run_check_single_file_accepts_qualified_calls_for_a0_20_erlang_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_20_erlang_accepts_qualified_calls");
    let path = fixture(
            &dir,
            "\
module a0_20_erlang_accepts_qualified_calls.\n\npub qualified(): Dynamic ->\n    std.core.Math.add(1, 2).\n\npub scoped(): Dynamic ->\n    User.default().\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A020Erlang,
            ..Default::default()
        },
    );

    assert_eq!(exit, ExitCode::SUCCESS);
}

/// Verifies the A0.19 profile does not silently widen when A0.20
/// qualified and scoped calls are introduced.
///
/// Inputs:
/// - Temporary source file with lowercase module-path and uppercase
///   scoped-call expressions.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::A019Erlang`
///   and asserts the earlier successor profile rejects the new expression
///   shape.
#[test]
fn run_check_single_file_keeps_qualified_calls_out_of_a0_19_erlang_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_19_erlang_rejects_qualified_calls");
    let path = fixture(
            &dir,
            "\
module a0_19_erlang_rejects_qualified_calls.\n\npub qualified(): Dynamic ->\n    std.core.Math.add(1, 2).\n\npub scoped(): Dynamic ->\n    User.default().\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A019Erlang,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies receiver-method calls remain outside CoreIR v0 until method
/// resolution is implemented.
///
/// Inputs:
/// - Temporary source file containing `receiver.method(args)` syntax in a
///   function body.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
///   asserts method-call syntax is parsed but rejected before a successful
///   backend-ready result can be returned.
#[test]
fn run_check_single_file_rejects_method_call_for_core_v0_target_profile() {
    let dir = make_temp_dir("check_single_file_core_v0_rejects_method_call");
    let path = fixture(
            &dir,
            "\
module core_v0_rejects_method_call.\n\npub display(user: Dynamic): Dynamic ->\n    user.display_name(\"short\").\n",
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

/// Verifies the `check` command rejects backend-specific remote function
/// references under the named A0.21 Erlang diagnostic target profile.
///
/// Inputs:
/// - Temporary source file with backend-specific `fun module:function/arity`
///   expression.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::A021Erlang`
///   and asserts backend-specific reference syntax is rejected by target
///   validation instead of being allowed into backend emission.
#[test]
fn run_check_single_file_rejects_remote_fun_ref_for_a0_21_erlang_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_21_erlang_rejects_remote_fun_ref");
    let path = fixture(
            &dir,
            "\
module a0_21_erlang_rejects_remote_fun_ref.\n\npub reference(): Dynamic ->\n    fun erlang:abs/1.\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A021Erlang,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies the frozen A0 profile does not silently widen when A0.1 is
/// introduced.
///
/// Inputs:
/// - Temporary source file using subtraction, which belongs to the named
///   A0.1 successor matrix rather than the frozen A0 matrix.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::A0Erlang`
///   and asserts the frozen profile still rejects the successor feature.
#[test]
fn run_check_single_file_keeps_subtraction_out_of_a0_erlang_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_erlang_rejects_subtraction");
    let path = fixture(
        &dir,
        "\
module a0_erlang_rejects_subtraction.\n\npub subtract(x: Int): Int ->\n    x - 1.\n",
    );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A0Erlang,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}
