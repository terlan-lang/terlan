use super::*;

/// Writes a two-module project whose consumer imports one exported shape.
///
/// Inputs:
/// - `fixture_name`: temporary project identity.
/// - `shape_source`: complete provider module source.
/// - `consumer_source`: complete importing module source.
///
/// Output:
/// - Source root, output root, and consumer source path.
///
/// Transformation:
/// - Materializes module-layout-valid `app.Shapes` and `app.Classifier`
///   sources for real directory build coverage.
fn write_imported_shape_project(
    fixture_name: &str,
    shape_source: &str,
    consumer_source: &str,
) -> (PathBuf, PathBuf, PathBuf) {
    let dir = make_temp_dir(fixture_name);
    let source_dir = dir.join("src");
    let app_dir = source_dir.join("app");
    let out_dir = dir.join("build");
    let consumer_path = app_dir.join("Classifier.terl");
    fs::create_dir_all(&app_dir).expect("create JS shape source directory");
    fs::write(app_dir.join("Shapes.terl"), shape_source).expect("write exported JS shape provider");
    fs::write(&consumer_path, consumer_source).expect("write imported JS shape consumer");
    (source_dir, out_dir, consumer_path)
}

/// Runs a shared-JavaScript directory build for an imported-shape fixture.
///
/// Inputs:
/// - `source_dir`: module-layout-valid source root.
/// - `out_dir`: isolated build output root.
///
/// Output:
/// - Build command exit code.
///
/// Transformation:
/// - Exercises the public `terlc build --target js` command path with default
///   non-incremental validation.
fn run_imported_shape_js_build(source_dir: &Path, out_dir: &Path) -> ExitCode {
    run(
        CliCommand {
            verb: Some("build".to_string()),
            args: vec![
                source_dir.to_string_lossy().into_owned(),
                "--target".to_string(),
                "js".to_string(),
            ],
        },
        CliState {
            out_dir: out_dir.to_path_buf(),
            ..CliState::default()
        },
    )
}

/// Executes generated imported-shape JavaScript when Node is available.
///
/// Inputs:
/// - `js_path`: emitted ES module under test.
/// - `function_name`: exported function used by the assertions.
/// - `setup`: optional JavaScript fixture setup statements.
/// - `cases`: JavaScript array of `[actual, expected]` assertions.
///
/// Output:
/// - Successful assertions for the supplied cases, or a documented skip when
///   Node is unavailable.
///
/// Transformation:
/// - Imports the artifact through `pathToFileURL` and evaluates explicit
///   positive and adversarial cases without platform-specific file URLs.
fn assert_imported_shape_js_runtime(js_path: &Path, function_name: &str, setup: &str, cases: &str) {
    let script = r#"
import { pathToFileURL } from "node:url";
const { __TERLAN_FUNCTION__ } = await import(pathToFileURL(process.argv[1]).href);
__TERLAN_SETUP__
const cases = __TERLAN_CASES__;
if (cases.some(([actual, expected]) => actual !== expected)) {
    throw new Error(JSON.stringify(cases));
}
"#
    .replace("__TERLAN_FUNCTION__", function_name)
    .replace("__TERLAN_SETUP__", setup)
    .replace("__TERLAN_CASES__", cases);
    let output = match std::process::Command::new("node")
        .args(["--input-type=module", "--eval"])
        .arg(&script)
        .arg(js_path)
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => panic!("run generated imported-shape JavaScript: {error}"),
    };
    assert!(
        output.status.success(),
        "generated imported-shape JavaScript failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Proves imported shape aliases inherit the JavaScript backend capability of
/// their expanded ordinary pattern.
///
/// Inputs:
/// - A provider module exporting a zero-literal shape with one ignored call
///   slot, as required by constructor-pattern invocation syntax.
/// - A consumer selecting that shape under a local alias and using it in a
///   case expression.
///
/// Output:
/// - Successful directory build with a consumer ES module containing the
///   ordinary strict-equality test and no shape-level runtime symbol.
///
/// Transformation:
/// - Exercises interface generation, imported-shape expansion, CoreIR literal
///   pattern lowering, and Oxc-backed JavaScript emission in one build.
#[test]
fn build_command_emits_imported_literal_shape_for_js_target() {
    let (source_dir, out_dir, _) = write_imported_shape_project(
        "imported_shape_js",
        "module app.Shapes.\n\npub shape Zero(ignored) = 0.\n",
        r#"module app.Classifier.

import app.Shapes.{Zero as Empty}.

pub classify(value: Int): Int ->
    case value {
        Empty(_) -> 1;
        _ -> 2
    }.
"#,
    );

    let status = run_imported_shape_js_build(&source_dir, &out_dir);

    assert_eq!(status, ExitCode::SUCCESS);
    let js_path = out_dir.join("js/modules/app/Classifier.js");
    let js = fs::read_to_string(&js_path)
        .unwrap_or_else(|error| panic!("read {}: {error}", js_path.display()));
    assert!(js.contains("value === 0"), "emitted JS:\n{js}");
    assert!(!js.contains("Empty"), "shape alias leaked into JS:\n{js}");
    assert!(!js.contains("Zero"), "provider shape leaked into JS:\n{js}");
}

/// Proves imported tuple shapes inherit direct JavaScript structural matching.
///
/// Inputs:
/// - An exported tuple shape selected under a consumer alias.
/// - A consumer case branch that reads both destructured bindings.
///
/// Output:
/// - Successful build containing array-kind, exact-arity, and destructuring
///   branch logic without shape-level runtime symbols.
///
/// Transformation:
/// - Exercises imported-shape expansion followed by direct CoreIR-to-Oxc tuple
///   case lowering through the release build command.
#[test]
fn build_command_emits_imported_tuple_shape_for_js_target() {
    let (source_dir, out_dir, _) = write_imported_shape_project(
        "imported_tuple_shape_js",
        "module app.Shapes.\n\npub shape Pair(left, right) = {left, right}.\n",
        r#"module app.Classifier.

import app.Shapes.{Pair as Values}.

pub sum(value: {Int, Int}): Int ->
    case value {
        Values(left, right) -> left + right;
        _ -> 0
    }.
"#,
    );

    let status = run_imported_shape_js_build(&source_dir, &out_dir);

    assert_eq!(status, ExitCode::SUCCESS);
    let js_path = out_dir.join("js/modules/app/Classifier.js");
    let js = fs::read_to_string(&js_path)
        .unwrap_or_else(|error| panic!("read {}: {error}", js_path.display()));
    assert!(js.contains("Array.isArray(value)"), "emitted JS:\n{js}");
    assert!(js.contains("value.length === 2"), "emitted JS:\n{js}");
    assert!(js.contains("[left, right]"), "emitted JS:\n{js}");
    assert!(!js.contains("Values"), "shape alias leaked into JS:\n{js}");
    assert!(!js.contains("Pair"), "provider shape leaked into JS:\n{js}");
    assert_imported_shape_js_runtime(
        &js_path,
        "sum",
        "",
        r#"[
    [sum([2, 3]), 5],
    [sum([2]), 0],
    [sum([2, 3, 4]), 0],
    [sum("23"), 0],
]"#,
    );
}

/// Proves imported nested shapes preserve literal tests and nested bindings.
///
/// Inputs:
/// - An exported shape containing a literal tag and a nested tuple payload.
/// - A consumer branch that reads bindings from the nested payload.
///
/// Output:
/// - Successful build and Node execution for the valid shape, plus fallback
///   behavior for wrong tags, arities, and nested value kinds.
///
/// Transformation:
/// - Lowers the literal tag into a member equality test, validates both tuple
///   levels, then destructures only binding-bearing positions.
#[test]
fn build_command_emits_imported_nested_literal_shape_for_js_target() {
    let (source_dir, out_dir, _) = write_imported_shape_project(
        "imported_nested_literal_shape_js",
        "module app.Shapes.\n\npub shape TaggedPair(left, right) = {7, {left, right}}.\n",
        r#"module app.Classifier.

import app.Shapes.{TaggedPair as Values}.

pub sum(value: {Int, {Int, Int}}): Int ->
    case value {
        Values(left, right) -> left + right;
        _ -> 0
    }.
"#,
    );

    let status = run_imported_shape_js_build(&source_dir, &out_dir);

    assert_eq!(status, ExitCode::SUCCESS);
    let js_path = out_dir.join("js/modules/app/Classifier.js");
    let js = fs::read_to_string(&js_path)
        .unwrap_or_else(|error| panic!("read {}: {error}", js_path.display()));
    assert!(js.contains("Array.isArray(value)"), "emitted JS:\n{js}");
    assert!(js.contains("value[0] === 7"), "emitted JS:\n{js}");
    assert!(js.contains("Array.isArray(value[1])"), "emitted JS:\n{js}");
    assert!(js.contains("value[1].length === 2"), "emitted JS:\n{js}");
    assert!(!js.contains("Values"), "shape alias leaked into JS:\n{js}");
    assert!(!js.contains("TaggedPair"), "shape leaked into JS:\n{js}");
    assert_imported_shape_js_runtime(
        &js_path,
        "sum",
        "",
        r#"[
    [sum([7, [2, 3]]), 5],
    [sum([8, [2, 3]]), 0],
    [sum([7, [2]]), 0],
    [sum([7, [2, 3, 4]]), 0],
    [sum([7, "23"]), 0],
    [sum([7, [2, 3], 4]), 0],
]"#,
    );
}

/// Proves imported map shapes use own-field structural matching in JavaScript.
///
/// Inputs:
/// - An exported map shape with a literal tag and one payload binding.
/// - A consumer branch returning the bound payload.
///
/// Output:
/// - Successful build and Node execution for valid maps with or without extra
///   fields, plus fallback behavior for invalid object shapes.
///
/// Transformation:
/// - Validates object kind, required own fields, and the literal tag before a
///   binding-only object destructuring closure executes the branch.
#[test]
fn build_command_emits_imported_map_shape_for_js_target() {
    let (source_dir, out_dir, _) = write_imported_shape_project(
        "imported_map_shape_js",
        "module app.Shapes.\n\npub shape OkValue(item) = {kind: Atom[\"ok\"], value: item}.\n",
        r#"module app.Classifier.

import app.Shapes.{OkValue as Success}.

pub unwrap(value: {kind: Atom["ok"], value: Int}): Int ->
    case value {
        Success(item) -> item;
        _ -> 0
    }.
"#,
    );

    let status = run_imported_shape_js_build(&source_dir, &out_dir);

    assert_eq!(status, ExitCode::SUCCESS);
    let js_path = out_dir.join("js/modules/app/Classifier.js");
    let js = fs::read_to_string(&js_path)
        .unwrap_or_else(|error| panic!("read {}: {error}", js_path.display()));
    assert!(
        js.contains("typeof value === \"object\""),
        "emitted JS:\n{js}"
    );
    assert!(js.contains("value !== null"), "emitted JS:\n{js}");
    assert!(js.contains("!Array.isArray(value)"), "emitted JS:\n{js}");
    assert!(
        js.contains("Object.prototype.hasOwnProperty.call"),
        "emitted JS:\n{js}"
    );
    assert!(
        js.contains("value[\"kind\"] === \"ok\""),
        "emitted JS:\n{js}"
    );
    assert!(!js.contains("Success"), "shape alias leaked into JS:\n{js}");
    assert!(!js.contains("OkValue"), "shape leaked into JS:\n{js}");

    assert_imported_shape_js_runtime(
        &js_path,
        "unwrap",
        r#"const inherited = Object.assign(Object.create({kind: "ok"}), {value: 4});
const arrayValue = [];
arrayValue.kind = "ok";
arrayValue.value = 4;"#,
        r#"[
    [unwrap({kind: "ok", value: 4}), 4],
    [unwrap({kind: "ok", value: 4, extra: true}), 4],
    [unwrap({kind: "error", value: 4}), 0],
    [unwrap({value: 4}), 0],
    [unwrap({kind: "ok"}), 0],
    [unwrap(null), 0],
    [unwrap(arrayValue), 0],
    [unwrap(inherited), 0],
]"#,
    );
}

/// Proves imported record shapes use the same safe object matching as records.
#[test]
fn build_command_emits_imported_record_shape_for_js_target() {
    let (source_dir, out_dir, _) = write_imported_shape_project(
        "imported_record_shape_js",
        r#"module app.Shapes.

pub struct User {
    name: String,
    level: Int
}.

pub shape ActiveUser(name) = User{name: name, level: 7}.
"#,
        r#"module app.Classifier.

import app.Shapes.{ActiveUser as Active}.

pub name(value: Dynamic): String ->
    case value {
        Active(name) -> name;
        _ -> "inactive"
    }.
"#,
    );

    let status = run_imported_shape_js_build(&source_dir, &out_dir);

    assert_eq!(status, ExitCode::SUCCESS);
    let js_path = out_dir.join("js/modules/app/Classifier.js");
    let js = fs::read_to_string(&js_path)
        .unwrap_or_else(|error| panic!("read {}: {error}", js_path.display()));
    assert!(
        js.contains("typeof value === \"object\""),
        "emitted JS:\n{js}"
    );
    assert!(js.contains("value !== null"), "emitted JS:\n{js}");
    assert!(js.contains("!Array.isArray(value)"), "emitted JS:\n{js}");
    assert!(
        js.contains("Object.prototype.hasOwnProperty.call"),
        "emitted JS:\n{js}"
    );
    assert!(js.contains("value[\"level\"] === 7"), "emitted JS:\n{js}");
    assert!(!js.contains("ActiveUser"), "shape leaked into JS:\n{js}");

    assert_imported_shape_js_runtime(
        &js_path,
        "name",
        r#"const inherited = Object.assign(Object.create({level: 7}), {name: "Ada"});
const arrayValue = [];
arrayValue.name = "Ada";
arrayValue.level = 7;"#,
        r#"[
    [name({name: "Ada", level: 7}), "Ada"],
    [name({name: "Ada", level: 7, role: "admin"}), "Ada"],
    [name({name: "Ada", level: 6}), "inactive"],
    [name({name: "Ada"}), "inactive"],
    [name({level: 7}), "inactive"],
    [name(null), "inactive"],
    [name(arrayValue), "inactive"],
    [name(inherited), "inactive"],
]"#,
    );
}

/// Proves imported constructor shapes match exact VM tagged tuples in JS.
#[test]
fn build_command_emits_imported_constructor_shape_for_js_target() {
    let (source_dir, out_dir, _) = write_imported_shape_project(
        "imported_constructor_shape_js",
        r#"module app.Shapes.

pub constructor Box {
    (value: Int, marker: Int): Dynamic -> {box, value, marker}
}.

pub constructor Envelope {
    (value: Dynamic): Dynamic -> {envelope, value}
}.

pub shape MarkedBox(value) = Envelope(Box(value, 7)).
"#,
        r#"module app.Classifier.

import app.Shapes.{Box, Envelope, MarkedBox as Marked}.

pub unwrap(value: Dynamic): Int ->
    case value {
        Marked(payload) -> payload;
        _ -> 0
    }.
"#,
    );

    let status = run_imported_shape_js_build(&source_dir, &out_dir);

    assert_eq!(status, ExitCode::SUCCESS);
    let js_path = out_dir.join("js/modules/app/Classifier.js");
    let js = fs::read_to_string(&js_path)
        .unwrap_or_else(|error| panic!("read {}: {error}", js_path.display()));
    assert!(js.contains("Array.isArray(value)"), "emitted JS:\n{js}");
    assert!(js.contains("value.length === 2"), "emitted JS:\n{js}");
    assert!(
        js.contains("value[0] === \"envelope\""),
        "emitted JS:\n{js}"
    );
    assert!(js.contains("value[1].length === 3"), "emitted JS:\n{js}");
    assert!(js.contains("value[1][0] === \"box\""), "emitted JS:\n{js}");
    assert!(js.contains("value[1][2] === 7"), "emitted JS:\n{js}");
    assert!(!js.contains("MarkedBox"), "shape leaked into JS:\n{js}");
    assert!(!js.contains("Marked("), "shape alias leaked into JS:\n{js}");

    assert_imported_shape_js_runtime(
        &js_path,
        "unwrap",
        "",
        r#"[
    [unwrap(["envelope", ["box", 4, 7]]), 4],
    [unwrap(["envelope", ["box", -3, 7]]), -3],
    [unwrap(["other", ["box", 4, 7]]), 0],
    [unwrap(["envelope", ["other", 4, 7]]), 0],
    [unwrap(["envelope", ["box", 4, 6]]), 0],
    [unwrap(["envelope", ["box", 4]]), 0],
    [unwrap(["envelope", ["box", 4, 7, 8]]), 0],
    [unwrap(["envelope", {0: "box", 1: 4, 2: 7, length: 3}]), 0],
    [unwrap(["envelope"]), 0],
    [unwrap(null), 0],
]"#,
    );
}

/// Proves imported zero-arity constructor shapes match canonical atoms in JS.
#[test]
fn build_command_emits_imported_zero_arity_constructor_shape_for_js_target() {
    let (source_dir, out_dir, _) = write_imported_shape_project(
        "imported_zero_arity_constructor_shape_js",
        r#"module app.Shapes.

pub constructor Ready {
    (): Dynamic -> Atom["ready"]
}.

pub shape ReadyValue() = Ready().
"#,
        r#"module app.Classifier.

import app.Shapes.{Ready, ReadyValue as IsReady}.

pub is_ready(value: Dynamic): Bool ->
    case value {
        IsReady() -> true;
        _ -> false
    }.
"#,
    );

    let status = run_imported_shape_js_build(&source_dir, &out_dir);

    assert_eq!(status, ExitCode::SUCCESS);
    let js_path = out_dir.join("js/modules/app/Classifier.js");
    let js = fs::read_to_string(&js_path)
        .unwrap_or_else(|error| panic!("read {}: {error}", js_path.display()));
    assert!(js.contains("value === \"ready\""), "emitted JS:\n{js}");
    assert!(!js.contains("ReadyValue"), "shape leaked into JS:\n{js}");
    assert!(
        !js.contains("IsReady("),
        "shape alias leaked into JS:\n{js}"
    );

    assert_imported_shape_js_runtime(
        &js_path,
        "is_ready",
        "",
        r#"[
    [is_ready("ready"), true],
    [is_ready("Ready"), false],
    [is_ready("other"), false],
    [is_ready([]), false],
    [is_ready(null), false],
]"#,
    );
}

/// Proves an imported guarded shape executes through the JavaScript backend.
///
/// Inputs:
/// - An exported guarded tuple shape selected under a consumer alias.
/// - A consumer case expression requiring guarded tuple destructuring.
///
/// Output:
/// - Successful build and Node execution for accepted, guard-rejected, and
///   structurally invalid values.
///
/// Transformation:
/// - Short-circuits the structural tuple test before evaluating the guard in a
///   destructuring closure, preserving fallback behavior without a
///   shape-specific runtime representation.
#[test]
fn build_command_emits_guarded_imported_shape_for_js_target() {
    let (source_dir, out_dir, _) = write_imported_shape_project(
        "guarded_imported_shape_js",
        "module app.Shapes.\n\npub shape PositivePair(left, right) =\n    {left, right} where left > 0.\n",
        r#"module app.Classifier.

import app.Shapes.{PositivePair as Values}.

pub sum(value: {Int, Int}): Int ->
    case value {
        Values(left, right) -> left + right;
        _ -> 0
    }.
"#,
    );

    let status = run_imported_shape_js_build(&source_dir, &out_dir);

    assert_eq!(status, ExitCode::SUCCESS);
    let js_path = out_dir.join("js/modules/app/Classifier.js");
    let js = fs::read_to_string(&js_path)
        .unwrap_or_else(|error| panic!("read {}: {error}", js_path.display()));
    assert!(js.contains("Array.isArray(value)"), "emitted JS:\n{js}");
    assert!(js.contains("left > 0"), "emitted JS:\n{js}");
    assert!(!js.contains("Values"), "shape alias leaked into JS:\n{js}");
    assert!(!js.contains("PositivePair"), "shape leaked into JS:\n{js}");
    assert_imported_shape_js_runtime(
        &js_path,
        "sum",
        "",
        r#"[
    [sum([1, 2]), 3],
    [sum([0, 2]), 0],
    [sum([-1, 2]), 0],
    [sum([1]), 0],
    [sum([1, 2, 3]), 0],
    [sum("not a pair"), 0],
]"#,
    );
}

/// Proves structural implication evidence is erased before JavaScript emission
/// while its field access remains executable.
#[test]
fn build_command_executes_structural_implication_for_js_target() {
    let (source_dir, out_dir, _) = write_imported_shape_project(
        "implication_js",
        "module app.Shapes.\n",
        r#"module app.Classifier.

pub struct User { name: String }.

pub display_name[T => {name: String}](value: T): String ->
    value.name.

pub name(value: User): String -> display_name(value).
"#,
    );

    let status = run_imported_shape_js_build(&source_dir, &out_dir);

    assert_eq!(status, ExitCode::SUCCESS);
    let js_path = out_dir.join("js/modules/app/Classifier.js");
    let js = fs::read_to_string(&js_path)
        .unwrap_or_else(|error| panic!("read {}: {error}", js_path.display()));
    assert!(js.contains("display_name(value)"), "emitted JS:\n{js}");
    assert!(
        !js.contains("=> {name:"),
        "type evidence leaked into JS:\n{js}"
    );
    assert_imported_shape_js_runtime(
        &js_path,
        "name",
        "",
        r#"[
    [name({name: "Ada"}), "Ada"],
    [name({name: "Grace"}), "Grace"],
]"#,
    );
}
