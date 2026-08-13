use super::*;

#[test]
pub(super) fn target_profile_allows_field_access_expr_for_vm_profile() {
    let module = lower(
        "\
module profile_test_field_access_expr.\n\npub f(point: Point): Dynamic ->\n    point.x.\n",
        "src/profile_test_field_access_expr.terl",
    );

    let vm = target_profile_checks(&module, TargetProfile::Vm);

    assert!(
        vm.is_empty(),
        "VM profile should allow typed field-access core terms"
    );
}

#[test]
pub(super) fn target_profile_allows_record_access_expr_for_vm_profile() {
    let module = lower(
        "\
module profile_test_record_access_expr.\n\npub f(point: Point): Dynamic ->\n    point#Point.x.\n",
        "src/profile_test_record_access_expr.terl",
    );

    let vm = target_profile_checks(&module, TargetProfile::Vm);

    assert!(
        vm.is_empty(),
        "VM profile should allow typed record-access core terms"
    );
}

/// Verifies CoreV0 rejects record access expressions.
///
/// Inputs:
/// - Source containing a typed record access expression.
///
/// Output:
/// - Test passes when target-profile validation reports the expression as
///   unsupported for `core-v0`.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   target-subset validation without mutating compiler artifacts.
#[test]
pub(super) fn target_profile_rejects_record_access_expr_for_core_v0_profile() {
    let module = lower(
        "\
module profile_test_record_access_expr_core_v0.\n\npub f(point: Point): Dynamic ->\n    point#Point.x.\n",
        "src/profile_test_record_access_expr_core_v0.terl",
    );

    let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

    assert!(
        core_v0
            .iter()
            .any(|violation| violation.code == "target_profile_unsupported"
                && violation.message.contains("RecordAccess")),
        "CoreV0 profile should reject record-access core terms: {:?}",
        core_v0
    );
}

#[test]
pub(super) fn target_profile_allows_record_update_expr_for_vm_profile() {
    let module = lower(
        "\
module profile_test_record_update_expr.\n\npub f(point: Point): Dynamic ->\n    point#Point { x: 1 }.\n",
        "src/profile_test_record_update_expr.terl",
    );

    let vm = target_profile_checks(&module, TargetProfile::Vm);

    assert!(
        vm.is_empty(),
        "VM profile should allow typed record-update core terms"
    );
}

/// Verifies CoreV0 rejects record update expressions.
///
/// Inputs:
/// - Source containing a typed record update expression.
///
/// Output:
/// - Test passes when target-profile validation reports the expression as
///   unsupported for `core-v0`.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   target-subset validation without mutating compiler artifacts.
#[test]
pub(super) fn target_profile_rejects_record_update_expr_for_core_v0_profile() {
    let module = lower(
        "\
module profile_test_record_update_expr_core_v0.\n\npub f(point: Point): Dynamic ->\n    point#Point { x: 1 }.\n",
        "src/profile_test_record_update_expr_core_v0.terl",
    );

    let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

    assert!(
        core_v0
            .iter()
            .any(|violation| violation.code == "target_profile_unsupported"
                && violation.message.contains("RecordUpdate")),
        "CoreV0 profile should reject record-update core terms: {:?}",
        core_v0
    );
}

#[test]
pub(super) fn target_profile_allows_template_instantiate_expr_for_vm_profile() {
    let module = lower(
        "\
module profile_test_template_instantiate_expr.\n\ntemplate UserCard from \"./user_card.terl.html\" {\n    name: Text\n}.\n\npub f(): Dynamic ->\n    UserCard(name = \"Ada\").\n",
        "src/profile_test_template_instantiate_expr.terl",
    );

    let vm = target_profile_checks(&module, TargetProfile::Vm);

    assert!(
        vm.is_empty(),
        "VM profile should allow typed template-instantiation core terms"
    );
}

/// Verifies CoreV0 rejects template instantiation expressions.
///
/// Inputs:
/// - Source containing a typed template instantiation expression.
///
/// Output:
/// - Test passes when target-profile validation reports the expression as
///   unsupported for `core-v0`.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   target-subset validation without mutating compiler artifacts.
#[test]
pub(super) fn target_profile_rejects_template_instantiate_expr_for_core_v0_profile() {
    let module = lower(
        "\
module profile_test_template_instantiate_expr_core_v0.\n\ntemplate UserCard from \"./user_card.terl.html\" {\n    name: Text\n}.\n\npub f(): Dynamic ->\n    UserCard(name = \"Ada\").\n",
        "src/profile_test_template_instantiate_expr_core_v0.terl",
    );

    let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

    assert!(
        core_v0
            .iter()
            .any(|violation| violation.code == "target_profile_unsupported"
                && violation.message.contains("TemplateInstantiate")),
        "CoreV0 profile should reject template-instantiation core terms: {:?}",
        core_v0
    );
}

#[test]
pub(super) fn target_profile_allows_constructor_chain_expr_for_vm_profile() {
    let module = lower(
        "\
module profile_test_constructor_chain_expr.\n\npub constructor User {\n    (id: Int, name: Binary): Dynamic -> id\n}.\n\npub f(id: Int, name: Binary): Dynamic ->\n    User(id, name) with Admin { id: id, name: name }.\n",
        "src/profile_test_constructor_chain_expr.terl",
    );

    let vm = target_profile_checks(&module, TargetProfile::Vm);

    assert!(
        vm.is_empty(),
        "VM profile should allow typed constructor-chain core terms"
    );
}

/// Verifies CoreV0 rejects partial constructor-chain expressions.
///
/// Inputs:
/// - Source containing a declared constructor-chain expression whose base
///   constructor identity resolves but whose proof coverage remains partial.
///
/// Output:
/// - Test passes when target-profile validation reports the expression as
///   unsupported for `core-v0`.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   target-subset validation without mutating compiler artifacts.
#[test]
pub(super) fn target_profile_rejects_constructor_chain_expr_for_core_v0_profile() {
    let module = lower(
        "\
module profile_test_constructor_chain_expr_core_v0.\n\npub constructor User {\n    (id: Int, name: Binary): Dynamic -> id\n}.\n\npub f(id: Int, name: Binary): Dynamic ->\n    User(id, name) with Admin { id: id, name: name }.\n",
        "src/profile_test_constructor_chain_expr_core_v0.terl",
    );

    let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

    assert!(
        core_v0
            .iter()
            .any(|violation| violation.code == "target_profile_unsupported"
                && violation.message.contains("constructor chain")),
        "CoreV0 profile should reject partial constructor-chain core terms: {:?}",
        core_v0
    );
}

#[test]
pub(super) fn target_profile_allows_resolved_constructor_call_for_vm_profile() {
    let module = lower(
        "\
module profile_test_constructor_call_candidate.\n\npub constructor Ok {\n    (value: Int): Dynamic -> value\n}.\n\npub f(value: Int): Dynamic ->\n    Ok(value).\n",
        "src/profile_test_constructor_call_candidate.terl",
    );

    let vm = target_profile_checks(&module, TargetProfile::Vm);

    assert!(
        vm.is_empty(),
        "VM profile should allow resolved constructor-call core terms"
    );
}

/// Verifies unresolved constructor-call metadata blocks backend validation.
///
/// Inputs:
/// - A directly constructed Lean-covered Core module whose metadata reports
///   one unresolved constructor-call candidate.
///
/// Output:
/// - Test passes when target-profile validation reports
///   `target_profile_unresolved_constructor`.
///
/// Transformation:
/// - Uses the unresolved-constructor fixture helper to isolate the call
///   metadata counter from parser and typechecker diagnostics.
#[test]
pub(super) fn target_profile_rejects_unresolved_constructor_call_candidate() {
    let module = module_with_unresolved_constructor_candidates(1, 0, 0);

    let vm = target_profile_checks(&module, TargetProfile::Vm);

    assert_unresolved_constructor_violation(&vm, 1, 0, 0);
}

/// Verifies unresolved constructor-pattern metadata blocks backend validation.
///
/// Inputs:
/// - A directly constructed Lean-covered Core module whose metadata reports
///   one unresolved constructor-pattern candidate.
///
/// Output:
/// - Test passes when target-profile validation reports
///   `target_profile_unresolved_constructor`.
///
/// Transformation:
/// - Uses the unresolved-constructor fixture helper to isolate the pattern
///   metadata counter from parser and typechecker diagnostics.
#[test]
pub(super) fn target_profile_rejects_unresolved_constructor_pattern_candidate() {
    let module = module_with_unresolved_constructor_candidates(0, 0, 1);

    let vm = target_profile_checks(&module, TargetProfile::Vm);

    assert_unresolved_constructor_violation(&vm, 0, 0, 1);
}

/// Verifies unresolved constructor-chain metadata blocks backend validation.
///
/// Inputs:
/// - A directly constructed Lean-covered Core module whose metadata reports
///   one unresolved constructor-chain candidate.
///
/// Output:
/// - Test passes when target-profile validation reports
///   `target_profile_unresolved_constructor`.
///
/// Transformation:
/// - Uses the unresolved-constructor fixture helper to isolate the chain
///   metadata counter from parser and typechecker diagnostics.
#[test]
pub(super) fn target_profile_rejects_unresolved_constructor_chain_candidate() {
    let module = module_with_unresolved_constructor_candidates(0, 1, 0);

    let vm = target_profile_checks(&module, TargetProfile::Vm);

    assert_unresolved_constructor_violation(&vm, 0, 1, 0);
}

/// Verifies JS target-profile validation rejects unresolved constructor
/// metadata before JavaScript lowering.
///
/// Inputs:
/// - A directly constructed Lean-covered Core module whose metadata reports
///   unresolved constructor call, chain, and pattern candidates.
///
/// Output:
/// - Test passes when JS target-profile validation reports
///   `target_profile_unresolved_constructor`.
///
/// Transformation:
/// - Uses the unresolved-constructor fixture helper to isolate metadata
///   validation from parser, typechecker, and JavaScript emitter diagnostics.
#[test]
pub(super) fn target_profile_rejects_unresolved_constructor_candidates_for_js_profile() {
    let module = module_with_unresolved_constructor_candidates(1, 1, 1);

    let js = target_profile_checks(&module, TargetProfile::JsShared);

    assert_unresolved_constructor_violation_for_profile(&js, TargetProfile::JsShared, 1, 1, 1);
}

#[test]
pub(super) fn target_profile_rejects_remote_fun_ref_source_syntax_before_profile_validation() {
    let parsed = parse_module_as_syntax_output(
        "\
module profile_test_remote_fun_ref_expr.\n\npub f(): Dynamic ->\n    fun erlang:abs/1.\n",
    );

    assert!(
        parsed.is_err(),
        "remote fun references are backend output syntax, not canonical Terlan source"
    );
}

/// Verifies CoreV0 rejects remote function references.
///
/// Inputs:
/// - Source containing a typed remote function reference expression.
///
/// Output:
/// - Test passes when target-profile validation reports the expression as
///   unsupported for `core-v0`.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   target-subset validation without mutating compiler artifacts.
#[test]
pub(super) fn target_profile_rejects_remote_fun_ref_expr_for_core_v0_profile() {
    let parsed = parse_module_as_syntax_output(
        "\
module profile_test_remote_fun_ref_expr_core_v0.\n\npub f(): Dynamic ->\n    fun erlang:abs/1.\n",
    );

    assert!(
        parsed.is_err(),
        "remote fun references are backend output syntax, not canonical Terlan source"
    );
}

#[test]
pub(super) fn target_profile_allows_remote_call_expr_for_vm_profile() {
    let module = lower(
        "\
module profile_test_remote_call_expr.\n\npub f(): Int ->\n    erlang.Math.abs(1).\n",
        "src/profile_test_remote_call_expr.terl",
    );

    let vm = target_profile_checks(&module, TargetProfile::Vm);

    assert!(
        vm.is_empty(),
        "VM profile should allow typed remote-call core terms"
    );
}

/// Verifies CoreV0 rejects proof-model-required remote calls.
///
/// Inputs:
/// - Source containing a typed remote-call expression.
///
/// Output:
/// - Test passes when target-profile validation reports the expression as
///   unsupported for `core-v0`.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   target-subset validation without mutating compiler artifacts.
#[test]
pub(super) fn target_profile_rejects_remote_call_expr_for_core_v0_profile() {
    let module = lower(
        "\
module profile_test_remote_call_expr_core_v0.\n\npub f(): Int ->\n    erlang.Math.abs(1).\n",
        "src/profile_test_remote_call_expr_core_v0.terl",
    );

    let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

    assert!(
        core_v0
            .iter()
            .any(|violation| violation.code == "target_profile_unsupported"
                && violation.message.contains("remote call")),
        "CoreV0 profile should reject proof-model-required remote-call core terms: {:?}",
        core_v0
    );
}

/// Verifies CoreV0 rejects function-value invocation once it lowers to CoreIR.
///
/// Inputs:
/// - Source containing `f(value)` in a function body.
///
/// Output:
/// - Test passes when target-profile validation reports the proof-required
///   function-call expression as unsupported for `core-v0`.
///
/// Transformation:
/// - Locks callable-value invocation outside the direct CoreV0 subset while
///   leaving VM-profile acceptance covered by the A0 progression suite.
#[test]
pub(super) fn target_profile_rejects_function_value_invocation_for_core_v0_profile() {
    let module = lower(
        "\
module profile_test_core_v0_fun_call.\n\npub apply(value: Int, f: (Int) -> Int): Int ->\n    f(value).\n",
        "src/profile_test_core_v0_fun_call.terl",
    );

    let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

    assert!(
        core_v0
            .iter()
            .any(|violation| violation.code == "target_profile_unsupported"
                && violation.message.contains("ProofModelRequired")),
        "CoreV0 profile should reject function-value invocation: {:?}",
        core_v0
    );
}

/// Verifies CoreV0 admits VM-executed std test assertion calls.
///
/// Inputs:
/// - Source containing a `std.test.Test.assert_equal/2` remote call.
///
/// Output:
/// - Test passes when target-profile validation accepts the remote call for
///   VM-default stdlib tests.
///
/// Transformation:
/// - Locks the narrow std remote-call exception without admitting arbitrary
///   runtime-boundary calls.
#[test]
pub(super) fn target_profile_accepts_vm_std_test_remote_call_for_core_v0_profile() {
    let module = lower(
        "\
module profile_test_std_test_remote_call_core_v0.\n\npub f(): Bool ->\n    std.test.Test.assert_equal(1, 1).\n",
        "src/profile_test_std_test_remote_call_core_v0.terl",
    );

    let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

    assert!(
        core_v0.is_empty(),
        "CoreV0 profile should accept VM-executed std.test remote calls: {:?}",
        core_v0
    );
}

/// Verifies CoreV0 admits expressions composed around VM std test calls.
///
/// Inputs:
/// - Source comparing `std.test.Test.fail()` with `false`.
///
/// Output:
/// - Test passes when target-profile validation accepts the expression for
///   VM-default stdlib release tests.
///
/// Transformation:
/// - Protects the parent-expression proof exception needed when a supported
///   std remote call appears inside an otherwise valid CoreV0 expression.
#[test]
pub(super) fn target_profile_accepts_composed_vm_std_test_remote_call_for_core_v0_profile() {
    let module = lower(
        "\
module profile_test_composed_std_test_remote_call_core_v0.\n\npub f(): Bool ->\n    std.test.Test.fail() == false.\n",
        "src/profile_test_composed_std_test_remote_call_core_v0.terl",
    );

    let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

    assert!(
        core_v0.is_empty(),
        "CoreV0 profile should accept composed VM-executed std.test remote calls: {:?}",
        core_v0
    );
}

#[test]
pub(super) fn target_profile_allows_if_expr_for_vm_profile() {
    let module = lower(
        "\
module profile_test_if_expr.\n\npub f(flag: Bool): Int ->\n    if { flag -> 1; true -> 0 }.\n",
        "src/profile_test_if_expr.terl",
    );

    let vm = target_profile_checks(&module, TargetProfile::Vm);

    assert!(
        vm.is_empty(),
        "VM profile should allow typed if-expression core terms"
    );
}

#[test]
pub(super) fn target_profile_allows_try_expr_for_vm_profile() {
    let module = lower(
        "\
module profile_test_try_expr.\n\npub f(): Dynamic ->\n    try 1 {\n        value -> value\n    catch\n        reason -> reason\n    after\n        0 -> Atom[\"done\"]\n    }.\n",
        "src/profile_test_try_expr.terl",
    );

    let vm = target_profile_checks(&module, TargetProfile::Vm);

    assert!(
        vm.is_empty(),
        "VM profile should allow typed try-expression core terms"
    );
}

/// Verifies CoreV0 rejects try expressions.
///
/// Inputs:
/// - Source containing a typed try expression with `of`, `catch`, and
///   `after` branches.
///
/// Output:
/// - Test passes when target-profile validation reports the expression as
///   unsupported for `core-v0`.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   target-subset validation without mutating compiler artifacts.
#[test]
pub(super) fn target_profile_rejects_try_expr_for_core_v0_profile() {
    let module = lower(
        "\
module profile_test_try_expr_core_v0.\n\npub f(): Dynamic ->\n    try 1 {\n        value -> value\n    catch\n        reason -> reason\n    after\n        0 -> Atom[\"done\"]\n    }.\n",
        "src/profile_test_try_expr_core_v0.terl",
    );

    let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

    assert!(
        core_v0
            .iter()
            .any(|violation| violation.code == "target_profile_unsupported"
                && violation.message.contains("Try")),
        "CoreV0 profile should reject try core terms: {:?}",
        core_v0
    );
}

#[test]
pub(super) fn target_profile_allows_unary_op_expr_for_vm_profile() {
    let module = lower(
        "\
module profile_test_unary_op_expr.\n\npub f(value: Int): Int ->\n    -value.\n",
        "src/profile_test_unary_op_expr.terl",
    );

    let vm = target_profile_checks(&module, TargetProfile::Vm);

    assert!(
        vm.is_empty(),
        "VM profile should allow typed unary-op core terms"
    );
}

#[test]
pub(super) fn target_profile_allows_map_pattern_for_vm_profile() {
    let module = lower(
        "\
module profile_test_map_pattern.\n\npub f(value: Dynamic): Dynamic ->\n    case value {\n        {a: x} -> x;\n        _ -> value\n    }.\n",
        "src/profile_test_map_pattern.terl",
    );

    let vm = target_profile_checks(&module, TargetProfile::Vm);

    assert!(
        vm.is_empty(),
        "VM profile should allow typed map-pattern core terms"
    );
}

/// Verifies VM accepts float patterns.
///
/// Inputs:
/// - Source containing a typed case expression with a float pattern.
///
/// Output:
/// - Test passes when target-profile validation reports no VM-profile
///   violations for the lowered module.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   permissive VM-profile validation without mutating compiler
///   artifacts.
#[test]
pub(super) fn target_profile_allows_float_pattern_for_vm_profile() {
    let module = lower(
        "\
module profile_test_float_pattern.\n\npub f(value: Dynamic): Dynamic ->\n    case value {\n        1.0 -> Atom[\"float\"];\n        _ -> Atom[\"other\"]\n    }.\n",
        "src/profile_test_float_pattern.terl",
    );

    let vm = target_profile_checks(&module, TargetProfile::Vm);

    assert!(
        vm.is_empty(),
        "VM profile should allow typed float-pattern core terms"
    );
}

/// Verifies VM accepts string capture patterns.
///
/// Inputs:
/// - Source containing a typed case expression with `${...}` string captures.
///
/// Output:
/// - Test passes when target-profile validation reports no VM-profile
///   violations for the lowered module.
///
/// Transformation:
/// - Lowers source through parser, syntax output, typecheck, and CoreIR before
///   checking that the VM profile owns direct string-pattern execution.
#[test]
pub(super) fn target_profile_allows_string_capture_pattern_for_vm_profile() {
    let module = lower(
        "\
module profile_test_string_capture_pattern.\n\npub f(path: String): String ->\n    case path {\n        \"users/${id: Int}/${name}.json\" where id > 0 -> name;\n        _ -> \"missing\"\n    }.\n",
        "src/profile_test_string_capture_pattern.terl",
    );

    let vm = target_profile_checks(&module, TargetProfile::Vm);

    assert!(
        vm.is_empty(),
        "VM profile should allow typed string-capture pattern core terms: {:?}",
        vm
    );
}

/// Verifies CoreV0 rejects float patterns.
///
/// Inputs:
/// - Source containing a typed case expression with a float pattern.
///
/// Output:
/// - Test passes when target-profile validation reports the pattern as
///   unsupported for `core-v0`.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   target-subset validation without mutating compiler artifacts.
#[test]
pub(super) fn target_profile_rejects_float_pattern_for_core_v0_profile() {
    let module = lower(
        "\
module profile_test_float_pattern_core_v0.\n\npub f(value: Dynamic): Dynamic ->\n    case value {\n        1.0 -> Atom[\"float\"];\n        _ -> Atom[\"other\"]\n    }.\n",
        "src/profile_test_float_pattern_core_v0.terl",
    );

    let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

    assert!(
        core_v0
            .iter()
            .any(|violation| violation.code == "target_profile_unsupported"
                && violation.message.contains("Float")),
        "CoreV0 profile should reject float-pattern core terms: {:?}",
        core_v0
    );
}

/// Verifies CoreV0 rejects string capture patterns.
///
/// Inputs:
/// - Source containing a typed case expression with `${...}` string captures.
///
/// Output:
/// - Test passes when target-profile validation reports the pattern as
///   unsupported for `core-v0`.
///
/// Transformation:
/// - Lowers source through parser, syntax output, typecheck, and CoreIR before
///   checking that the legacy core subset cannot silently accept string capture
///   patterns.
#[test]
pub(super) fn target_profile_rejects_string_capture_pattern_for_core_v0_profile() {
    let module = lower(
        "\
module profile_test_string_capture_pattern_core_v0.\n\npub f(path: String): String ->\n    case path {\n        \"users/${id: Int}/${name}.json\" where id > 0 -> name;\n        _ -> \"missing\"\n    }.\n",
        "src/profile_test_string_capture_pattern_core_v0.terl",
    );

    let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

    assert!(
        core_v0
            .iter()
            .any(|violation| violation.code == "target_profile_unsupported"
                && violation.message.contains("StringPattern")),
        "CoreV0 profile should reject string-capture pattern core terms: {:?}",
        core_v0
    );
}

/// Verifies CoreV0 rejects map patterns.
///
/// Inputs:
/// - Source containing a typed case expression with a map pattern.
///
/// Output:
/// - Test passes when target-profile validation reports the pattern as
///   unsupported for `core-v0`.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   target-subset validation without mutating compiler artifacts.
#[test]
pub(super) fn target_profile_rejects_map_pattern_for_core_v0_profile() {
    let module = lower(
        "\
module profile_test_map_pattern_core_v0.\n\npub f(value: Dynamic): Dynamic ->\n    case value {\n        {a: x} -> x;\n        _ -> value\n    }.\n",
        "src/profile_test_map_pattern_core_v0.terl",
    );

    let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

    assert!(
        core_v0
            .iter()
            .any(|violation| violation.code == "target_profile_unsupported"
                && violation.message.contains("Map")),
        "CoreV0 profile should reject map-pattern core terms: {:?}",
        core_v0
    );
}

#[test]
pub(super) fn target_profile_allows_list_cons_pattern_for_vm_profile() {
    let module = lower(
        "\
module profile_test_list_cons_pattern.\n\npub f(value: List[Int]): Dynamic ->\n    case value {\n        [head | tail] -> head;\n        _ -> value\n    }.\n",
        "src/profile_test_list_cons_pattern.terl",
    );

    let vm = target_profile_checks(&module, TargetProfile::Vm);

    assert!(
        vm.is_empty(),
        "VM profile should allow typed list-cons pattern core terms"
    );
}

/// Verifies CoreV0 rejects list-cons patterns.
///
/// Inputs:
/// - Source containing a typed case expression with a list-cons pattern.
///
/// Output:
/// - Test passes when target-profile validation reports the pattern as
///   unsupported for `core-v0`.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   target-subset validation without mutating compiler artifacts.
#[test]
pub(super) fn target_profile_rejects_list_cons_pattern_for_core_v0_profile() {
    let module = lower(
        "\
module profile_test_list_cons_pattern_core_v0.\n\npub f(value: List[Int]): Dynamic ->\n    case value {\n        [head | tail] -> head;\n        _ -> value\n    }.\n",
        "src/profile_test_list_cons_pattern_core_v0.terl",
    );

    let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

    assert!(
        core_v0
            .iter()
            .any(|violation| violation.code == "target_profile_unsupported"
                && violation.message.contains("ListCons")),
        "CoreV0 profile should reject list-cons pattern core terms: {:?}",
        core_v0
    );
}

#[test]
pub(super) fn target_profile_allows_record_pattern_for_vm_profile() {
    let module = lower(
        "\
module profile_test_record_pattern.\n\npub f(value: Dynamic): Dynamic ->\n    case value {\n        Point { x: x } -> x;\n        _ -> value\n    }.\n",
        "src/profile_test_record_pattern.terl",
    );

    let vm = target_profile_checks(&module, TargetProfile::Vm);

    assert!(
        vm.is_empty(),
        "VM profile should allow typed record-pattern core terms"
    );
}

/// Verifies CoreV0 rejects record patterns.
///
/// Inputs:
/// - Source containing a typed case expression with a record pattern.
///
/// Output:
/// - Test passes when target-profile validation reports the pattern as
///   unsupported for `core-v0`.
///
/// Transformation:
/// - Lowers source through the formal syntax-output/CoreIR path and checks
///   target-subset validation without mutating compiler artifacts.
#[test]
pub(super) fn target_profile_rejects_record_pattern_for_core_v0_profile() {
    let module = lower(
        "\
module profile_test_record_pattern_core_v0.\n\npub f(value: Dynamic): Dynamic ->\n    case value {\n        Point { x: x } -> x;\n        _ -> value\n    }.\n",
        "src/profile_test_record_pattern_core_v0.terl",
    );

    let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

    assert!(
        core_v0
            .iter()
            .any(|violation| violation.code == "target_profile_unsupported"
                && violation.message.contains("Record")),
        "CoreV0 profile should reject record-pattern core terms: {:?}",
        core_v0
    );
}

/// Verifies the portable CoreIR v0 profile accepts a Lean-covered arithmetic
/// expression.
///
/// Inputs:
/// - A source module whose function body lowers to typed `BinaryOp(-)`.
///
/// Output:
/// - Test assertion only; no compiler artifacts are written.
///
/// Transformation:
/// - Lowers source through syntax output to CoreIR, then validates it under
///   the `core-v0` target profile.
#[test]
pub(super) fn target_profile_accepts_subtraction_for_core_v0_profile() {
    let module = lower(
        "\
module profile_test_core_v0_sub.\n\npub f(x: Int, y: Int): Int ->\n    x - y.\n",
        "src/profile_test_core_v0_sub.terl",
    );

    let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

    assert!(
        core_v0.is_empty(),
        "Core v0 profile should accept Lean-covered subtraction: {:?}",
        core_v0
    );
}

/// Verifies the portable CoreIR v0 profile rejects a broad backend-specific
/// expression form while the VM profile remains permissive.
///
/// Inputs:
/// - A source module whose function body lowers to typed map CoreIR.
///
/// Output:
/// - Test assertion only; no compiler artifacts are written.
///
/// Transformation:
/// - Lowers source through syntax output to CoreIR, checks that VM still
///   accepts the shape, then checks that `core-v0` reports unsupported
///   expression coverage or shape.
#[test]
pub(super) fn target_profile_rejects_map_expr_for_core_v0_profile() {
    let module = lower(
        "\
module profile_test_core_v0_map.\n\npub f(): Map ->\n    {a: 1}.\n",
        "src/profile_test_core_v0_map.terl",
    );

    let vm = target_profile_checks(&module, TargetProfile::Vm);
    let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

    assert!(
        vm.is_empty(),
        "VM profile should remain permissive for map core terms"
    );
    assert!(
        core_v0
            .iter()
            .any(|violation| violation.code == "target_profile_unsupported"),
        "Core v0 profile should reject map core terms: {:?}",
        core_v0
    );
}
