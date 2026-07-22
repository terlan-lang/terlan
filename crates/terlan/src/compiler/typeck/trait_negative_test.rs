use super::test_support::*;

/// Verifies an imported trait may be denied for a locally owned target type.
#[test]
fn syntax_output_accepts_imported_trait_negative_impl_for_local_target() {
    let diagnostics = check_syntax_output_with_interface(
        "module imported_trait_local_negative_target.\n\
import negative_provider.{JsonEncode}.\n\
pub opaque type SecretKey.\n\
pub impl not JsonEncode[SecretKey].\n",
        "module negative_provider.\n\
pub trait JsonEncode[T] { encode(value: T): String. }.\n",
    );

    assert!(diagnostics.is_empty(), "diagnostics: {:?}", diagnostics);
}

/// Verifies an impl cannot deny a foreign trait for a foreign target type.
#[test]
fn syntax_output_rejects_negative_impl_for_imported_trait_and_target() {
    let diagnostics = check_syntax_output_with_interface(
        "module imported_negative_orphan.\n\
import negative_provider.{JsonEncode, SecretKey}.\n\
pub impl not JsonEncode[SecretKey].\n",
        "module negative_provider.\n\
pub opaque type SecretKey.\n\
pub trait JsonEncode[T] { encode(value: T): String. }.\n",
    );

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic.message
            == "negative impl orphan rule violation: trait `JsonEncode` and target `SecretKey` are both non-local"),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies compiler-owned primitive targets do not bypass the orphan rule.
#[test]
fn syntax_output_rejects_negative_impl_for_imported_trait_and_primitive_target() {
    let diagnostics = check_syntax_output_with_interface(
        "module primitive_negative_orphan.\n\
import negative_provider.{JsonEncode}.\n\
pub impl not JsonEncode[Int].\n",
        "module negative_provider.\n\
pub trait JsonEncode[T] { encode(value: T): String. }.\n",
    );

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic.message
            == "negative impl orphan rule violation: trait `JsonEncode` and target `Int` are both non-local"),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies structural collection targets do not bypass the orphan rule.
#[test]
fn syntax_output_rejects_negative_impl_for_imported_trait_and_structural_target() {
    let diagnostics = check_syntax_output_with_interface(
        "module structural_negative_orphan.\n\
import negative_provider.{JsonEncode}.\n\
pub impl not JsonEncode[List[Int]].\n",
        "module negative_provider.\n\
pub trait JsonEncode[T] { encode(value: T): String. }.\n",
    );

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic.message
            == "negative impl orphan rule violation: trait `JsonEncode` and target `List[Int]` are both non-local"),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies a concrete denial outranks a visible generic positive impl.
#[test]
fn syntax_output_rejects_generic_trait_fallback_for_denied_local_target() {
    let diagnostics = check_syntax_output(
        "module denied_local_generic_fallback.\n\
pub opaque type SecretKey.\n\
pub trait JsonEncode[T] { encode(value: T): String. }.\n\
pub impl JsonEncode[T] for T {\n\
    encode(value: T): String -> \"generic\".\n\
}.\n\
pub impl not JsonEncode[SecretKey].\n\
pub encode_any[T](value: T)[JsonEncode[T]]: String -> JsonEncode.encode(value).\n\
pub leak(value: SecretKey): String -> encode_any(value).\n",
    );

    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.message
            == "at `encode_any` call site: trait bound `JsonEncode[SecretKey]` is explicitly denied"
        }),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies a concrete denial does not disable generic evidence for other types.
#[test]
fn syntax_output_accepts_generic_trait_fallback_for_non_denied_target() {
    let diagnostics = check_syntax_output(
        "module allowed_local_generic_fallback.\n\
pub opaque type SecretKey.\n\
pub opaque type PublicValue.\n\
pub trait JsonEncode[T] { encode(value: T): String. }.\n\
pub impl JsonEncode[T] for T {\n\
    encode(value: T): String -> \"generic\".\n\
}.\n\
pub impl not JsonEncode[SecretKey].\n\
pub encode_any[T](value: T)[JsonEncode[T]]: String -> JsonEncode.encode(value).\n\
pub expose(value: PublicValue): String -> encode_any(value).\n",
    );

    assert!(diagnostics.is_empty(), "diagnostics: {:?}", diagnostics);
}

/// Verifies imported public denials also outrank imported generic evidence.
#[test]
fn syntax_output_rejects_imported_generic_trait_fallback_for_denied_target() {
    let diagnostics = check_syntax_output_with_interface(
        "module denied_imported_generic_fallback.\n\
import negative_provider.{JsonEncode, SecretKey}.\n\
pub encode_any[T](value: T)[JsonEncode[T]]: String -> JsonEncode.encode(value).\n\
pub leak(value: SecretKey): String -> encode_any(value).\n",
        "module negative_provider.\n\
pub opaque type SecretKey.\n\
pub trait JsonEncode[T] { encode(value: T): String. }.\n\
pub impl JsonEncode[T] for T { encode(value: T): String. }.\n\
pub impl not JsonEncode[SecretKey].\n",
    );

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic.message
            == "at `encode_any` call site: trait bound `JsonEncode[negative_provider.SecretKey]` is explicitly denied"),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies shipped std negative facts block generic std trait APIs.
#[test]
fn syntax_output_rejects_std_show_fallback_for_secret() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "module denied_std_show_fallback.\n\
import std.core.Secret.{Secret}.\n\
import std.core.String.{Show}.\n\
pub render[T](value: T)[Show[T]]: String -> Show.to_string(value).\n\
pub leak(value: Secret): String -> render(value).\n",
        "std/core/Secret.terl",
    );

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic.message
            == "at `render` call site: trait bound `Show[std.core.Secret.Secret]` is explicitly denied"),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies std negative facts do not disable positive primitive evidence.
#[test]
fn syntax_output_accepts_std_show_for_int() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "module allowed_std_show.\n\
import std.core.String.{Show}.\n\
pub render[T](value: T)[Show[T]]: String -> Show.to_string(value).\n\
pub expose(value: Int): String -> render(value).\n",
        "std/core/String.terl",
    );

    assert!(diagnostics.is_empty(), "diagnostics: {:?}", diagnostics);
}

/// Verifies a negative native-transfer fact blocks native bridge creation.
#[test]
fn syntax_output_rejects_native_bridge_resource_with_denied_transfer() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "module denied_native_bridge_resource.\n\
import std.vm.NativeBridge.\n\
import std.vm.NativeBridge.{NativeTransfer}.\n\
pub struct LocalResource { value: Int }.\n\
pub impl not NativeTransfer[LocalResource].\n\
pub start_local(resource: LocalResource): Dynamic -> NativeBridge.start(resource).\n",
        "std/vm/NativeBridge.terl",
    );

    let expected = concat!(
        "at `std.vm.NativeBridge.start` call site: trait bound ",
        "`NativeTransfer[LocalResource]` is explicitly denied"
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message == expected),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies a negative native-transfer fact blocks native bridge commands.
#[test]
fn syntax_output_rejects_native_bridge_command_with_denied_transfer() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "module denied_native_bridge_command.\n\
import std.core.{Error, Result}.\n\
import std.vm.NativeBridge.\n\
import std.vm.NativeBridge.{NativeTransfer}.\n\
pub struct LocalCommand { value: Int }.\n\
pub impl not NativeTransfer[LocalCommand].\n\
pub call_local(bridge: NativeBridge[String], command: LocalCommand): Result[String, Error] ->\n\
    bridge.call(command).\n",
        "std/vm/NativeBridge.terl",
    );

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic.message
            == "at `call` call site: trait bound `NativeTransfer[LocalCommand]` is explicitly denied"),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies the default transfer proof keeps ordinary bridge values usable.
#[test]
fn syntax_output_accepts_native_bridge_values_with_default_transfer() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "module allowed_native_bridge_values.\n\
import std.core.{Error, Result}.\n\
import std.vm.NativeBridge.\n\
import std.vm.NativeBridge.{NativeTransfer}.\n\
pub start_string(resource: String): Result[NativeBridge[String], Error] ->\n\
    NativeBridge.start(resource).\n\
pub call_string(bridge: NativeBridge[String], command: String): Result[String, Error] ->\n\
    bridge.call(command).\n",
        "std/vm/NativeBridge.terl",
    );

    assert!(diagnostics.is_empty(), "diagnostics: {:?}", diagnostics);
}

/// Verifies a negative persistence fact blocks persistent actor snapshots.
#[test]
fn syntax_output_rejects_persistent_actor_state_with_denied_persistence() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "module denied_persistent_actor_state.\n\
import std.vm.PersistentActor.\n\
import std.vm.PersistentActor.{ActorId, Persistable, SchemaId, SnapshotPlan}.\n\
pub struct EphemeralState { value: Int }.\n\
pub impl not Persistable[EphemeralState].\n\
pub snapshot(actor: ActorId, schema: SchemaId, state: EphemeralState): SnapshotPlan ->\n\
    PersistentActor.snapshot(actor, schema, 1, state).\n",
        "std/vm/PersistentActor.terl",
    );

    let expected = concat!(
        "at `std.vm.PersistentActor.snapshot` call site: trait bound ",
        "`Persistable[EphemeralState]` is explicitly denied"
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message == expected),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies default persistence evidence keeps ordinary actor state usable.
#[test]
fn syntax_output_accepts_persistent_actor_state_with_default_persistence() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "module allowed_persistent_actor_state.\n\
import std.vm.PersistentActor.\n\
import std.vm.PersistentActor.{ActorId, Persistable, SchemaId, SnapshotPlan}.\n\
pub snapshot(actor: ActorId, schema: SchemaId, state: String): SnapshotPlan ->\n\
    PersistentActor.snapshot(actor, schema, 1, state).\n",
        "std/vm/PersistentActor.terl",
    );

    assert!(diagnostics.is_empty(), "diagnostics: {:?}", diagnostics);
}

/// Verifies a negative actor-message fact blocks local mailbox delivery.
#[test]
fn syntax_output_rejects_actor_message_with_denied_delivery() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "module denied_actor_message.\n\
import std.vm.Message.\n\
import std.vm.Process.\n\
import std.vm.Process.{ActorMessage}.\n\
import type std.vm.Process.{Process as ProcessHandle}.\n\
pub struct LocalOnlyMessage { value: Int }.\n\
pub impl not ActorMessage[LocalOnlyMessage].\n\
pub deliver(process: ProcessHandle[LocalOnlyMessage], value: LocalOnlyMessage): Unit ->\n\
    Process.send[LocalOnlyMessage](process, Message.wrap[LocalOnlyMessage](value)).\n",
        "std/vm/Process.terl",
    );

    let expected = concat!(
        "at `std.vm.Process.send` call site: trait bound ",
        "`ActorMessage[LocalOnlyMessage]` is explicitly denied"
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message == expected),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies a negative node-transfer fact blocks distributed delivery.
#[test]
fn syntax_output_rejects_cluster_payload_with_denied_node_transfer() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "module denied_cluster_payload.\n\
import std.vm.Cluster.\n\
import std.vm.Cluster.{SendAcrossNode, SendResult, Session}.\n\
pub struct LocalOnlyPayload { value: Int }.\n\
pub impl not SendAcrossNode[LocalOnlyPayload].\n\
pub deliver(session: Session, payload: LocalOnlyPayload): Frame ->\n\
    session.send(\"message\", payload).\n",
        "std/vm/Cluster.terl",
    );

    let expected = concat!(
        "at `send` call site: trait bound ",
        "`SendAcrossNode[LocalOnlyPayload]` is explicitly denied"
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message == expected),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies default delivery evidence keeps ordinary actor and node values usable.
#[test]
fn syntax_output_accepts_actor_and_node_values_with_default_delivery() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "module allowed_actor_and_node_values.\n\
import std.vm.Cluster.\n\
import std.vm.Cluster.{Frame, SendAcrossNode, SendResult, Session}.\n\
import std.vm.Message.\n\
import std.vm.Process.\n\
import std.vm.Process.{ActorMessage}.\n\
import type std.vm.Process.{Process as ProcessHandle}.\n\
pub actor(process: ProcessHandle[String], value: String): Unit ->\n\
    Process.send[String](process, Message.wrap[String](value)).\n\
pub node(session: Session, value: String): SendResult ->\n\
    session.send(\"message\", value).\n",
        "std/vm/Process.terl",
    );

    assert!(diagnostics.is_empty(), "diagnostics: {:?}", diagnostics);
}

/// Verifies colliding imported trait aliases cannot select a denial provider.
#[test]
fn syntax_output_rejects_ambiguous_imported_trait_alias_in_negative_impl() {
    let diagnostics = check_syntax_output_with_interfaces(
        "module ambiguous_negative_trait_alias.\n\
import provider.audit.{Audit as Denied}.\n\
import provider.encode.{Encode as Denied}.\n\
pub opaque type Secret.\n\
pub impl not Denied[Secret].\n",
        &[
            "module provider.audit.\n\
pub trait Audit[T] { audit(value: T): String. }.\n",
            "module provider.encode.\n\
pub trait Encode[T] { encode(value: T): String. }.\n",
        ],
    );

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic.message
            == "ambiguous imported trait alias 'Denied': provider.audit.Audit conflicts with provider.encode.Encode"),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies wildcard imports share the trait-alias ambiguity contract.
#[test]
fn syntax_output_rejects_wildcard_trait_alias_collision_in_negative_impl() {
    let diagnostics = check_syntax_output_with_interfaces(
        "module ambiguous_wildcard_negative_trait_alias.\n\
import provider.audit.{Audit as Denied}.\n\
import provider.encode.{*}.\n\
pub opaque type Secret.\n\
pub impl not Denied[Secret].\n",
        &[
            "module provider.audit.\n\
pub trait Audit[T] { audit(value: T): String. }.\n",
            "module provider.encode.\n\
pub trait Denied[T] { encode(value: T): String. }.\n",
        ],
    );

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic.message
            == "ambiguous imported trait alias 'Denied': provider.audit.Audit conflicts with provider.encode.Denied"),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies distinct aliases preserve independent imported negative facts.
#[test]
fn syntax_output_accepts_distinct_imported_trait_aliases_in_negative_impls() {
    let diagnostics = check_syntax_output_with_interfaces(
        "module distinct_negative_trait_aliases.\n\
import provider.audit.{Audit as AuditDenied}.\n\
import provider.encode.{Encode as EncodeDenied}.\n\
pub opaque type Secret.\n\
pub impl not AuditDenied[Secret].\n\
pub impl not EncodeDenied[Secret].\n",
        &[
            "module provider.audit.\n\
pub trait Audit[T] { audit(value: T): String. }.\n",
            "module provider.encode.\n\
pub trait Encode[T] { encode(value: T): String. }.\n",
        ],
    );

    assert!(diagnostics.is_empty(), "diagnostics: {:?}", diagnostics);
}
