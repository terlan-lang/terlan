use std::collections::HashSet;

pub(crate) const VM_LIVE_TEMPLATE_PROTOCOL_SCHEMA: &str = "terlan-vm-live-template-protocol-v1";
pub(crate) const VM_LIVE_TEMPLATE_PROTOCOL_VERSION: u32 = 1;

#[cfg(test)]
#[path = "live_template_protocol_test.rs"]
#[cfg(test)]
mod live_template_protocol_test;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmLiveTemplateProtocolEventKind {
    InitialRender,
    IncrementalPatch,
    CommandPostback,
    ErrorPatch,
    RedirectPatch,
    ReconnectToken,
    Heartbeat,
    ClientCancellation,
    BackpressureSignal,
    VersionNegotiation,
}

impl VmLiveTemplateProtocolEventKind {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::InitialRender => "initialRender",
            Self::IncrementalPatch => "incrementalPatch",
            Self::CommandPostback => "commandPostback",
            Self::ErrorPatch => "errorPatch",
            Self::RedirectPatch => "redirectPatch",
            Self::ReconnectToken => "reconnectToken",
            Self::Heartbeat => "heartbeat",
            Self::ClientCancellation => "clientCancellation",
            Self::BackpressureSignal => "backpressureSignal",
            Self::VersionNegotiation => "versionNegotiation",
        }
    }

    pub(crate) fn required_fields(self) -> &'static [&'static str] {
        match self {
            Self::InitialRender => &[
                "protocolVersion",
                "assetHash",
                "templateId",
                "modelVersion",
                "html",
            ],
            Self::IncrementalPatch => &[
                "protocolVersion",
                "assetHash",
                "patchId",
                "modelVersion",
                "patches",
            ],
            Self::CommandPostback => {
                &["protocolVersion", "commandId", "target", "event", "payload"]
            }
            Self::ErrorPatch => &["protocolVersion", "code", "message", "recoverable"],
            Self::RedirectPatch => &["protocolVersion", "location", "replace"],
            Self::ReconnectToken => &["protocolVersion", "token", "expiresAt", "assetHash"],
            Self::Heartbeat => &["protocolVersion", "streamId", "sequence"],
            Self::ClientCancellation => &["protocolVersion", "commandId", "reason"],
            Self::BackpressureSignal => &["protocolVersion", "queuedPatches", "limit"],
            Self::VersionNegotiation => &[
                "protocolVersion",
                "accepted",
                "minVersion",
                "maxVersion",
                "assetHash",
            ],
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmLiveTemplateProtocolEvent {
    pub(crate) name: &'static str,
    pub(crate) required_fields: &'static [&'static str],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmLiveTemplateProtocolManifest {
    pub(crate) schema: &'static str,
    pub(crate) version: u32,
    pub(crate) schema_hash: String,
    pub(crate) events: Vec<VmLiveTemplateProtocolEvent>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct VmLiveTemplateAngularTsRuntimeModule {
    pub(crate) path: &'static str,
    pub(crate) source: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct VmLiveTemplateWasmProtocolBindingModule {
    pub(crate) path: &'static str,
    pub(crate) manifest: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct VmLiveTemplateJsProtocolBindingValidation {
    pub(crate) target: &'static str,
    pub(crate) checked_events: usize,
    pub(crate) checked_fields: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct VmLiveTemplateWasmProtocolBindingValidation {
    pub(crate) target: &'static str,
    pub(crate) checked_events: usize,
    pub(crate) checked_fields: usize,
    pub(crate) checked_exports: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct VmLiveTemplateRollingDeployCompatibilityPlan {
    pub(crate) previous_protocol_version: u32,
    pub(crate) next_protocol_version: u32,
    pub(crate) min_supported_version: u32,
    pub(crate) max_supported_version: u32,
    pub(crate) previous_schema_hash: String,
    pub(crate) next_schema_hash: String,
    pub(crate) previous_asset_hash: String,
    pub(crate) next_asset_hash: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct VmLiveTemplateRollingDeployCompatibilityValidation {
    pub(crate) negotiated_protocol_version: u32,
    pub(crate) schema_compatible: bool,
    pub(crate) asset_hash_rotated: bool,
}

pub(crate) fn generate_vm_live_template_protocol_manifest() -> VmLiveTemplateProtocolManifest {
    let events = vm_live_template_protocol_events()
        .iter()
        .map(|kind| VmLiveTemplateProtocolEvent {
            name: kind.name(),
            required_fields: kind.required_fields(),
        })
        .collect::<Vec<_>>();
    let schema_hash = live_template_protocol_schema_hash(
        VM_LIVE_TEMPLATE_PROTOCOL_SCHEMA,
        VM_LIVE_TEMPLATE_PROTOCOL_VERSION,
        &events,
    );

    VmLiveTemplateProtocolManifest {
        schema: VM_LIVE_TEMPLATE_PROTOCOL_SCHEMA,
        version: VM_LIVE_TEMPLATE_PROTOCOL_VERSION,
        schema_hash,
        events,
    }
}

#[cfg(test)]
pub(crate) fn generate_vm_live_template_angular_ts_runtime_module(
    manifest: &VmLiveTemplateProtocolManifest,
) -> Result<VmLiveTemplateAngularTsRuntimeModule, String> {
    validate_vm_live_template_protocol_manifest(manifest)?;

    let mut events = String::new();
    for event in &manifest.events {
        events.push_str("    {\n");
        events.push_str("      name: ");
        events.push_str(&js_string(event.name));
        events.push_str(",\n      requiredFields: Object.freeze([");
        for (index, field) in event.required_fields.iter().enumerate() {
            if index > 0 {
                events.push_str(", ");
            }
            events.push_str(&js_string(field));
        }
        events.push_str("]),\n    },\n");
    }

    let source = format!(
        r#"// Generated by Terlan. Do not edit by hand.
// Runtime boundary: @angular-wave/angular.ts owns browser DOM/SSE integration.

export const angularTsRuntime = "@angular-wave/angular.ts";

export const terlanLiveTemplateProtocolManifest = Object.freeze({{
  schema: {schema},
  version: {version},
  schemaHash: {schema_hash},
  events: Object.freeze([
{events}  ]),
}});

export function validateTerlanLiveTemplateEvent(message) {{
  if (!message || typeof message !== "object") {{
    return {{ ok: false, reason: "message must be an object" }};
  }}
  const event = terlanLiveTemplateProtocolManifest.events.find((candidate) => candidate.name === message.type);
  if (!event) {{
    return {{ ok: false, reason: `unknown protocol event: ${{message.type}}` }};
  }}
  for (const field of event.requiredFields) {{
    if (!(field in message)) {{
      return {{ ok: false, reason: `missing required field: ${{field}}` }};
    }}
  }}
  return {{ ok: true, event: event.name }};
}}

export function connectTerlanLiveTemplateSse($sse, url, dispatch) {{
  if (typeof $sse !== "function") {{
    throw new TypeError("$sse service is required by the Angular.ts runtime boundary");
  }}
  if (typeof dispatch !== "function") {{
    throw new TypeError("dispatch must be a function");
  }}
  return $sse(url, {{
    eventTypes: Object.freeze(["SseProtocolMessage"]),
    onEvent(event) {{
      if (!event || event.type !== "SseProtocolMessage") {{
        return;
      }}
      const message = event.data;
      const validation = validateTerlanLiveTemplateEvent(message);
      if (!validation.ok) {{
        throw new Error(validation.reason);
      }}
      dispatch(message);
    }},
  }});
}}
"#,
        schema = js_string(manifest.schema),
        version = manifest.version,
        schema_hash = js_string(&manifest.schema_hash),
        events = events,
    );

    Ok(VmLiveTemplateAngularTsRuntimeModule {
        path: "generated/terlan/live-template-protocol.mjs",
        source,
    })
}

#[cfg(test)]
pub(crate) fn generate_vm_live_template_wasm_protocol_binding_module(
    manifest: &VmLiveTemplateProtocolManifest,
) -> Result<VmLiveTemplateWasmProtocolBindingModule, String> {
    validate_vm_live_template_protocol_manifest(manifest)?;

    let mut events = String::new();
    for event in &manifest.events {
        events.push_str("    {\n");
        events.push_str("      \"name\": ");
        events.push_str(&js_string(event.name));
        events.push_str(",\n      \"requiredFields\": [");
        for (index, field) in event.required_fields.iter().enumerate() {
            if index > 0 {
                events.push_str(", ");
            }
            events.push_str(&js_string(field));
        }
        events.push_str("]\n    },\n");
    }

    let wasm_manifest = format!(
        r#"{{
  "schema": {schema},
  "version": {version},
  "schemaHash": {schema_hash},
  "target": "wasm.core",
  "memory": {{
    "minimumPages": 1,
    "maximumPages": 16
  }},
  "imports": [
    {{
      "module": "terlan_live_template",
      "name": "dispatch_event",
      "params": ["i32", "i32"],
      "results": ["i32"]
    }}
  ],
  "exports": [
    {{
      "name": "terlan_live_template_protocol_version",
      "params": [],
      "results": ["i32"]
    }},
    {{
      "name": "terlan_live_template_validate_event",
      "params": ["i32", "i32"],
      "results": ["i32"]
    }},
    {{
      "name": "terlan_live_template_required_field_count",
      "params": ["i32"],
      "results": ["i32"]
    }}
  ],
  "events": [
{events}  ]
}}
"#,
        schema = js_string(manifest.schema),
        version = manifest.version,
        schema_hash = js_string(&manifest.schema_hash),
        events = events,
    );

    Ok(VmLiveTemplateWasmProtocolBindingModule {
        path: "generated/terlan/live-template-protocol.wasm.json",
        manifest: wasm_manifest,
    })
}

#[cfg(test)]
pub(crate) fn validate_vm_live_template_js_protocol_binding(
    manifest: &VmLiveTemplateProtocolManifest,
    module: &VmLiveTemplateAngularTsRuntimeModule,
) -> Result<VmLiveTemplateJsProtocolBindingValidation, String> {
    validate_vm_live_template_protocol_manifest(manifest)?;

    if !module.path.ends_with(".mjs") {
        return Err(
            "error[vm_live_template_protocol]: generated JS binding must be an ES module"
                .to_string(),
        );
    }
    for required in [
        "@angular-wave/angular.ts",
        "terlanLiveTemplateProtocolManifest",
        "validateTerlanLiveTemplateEvent",
        "connectTerlanLiveTemplateSse",
        "SseProtocolMessage",
        "eventTypes",
        "onEvent",
        "Object.freeze",
        manifest.schema,
        &manifest.schema_hash,
    ] {
        if !module.source.contains(required) {
            return Err(format!(
                "error[vm_live_template_protocol]: generated JS binding is missing `{required}`"
            ));
        }
    }
    if module.source.contains("TODO") || module.source.contains("throw new Error(\"not implemented")
    {
        return Err(
            "error[vm_live_template_protocol]: generated JS binding contains placeholder code"
                .to_string(),
        );
    }

    let mut checked_fields = 0;
    for event in &manifest.events {
        if !module.source.contains(&js_string(event.name)) {
            return Err(format!(
                "error[vm_live_template_protocol]: generated JS binding is missing event `{}`",
                event.name
            ));
        }
        for field in event.required_fields {
            checked_fields += 1;
            if !module.source.contains(&js_string(field)) {
                return Err(format!(
                    "error[vm_live_template_protocol]: generated JS binding is missing field `{}` for event `{}`",
                    field, event.name
                ));
            }
        }
    }

    Ok(VmLiveTemplateJsProtocolBindingValidation {
        target: "js.browser",
        checked_events: manifest.events.len(),
        checked_fields,
    })
}

#[cfg(test)]
pub(crate) fn validate_vm_live_template_wasm_protocol_binding(
    manifest: &VmLiveTemplateProtocolManifest,
    module: &VmLiveTemplateWasmProtocolBindingModule,
) -> Result<VmLiveTemplateWasmProtocolBindingValidation, String> {
    validate_vm_live_template_protocol_manifest(manifest)?;

    if !module.path.ends_with(".wasm.json") {
        return Err(
            "error[vm_live_template_protocol]: generated Wasm binding must use a Wasm manifest"
                .to_string(),
        );
    }

    let required_exports = [
        "terlan_live_template_protocol_version",
        "terlan_live_template_validate_event",
        "terlan_live_template_required_field_count",
    ];
    for required in [
        "wasm.core",
        "terlan_live_template",
        "dispatch_event",
        "minimumPages",
        "maximumPages",
        "i32",
        manifest.schema,
        &manifest.schema_hash,
    ] {
        if !module.manifest.contains(required) {
            return Err(format!(
                "error[vm_live_template_protocol]: generated Wasm binding is missing `{required}`"
            ));
        }
    }
    for export in required_exports {
        if !module.manifest.contains(export) {
            return Err(format!(
                "error[vm_live_template_protocol]: generated Wasm binding is missing export `{export}`"
            ));
        }
    }
    if module.manifest.contains("TODO")
        || module
            .manifest
            .contains("throw new Error(\"not implemented")
    {
        return Err(
            "error[vm_live_template_protocol]: generated Wasm binding contains placeholder code"
                .to_string(),
        );
    }

    let mut checked_fields = 0;
    for event in &manifest.events {
        if !module.manifest.contains(&js_string(event.name)) {
            return Err(format!(
                "error[vm_live_template_protocol]: generated Wasm binding is missing event `{}`",
                event.name
            ));
        }
        for field in event.required_fields {
            checked_fields += 1;
            if !module.manifest.contains(&js_string(field)) {
                return Err(format!(
                    "error[vm_live_template_protocol]: generated Wasm binding is missing field `{}` for event `{}`",
                    field, event.name
                ));
            }
        }
    }

    Ok(VmLiveTemplateWasmProtocolBindingValidation {
        target: "wasm.core",
        checked_events: manifest.events.len(),
        checked_fields,
        checked_exports: required_exports.len(),
    })
}

#[cfg(test)]
pub(crate) fn validate_vm_live_template_rolling_deploy_compatibility(
    plan: &VmLiveTemplateRollingDeployCompatibilityPlan,
) -> Result<VmLiveTemplateRollingDeployCompatibilityValidation, String> {
    if plan.min_supported_version == 0 || plan.max_supported_version == 0 {
        return Err(
            "error[vm_live_template_protocol]: rolling deploy protocol versions must be non-zero"
                .to_string(),
        );
    }
    if plan.min_supported_version > plan.max_supported_version {
        return Err(
            "error[vm_live_template_protocol]: rolling deploy protocol version window is invalid"
                .to_string(),
        );
    }
    if plan.previous_protocol_version < plan.min_supported_version
        || plan.previous_protocol_version > plan.max_supported_version
    {
        return Err(format!(
            "error[vm_live_template_protocol]: previous protocol version `{}` is outside the rolling deploy compatibility window",
            plan.previous_protocol_version
        ));
    }
    if plan.next_protocol_version < plan.min_supported_version
        || plan.next_protocol_version > plan.max_supported_version
    {
        return Err(format!(
            "error[vm_live_template_protocol]: next protocol version `{}` is outside the rolling deploy compatibility window",
            plan.next_protocol_version
        ));
    }
    if plan.previous_schema_hash != plan.next_schema_hash {
        return Err(
            "error[vm_live_template_protocol]: rolling deploy requires matching protocol schema hashes"
                .to_string(),
        );
    }
    if plan.previous_asset_hash.trim().is_empty() || plan.next_asset_hash.trim().is_empty() {
        return Err(
            "error[vm_live_template_protocol]: rolling deploy asset hashes must be non-empty"
                .to_string(),
        );
    }
    if plan.previous_asset_hash == plan.next_asset_hash
        && plan.previous_protocol_version != plan.next_protocol_version
    {
        return Err(
            "error[vm_live_template_protocol]: protocol version changes must rotate browser asset hashes"
                .to_string(),
        );
    }

    Ok(VmLiveTemplateRollingDeployCompatibilityValidation {
        negotiated_protocol_version: plan
            .previous_protocol_version
            .min(plan.next_protocol_version),
        schema_compatible: true,
        asset_hash_rotated: plan.previous_asset_hash != plan.next_asset_hash,
    })
}

pub(crate) fn validate_vm_live_template_protocol_manifest(
    manifest: &VmLiveTemplateProtocolManifest,
) -> Result<(), String> {
    if manifest.schema != VM_LIVE_TEMPLATE_PROTOCOL_SCHEMA {
        return Err("error[vm_live_template_protocol]: unexpected protocol schema".to_string());
    }
    if manifest.version == 0 {
        return Err("error[vm_live_template_protocol]: version must be non-zero".to_string());
    }
    if manifest.events.is_empty() {
        return Err(
            "error[vm_live_template_protocol]: protocol manifest must declare events".to_string(),
        );
    }

    let expected_hash =
        live_template_protocol_schema_hash(manifest.schema, manifest.version, &manifest.events);
    if manifest.schema_hash != expected_hash {
        return Err("error[vm_live_template_protocol]: protocol schema hash is stale".to_string());
    }

    let mut event_names = HashSet::new();
    for event in &manifest.events {
        if event.name.trim().is_empty() {
            return Err(
                "error[vm_live_template_protocol]: event name must be non-empty".to_string(),
            );
        }
        if !event_names.insert(event.name) {
            return Err(format!(
                "error[vm_live_template_protocol]: duplicate protocol event `{}`",
                event.name
            ));
        }
        if event.required_fields.is_empty() {
            return Err(format!(
                "error[vm_live_template_protocol]: event `{}` must declare required fields",
                event.name
            ));
        }
        let mut field_names = HashSet::new();
        for field in event.required_fields {
            if field.trim().is_empty() {
                return Err(format!(
                    "error[vm_live_template_protocol]: event `{}` has an empty field name",
                    event.name
                ));
            }
            if !field_names.insert(*field) {
                return Err(format!(
                    "error[vm_live_template_protocol]: event `{}` has duplicate field `{}`",
                    event.name, field
                ));
            }
        }
    }

    Ok(())
}

fn vm_live_template_protocol_events() -> &'static [VmLiveTemplateProtocolEventKind] {
    &[
        VmLiveTemplateProtocolEventKind::InitialRender,
        VmLiveTemplateProtocolEventKind::IncrementalPatch,
        VmLiveTemplateProtocolEventKind::CommandPostback,
        VmLiveTemplateProtocolEventKind::ErrorPatch,
        VmLiveTemplateProtocolEventKind::RedirectPatch,
        VmLiveTemplateProtocolEventKind::ReconnectToken,
        VmLiveTemplateProtocolEventKind::Heartbeat,
        VmLiveTemplateProtocolEventKind::ClientCancellation,
        VmLiveTemplateProtocolEventKind::BackpressureSignal,
        VmLiveTemplateProtocolEventKind::VersionNegotiation,
    ]
}

fn live_template_protocol_schema_hash(
    schema: &str,
    version: u32,
    events: &[VmLiveTemplateProtocolEvent],
) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    fn fold(hash: &mut u64, text: &str) {
        for byte in text.as_bytes() {
            *hash ^= u64::from(*byte);
            *hash = hash.wrapping_mul(0x100000001b3);
        }
        *hash ^= 0xff;
        *hash = hash.wrapping_mul(0x100000001b3);
    }

    fold(&mut hash, schema);
    fold(&mut hash, &version.to_string());
    for event in events {
        fold(&mut hash, event.name);
        for field in event.required_fields {
            fold(&mut hash, field);
        }
    }

    format!("{hash:016x}")
}

#[cfg(test)]
fn js_string(value: &str) -> String {
    let mut escaped = String::from("\"");
    for character in value.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            character => escaped.push(character),
        }
    }
    escaped.push('"');
    escaped
}
