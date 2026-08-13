use super::test_support::check_syntax_output;
use super::*;
use crate::terlan_hir::resolve_syntax_module_output;
use crate::terlan_syntax::parse_module_as_syntax_output;

fn analysis(source: &str) -> BindingAnalysis {
    let module = parse_module_as_syntax_output(source)
        .unwrap_or_else(|error| panic!("parse binding fixture `{source}`: {error:?}"));
    analyze_syntax_bindings(&module)
}

fn formatted_collision_names(source: &str) -> Vec<String> {
    analysis(source)
        .collisions
        .iter()
        .map(|collision| collision.name.clone())
        .collect()
}

#[test]
fn parameter_and_sequential_let_share_one_binding_region() {
    let source = r#"
module same_region_parameter.

run(value: Int): Int ->
    let first = value;
    let value = first + 1;
    value.
"#;
    let analysis = analysis(source);
    assert_eq!(analysis.collisions.len(), 1, "{analysis:#?}");
    let collision = &analysis.collisions[0];
    assert_eq!(collision.name, "value");
    assert_eq!(collision.suggested_name, "value_2");
    assert!(collision.span.end > collision.span.start);

    let diagnostics = check_syntax_output(source);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.message
            == "error[binding.same_region]: `value` is already bound in this lexical region; use `value_2` for a distinct immutable binding"
    }));
}

#[test]
fn equals_never_changes_from_update_syntax_into_bind_or_match() {
    let error = parse_module_as_syntax_output(
        r#"
module no_contextual_equals.
run(): Int ->
    let value = 1;
    value = 2;
    value.
"#,
    )
    .expect_err("bare equals cannot become a contextual rebind or match");
    assert!(
        format!("{error:?}").contains("let"),
        "unexpected parser diagnostic: {error:?}"
    );

    let diagnostics = check_syntax_output(
        r#"
module explicit_equality.
run(): Bool ->
    let value = 1;
    value == 2.
"#,
    );
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
}

#[test]
fn formatter_preserves_nested_shadowing_without_creating_same_region_collisions() {
    let source = r#"
module formatter_binding_identity.

pub choose(value: Int): Int ->
    let first = value;
    case first {
        value -> value
    }.
"#;
    assert!(formatted_collision_names(source).is_empty());
    let formatted =
        crate::terlan_syntax::format_source_module(source).expect("format nested binding fixture");
    assert!(
        formatted_collision_names(&formatted).is_empty(),
        "{formatted}"
    );
}

#[test]
fn repeated_let_migration_keeps_distinct_bindings_distinct() {
    let legacy = r#"
module formatter_binding_migration.

pub total(value: Int): Int ->
    let first = value;
    second = first + 1;
    second.
"#;
    let formatted = crate::terlan_syntax::format_source_module_migrating_repeated_lets(legacy)
        .expect("migrate retired implicit repeated let");
    assert!(formatted.contains("let second = first + 1;"), "{formatted}");
    assert!(
        formatted_collision_names(&formatted).is_empty(),
        "{formatted}"
    );
}

#[test]
fn long_sequential_chain_rejects_every_rebinding_without_overwriting_identity() {
    let analysis = analysis(
        r#"
module long_same_region_chain.

run(input: Int): Int ->
    let a = input;
    let b = a;
    let c = b;
    let a = c;
    let b = a;
    b.
"#,
    );
    assert_eq!(
        analysis
            .collisions
            .iter()
            .map(|collision| collision.name.as_str())
            .collect::<Vec<_>>(),
        ["a", "b"]
    );
    let a = analysis
        .evidence
        .bindings
        .iter()
        .find(|binding| binding.name == "a")
        .expect("original a identity");
    assert!(analysis
        .evidence
        .references
        .iter()
        .any(|reference| reference.name == "a" && reference.binding == a.id));
    analysis
        .evidence
        .validate()
        .expect("valid binding evidence");
}

#[test]
fn every_structural_pattern_family_rejects_duplicate_names() {
    let fixtures = [
        (
            "tuple",
            r#"module duplicate_tuple.
run({value, value}: Dynamic): Dynamic -> value.
"#,
        ),
        (
            "list",
            r#"module duplicate_list.
run([value, value]: Dynamic): Dynamic -> value.
"#,
        ),
        (
            "list_cons",
            r#"module duplicate_list_cons.
run([value | value]: Dynamic): Dynamic -> value.
"#,
        ),
        (
            "map",
            r#"module duplicate_map.
run({left: value, right: value}: Dynamic): Dynamic -> value.
"#,
        ),
        (
            "record",
            r#"module duplicate_record.
struct User { left: Int, right: Int }.
run(User { left: value, right: value }: User): Int -> value.
"#,
        ),
        (
            "constructor",
            r#"module duplicate_constructor.
run(Pair(value, value): Dynamic): Dynamic -> value.
"#,
        ),
        (
            "string_capture",
            r#"module duplicate_string_capture.
run("left/${value}/right/${value}": String): String -> value.
"#,
        ),
        (
            "alias",
            r#"module duplicate_alias.
run({value, other} = value: Dynamic): Dynamic -> value.
"#,
        ),
    ];

    for (family, source) in fixtures {
        let analysis = analysis(source);
        assert!(
            analysis
                .collisions
                .iter()
                .any(|collision| collision.name == "value"),
            "{family} did not reject duplicate binding: {analysis:#?}"
        );
    }

    let binary_error = parse_module_as_syntax_output(
        r#"module duplicate_binary_layout.
run(Binary[big] { value: UInt[8], value: UInt[8] }): Int -> value.
"#,
    )
    .expect_err("binary layout duplicate must fail during descriptor parsing");
    assert!(format!("{binary_error:?}").contains("duplicate binary layout field `value`"));
}

#[test]
fn binary_layout_keys_bind_while_repeated_descriptors_remain_metadata() {
    let analysis = analysis(
        r#"
module binary_layout_binding_identity.

run(packet: Binary): Int ->
    case packet {
        Binary[big] { first: Utf16, second: Utf16, third: Utf32 } ->
            first + second + third;
        _ -> 0
    }.
"#,
    );

    assert!(analysis.collisions.is_empty(), "{analysis:#?}");
    let names = analysis
        .evidence
        .bindings
        .iter()
        .map(|binding| binding.name.as_str())
        .collect::<Vec<_>>();
    assert!(names.contains(&"first"), "{analysis:#?}");
    assert!(names.contains(&"second"), "{analysis:#?}");
    assert!(names.contains(&"third"), "{analysis:#?}");
    assert!(!names.contains(&"Utf16"), "{analysis:#?}");
    assert!(!names.contains(&"Utf32"), "{analysis:#?}");
}

#[test]
fn nested_branches_lambdas_comprehensions_and_handlers_get_distinct_identities() {
    let analysis = analysis(
        r#"
module nested_binding_regions.

branches(value: Int, flag: Bool): Int ->
    case flag {
        true ->
            case value {
                value -> value
            };
        false ->
            case value {
                value -> value
            }
    }.

lambda(value: Int): Int ->
    let callback = ((value) -> value);
    callback(value).

comprehension(value: Int, items: List[Int]): List[Int] ->
    [value | value <- items].

handler(value: Dynamic): Dynamic ->
    try value {
        result -> result
    catch
        value -> value
    after
        value -> value
    }.
"#,
    );
    assert!(analysis.collisions.is_empty(), "{analysis:#?}");
    let value_bindings = analysis
        .evidence
        .bindings
        .iter()
        .filter(|binding| binding.name == "value")
        .collect::<Vec<_>>();
    assert!(value_bindings.len() >= 7, "{value_bindings:#?}");
    assert_eq!(
        value_bindings
            .iter()
            .map(|binding| binding.id)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        value_bindings.len()
    );
    assert!(
        value_bindings
            .iter()
            .map(|binding| binding.region)
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            >= 6
    );
}

#[test]
fn grouped_fallible_let_success_chain_is_one_transactional_region() {
    let analysis = analysis(
        r#"
module grouped_fallible_duplicate.

type Ok[T] = {Atom["ok"], value: T}.
type Err[E] = {Atom["error"], reason: E}.
type Result[T, E] = Ok[T] | Err[E].

run(first: Result[Int, String], second: Result[Int, String]): Int ->
    let {
        Ok(value) <- first;
        Ok(value) <- second
    } else {
        Err(_reason) -> 0
    };
    value.
"#,
    );
    assert_eq!(analysis.collisions.len(), 1, "{analysis:#?}");
    assert_eq!(analysis.collisions[0].name, "value");
    let value_bindings = analysis
        .evidence
        .bindings
        .iter()
        .filter(|binding| binding.name == "value")
        .count();
    assert_eq!(value_bindings, 1, "failed binding must not overwrite state");
}

#[test]
fn macro_hygiene_and_post_expansion_collision_analysis_share_one_contract() {
    let module = parse_module_as_syntax_output(
        r#"
module binding_macro_hygiene.

macro with_local(X: Expr): Ast[Int] ->
    quote (let value = 1; unquote(X) + value).

run(value: Int): Int ->
    ?with_local(value).
"#,
    )
    .expect("parse macro fixture");
    let (expanded, diagnostics) = expand_syntax_raw_macros(module);
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    let analysis = analyze_syntax_bindings(&expanded);
    assert!(analysis.collisions.is_empty(), "{analysis:#?}");
    assert!(analysis
        .evidence
        .bindings
        .iter()
        .any(|binding| binding.name.starts_with("__macro_")));
}

#[test]
fn core_ir_carries_valid_binding_references_and_debugger_locals() {
    let module = parse_module_as_syntax_output(
        r#"
module core_binding_identity.

run(input: Int): Int ->
    let output = input + 1;
    output.
"#,
    )
    .expect("parse core binding fixture");
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);
    core.binding_identities
        .validate()
        .expect("CoreIR binding evidence");
    let output = core
        .binding_identities
        .bindings
        .iter()
        .find(|binding| binding.name == "output")
        .expect("output binding");
    assert_eq!(
        core.binding_identities.debugger_locals(output.region),
        core.binding_identities
            .bindings
            .iter()
            .filter(|binding| binding.region == output.region)
            .collect::<Vec<_>>()
    );
    let (declaration, references) = core.binding_identities.references_for(output.id);
    assert_eq!(declaration, Some(output));
    assert_eq!(references.len(), 1);
    assert!(core.contract_text().contains("binding_fingerprint="));
    assert!(core
        .contract_text()
        .contains(&format!("binding={:016x}", output.id.0)));
}

#[test]
fn closure_capture_reference_targets_outer_binding_identity() {
    let analysis = analysis(
        r#"
module closure_binding_identity.

run(value: Int): Int ->
    let callback = ((increment) -> value + increment);
    callback(1).
"#,
    );
    assert!(analysis.collisions.is_empty(), "{analysis:#?}");
    let value = analysis
        .evidence
        .bindings
        .iter()
        .find(|binding| binding.name == "value")
        .expect("outer value binding");
    let (_, references) = analysis.evidence.references_for(value.id);
    assert_eq!(references.len(), 1, "{references:#?}");
    assert!(references[0].path.contains("lambda"), "{references:#?}");
}

#[test]
fn binding_ids_survive_unrelated_declaration_insertion_for_incremental_caches() {
    let before = analysis(
        r#"
module stable_binding_identity.
run(input: Int): Int ->
    let output = input;
    output.
"#,
    );
    let after = analysis(
        r#"
module stable_binding_identity.
const UNRELATED: Int = 1.
run(input: Int): Int ->
    let output = input;
    output.
"#,
    );
    let ids = |analysis: &BindingAnalysis| {
        analysis
            .evidence
            .bindings
            .iter()
            .filter(|binding| binding.path.contains("function:run/1"))
            .map(|binding| (binding.name.clone(), binding.id))
            .collect::<Vec<_>>()
    };
    assert_eq!(ids(&before), ids(&after));
}

#[test]
fn same_name_and_arity_overloads_receive_distinct_binding_regions() {
    let analysis = analysis(
        r#"
module overloaded_binding_identity.

run(value: Int): Int -> value.
run(value: Float): Float -> value.
"#,
    );
    assert!(analysis.collisions.is_empty(), "{analysis:#?}");
    analysis
        .evidence
        .validate()
        .expect("overloaded declarations must have unique binding identities");
    let values = analysis
        .evidence
        .bindings
        .iter()
        .filter(|binding| binding.name == "value")
        .collect::<Vec<_>>();
    assert_eq!(values.len(), 2, "{values:#?}");
    assert_ne!(values[0].id, values[1].id);
    assert_ne!(values[0].region, values[1].region);
    assert!(values[1].path.contains("overload:1"), "{values:#?}");
}

#[test]
fn forged_or_stale_binding_evidence_fails_closed() {
    CoreBindingIdentityEvidence::default()
        .validate()
        .expect("empty resolved-only CoreIR binding evidence");
    let mut evidence = analysis(
        r#"
module forged_binding_identity.
run(value: Int): Int -> value.
"#,
    )
    .evidence;
    evidence.fingerprint = "forged".to_string();
    assert!(evidence
        .validate()
        .expect_err("forged evidence must fail")
        .context()
        .contains("stale fingerprint"));
}
