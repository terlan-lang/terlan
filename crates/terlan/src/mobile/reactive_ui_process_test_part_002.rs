
/// Verifies native reply fixtures replay in declaration order.
///
/// Inputs:
/// - Two native reply fixtures for the same process reply.
///
/// Output:
/// - Schema-versioned replay messages in fixture declaration order.
///
/// Transformation:
/// - Converts fixture-supplied native bridge replies into deterministic process
///   reply messages for process-driven UI tests.
#[test]
fn reactive_ui_process_replays_native_reply_fixtures_deterministically() {
    let replay = replay_reactive_ui_native_reply_fixtures(
        &[counter_process()],
        &[
            navigation_reply_fixture("reply-1", true),
            navigation_reply_fixture("reply-2", false),
        ],
    )
    .expect("fixture replay");

    assert_eq!(replay.schema_version, 1);
    assert_eq!(replay.messages.len(), 2);
    assert_eq!(replay.messages[0].reply, "navigationComplete");
    assert_eq!(
        replay.messages[0].payload[0].value,
        MobileAngularBridgeValue::Bool(true)
    );
    assert_eq!(
        replay.messages[1].payload[0].value,
        MobileAngularBridgeValue::Bool(false)
    );
}

/// Verifies native reply fixtures reject duplicate ids and malformed payloads.
///
/// Inputs:
/// - Duplicate fixture ids and one payload that violates the process reply
///   contract.
///
/// Output:
/// - Stable duplicate-fixture and reply-payload diagnostics.
///
/// Transformation:
/// - Keeps fixture-driven process tests deterministic and typechecked.
#[test]
fn reactive_ui_process_rejects_malformed_native_reply_fixtures() {
    let mut bad_payload = navigation_reply_fixture("reply-3", true);
    bad_payload.payload = vec![event_payload(
        "ok",
        MobileAngularBridgeValue::String("true".to_string()),
    )];

    let diagnostics = replay_reactive_ui_native_reply_fixtures(
        &[counter_process()],
        &[
            navigation_reply_fixture("reply-1", true),
            navigation_reply_fixture("reply-1", false),
            bad_payload,
            ReactiveUiNativeReplyFixture {
                fixture_id: String::new(),
                process: "CounterProcess".to_string(),
                reply: "navigationComplete".to_string(),
                payload: vec![event_payload("ok", MobileAngularBridgeValue::Bool(true))],
            },
        ],
    )
    .expect_err("malformed fixtures");
    let codes = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect::<Vec<_>>();

    assert!(codes.contains(&"reactive_ui_process_duplicate_reply_fixture_id"));
    assert!(codes.contains(&"reactive_ui_process_reply_payload_type_mismatch"));
    assert!(codes.contains(&"reactive_ui_process_empty_reply_fixture_id"));
}

/// Verifies the first full reactive UI process cycle is deterministic.
///
/// Inputs:
/// - Initial process state, state binding metadata, AngularTS event payload,
///   a typed native command effect, a native reply fixture, and a state patch.
///
/// Output:
/// - Updated process state and typed messages at each boundary.
///
/// Transformation:
/// - Exercises process state -> AngularTS render binding -> component event ->
///   process message -> native command effect -> native reply fixture ->
///   process reply message -> process state update.
#[test]
fn reactive_ui_process_runs_full_cycle_with_native_reply_fixture() {
    let contracts = [counter_process()];
    let initial_state = counter_state(0);
    let state_bindings =
        generate_reactive_ui_state_binding_metadata(&contracts, &[counter_count_binding()])
            .expect("state binding metadata");
    let event_message = encode_reactive_ui_event_message(
        &contracts,
        "CounterProcess",
        "increment",
        &[event_payload("amount", MobileAngularBridgeValue::Int(1))],
    )
    .expect("event message");
    let native_effect = reactive_ui_native_component_command_effect("openCounterMenu");
    let native_replies = replay_reactive_ui_native_reply_fixtures(
        &contracts,
        &[navigation_reply_fixture("reply-1", true)],
    )
    .expect("reply fixture replay");
    let updated_state = apply_reactive_ui_state_patch(
        &contracts,
        &initial_state,
        "patchCount",
        &[event_payload("count", MobileAngularBridgeValue::Int(1))],
    )
    .expect("state patch");

    assert_eq!(state_bindings.bindings[0].component_id, "counter-label");
    assert_eq!(state_bindings.bindings[0].state_type, "Int");
    assert_eq!(event_message.event, "increment");
    assert_eq!(
        event_message.payload[0].value,
        MobileAngularBridgeValue::Int(1)
    );
    assert_eq!(
        native_effect.required_capability,
        Some(MobileBridgeCapability::NativeComponents)
    );
    assert_eq!(native_replies.messages[0].reply, "navigationComplete");
    assert_eq!(
        native_replies.messages[0].payload[0].value,
        MobileAngularBridgeValue::Bool(true)
    );
    assert_eq!(updated_state.fields[0].name, "count");
    assert_eq!(
        updated_state.fields[0].value,
        MobileAngularBridgeValue::Int(1)
    );
}

/// Verifies state patch application rejects stale or mismatched fields.
///
/// Inputs:
/// - A valid process state plus bad patch payloads.
///
/// Output:
/// - Stable state patch diagnostics.
///
/// Transformation:
/// - Prevents full-cycle tests from silently accepting stale state update
///   payloads.
#[test]
fn reactive_ui_process_rejects_bad_state_patch_payloads() {
    let contracts = [counter_process()];
    let unknown = apply_reactive_ui_state_patch(
        &contracts,
        &counter_state(0),
        "patchCount",
        &[event_payload("missing", MobileAngularBridgeValue::Int(1))],
    )
    .expect_err("unknown patch field");
    let type_mismatch = apply_reactive_ui_state_patch(
        &contracts,
        &counter_state(0),
        "patchCount",
        &[event_payload(
            "count",
            MobileAngularBridgeValue::String("1".to_string()),
        )],
    )
    .expect_err("patch type mismatch");
    let wrong_effect = apply_reactive_ui_state_patch(
        &contracts,
        &counter_state(0),
        "navigateHome",
        &[event_payload("count", MobileAngularBridgeValue::Int(1))],
    )
    .expect_err("non state patch");

    assert_eq!(
        unknown.code,
        "reactive_ui_process_unknown_state_patch_field"
    );
    assert_eq!(
        type_mismatch.code,
        "reactive_ui_process_state_patch_type_mismatch"
    );
    assert_eq!(
        wrong_effect.code,
        "reactive_ui_process_non_state_patch_effect"
    );
}

/// Verifies effect type names are stable.
///
/// Inputs:
/// - Every first-slice reactive UI effect type.
///
/// Output:
/// - Stable metadata spelling for each effect type.
///
/// Transformation:
/// - Keeps process metadata and future AngularTS/native-shell dispatch aligned.
#[test]
fn reactive_ui_process_effect_type_names_are_stable() {
    assert_eq!(ReactiveUiEffectType::StatePatch.as_str(), "state_patch");
    assert_eq!(ReactiveUiEffectType::Navigation.as_str(), "navigation");
    assert_eq!(
        ReactiveUiEffectType::NativeCommand.as_str(),
        "native_command"
    );
    assert_eq!(
        ReactiveUiEffectType::PlatformPermission.as_str(),
        "platform_permission"
    );
    assert_eq!(
        ReactiveUiEffectType::NativeResource.as_str(),
        "native_resource"
    );
    assert_eq!(ReactiveUiBindingKind::Text.as_str(), "text");
    assert_eq!(ReactiveUiBindingKind::Prop.as_str(), "prop");
    assert_eq!(ReactiveUiBindingKind::Attribute.as_str(), "attribute");
    assert_eq!(ReactiveUiBindingKind::Class.as_str(), "class");
    assert_eq!(ReactiveUiBindingKind::Style.as_str(), "style");
}
