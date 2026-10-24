//! Host bindings are checked through the real Terlan frontend and JS backend.

use super::*;

#[test]
fn real_array_program_compiles_with_explicit_type_arguments_and_call_scrutinee() {
    let source = r#"module tests.js.ArrayCompilation.
import std.js.Array.
pub check(): Bool ->
    let values = Array.from_list[Int]([10, 20, 30]);
    values.length() == 3 and values.get_at(1) == 20
        and case values.to_list() { [10, 20, 30] -> true; _ -> false }.
"#;
    let javascript = compiled(source);
    execute(
        &javascript,
        "if (check() !== true) throw new Error('array conversion failed');",
    );
}

#[test]
fn case_call_scrutinee_runs_once_with_lexical_state_preserved() {
    let source = r#"module tests.js.CaseCompilation.
pub check(read: (Int) -> Int, expected: Int): Bool ->
    case read(1) { 1 -> expected == 17; _ -> false }.
"#;
    let javascript = compiled(source);
    execute(&javascript, "let reads = 0; if (!check(value => { reads++; return value; }, 17) || reads !== 1) throw new Error('case scrutinee repeated'); if (check(() => 2, 17) !== false) throw new Error('case fallback failed');");
}

#[test]
fn javascript_reserved_parameter_names_preserve_their_values() {
    let source = r#"module tests.js.ReservedBindings.
pub check(default: Int, arguments: Int): Int ->
    let eval = default + arguments;
    case eval { 42 -> eval; _ -> default }.
"#;
    execute(&compiled(source), "if (check(17, 25) !== 42 || check(3, 4) !== 3) throw new Error('reserved binding was lost');");
}

#[test]
fn foreign_operations_reject_unknown_names_and_incorrect_arities() {
    assert!(adapter("std.js.String.to_core", 2)
        .unwrap_err()
        .to_string()
        .contains("expects 1 arguments"));
    assert!(adapter("std.js.Dom.Element.missing", 1)
        .unwrap_err()
        .to_string()
        .contains("unsupported host operation"));
}

#[test]
fn recursive_list_patterns_and_integer_text_execute_in_javascript() {
    let source = r#"module tests.js.ListPatterns.
import std.core.Int.{to_string}.
pub count(values: List[Int]): Int ->
    case values { [] -> 0; [_head | tail] -> 1 + count(tail) }.
pub nested(values: List[Int]): Int ->
    case values { [first | [second | _rest]] -> first + second; _ -> 0 }.
pub text(value: Int): String -> to_string(value) + "px".
"#;
    execute(&compiled(source), "if (count([4, 5, 6]) !== 3 || count([]) !== 0 || nested([7, 8, 9]) !== 15 || nested([7]) !== 0 || text(120) !== '120px') throw new Error('Terlan list or integer semantics changed'); let rejected = false; try { count(null); } catch { rejected = true; } if (!rejected) throw new Error('unmatched case was accepted');");
}

#[test]
fn float_rounding_preserves_pointer_offsets_in_javascript() {
    let source = r#"module tests.js.FloatRounding.
import std.core.Float.{floor, ceil}.
pub down(value: Float): Float -> floor(value).
pub up(value: Float): Float -> ceil(value).
pub shifted(pointer: Float, grab_offset: Float): Float -> floor(pointer - grab_offset).
"#;
    execute(&compiled(source), "if (down(15.8) !== 15 || down(-0.2) !== -1 || up(15.2) !== 16 || up(-1.8) !== -1 || shifted(29.8, 30) !== -1) throw new Error('float rounding changed');");
}

#[test]
fn optional_generic_host_values_distinguish_null_from_missing() {
    for kind in ["property", "method"] {
        let member = serde_json::from_value(serde_json::json!({
            "name": "view", "js_name": "view", "kind": kind, "arity": 1,
            "return_type": "Option[Value]", "undefined_only": true,
        }))
        .unwrap();
        let expression = member_expression(&member).unwrap();
        let value = if kind == "method" {
            "() => value"
        } else {
            "value"
        };
        execute("", &format!("for (const value of [null, undefined, {{id: 7}}]) {{ const a0 = {{view: {value}}}; const result = {expression}; if (value === undefined ? result !== 'none' : !Array.isArray(result) || result[0] !== 'some' || result[1] !== value) throw new Error('optional generic value changed'); }}"));
    }
}

#[test]
fn declaration_callback_adapters_preserve_object_identity_and_javascript_names() {
    let member = serde_json::from_value(serde_json::json!({
        "name": "with_members", "js_name": "", "kind": "mixin", "arity": 3,
        "return_type": "BindingTarget", "js_names": ["$onInit", "$onDestroy"],
        "parameters": [{"optional": false, "type": "() -> Unit"}, {"optional": false, "type": "() -> Unit"}],
    })).unwrap();
    let expression = member_expression(&member).unwrap();
    execute("", &format!("const calls = []; const a0 = {{state: 42}}; const a1 = function() {{ if (this !== a0) throw new Error('callback receiver changed'); calls.push('init'); }}; const a2 = () => calls.push('destroy'); const result = {expression}; result.$onInit(); result.$onDestroy(); if (result !== a0 || result.state !== 42 || calls.join(',') !== 'init,destroy') throw new Error('lifecycle adapter lost state or callbacks');"));
}

#[test]
fn primitive_unit_values_cross_callbacks_in_javascript() {
    let source = r#"module tests.js.UnitValues.
pub completed(): Unit -> Unit.
pub check(effect: () -> Unit): Bool ->
    let value = effect();
    value == Unit.
"#;
    execute(&compiled(source), "let calls = 0; if (completed() !== 'unit' || !check(() => { calls++; return completed(); }) || calls !== 1) throw new Error('Unit did not retain its singleton value');");
}

#[test]
fn optional_values_construct_through_the_checked_frontend() {
    let source = r#"module tests.js.OptionalConstruction.
import std.core.Option.{Option, Some}.
pub boxed(value: Bool): Option[Bool] -> Some(value).
"#;
    execute(&compiled(source), "const result = boxed(true); if (!Array.isArray(result) || result[0] !== 'some' || result[1] !== true) throw new Error('present optional value changed');");
}

fn compiled(source: &str) -> String {
    let artifacts = crate::formal_pipeline::compile_syntax_module_through_phases_with_profile(
        "tests/js/Compilation.terl",
        source,
        crate::DiagnosticFormat::default(),
        None,
        crate::validation::native_policy::NativePolicy::default(),
        crate::validation::target_profile::TargetProfile::JsBrowser,
    )
    .expect("compile the Terlan application");
    crate::commands::emit_js::emit_core_module_with_template_runtime(
        &artifacts.core,
        &artifacts.syntax_output,
        std::path::Path::new("tests/js/Compilation.terl"),
        crate::validation::target_profile::TargetProfile::JsBrowser,
        None,
    )
    .expect("emit checked JavaScript")
    .unwrap_or_else(|| panic!("unsupported application CoreIR: {:#?}", artifacts.core))
}

fn execute(javascript: &str, assertion: &str) {
    let result = std::process::Command::new("node")
        .args([
            "--input-type=module",
            "--eval",
            &format!("{javascript}\n{assertion}"),
        ])
        .output()
        .expect("execute emitted JavaScript");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn record_construction_preserves_host_keys_and_nullable_payload_identity() {
    let member = serde_json::from_value(serde_json::json!({
        "name": "construct", "js_name": "type", "kind": "record", "arity": 4,
        "return_type": "Host[Value]", "js_names": ["type", "WAITING", "view", "__proto__"],
        "parameters": [
            {"optional": false, "type": "String"},
            {"optional": false, "type": "Bool"},
            {"optional": false, "type": "Option[Value]", "undefined_only": true},
            {"optional": false, "type": "Value"},
        ],
    }))
    .unwrap();
    let expression = member_expression(&member).unwrap();
    execute("", &format!("const a0 = 'move'; const a1 = true; const a3 = {{marker: 42}}; for (const payload of [null, undefined, {{id: 7}}]) {{ const a2 = payload === undefined ? 'none' : ['some', payload]; const result = {expression}; if (result.type !== 'move' || result.WAITING !== true || result.view !== payload || result.__proto__ !== a3 || Object.getPrototypeOf(result) !== Object.prototype || !Object.hasOwn(result, '__proto__')) throw new Error('record construction changed host data'); }}"));
}

#[test]
fn property_setters_preserve_proxy_receivers_and_optional_null_semantics() {
    for (undefined_only, missing) in [(true, "undefined"), (false, "null")] {
        let member = serde_json::from_value(serde_json::json!({
            "name": "set_view", "js_name": "view", "kind": "setter", "arity": 2,
            "return_type": "Unit", "js_names": ["view"],
            "parameters": [{"optional": false, "type": "Option[Value]", "undefined_only": undefined_only}],
        })).unwrap();
        let expression = member_expression(&member).unwrap();
        execute("", &format!("for (const value of [null, {{id: 7}}]) {{ let writes = 0; let received; let receiver; const a0 = new Proxy({{view: 'previous'}}, {{set(target, key, value, proxy) {{writes++; received = value; receiver = proxy; return Reflect.set(target, key, value, proxy);}}}}); let a1 = ['some', value]; if ({expression} !== 'unit' || writes !== 1 || receiver !== a0 || received !== value || a0.view !== value) throw new Error('setter lost identity or repeated a write'); a1 = 'none'; if ({expression} !== 'unit' || writes !== 2 || a0.view !== {missing}) throw new Error('missing field changed semantics'); }}"));
    }
}
