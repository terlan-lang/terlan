use super::{
    generate_vm_live_template_angular_ts_runtime_module,
    generate_vm_live_template_protocol_manifest,
    generate_vm_live_template_wasm_protocol_binding_module,
    validate_vm_live_template_js_protocol_binding, validate_vm_live_template_protocol_manifest,
    validate_vm_live_template_rolling_deploy_compatibility,
    validate_vm_live_template_wasm_protocol_binding, VmLiveTemplateProtocolEvent,
    VmLiveTemplateRollingDeployCompatibilityPlan, VM_LIVE_TEMPLATE_PROTOCOL_SCHEMA,
    VM_LIVE_TEMPLATE_PROTOCOL_VERSION,
};

#[test]
fn vm_live_template_protocol_manifest_lists_required_events_and_schema_hash() {
    let manifest = generate_vm_live_template_protocol_manifest();

    assert_eq!(manifest.schema, VM_LIVE_TEMPLATE_PROTOCOL_SCHEMA);
    assert_eq!(manifest.version, VM_LIVE_TEMPLATE_PROTOCOL_VERSION);
    assert_eq!(manifest.schema_hash.len(), 16);
    assert!(manifest
        .schema_hash
        .chars()
        .all(|character| character.is_ascii_hexdigit()));
    assert_eq!(
        manifest
            .events
            .iter()
            .map(|event| event.name)
            .collect::<Vec<_>>(),
        vec![
            "initialRender",
            "incrementalPatch",
            "commandPostback",
            "errorPatch",
            "redirectPatch",
            "reconnectToken",
            "heartbeat",
            "clientCancellation",
            "backpressureSignal",
            "versionNegotiation",
        ]
    );

    let incremental_patch = manifest
        .events
        .iter()
        .find(|event| event.name == "incrementalPatch")
        .expect("incremental patch event");
    assert_eq!(
        incremental_patch.required_fields,
        &[
            "protocolVersion",
            "assetHash",
            "patchId",
            "modelVersion",
            "patches",
        ]
    );

    validate_vm_live_template_protocol_manifest(&manifest).expect("valid manifest");
}

#[test]
fn vm_live_template_protocol_manifest_rejects_duplicate_event_names() {
    let mut manifest = generate_vm_live_template_protocol_manifest();
    manifest.events.push(VmLiveTemplateProtocolEvent {
        name: "heartbeat",
        required_fields: &["protocolVersion"],
    });
    manifest.schema_hash = "stale".to_string();

    let stale_hash_error =
        validate_vm_live_template_protocol_manifest(&manifest).expect_err("stale hash");
    assert!(stale_hash_error.contains("schema hash is stale"));

    manifest = generate_vm_live_template_protocol_manifest();
    let heartbeat = manifest
        .events
        .iter()
        .find(|event| event.name == "heartbeat")
        .expect("heartbeat")
        .clone();
    manifest.events.push(heartbeat);
    manifest.schema_hash = super::live_template_protocol_schema_hash(
        manifest.schema,
        manifest.version,
        &manifest.events,
    );

    let duplicate_error =
        validate_vm_live_template_protocol_manifest(&manifest).expect_err("duplicate event");
    assert!(duplicate_error.contains("duplicate protocol event `heartbeat`"));
}

#[test]
fn vm_live_template_protocol_generates_angular_ts_browser_runtime_module() {
    let manifest = generate_vm_live_template_protocol_manifest();
    let module = generate_vm_live_template_angular_ts_runtime_module(&manifest)
        .expect("generated Angular.ts runtime module");

    assert_eq!(module.path, "generated/terlan/live-template-protocol.mjs");
    assert!(module.source.contains("angular-wave/angular.ts"));
    assert!(module.source.contains("terlanLiveTemplateProtocolManifest"));
    assert!(module.source.contains("validateTerlanLiveTemplateEvent"));
    assert!(module.source.contains("connectTerlanLiveTemplateSse"));
    assert!(module.source.contains("SseProtocolMessage"));
    assert!(module.source.contains("eventTypes"));
    assert!(module.source.contains("onEvent"));
    assert!(module.source.contains(&manifest.schema_hash));
    for event in &manifest.events {
        assert!(module.source.contains(event.name));
        for field in event.required_fields {
            assert!(module.source.contains(field));
        }
    }
    assert!(!module.source.contains("SseProvider.open"));
    assert!(!module.source.contains("addEventListener"));
    assert!(!module.source.contains("TODO"));
}

#[test]
fn vm_live_template_protocol_executes_through_angular_ts_sse_service() {
    let manifest = generate_vm_live_template_protocol_manifest();
    let module = generate_vm_live_template_angular_ts_runtime_module(&manifest)
        .expect("generated Angular.ts runtime module");
    let script = format!(
        r#"{}
let observedUrl;
let observedConfig;
let closeCount = 0;
const connection = {{
  close() {{ closeCount += 1; }},
  reconnect() {{}},
}};
const $sse = (url, config) => {{
  observedUrl = url;
  observedConfig = config;
  return connection;
}};
const dispatched = [];
const returned = connectTerlanLiveTemplateSse($sse, "/events", (message) => dispatched.push(message));
if (returned !== connection) throw new Error("managed connection was not returned");
if (observedUrl !== "/events") throw new Error(`unexpected URL: ${{observedUrl}}`);
if (JSON.stringify(observedConfig.eventTypes) !== '["SseProtocolMessage"]') {{
  throw new Error(`unexpected event types: ${{JSON.stringify(observedConfig.eventTypes)}}`);
}}
const patch = {{
  type: "incrementalPatch",
  protocolVersion: 1,
  assetHash: "asset-v1",
  patchId: "patch-1",
  modelVersion: 2,
  patches: [],
}};
observedConfig.onEvent({{ type: "message", data: patch }});
if (dispatched.length !== 0) throw new Error("default SSE message was dispatched");
observedConfig.onEvent({{ type: "SseProtocolMessage", data: patch }});
if (dispatched.length !== 1 || dispatched[0] !== patch) {{
  throw new Error("validated protocol patch was not dispatched");
}}
let rejectedMalformedPatch = false;
try {{
  observedConfig.onEvent({{
    type: "SseProtocolMessage",
    data: {{ type: "incrementalPatch" }},
  }});
}} catch (error) {{
  rejectedMalformedPatch = String(error.message).includes("missing required field: protocolVersion");
}}
if (!rejectedMalformedPatch) throw new Error("malformed protocol patch was accepted");
returned.close();
if (closeCount !== 1) throw new Error("managed connection close was not delegated");
"#,
        module.source
    );

    let output = match std::process::Command::new("node")
        .args(["--input-type=module", "--eval", &script])
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => panic!("run generated Angular.ts SSE adapter: {error}"),
    };
    assert!(
        output.status.success(),
        "generated Angular.ts SSE adapter failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn vm_live_template_protocol_rejects_angular_ts_runtime_generation_for_stale_manifest() {
    let mut manifest = generate_vm_live_template_protocol_manifest();
    manifest.schema_hash = "stale".to_string();

    let error =
        generate_vm_live_template_angular_ts_runtime_module(&manifest).expect_err("stale manifest");

    assert!(error.contains("schema hash is stale"));
}

#[test]
fn vm_live_template_protocol_validates_generated_js_protocol_binding() {
    let manifest = generate_vm_live_template_protocol_manifest();
    let module = generate_vm_live_template_angular_ts_runtime_module(&manifest)
        .expect("generated Angular.ts runtime module");

    let validation = validate_vm_live_template_js_protocol_binding(&manifest, &module)
        .expect("valid JS protocol binding");

    assert_eq!(validation.target, "js.browser");
    assert_eq!(validation.checked_events, 10);
    assert_eq!(
        validation.checked_fields,
        manifest
            .events
            .iter()
            .map(|event| event.required_fields.len())
            .sum::<usize>()
    );
}

#[test]
fn vm_live_template_protocol_rejects_generated_js_binding_missing_event_field() {
    let manifest = generate_vm_live_template_protocol_manifest();
    let mut module = generate_vm_live_template_angular_ts_runtime_module(&manifest)
        .expect("generated Angular.ts runtime module");
    module.source = module.source.replace("\"patches\"", "\"patchesMissing\"");

    let error = validate_vm_live_template_js_protocol_binding(&manifest, &module)
        .expect_err("missing field");

    assert!(error.contains("generated JS binding is missing field `patches`"));
}

#[test]
fn vm_live_template_protocol_generates_wasm_protocol_binding_manifest() {
    let manifest = generate_vm_live_template_protocol_manifest();
    let module = generate_vm_live_template_wasm_protocol_binding_module(&manifest)
        .expect("generated Wasm protocol binding manifest");

    assert_eq!(
        module.path,
        "generated/terlan/live-template-protocol.wasm.json"
    );
    assert!(module.manifest.contains("\"target\": \"wasm.core\""));
    assert!(module.manifest.contains("\"terlan_live_template\""));
    assert!(module.manifest.contains("\"dispatch_event\""));
    assert!(module
        .manifest
        .contains("\"terlan_live_template_validate_event\""));
    assert!(module.manifest.contains(&manifest.schema_hash));
    for event in &manifest.events {
        assert!(module.manifest.contains(event.name));
        for field in event.required_fields {
            assert!(module.manifest.contains(field));
        }
    }
    assert!(!module.manifest.contains("TODO"));
}

#[test]
fn vm_live_template_protocol_validates_generated_wasm_protocol_binding() {
    let manifest = generate_vm_live_template_protocol_manifest();
    let module = generate_vm_live_template_wasm_protocol_binding_module(&manifest)
        .expect("generated Wasm protocol binding manifest");

    let validation = validate_vm_live_template_wasm_protocol_binding(&manifest, &module)
        .expect("valid Wasm protocol binding");

    assert_eq!(validation.target, "wasm.core");
    assert_eq!(validation.checked_events, 10);
    assert_eq!(validation.checked_exports, 3);
    assert_eq!(
        validation.checked_fields,
        manifest
            .events
            .iter()
            .map(|event| event.required_fields.len())
            .sum::<usize>()
    );
}

#[test]
fn vm_live_template_protocol_rejects_generated_wasm_binding_missing_export() {
    let manifest = generate_vm_live_template_protocol_manifest();
    let mut module = generate_vm_live_template_wasm_protocol_binding_module(&manifest)
        .expect("generated Wasm protocol binding manifest");
    module.manifest = module.manifest.replace(
        "terlan_live_template_validate_event",
        "missing_validate_event",
    );

    let error = validate_vm_live_template_wasm_protocol_binding(&manifest, &module)
        .expect_err("missing export");

    assert!(error.contains(
        "generated Wasm binding is missing export `terlan_live_template_validate_event`"
    ));
}

#[test]
fn vm_live_template_protocol_accepts_mixed_version_rolling_deploy_compatibility() {
    let manifest = generate_vm_live_template_protocol_manifest();
    let plan = VmLiveTemplateRollingDeployCompatibilityPlan {
        previous_protocol_version: manifest.version,
        next_protocol_version: manifest.version + 1,
        min_supported_version: manifest.version,
        max_supported_version: manifest.version + 1,
        previous_schema_hash: manifest.schema_hash.clone(),
        next_schema_hash: manifest.schema_hash,
        previous_asset_hash: "asset-v1".to_string(),
        next_asset_hash: "asset-v2".to_string(),
    };

    let validation = validate_vm_live_template_rolling_deploy_compatibility(&plan)
        .expect("compatible rolling deploy");

    assert_eq!(validation.negotiated_protocol_version, 1);
    assert!(validation.schema_compatible);
    assert!(validation.asset_hash_rotated);
}

#[test]
fn vm_live_template_protocol_rejects_mixed_version_rolling_deploy_schema_drift() {
    let manifest = generate_vm_live_template_protocol_manifest();
    let plan = VmLiveTemplateRollingDeployCompatibilityPlan {
        previous_protocol_version: manifest.version,
        next_protocol_version: manifest.version + 1,
        min_supported_version: manifest.version,
        max_supported_version: manifest.version + 1,
        previous_schema_hash: manifest.schema_hash,
        next_schema_hash: "different-schema".to_string(),
        previous_asset_hash: "asset-v1".to_string(),
        next_asset_hash: "asset-v2".to_string(),
    };

    let error =
        validate_vm_live_template_rolling_deploy_compatibility(&plan).expect_err("schema drift");

    assert!(error.contains("rolling deploy requires matching protocol schema hashes"));
}

#[test]
fn vm_live_template_protocol_rejects_mixed_version_rolling_deploy_stale_assets() {
    let manifest = generate_vm_live_template_protocol_manifest();
    let plan = VmLiveTemplateRollingDeployCompatibilityPlan {
        previous_protocol_version: manifest.version,
        next_protocol_version: manifest.version + 1,
        min_supported_version: manifest.version,
        max_supported_version: manifest.version + 1,
        previous_schema_hash: manifest.schema_hash.clone(),
        next_schema_hash: manifest.schema_hash,
        previous_asset_hash: "same-asset".to_string(),
        next_asset_hash: "same-asset".to_string(),
    };

    let error =
        validate_vm_live_template_rolling_deploy_compatibility(&plan).expect_err("stale assets");

    assert!(error.contains("protocol version changes must rotate browser asset hashes"));
}

#[test]
fn vm_live_template_protocol_rejects_mixed_version_rolling_deploy_version_window_gap() {
    let manifest = generate_vm_live_template_protocol_manifest();
    let plan = VmLiveTemplateRollingDeployCompatibilityPlan {
        previous_protocol_version: manifest.version,
        next_protocol_version: manifest.version + 2,
        min_supported_version: manifest.version,
        max_supported_version: manifest.version + 1,
        previous_schema_hash: manifest.schema_hash.clone(),
        next_schema_hash: manifest.schema_hash,
        previous_asset_hash: "asset-v1".to_string(),
        next_asset_hash: "asset-v3".to_string(),
    };

    let error =
        validate_vm_live_template_rolling_deploy_compatibility(&plan).expect_err("window gap");

    assert!(error.contains("next protocol version `3` is outside"));
}
