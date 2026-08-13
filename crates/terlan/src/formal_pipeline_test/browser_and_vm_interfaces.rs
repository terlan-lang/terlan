use super::*;

/// Verifies generated DOM summaries resolve for the browser JS profile.
///
/// Inputs:
/// - A source module importing generated `std.js.Dom.Document.Document`.
///
/// Output:
/// - Test assertion only; compilation succeeds under `js.browser`.
///
/// Transformation:
/// - Runs the full formal compilation path without a cache directory, proving
///   generated DOM summaries participate in import/typecheck like hand-authored
///   std modules once the selected target profile admits browser APIs.
#[test]
fn compile_syntax_module_with_browser_profile_resolves_generated_dom_summary() {
    let source = "\
module js_dom_summary_accept.

import type std.js.Dom.Document.Document.

pub accepts(value: Document): Document ->
  value.
";

    let result = compile_syntax_module_through_phases_with_diagnostics_for_profile(
        "src/js_dom_summary_accept.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsBrowser,
    );

    assert_eq!(result.exit_code, ExitCode::SUCCESS);
    assert!(result.artifacts.is_some());
    assert!(result.core_diagnostics.is_empty());
}

/// Verifies generated DOM summaries stay gated from shared JS compilation.
///
/// Inputs:
/// - A source module importing generated `std.js.Dom.Document.Document`.
///
/// Output:
/// - Test passes when full formal compilation rejects the module under
///   `js.shared` with a target-profile diagnostic.
///
/// Transformation:
/// - Exercises generated `std.js` binding metadata through parse, embedded
///   summary loading, typechecking, CoreIR, and target-profile validation.
#[test]
fn adversarial_compile_with_shared_js_profile_rejects_generated_dom_summary() {
    let source = "\
module js_dom_summary_reject_shared.

import type std.js.Dom.Document.Document.

pub accepts(value: Document): Document ->
  value.
";

    let result = compile_syntax_module_through_phases_with_diagnostics_for_profile(
        "src/js_dom_summary_reject_shared.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    );

    assert_ne!(result.exit_code, ExitCode::SUCCESS);
    assert!(result.artifacts.is_none());
    let diagnostic_text = result
        .core_diagnostics
        .iter()
        .map(|diagnostic| format!("{}: {}", diagnostic.code, diagnostic.message))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        result.core_diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "target_profile_unsupported"
                && diagnostic
                    .message
                    .contains("JavaScript std module std.js.Dom.Document")
        }),
        "expected generated DOM target-profile diagnostic, got {diagnostic_text}"
    );
}

/// Verifies embedded std summaries include the VM bridge contracts.
///
/// Inputs:
/// - Compiler-embedded std interface summaries.
///
/// Output:
/// - Test passes when the first VM bridge and Agent contract modules are
///   loaded from the embedded summary list with their target-gated types,
///   traits, and receiver methods.
///
/// Transformation:
/// - Exercises normal embedded summary parsing so VM supervision,
///   process, message, backpressure, and native-bridge contracts can be
///   resolved without adding VM-specific grammar to Terlan source.
#[test]
fn embedded_std_interfaces_include_beam_bridge_contracts() {
    let mut interfaces = HashMap::new();

    load_embedded_std_interfaces(&mut interfaces);

    let agent = interfaces
        .get("std.vm.Agent")
        .expect("embedded VM Agent interface");
    assert!(agent.public_types.contains("Agent"));
    assert!(agent.public_types.contains("AgentCommand"));
    assert!(agent.public_types.contains("AgentReply"));
    assert!(agent.functions.contains_key(&("start".to_string(), 2)));
    let get = agent
        .functions
        .get(&("get".to_string(), 2))
        .expect("Agent.get source function");
    assert!(!get.receiver_method);
    assert!(!get.receiver_mutable);
    let update = agent
        .functions
        .get(&("update".to_string(), 2))
        .expect("Agent.update source function");
    assert!(!update.receiver_method);
    assert!(!update.receiver_mutable);
    let get_and_update = agent
        .functions
        .get(&("get_and_update".to_string(), 3))
        .expect("Agent.get_and_update source function");
    assert!(!get_and_update.receiver_method);
    assert!(!get_and_update.receiver_mutable);

    let process = interfaces
        .get("std.vm.Process")
        .expect("embedded VM process interface");
    assert!(process.opaque_types.contains("Process"));
    let process_like = process
        .traits
        .get("ProcessLike")
        .expect("embedded ProcessLike trait contract");
    assert!(process_like.methods.contains_key("send"));
    assert!(process_like.methods.contains_key("stop"));
    let actor_message = process
        .traits
        .get("ActorMessage")
        .expect("embedded ActorMessage trait contract");
    assert!(actor_message.methods.contains_key("message"));
    let process_send = process
        .functions
        .get(&("send".to_string(), 2))
        .expect("Process.send function");
    assert_eq!(process_send.generic_params, vec!["T"]);
    assert_eq!(process_send.generic_bounds, vec!["ActorMessage[T]"]);

    let message = interfaces
        .get("std.vm.Message")
        .expect("embedded VM message interface");
    assert!(message.opaque_types.contains("Message"));
    let message_codec = message
        .traits
        .get("MessageCodec")
        .expect("embedded MessageCodec trait contract");
    assert!(message_codec.methods.contains_key("wrap"));
    assert!(message_codec.methods.contains_key("unwrap"));

    let cluster = interfaces
        .get("std.vm.Cluster")
        .expect("embedded VM Cluster interface");
    assert!(cluster.opaque_types.contains("Profile"));
    assert!(cluster.opaque_types.contains("Membership"));
    assert!(cluster.opaque_types.contains("Session"));
    assert!(cluster.opaque_types.contains("Frame"));
    assert!(cluster.public_types.contains("InboundOutcome"));
    assert!(cluster.public_types.contains("Delivery"));
    assert!(cluster.public_types.contains("SendResult"));
    assert!(cluster.public_types.contains("AcceptResult"));
    assert!(cluster.public_types.contains("AcknowledgeResult"));
    assert!(cluster.public_types.contains("SessionState"));
    assert!(cluster.public_types.contains("DisconnectReason"));
    assert!(cluster.public_types.contains("DisconnectResult"));
    assert!(cluster.public_types.contains("ReconnectOutcome"));
    assert!(cluster.public_types.contains("ReconnectResult"));
    assert!(cluster.struct_fields.contains_key("OutOfOrder"));
    assert!(cluster.struct_fields.contains_key("AcknowledgeResult"));
    assert!(cluster.struct_fields.contains_key("DisconnectEvent"));
    assert!(cluster.struct_fields.contains_key("Reconnected"));
    assert!(cluster.functions.contains_key(&("profile".to_string(), 6)));
    assert!(cluster
        .functions
        .contains_key(&("membership".to_string(), 2)));
    let profile_epoch = cluster
        .functions
        .get(&("epoch".to_string(), 1))
        .expect("Cluster.Profile.epoch receiver method");
    assert!(profile_epoch.receiver_method);
    assert_eq!(profile_epoch.return_type, "Int");
    let profile_next_epoch = cluster
        .functions
        .get(&("next_epoch".to_string(), 1))
        .expect("Cluster.Profile.next_epoch receiver method");
    assert!(profile_next_epoch.receiver_method);
    assert_eq!(profile_next_epoch.return_type, "Profile");
    let join = cluster
        .functions
        .get(&("join".to_string(), 4))
        .expect("Cluster.join receiver method");
    assert!(join.receiver_method);
    assert!(!join.receiver_mutable);
    let restart = cluster
        .functions
        .get(&("restart".to_string(), 3))
        .expect("Cluster.restart receiver method");
    assert!(restart.receiver_method);
    assert!(!restart.receiver_mutable);
    assert_eq!(restart.return_type, "Membership");
    let partition = cluster
        .functions
        .get(&("partition".to_string(), 3))
        .expect("Cluster.partition receiver method");
    assert!(partition.receiver_method);
    assert!(!partition.receiver_mutable);
    assert_eq!(partition.return_type, "Membership");
    let heal = cluster
        .functions
        .get(&("heal".to_string(), 3))
        .expect("Cluster.heal receiver method");
    assert!(heal.receiver_method);
    assert!(!heal.receiver_mutable);
    assert_eq!(heal.return_type, "Membership");
    let prune = cluster
        .functions
        .get(&("prune".to_string(), 3))
        .expect("Cluster.prune receiver method");
    assert!(prune.receiver_method);
    assert!(!prune.receiver_mutable);
    assert_eq!(prune.return_type, "Membership");
    let accept = cluster
        .functions
        .get(&("accept".to_string(), 2))
        .expect("Cluster.accept receiver method");
    assert!(accept.receiver_method);
    assert!(!accept.receiver_mutable);
    assert_eq!(accept.return_type, "AcceptResult");
    let send_across_node = cluster
        .traits
        .get("SendAcrossNode")
        .expect("embedded SendAcrossNode trait contract");
    assert!(send_across_node.methods.contains_key("transfer"));
    let cluster_send = cluster
        .functions
        .get(&("send".to_string(), 3))
        .expect("Cluster.send receiver method");
    assert!(cluster_send.receiver_method);
    assert_eq!(cluster_send.return_type, "SendResult");
    assert_eq!(cluster_send.generic_params, vec!["Payload"]);
    assert_eq!(cluster_send.generic_bounds, vec!["SendAcrossNode[Payload]"]);
    let cluster_send_with = cluster
        .functions
        .get(&("send_with".to_string(), 4))
        .expect("Cluster.send_with receiver method");
    assert!(cluster_send_with.receiver_method);
    assert_eq!(cluster_send_with.return_type, "SendResult");
    assert_eq!(cluster_send_with.generic_params, vec!["Payload"]);
    assert_eq!(
        cluster_send_with.generic_bounds,
        vec!["SendAcrossNode[Payload]"]
    );
    let acknowledge = cluster
        .functions
        .get(&("acknowledge".to_string(), 2))
        .expect("Cluster.acknowledge receiver method");
    assert!(acknowledge.receiver_method);
    assert_eq!(acknowledge.return_type, "AcknowledgeResult");
    let frame_message_id = cluster
        .functions
        .get(&("message_id".to_string(), 1))
        .expect("Cluster.Frame.message_id receiver method");
    assert!(frame_message_id.receiver_method);
    assert_eq!(frame_message_id.return_type, "Int");
    let frame_delivery = cluster
        .functions
        .get(&("delivery".to_string(), 1))
        .expect("Cluster.Frame.delivery receiver method");
    assert!(frame_delivery.receiver_method);
    assert_eq!(frame_delivery.return_type, "Delivery");
    let needs_ack = cluster
        .functions
        .get(&("needs_ack".to_string(), 2))
        .expect("Cluster.needs_ack receiver method");
    assert!(needs_ack.receiver_method);
    assert_eq!(needs_ack.return_type, "Bool");
    let pending_ack_count = cluster
        .functions
        .get(&("pending_ack_count".to_string(), 1))
        .expect("Cluster.pending_ack_count receiver method");
    assert!(pending_ack_count.receiver_method);
    assert_eq!(pending_ack_count.return_type, "Int");
    let session_state = cluster
        .functions
        .get(&("state".to_string(), 1))
        .expect("Cluster.Session.state receiver method");
    assert!(session_state.receiver_method);
    assert_eq!(session_state.return_type, "SessionState");
    let disconnect = cluster
        .functions
        .get(&("disconnect".to_string(), 3))
        .expect("Cluster.disconnect receiver method");
    assert!(disconnect.receiver_method);
    assert_eq!(disconnect.return_type, "DisconnectResult");
    let reconnect = cluster
        .functions
        .get(&("reconnect".to_string(), 3))
        .expect("Cluster.reconnect receiver method");
    assert!(reconnect.receiver_method);
    assert_eq!(reconnect.return_type, "ReconnectResult");

    let backpressure = interfaces
        .get("std.vm.Backpressure")
        .expect("embedded VM backpressure interface");
    assert!(backpressure.public_types.contains("Credit"));
    let backpressure_trait = backpressure
        .traits
        .get("Backpressure")
        .expect("embedded Backpressure trait contract");
    assert!(backpressure_trait.methods.contains_key("available"));
    assert!(backpressure_trait.methods.contains_key("request"));
    assert!(backpressure_trait.methods.contains_key("release"));

    let supervisor = interfaces
        .get("std.vm.Supervisor")
        .expect("embedded VM supervisor interface");
    assert!(supervisor.public_types.contains("Supervisor"));
    assert!(supervisor.public_types.contains("ChildSpec"));
    assert!(supervisor.public_types.contains("RestartStrategy"));
    assert!(supervisor.public_types.contains("RestartClass"));
    assert!(supervisor
        .functions
        .contains_key(&("child_spec".to_string(), 7)));
    let supervisor_root = supervisor
        .functions
        .get(&("root".to_string(), 4))
        .expect("Supervisor.root source policy");
    assert!(!supervisor_root.receiver_method);
    assert!(!supervisor_root.receiver_mutable);
    let selects_child = supervisor
        .functions
        .get(&("selects_child".to_string(), 3))
        .expect("Supervisor.selects_child source policy");
    assert!(!selects_child.receiver_method);
    assert!(!selects_child.receiver_mutable);
    assert!(supervisor.traits.contains_key("Supervised"));

    let gen_server = interfaces
        .get("std.vm.GenServer")
        .expect("embedded VM GenServer interface");
    assert!(gen_server.public_types.contains("CallReply"));
    assert!(gen_server.public_types.contains("ServerRef"));
    assert!(gen_server.public_types.contains("GenServerCommand"));
    assert!(gen_server.functions.contains_key(&("start".to_string(), 2)));
    let call = gen_server
        .functions
        .get(&("call".to_string(), 3))
        .expect("GenServer.call source function");
    assert!(!call.receiver_method);
    assert!(!call.receiver_mutable);
    let cast = gen_server
        .functions
        .get(&("cast".to_string(), 2))
        .expect("GenServer.cast source function");
    assert!(!cast.receiver_method);
    assert!(!cast.receiver_mutable);
    let stop = gen_server
        .functions
        .get(&("stop".to_string(), 2))
        .expect("GenServer.stop source function");
    assert!(!stop.receiver_method);
    assert!(!stop.receiver_mutable);

    let native_bridge = interfaces
        .get("std.vm.NativeBridge")
        .expect("embedded VM native bridge interface");
    assert!(native_bridge.opaque_types.contains("NativeBridge"));
    assert!(native_bridge
        .functions
        .contains_key(&("start".to_string(), 1)));
    let native_call = native_bridge
        .functions
        .get(&("call".to_string(), 2))
        .expect("NativeBridge.call receiver method");
    assert!(native_call.receiver_method);
    assert!(!native_call.receiver_mutable);
    let dispose = native_bridge
        .functions
        .get(&("dispose".to_string(), 1))
        .expect("NativeBridge.dispose mutable receiver method");
    assert!(dispose.receiver_method);
    assert!(dispose.receiver_mutable);
    let native_stop = native_bridge
        .functions
        .get(&("stop".to_string(), 1))
        .expect("NativeBridge.stop mutable receiver method");
    assert!(native_stop.receiver_method);
    assert!(native_stop.receiver_mutable);
    let native_bridge_runtime = native_bridge
        .traits
        .get("NativeBridgeRuntime")
        .expect("embedded NativeBridgeRuntime trait contract");
    assert!(native_bridge_runtime
        .super_traits
        .contains(&"Supervised[NativeBridge[Resource]]".to_string()));
    assert!(native_bridge_runtime
        .super_traits
        .contains(&"Backpressure[NativeBridge[Resource]]".to_string()));
    assert!(native_bridge_runtime
        .super_traits
        .contains(&"MessageCodec[Command]".to_string()));
    assert!(native_bridge_runtime
        .super_traits
        .contains(&"MessageCodec[Reply]".to_string()));

    let task = interfaces
        .get("std.vm.Task")
        .expect("embedded VM Task interface");
    assert!(task.public_types.contains("Task"));
    assert!(task.public_types.contains("TaskCommand"));
    assert!(task.functions.contains_key(&("start".to_string(), 2)));
    let result = task
        .functions
        .get(&("result".to_string(), 2))
        .expect("Task.result source function");
    assert!(!result.receiver_method);
    assert!(!result.receiver_mutable);
    let cancel = task
        .functions
        .get(&("cancel".to_string(), 1))
        .expect("Task.cancel source function");
    assert!(!cancel.receiver_method);
    assert!(!cancel.receiver_mutable);
}

/// Verifies embedded std summaries include the VM scheduler contract.
///
/// Inputs:
/// - Compiler-embedded std interface summaries.
///
/// Output:
/// - Test passes when the VM scheduler source façade is available through the
///   embedded summary list with opaque descriptor types and receiver methods.
///
/// Transformation:
/// - Exercises normal embedded summary parsing so distributed scheduler source
///   scenarios can typecheck without file-system summary discovery.
#[test]
fn embedded_std_interfaces_include_vm_scheduler_contract() {
    let mut interfaces = HashMap::new();

    load_embedded_std_interfaces(&mut interfaces);

    let scheduler = interfaces
        .get("std.vm.Scheduler")
        .expect("embedded VM Scheduler interface");
    assert!(scheduler.opaque_types.contains("Node"));
    assert!(scheduler.opaque_types.contains("Scheduler"));
    assert!(scheduler.opaque_types.contains("Policy"));
    assert!(scheduler.opaque_types.contains("Placement"));
    assert!(scheduler.opaque_types.contains("PlacementResult"));
    assert!(scheduler.opaque_types.contains("Migration"));
    assert!(scheduler.opaque_types.contains("MigrationResult"));
    assert!(scheduler.opaque_types.contains("MigrationOutcome"));
    assert!(scheduler.opaque_types.contains("MigrationOutcomeResult"));
    assert!(scheduler.opaque_types.contains("Event"));
    assert!(scheduler.functions.contains_key(&("node".to_string(), 2)));
    assert!(scheduler.functions.contains_key(&("new".to_string(), 1)));
    assert!(scheduler
        .functions
        .contains_key(&("round_robin".to_string(), 0)));
    assert!(scheduler
        .functions
        .contains_key(&("least_connections".to_string(), 0)));
    assert!(scheduler.functions.contains_key(&("pinned".to_string(), 1)));
    assert!(scheduler
        .functions
        .contains_key(&("shard_affinity".to_string(), 2)));
    assert!(scheduler
        .functions
        .contains_key(&("placement_scheduler".to_string(), 1)));
    assert!(scheduler
        .functions
        .contains_key(&("placement_node_id".to_string(), 1)));
    assert!(scheduler
        .functions
        .contains_key(&("declare_route_policy".to_string(), 3)));
    assert!(scheduler
        .functions
        .contains_key(&("declare_actor_group_policy".to_string(), 4)));
    assert!(scheduler
        .functions
        .contains_key(&("place_for_route".to_string(), 4)));
    assert!(scheduler
        .functions
        .contains_key(&("place_for_actor_group".to_string(), 5)));
    assert!(scheduler
        .functions
        .contains_key(&("migration_scheduler".to_string(), 1)));
    assert!(scheduler
        .functions
        .contains_key(&("migration_phase".to_string(), 1)));
    assert!(scheduler
        .functions
        .contains_key(&("commit_migration".to_string(), 2)));
    assert!(scheduler
        .functions
        .contains_key(&("rollback_migration".to_string(), 3)));
    assert!(scheduler
        .functions
        .contains_key(&("abort_migration".to_string(), 3)));
    assert!(scheduler
        .functions
        .contains_key(&("outcome_kind".to_string(), 1)));

    let update_load = scheduler
        .functions
        .get(&("update_load".to_string(), 3))
        .expect("Scheduler.update_load receiver method");
    assert!(update_load.receiver_method);
    assert!(!update_load.receiver_mutable);
    let place = scheduler
        .functions
        .get(&("place".to_string(), 3))
        .expect("Scheduler.place receiver method");
    assert!(place.receiver_method);
    assert!(!place.receiver_mutable);
    let place_for_actor_group = scheduler
        .functions
        .get(&("place_for_actor_group".to_string(), 5))
        .expect("Scheduler.place_for_actor_group receiver method");
    assert!(place_for_actor_group.receiver_method);
    assert!(!place_for_actor_group.receiver_mutable);
    let request_migration = scheduler
        .functions
        .get(&("request_migration".to_string(), 5))
        .expect("Scheduler.request_migration receiver method");
    assert!(request_migration.receiver_method);
    assert!(!request_migration.receiver_mutable);
    let commit_migration = scheduler
        .functions
        .get(&("commit_migration".to_string(), 2))
        .expect("Scheduler.commit_migration receiver method");
    assert!(commit_migration.receiver_method);
    assert!(!commit_migration.receiver_mutable);
    let events_after = scheduler
        .functions
        .get(&("events_after".to_string(), 2))
        .expect("Scheduler.events_after receiver method");
    assert!(events_after.receiver_method);
    assert!(!events_after.receiver_mutable);
}

/// Verifies embedded std summaries include the VM fault contract.
///
/// Inputs:
/// - Compiler-embedded std interface summaries.
///
/// Output:
/// - Test passes when the VM fault source façade is available through the
///   embedded summary list with opaque descriptors and public constructors.
///
/// Transformation:
/// - Exercises normal embedded summary parsing so distributed fault/recovery
///   source scenarios can typecheck without file-system summary discovery.
#[test]
fn embedded_std_interfaces_include_vm_fault_contract() {
    let mut interfaces = HashMap::new();

    load_embedded_std_interfaces(&mut interfaces);

    let fault = interfaces
        .get("std.vm.Fault")
        .expect("embedded VM Fault interface");
    assert!(fault.opaque_types.contains("Policy"));
    assert!(fault.opaque_types.contains("State"));
    assert!(fault.opaque_types.contains("Transition"));
    assert!(fault.opaque_types.contains("Recovery"));
    assert!(fault.opaque_types.contains("Failure"));
    assert!(fault.opaque_types.contains("Rollback"));
    assert!(fault.functions.contains_key(&("policy".to_string(), 3)));
    assert!(fault
        .functions
        .contains_key(&("resolve_policy".to_string(), 2)));
    assert!(fault
        .functions
        .contains_key(&("compatibility".to_string(), 2)));
    assert!(fault.functions.contains_key(&("state".to_string(), 2)));
    assert!(fault
        .functions
        .contains_key(&("classify_heartbeat".to_string(), 5)));
    assert!(fault
        .functions
        .contains_key(&("isolate_partition".to_string(), 5)));
    assert!(fault
        .functions
        .contains_key(&("start_recovery".to_string(), 3)));
    assert!(fault
        .functions
        .contains_key(&("complete_recovery".to_string(), 3)));
    assert!(fault
        .functions
        .contains_key(&("expire_recovery".to_string(), 4)));
    assert!(fault
        .functions
        .contains_key(&("migration_timeout".to_string(), 7)));
    assert!(fault
        .functions
        .contains_key(&("migration_partial_commit".to_string(), 7)));
    assert!(fault
        .functions
        .contains_key(&("stale_placement_update".to_string(), 6)));
    assert!(fault.functions.contains_key(&("failure".to_string(), 1)));
    assert!(fault
        .functions
        .contains_key(&("transition_diagnostic_kind".to_string(), 1)));
    assert!(fault
        .functions
        .contains_key(&("failure_diagnostic_kind".to_string(), 1)));
}

/// Verifies embedded std summaries include the VM distributed state contract.
///
/// Inputs:
/// - Compiler-embedded std interface summaries.
///
/// Output:
/// - Test passes when the VM distributed state source façade is available
///   through the embedded summary list with opaque descriptors and receiver
///   methods.
///
/// Transformation:
/// - Exercises normal embedded summary parsing so distributed state source
///   scenarios can typecheck without file-system summary discovery.
#[test]
fn embedded_std_interfaces_include_vm_distributed_state_contract() {
    let mut interfaces = HashMap::new();

    load_embedded_std_interfaces(&mut interfaces);

    let state = interfaces
        .get("std.vm.DistributedState")
        .expect("embedded VM DistributedState interface");
    assert!(state.opaque_types.contains("Scope"));
    assert!(state.opaque_types.contains("Version"));
    assert!(state.opaque_types.contains("Policy"));
    assert!(state.opaque_types.contains("Store"));
    assert!(state.opaque_types.contains("Entry"));
    assert!(state.opaque_types.contains("Outcome"));
    assert!(state.opaque_types.contains("Conflict"));
    assert!(state.opaque_types.contains("Snapshot"));
    assert!(state.functions.contains_key(&("scope".to_string(), 2)));
    assert!(state.functions.contains_key(&("version".to_string(), 2)));
    assert!(state.functions.contains_key(&("policy".to_string(), 1)));
    assert!(state.functions.contains_key(&("store".to_string(), 0)));
    assert!(state.functions.contains_key(&("conflict".to_string(), 1)));
    assert!(state.functions.contains_key(&("restore".to_string(), 1)));
    assert!(state.functions.contains_key(&("kind".to_string(), 1)));

    let write = state
        .functions
        .get(&("write".to_string(), 6))
        .expect("DistributedState.write receiver method");
    assert!(write.receiver_method);
    assert!(!write.receiver_mutable);
    let get = state
        .functions
        .get(&("get".to_string(), 2))
        .expect("DistributedState.get receiver method");
    assert!(get.receiver_method);
    assert!(!get.receiver_mutable);
    let export_snapshot = state
        .functions
        .get(&("export_snapshot".to_string(), 1))
        .expect("DistributedState.export_snapshot receiver method");
    assert!(export_snapshot.receiver_method);
    assert!(!export_snapshot.receiver_mutable);
}
