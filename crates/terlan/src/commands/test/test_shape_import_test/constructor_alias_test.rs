use super::*;

/// Same-named constructors from different providers retain their own identities.
#[test]
fn project_vm_tests_execute_aliased_constructor_union_patterns() {
    let exit = run_multi_module_project_vm_test(
        "terlan_constructor_alias_vm_test",
        "constructor-alias-app",
        &[
            (
                "src/app/First.terl",
                r#"module app.First.
pub type Outcome = {Atom["accepted"], Int} | {Atom["rejected"], Bool}.
pub constructor Accepted { (value: Int): Outcome -> {Atom["accepted"], value} }.
pub constructor Rejected { (flag: Bool): Outcome -> {Atom["rejected"], flag} }.
"#,
            ),
            (
                "src/app/Second.terl",
                r#"module app.Second.
pub type Outcome = {Atom["completed"], String} | {Atom["rejected"], String}.
pub constructor Completed { (value: String): Outcome -> {Atom["completed"], value} }.
pub constructor Rejected { (detail: String): Outcome -> {Atom["rejected"], detail} }.
"#,
            ),
        ],
        "tests/app/AliasTest.terl",
        r#"module app.AliasTest.
import app.First.{Accepted, Rejected as FirstFailure}.
import app.Second.{Completed, Rejected as SecondFailure}.
import type app.First.{Outcome as FirstOutcome}.
import type app.Second.{Outcome as SecondOutcome}.
pub first(value: FirstOutcome): Bool -> case value {
    Accepted(n) -> n == 7;
    FirstFailure(flag) -> flag
}.
pub second(value: SecondOutcome): Bool -> case value {
    SecondFailure(text) -> text == "no";
    Completed(text) -> text == "ok"
}.
@test
pub first_success_executes(): Bool -> first(Accepted(7)).
@test
pub first_alias_executes(): Bool -> first(FirstFailure(true)).
@test
pub second_success_executes(): Bool -> second(Completed("ok")).
@test
pub second_alias_executes(): Bool -> second(SecondFailure("no")).
"#,
    );
    assert_eq!(exit, ExitCode::SUCCESS);
}

/// Nullary source constructors retain identity across selected import aliases.
#[test]
fn project_vm_tests_execute_nullary_constructor_alias_patterns() {
    let exit = run_multi_module_project_vm_test(
        "terlan_nullary_alias_vm_test",
        "nullary-alias-app",
        &[(
            "src/app/State.terl",
            r#"module app.State.
pub type Action = Atom["pause_work"] | {Atom["resume_work"], Int}.
pub constructor PauseWork { (): Action -> Atom["pause_work"] }.
pub constructor ResumeWork { (value: Int): Action -> {Atom["resume_work"], value} }.
pub type Signal = Atom["enabled"] | Atom["disabled"].
pub constructor Enabled { (): Signal -> Atom["enabled"] }.
pub constructor Disabled { (): Signal -> Atom["disabled"] }.
"#,
        )],
        "tests/app/AliasTest.terl",
        r#"module app.AliasTest.
import app.State.{PauseWork as Paused, ResumeWork as Resumed, Enabled as Active, Disabled as Inactive}.
import type app.State.{Action, Signal}.
pub classify(value: Action): Int -> case value {
    Paused() -> 0;
    Resumed(n) -> n
}.
pub enabled(value: Signal): Bool -> case value {
    Active() -> true;
    Inactive() -> false
}.
@test
pub nullary_alias_matches(): Bool -> classify(Paused()) == 0.
@test
pub payload_alias_matches(): Bool -> classify(Resumed(9)) == 9.
@test
pub first_atom_alias_matches(): Bool -> enabled(Active()).
@test
pub second_atom_alias_matches(): Bool -> not enabled(Inactive()).
"#,
    );
    assert_eq!(exit, ExitCode::SUCCESS);
}

/// Local nullary patterns work in both managed and immediate atom unions.
#[test]
fn project_vm_tests_execute_local_nullary_constructor_patterns() {
    let exit = run_project_vm_test(
        "terlan_local_nullary_vm_test",
        "local-nullary-app",
        "src/app/Main.terl",
        "module app.Main.\npub ready(): Bool -> true.\n",
        "tests/language/NullaryConstructorPatternTest.terl",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/language/NullaryConstructorPatternTest.terl"
        )),
    );
    assert_eq!(exit, ExitCode::SUCCESS);
}

#[test]
fn project_vm_tests_execute_constructor_patterns_with_imported_atom_tags() {
    let exit = run_multi_module_project_vm_test(
        "terlan_imported_atom_constructor_vm_test",
        "imported-atom-constructor-app",
        &[
            (
                "src/fixture/Tags.terl",
                include_str!("../../../../../../tests/fixtures/constructor-atom-alias/Tags.terl"),
            ),
            (
                "src/fixture/State.terl",
                include_str!("../../../../../../tests/fixtures/constructor-atom-alias/State.terl"),
            ),
        ],
        "tests/fixture/StateTest.terl",
        include_str!("../../../../../../tests/fixtures/constructor-atom-alias/StateTest.terl"),
    );
    assert_eq!(exit, ExitCode::SUCCESS);
}

/// Shipped atom providers must also survive package-summary loading.
#[test]
fn project_vm_tests_execute_constructor_patterns_with_standard_atom_tags() {
    let provider =
        include_str!("../../../../../../tests/fixtures/constructor-atom-alias/State.terl")
            .replace("fixture.Tags", "std.vm.Cluster")
            .replace("Offline", "Disconnected");
    let tests =
        include_str!("../../../../../../tests/fixtures/constructor-atom-alias/StateTest.terl")
            .replace("Offline", "Disconnected");
    let exit = run_multi_module_project_vm_test(
        "terlan_standard_atom_constructor_vm_test",
        "standard-atom-constructor-app",
        &[("src/fixture/State.terl", &provider)],
        "tests/fixture/StateTest.terl",
        &tests,
    );
    assert_eq!(exit, ExitCode::SUCCESS);
}

#[test]
fn project_vm_tests_execute_nullary_constructor_atom_alias_bodies() {
    let exit = run_project_vm_test(
        "terlan_nullary_atom_body_vm_test",
        "nullary-atom-body-app",
        "src/app/Main.terl",
        "module app.Main.\npub ready(): Bool -> true.\n",
        "tests/language/NullaryConstructorAtomAliasTest.terl",
        include_str!("../../../../../../tests/language/NullaryConstructorAtomAliasTest.terl"),
    );
    assert_eq!(exit, ExitCode::SUCCESS);
}
