use std::fs;
use std::path::Path;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

/// Proves exported shape aliases survive project interface generation and
/// execute as ordinary VM patterns in an importing test module.
#[test]
fn project_vm_tests_execute_selected_imported_shape_alias() {
    let exit_code = run_project_vm_test(
        "terlan_imported_shape_vm_test",
        "shape-import-app",
        "src/app/Shapes.terl",
        r#"module app.Shapes.

pub shape Positive(value) =
    value where value > 0.

pub shape Tagged(value) =
    {Atom["ok"], Positive(value)}.
"#,
        "tests/app/ShapeImportTest.terl",
        r#"module app.ShapeImportTest.

import app.Shapes.{Tagged as Success}.

@test
pub imported_shape_matches(): Bool ->
    case {Atom["ok"], 7} {
        Success(value) -> value == 7;
        _ -> false
    }.

@test
pub imported_shape_guard_rejects_nonpositive_value(): Bool ->
    case {Atom["ok"], 0} {
        Success(value) -> false;
        _ -> true
    }.
"#,
    );

    assert_eq!(exit_code, ExitCode::SUCCESS);
}

/// Proves exported binary layout shapes preserve descriptors and substitute
/// capture names when imported under an alias.
#[test]
fn project_vm_tests_execute_imported_binary_layout_shape() {
    let exit_code = run_project_vm_test(
        "terlan_imported_binary_shape_vm_test",
        "binary-shape-import-app",
        "src/app/Packets.terl",
        r#"module app.Packets.

pub shape Packet(port, body) =
    Binary[big] {
        port: UInt[16],
        body: Rest
    }.
"#,
        "tests/app/BinaryShapeImportTest.terl",
        r#"module app.BinaryShapeImportTest.

import app.Packets.{Packet as TransportPacket}.
import std.vm.Bytes.{from_list}.

@test
pub imported_binary_shape_matches(): Bool ->
    let port = 8080;
    let body = from_list([1, 2, 3]);
    let packet = Binary[big] { port: UInt[16], body: Rest };
    case packet {
        TransportPacket(decoded_port, decoded_body) ->
            decoded_port == 8080 and decoded_body.to_list() == [1, 2, 3];
        _ -> false
    }.
"#,
    );

    assert_eq!(exit_code, ExitCode::SUCCESS);
}

/// Proves imported receiver methods retain implication evidence and execute
/// through the default VM project-test lane.
#[test]
fn project_vm_tests_execute_imported_receiver_method_implication() {
    let exit_code = run_project_vm_test(
        "terlan_imported_receiver_implication_vm_test",
        "receiver-implication-app",
        "src/app/Presentation.terl",
        r#"module app.Presentation.

pub struct Presenter {
    prefix: String
}.

pub struct User {
    name: String
}.

pub presenter(prefix: String): Presenter ->
    Presenter {prefix: prefix}.

pub user(name: String): User ->
    User {name: name}.

pub (presenter: Presenter) present[T => {name: String}](value: T): String ->
    presenter.prefix + value.name.
"#,
        "tests/app/PresentationTest.terl",
        r#"module app.PresentationTest.

import app.Presentation.{Presenter, User, presenter, user}.

@test
pub imported_receiver_implication_executes(): Bool ->
    let view = presenter("User: ");
    let account = user("Ada");
    view.present(account) == "User: Ada".
"#,
    );

    assert_eq!(exit_code, ExitCode::SUCCESS);
}

/// Proves imported generic struct implication metadata supports VM projection.
#[test]
fn project_vm_tests_execute_imported_generic_struct_implication() {
    let exit_code = run_project_vm_test(
        "terlan_imported_generic_struct_implication_vm_test",
        "generic-struct-implication-app",
        "src/app/Pages.terl",
        r#"module app.Pages.

pub struct Profile {
    title: String
}.

pub struct Page[T => {title: String}] {
    model: T
}.

pub profile(title: String): Profile -> Profile {title: title}.

pub page[T => {title: String}](model: T): Page[T] -> Page {model: model}.
"#,
        "tests/app/PagesTest.terl",
        r#"module app.PagesTest.

import app.Pages.{Page, Profile, page, profile}.

@test
pub imported_generic_struct_implication_executes(): Bool ->
    let view = page(profile("Overview"));
    view.model.title == "Overview".
"#,
    );

    assert_eq!(exit_code, ExitCode::SUCCESS);
}

/// Runs one temporary project test against a prepared source-root module.
fn run_project_vm_test(
    prefix: &str,
    package: &str,
    source_path: &str,
    source: &str,
    test_path: &str,
    test_source: &str,
) -> ExitCode {
    let root = std::env::temp_dir().join(format!(
        "{prefix}_{}_{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should follow Unix epoch")
            .as_nanos()
    ));
    write_project_file(
        &root,
        "terlan.toml",
        &format!(
            "[package]\nname = \"{package}\"\nversion = \"0.0.0\"\n\n[build]\nsource_roots = [\"src\"]\n"
        ),
    );
    write_project_file(&root, source_path, source);
    write_project_file(&root, test_path, test_source);

    let exit_code = run(
        CliCommand {
            verb: Some("test".to_string()),
            args: vec![root.join("tests").to_string_lossy().into_owned()],
        },
        CliState::default(),
    );
    let _ = fs::remove_dir_all(&root);
    exit_code
}

/// Writes one fixture file after creating its parent directory.
fn write_project_file(root: &Path, relative: &str, contents: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().expect("fixture path parent"))
        .expect("create fixture directory");
    fs::write(path, contents).expect("write project fixture");
}
use crate::{CliCommand, CliState};
