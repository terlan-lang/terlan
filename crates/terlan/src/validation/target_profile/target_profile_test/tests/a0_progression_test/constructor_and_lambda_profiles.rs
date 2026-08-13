use super::*;

/// Verifies the named A0.12 VM successor profile does not silently widen
/// to include A0.13 constructor-pattern forms.
///
/// Inputs:
/// - Source containing a resolved constructor pattern in a case expression.
///
/// Output:
/// - Test passes when target-profile validation reports
///   `target_profile_unsupported` for `a0.12-vm`.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   that A0.12 remains narrower than the A0.13 successor profile.
#[test]
pub(super) fn target_profile_keeps_constructor_pattern_out_of_a0_12_vm_profile() {
    let module = lower(
        "\
module profile_test_a0_12_constructor_pattern.\n\npub constructor Some {\n    (value: Dynamic): Dynamic ->\n        {Atom[\"some\"], value}\n}.\n\npub unwrap(input: Dynamic): Dynamic ->\n    case input {\n        Some(value) -> value\n    }.\n",
        "src/profile_test_a0_12_constructor_pattern.terl",
    );

    let a0_12 = target_profile_checks(&module, TargetProfile::A012Vm);

    assert!(
        a0_12.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation.message.contains("target `a0.12-vm`")
                && (violation.message.contains("expression")
                    || violation.message.contains("pattern"))
        }),
        "A0.12 VM profile should reject A0.13 constructor patterns: {:?}",
        a0_12
    );
}

/// Verifies the named A0.14 VM successor profile accepts anonymous
/// function values over the A0.13-compatible subset.
///
/// Inputs:
/// - Source containing `(x) -> x` as an expression body.
///
/// Output:
/// - Test passes when target-profile validation reports no violations.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   the named A0.14 profile without mutating compiler artifacts.
#[test]
pub(super) fn target_profile_accepts_lambda_for_a0_14_vm_profile() {
    let module = lower(
        "\
module profile_test_a0_14_lambda.\n\npub id_fun(): Term ->\n    (x) -> x.\n",
        "src/profile_test_a0_14_lambda.terl",
    );

    let a0_14 = target_profile_checks(&module, TargetProfile::A014Vm);

    assert!(
        a0_14.is_empty(),
        "A0.14 VM profile should accept anonymous function values: {:?}",
        a0_14
    );
}

/// Verifies the named A0.13 VM successor profile does not silently widen
/// to include A0.14 anonymous function values.
///
/// Inputs:
/// - Source containing `(x) -> x` as an expression body.
///
/// Output:
/// - Test passes when target-profile validation reports
///   `target_profile_unsupported` for `a0.13-vm`.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   that A0.13 remains narrower than the A0.14 successor profile.
#[test]
pub(super) fn target_profile_keeps_lambda_out_of_a0_13_vm_profile() {
    let module = lower(
        "\
module profile_test_a0_13_lambda.\n\npub id_fun(): Term ->\n    (x) -> x.\n",
        "src/profile_test_a0_13_lambda.terl",
    );

    let a0_13 = target_profile_checks(&module, TargetProfile::A013Vm);

    assert!(
        a0_13.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation.message.contains("target `a0.13-vm`")
                && violation.message.contains("expression")
        }),
        "A0.13 VM profile should reject A0.14 lambda expressions: {:?}",
        a0_13
    );
}

/// Verifies the named A0.15 VM successor profile accepts constructor
/// extension expressions over the A0.14-compatible subset.
///
/// Inputs:
/// - Source containing `User(id, name) with Admin { ... }` as an expression
///   body.
///
/// Output:
/// - Test passes when target-profile validation reports no violations.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   the named A0.15 profile without mutating compiler artifacts.
#[test]
pub(super) fn target_profile_accepts_constructor_extension_for_a0_15_vm_profile() {
    let module = lower(
        "\
module profile_test_a0_15_constructor_extension.\n\npub constructor User {\n    (id: Int, name: Binary): Dynamic -> id\n}.\n\npub build(id: Int, name: Binary): Dynamic ->\n    User(id, name) with Admin { id: id, name: name }.\n",
        "src/profile_test_a0_15_constructor_extension.terl",
    );

    let a0_15 = target_profile_checks(&module, TargetProfile::A015Vm);

    assert!(
        a0_15.is_empty(),
        "A0.15 VM profile should accept constructor extension: {:?}",
        a0_15
    );
}

/// Verifies the named A0.14 VM successor profile does not silently
/// widen to include A0.15 constructor extension expressions.
///
/// Inputs:
/// - Source containing `User(id, name) with Admin { ... }` as an expression
///   body.
///
/// Output:
/// - Test passes when target-profile validation reports
///   `target_profile_unsupported` for `a0.14-vm`.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   that A0.14 remains narrower than the A0.15 successor profile.
#[test]
pub(super) fn target_profile_keeps_constructor_extension_out_of_a0_14_vm_profile() {
    let module = lower(
        "\
module profile_test_a0_14_constructor_extension.\n\npub constructor User {\n    (id: Int, name: Binary): Dynamic -> id\n}.\n\npub build(id: Int, name: Binary): Dynamic ->\n    User(id, name) with Admin { id: id, name: name }.\n",
        "src/profile_test_a0_14_constructor_extension.terl",
    );

    let a0_14 = target_profile_checks(&module, TargetProfile::A014Vm);

    assert!(
        a0_14.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation.message.contains("target `a0.14-vm`")
                && violation.message.contains("expression")
        }),
        "A0.14 VM profile should reject A0.15 constructor extension: {:?}",
        a0_14
    );
}

/// Verifies the named A0.16 VM successor profile accepts dedicated
/// function-value invocation syntax.
///
/// Inputs:
/// - Source containing `f(value)` in a function body.
///
/// Output:
/// - Test passes when target-profile validation reports no violations.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   the named A0.16 profile without mutating compiler artifacts.
#[test]
pub(super) fn target_profile_accepts_fun_call_for_a0_16_vm_profile() {
    let module = lower(
        "\
module profile_test_a0_16_fun_call.\n\npub apply(value: Int, f: (Int) -> Int): Int ->\n    f(value).\n",
        "src/profile_test_a0_16_fun_call.terl",
    );

    let a0_16 = target_profile_checks(&module, TargetProfile::A016Vm);

    assert!(
        a0_16.is_empty(),
        "A0.16 VM profile should accept function-value invocation: {:?}",
        a0_16
    );
}

/// Verifies the named A0.15 VM successor profile does not silently
/// widen to include A0.16 function-value invocation syntax.
///
/// Inputs:
/// - Source containing `f(value)` in a function body.
///
/// Output:
/// - Test passes when target-profile validation reports
///   `target_profile_unsupported` for `a0.15-vm`.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   that A0.15 remains narrower than the A0.16 successor profile.
#[test]
pub(super) fn target_profile_keeps_fun_call_out_of_a0_15_vm_profile() {
    let module = lower(
        "\
module profile_test_a0_15_fun_call.\n\npub apply(value: Int, f: (Int) -> Int): Int ->\n    f(value).\n",
        "src/profile_test_a0_15_fun_call.terl",
    );

    let a0_15 = target_profile_checks(&module, TargetProfile::A015Vm);

    assert!(
        a0_15.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation.message.contains("target `a0.15-vm`")
                && violation.message.contains("FunctionCall")
        }),
        "A0.15 VM profile should reject A0.16 function-value invocation: {:?}",
        a0_15
    );
}

/// Verifies the named A0.17 VM successor profile accepts struct field
/// access expressions.
///
/// Inputs:
/// - Source containing a public struct and `point.x` expression.
///
/// Output:
/// - Test passes when target-profile validation reports no violations.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   the named A0.17 profile without mutating compiler artifacts.
#[test]
pub(super) fn target_profile_accepts_field_access_for_a0_17_vm_profile() {
    let module = lower(
        "\
module profile_test_a0_17_field_access.\n\npub struct Point {\n    x: Int\n}.\n\npub read(point: Point): Int ->\n    point.x.\n",
        "src/profile_test_a0_17_field_access.terl",
    );

    let a0_17 = target_profile_checks(&module, TargetProfile::A017Vm);

    assert!(
        a0_17.is_empty(),
        "A0.17 VM profile should accept struct field access: {:?}",
        a0_17
    );
}

/// Verifies the named A0.16 VM successor profile does not silently
/// widen to include A0.17 struct field access.
///
/// Inputs:
/// - Source containing a public struct and `point.x` expression.
///
/// Output:
/// - Test passes when target-profile validation reports
///   `target_profile_unsupported` for `a0.16-vm`.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   that A0.16 remains narrower than the A0.17 successor profile.
#[test]
pub(super) fn target_profile_keeps_field_access_out_of_a0_16_vm_profile() {
    let module = lower(
        "\
module profile_test_a0_16_field_access.\n\npub struct Point {\n    x: Int\n}.\n\npub read(point: Point): Int ->\n    point.x.\n",
        "src/profile_test_a0_16_field_access.terl",
    );

    let a0_16 = target_profile_checks(&module, TargetProfile::A016Vm);

    assert!(
        a0_16.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation.message.contains("target `a0.16-vm`")
                && violation.message.contains("FieldAccess")
        }),
        "A0.16 VM profile should reject A0.17 field access: {:?}",
        a0_16
    );
}

/// Verifies the named A0.18 VM successor profile accepts local let
/// binding expressions.
///
/// Inputs:
/// - Source containing `let y = expr; let z = expr; body`.
///
/// Output:
/// - Test passes when target-profile validation reports no violations.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   the named A0.18 profile without mutating compiler artifacts.
#[test]
pub(super) fn target_profile_accepts_let_expr_for_a0_18_vm_profile() {
    let module = lower(
        "\
module profile_test_a0_18_let_expr.\n\npub calc(x: Int): Int ->\n    let y = x + 1; let z = y * 2; z + y.\n",
        "src/profile_test_a0_18_let_expr.terl",
    );

    let a0_18 = target_profile_checks(&module, TargetProfile::A018Vm);

    assert!(
        a0_18.is_empty(),
        "A0.18 VM profile should accept local let expressions: {:?}",
        a0_18
    );
}

/// Verifies the named A0.17 VM successor profile does not silently
/// widen to include A0.18 local let binding expressions.
///
/// Inputs:
/// - Source containing `let y = expr; let z = expr; body`.
///
/// Output:
/// - Test passes when target-profile validation reports
///   `target_profile_unsupported` for `a0.17-vm`.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   that A0.17 remains narrower than the A0.18 successor profile.
#[test]
pub(super) fn target_profile_keeps_let_expr_out_of_a0_17_vm_profile() {
    let module = lower(
        "\
module profile_test_a0_17_let_expr.\n\npub calc(x: Int): Int ->\n    let y = x + 1; let z = y * 2; z + y.\n",
        "src/profile_test_a0_17_let_expr.terl",
    );

    let a0_17 = target_profile_checks(&module, TargetProfile::A017Vm);

    assert!(
        a0_17.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation.message.contains("target `a0.17-vm`")
                && violation.message.contains("Let")
        }),
        "A0.17 VM profile should reject A0.18 let expressions: {:?}",
        a0_17
    );
}

/// Verifies the named A0.19 VM successor profile accepts index-access
/// expressions.
///
/// Inputs:
/// - Source containing `values[0]`.
///
/// Output:
/// - Test passes when target-profile validation reports no violations.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   the named A0.19 profile without mutating compiler artifacts.
#[test]
pub(super) fn target_profile_accepts_index_access_for_a0_19_vm_profile() {
    let module = lower(
        "\
module profile_test_a0_19_index_access.\n\npub first(values: Dynamic): Dynamic ->\n    values[0].\n",
        "src/profile_test_a0_19_index_access.terl",
    );

    let a0_19 = target_profile_checks(&module, TargetProfile::A019Vm);

    assert!(
        a0_19.is_empty(),
        "A0.19 VM profile should accept index access: {:?}",
        a0_19
    );
}

/// Verifies the named A0.20 VM successor profile accepts qualified and
/// scoped call expressions.
///
/// Inputs:
/// - Source containing fully qualified calls to real std modules.
///
/// Output:
/// - Test passes when target-profile validation reports no violations.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   the named A0.20 profile without mutating compiler artifacts.
#[test]
pub(super) fn target_profile_accepts_qualified_calls_for_a0_20_vm_profile() {
    let module = lower(
        "\
module profile_test_a0_20_qualified_calls.\n\npub value(): Int ->\n    1.\n\npub qualified(): Dynamic ->\n    profile_test_a0_20_qualified_calls.value().\n",
        "src/profile_test_a0_20_qualified_calls.terl",
    );

    let a0_20 = target_profile_checks(&module, TargetProfile::A020Vm);

    assert!(
        a0_20.is_empty(),
        "A0.20 VM profile should accept qualified/scoped calls: {:?}",
        a0_20
    );
}

/// Verifies the named A0.19 VM successor profile does not silently
/// widen to include A0.20 qualified and scoped call expressions.
///
/// Inputs:
/// - Source containing fully qualified calls to real std modules.
///
/// Output:
/// - Test passes when target-profile validation reports
///   `target_profile_unsupported` for `a0.19-vm`.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   that A0.19 remains narrower than the A0.20 successor profile.
#[test]
pub(super) fn target_profile_keeps_qualified_calls_out_of_a0_19_vm_profile() {
    let module = lower(
        "\
module profile_test_a0_19_qualified_calls.\n\npub value(): Int ->\n    1.\n\npub qualified(): Dynamic ->\n    profile_test_a0_19_qualified_calls.value().\n",
        "src/profile_test_a0_19_qualified_calls.terl",
    );

    let a0_19 = target_profile_checks(&module, TargetProfile::A019Vm);

    assert!(
        a0_19.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation.message.contains("target `a0.19-vm`")
                && violation
                    .message
                    .contains("typed expression shape RemoteCall")
        }),
        "A0.19 VM profile should reject A0.20 qualified/scoped calls: {:?}",
        a0_19
    );
}

/// Verifies the named A0.21 VM diagnostic profile rejects
/// backend-specific remote function references.
///
/// Inputs:
/// - Source containing backend-specific `fun module:function/arity` syntax.
///
/// Output:
/// - Test passes when parsing rejects the backend-specific source form
///   before target-profile validation.
///
/// Transformation:
/// - Parses through the formal syntax-output path and confirms remote
///   function references are no longer canonical Terlan source.
#[test]
pub(super) fn target_profile_rejects_remote_fun_ref_for_a0_21_vm_profile() {
    let parsed = parse_module_as_syntax_output(
        "\
module profile_test_a0_21_remote_fun_ref.\n\npub reference(): Dynamic ->\n    fun erlang:abs/1.\n",
    );

    assert!(
        parsed.is_err(),
        "remote fun references are backend output syntax, not canonical Terlan source"
    );
}

/// Verifies the frozen A0 VM target profile does not accept A0.1
/// successor arithmetic forms.
///
/// Inputs:
/// - Source containing subtraction in an otherwise A0-shaped function.
///
/// Output:
/// - Test passes when target-profile validation reports
///   `target_profile_unsupported` for `a0-vm`.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   that the frozen A0 profile remains narrower than the successor
///   profile.
#[test]
pub(super) fn target_profile_keeps_subtraction_out_of_a0_vm_profile() {
    let module = lower(
        "\
module profile_test_a0_subtraction.\n\npub subtract(x: Int): Int ->\n    x - 1.\n",
        "src/profile_test_a0_subtraction.terl",
    );

    let a0 = target_profile_checks(&module, TargetProfile::A0Vm);

    assert!(
        a0.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation.message.contains("target `a0-vm`")
                && violation.message.contains("expression")
        }),
        "A0 VM profile should reject successor subtraction: {:?}",
        a0
    );
}
