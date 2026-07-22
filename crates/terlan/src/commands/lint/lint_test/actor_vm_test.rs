use std::path::Path;

use super::lint_source;

/// Verifies actor handlers prefer patterns over manual tag equality checks.
#[test]
fn lint_reports_manual_message_tag_equality_in_actor_handler() {
    let diagnostics = lint_source(
        Path::new("ChatActor.terl"),
        r#"
module app.ChatActor.

/**
 * Handles inbound room messages.
 */
pub handle_message(message: Message): Unit ->
    if {
        message.kind == "join" -> join(message);
        true -> ignore(message)
    }.
"#,
    );

    let rendered = diagnostics
        .iter()
        .map(super::super::render_diagnostic)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("warning[TL0905:actor-vm.message-tag-equality]"));
    assert!(rendered.contains("prefer pattern or shape matching"));
}

/// Verifies pattern-based handlers satisfy the actor message rule.
#[test]
fn lint_accepts_pattern_matched_actor_message_handler() {
    let diagnostics = lint_source(
        Path::new("ChatActor.terl"),
        r#"
module app.ChatActor.

/**
 * Handles inbound room messages.
 */
pub handle_message(message: Message): Unit ->
    case message {
        {"join", room} -> join(room);
        _ -> ignore(message)
    }.
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0905"),
        "pattern-matched handlers must not trigger actor-vm tag lint: {diagnostics:?}"
    );
}

/// Verifies ordinary classifiers are not treated as actor handlers.
#[test]
fn lint_accepts_tag_equality_outside_actor_handler() {
    let diagnostics = lint_source(
        Path::new("MessageClassifier.terl"),
        r#"
module app.MessageClassifier.

/**
 * Classifies a message without handling actor lifecycle.
 */
pub is_join_message(message: Message): Bool ->
    message.kind == "join".
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0905"),
        "non-handler helpers must not trigger actor-vm tag lint: {diagnostics:?}"
    );
}

/// Verifies test fixtures may compare raw message tags while constructing cases.
#[test]
fn lint_accepts_message_tag_equality_in_test_source() {
    let diagnostics = lint_source(
        Path::new("ChatActorTest.terl"),
        r#"
module app.ChatActorTest.

@test
pub handle_join_fixture_is_stable(): Bool ->
    let message = make_join_message();
    message.kind == "join".
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0905"),
        "test source must not trigger actor-vm tag lint: {diagnostics:?}"
    );
}

/// Verifies lifecycle callbacks name state-typed parameters explicitly.
#[test]
fn lint_reports_actor_lifecycle_state_parameter_without_state_name() {
    let diagnostics = lint_source(
        Path::new("ChatActor.terl"),
        r#"
module app.ChatActor.

/**
 * Handles inbound room messages.
 */
pub handle_message(message: Message, current: ChatState): ChatState ->
    current.
"#,
    );

    let rendered = diagnostics
        .iter()
        .map(super::super::render_diagnostic)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("warning[TL0906:actor-vm.state-parameter-name]"));
    assert!(rendered.contains("should be named `state`"));
}

/// Verifies canonical state parameter names are accepted.
#[test]
fn lint_accepts_actor_lifecycle_state_parameter_with_state_name() {
    let diagnostics = lint_source(
        Path::new("ChatActor.terl"),
        r#"
module app.ChatActor.

/**
 * Handles inbound room messages.
 */
pub handle_message(message: Message, state: ChatState): ChatState ->
    state.
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0906"),
        "canonical state names must not trigger actor-vm state lint: {diagnostics:?}"
    );
}

/// Verifies state-shaped helper parameters outside lifecycle callbacks are ignored.
#[test]
fn lint_accepts_state_typed_parameter_outside_actor_lifecycle() {
    let diagnostics = lint_source(
        Path::new("StateClassifier.terl"),
        r#"
module app.StateClassifier.

/**
 * Classifies state without handling actor lifecycle.
 */
pub classify(current: ChatState): Bool ->
    current.ready.
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0906"),
        "non-lifecycle helpers must not trigger actor-vm state lint: {diagnostics:?}"
    );
}

/// Verifies test fixtures may use compact state fixture names.
#[test]
fn lint_accepts_actor_lifecycle_state_parameter_in_test_source() {
    let diagnostics = lint_source(
        Path::new("ChatActorTest.terl"),
        r#"
module app.ChatActorTest.

pub handle_message(message: Message, current: ChatState): ChatState ->
    current.
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0906"),
        "test source must not trigger actor-vm state lint: {diagnostics:?}"
    );
}
