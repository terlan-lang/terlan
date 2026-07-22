
/// Verifies the `check` command accepts unary negation under the named
/// A0.11 VM successor target profile.
///
/// Inputs:
/// - Temporary source file with a unary negation expression body.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::A011Vm`
///   and asserts the documented successor matrix exits successfully.
#[test]
fn run_check_single_file_accepts_unary_neg_for_a0_11_vm_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_11_vm_accepts_unary_neg");
    let path = fixture(
        &dir,
        "\
module a0_11_vm_accepts_unary_neg.\n\npub negate(value: Int): Int ->\n    -value.\n",
    );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A011Vm,
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
/// - Runs the command-level `check` path with `TargetProfile::A010Vm`
///   and asserts the earlier successor profile still rejects the new
///   feature.
#[test]
fn run_check_single_file_keeps_unary_neg_out_of_a0_10_vm_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_10_vm_rejects_unary_neg");
    let path = fixture(
        &dir,
        "\
module a0_10_vm_rejects_unary_neg.\n\npub negate(value: Int): Int ->\n    -value.\n",
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

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command accepts resolved constructor calls under
/// the named A0.12 VM successor target profile.
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
/// - Runs the command-level `check` path with `TargetProfile::A012Vm`
///   and asserts the documented successor matrix exits successfully.
#[test]
fn run_check_single_file_accepts_constructor_call_for_a0_12_vm_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_12_vm_accepts_constructor_call");
    let path = fixture(
            &dir,
            "\
module a0_12_vm_accepts_constructor_call.\n\npub constructor Ok {\n    (Value: Int): Dynamic ->\n        Value\n}.\n\npub make(): Dynamic ->\n    Ok(1).\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A012Vm,
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
/// - Runs the command-level `check` path with `TargetProfile::A011Vm`
///   and asserts the earlier successor profile still rejects the new
///   feature.
#[test]
fn run_check_single_file_keeps_constructor_call_out_of_a0_11_vm_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_11_vm_rejects_constructor_call");
    let path = fixture(
            &dir,
            "\
module a0_11_vm_rejects_constructor_call.\n\npub constructor Ok {\n    (Value: Int): Dynamic ->\n        Value\n}.\n\npub make(): Dynamic ->\n    Ok(1).\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A011Vm,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command accepts resolved constructor patterns under
/// the named A0.13 VM successor target profile.
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
/// - Runs the command-level `check` path with `TargetProfile::A013Vm`
///   and asserts the documented successor matrix exits successfully.
#[test]
fn run_check_single_file_accepts_constructor_pattern_for_a0_13_vm_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_13_vm_accepts_constructor_pattern");
    let path = fixture(
            &dir,
            "\
module a0_13_vm_accepts_constructor_pattern.\n\npub constructor Some {\n    (value: Dynamic): Dynamic ->\n        {Atom[\"some\"], value}\n}.\n\npub unwrap(input: Dynamic): Dynamic ->\n    case input {\n        Some(value) -> value\n    }.\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A013Vm,
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
/// - Runs the command-level `check` path with `TargetProfile::A012Vm`
///   and asserts the earlier successor profile still rejects the new
///   feature.
#[test]
fn run_check_single_file_keeps_constructor_pattern_out_of_a0_12_vm_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_12_vm_rejects_constructor_pattern");
    let path = fixture(
            &dir,
            "\
module a0_12_vm_rejects_constructor_pattern.\n\npub constructor Some {\n    (value: Dynamic): Dynamic ->\n        {Atom[\"some\"], value}\n}.\n\npub unwrap(input: Dynamic): Dynamic ->\n    case input {\n        Some(value) -> value\n    }.\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A012Vm,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command accepts anonymous function values under the
/// named A0.14 VM successor target profile.
///
/// Inputs:
/// - Temporary source file with a `(x) -> x` expression body.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::A014Vm`
///   and asserts the documented successor matrix exits successfully.
#[test]
fn run_check_single_file_accepts_lambda_for_a0_14_vm_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_14_vm_accepts_lambda");
    let path = fixture(
        &dir,
        "\
module a0_14_vm_accepts_lambda.\n\npub id_fun(): Term ->\n    (x) -> x.\n",
    );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A014Vm,
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
/// - Runs the command-level `check` path with `TargetProfile::A013Vm`
///   and asserts the earlier successor profile still rejects the new
///   feature.
#[test]
fn run_check_single_file_keeps_lambda_out_of_a0_13_vm_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_13_vm_rejects_lambda");
    let path = fixture(
        &dir,
        "\
module a0_13_vm_rejects_lambda.\n\npub id_fun(): Term ->\n    (x) -> x.\n",
    );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A013Vm,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command accepts constructor extension under the
/// named A0.15 VM successor target profile.
///
/// Inputs:
/// - Temporary source file with `User(id, name) with Admin { ... }`.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::A015Vm`
///   and asserts the documented successor matrix exits successfully.
#[test]
fn run_check_single_file_accepts_constructor_extension_for_a0_15_vm_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_15_vm_accepts_constructor_extension");
    let path = fixture(
            &dir,
            "\
module a0_15_vm_accepts_constructor_extension.\n\npub constructor User {\n    (id: Int, name: Binary): Dynamic -> id\n}.\n\npub build(id: Int, name: Binary): Dynamic ->\n    User(id, name) with Admin { id: id, name: name }.\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A015Vm,
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
/// - Runs the command-level `check` path with `TargetProfile::A014Vm`
///   and asserts the earlier successor profile still rejects the new
///   feature.
#[test]
fn run_check_single_file_keeps_constructor_extension_out_of_a0_14_vm_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_14_vm_rejects_constructor_extension");
    let path = fixture(
            &dir,
            "\
module a0_14_vm_rejects_constructor_extension.\n\npub constructor User {\n    (id: Int, name: Binary): Dynamic -> id\n}.\n\npub build(id: Int, name: Binary): Dynamic ->\n    User(id, name) with Admin { id: id, name: name }.\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A014Vm,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command accepts function-value invocation under the
/// named A0.16 VM successor target profile.
///
/// Inputs:
/// - Temporary source file using dedicated `f(value)` syntax.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::A016Vm`
///   and asserts the documented successor matrix exits successfully.
#[test]
fn run_check_single_file_accepts_fun_call_for_a0_16_vm_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_16_vm_accepts_fun_call");
    let path = fixture(
            &dir,
            "\
module a0_16_vm_accepts_fun_call.\n\npub apply(value: Int, f: (Int) -> Int): Int ->\n    f(value).\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A016Vm,
            ..Default::default()
        },
    );

    assert_eq!(exit, ExitCode::SUCCESS);
}

/// Verifies the A0.15 profile does not silently widen when A0.16
/// function-value invocation syntax is introduced.
///
/// Inputs:
/// - Temporary source file using dedicated `f(value)` syntax.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::A015Vm`
///   and asserts the earlier successor profile rejects the new expression
///   kind.
#[test]
fn run_check_single_file_keeps_fun_call_out_of_a0_15_vm_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_15_vm_rejects_fun_call");
    let path = fixture(
            &dir,
            "\
module a0_15_vm_rejects_fun_call.\n\npub apply(value: Int, f: (Int) -> Int): Int ->\n    f(value).\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A015Vm,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command accepts struct field access under the named
/// A0.17 VM successor target profile.
///
/// Inputs:
/// - Temporary source file with a public struct and `point.x`.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::A017Vm`
///   and asserts the documented successor matrix exits successfully.
#[test]
fn run_check_single_file_accepts_field_access_for_a0_17_vm_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_17_vm_accepts_field_access");
    let path = fixture(
            &dir,
            "\
module a0_17_vm_accepts_field_access.\n\npub struct Point {\n    x: Int\n}.\n\npub read(point: Point): Int ->\n    point.x.\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A017Vm,
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
/// - Runs the command-level `check` path with `TargetProfile::A016Vm`
///   and asserts the earlier successor profile rejects the new expression
///   shape.
#[test]
fn run_check_single_file_keeps_field_access_out_of_a0_16_vm_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_16_vm_rejects_field_access");
    let path = fixture(
            &dir,
            "\
module a0_16_vm_rejects_field_access.\n\npub struct Point {\n    x: Int\n}.\n\npub read(point: Point): Int ->\n    point.x.\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A016Vm,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command accepts local let bindings under the named
/// A0.18 VM successor target profile.
///
/// Inputs:
/// - Temporary source file with a `let y = ...; let z = ...; body` expression.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::A018Vm`
///   and asserts the documented successor matrix exits successfully.
#[test]
fn run_check_single_file_accepts_let_expr_for_a0_18_vm_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_18_vm_accepts_let_expr");
    let path = fixture(
            &dir,
            "\
module a0_18_vm_accepts_let_expr.\n\npub calc(x: Int): Int ->\n    let y = x + 1; let z = y * 2; z + y.\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A018Vm,
            ..Default::default()
        },
    );

    assert_eq!(exit, ExitCode::SUCCESS);
}

/// Verifies the A0.17 profile does not silently widen when A0.18 local let
/// bindings are introduced.
///
/// Inputs:
/// - Temporary source file with a `let y = ...; let z = ...; body` expression.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::A017Vm`
///   and asserts the earlier successor profile rejects the new expression
///   shape.
#[test]
fn run_check_single_file_keeps_let_expr_out_of_a0_17_vm_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_17_vm_rejects_let_expr");
    let path = fixture(
            &dir,
            "\
module a0_17_vm_rejects_let_expr.\n\npub calc(x: Int): Int ->\n    let y = x + 1; let z = y * 2; z + y.\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A017Vm,
            ..Default::default()
        },
    );

    assert_ne!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command accepts index access under the named A0.19
/// VM successor target profile.
///
/// Inputs:
/// - Temporary source file with a `values[0]` expression.
///
/// Output:
/// - Test assertion only; the temporary source file is deleted by the OS
///   temp directory lifecycle.
///
/// Transformation:
/// - Runs the command-level `check` path with `TargetProfile::A019Vm`
///   and asserts the documented successor matrix exits successfully.
#[test]
fn run_check_single_file_accepts_index_access_for_a0_19_vm_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_19_vm_accepts_index_access");
    let path = fixture(
        &dir,
        "\
module a0_19_vm_accepts_index_access.\n\npub first(values: Dynamic): Dynamic ->\n    values[0].\n",
    );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A019Vm,
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
/// - Successful check command exit code under the VM profile.
///
/// Transformation:
/// - Runs the public check command with the VM profile and verifies native
///   vector modules are admitted through the mandatory VM/native collection
///   bridge contract.
#[test]
fn run_check_single_file_accepts_native_vector_selection_sort_for_vm_profile() {
    let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/native/vector_selection_sort.terl");
    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![fixture_path.to_string_lossy().into_owned()],
        },
        CliState {
            target_profile: TargetProfile::Vm,
            ..Default::default()
        },
    );

    assert_eq!(exit, ExitCode::SUCCESS);
}

/// Verifies the `check` command accepts qualified and scoped calls under
/// the named A0.20 VM successor target profile.
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
/// - Runs the command-level `check` path with `TargetProfile::A020Vm`
///   and asserts the documented successor matrix exits successfully.
#[test]
fn run_check_single_file_accepts_qualified_calls_for_a0_20_vm_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_20_vm_accepts_qualified_calls");
    let path = fixture(
            &dir,
            "\
module a0_20_vm_accepts_qualified_calls.\n\npub value(): Int ->\n    1.\n\npub qualified(): Dynamic ->\n    a0_20_vm_accepts_qualified_calls.value().\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A020Vm,
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
/// - Runs the command-level `check` path with `TargetProfile::A019Vm`
///   and asserts the earlier successor profile rejects the new expression
///   shape.
#[test]
fn run_check_single_file_keeps_qualified_calls_out_of_a0_19_vm_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_19_vm_rejects_qualified_calls");
    let path = fixture(
            &dir,
            "\
module a0_19_vm_rejects_qualified_calls.\n\npub value(): Int ->\n    1.\n\npub qualified(): Dynamic ->\n    a0_19_vm_rejects_qualified_calls.value().\n",
        );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A019Vm,
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
/// references under the named A0.21 VM diagnostic target profile.
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
/// - Runs the command-level `check` path with `TargetProfile::A021Vm`
///   and asserts backend-specific reference syntax is rejected by target
///   validation instead of being allowed into backend emission.
#[test]
fn run_check_single_file_rejects_remote_fun_ref_for_a0_21_vm_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_21_vm_rejects_remote_fun_ref");
    let path = fixture(
        &dir,
        "\
module a0_21_vm_rejects_remote_fun_ref.\n\npub reference(): Dynamic ->\n    fun erlang:abs/1.\n",
    );

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![path],
        },
        CliState {
            target_profile: TargetProfile::A021Vm,
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
/// - Runs the command-level `check` path with `TargetProfile::A0Vm`
///   and asserts the frozen profile still rejects the successor feature.
#[test]
fn run_check_single_file_keeps_subtraction_out_of_a0_vm_target_profile() {
    let dir = make_temp_dir("check_single_file_a0_vm_rejects_subtraction");
    let path = fixture(
        &dir,
        "\
module a0_vm_rejects_subtraction.\n\npub subtract(x: Int): Int ->\n    x - 1.\n",
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
