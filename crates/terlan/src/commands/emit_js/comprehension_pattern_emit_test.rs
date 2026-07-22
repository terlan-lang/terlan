use super::*;

use crate::formal_pipeline::compile_syntax_module_through_phases_with_profile;
use crate::validation::native_policy::NativePolicy;
use crate::validation::target_profile::TargetProfile;
use crate::DiagnosticFormat;

/// Proves JavaScript comprehension patterns filter malformed boundary values.
#[test]
fn direct_oxc_comprehension_filters_nonmatching_generator_values() {
    let source = "\
module js_comprehension_pattern_filter.

pub firsts(items: List[{Int, Int}]): List[Int] ->
    [left | {left, _right} <- items].
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_comprehension_pattern_filter.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    )
    .expect("compile comprehension pattern filter to CoreIR");
    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("emit comprehension pattern filter through direct Oxc AST");

    assert!(
        js.contains("Array.isArray(__terlan_comprehension_candidate)"),
        "{js}"
    );
    assert!(
        js.contains("__terlan_comprehension_candidate.length === 2"),
        "{js}"
    );

    let script = format!(
        "{js}\nconst actual = firsts([[1, 2], [3], [4, 5, 6], null, 'bad', [7, 8]]);\n\
         if (JSON.stringify(actual) !== '[1,7]') throw new Error(JSON.stringify(actual));"
    );
    let output = match std::process::Command::new("node")
        .args(["--input-type=module", "--eval", &script])
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => panic!("run generated comprehension JavaScript: {error}"),
    };
    assert!(
        output.status.success(),
        "generated comprehension JavaScript failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Proves object, record, and constructor generators retain exact JS matching.
#[test]
fn direct_oxc_comprehension_filters_structural_generator_values() {
    let source = r#"
module js_comprehension_structural_pattern_filter.

import std.core.Option.{Option, Some}.

import type std.core.Option.

pub struct User {
    name: String
}.

pub map_names(items: List[{name: String}]): List[String] ->
    [name | {name: name} <- items].

pub record_names(items: List[User]): List[String] ->
    [name | User {name: name} <- items].

pub option_values(items: List[Option[Int]]): List[Int] ->
    [value | Some(value) <- items].
"#;
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_comprehension_structural_pattern_filter.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    )
    .expect("compile structural comprehension pattern filters to CoreIR");
    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("emit structural comprehension pattern filters through direct Oxc AST");

    assert!(js.contains("Object.prototype.hasOwnProperty.call"), "{js}");
    assert!(
        js.contains("__terlan_comprehension_candidate[0] === \"some\""),
        "{js}"
    );

    let script = format!(
        "{js}\n\
         const inherited = Object.create({{name: 'inherited'}});\n\
         const mapActual = map_names([{{name: 'Ada'}}, inherited, null, [], {{other: 'skip'}}]);\n\
         const recordActual = record_names([{{name: 'Grace'}}, inherited, null, 'bad']);\n\
         const optionActual = option_values([['some', 1], ['none'], ['some'], ['some', 2, 3], null, ['some', 4]]);\n\
         if (JSON.stringify(mapActual) !== '[\"Ada\"]') throw new Error('map:' + JSON.stringify(mapActual));\n\
         if (JSON.stringify(recordActual) !== '[\"Grace\"]') throw new Error('record:' + JSON.stringify(recordActual));\n\
         if (JSON.stringify(optionActual) !== '[1,4]') throw new Error('option:' + JSON.stringify(optionActual));"
    );
    let output = match std::process::Command::new("node")
        .args(["--input-type=module", "--eval", &script])
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => panic!("run generated structural comprehension JavaScript: {error}"),
    };
    assert!(
        output.status.success(),
        "generated structural comprehension JavaScript failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
