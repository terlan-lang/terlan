use super::*;

/// Verifies executable tuple and list destructuring patterns.
///
/// Inputs:
/// - A manifest-backed project with functions that pattern match a tuple
///   pair and a list-cons shape.
/// - A `main` function that prints the sum of tuple fields and the head of
///   a non-empty list.
///
/// Output:
/// - Test passes when `terlc build --target erlang` emits a runnable BEAM
///   artifact that prints `7` and `5`.
///
/// Transformation:
/// - Forces P0.5c data-pattern closure through the public build command,
///   proving tuple destructuring, empty-list matching, and list-cons
///   destructuring execute through the formal compiler path.
#[test]
fn build_command_compiles_executable_tuple_and_list_destructuring_patterns() {
    let dir = make_temp_dir("directory_project_data_pattern_closure");
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
pub sum(pair: {Int, Int}): Int ->\n\
case pair {\n\
    {left, right} ->\n\
        left + right\n\
}.\n\
\n\
pub head_or_zero(values: [Int]): Int ->\n\
case values {\n\
    [] ->\n\
        0;\n\
    [head | _tail] ->\n\
        head\n\
}.\n\
\n\
pub main(): Unit ->\n\
println(Int.to_string(sum({3, 4})));\n\
println(Int.to_string(head_or_zero([5, 6]))).\n",
    )
    .expect("failed to write data pattern closure module");

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
        erl_source.contains("{Left, Right}"),
        "tuple destructuring should lower to an Erlang tuple pattern: {}",
        erl_source
    );
    assert!(
        erl_source.contains("[Head|_tail]"),
        "list-cons destructuring should lower to an Erlang cons pattern: {}",
        erl_source
    );
    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run data pattern closure fixture");
    assert!(
        launcher_output.status.success(),
        "data pattern closure launcher should exit successfully: stderr={}",
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "7\n5\n");
}

/// Verifies executable map expressions and map destructuring patterns.
///
/// Inputs:
/// - A manifest-backed project with a typed map value whose required keys
///   are `name` and `age`.
/// - A function that pattern matches the map and returns the `name` field.
///
/// Output:
/// - Test passes when `terlc build --target erlang` emits a runnable BEAM
///   artifact that prints `Alice`.
///
/// Transformation:
/// - Forces P0.5c map data closure through the public build command,
///   proving map construction, required-key matching, ignored bindings,
///   and typed map specs execute through the formal compiler path.
#[test]
fn build_command_compiles_executable_map_expression_and_pattern() {
    let dir = make_temp_dir("directory_project_map_pattern_closure");
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
\n\
pub display_name(user: #{name := String, age := Int}): String ->\n\
case user {\n\
    #{name := name, age := _age} ->\n\
        name\n\
}.\n\
\n\
pub main(): Unit ->\n\
println(display_name(#{name := \"Alice\", age := 30})).\n",
    )
    .expect("failed to write map pattern closure module");

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
        erl_source.contains("#{name:=Name, age:=_age}"),
        "map destructuring should lower to an Erlang map pattern: {}",
        erl_source
    );
    assert!(
        erl_source.contains("#{name=>\"Alice\", age=>30}"),
        "map construction should lower to an Erlang map expression: {}",
        erl_source
    );
    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run map pattern closure fixture");
    assert!(
        launcher_output.status.success(),
        "map pattern closure launcher should exit successfully: stderr={}",
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "Alice\n");
}

/// Verifies executable struct-backed record construction, field access, and
/// record patterns.
///
/// Inputs:
/// - A manifest-backed project declaring a source-level `User` struct.
/// - Functions that construct `User`, read `user.name`, update a record
///   field, and pattern match `#User{name = name}`.
///
/// Output:
/// - Test passes when `terlc build --target erlang` emits a runnable BEAM
///   artifact that prints `Alice` and `Bob`.
///
/// Transformation:
/// - Forces P0.5c struct/record data closure through the public build
///   command, proving source struct declarations generate record metadata
///   and executable record construction/access/pattern lowering.
#[test]
fn build_command_compiles_executable_struct_record_construction_access_and_pattern() {
    let dir = make_temp_dir("directory_project_struct_record_pattern_closure");
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
\n\
pub struct User {\n\
id: Int,\n\
name: String\n\
}.\n\
\n\
pub make_user(id: Int, name: String): User ->\n\
User(id = id, name = name).\n\
\n\
pub display_name(user: User): String ->\n\
user.name.\n\
\n\
pub rename(user: User, name: String): User ->\n\
user#User{name = name}.\n\
\n\
pub matched_name(user: User): String ->\n\
case user {\n\
    #User{name = name} ->\n\
        name\n\
}.\n\
\n\
pub main(): Unit ->\n\
let alice = make_user(1, \"Alice\");\n\
let bob = rename(alice, \"Bob\");\n\
println(display_name(alice));\n\
println(matched_name(bob)).\n",
    )
    .expect("failed to write struct record pattern closure module");

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
        erl_source.contains("-record(user, {id, name})."),
        "struct should emit an Erlang record declaration: {}",
        erl_source
    );
    assert!(
        erl_source.contains("#user{id = Id, name = Name}"),
        "record construction should lower to an Erlang record: {}",
        erl_source
    );
    assert!(
        erl_source.contains("User#user.name"),
        "field access should lower through the declared struct type: {}",
        erl_source
    );
    assert!(
        erl_source.contains("#user{name = Name} -> Name"),
        "record pattern should lower to an Erlang record pattern: {}",
        erl_source
    );
    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run struct record pattern closure fixture");
    assert!(
        launcher_output.status.success(),
        "struct record pattern closure launcher should exit successfully: stderr={}",
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&launcher_output.stdout),
        "Alice\nBob\n"
    );
}

/// Verifies executable list comprehensions with stacked boolean filters.
///
/// Inputs:
/// - A manifest-backed project with a function that filters a list of
///   integers using two ordered boolean qualifiers.
/// - A caller that pattern matches the filtered result and prints the sum
///   of the first two selected values.
///
/// Output:
/// - Test passes when `terlc build --target erlang` emits a runnable BEAM
///   artifact that prints `6`.
///
/// Transformation:
/// - Forces P0.5c comprehension closure through the public build command,
///   proving stacked filters parse, typecheck as `Bool`, lower to backend
///   list-comprehension filters, and execute before generic `Iterable`
///   desugaring broadens comprehension sources.
#[test]
fn build_command_compiles_executable_list_comprehension_stacked_filters() {
    let dir = make_temp_dir("directory_project_list_comprehension_filter_closure");
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
pub selected(values: [Int]): [Int] ->\n\
[value | value <- values, value > 1, value < 5].\n\
\n\
pub sum_first_two(values: [Int]): Int ->\n\
case selected(values) {\n\
    [first | rest] ->\n\
        case rest {\n\
            [second | _tail] ->\n\
                first + second;\n\
            [] ->\n\
                first\n\
        };\n\
    [] ->\n\
        0\n\
}.\n\
\n\
pub main(): Unit ->\n\
println(Int.to_string(sum_first_two([0, 2, 4, 7]))).\n",
    )
    .expect("failed to write list comprehension filter closure module");

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
        erl_source.contains("Value > 1") && erl_source.contains("Value < 5"),
        "list comprehension should preserve both stacked filters: {}",
        erl_source
    );
    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run list comprehension filter closure fixture");
    assert!(
        launcher_output.status.success(),
        "list comprehension filter launcher should exit successfully: stderr={}",
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "6\n");
}

/// Verifies executable generic iterable list-comprehension lowering.
///
/// Inputs:
/// - A manifest-backed project declaring a local `Iterable[C, T]` trait.
/// - A struct that implements the trait with an `iterator()` receiver
///   method returning a list-backed iterator state.
/// - A comprehension whose source is the struct, not a raw list.
///
/// Output:
/// - Test passes when `terlc build --target erlang` emits a runnable BEAM
///   artifact that prints the sum of selected values.
///
/// Transformation:
/// - Proves the first P0.4d generic iterable slice rewrites the
///   comprehension source through the local receiver `iterator` method
///   before using the existing list-backed Erlang comprehension lowering.
#[test]
fn build_command_compiles_executable_iterable_list_comprehension_source() {
    let dir = make_temp_dir("directory_project_iterable_comprehension_source");
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
pub type Iterator[T] = List[T].\n\
\n\
pub trait Iterable[C, T] {\n\
iterator(collection: C): Iterator[T].\n\
}.\n\
\n\
pub struct IntCollection implements Iterable[IntCollection, Int] {\n\
values: List[Int]\n\
}.\n\
\n\
pub (collection: IntCollection) iterator(): Iterator[Int] ->\n\
collection.values.\n\
\n\
pub selected(items: IntCollection): List[Int] ->\n\
[value | value <- items, value > 1, value < 5].\n\
\n\
pub sum_first_two(values: List[Int]): Int ->\n\
case values {\n\
    [first | rest] ->\n\
        case rest {\n\
            [second | _tail] ->\n\
                first + second;\n\
            [] ->\n\
                first\n\
        };\n\
    [] ->\n\
        0\n\
}.\n\
\n\
pub main(): Unit ->\n\
println(Int.to_string(sum_first_two(selected(IntCollection(values = [0, 2, 4, 7]))))).\n",
    )
    .expect("failed to write iterable comprehension source module");

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
        erl_source.contains("TerlanIterator") && erl_source.contains("iterator(Items)"),
        "iterable comprehension should bind an explicit iterator state: {}",
        erl_source
    );
    assert!(
        erl_source.contains("case TerlanIter")
            && erl_source.contains("'none' -> lists:reverse")
            && erl_source.contains("{'some', {Value, TerlanNext"),
        "iterable comprehension should lower to explicit next-state traversal: {}",
        erl_source
    );
    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run iterable comprehension source fixture");
    assert!(
        launcher_output.status.success(),
        "iterable comprehension launcher should exit successfully: stderr={}",
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "6\n");
}

/// Verifies std-facing `Option` and `Result` alias constructors execute.
///
/// Inputs:
/// - A manifest-backed project importing `None`, `Some`, `Ok`, and `Err`
///   from release std summaries.
/// - Functions that pattern match imported singleton and payload aliases.
///
/// Output:
/// - Test passes when `terlc build --target erlang` emits a runnable BEAM
///   artifact that prints `3`, `0`, `4`, and `0`.
///
/// Transformation:
/// - Forces P0.5c constructor/alias pattern closure through std-facing
///   imports, proving release `Option` / `Result` aliases construct,
///   destructure, and execute without unresolved std module calls.
#[test]
fn build_command_compiles_executable_std_option_result_alias_patterns() {
    let dir = make_temp_dir("directory_project_std_option_result_alias_patterns");
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
import std.core.Option.{None, Some}.\n\
import type std.core.Option.{Option}.\n\
import std.core.Result.{Err, Ok}.\n\
import type std.core.Result.{Result}.\n\
\n\
pub option_value(value: Option[Int]): Int ->\n\
case value {\n\
    None ->\n\
        0;\n\
    Some(x) ->\n\
        x\n\
}.\n\
\n\
pub result_value(value: Result[Int, String]): Int ->\n\
case value {\n\
    Ok(x) ->\n\
        x;\n\
    Err(_reason) ->\n\
        0\n\
}.\n\
\n\
pub main(): Unit ->\n\
println(Int.to_string(option_value(Some(3))));\n\
println(Int.to_string(option_value(None)));\n\
println(Int.to_string(result_value(Ok(4))));\n\
println(Int.to_string(result_value(Err(\"bad\")))).\n",
    )
    .expect("failed to write std option/result alias pattern module");

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
        erl_source.contains("{'some', 3}") && erl_source.contains("{ok, 4}"),
        "std alias constructor calls should lower to runtime tuples: {}",
        erl_source
    );
    assert!(
        erl_source.contains("{'some', X} -> X") && erl_source.contains("{ok, X} -> X"),
        "std alias constructor patterns should lower to runtime tuple patterns: {}",
        erl_source
    );
    assert!(
        !erl_source.contains("std_core_option:None")
            && !erl_source.contains("std_core_option:Some")
            && !erl_source.contains("std_core_result:Ok")
            && !erl_source.contains("std_core_result:Err"),
        "std alias constructors should not emit unresolved std module calls: {}",
        erl_source
    );
    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run std option/result alias pattern fixture");
    assert!(
        launcher_output.status.success(),
        "std option/result alias launcher should exit successfully: stderr={}",
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&launcher_output.stdout),
        "3\n0\n4\n0\n"
    );
}

/// Verifies mutable receiver methods may fluently return the receiver type.
///
/// Inputs:
/// - A manifest-backed project declaring a `mut` receiver method whose
///   source return type is the receiver type instead of `Unit`.
/// - A sequence that calls the method and then observes the rebound
///   receiver through an immutable receiver method.
///
/// Output:
/// - Test passes when `terlc build` emits runnable BEAM-backed artifacts
///   and the launcher prints the state produced by the fluent mutator.
///
/// Transformation:
/// - Runs the fluent mutable-receiver shape through the formal build path
///   and proves it uses the same receiver-rebinding backend ABI as
///   command-style `Unit` mutators.
#[test]
fn build_command_compiles_mutable_receiver_method_with_receiver_return() {
    let dir = make_temp_dir("directory_project_mutable_receiver_return");
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
pub struct Map {\n\
size: Int\n\
}.\n\
\n\
pub constructor Map {\n\
(size: Int): Map -> Map(size = size)\n\
}.\n\
\n\
pub (mut map: Map) put(): Map ->\n\
Map(map.size + 1).\n\
\n\
pub (map: Map) length(): Int ->\n\
map.size.\n\
\n\
run(map: Map): String ->\n\
map.put();\n\
std.core.Int.to_string(map.length()).\n\
\n\
pub main(): Unit ->\n\
std.io.Console.println(run(Map(0))).\n",
    )
    .expect("failed to write fluent mutable receiver module");

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
        fs::read_to_string(out_dir.join("src/app_main.erl")).expect("read generated app_main.erl");
    assert!(
        erl_text.contains("_TerlanMutReceiver0 = put("),
        "fluent mutable receiver should bind updated receiver:\n{}",
        erl_text
    );

    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run fluent mutable receiver launcher");
    assert!(
        launcher_output.status.success(),
        "launcher failed: stdout={} stderr={}",
        String::from_utf8_lossy(&launcher_output.stdout),
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "1\n");
}
