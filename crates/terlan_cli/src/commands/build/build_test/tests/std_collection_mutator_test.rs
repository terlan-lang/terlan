use super::*;

/// Verifies loop-shaped binary search can be expressed with tail recursion.
///
/// Inputs:
/// - A manifest-backed project importing `std.native.collections.Vector`.
/// - Source that mirrors an imperative `for min < max` binary-search loop
///   with recursive `min` and `max` accumulator parameters.
///
/// Output:
/// - Test passes when the generated executable prints the found index and `-1`
///   for a missing value.
///
/// Transformation:
/// - Proves algorithm-book indexed collection code can be represented without
///   a dedicated `for` loop while preserving vector length, indexed reads,
///   integer midpoint calculation, and tail-call loop state.
#[test]
fn build_command_compiles_native_vector_binary_search_loop_shape() {
    let dir = make_temp_dir("directory_project_native_vector_binary_search_loop_shape");
    let project_dir = dir.join("project");
    let app_dir = project_dir.join("src/app");
    let out_dir = dir.join("build");
    fs::create_dir_all(&app_dir).expect("failed to create project src dir");
    fs::write(
        project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
        "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n",
    )
    .expect("failed to write project manifest fixture");
    fs::write(
        app_dir.join("Main.terl"),
        "\
module app.Main.\n\
\n\
import std.io.Console.{println}.\n\
import std.core.Int.\n\
import std.native.collections.Vector.\n\
\n\
pub main(): Unit ->\n\
let values = Vector(1, 3, 5, 7, 9, 11, 13);\n\
    found = binary_search(values, 7);\n\
    missing = binary_search(values, 4);\n\
println(Int.to_string(found));\n\
println(Int.to_string(missing)).\n\
\n\
binary_search(values: Vector[Int], goal: Int): Int ->\n\
    binary_search_loop(values, goal, 0, values.len()).\n\
\n\
binary_search_loop(values: Vector[Int], goal: Int, min: Int, max: Int): Int ->\n\
    if {\n\
        min < max -> binary_search_step(values, goal, min, max);\n\
        _ -> -1\n\
    }.\n\
\n\
binary_search_step(values: Vector[Int], goal: Int, min: Int, max: Int): Int ->\n\
    let mid = (max + min) div 2;\n\
        value = values[mid];\n\
    if {\n\
        value == goal -> mid;\n\
        value < goal -> binary_search_loop(values, goal, mid + 1, max);\n\
        _ -> binary_search_loop(values, goal, min, mid)\n\
    }.\n\
",
    )
    .expect("failed to write native vector loop-shaped binary search module");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            project_dir.display().to_string(),
            "--target".to_string(),
            "erlang".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    let erl_text =
        fs::read_to_string(out_dir.join("src/app_main.erl")).expect("read generated app_main");
    assert!(
        erl_text.contains("std_native_collections_vector_safe_native:length"),
        "loop-shaped binary search should lower vector length through the native bridge:\n{}",
        erl_text
    );
    assert!(
        erl_text.contains("(Max + Min) div 2"),
        "loop-shaped binary search should preserve integer midpoint grouping:\n{}",
        erl_text
    );

    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run loop-shaped native vector binary search launcher");
    assert!(
        launcher_output.status.success(),
        "launcher failed: stdout={} stderr={}",
        String::from_utf8_lossy(&launcher_output.stdout),
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "3\n-1\n");
}

/// Verifies compiler-owned map intrinsics execute with mutable receivers.
///
/// Inputs:
/// - A manifest-backed project declaring a local opaque map surface with
///   `@compiler.intrinsic` annotations for `new`, `put`, and `size`.
/// - A command-style mutable receiver method sequence that inserts one
///   value and then observes the updated receiver.
///
/// Output:
/// - Test passes when `terlc build` emits runnable BEAM artifacts and the
///   launcher prints `1`.
///
/// Transformation:
/// - Proves the selected P0.3b map intrinsic contract can drive BEAM map
///   construction, command-style receiver mutation, compiler-owned
///   rebinding, and observer calls without exposing the eventual release
///   `std.collections.Map` module.
#[test]
fn build_command_compiles_map_intrinsic_command_style_mutator_sequence() {
    let dir = make_temp_dir("directory_project_map_intrinsic_mutator_sequence");
    let project_dir = dir.join("project");
    let app_dir = project_dir.join("src/app");
    let out_dir = dir.join("build");
    fs::create_dir_all(&app_dir).expect("failed to create project src dir");
    fs::write(
        project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
        "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n",
    )
    .expect("failed to write project manifest fixture");
    fs::write(
        app_dir.join("Main.terl"),
        "\
module app.Main.\n\
\n\
import std.io.Console.{println}.\n\
import std.core.Int.\n\
\n\
pub struct Map {\n\
dummy: Int\n\
}.\n\
\n\
@compiler.intrinsic {core.map.new}\n\
pub new(): Map ->\n\
Map(dummy = 0).\n\
\n\
@compiler.intrinsic {core.map.put}\n\
pub (mut map: Map) put(key: String, value: String): Unit ->\n\
map.\n\
\n\
@compiler.intrinsic {core.map.size}\n\
pub (map: Map) size(): Int ->\n\
0.\n\
\n\
run(map: Map): Int ->\n\
map.put(\"key\", \"value\");\n\
map.size().\n\
\n\
pub main(): Unit ->\n\
println(Int.to_string(run(new()))).\n",
    )
    .expect("failed to write map intrinsic module");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            project_dir.display().to_string(),
            "--target".to_string(),
            "erlang".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    let erl_text =
        fs::read_to_string(out_dir.join("src/app_main.erl")).expect("read generated erl");
    assert!(
        erl_text.contains("maps:put"),
        "map put should lower through BEAM map intrinsic:\n{}",
        erl_text
    );
    assert!(
        erl_text.contains("_TerlanMutReceiver0 = maps:put"),
        "map command mutator should use receiver rebinding:\n{}",
        erl_text
    );

    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run map intrinsic launcher");
    assert!(
        launcher_output.status.success(),
        "launcher failed: stdout={} stderr={}",
        String::from_utf8_lossy(&launcher_output.stdout),
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "1\n");
}

/// Verifies compiler-owned list intrinsics execute with mutable receivers.
///
/// Inputs:
/// - A manifest-backed project declaring a local nominal list surface with
///   `@compiler.intrinsic` annotations for `new`, `push`, and `length`.
/// - A command-style mutable receiver method sequence that appends one
///   value and then observes the updated receiver.
///
/// Output:
/// - Test passes when `terlc build` emits runnable BEAM artifacts and the
///   launcher prints `1`.
///
/// Transformation:
/// - Proves the selected P0.3b list intrinsic contract can drive BEAM list
///   construction, command-style receiver mutation, compiler-owned
///   rebinding, and observer calls without exposing the eventual release
///   `std.collections.List` module.
#[test]
fn build_command_compiles_list_intrinsic_command_style_mutator_sequence() {
    let dir = make_temp_dir("directory_project_list_intrinsic_mutator_sequence");
    let project_dir = dir.join("project");
    let app_dir = project_dir.join("src/app");
    let out_dir = dir.join("build");
    fs::create_dir_all(&app_dir).expect("failed to create project src dir");
    fs::write(
        project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
        "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n",
    )
    .expect("failed to write project manifest fixture");
    fs::write(
        app_dir.join("Main.terl"),
        "\
module app.Main.\n\
\n\
import std.io.Console.{println}.\n\
import std.core.Int.\n\
\n\
pub struct List {\n\
dummy: Int\n\
}.\n\
\n\
@compiler.intrinsic {core.list.new}\n\
pub new(): List ->\n\
List(dummy = 0).\n\
\n\
@compiler.intrinsic {core.list.push}\n\
pub (mut list: List) push(value: String): Unit ->\n\
list.\n\
\n\
@compiler.intrinsic {core.list.length}\n\
pub (list: List) length(): Int ->\n\
0.\n\
\n\
run(list: List): Int ->\n\
list.push(\"value\");\n\
list.length().\n\
\n\
pub main(): Unit ->\n\
println(Int.to_string(run(new()))).\n",
    )
    .expect("failed to write list intrinsic module");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            project_dir.display().to_string(),
            "--target".to_string(),
            "erlang".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    let erl_text =
        fs::read_to_string(out_dir.join("src/app_main.erl")).expect("read generated erl");
    assert!(
        erl_text.contains("lists:append"),
        "list push should lower through BEAM list intrinsic:\n{}",
        erl_text
    );
    assert!(
        erl_text.contains("_TerlanMutReceiver0 = lists:append"),
        "list command mutator should use receiver rebinding:\n{}",
        erl_text
    );

    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run list intrinsic launcher");
    assert!(
        launcher_output.status.success(),
        "launcher failed: stdout={} stderr={}",
        String::from_utf8_lossy(&launcher_output.stdout),
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "1\n");
}

/// Verifies compiler-owned set intrinsics execute with mutable receivers.
///
/// Inputs:
/// - A manifest-backed project declaring a local nominal set surface with
///   `@compiler.intrinsic` annotations for `new`, `add`, and `size`.
/// - A command-style mutable receiver method sequence that adds one value
///   and then observes the updated receiver.
///
/// Output:
/// - Test passes when `terlc build` emits runnable BEAM artifacts and the
///   launcher prints `1`.
///
/// Transformation:
/// - Proves the selected P0.3b set intrinsic contract can drive the
///   compiler-owned BEAM set backing shape, command-style receiver
///   mutation, compiler-owned rebinding, and observer calls without
///   exposing the eventual release `std.collections.Set` module.
#[test]
fn build_command_compiles_set_intrinsic_command_style_mutator_sequence() {
    let dir = make_temp_dir("directory_project_set_intrinsic_mutator_sequence");
    let project_dir = dir.join("project");
    let app_dir = project_dir.join("src/app");
    let out_dir = dir.join("build");
    fs::create_dir_all(&app_dir).expect("failed to create project src dir");
    fs::write(
        project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
        "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n",
    )
    .expect("failed to write project manifest fixture");
    fs::write(
        app_dir.join("Main.terl"),
        "\
module app.Main.\n\
\n\
import std.io.Console.{println}.\n\
import std.core.Int.\n\
\n\
pub struct Set {\n\
dummy: Int\n\
}.\n\
\n\
@compiler.intrinsic {core.set.new}\n\
pub new(): Set ->\n\
Set(dummy = 0).\n\
\n\
@compiler.intrinsic {core.set.add}\n\
pub (mut set: Set) add(value: String): Unit ->\n\
set.\n\
\n\
@compiler.intrinsic {core.set.size}\n\
pub (set: Set) size(): Int ->\n\
0.\n\
\n\
run(set: Set): Int ->\n\
set.add(\"value\");\n\
set.size().\n\
\n\
pub main(): Unit ->\n\
println(Int.to_string(run(new()))).\n",
    )
    .expect("failed to write set intrinsic module");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            project_dir.display().to_string(),
            "--target".to_string(),
            "erlang".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    let erl_text =
        fs::read_to_string(out_dir.join("src/app_main.erl")).expect("read generated erl");
    assert!(
        erl_text.contains("maps:put"),
        "set add should lower through BEAM set backing intrinsic:\n{}",
        erl_text
    );
    assert!(
        erl_text.contains("_TerlanMutReceiver0 = maps:put"),
        "set command mutator should use receiver rebinding:\n{}",
        erl_text
    );

    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run set intrinsic launcher");
    assert!(
        launcher_output.status.success(),
        "launcher failed: stdout={} stderr={}",
        String::from_utf8_lossy(&launcher_output.stdout),
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "1\n");
}

/// Verifies release std collection imports execute through embedded summaries.
///
/// Inputs:
/// - A manifest-backed external project without local `std/summaries`
///   files.
/// - Source imports for `std.collections.Map`, `std.collections.List`, and
///   `std.collections.Set` module/type surfaces.
/// - Command-style mutable receiver calls followed by observers.
///
/// Output:
/// - Test passes when `terlc build` emits runnable BEAM artifacts and the
///   launcher prints `1`, `1`, and `1`.
///
/// Transformation:
/// - Proves promoted release collection summaries are embedded into the
///   compiler, imported receiver mutators use compiler-owned intrinsic
///   lowering, and sequence lowering rebinds updated receivers before
///   observer calls.
#[test]
fn build_command_compiles_release_std_collection_receiver_mutators() {
    let dir = make_temp_dir("directory_project_release_std_collection_mutators");
    let project_dir = dir.join("project");
    let app_dir = project_dir.join("src/app");
    let out_dir = dir.join("build");
    fs::create_dir_all(&app_dir).expect("failed to create project src dir");
    fs::write(
        project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
        "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n",
    )
    .expect("failed to write project manifest fixture");
    fs::write(
        app_dir.join("Main.terl"),
        "\
module app.Main.\n\
\n\
import std.io.Console.{println}.\n\
import std.core.Int.\n\
import std.collections.Map.\n\
import type std.collections.Map.Map.\n\
import std.collections.List.\n\
import type std.collections.List.List.\n\
import std.collections.Set.\n\
import type std.collections.Set.Set.\n\
\n\
pub mapcount(users: Map[String, String]): Int ->\n\
users.put(\"alice\", \"Alice\");\n\
users.size().\n\
\n\
pub listcount(values: List[String]): Int ->\n\
values.push(\"Alice\");\n\
values.length().\n\
\n\
pub setcount(values: Set[String]): Int ->\n\
values.add(\"Alice\");\n\
values.size().\n\
\n\
pub main(): Unit ->\n\
println(Int.to_string(mapcount(Map.new())));\n\
println(Int.to_string(listcount(List.new())));\n\
println(Int.to_string(setcount(Set.new()))).\n",
    )
    .expect("failed to write release std collection module");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            project_dir.display().to_string(),
            "--target".to_string(),
            "erlang".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    let erl_text =
        fs::read_to_string(out_dir.join("src/app_main.erl")).expect("read generated erl");
    assert!(
        erl_text.contains("_TerlanMutReceiver0 = maps:put"),
        "map/set mutators should rebind intrinsic receiver updates:\n{}",
        erl_text
    );
    assert!(
        erl_text.contains("_TerlanMutReceiver0 = lists:append"),
        "list mutator should rebind intrinsic receiver update:\n{}",
        erl_text
    );

    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run release std collection launcher");
    assert!(
        launcher_output.status.success(),
        "launcher failed: stdout={} stderr={}",
        String::from_utf8_lossy(&launcher_output.stdout),
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&launcher_output.stdout),
        "1\n1\n1\n"
    );
}

/// Verifies explicit traversal calls execute through compiler-owned intrinsics.
///
/// Inputs:
/// - A manifest-backed project importing release `std.collections.List`,
///   `std.collections.Iterator`, and `std.core.Option`.
/// - A function that calls `Iterator.next(values.iterator())` and pattern
///   matches `None` plus `Some({value, _})`.
///
/// Output:
/// - Test passes when `terlc build --target erlang` emits Erlang source,
///   a runnable BEAM artifact, and prints the first traversed list value
///   from the executable.
///
/// Transformation:
/// - Proves explicit source traversal now lowers via the collection receiver
///   intrinsic, runs through BEAM, and stays on the compiler-owned
///   `Iterator.next` state shape.
#[test]
fn build_command_compiles_source_traversal_receiver_iterator_next() {
    let dir = make_temp_dir("directory_project_source_traversal_receiver_iterator_next");
    let project_dir = dir.join("project");
    let app_dir = project_dir.join("src/app");
    let out_dir = dir.join("build");
    fs::create_dir_all(&app_dir).expect("failed to create project src dir");
    fs::write(
        project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
        "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n",
    )
    .expect("failed to write project manifest fixture");
    fs::write(
        app_dir.join("Main.terl"),
        "\
module app.Main.\n\
\n\
import std.io.Console.{println}.\n\
import std.core.Int.\n\
import std.collections.List.\n\
import type std.collections.List.List.\n\
import std.collections.Iterator.\n\
import type std.collections.Iterator.\n\
import std.core.Option.{None, Some}.\n\
import type std.core.Option.{Option}.\n\
\
    pub first(values: List[Int]): Int ->\n\
case Iterator.next(values.iterator()) {\n\
    None ->\n\
        0;\n\
    Some({value, _}) ->\n\
        value\n\
}.\n\
\
pub main(): Unit ->\n\
println(Int.to_string(first([42]))).
",
    )
    .expect("failed to write source traversal module");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            project_dir.display().to_string(),
            "--target".to_string(),
            "erlang".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    let erl_source =
        fs::read_to_string(out_dir.join("src/app_main.erl")).expect("read emitted Erlang");
    assert!(
        erl_source.contains("case Values of"),
        "iterator receiver should lower through List.iterator intrinsic: {}",
        erl_source
    );
    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run release traversal fixture");
    assert!(
        launcher_output.status.success(),
        "traversal launcher should exit successfully: stderr={}",
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "42\n");
}

#[test]
fn build_command_compiles_source_traversal_iterator_next() {
    let dir = make_temp_dir("directory_project_source_traversal_iterator_next");
    let project_dir = dir.join("project");
    let app_dir = project_dir.join("src/app");
    let out_dir = dir.join("build");
    fs::create_dir_all(&app_dir).expect("failed to create project src dir");
    fs::write(
        project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
        "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n",
    )
    .expect("failed to write project manifest fixture");
    fs::write(
        app_dir.join("Main.terl"),
        "\
module app.Main.\n\
\
import std.io.Console.{println}.\n\
import std.core.Int.\n\
import std.core.Option.{None, Some}.\n\
import type std.core.Option.{Option}.\n\
import std.collections.List.\n\
import type std.collections.List.List.\n\
import std.collections.Iterator.\n\
import type std.collections.Iterator.\n\
\
pub first(values: List[Int]): Int ->\n\
case Iterator.next(List.iterator(values)) {\n\
    None ->\n\
        0;\n\
    Some({value, _}) ->\n\
        value\n\
}.\n\
\
pub main(): Unit ->\n    println(Int.to_string(first([42]))).\n",
    )
    .expect("failed to write source traversal module");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            project_dir.display().to_string(),
            "--target".to_string(),
            "erlang".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    let erl_source =
        fs::read_to_string(out_dir.join("src/app_main.erl")).expect("read emitted Erlang");
    assert!(
        !erl_source.contains("std_collections_iterator:next")
            && !erl_source.contains("std_collections_list:iterator"),
        "explicit traversal should not use unresolved std runtime module calls: {}",
        erl_source
    );
    assert!(
        erl_source.contains("case Values of"),
        "explicit traversal intrinsic should lower through iterator state case shape: {}",
        erl_source
    );
    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run release traversal fixture");
    assert!(
        launcher_output.status.success(),
        "traversal launcher should exit successfully: stderr={}",
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "42\n");
}

/// Verifies list receiver traversal can call a function-valued callback.
///
/// Inputs:
/// - A manifest-backed project importing release `std.collections.List`.
/// - A callback function with shape `(String) -> Unit`.
/// - A `List[String]` receiver expression calling
///   `values.each((value) -> print_value(value))`.
///
/// Output:
/// - Test passes when `terlc build --target erlang` emits Erlang source,
///   a runnable BEAM artifact, and prints every list value in traversal
///   order.
///
/// Transformation:
/// - Forces the formal build path through source std receiver dispatch,
///   lambda-to-function-value invocation, and `Iterator.each` traversal
///   without mutating the original list.
#[test]
fn build_command_compiles_source_traversal_list_each_receiver_callback() {
    let dir = make_temp_dir("directory_project_source_traversal_list_each");
    let project_dir = dir.join("project");
    let app_dir = project_dir.join("src/app");
    let out_dir = dir.join("build");
    fs::create_dir_all(&app_dir).expect("failed to create project src dir");
    fs::write(
        project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
        "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n",
    )
    .expect("failed to write project manifest fixture");
    fs::write(
        app_dir.join("Main.terl"),
        "\
module app.Main.\n\
\n\
import std.io.Console.{println}.\n\
import std.collections.List.\n\
import type std.collections.List.List.\n\
\n\
pub print_value(value: String): Unit ->\n\
println(value).\n\
\n\
pub main(): Unit ->\n\
let values = [\"Alice\", \"Bob\"];\n\
values.each((value) -> print_value(value)).\n",
    )
    .expect("failed to write source traversal list each module");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            project_dir.display().to_string(),
            "--target".to_string(),
            "erlang".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    let erl_source =
        fs::read_to_string(out_dir.join("src/app_main.erl")).expect("read emitted Erlang");
    assert!(
        !erl_source.contains("std_collections_list"),
        "source traversal should not emit unresolved std list module calls: {}",
        erl_source
    );
    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run list each traversal fixture");
    assert!(
        launcher_output.status.success(),
        "list each launcher should exit successfully: stderr={}",
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&launcher_output.stdout),
        "Alice\nBob\n"
    );
}
