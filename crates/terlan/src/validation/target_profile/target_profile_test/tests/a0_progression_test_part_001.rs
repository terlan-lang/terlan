use super::*;

/// Verifies the A0 VM target profile accepts the frozen arithmetic
/// fixture shape.
///
/// Inputs:
/// - Source containing one public function with an `Int` parameter, `Int`
///   return annotation, and integer addition body.
///
/// Output:
/// - Test passes when target-profile validation reports no violations.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   the frozen A0 profile without mutating compiler artifacts.
#[test]
fn target_profile_accepts_mathx_for_a0_vm_profile() {
    let module = lower(
        "\
module profile_test_a0_mathx.\n\npub add(x: Int): Int ->\n    x + 1.\n",
        "src/profile_test_a0_mathx.terl",
    );

    let a0 = target_profile_checks(&module, TargetProfile::A0Vm);

    assert!(
        a0.is_empty(),
        "A0 VM profile should accept the frozen arithmetic shape: {:?}",
        a0
    );
}

/// Verifies the A0 VM target profile reports a stable unsupported-form
/// diagnostic for features outside the frozen fixture matrix.
///
/// Inputs:
/// - Source containing a binary/string literal body.
///
/// Output:
/// - Test passes when target-profile validation reports
///   `target_profile_unsupported` for `a0-vm`.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   that an excluded expression shape is rejected with a stable diagnostic.
#[test]
fn target_profile_rejects_binary_for_a0_vm_profile() {
    let module = lower(
        "\
module profile_test_a0_binary.\n\npub value(): Binary ->\n    \"hello\".\n",
        "src/profile_test_a0_binary.terl",
    );

    let a0 = target_profile_checks(&module, TargetProfile::A0Vm);

    assert!(
        a0.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation.message.contains("target `a0-vm`")
                && violation.message.contains("expression")
        }),
        "A0 VM profile should reject excluded binary/string literals: {:?}",
        a0
    );
}

/// Verifies the named A0.1 VM successor profile accepts simple Int
/// arithmetic and comparison expressions.
///
/// Inputs:
/// - Source containing multiplication, subtraction, and greater-than over
///   `Int` parameters.
///
/// Output:
/// - Test passes when target-profile validation reports no violations.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   the named A0.1 profile without mutating compiler artifacts.
#[test]
fn target_profile_accepts_arithmetic_for_a0_1_vm_profile() {
    let module = lower(
        "\
module profile_test_a0_1_arithmetic.\n\npub bigger(x: Int, y: Int): Bool ->\n    x * 2 - 1 > y.\n",
        "src/profile_test_a0_1_arithmetic.terl",
    );

    let a0_1 = target_profile_checks(&module, TargetProfile::A01Vm);

    assert!(
        a0_1.is_empty(),
        "A0.1 VM profile should accept simple arithmetic/comparison: {:?}",
        a0_1
    );
}

/// Verifies the named A0.2 VM successor profile accepts boolean
/// literals and boolean operators on top of the A0.1 arithmetic subset.
///
/// Inputs:
/// - Source containing `Bool` return annotation, `true`, `and`, `or`, and
///   comparison expressions over `Int` parameters.
///
/// Output:
/// - Test passes when target-profile validation reports no violations.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   the named A0.2 profile without mutating compiler artifacts.
#[test]
fn target_profile_accepts_bool_ops_for_a0_2_vm_profile() {
    let module = lower(
        "\
module profile_test_a0_2_bool_ops.\n\npub both(x: Int, y: Int): Bool ->\n    true and x > 0 or y > 0.\n",
        "src/profile_test_a0_2_bool_ops.terl",
    );

    let a0_2 = target_profile_checks(&module, TargetProfile::A02Vm);

    assert!(
        a0_2.is_empty(),
        "A0.2 VM profile should accept boolean literals/operators: {:?}",
        a0_2
    );
}

/// Verifies the named A0.1 VM successor profile does not silently widen
/// to include A0.2 boolean operators.
///
/// Inputs:
/// - Source containing `and` over comparison expressions.
///
/// Output:
/// - Test passes when target-profile validation reports
///   `target_profile_unsupported` for `a0.1-vm`.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   that A0.1 remains narrower than the A0.2 successor profile.
#[test]
fn target_profile_keeps_bool_ops_out_of_a0_1_vm_profile() {
    let module = lower(
        "\
module profile_test_a0_1_bool_ops.\n\npub both(x: Int, y: Int): Bool ->\n    x > 0 and y > 0.\n",
        "src/profile_test_a0_1_bool_ops.terl",
    );

    let a0_1 = target_profile_checks(&module, TargetProfile::A01Vm);

    assert!(
        a0_1.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation.message.contains("target `a0.1-vm`")
                && violation.message.contains("expression")
        }),
        "A0.1 VM profile should reject A0.2 boolean operators: {:?}",
        a0_1
    );
}

/// Verifies the named A0.3 VM successor profile accepts simple
/// conditional expressions over the A0.2 boolean subset.
///
/// Inputs:
/// - Source containing an `if` expression with comparison and boolean
///   literal branch conditions.
///
/// Output:
/// - Test passes when target-profile validation reports no violations.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   the named A0.3 profile without mutating compiler artifacts.
#[test]
fn target_profile_accepts_if_expr_for_a0_3_vm_profile() {
    let module = lower(
        "\
module profile_test_a0_3_if_expr.\n\npub choose(x: Int): Int ->\n    if { x > 0 -> x; true -> 0 }.\n",
        "src/profile_test_a0_3_if_expr.terl",
    );

    let a0_3 = target_profile_checks(&module, TargetProfile::A03Vm);

    assert!(
        a0_3.is_empty(),
        "A0.3 VM profile should accept simple if expressions: {:?}",
        a0_3
    );
}

/// Verifies the named A0.2 VM successor profile does not silently widen
/// to include A0.3 conditional expressions.
///
/// Inputs:
/// - Source containing an `if` expression over A0.2-compatible child
///   expressions.
///
/// Output:
/// - Test passes when target-profile validation reports
///   `target_profile_unsupported` for `a0.2-vm`.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   that A0.2 remains narrower than the A0.3 successor profile.
#[test]
fn target_profile_keeps_if_expr_out_of_a0_2_vm_profile() {
    let module = lower(
        "\
module profile_test_a0_2_if_expr.\n\npub choose(x: Int): Int ->\n    if { x > 0 -> x; true -> 0 }.\n",
        "src/profile_test_a0_2_if_expr.terl",
    );

    let a0_2 = target_profile_checks(&module, TargetProfile::A02Vm);

    assert!(
        a0_2.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation.message.contains("target `a0.2-vm`")
                && violation.message.contains("expression")
        }),
        "A0.2 VM profile should reject A0.3 if expressions: {:?}",
        a0_2
    );
}

/// Verifies the named A0.4 VM successor profile accepts simple case
/// expressions over integer and variable patterns.
///
/// Inputs:
/// - Source containing a `case` expression with one integer pattern and one
///   variable pattern.
///
/// Output:
/// - Test passes when target-profile validation reports no violations.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   the named A0.4 profile without mutating compiler artifacts.
#[test]
fn target_profile_accepts_case_expr_for_a0_4_vm_profile() {
    let module = lower(
        "\
module profile_test_a0_4_case_expr.\n\npub choose(x: Int): Int ->\n    case x { 0 -> 0; n -> n }.\n",
        "src/profile_test_a0_4_case_expr.terl",
    );

    let a0_4 = target_profile_checks(&module, TargetProfile::A04Vm);

    assert!(
        a0_4.is_empty(),
        "A0.4 VM profile should accept simple case expressions: {:?}",
        a0_4
    );
}

/// Verifies the named A0.3 VM successor profile does not silently widen
/// to include A0.4 case expressions.
///
/// Inputs:
/// - Source containing a `case` expression over A0.3-compatible child
///   expressions.
///
/// Output:
/// - Test passes when target-profile validation reports
///   `target_profile_unsupported` for `a0.3-vm`.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   that A0.3 remains narrower than the A0.4 successor profile.
#[test]
fn target_profile_keeps_case_expr_out_of_a0_3_vm_profile() {
    let module = lower(
        "\
module profile_test_a0_3_case_expr.\n\npub choose(x: Int): Int ->\n    case x { 0 -> 0; n -> n }.\n",
        "src/profile_test_a0_3_case_expr.terl",
    );

    let a0_3 = target_profile_checks(&module, TargetProfile::A03Vm);

    assert!(
        a0_3.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation.message.contains("target `a0.3-vm`")
                && violation.message.contains("expression")
        }),
        "A0.3 VM profile should reject A0.4 case expressions: {:?}",
        a0_3
    );
}

/// Verifies the named A0.5 VM successor profile accepts raw atom
/// literals as expression values and case patterns.
///
/// Inputs:
/// - Source containing a raw atom function body and a case expression with a
///   raw atom literal pattern.
///
/// Output:
/// - Test passes when target-profile validation reports no violations.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   the named A0.5 profile without mutating compiler artifacts.
#[test]
fn target_profile_accepts_raw_atoms_for_a0_5_vm_profile() {
    let module = lower(
        "\
module profile_test_a0_5_raw_atoms.\n\npub none(): Dynamic ->\n    Atom[\"none\"].\n\npub is_none(x: Dynamic): Bool ->\n    case x { Atom[\"none\"] -> true; _ -> false }.\n",
        "src/profile_test_a0_5_raw_atoms.terl",
    );

    let a0_5 = target_profile_checks(&module, TargetProfile::A05Vm);

    assert!(
        a0_5.is_empty(),
        "A0.5 VM profile should accept raw atom literals: {:?}",
        a0_5
    );
}

/// Verifies the named A0.4 VM successor profile does not silently widen
/// to include A0.5 raw atom literals.
///
/// Inputs:
/// - Source containing a raw atom literal expression and pattern.
///
/// Output:
/// - Test passes when target-profile validation reports
///   `target_profile_unsupported` for `a0.4-vm`.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   that A0.4 remains narrower than the A0.5 successor profile.
#[test]
fn target_profile_keeps_raw_atoms_out_of_a0_4_vm_profile() {
    let module = lower(
        "\
module profile_test_a0_4_raw_atoms.\n\npub none(): Dynamic ->\n    Atom[\"none\"].\n\npub is_none(x: Dynamic): Bool ->\n    case x { Atom[\"none\"] -> true; _ -> false }.\n",
        "src/profile_test_a0_4_raw_atoms.terl",
    );

    let a0_4 = target_profile_checks(&module, TargetProfile::A04Vm);

    assert!(
        a0_4.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation.message.contains("target `a0.4-vm`")
                && violation.message.contains("expression")
        }),
        "A0.4 VM profile should reject A0.5 raw atom literals: {:?}",
        a0_4
    );
}

/// Verifies the named A0.6 VM successor profile accepts tuple
/// expressions and tuple case patterns over A0.5-compatible children.
///
/// Inputs:
/// - Source containing tuple construction and tuple pattern matching.
///
/// Output:
/// - Test passes when target-profile validation reports no violations.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   the named A0.6 profile without mutating compiler artifacts.
#[test]
fn target_profile_accepts_tuples_for_a0_6_vm_profile() {
    let module = lower(
        "\
module profile_test_a0_6_tuples.\n\npub pair(x: Int): Dynamic ->\n    {x, Atom[\"none\"]}.\n\npub first(value: Dynamic): Int ->\n    case value { {n, Atom[\"none\"]} -> n; _ -> 0 }.\n",
        "src/profile_test_a0_6_tuples.terl",
    );

    let a0_6 = target_profile_checks(&module, TargetProfile::A06Vm);

    assert!(
        a0_6.is_empty(),
        "A0.6 VM profile should accept tuple expressions/patterns: {:?}",
        a0_6
    );
}

/// Verifies the named A0.5 VM successor profile does not silently widen
/// to include A0.6 tuple expressions and patterns.
///
/// Inputs:
/// - Source containing tuple construction and tuple pattern matching.
///
/// Output:
/// - Test passes when target-profile validation reports
///   `target_profile_unsupported` for `a0.5-vm`.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   that A0.5 remains narrower than the A0.6 successor profile.
#[test]
fn target_profile_keeps_tuples_out_of_a0_5_vm_profile() {
    let module = lower(
        "\
module profile_test_a0_5_tuples.\n\npub pair(x: Int): Dynamic ->\n    {x, Atom[\"none\"]}.\n\npub first(value: Dynamic): Int ->\n    case value { {n, Atom[\"none\"]} -> n; _ -> 0 }.\n",
        "src/profile_test_a0_5_tuples.terl",
    );

    let a0_5 = target_profile_checks(&module, TargetProfile::A05Vm);

    assert!(
        a0_5.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation.message.contains("target `a0.5-vm`")
                && violation.message.contains("expression")
        }),
        "A0.5 VM profile should reject A0.6 tuple forms: {:?}",
        a0_5
    );
}

/// Verifies the named A0.7 VM successor profile accepts list
/// expressions and fixed-list case patterns over A0.6-compatible children.
///
/// Inputs:
/// - Source containing list construction and fixed-list pattern matching.
///
/// Output:
/// - Test passes when target-profile validation reports no violations.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   the named A0.7 profile without mutating compiler artifacts.
#[test]
fn target_profile_accepts_lists_for_a0_7_vm_profile() {
    let module = lower(
        "\
module profile_test_a0_7_lists.\n\npub values(): Dynamic ->\n    [1, 2, 3].\n\npub first(value: Dynamic): Int ->\n    case value { [n, _] -> n; _ -> 0 }.\n",
        "src/profile_test_a0_7_lists.terl",
    );

    let a0_7 = target_profile_checks(&module, TargetProfile::A07Vm);

    assert!(
        a0_7.is_empty(),
        "A0.7 VM profile should accept list expressions/patterns: {:?}",
        a0_7
    );
}

/// Verifies the named A0.6 VM successor profile does not silently widen
/// to include A0.7 list expressions and patterns.
///
/// Inputs:
/// - Source containing list construction and fixed-list pattern matching.
///
/// Output:
/// - Test passes when target-profile validation reports
///   `target_profile_unsupported` for `a0.6-vm`.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   that A0.6 remains narrower than the A0.7 successor profile.
#[test]
fn target_profile_keeps_lists_out_of_a0_6_vm_profile() {
    let module = lower(
        "\
module profile_test_a0_6_lists.\n\npub values(): Dynamic ->\n    [1, 2, 3].\n\npub first(value: Dynamic): Int ->\n    case value { [n, _] -> n; _ -> 0 }.\n",
        "src/profile_test_a0_6_lists.terl",
    );

    let a0_6 = target_profile_checks(&module, TargetProfile::A06Vm);

    assert!(
        a0_6.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation.message.contains("target `a0.6-vm`")
                && violation.message.contains("expression")
        }),
        "A0.6 VM profile should reject A0.7 list forms: {:?}",
        a0_6
    );
}

/// Verifies the named A0.8 VM successor profile accepts binary/string
/// literal expressions over the A0.7-compatible subset.
///
/// Inputs:
/// - Source containing a `Binary` return annotation and string literal
///   expression body.
///
/// Output:
/// - Test passes when target-profile validation reports no violations.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   the named A0.8 profile without mutating compiler artifacts.
#[test]
fn target_profile_accepts_binary_for_a0_8_vm_profile() {
    let module = lower(
        "\
module profile_test_a0_8_binary.\n\npub value(): Binary ->\n    \"hello\".\n",
        "src/profile_test_a0_8_binary.terl",
    );

    let a0_8 = target_profile_checks(&module, TargetProfile::A08Vm);

    assert!(
        a0_8.is_empty(),
        "A0.8 VM profile should accept binary/string literals: {:?}",
        a0_8
    );
}

/// Verifies the named A0.7 VM successor profile does not silently widen
/// to include A0.8 binary/string literal expressions.
///
/// Inputs:
/// - Source containing a string literal expression.
///
/// Output:
/// - Test passes when target-profile validation reports
///   `target_profile_unsupported` for `a0.7-vm`.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   that A0.7 remains narrower than the A0.8 successor profile.
#[test]
fn target_profile_keeps_binary_out_of_a0_7_vm_profile() {
    let module = lower(
        "\
module profile_test_a0_7_binary.\n\npub value(): Binary ->\n    \"hello\".\n",
        "src/profile_test_a0_7_binary.terl",
    );

    let a0_7 = target_profile_checks(&module, TargetProfile::A07Vm);

    assert!(
        a0_7.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation.message.contains("target `a0.7-vm`")
                && violation.message.contains("expression")
        }),
        "A0.7 VM profile should reject A0.8 binary/string literals: {:?}",
        a0_7
    );
}

/// Verifies the named A0.9 VM successor profile accepts expression-side
/// list cons over the A0.8-compatible subset.
///
/// Inputs:
/// - Source containing `[head | tail]` as an expression body.
///
/// Output:
/// - Test passes when target-profile validation reports no violations.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   the named A0.9 profile without mutating compiler artifacts.
#[test]
fn target_profile_accepts_list_cons_for_a0_9_vm_profile() {
    let module = lower(
        "\
module profile_test_a0_9_list_cons.\n\npub prepend(head: Int, tail: List[Int]): List[Int] ->\n    [head | tail].\n",
        "src/profile_test_a0_9_list_cons.terl",
    );

    let a0_9 = target_profile_checks(&module, TargetProfile::A09Vm);

    assert!(
        a0_9.is_empty(),
        "A0.9 VM profile should accept expression-side list cons: {:?}",
        a0_9
    );
}

/// Verifies the named A0.8 VM successor profile does not silently widen
/// to include A0.9 list cons expressions.
///
/// Inputs:
/// - Source containing `[head | tail]` as an expression body.
///
/// Output:
/// - Test passes when target-profile validation reports
///   `target_profile_unsupported` for `a0.8-vm`.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   that A0.8 remains narrower than the A0.9 successor profile.
#[test]
fn target_profile_keeps_list_cons_out_of_a0_8_vm_profile() {
    let module = lower(
        "\
module profile_test_a0_8_list_cons.\n\npub prepend(head: Int, tail: List[Int]): List[Int] ->\n    [head | tail].\n",
        "src/profile_test_a0_8_list_cons.terl",
    );

    let a0_8 = target_profile_checks(&module, TargetProfile::A08Vm);

    assert!(
        a0_8.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation.message.contains("target `a0.8-vm`")
                && violation.message.contains("expression")
        }),
        "A0.8 VM profile should reject A0.9 list cons expressions: {:?}",
        a0_8
    );
}

/// Verifies the named A0.10 VM successor profile accepts lowercase
/// local named calls over the A0.9-compatible subset.
///
/// Inputs:
/// - Source containing a private lowercase local function and a public
///   function that calls it.
///
/// Output:
/// - Test passes when target-profile validation reports no violations.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   the named A0.10 profile without mutating compiler artifacts.
#[test]
fn target_profile_accepts_named_call_for_a0_10_vm_profile() {
    let module = lower(
        "\
module profile_test_a0_10_named_call.\n\nidentity(x: Int): Int ->\n    x.\n\npub call_it(): Int ->\n    identity(1).\n",
        "src/profile_test_a0_10_named_call.terl",
    );

    let a0_10 = target_profile_checks(&module, TargetProfile::A010Vm);

    assert!(
        a0_10.is_empty(),
        "A0.10 VM profile should accept lowercase local named calls: {:?}",
        a0_10
    );
}

/// Verifies the named A0.9 VM successor profile does not silently widen
/// to include A0.10 local named-call expressions.
///
/// Inputs:
/// - Source containing a lowercase local named call.
///
/// Output:
/// - Test passes when target-profile validation reports
///   `target_profile_unsupported` for `a0.9-vm`.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   that A0.9 remains narrower than the A0.10 successor profile.
#[test]
fn target_profile_keeps_named_call_out_of_a0_9_vm_profile() {
    let module = lower(
        "\
module profile_test_a0_9_named_call.\n\nidentity(x: Int): Int ->\n    x.\n\npub call_it(): Int ->\n    identity(1).\n",
        "src/profile_test_a0_9_named_call.terl",
    );

    let a0_9 = target_profile_checks(&module, TargetProfile::A09Vm);

    assert!(
        a0_9.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation.message.contains("target `a0.9-vm`")
                && violation.message.contains("expression")
        }),
        "A0.9 VM profile should reject A0.10 local named calls: {:?}",
        a0_9
    );
}

/// Verifies the named A0.11 VM successor profile accepts unary negation
/// over the A0.10-compatible subset.
///
/// Inputs:
/// - Source containing `-value` as an expression body.
///
/// Output:
/// - Test passes when target-profile validation reports no violations.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   the named A0.11 profile without mutating compiler artifacts.
#[test]
fn target_profile_accepts_unary_neg_for_a0_11_vm_profile() {
    let module = lower(
        "\
module profile_test_a0_11_unary_neg.\n\npub negate(value: Int): Int ->\n    -value.\n",
        "src/profile_test_a0_11_unary_neg.terl",
    );

    let a0_11 = target_profile_checks(&module, TargetProfile::A011Vm);

    assert!(
        a0_11.is_empty(),
        "A0.11 VM profile should accept unary negation: {:?}",
        a0_11
    );
}

/// Verifies the named A0.10 VM successor profile does not silently widen
/// to include A0.11 unary negation expressions.
///
/// Inputs:
/// - Source containing `-value` as an expression body.
///
/// Output:
/// - Test passes when target-profile validation reports
///   `target_profile_unsupported` for `a0.10-vm`.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   that A0.10 remains narrower than the A0.11 successor profile.
#[test]
fn target_profile_keeps_unary_neg_out_of_a0_10_vm_profile() {
    let module = lower(
        "\
module profile_test_a0_10_unary_neg.\n\npub negate(value: Int): Int ->\n    -value.\n",
        "src/profile_test_a0_10_unary_neg.terl",
    );

    let a0_10 = target_profile_checks(&module, TargetProfile::A010Vm);

    assert!(
        a0_10.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation.message.contains("target `a0.10-vm`")
                && violation.message.contains("expression")
        }),
        "A0.10 VM profile should reject A0.11 unary negation: {:?}",
        a0_10
    );
}

/// Verifies the named A0.12 VM successor profile accepts resolved
/// constructor calls over the A0.11-compatible subset.
///
/// Inputs:
/// - Source containing an explicit constructor declaration and a matching
///   constructor call expression.
///
/// Output:
/// - Test passes when target-profile validation reports no violations.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   the named A0.12 profile without mutating compiler artifacts.
#[test]
fn target_profile_accepts_constructor_call_for_a0_12_vm_profile() {
    let module = lower(
        "\
module profile_test_a0_12_constructor_call.\n\npub constructor Ok {\n    (Value: Int): Dynamic ->\n        Value\n}.\n\npub make(): Dynamic ->\n    Ok(1).\n",
        "src/profile_test_a0_12_constructor_call.terl",
    );

    let a0_12 = target_profile_checks(&module, TargetProfile::A012Vm);

    assert!(
        a0_12.is_empty(),
        "A0.12 VM profile should accept resolved constructor calls: {:?}",
        a0_12
    );
}

/// Verifies the named A0.11 VM successor profile does not silently widen
/// to include A0.12 constructor-call expressions.
///
/// Inputs:
/// - Source containing a resolved constructor call expression.
///
/// Output:
/// - Test passes when target-profile validation reports
///   `target_profile_unsupported` for `a0.11-vm`.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   that A0.11 remains narrower than the A0.12 successor profile.
#[test]
fn target_profile_keeps_constructor_call_out_of_a0_11_vm_profile() {
    let module = lower(
        "\
module profile_test_a0_11_constructor_call.\n\npub constructor Ok {\n    (Value: Int): Dynamic ->\n        Value\n}.\n\npub make(): Dynamic ->\n    Ok(1).\n",
        "src/profile_test_a0_11_constructor_call.terl",
    );

    let a0_11 = target_profile_checks(&module, TargetProfile::A011Vm);

    assert!(
        a0_11.iter().any(|violation| {
            violation.code == "target_profile_unsupported"
                && violation.message.contains("target `a0.11-vm`")
                && violation.message.contains("expression")
        }),
        "A0.11 VM profile should reject A0.12 constructor calls: {:?}",
        a0_11
    );
}

/// Verifies the named A0.13 VM successor profile accepts resolved
/// constructor patterns over the A0.12-compatible subset.
///
/// Inputs:
/// - Source containing an explicit constructor declaration and matching
///   constructor pattern in a case expression.
///
/// Output:
/// - Test passes when target-profile validation reports no violations.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   the named A0.13 profile without mutating compiler artifacts.
#[test]
fn target_profile_accepts_constructor_pattern_for_a0_13_vm_profile() {
    let module = lower(
        "\
module profile_test_a0_13_constructor_pattern.\n\npub constructor Some {\n    (value: Dynamic): Dynamic ->\n        {Atom[\"some\"], value}\n}.\n\npub unwrap(input: Dynamic): Dynamic ->\n    case input {\n        Some(value) -> value\n    }.\n",
        "src/profile_test_a0_13_constructor_pattern.terl",
    );

    let a0_13 = target_profile_checks(&module, TargetProfile::A013Vm);

    assert!(
        a0_13.is_empty(),
        "A0.13 VM profile should accept resolved constructor patterns: {:?}",
        a0_13
    );
}
