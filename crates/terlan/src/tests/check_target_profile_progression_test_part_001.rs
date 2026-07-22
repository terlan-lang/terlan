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
/// the A0 VM target profile.
///
/// Inputs:
/// - Temporary source file matching the frozen A0 arithmetic fixture.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::A0Vm` and
///   asserts the documented A0 baseline exits successfully.
#[test]
fn run_check_single_file_accepts_mathx_for_a0_vm_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_vm_accepts_mathx");
    let path = fixture(
        &dir,
        "\
module a0_vm_accepts_mathx.\n\npub add(x: Int): Int ->\n    x + 1.\n",
    );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A0Vm,
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
/// - Runs the command-level `check` path with `TargetProfile::A0Vm` and
///   asserts excluded syntax fails before a successful result is returned.
#[test]
fn run_check_single_file_rejects_binary_for_a0_vm_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_vm_rejects_binary");
    let path = fixture(
        &dir,
        "\
module a0_vm_rejects_binary.\n\npub value(): Binary ->\n    \"hello\".\n",
    );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A0Vm,
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
/// - Runs the command-level `check` path with `TargetProfile::A01Vm`
///   and asserts the documented successor matrix exits successfully.
#[test]
fn run_check_single_file_accepts_arithmetic_for_a0_1_vm_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_1_vm_accepts_arithmetic");
    let path = fixture(
        &dir,
        "\
module a0_1_vm_accepts_arithmetic.\n\npub bigger(x: Int, y: Int): Bool ->\n    x * 2 - 1 > y.\n",
    );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A01Vm,
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
/// - Runs the command-level `check` path with `TargetProfile::A02Vm`
///   and asserts the documented successor matrix exits successfully.
#[test]
fn run_check_single_file_accepts_bool_ops_for_a0_2_vm_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_2_vm_accepts_bool_ops");
    let path = fixture(
            &dir,
            "\
module a0_2_vm_accepts_bool_ops.\n\npub both(x: Int, y: Int): Bool ->\n    true and x > 0 or y > 0.\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A02Vm,
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
/// - Runs the command-level `check` path with `TargetProfile::A01Vm`
///   and asserts the earlier successor profile still rejects the new
///   feature.
#[test]
fn run_check_single_file_keeps_bool_ops_out_of_a0_1_vm_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_1_vm_rejects_bool_ops");
    let path = fixture(
        &dir,
        "\
module a0_1_vm_rejects_bool_ops.\n\npub both(x: Int, y: Int): Bool ->\n    x > 0 and y > 0.\n",
    );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A01Vm,
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
/// - Runs the command-level `check` path with `TargetProfile::A03Vm`
///   and asserts the documented successor matrix exits successfully.
#[test]
fn run_check_single_file_accepts_if_expr_for_a0_3_vm_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_3_vm_accepts_if_expr");
    let path = fixture(
        &dir,
        "\
module a0_3_vm_accepts_if_expr.\n\npub choose(x: Int): Int ->\n    if { x > 0 -> x; true -> 0 }.\n",
    );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A03Vm,
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
/// - Runs the command-level `check` path with `TargetProfile::A02Vm`
///   and asserts the earlier successor profile still rejects the new
///   feature.
#[test]
fn run_check_single_file_keeps_if_expr_out_of_a0_2_vm_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_2_vm_rejects_if_expr");
    let path = fixture(
        &dir,
        "\
module a0_2_vm_rejects_if_expr.\n\npub choose(x: Int): Int ->\n    if { x > 0 -> x; true -> 0 }.\n",
    );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A02Vm,
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
/// - Runs the command-level `check` path with `TargetProfile::A04Vm`
///   and asserts the documented successor matrix exits successfully.
#[test]
fn run_check_single_file_accepts_case_expr_for_a0_4_vm_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_4_vm_accepts_case_expr");
    let path = fixture(
        &dir,
        "\
module a0_4_vm_accepts_case_expr.\n\npub choose(x: Int): Int ->\n    case x { 0 -> 0; n -> n }.\n",
    );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A04Vm,
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
/// - Runs the command-level `check` path with `TargetProfile::A03Vm`
///   and asserts the earlier successor profile still rejects the new
///   feature.
#[test]
fn run_check_single_file_keeps_case_expr_out_of_a0_3_vm_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_3_vm_rejects_case_expr");
    let path = fixture(
        &dir,
        "\
module a0_3_vm_rejects_case_expr.\n\npub choose(x: Int): Int ->\n    case x { 0 -> 0; n -> n }.\n",
    );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A03Vm,
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
/// - Runs the command-level `check` path with `TargetProfile::A05Vm`
///   and asserts the documented successor matrix exits successfully.
#[test]
fn run_check_single_file_accepts_raw_atoms_for_a0_5_vm_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_5_vm_accepts_raw_atoms");
    let path = fixture(
            &dir,
            "\
module a0_5_vm_accepts_raw_atoms.\n\npub none(): Dynamic ->\n    Atom[\"none\"].\n\npub is_none(x: Dynamic): Bool ->\n    case x { Atom[\"none\"] -> true; _ -> false }.\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A05Vm,
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
/// - Runs the command-level `check` path with `TargetProfile::A04Vm`
///   and asserts the earlier successor profile still rejects the new
///   feature.
#[test]
fn run_check_single_file_keeps_raw_atoms_out_of_a0_4_vm_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_4_vm_rejects_raw_atoms");
    let path = fixture(
            &dir,
            "\
module a0_4_vm_rejects_raw_atoms.\n\npub none(): Dynamic ->\n    Atom[\"none\"].\n\npub is_none(x: Dynamic): Bool ->\n    case x { Atom[\"none\"] -> true; _ -> false }.\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A04Vm,
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
/// - Runs the command-level `check` path with `TargetProfile::A06Vm`
///   and asserts the documented successor matrix exits successfully.
#[test]
fn run_check_single_file_accepts_tuples_for_a0_6_vm_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_6_vm_accepts_tuples");
    let path = fixture(
            &dir,
            "\
module a0_6_vm_accepts_tuples.\n\npub pair(x: Int): Dynamic ->\n    {x, Atom[\"none\"]}.\n\npub first(value: Dynamic): Int ->\n    case value { {n, Atom[\"none\"]} -> n; _ -> 0 }.\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A06Vm,
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
/// - Runs the command-level `check` path with `TargetProfile::A05Vm`
///   and asserts the earlier successor profile still rejects the new
///   feature.
#[test]
fn run_check_single_file_keeps_tuples_out_of_a0_5_vm_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_5_vm_rejects_tuples");
    let path = fixture(
            &dir,
            "\
module a0_5_vm_rejects_tuples.\n\npub pair(x: Int): Dynamic ->\n    {x, Atom[\"none\"]}.\n\npub first(value: Dynamic): Int ->\n    case value { {n, Atom[\"none\"]} -> n; _ -> 0 }.\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A05Vm,
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
/// - Runs the command-level `check` path with `TargetProfile::A07Vm`
///   and asserts the documented successor matrix exits successfully.
#[test]
fn run_check_single_file_accepts_lists_for_a0_7_vm_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_7_vm_accepts_lists");
    let path = fixture(
            &dir,
            "\
module a0_7_vm_accepts_lists.\n\npub values(): Dynamic ->\n    [1, 2, 3].\n\npub first(value: Dynamic): Int ->\n    case value { [n, _] -> n; _ -> 0 }.\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A07Vm,
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
/// - Runs the command-level `check` path with `TargetProfile::A06Vm`
///   and asserts the earlier successor profile still rejects the new
///   feature.
#[test]
fn run_check_single_file_keeps_lists_out_of_a0_6_vm_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_6_vm_rejects_lists");
    let path = fixture(
            &dir,
            "\
module a0_6_vm_rejects_lists.\n\npub values(): Dynamic ->\n    [1, 2, 3].\n\npub first(value: Dynamic): Int ->\n    case value { [n, _] -> n; _ -> 0 }.\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A06Vm,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command accepts binary/string literal expressions
/// under the named A0.8 VM successor target profile.
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
/// - Runs the command-level `check` path with `TargetProfile::A08Vm`
///   and asserts the documented successor matrix exits successfully.
#[test]
fn run_check_single_file_accepts_binary_for_a0_8_vm_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_8_vm_accepts_binary");
    let path = fixture(
        &dir,
        "\
module a0_8_vm_accepts_binary.\n\npub value(): Binary ->\n    \"hello\".\n",
    );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A08Vm,
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
/// - Runs the command-level `check` path with `TargetProfile::A07Vm`
///   and asserts the earlier successor profile still rejects the new
///   feature.
#[test]
fn run_check_single_file_keeps_binary_out_of_a0_7_vm_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_7_vm_rejects_binary");
    let path = fixture(
        &dir,
        "\
module a0_7_vm_rejects_binary.\n\npub value(): Binary ->\n    \"hello\".\n",
    );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A07Vm,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command accepts expression-side list cons under the
/// named A0.9 VM successor target profile.
///
/// Inputs:
/// - Temporary source file with a list cons expression body.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::A09Vm`
///   and asserts the documented successor matrix exits successfully.
#[test]
fn run_check_single_file_accepts_list_cons_for_a0_9_vm_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_9_vm_accepts_list_cons");
    let path = fixture(
            &dir,
            "\
module a0_9_vm_accepts_list_cons.\n\npub prepend(head: Int, tail: List[Int]): List[Int] ->\n    [head | tail].\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A09Vm,
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
/// - Runs the command-level `check` path with `TargetProfile::A08Vm`
///   and asserts the earlier successor profile still rejects the new
///   feature.
#[test]
fn run_check_single_file_keeps_list_cons_out_of_a0_8_vm_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_8_vm_rejects_list_cons");
    let path = fixture(
            &dir,
            "\
module a0_8_vm_rejects_list_cons.\n\npub prepend(head: Int, tail: List[Int]): List[Int] ->\n    [head | tail].\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A08Vm,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command accepts lowercase local named calls under
/// the named A0.10 VM successor target profile.
///
/// Inputs:
/// - Temporary source file with a private local function and public caller.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::A010Vm`
///   and asserts the documented successor matrix exits successfully.
#[test]
fn run_check_single_file_accepts_named_call_for_a0_10_vm_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_10_vm_accepts_named_call");
    let path = fixture(
            &dir,
            "\
module a0_10_vm_accepts_named_call.\n\nidentity(x: Int): Int ->\n    x.\n\npub call_it(): Int ->\n    identity(1).\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A010Vm,
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
/// - Runs the command-level `check` path with `TargetProfile::A09Vm`
///   and asserts the earlier successor profile still rejects the new
///   feature.
#[test]
fn run_check_single_file_keeps_named_call_out_of_a0_9_vm_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_9_vm_rejects_named_call");
    let path = fixture(
            &dir,
            "\
module a0_9_vm_rejects_named_call.\n\nidentity(x: Int): Int ->\n    x.\n\npub call_it(): Int ->\n    identity(1).\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A09Vm,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}
