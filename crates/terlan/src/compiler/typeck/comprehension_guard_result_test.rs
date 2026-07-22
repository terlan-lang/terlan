use super::test_support::check_syntax_output;

#[test]
fn syntax_output_accepts_completed_guard_result_filter() {
    let diagnostics = check_syntax_output(
        "\
module comprehension_guard_result.\n\
\n\
pub type CompletedGuardResult = {Atom[\"guard_result\"], value: Bool}.\n\
\n\
completed(value: Bool): CompletedGuardResult ->\n\
    CompletedGuardResult(value).\n\
\n\
pub positives(items: List[Int]): List[Int] ->\n\
    [value | value <- items, completed(value > 0)].\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {diagnostics:?}"
    );
}

#[test]
fn syntax_output_rejects_malformed_guard_result_filter() {
    let diagnostics = check_syntax_output(
        "\
module malformed_comprehension_guard_result.\n\
\n\
pub type InvalidGuardResult = {Atom[\"guard_result\"], value: Int}.\n\
\n\
invalid(value: Int): InvalidGuardResult ->\n\
    InvalidGuardResult(value).\n\
\n\
pub values(items: List[Int]): List[Int] ->\n\
    [value | value <- items, invalid(value)].\n\
",
    );

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("does not implement GuardResult")),
        "diagnostics: {diagnostics:?}"
    );
}

#[test]
fn syntax_output_lifts_effectful_guard_result_through_declared_container() {
    let diagnostics = check_syntax_output(
        r#"
module effectful_comprehension_guard.

pub type Deferred[T] = {Atom["deferred"], value: T}.
pub trait GuardResult[R, F[_]] { into_guard(result: R): F[Bool]. }.
pub impl GuardResult[Deferred[Bool], Deferred] for Deferred[Bool] {
    into_guard(result: Deferred[Bool]): Deferred[Bool] -> result.
}.

defer(value: Bool): Deferred[Bool] -> Deferred(value).

pub values(items: List[Int]): Deferred[List[Int]] ->
    [value | value <- items, defer(value > 0)].
"#,
    );
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

#[test]
fn syntax_output_rejects_effectful_guard_result_in_pure_comprehension_return() {
    let diagnostics = check_syntax_output(
        r#"
module pure_effectful_comprehension_guard.

pub type Deferred[T] = {Atom["deferred"], value: T}.
pub trait GuardResult[R, F[_]] { into_guard(result: R): F[Bool]. }.
pub impl GuardResult[Deferred[Bool], pure_effectful_comprehension_guard.Deferred] for Deferred[Bool] {
    into_guard(result: Deferred[Bool]): Deferred[Bool] -> result.
}.

defer(value: Bool): Deferred[Bool] -> Deferred(value).

pub values(items: List[Int]): List[Int] ->
    [value | value <- items, defer(value > 0)].
"#,
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("expected List[Int]")),
        "diagnostics: {diagnostics:?}"
    );
}

#[test]
fn syntax_output_rejects_conflicting_guard_lift_containers() {
    let diagnostics = check_syntax_output(
        r#"
module conflicting_comprehension_guards.

pub type First[T] = {Atom["first"], value: T}.
pub type Second[T] = {Atom["second"], value: T}.
pub trait GuardResult[R, F[_]] { into_guard(result: R): F[Bool]. }.
pub impl GuardResult[First[Bool], conflicting_comprehension_guards.First] for First[Bool] {
    into_guard(result: First[Bool]): First[Bool] -> result.
}.
pub impl GuardResult[Second[Bool], conflicting_comprehension_guards.Second] for Second[Bool] {
    into_guard(result: Second[Bool]): Second[Bool] -> result.
}.

first(value: Bool): First[Bool] -> First(value).
second(value: Bool): Second[Bool] -> Second(value).

pub values(items: List[Int]): Dynamic ->
    [value | value <- items, first(value > 0), second(value < 10)].
"#,
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("conflicting lift containers")),
        "diagnostics: {diagnostics:?}"
    );
}

#[test]
fn syntax_output_keeps_pattern_guards_pure_bool_only() {
    let diagnostics = check_syntax_output(
        r#"
module effectful_pattern_guard.

pub type Deferred[T] = {Atom["deferred"], value: T}.
defer(value: Bool): Deferred[Bool] -> Deferred(value).
pub guarded(value: Int): Int.
guarded(value) where defer(value > 0) -> value.
"#,
    );
    assert!(
        !diagnostics.is_empty(),
        "effectful pattern guard was accepted"
    );
}
