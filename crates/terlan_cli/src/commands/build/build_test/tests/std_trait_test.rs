use super::*;

/// Verifies imported std `Show` trait dispatch lowers to primitive intrinsics.
///
/// Inputs:
/// - A project root without local `std/summaries` files.
/// - A source file importing `std.core.String.{Show}` and calling
///   `Show.to_string` for a primitive `Int` value.
///
/// Output:
/// - Test passes when `terlc build <project> --target erlang` emits a
///   launcher that prints the primitive values through the trait surface.
///
/// Transformation:
/// - Resolves `Show` from the shipped `std.core.String` summary, selects
///   public conformance facts for primitive source types, and lowers the
///   trait calls to compiler-owned conversion intrinsics instead of remote
///   std wrapper calls.
#[test]
fn build_command_compiles_imported_std_show_trait_dispatch() {
    let dir = make_temp_dir("directory_project_imported_std_show_trait_dispatch");
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
import std.core.String.{Show}.\n\
\n\
pub main(): Unit ->\n\
println(Show.to_string(2)).\n",
    )
    .expect("failed to write imported std Show trait dispatch fixture");

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
        erl_source.contains("erlang:integer_to_list(2)"),
        "Show[Int] should lower through the Int primitive intrinsic: {}",
        erl_source
    );
    assert!(
        !erl_source.contains("std_core_string:typer_trait_show_to_string"),
        "imported std Show calls must not require an uncompiled std wrapper: {}",
        erl_source
    );
    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run generated project launcher");
    assert!(
        launcher_output.status.success(),
        "launcher should exit successfully: stderr={}",
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "2\n");
}

/// Verifies imported std `Equal` trait dispatch lowers to exact equality.
///
/// Inputs:
/// - A project root without local `std/summaries` files.
/// - A source file importing `std.core.Equal.{Equal}` and calling
///   `Equal[Int].equal` for primitive `Int` values.
///
/// Output:
/// - Test passes when `terlc build <project> --target erlang` emits a
///   launcher that prints `true`.
///
/// Transformation:
/// - Resolves `Equal` from the shipped `std.core.Equal` summary, selects
///   public conformance facts for primitive source types, and lowers the
///   trait call through the formal trait dispatch path instead of exposing
///   ad hoc equality helpers.
#[test]
fn build_command_compiles_imported_std_equal_trait_dispatch() {
    let dir = make_temp_dir("directory_project_imported_std_equal_trait_dispatch");
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
import std.core.Bool.\n\
import std.core.Equal.{Equal}.\n\
\n\
pub main(): Unit ->\n\
println(Bool.to_string(Equal[Int].equal(2, 2))).\n",
    )
    .expect("failed to write imported std Equal trait dispatch fixture");

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
        !erl_source.contains("std_core_bool:equal")
            && !erl_source.contains("std_core_string:equal")
            && !erl_source.contains("std_core_atom:equal"),
        "Equal trait calls must not lower through removed ad hoc equality modules: {}",
        erl_source
    );
    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run generated project launcher");
    assert!(
        launcher_output.status.success(),
        "launcher should exit successfully: stderr={}",
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "true\n");
}

/// Verifies imported std `Parse` trait dispatch lowers by explicit target.
///
/// Inputs:
/// - A project root without local `std/summaries` files.
/// - A source file importing `std.core.String.{Parse}` and calling
///   `Parse[Int].from_string` for a string literal.
///
/// Output:
/// - Test passes when `terlc build <project> --target erlang` emits a
///   launcher that prints the parsed integer through the trait surface.
///
/// Transformation:
/// - Resolves `Parse` from the shipped `std.core.String` summary, uses the
///   explicit `[Int]` target to select the public primitive conformance,
///   and lowers the parse call to the compiler-owned integer parse
///   intrinsic instead of an uncompiled std wrapper call.
#[test]
fn build_command_compiles_imported_std_parse_trait_dispatch() {
    let dir = make_temp_dir("directory_project_imported_std_parse_trait_dispatch");
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
import type std.core.Option.Option.\n\
import std.core.String.{Parse}.\n\
\n\
pub parsed_value(value: Option[Int]): Int ->\n\
case value {\n\
    None ->\n\
        0;\n\
    Some(parsed) ->\n\
        parsed\n\
}.\n\
\n\
pub main(): Unit ->\n\
println(Int.to_string(parsed_value(Parse[Int].from_string(\"42\")))).\n",
    )
    .expect("failed to write imported std Parse trait dispatch fixture");

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
        erl_source.contains("string:to_integer"),
        "Parse[Int] should lower through the Int parse primitive intrinsic: {}",
        erl_source
    );
    assert!(
        !erl_source.contains("std_core_string:typer_trait_parse_from_string"),
        "imported std Parse calls must not require an uncompiled std wrapper: {}",
        erl_source
    );
    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run generated project launcher");
    assert!(
        launcher_output.status.success(),
        "launcher should exit successfully: stderr={}",
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "42\n");
}

/// Verifies imported std `Iterable` trait dispatch lowers to list traversal.
///
/// Inputs:
/// - A project root without local `std/summaries` files.
/// - A source file importing `std.collections.Iterable.{Iterable}` and
///   calling `Iterable.iterator` for a `List[Int]` value.
///
/// Output:
/// - Test passes when `terlc build <project> --target erlang` emits a
///   launcher that prints the first value yielded through the trait
///   surface.
///
/// Transformation:
/// - Resolves `Iterable` from the shipped `std.collections.Iterable`
///   summary, selects the public `Iterable[List[T], T]` conformance, and
///   lowers the trait call to the compiler-owned list iterator intrinsic
///   instead of requiring an uncompiled std wrapper function.
#[test]
fn build_command_compiles_imported_std_iterable_trait_dispatch() {
    let dir = make_temp_dir("directory_project_imported_std_iterable_trait_dispatch");
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
import std.collections.Iterable.{Iterable}.\n\
import std.collections.Iterator.\n\
import type std.collections.List.List.\n\
\n\
pub first(values: List[Int]): Int ->\n\
case Iterator.next(Iterable.iterator(values)) {\n\
    None ->\n\
        0;\n\
    Some({value, _}) ->\n\
        value\n\
}.\n\
\n\
pub main(): Unit ->\n\
println(Int.to_string(first([42]))).\n",
    )
    .expect("failed to write imported std Iterable trait dispatch fixture");

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
        "Iterable[List[T]] should lower through the list iterator intrinsic: {}",
        erl_source
    );
    assert!(
        !erl_source.contains("std_collections_iterable:typer_trait_iterable_iterator"),
        "imported std Iterable calls must not require an uncompiled std wrapper: {}",
        erl_source
    );
    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run generated project launcher");
    assert!(
        launcher_output.status.success(),
        "launcher should exit successfully: stderr={}",
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "42\n");
}

/// Verifies imported std `Enumerable` trait dispatch lowers to list each.
///
/// Inputs:
/// - A project root without local `std/summaries` files.
/// - A source file importing `std.collections.Enumerable.{Enumerable}` and
///   calling `Enumerable.each` for a `List[String]` value.
///
/// Output:
/// - Test passes when `terlc build <project> --target erlang` emits a
///   launcher that prints every list value in traversal order.
///
/// Transformation:
/// - Resolves `Enumerable` from the shipped `std.collections.Enumerable`
///   summary, selects the public `Enumerable[List[T], T]` conformance, and
///   lowers the trait call through the same list foreach bridge as
///   receiver syntax `values.each(cb)`.
#[test]
fn build_command_compiles_imported_std_enumerable_trait_each_dispatch() {
    let dir = make_temp_dir("directory_project_imported_std_enumerable_trait_each_dispatch");
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
import std.collections.Enumerable.{Enumerable}.\n\
import type std.collections.List.List.\n\
\n\
pub print_value(value: String): Unit ->\n\
println(value).\n\
\n\
pub main(): Unit ->\n\
let values = [\"Alice\", \"Bob\"];\n\
Enumerable.each(values, (value) -> print_value(value)).\n",
    )
    .expect("failed to write imported std Enumerable trait each fixture");

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
        erl_source.contains("lists:foreach"),
        "Enumerable[List[T]].each should lower through the list foreach bridge: {}",
        erl_source
    );
    assert!(
        !erl_source.contains("std_collections_enumerable:typer_trait_enumerable_each"),
        "imported std Enumerable calls must not require an uncompiled std wrapper: {}",
        erl_source
    );
    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run generated project launcher");
    assert!(
        launcher_output.status.success(),
        "launcher should exit successfully: stderr={}",
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&launcher_output.stdout),
        "Alice\nBob\n"
    );
}

/// Verifies imported std `Enumerable.map` lowers to list transformation.
///
/// Inputs:
/// - A project root without local `std/summaries` files.
/// - A source file importing `std.collections.Enumerable.{Enumerable}` and
///   calling `Enumerable.map` for a `List[String]` value.
///
/// Output:
/// - Test passes when `terlc build <project> --target erlang` emits a
///   launcher that prints the first mapped value.
///
/// Transformation:
/// - Resolves the generic `Enumerable.map[U]` method from the embedded std
///   summary, selects the public `Enumerable[List[T], T]` conformance, and
///   lowers the trait call through the selected list map bridge before
///   pattern matching the returned list.
#[test]
fn build_command_compiles_imported_std_enumerable_trait_map_dispatch() {
    let dir = make_temp_dir("directory_project_imported_std_enumerable_trait_map_dispatch");
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
import std.collections.Enumerable.{Enumerable}.\n\
import type std.collections.List.List.\n\
\n\
pub print_value(value: String): Unit ->\n\
println(value).\n\
\n\
pub main(): Unit ->\n\
let values = [\"Alice\", \"Bob\"];\n\
    mapped = Enumerable.map(values, (value) -> value);\n\
case mapped {\n\
    [first | _] ->\n\
        print_value(first);\n\
    [] ->\n\
        println(\"empty\")\n\
}.\n",
    )
    .expect("failed to write imported std Enumerable trait map fixture");

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
        erl_source.contains("lists:map"),
        "Enumerable[List[T]].map should lower through the list map bridge: {}",
        erl_source
    );
    assert!(
        !erl_source.contains("std_collections_enumerable:typer_trait_enumerable_map"),
        "imported std Enumerable map calls must not require an uncompiled std wrapper: {}",
        erl_source
    );
    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run generated project launcher");
    assert!(
        launcher_output.status.success(),
        "launcher should exit successfully: stderr={}",
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "Alice\n");
}

/// Verifies imported std `Enumerable.filter` lowers to list filtering.
///
/// Inputs:
/// - A project root without local `std/summaries` files.
/// - A source file importing `std.collections.Enumerable.{Enumerable}` and
///   calling `Enumerable.filter` for a `List[Int]` value.
///
/// Output:
/// - Test passes when `terlc build <project> --target erlang` emits a
///   launcher that prints the first retained value.
///
/// Transformation:
/// - Resolves `Enumerable.filter` from the embedded std summary, selects the
///   public `Enumerable[List[T], T]` conformance, lowers the trait call
///   through the selected list filter bridge, and pattern matches the
///   resulting list.
#[test]
fn build_command_compiles_imported_std_enumerable_trait_filter_dispatch() {
    let dir = make_temp_dir("directory_project_imported_std_enumerable_trait_filter_dispatch");
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
import std.collections.Enumerable.{Enumerable}.\n\
import type std.collections.List.List.\n\
\n\
pub main(): Unit ->\n\
let values = [0, 2, 4];\n\
    filtered = Enumerable.filter(values, (value) -> value > 1);\n\
case filtered {\n\
    [first | _] ->\n\
        println(Int.to_string(first));\n\
    [] ->\n\
        println(\"empty\")\n\
}.\n",
    )
    .expect("failed to write imported std Enumerable trait filter fixture");

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
        erl_source.contains("lists:filter"),
        "Enumerable[List[T]].filter should lower through the list filter bridge: {}",
        erl_source
    );
    assert!(
        !erl_source.contains("std_collections_enumerable:typer_trait_enumerable_filter"),
        "imported std Enumerable filter calls must not require an uncompiled std wrapper: {}",
        erl_source
    );
    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run generated project launcher");
    assert!(
        launcher_output.status.success(),
        "launcher should exit successfully: stderr={}",
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "2\n");
}

/// Verifies imported std `Enumerable.fold` lowers to list folding.
///
/// Inputs:
/// - A project root without local `std/summaries` files.
/// - A source file importing `std.collections.Enumerable.{Enumerable}` and
///   calling `Enumerable.fold` for a `List[Int]` value.
///
/// Output:
/// - Test passes when `terlc build <project> --target erlang` emits a
///   launcher that prints the final accumulator.
///
/// Transformation:
/// - Resolves `Enumerable.fold` from the embedded std summary, selects the
///   public `Enumerable[List[T], T]` conformance, lowers the trait call
///   through the selected list fold bridge, and adapts Terlan's
///   accumulator-first reducer to the BEAM fold callback order.
#[test]
fn build_command_compiles_imported_std_enumerable_trait_fold_dispatch() {
    let dir = make_temp_dir("directory_project_imported_std_enumerable_trait_fold_dispatch");
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
import std.collections.Enumerable.{Enumerable}.\n\
import type std.collections.List.List.\n\
\n\
pub main(): Unit ->\n\
let values = [1, 2, 3];\n\
    total = Enumerable.fold(values, 0, (acc, value) -> acc + value);\n\
println(Int.to_string(total)).\n",
    )
    .expect("failed to write imported std Enumerable trait fold fixture");

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
        erl_source.contains("lists:foldl"),
        "Enumerable[List[T]].fold should lower through the list fold bridge: {}",
        erl_source
    );
    assert!(
        !erl_source.contains("std_collections_enumerable:typer_trait_enumerable_fold"),
        "imported std Enumerable fold calls must not require an uncompiled std wrapper: {}",
        erl_source
    );
    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run generated project launcher");
    assert!(
        launcher_output.status.success(),
        "launcher should exit successfully: stderr={}",
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "6\n");
}

/// Verifies imported std `Enumerable.fold` lowers map traversal to pairs.
///
/// Inputs:
/// - A project root without local `std/summaries` files.
/// - A source file importing `std.collections.Map` and
///   `std.collections.Enumerable.{Enumerable}`.
///
/// Output:
/// - Test passes when `terlc build <project> --target erlang` emits a
///   launcher that counts two map entries.
///
/// Transformation:
/// - Resolves `Enumerable[Map[K, V], {K, V}]` from the embedded std
///   summary and lowers traversal through `maps:to_list/1` before the
///   shared `lists:foldl/3` bridge consumes entry pairs.
#[test]
fn build_command_compiles_imported_std_enumerable_trait_map_fold_dispatch() {
    let dir = make_temp_dir("directory_project_imported_std_enumerable_trait_map_fold_dispatch");
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
import std.collections.Enumerable.{Enumerable}.\n\
import std.collections.Map.\n\
\n\
pub main(): Unit ->\n\
let users = Map.new();\n\
users.put(\"alice\", 1);\n\
users.put(\"bob\", 2);\n\
let total = Enumerable.fold(users, 0, (acc, _entry) -> acc + 1);\n\
println(Int.to_string(total)).\n",
    )
    .expect("failed to write imported std Enumerable map fold fixture");

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
        erl_source.contains("maps:to_list"),
        "Enumerable[Map[K, V]].fold should lower through map pair traversal: {}",
        erl_source
    );
    assert!(
        erl_source.contains("lists:foldl"),
        "Enumerable[Map[K, V]].fold should share the fold bridge: {}",
        erl_source
    );
    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run generated project launcher");
    assert!(
        launcher_output.status.success(),
        "launcher should exit successfully: stderr={}",
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "2\n");
}

/// Verifies imported std `Enumerable.fold` lowers set traversal to values.
///
/// Inputs:
/// - A project root without local `std/summaries` files.
/// - A source file importing `std.collections.Set` and
///   `std.collections.Enumerable.{Enumerable}`.
///
/// Output:
/// - Test passes when `terlc build <project> --target erlang` emits a
///   launcher that sums two set values.
///
/// Transformation:
/// - Resolves `Enumerable[Set[T], T]` from the embedded std summary and
///   lowers traversal through `maps:keys/1` before the shared
///   `lists:foldl/3` bridge consumes values.
#[test]
fn build_command_compiles_imported_std_enumerable_trait_set_fold_dispatch() {
    let dir = make_temp_dir("directory_project_imported_std_enumerable_trait_set_fold_dispatch");
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
import std.collections.Enumerable.{Enumerable}.\n\
import std.collections.Set.\n\
\n\
pub main(): Unit ->\n\
let values = Set.new();\n\
values.add(1);\n\
values.add(2);\n\
let total = Enumerable.fold(values, 0, (acc, value) -> acc + value);\n\
println(Int.to_string(total)).\n",
    )
    .expect("failed to write imported std Enumerable set fold fixture");

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
        erl_source.contains("maps:keys"),
        "Enumerable[Set[T]].fold should lower through set value traversal: {}",
        erl_source
    );
    assert!(
        erl_source.contains("lists:foldl"),
        "Enumerable[Set[T]].fold should share the fold bridge: {}",
        erl_source
    );
    let executable_path = out_dir.join("bin/app");
    let launcher_output = Command::new(&executable_path)
        .output()
        .expect("run generated project launcher");
    assert!(
        launcher_output.status.success(),
        "launcher should exit successfully: stderr={}",
        String::from_utf8_lossy(&launcher_output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&launcher_output.stdout), "3\n");
}
