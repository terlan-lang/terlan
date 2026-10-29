use std::fs;
use std::path::Path;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

#[path = "test_shape_import_test/constructor_alias_test.rs"]
mod constructor_alias_test;
#[path = "test_shape_import_test/path_dependency_identity_test.rs"]
mod path_dependency_identity_test;
#[path = "test_shape_import_test/supervision_test.rs"]
mod supervision_test;

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

/// Exercises signed valued-union namespaces through generated package summaries.
#[test]
fn project_vm_tests_execute_imported_signed_valued_union_namespaces() {
    let exit_code = run_project_vm_test(
        "terlan_imported_signed_union_vm_test",
        "signed-union-app",
        "src/app/Constants.terl",
        r#"module app.Constants.
pub type Direction: Int = FORWARD = 1 | BACKWARD = -1.
pub type Mode: Int = FAST = 1 | SAFE = 3.
pub accepts(direction: Direction, mode: Mode): Bool ->
    direction == Direction.BACKWARD and mode == Mode.FAST.
pub (current: Direction) matches(other: Direction): Bool -> current == other.
pub local_receiver(): Bool -> Direction.BACKWARD.matches(Direction.BACKWARD).
"#,
        "tests/app/ConstantsTest.terl",
        r#"module app.ConstantsTest.
import app.Constants.{accepts, matches, local_receiver}.
import type app.Constants.{Direction as Heading, Mode}.
@test
pub imported_signed_arm_executes(): Bool ->
    accepts(Heading.BACKWARD, Mode.FAST).
@test
pub positive_arm_remains_distinct(): Bool ->
    not accepts(Heading.FORWARD, Mode.FAST).
@test
pub imported_signed_pattern_executes(): Bool ->
    case Heading.BACKWARD { Heading.BACKWARD -> true; _ -> false }.
@test
pub local_constant_receiver_executes(): Bool -> local_receiver().
@test
pub selected_constant_receiver_executes(): Bool -> (Heading.BACKWARD).matches(Heading.BACKWARD).
@test
pub qualified_constant_receiver_executes(): Bool -> app.Constants.Direction.FORWARD.matches(Heading.FORWARD).
"#,
    );
    assert_eq!(exit_code, ExitCode::SUCCESS);
}

/// Constructor-backed closed unions remain exhaustive across package summaries.
#[test]
fn project_vm_tests_execute_imported_constructor_union_patterns() {
    let exit_code = run_project_vm_test(
        "terlan_constructor_union_coverage_vm_test",
        "constructor-union-app",
        "src/app/Events.terl",
        r#"module app.Events.
pub type Reason: Int = READY = 1 | WAITING = 2.
pub struct Payload { value: Int }.
pub payload(value: Int): Payload -> Payload {value: value}.
pub type Message = {Atom["idle"], Int, Reason} | {Atom["send_packet"], Payload}
    | {Atom["http_error"], Int} | {Atom["frame_2_d"], Bool}.
pub constructor Idle {
    (sequence: Int, reason: Reason): Message -> {Atom["idle"], sequence, reason}
}.
pub constructor SendPacket {
    (payload: Payload): Message -> {Atom["send_packet"], payload}
}.
pub constructor HTTPError {
    (code: Int): Message -> {Atom["http_error"], code}
}.
pub constructor Frame2D {
    (flag: Bool): Message -> {Atom["frame_2_d"], flag}
}.
"#,
        "tests/app/EventsTest.terl",
        r#"module app.EventsTest.
import app.Events.{Frame2D, HTTPError, Idle, SendPacket, payload}.
import type app.Events.{Reason, Payload, Message}.
pub inspect(message: Message): Bool -> case message {
    Idle(sequence, reason) -> sequence == 7 and reason == Reason.READY;
    SendPacket(payload) -> payload.value == 9;
    HTTPError(code) -> code == 13;
    Frame2D(flag) -> flag
}.
@test
pub imported_idle_variant_executes(): Bool -> inspect(Idle(7, Reason.READY)).
@test
pub imported_multiword_variant_executes(): Bool -> inspect(SendPacket(payload(9))).
@test
pub imported_acronym_variant_executes(): Bool -> inspect(HTTPError(13)).
@test
pub imported_digit_boundary_variant_executes(): Bool -> inspect(Frame2D(true)).
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
    run_multi_module_project_vm_test(
        prefix,
        package,
        &[(source_path, source)],
        test_path,
        test_source,
    )
}

fn run_multi_module_project_vm_test(
    prefix: &str,
    package: &str,
    sources: &[(&str, &str)],
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
    for (source_path, source) in sources {
        write_project_file(&root, source_path, source);
    }
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
