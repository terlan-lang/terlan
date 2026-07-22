use super::super::mobile_angular_bridge::{
    MobileAngularBridgePayloadField, MobileAngularBridgeValue,
};
use super::super::mobile_bridge::{MobileBridgeCapability, MobileBridgeField, MobileBridgeType};
use super::*;

/// Builds one typed payload field.
fn field(name: &str, field_type: MobileBridgeType) -> MobileBridgeField {
    MobileBridgeField {
        name: name.to_string(),
        field_type,
    }
}

/// Builds one representative counter process contract.
fn counter_process() -> ReactiveUiProcessContract {
    ReactiveUiProcessContract {
        name: "CounterProcess".to_string(),
        state: vec![ReactiveUiStateField {
            name: "count".to_string(),
            field_type: MobileBridgeType::Int,
        }],
        events: vec![ReactiveUiEvent {
            name: "increment".to_string(),
            payload: vec![field("amount", MobileBridgeType::Int)],
        }],
        effects: vec![
            ReactiveUiEffect {
                name: "patchCount".to_string(),
                effect_type: ReactiveUiEffectType::StatePatch,
                required_capability: None,
                payload: vec![field("count", MobileBridgeType::Int)],
            },
            ReactiveUiEffect {
                name: "navigateHome".to_string(),
                effect_type: ReactiveUiEffectType::Navigation,
                required_capability: Some(MobileBridgeCapability::Navigation),
                payload: vec![field("route", MobileBridgeType::String)],
            },
        ],
        replies: vec![ReactiveUiReply {
            name: "navigationComplete".to_string(),
            source_effect: Some("navigateHome".to_string()),
            payload: vec![field("ok", MobileBridgeType::Bool)],
        }],
    }
}

/// Builds one state-to-component binding for the counter process.
fn counter_count_binding() -> ReactiveUiStateBinding {
    ReactiveUiStateBinding {
        process: "CounterProcess".to_string(),
        state_field: "count".to_string(),
        component_id: "counter-label".to_string(),
        component_prop: "text".to_string(),
        binding_kind: ReactiveUiBindingKind::Text,
    }
}

/// Builds one component-event to process-event binding for the counter process.
fn counter_increment_event_binding() -> ReactiveUiEventBinding {
    ReactiveUiEventBinding {
        process: "CounterProcess".to_string(),
        event: "increment".to_string(),
        component_id: "increment-button".to_string(),
        component_event: "press".to_string(),
    }
}

/// Builds one native bridge reply binding for the counter process.
fn counter_navigation_reply_binding() -> ReactiveUiNativeReplyBinding {
    ReactiveUiNativeReplyBinding {
        process: "CounterProcess".to_string(),
        reply: "navigationComplete".to_string(),
        bridge: "ShellBridge".to_string(),
        command: "openRoute".to_string(),
    }
}

/// Builds one named AngularTS payload field.
fn event_payload(name: &str, value: MobileAngularBridgeValue) -> MobileAngularBridgePayloadField {
    MobileAngularBridgePayloadField {
        name: name.to_string(),
        value,
    }
}

/// Builds one native reply fixture for deterministic process tests.
fn navigation_reply_fixture(id: &str, ok: bool) -> ReactiveUiNativeReplyFixture {
    ReactiveUiNativeReplyFixture {
        fixture_id: id.to_string(),
        process: "CounterProcess".to_string(),
        reply: "navigationComplete".to_string(),
        payload: vec![event_payload("ok", MobileAngularBridgeValue::Bool(ok))],
    }
}

/// Builds the initial counter process state.
fn counter_state(count: i64) -> ReactiveUiProcessState {
    ReactiveUiProcessState {
        process: "CounterProcess".to_string(),
        fields: vec![event_payload("count", MobileAngularBridgeValue::Int(count))],
    }
}

/// Verifies reactive UI process metadata generation.
///
/// Inputs:
/// - One process with state, event, effect, and reply declarations.
///
/// Output:
/// - Schema-versioned metadata with stable names and type spellings.
///
/// Transformation:
/// - Converts the canonical process contract into metadata that later
///   AngularTS/native-shell wiring can consume.
#[test]
fn reactive_ui_process_generates_metadata() {
    let metadata =
        generate_reactive_ui_process_metadata(&[counter_process()]).expect("process metadata");

    assert_eq!(metadata.schema_version, 1);
    assert_eq!(metadata.processes.len(), 1);
    let process = &metadata.processes[0];
    assert_eq!(process.name, "CounterProcess");
    assert_eq!(process.state[0].name, "count");
    assert_eq!(process.state[0].field_type, "Int");
    assert_eq!(process.events[0].name, "increment");
    assert_eq!(process.events[0].payload[0].field_type, "Int");
    assert_eq!(process.effects[0].effect_type, "state_patch");
    assert_eq!(process.effects[1].effect_type, "navigation");
    assert_eq!(process.effects[1].required_capability, Some("navigation"));
    assert_eq!(
        process.replies[0].source_effect.as_deref(),
        Some("navigateHome")
    );
}

/// Verifies typed effect helpers generate stable process effects.
///
/// Inputs:
/// - Navigation, native component command, platform permission, and native
///   resource helper constructors.
///
/// Output:
/// - Effects with stable types, required capabilities, and payload field names.
///
/// Transformation:
/// - Gives later process code generation a canonical effect vocabulary instead
///   of hand-built ad hoc effect declarations.
#[test]
fn reactive_ui_process_effect_helpers_generate_typed_effects() {
    let navigation = reactive_ui_navigation_effect("goHome");
    let native_command = reactive_ui_native_component_command_effect("openMenu");
    let permission = reactive_ui_platform_permission_effect("requestCamera");
    let resource = reactive_ui_native_resource_effect("pickFile", MobileBridgeCapability::Files)
        .expect("resource effect");

    assert_eq!(navigation.effect_type, ReactiveUiEffectType::Navigation);
    assert_eq!(
        navigation.required_capability,
        Some(MobileBridgeCapability::Navigation)
    );
    assert_eq!(navigation.payload[0].name, "route");
    assert_eq!(navigation.payload[0].field_type, MobileBridgeType::String);

    assert_eq!(
        native_command.effect_type,
        ReactiveUiEffectType::NativeCommand
    );
    assert_eq!(
        native_command.required_capability,
        Some(MobileBridgeCapability::NativeComponents)
    );
    assert_eq!(native_command.payload[0].name, "component_id");
    assert_eq!(native_command.payload[1].name, "command");
    assert_eq!(native_command.payload[2].field_type, MobileBridgeType::Json);

    assert_eq!(
        permission.effect_type,
        ReactiveUiEffectType::PlatformPermission
    );
    assert_eq!(
        permission.required_capability,
        Some(MobileBridgeCapability::Permissions)
    );
    assert_eq!(permission.payload[0].name, "permission");
    assert_eq!(permission.payload[1].name, "reason");

    assert_eq!(resource.effect_type, ReactiveUiEffectType::NativeResource);
    assert_eq!(
        resource.required_capability,
        Some(MobileBridgeCapability::Files)
    );
    assert_eq!(resource.payload[0].name, "resource");
    assert_eq!(resource.payload[1].name, "action");
    assert_eq!(resource.payload[2].name, "request");
}

/// Verifies typed effect helpers produce valid process contracts.
///
/// Inputs:
/// - One process using every first-slice typed effect helper.
///
/// Output:
/// - Successful process contract validation.
///
/// Transformation:
/// - Ensures helper-generated effects satisfy the same validation path as
///   hand-authored effects.
#[test]
fn reactive_ui_process_effect_helpers_validate() {
    let mut process = counter_process();
    process.effects = vec![
        reactive_ui_navigation_effect("goHome"),
        reactive_ui_native_component_command_effect("openMenu"),
        reactive_ui_platform_permission_effect("requestCamera"),
        reactive_ui_native_resource_effect("pickFile", MobileBridgeCapability::Files)
            .expect("resource effect"),
    ];
    process.replies = vec![];

    assert_eq!(validate_reactive_ui_process_contracts(&[process]), Ok(()));
}

/// Verifies native resource helpers reject non-resource capabilities.
///
/// Inputs:
/// - A native resource helper requested with the navigation capability.
///
/// Output:
/// - Stable invalid-native-resource-capability diagnostic.
///
/// Transformation:
/// - Prevents native resource effects from accidentally using unrelated
///   platform capabilities.
#[test]
fn reactive_ui_process_native_resource_helper_rejects_invalid_capability() {
    let diagnostic =
        reactive_ui_native_resource_effect("badResource", MobileBridgeCapability::Navigation)
            .expect_err("invalid resource capability");

    assert_eq!(
        diagnostic.code,
        "reactive_ui_process_invalid_native_resource_capability"
    );
}

/// Verifies process state can be exposed as AngularTS component bindings.
///
/// Inputs:
/// - One process contract and one state-to-component binding.
///
/// Output:
/// - Schema-versioned binding metadata with the state field type carried
///   through to the component binding entry.
///
/// Transformation:
/// - Maps typed process state into AngularTS component binding metadata before
///   runtime DOM binding exists.
#[test]
fn reactive_ui_process_generates_state_binding_metadata() {
    let metadata = generate_reactive_ui_state_binding_metadata(
        &[counter_process()],
        &[counter_count_binding()],
    )
    .expect("state binding metadata");

    assert_eq!(metadata.schema_version, 1);
    assert_eq!(metadata.bindings.len(), 1);
    let binding = &metadata.bindings[0];
    assert_eq!(binding.process, "CounterProcess");
    assert_eq!(binding.state_field, "count");
    assert_eq!(binding.state_type, "Int");
    assert_eq!(binding.component_id, "counter-label");
    assert_eq!(binding.component_prop, "text");
    assert_eq!(binding.binding_kind, "text");
}

/// Verifies AngularTS events can be bound to process messages.
///
/// Inputs:
/// - One process contract and one component-event binding.
///
/// Output:
/// - Schema-versioned event binding metadata with the process event payload
///   shape carried into the binding entry.
///
/// Transformation:
/// - Maps AngularTS component events into Terlan process message metadata
///   before runtime event dispatch exists.
#[test]
fn reactive_ui_process_generates_event_binding_metadata() {
    let metadata = generate_reactive_ui_event_binding_metadata(
        &[counter_process()],
        &[counter_increment_event_binding()],
    )
    .expect("event binding metadata");

    assert_eq!(metadata.schema_version, 1);
    assert_eq!(metadata.bindings.len(), 1);
    let binding = &metadata.bindings[0];
    assert_eq!(binding.process, "CounterProcess");
    assert_eq!(binding.event, "increment");
    assert_eq!(binding.component_id, "increment-button");
    assert_eq!(binding.component_event, "press");
    assert_eq!(binding.payload[0].name, "amount");
    assert_eq!(binding.payload[0].field_type, "Int");
}

/// Verifies native bridge replies can be bound to process replies.
///
/// Inputs:
/// - One process contract and one native bridge reply binding.
///
/// Output:
/// - Schema-versioned native reply binding metadata with reply payload shape
///   and source effect metadata.
///
/// Transformation:
/// - Maps native bridge command replies into Terlan process reply metadata
///   before runtime bridge dispatch exists.
#[test]
fn reactive_ui_process_generates_native_reply_binding_metadata() {
    let metadata = generate_reactive_ui_native_reply_binding_metadata(
        &[counter_process()],
        &[counter_navigation_reply_binding()],
    )
    .expect("native reply binding metadata");

    assert_eq!(metadata.schema_version, 1);
    assert_eq!(metadata.bindings.len(), 1);
    let binding = &metadata.bindings[0];
    assert_eq!(binding.process, "CounterProcess");
    assert_eq!(binding.reply, "navigationComplete");
    assert_eq!(binding.source_effect.as_deref(), Some("navigateHome"));
    assert_eq!(binding.bridge, "ShellBridge");
    assert_eq!(binding.command, "openRoute");
    assert_eq!(binding.payload[0].name, "ok");
    assert_eq!(binding.payload[0].field_type, "Bool");
}

/// Verifies AngularTS payload values encode as typed process messages.
///
/// Inputs:
/// - One process event and one matching AngularTS payload.
///
/// Output:
/// - Typed process message with stable process/event names and payload values.
///
/// Transformation:
/// - Converts component event data into a Terlan process message while
///   enforcing the process event payload contract.
#[test]
fn reactive_ui_process_encodes_event_message() {
    let message = encode_reactive_ui_event_message(
        &[counter_process()],
        "CounterProcess",
        "increment",
        &[event_payload("amount", MobileAngularBridgeValue::Int(1))],
    )
    .expect("process message");

    assert_eq!(message.process, "CounterProcess");
    assert_eq!(message.event, "increment");
    assert_eq!(message.payload[0].name, "amount");
    assert_eq!(message.payload[0].value, MobileAngularBridgeValue::Int(1));
}

/// Verifies native bridge payload values encode as typed process reply messages.
///
/// Inputs:
/// - One process reply and one matching native bridge payload.
///
/// Output:
/// - Typed process reply message with stable process/reply/source-effect names.
///
/// Transformation:
/// - Converts native bridge reply data into a Terlan process reply message
///   while enforcing the process reply payload contract.
#[test]
fn reactive_ui_process_encodes_native_reply_message() {
    let message = encode_reactive_ui_reply_message(
        &[counter_process()],
        "CounterProcess",
        "navigationComplete",
        &[event_payload("ok", MobileAngularBridgeValue::Bool(true))],
    )
    .expect("process reply message");

    assert_eq!(message.process, "CounterProcess");
    assert_eq!(message.reply, "navigationComplete");
    assert_eq!(message.source_effect.as_deref(), Some("navigateHome"));
    assert_eq!(message.payload[0].name, "ok");
    assert_eq!(
        message.payload[0].value,
        MobileAngularBridgeValue::Bool(true)
    );
}

/// Verifies duplicate process names and local names are rejected.
///
/// Inputs:
/// - Two process contracts with duplicate names and repeated local fields.
///
/// Output:
/// - Stable duplicate diagnostics.
///
/// Transformation:
/// - Prevents ambiguous process metadata before any runtime binding exists.
#[test]
fn reactive_ui_process_rejects_duplicate_names() {
    let mut first = counter_process();
    first.state.push(first.state[0].clone());
    first.events.push(first.events[0].clone());
    first.effects.push(first.effects[0].clone());
    first.replies.push(first.replies[0].clone());
    let duplicate_payload = first.events[0].payload[0].clone();
    first.events[0].payload.push(duplicate_payload);

    let diagnostics = validate_reactive_ui_process_contracts(&[first, counter_process()])
        .expect_err("duplicates");
    let codes = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect::<Vec<_>>();

    assert!(codes.contains(&"reactive_ui_process_duplicate_name"));
    assert!(codes.contains(&"reactive_ui_process_duplicate_state_field"));
    assert!(codes.contains(&"reactive_ui_process_duplicate_event"));
    assert!(codes.contains(&"reactive_ui_process_duplicate_effect"));
    assert!(codes.contains(&"reactive_ui_process_duplicate_reply"));
    assert!(codes.contains(&"reactive_ui_process_duplicate_payload_field"));
}

/// Verifies empty process names and local names are rejected.
///
/// Inputs:
/// - One process with empty process, state, event, effect, reply, and payload
///   names.
///
/// Output:
/// - Stable empty-name diagnostics.
///
/// Transformation:
/// - Keeps generated process metadata addressable by nonblank keys.
#[test]
fn reactive_ui_process_rejects_empty_names() {
    let mut process = counter_process();
    process.name = String::new();
    process.state[0].name = String::new();
    process.events[0].name = String::new();
    process.events[0].payload[0].name = String::new();
    process.effects[0].name = String::new();
    process.replies[0].name = String::new();

    let diagnostics = validate_reactive_ui_process_contracts(&[process]).expect_err("empty names");
    let codes = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect::<Vec<_>>();

    assert!(codes.contains(&"reactive_ui_process_empty_name"));
    assert!(codes.contains(&"reactive_ui_process_empty_state_field"));
    assert!(codes.contains(&"reactive_ui_process_empty_event"));
    assert!(codes.contains(&"reactive_ui_process_empty_effect"));
    assert!(codes.contains(&"reactive_ui_process_empty_reply"));
    assert!(codes.contains(&"reactive_ui_process_empty_payload_field"));
}

/// Verifies native effects require capabilities.
///
/// Inputs:
/// - One navigation effect without its required capability.
///
/// Output:
/// - Stable missing-effect-capability diagnostic.
///
/// Transformation:
/// - Keeps side-effecting UI process output bound to explicit native bridge
///   capability declarations.
#[test]
fn reactive_ui_process_rejects_effect_without_required_capability() {
    let mut process = counter_process();
    process.effects[1].required_capability = None;

    let diagnostics =
        validate_reactive_ui_process_contracts(&[process]).expect_err("missing capability");

    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "reactive_ui_process_missing_effect_capability"));
}

/// Verifies replies can only reference declared effects.
///
/// Inputs:
/// - One reply with a stale source effect name.
///
/// Output:
/// - Stable unknown-reply-effect diagnostic.
///
/// Transformation:
/// - Prevents native/async replies from being disconnected from the effect
///   that produced them.
#[test]
fn reactive_ui_process_rejects_unknown_reply_effect() {
    let mut process = counter_process();
    process.replies[0].source_effect = Some("missingEffect".to_string());

    let diagnostics =
        validate_reactive_ui_process_contracts(&[process]).expect_err("unknown effect");

    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "reactive_ui_process_unknown_reply_effect"));
}

/// Verifies state bindings reject unknown processes and state fields.
///
/// Inputs:
/// - Bindings that reference a missing process and a missing state field.
///
/// Output:
/// - Stable stale-binding diagnostics.
///
/// Transformation:
/// - Prevents AngularTS component bindings from being generated against stale
///   process contracts.
#[test]
fn reactive_ui_process_rejects_unknown_state_bindings() {
    let mut unknown_process = counter_count_binding();
    unknown_process.process = "MissingProcess".to_string();
    let mut unknown_field = counter_count_binding();
    unknown_field.state_field = "missingCount".to_string();

    let diagnostics = validate_reactive_ui_state_bindings(
        &[counter_process()],
        &[unknown_process, unknown_field],
    )
    .expect_err("stale bindings");
    let codes = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect::<Vec<_>>();

    assert!(codes.contains(&"reactive_ui_process_unknown_binding_process"));
    assert!(codes.contains(&"reactive_ui_process_unknown_binding_state_field"));
}

/// Verifies state bindings reject duplicate or empty binding targets.
///
/// Inputs:
/// - Bindings with repeated component targets and blank binding names.
///
/// Output:
/// - Stable duplicate and empty binding diagnostics.
///
/// Transformation:
/// - Keeps generated AngularTS component binding metadata deterministic.
#[test]
fn reactive_ui_process_rejects_malformed_state_bindings() {
    let mut blank = counter_count_binding();
    blank.process = String::new();
    blank.state_field = String::new();
    blank.component_id = String::new();
    blank.component_prop = String::new();

    let diagnostics = validate_reactive_ui_state_bindings(
        &[counter_process()],
        &[counter_count_binding(), counter_count_binding(), blank],
    )
    .expect_err("malformed bindings");
    let codes = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect::<Vec<_>>();

    assert!(codes.contains(&"reactive_ui_process_duplicate_state_binding_target"));
    assert!(codes.contains(&"reactive_ui_process_empty_binding_process"));
    assert!(codes.contains(&"reactive_ui_process_empty_binding_state_field"));
    assert!(codes.contains(&"reactive_ui_process_empty_binding_component_id"));
    assert!(codes.contains(&"reactive_ui_process_empty_binding_component_prop"));
}

/// Verifies event bindings reject unknown processes and events.
///
/// Inputs:
/// - Bindings that reference a missing process and a missing event.
///
/// Output:
/// - Stable stale-event-binding diagnostics.
///
/// Transformation:
/// - Prevents AngularTS event wiring from being generated against stale
///   process contracts.
#[test]
fn reactive_ui_process_rejects_unknown_event_bindings() {
    let mut unknown_process = counter_increment_event_binding();
    unknown_process.process = "MissingProcess".to_string();
    let mut unknown_event = counter_increment_event_binding();
    unknown_event.event = "missingIncrement".to_string();

    let diagnostics = validate_reactive_ui_event_bindings(
        &[counter_process()],
        &[unknown_process, unknown_event],
    )
    .expect_err("stale event bindings");
    let codes = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect::<Vec<_>>();

    assert!(codes.contains(&"reactive_ui_process_unknown_event_binding_process"));
    assert!(codes.contains(&"reactive_ui_process_unknown_event_binding_event"));
}

/// Verifies event bindings reject duplicate or empty binding targets.
///
/// Inputs:
/// - Bindings with repeated component-event targets and blank binding names.
///
/// Output:
/// - Stable duplicate and empty event binding diagnostics.
///
/// Transformation:
/// - Keeps generated AngularTS event binding metadata deterministic.
#[test]
fn reactive_ui_process_rejects_malformed_event_bindings() {
    let mut blank = counter_increment_event_binding();
    blank.process = String::new();
    blank.event = String::new();
    blank.component_id = String::new();
    blank.component_event = String::new();

    let diagnostics = validate_reactive_ui_event_bindings(
        &[counter_process()],
        &[
            counter_increment_event_binding(),
            counter_increment_event_binding(),
            blank,
        ],
    )
    .expect_err("malformed event bindings");
    let codes = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect::<Vec<_>>();

    assert!(codes.contains(&"reactive_ui_process_duplicate_event_binding_target"));
    assert!(codes.contains(&"reactive_ui_process_empty_event_binding_process"));
    assert!(codes.contains(&"reactive_ui_process_empty_event_binding_event"));
    assert!(codes.contains(&"reactive_ui_process_empty_event_binding_component_id"));
    assert!(codes.contains(&"reactive_ui_process_empty_event_binding_component_event"));
}

/// Verifies native reply bindings reject unknown processes and replies.
///
/// Inputs:
/// - Bindings that reference a missing process and a missing reply.
///
/// Output:
/// - Stable stale-native-reply-binding diagnostics.
///
/// Transformation:
/// - Prevents native bridge reply wiring from being generated against stale
///   process contracts.
#[test]
fn reactive_ui_process_rejects_unknown_native_reply_bindings() {
    let mut unknown_process = counter_navigation_reply_binding();
    unknown_process.process = "MissingProcess".to_string();
    let mut unknown_reply = counter_navigation_reply_binding();
    unknown_reply.reply = "missingReply".to_string();

    let diagnostics = validate_reactive_ui_native_reply_bindings(
        &[counter_process()],
        &[unknown_process, unknown_reply],
    )
    .expect_err("stale native reply bindings");
    let codes = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect::<Vec<_>>();

    assert!(codes.contains(&"reactive_ui_process_unknown_native_reply_binding_process"));
    assert!(codes.contains(&"reactive_ui_process_unknown_native_reply_binding_reply"));
}

/// Verifies native reply bindings reject duplicate or empty bridge targets.
///
/// Inputs:
/// - Bindings with repeated bridge-command targets and blank binding names.
///
/// Output:
/// - Stable duplicate and empty native reply binding diagnostics.
///
/// Transformation:
/// - Keeps generated native reply binding metadata deterministic.
#[test]
fn reactive_ui_process_rejects_malformed_native_reply_bindings() {
    let mut blank = counter_navigation_reply_binding();
    blank.process = String::new();
    blank.reply = String::new();
    blank.bridge = String::new();
    blank.command = String::new();

    let diagnostics = validate_reactive_ui_native_reply_bindings(
        &[counter_process()],
        &[
            counter_navigation_reply_binding(),
            counter_navigation_reply_binding(),
            blank,
        ],
    )
    .expect_err("malformed native reply bindings");
    let codes = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect::<Vec<_>>();

    assert!(codes.contains(&"reactive_ui_process_duplicate_native_reply_binding_target"));
    assert!(codes.contains(&"reactive_ui_process_empty_native_reply_binding_process"));
    assert!(codes.contains(&"reactive_ui_process_empty_native_reply_binding_reply"));
    assert!(codes.contains(&"reactive_ui_process_empty_native_reply_binding_bridge"));
    assert!(codes.contains(&"reactive_ui_process_empty_native_reply_binding_command"));
}

/// Verifies event message encoding rejects stale or malformed payloads.
///
/// Inputs:
/// - Missing, unknown, duplicate, and mismatched AngularTS payload values.
///
/// Output:
/// - Stable process message payload diagnostics.
///
/// Transformation:
/// - Prevents malformed AngularTS event payloads from entering Terlan process
///   message handling.
#[test]
fn reactive_ui_process_rejects_event_message_payload_mismatches() {
    let contracts = [counter_process()];
    let missing = encode_reactive_ui_event_message(&contracts, "CounterProcess", "increment", &[])
        .expect_err("missing payload");
    let unknown = encode_reactive_ui_event_message(
        &contracts,
        "CounterProcess",
        "increment",
        &[event_payload("bad", MobileAngularBridgeValue::Int(1))],
    )
    .expect_err("unknown payload");
    let duplicate = encode_reactive_ui_event_message(
        &contracts,
        "CounterProcess",
        "increment",
        &[
            event_payload("amount", MobileAngularBridgeValue::Int(1)),
            event_payload("amount", MobileAngularBridgeValue::Int(1)),
        ],
    )
    .expect_err("duplicate payload");
    let type_mismatch = encode_reactive_ui_event_message(
        &contracts,
        "CounterProcess",
        "increment",
        &[event_payload(
            "amount",
            MobileAngularBridgeValue::String("1".to_string()),
        )],
    )
    .expect_err("type mismatch");

    assert_eq!(missing.code, "reactive_ui_process_missing_message_payload");
    assert_eq!(unknown.code, "reactive_ui_process_unknown_message_payload");
    assert_eq!(
        duplicate.code,
        "reactive_ui_process_duplicate_message_payload"
    );
    assert_eq!(
        type_mismatch.code,
        "reactive_ui_process_message_payload_type_mismatch"
    );
}

/// Verifies reply message encoding rejects stale or malformed payloads.
///
/// Inputs:
/// - Missing, unknown, duplicate, and mismatched native reply payload values.
///
/// Output:
/// - Stable process reply payload diagnostics.
///
/// Transformation:
/// - Prevents malformed native bridge replies from entering Terlan process
///   reply handling.
#[test]
fn reactive_ui_process_rejects_reply_message_payload_mismatches() {
    let contracts = [counter_process()];
    let missing =
        encode_reactive_ui_reply_message(&contracts, "CounterProcess", "navigationComplete", &[])
            .expect_err("missing payload");
    let unknown = encode_reactive_ui_reply_message(
        &contracts,
        "CounterProcess",
        "navigationComplete",
        &[event_payload("bad", MobileAngularBridgeValue::Bool(true))],
    )
    .expect_err("unknown payload");
    let duplicate = encode_reactive_ui_reply_message(
        &contracts,
        "CounterProcess",
        "navigationComplete",
        &[
            event_payload("ok", MobileAngularBridgeValue::Bool(true)),
            event_payload("ok", MobileAngularBridgeValue::Bool(true)),
        ],
    )
    .expect_err("duplicate payload");
    let type_mismatch = encode_reactive_ui_reply_message(
        &contracts,
        "CounterProcess",
        "navigationComplete",
        &[event_payload(
            "ok",
            MobileAngularBridgeValue::String("true".to_string()),
        )],
    )
    .expect_err("type mismatch");

    assert_eq!(missing.code, "reactive_ui_process_missing_reply_payload");
    assert_eq!(unknown.code, "reactive_ui_process_unknown_reply_payload");
    assert_eq!(
        duplicate.code,
        "reactive_ui_process_duplicate_reply_payload"
    );
    assert_eq!(
        type_mismatch.code,
        "reactive_ui_process_reply_payload_type_mismatch"
    );
}
