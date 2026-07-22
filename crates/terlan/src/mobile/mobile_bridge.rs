//! Typed mobile bridge declarations used by Terlan mobile-shell planning.
#![allow(dead_code)]
//!
//! Inputs:
//! - Compiler-owned declarations for native shell capabilities, commands, and
//!   events.
//!
//! Outputs:
//! - Validated bridge declaration summaries that later metadata emitters and
//!   typechecking passes can consume.
//!
//! Transformation:
//! - Keeps mobile bridge shape typed and explicit before mobile syntax and
//!   native shell generation are implemented.

use std::collections::{BTreeMap, BTreeSet};

use super::mobile_debug_identity::{
    generate_mobile_debug_identity_metadata, validate_mobile_source_identity,
    MobileDebugIdentityMetadata, MobileSourceIdentity,
};

/// One validated mobile bridge declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MobileBridgeDeclaration {
    pub(crate) name: String,
    pub(crate) capabilities: Vec<MobileBridgeCapability>,
    pub(crate) commands: Vec<MobileBridgeCommand>,
    pub(crate) events: Vec<MobileBridgeEvent>,
}

/// Mobile shell capability required by a bridge declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum MobileBridgeCapability {
    Navigation,
    NativeComponents,
    Permissions,
    Files,
    Camera,
    Geolocation,
    Storage,
    PushNotifications,
    PlatformEnvironment,
}

impl MobileBridgeCapability {
    /// Returns the stable manifest spelling for one capability.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Navigation => "navigation",
            Self::NativeComponents => "native_components",
            Self::Permissions => "permissions",
            Self::Files => "files",
            Self::Camera => "camera",
            Self::Geolocation => "geolocation",
            Self::Storage => "storage",
            Self::PushNotifications => "push_notifications",
            Self::PlatformEnvironment => "platform_environment",
        }
    }
}

/// One native command callable from Terlan/AngularTS code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MobileBridgeCommand {
    pub(crate) name: String,
    pub(crate) required_capability: MobileBridgeCapability,
    pub(crate) parameters: Vec<MobileBridgeField>,
    pub(crate) result: MobileBridgeType,
    pub(crate) source_identity: Option<MobileSourceIdentity>,
}

/// One native event emitted back into Terlan/AngularTS code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MobileBridgeEvent {
    pub(crate) name: String,
    pub(crate) payload: Vec<MobileBridgeField>,
    pub(crate) source_identity: Option<MobileSourceIdentity>,
}

/// Named typed field used by command parameters and event payloads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MobileBridgeField {
    pub(crate) name: String,
    pub(crate) field_type: MobileBridgeType,
}

/// Closed scalar type surface for the first mobile bridge contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MobileBridgeType {
    Unit,
    Bool,
    Int,
    Float,
    String,
    Json,
}

impl MobileBridgeType {
    /// Returns the stable manifest spelling for one field type.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Unit => "Unit",
            Self::Bool => "Bool",
            Self::Int => "Int",
            Self::Float => "Float",
            Self::String => "String",
            Self::Json => "Json",
        }
    }
}

/// Validation diagnostic for mobile bridge declarations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MobileBridgeDiagnostic {
    pub(crate) code: &'static str,
    pub(crate) message: String,
}

/// Generated mobile bridge metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MobileBridgeMetadata {
    pub(crate) schema_version: u32,
    pub(crate) declarations: Vec<MobileBridgeMetadataDeclaration>,
}

/// Generated metadata for one bridge declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MobileBridgeMetadataDeclaration {
    pub(crate) name: String,
    pub(crate) capabilities: Vec<&'static str>,
    pub(crate) commands: Vec<MobileBridgeMetadataCommand>,
    pub(crate) events: Vec<MobileBridgeMetadataEvent>,
}

/// Generated metadata for one bridge command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MobileBridgeMetadataCommand {
    pub(crate) name: String,
    pub(crate) required_capability: &'static str,
    pub(crate) parameters: Vec<MobileBridgeMetadataField>,
    pub(crate) result: &'static str,
    pub(crate) source_identity: Option<MobileDebugIdentityMetadata>,
}

/// Generated metadata for one bridge event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MobileBridgeMetadataEvent {
    pub(crate) name: String,
    pub(crate) payload: Vec<MobileBridgeMetadataField>,
    pub(crate) source_identity: Option<MobileDebugIdentityMetadata>,
}

/// Generated metadata for one typed field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MobileBridgeMetadataField {
    pub(crate) name: String,
    pub(crate) field_type: &'static str,
}

/// Validates typed mobile bridge declarations.
///
/// Inputs:
/// - `declarations`: bridge declarations collected from source or generated
///   profile configuration.
///
/// Output:
/// - `Ok(())` when declaration names, command names, event names, fields, and
///   capability references are coherent.
/// - `Err(Vec<MobileBridgeDiagnostic>)` with stable diagnostics otherwise.
///
/// Transformation:
/// - Performs semantic checks over already-typed declaration data without
///   reading source files or emitting metadata.
pub(crate) fn validate_mobile_bridge_declarations(
    declarations: &[MobileBridgeDeclaration],
) -> Result<(), Vec<MobileBridgeDiagnostic>> {
    let mut diagnostics = Vec::new();
    let mut declaration_names = BTreeSet::new();

    for declaration in declarations {
        if is_blank(&declaration.name) {
            diagnostics.push(diagnostic(
                "mobile_bridge_empty_name",
                "mobile bridge declaration name must not be empty",
            ));
        } else if !declaration_names.insert(declaration.name.as_str()) {
            diagnostics.push(diagnostic(
                "mobile_bridge_duplicate_name",
                format!(
                    "mobile bridge declaration `{}` is declared more than once",
                    declaration.name
                ),
            ));
        }
        diagnostics.extend(validate_capabilities(declaration));
        diagnostics.extend(validate_commands(declaration));
        diagnostics.extend(validate_events(declaration));
    }

    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(diagnostics)
    }
}

/// Generates typed mobile bridge metadata from declarations.
///
/// Inputs:
/// - `declarations`: bridge declarations collected from source or generated
///   profile configuration.
///
/// Output:
/// - Typed metadata with stable capability and type spellings.
/// - Stable validation diagnostics when declarations are invalid.
///
/// Transformation:
/// - Validates declarations, normalizes capabilities into sorted manifest
///   spelling order, and preserves command/event declaration order.
pub(crate) fn generate_mobile_bridge_metadata(
    declarations: &[MobileBridgeDeclaration],
) -> Result<MobileBridgeMetadata, Vec<MobileBridgeDiagnostic>> {
    validate_mobile_bridge_declarations(declarations)?;
    Ok(MobileBridgeMetadata {
        schema_version: 1,
        declarations: declarations
            .iter()
            .map(mobile_bridge_metadata_declaration)
            .collect(),
    })
}

/// Validates generated mobile bridge metadata against typed declarations.
///
/// Inputs:
/// - `declarations`: typed bridge declarations that source/typechecking
///   accepted.
/// - `metadata`: generated or loaded metadata to validate before native shell
///   consumption.
///
/// Output:
/// - `Ok(())` when metadata exactly matches declaration names, capabilities,
///   command/event arity, field names/types, result types, and source
///   identities.
/// - Stable diagnostics for stale or mismatched metadata.
///
/// Transformation:
/// - Compares already-typed declaration and metadata surfaces without
///   regenerating metadata, so release/build checks can detect stale committed
///   mobile bridge artifacts.
pub(crate) fn validate_mobile_bridge_metadata_matches_declarations(
    declarations: &[MobileBridgeDeclaration],
    metadata: &MobileBridgeMetadata,
) -> Result<(), Vec<MobileBridgeDiagnostic>> {
    let mut diagnostics = Vec::new();
    if metadata.schema_version != 1 {
        diagnostics.push(diagnostic(
            "mobile_bridge_stale_metadata_schema",
            format!(
                "mobile bridge metadata schema version {} is not supported",
                metadata.schema_version
            ),
        ));
    }
    diagnostics.extend(
        validate_mobile_bridge_declarations(declarations)
            .err()
            .unwrap_or_default(),
    );
    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }

    let metadata_by_name = metadata
        .declarations
        .iter()
        .map(|declaration| (declaration.name.as_str(), declaration))
        .collect::<BTreeMap<_, _>>();
    for declaration in declarations {
        let Some(metadata_declaration) = metadata_by_name.get(declaration.name.as_str()) else {
            diagnostics.push(diagnostic(
                "mobile_bridge_stale_metadata_declaration",
                format!(
                    "mobile bridge metadata is missing declaration `{}`",
                    declaration.name
                ),
            ));
            continue;
        };
        diagnostics.extend(validate_metadata_declaration(
            declaration,
            metadata_declaration,
        ));
    }
    for metadata_declaration in &metadata.declarations {
        if !declarations
            .iter()
            .any(|declaration| declaration.name == metadata_declaration.name)
        {
            diagnostics.push(diagnostic(
                "mobile_bridge_stale_metadata_declaration",
                format!(
                    "mobile bridge metadata contains unknown declaration `{}`",
                    metadata_declaration.name
                ),
            ));
        }
    }

    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(diagnostics)
    }
}

/// Converts one declaration into metadata.
fn mobile_bridge_metadata_declaration(
    declaration: &MobileBridgeDeclaration,
) -> MobileBridgeMetadataDeclaration {
    let mut capabilities = declaration
        .capabilities
        .iter()
        .map(|capability| capability.as_str())
        .collect::<Vec<_>>();
    capabilities.sort_unstable();
    MobileBridgeMetadataDeclaration {
        name: declaration.name.clone(),
        capabilities,
        commands: declaration
            .commands
            .iter()
            .map(mobile_bridge_metadata_command)
            .collect(),
        events: declaration
            .events
            .iter()
            .map(mobile_bridge_metadata_event)
            .collect(),
    }
}

/// Converts one command into metadata.
fn mobile_bridge_metadata_command(command: &MobileBridgeCommand) -> MobileBridgeMetadataCommand {
    MobileBridgeMetadataCommand {
        name: command.name.clone(),
        required_capability: command.required_capability.as_str(),
        parameters: command
            .parameters
            .iter()
            .map(mobile_bridge_metadata_field)
            .collect(),
        result: command.result.as_str(),
        source_identity: command.source_identity.as_ref().map(|identity| {
            generate_mobile_debug_identity_metadata(identity)
                .expect("validated bridge command source identity")
        }),
    }
}

/// Converts one event into metadata.
fn mobile_bridge_metadata_event(event: &MobileBridgeEvent) -> MobileBridgeMetadataEvent {
    MobileBridgeMetadataEvent {
        name: event.name.clone(),
        payload: event
            .payload
            .iter()
            .map(mobile_bridge_metadata_field)
            .collect(),
        source_identity: event.source_identity.as_ref().map(|identity| {
            generate_mobile_debug_identity_metadata(identity)
                .expect("validated bridge event source identity")
        }),
    }
}

/// Converts one field into metadata.
fn mobile_bridge_metadata_field(field: &MobileBridgeField) -> MobileBridgeMetadataField {
    MobileBridgeMetadataField {
        name: field.name.clone(),
        field_type: field.field_type.as_str(),
    }
}

/// Validates metadata for one bridge declaration.
fn validate_metadata_declaration(
    declaration: &MobileBridgeDeclaration,
    metadata: &MobileBridgeMetadataDeclaration,
) -> Vec<MobileBridgeDiagnostic> {
    let mut diagnostics = Vec::new();
    diagnostics.extend(validate_metadata_capabilities(declaration, metadata));
    diagnostics.extend(validate_metadata_commands(declaration, metadata));
    diagnostics.extend(validate_metadata_events(declaration, metadata));
    diagnostics
}

/// Validates capability metadata for one bridge declaration.
fn validate_metadata_capabilities(
    declaration: &MobileBridgeDeclaration,
    metadata: &MobileBridgeMetadataDeclaration,
) -> Vec<MobileBridgeDiagnostic> {
    let expected = declaration
        .capabilities
        .iter()
        .map(|capability| capability.as_str())
        .collect::<BTreeSet<_>>();
    let actual = metadata
        .capabilities
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    if expected == actual {
        Vec::new()
    } else {
        vec![diagnostic(
            "mobile_bridge_stale_metadata_capabilities",
            format!(
                "mobile bridge metadata capabilities for `{}` do not match declarations",
                declaration.name
            ),
        )]
    }
}

/// Validates command metadata for one bridge declaration.
fn validate_metadata_commands(
    declaration: &MobileBridgeDeclaration,
    metadata: &MobileBridgeMetadataDeclaration,
) -> Vec<MobileBridgeDiagnostic> {
    let mut diagnostics = Vec::new();
    let metadata_commands = metadata
        .commands
        .iter()
        .map(|command| (command.name.as_str(), command))
        .collect::<BTreeMap<_, _>>();
    for command in &declaration.commands {
        let Some(metadata_command) = metadata_commands.get(command.name.as_str()) else {
            diagnostics.push(diagnostic(
                "mobile_bridge_stale_metadata_command",
                format!(
                    "mobile bridge metadata for `{}` is missing command `{}`",
                    declaration.name, command.name
                ),
            ));
            continue;
        };
        diagnostics.extend(validate_metadata_command(
            &declaration.name,
            command,
            metadata_command,
        ));
    }
    for metadata_command in &metadata.commands {
        if !declaration
            .commands
            .iter()
            .any(|command| command.name == metadata_command.name)
        {
            diagnostics.push(diagnostic(
                "mobile_bridge_stale_metadata_command",
                format!(
                    "mobile bridge metadata for `{}` contains unknown command `{}`",
                    declaration.name, metadata_command.name
                ),
            ));
        }
    }
    diagnostics
}

/// Validates one command metadata entry.
fn validate_metadata_command(
    declaration_name: &str,
    command: &MobileBridgeCommand,
    metadata: &MobileBridgeMetadataCommand,
) -> Vec<MobileBridgeDiagnostic> {
    let mut diagnostics = Vec::new();
    if command.required_capability.as_str() != metadata.required_capability {
        diagnostics.push(diagnostic(
            "mobile_bridge_stale_metadata_capability",
            format!(
                "mobile bridge metadata for `{declaration_name}` command `{}` has stale required capability",
                command.name
            ),
        ));
    }
    if command.result.as_str() != metadata.result {
        diagnostics.push(diagnostic(
            "mobile_bridge_stale_metadata_result_type",
            format!(
                "mobile bridge metadata for `{declaration_name}` command `{}` has stale result type",
                command.name
            ),
        ));
    }
    diagnostics.extend(validate_metadata_fields(
        declaration_name,
        "command",
        &command.name,
        &command.parameters,
        &metadata.parameters,
    ));
    diagnostics.extend(validate_metadata_source_identity(
        declaration_name,
        "command",
        &command.name,
        command.source_identity.as_ref(),
        metadata.source_identity.as_ref(),
    ));
    diagnostics
}

/// Validates event metadata for one bridge declaration.
fn validate_metadata_events(
    declaration: &MobileBridgeDeclaration,
    metadata: &MobileBridgeMetadataDeclaration,
) -> Vec<MobileBridgeDiagnostic> {
    let mut diagnostics = Vec::new();
    let metadata_events = metadata
        .events
        .iter()
        .map(|event| (event.name.as_str(), event))
        .collect::<BTreeMap<_, _>>();
    for event in &declaration.events {
        let Some(metadata_event) = metadata_events.get(event.name.as_str()) else {
            diagnostics.push(diagnostic(
                "mobile_bridge_stale_metadata_event",
                format!(
                    "mobile bridge metadata for `{}` is missing event `{}`",
                    declaration.name, event.name
                ),
            ));
            continue;
        };
        diagnostics.extend(validate_metadata_event(
            &declaration.name,
            event,
            metadata_event,
        ));
    }
    for metadata_event in &metadata.events {
        if !declaration
            .events
            .iter()
            .any(|event| event.name == metadata_event.name)
        {
            diagnostics.push(diagnostic(
                "mobile_bridge_stale_metadata_event",
                format!(
                    "mobile bridge metadata for `{}` contains unknown event `{}`",
                    declaration.name, metadata_event.name
                ),
            ));
        }
    }
    diagnostics
}

/// Validates one event metadata entry.
fn validate_metadata_event(
    declaration_name: &str,
    event: &MobileBridgeEvent,
    metadata: &MobileBridgeMetadataEvent,
) -> Vec<MobileBridgeDiagnostic> {
    let mut diagnostics = validate_metadata_fields(
        declaration_name,
        "event",
        &event.name,
        &event.payload,
        &metadata.payload,
    );
    diagnostics.extend(validate_metadata_source_identity(
        declaration_name,
        "event",
        &event.name,
        event.source_identity.as_ref(),
        metadata.source_identity.as_ref(),
    ));
    diagnostics
}

/// Validates field arity, order, names, and types for metadata.
fn validate_metadata_fields(
    declaration_name: &str,
    owner_kind: &str,
    owner_name: &str,
    expected: &[MobileBridgeField],
    actual: &[MobileBridgeMetadataField],
) -> Vec<MobileBridgeDiagnostic> {
    let mut diagnostics = Vec::new();
    if expected.len() != actual.len() {
        diagnostics.push(diagnostic(
            "mobile_bridge_stale_metadata_arity",
            format!(
                "mobile bridge metadata for `{declaration_name}` {owner_kind} `{owner_name}` has stale field arity"
            ),
        ));
        return diagnostics;
    }
    for (expected_field, actual_field) in expected.iter().zip(actual) {
        if expected_field.name != actual_field.name {
            diagnostics.push(diagnostic(
                "mobile_bridge_stale_metadata_field_name",
                format!(
                    "mobile bridge metadata for `{declaration_name}` {owner_kind} `{owner_name}` has stale field name `{}`",
                    actual_field.name
                ),
            ));
        }
        if expected_field.field_type.as_str() != actual_field.field_type {
            diagnostics.push(diagnostic(
                "mobile_bridge_stale_metadata_field_type",
                format!(
                    "mobile bridge metadata for `{declaration_name}` {owner_kind} `{owner_name}` field `{}` has stale type",
                    expected_field.name
                ),
            ));
        }
    }
    diagnostics
}

/// Validates source identity metadata for one command/event.
fn validate_metadata_source_identity(
    declaration_name: &str,
    owner_kind: &str,
    owner_name: &str,
    expected: Option<&MobileSourceIdentity>,
    actual: Option<&MobileDebugIdentityMetadata>,
) -> Vec<MobileBridgeDiagnostic> {
    let expected = expected
        .map(generate_mobile_debug_identity_metadata)
        .transpose()
        .expect("validated bridge source identity");
    if expected.as_ref() == actual {
        Vec::new()
    } else {
        vec![diagnostic(
            "mobile_bridge_stale_metadata_source_identity",
            format!(
                "mobile bridge metadata for `{declaration_name}` {owner_kind} `{owner_name}` has stale source identity"
            ),
        )]
    }
}

/// Validates capability declarations for one bridge.
fn validate_capabilities(declaration: &MobileBridgeDeclaration) -> Vec<MobileBridgeDiagnostic> {
    let mut diagnostics = Vec::new();
    let mut seen = BTreeSet::new();
    for capability in &declaration.capabilities {
        if !seen.insert(*capability) {
            diagnostics.push(diagnostic(
                "mobile_bridge_duplicate_capability",
                format!(
                    "mobile bridge `{}` repeats capability `{}`",
                    declaration.name,
                    capability.as_str()
                ),
            ));
        }
    }
    diagnostics
}

/// Validates commands for one bridge.
fn validate_commands(declaration: &MobileBridgeDeclaration) -> Vec<MobileBridgeDiagnostic> {
    let declared_capabilities = declaration
        .capabilities
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let mut diagnostics = Vec::new();
    let mut command_names = BTreeSet::new();
    for command in &declaration.commands {
        if is_blank(&command.name) {
            diagnostics.push(diagnostic(
                "mobile_bridge_empty_command_name",
                format!(
                    "mobile bridge `{}` has an empty command name",
                    declaration.name
                ),
            ));
            continue;
        }
        if !command_names.insert(command.name.as_str()) {
            diagnostics.push(diagnostic(
                "mobile_bridge_duplicate_command",
                format!(
                    "mobile bridge `{}` declares command `{}` more than once",
                    declaration.name, command.name
                ),
            ));
        }
        if !declared_capabilities.contains(&command.required_capability) {
            diagnostics.push(diagnostic(
                "mobile_bridge_missing_capability",
                format!(
                    "mobile bridge `{}` command `{}` requires undeclared capability `{}`",
                    declaration.name,
                    command.name,
                    command.required_capability.as_str()
                ),
            ));
        }
        diagnostics.extend(validate_fields(
            &declaration.name,
            "command",
            &command.name,
            &command.parameters,
        ));
        diagnostics.extend(validate_optional_source_identity(
            &declaration.name,
            "command",
            &command.name,
            command.source_identity.as_ref(),
        ));
    }
    diagnostics
}

/// Validates events for one bridge.
fn validate_events(declaration: &MobileBridgeDeclaration) -> Vec<MobileBridgeDiagnostic> {
    let mut diagnostics = Vec::new();
    let mut event_names = BTreeSet::new();
    for event in &declaration.events {
        if is_blank(&event.name) {
            diagnostics.push(diagnostic(
                "mobile_bridge_empty_event_name",
                format!(
                    "mobile bridge `{}` has an empty event name",
                    declaration.name
                ),
            ));
            continue;
        }
        if !event_names.insert(event.name.as_str()) {
            diagnostics.push(diagnostic(
                "mobile_bridge_duplicate_event",
                format!(
                    "mobile bridge `{}` declares event `{}` more than once",
                    declaration.name, event.name
                ),
            ));
        }
        diagnostics.extend(validate_fields(
            &declaration.name,
            "event",
            &event.name,
            &event.payload,
        ));
        diagnostics.extend(validate_optional_source_identity(
            &declaration.name,
            "event",
            &event.name,
            event.source_identity.as_ref(),
        ));
    }
    diagnostics
}

/// Validates named fields for one command/event surface.
fn validate_fields(
    declaration_name: &str,
    owner_kind: &str,
    owner_name: &str,
    fields: &[MobileBridgeField],
) -> Vec<MobileBridgeDiagnostic> {
    let mut diagnostics = Vec::new();
    let mut field_names = BTreeSet::new();
    for field in fields {
        if is_blank(&field.name) {
            diagnostics.push(diagnostic(
                "mobile_bridge_empty_field_name",
                format!(
                    "mobile bridge `{declaration_name}` {owner_kind} `{owner_name}` has an empty field name"
                ),
            ));
            continue;
        }
        if !field_names.insert(field.name.as_str()) {
            diagnostics.push(diagnostic(
                "mobile_bridge_duplicate_field",
                format!(
                    "mobile bridge `{declaration_name}` {owner_kind} `{owner_name}` repeats field `{}`",
                    field.name
                ),
            ));
        }
    }
    diagnostics
}

/// Validates optional source identity for one command/event.
fn validate_optional_source_identity(
    declaration_name: &str,
    owner_kind: &str,
    owner_name: &str,
    source_identity: Option<&MobileSourceIdentity>,
) -> Vec<MobileBridgeDiagnostic> {
    let Some(identity) = source_identity else {
        return Vec::new();
    };
    validate_mobile_source_identity(identity)
        .err()
        .unwrap_or_default()
        .into_iter()
        .map(|source_diagnostic| {
            diagnostic(
                source_diagnostic.code,
                format!(
                    "mobile bridge `{declaration_name}` {owner_kind} `{owner_name}` has invalid source identity: {}",
                    source_diagnostic.message
                ),
            )
        })
        .collect()
}

/// Returns whether a bridge identifier is blank after trimming whitespace.
fn is_blank(value: &str) -> bool {
    value.trim().is_empty()
}

/// Builds a stable mobile bridge diagnostic.
fn diagnostic(code: &'static str, message: impl Into<String>) -> MobileBridgeDiagnostic {
    MobileBridgeDiagnostic {
        code,
        message: message.into(),
    }
}

#[cfg(test)]
#[path = "mobile_bridge_test.rs"]
mod mobile_bridge_test;
