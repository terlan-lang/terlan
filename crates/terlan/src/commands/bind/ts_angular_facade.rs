use std::collections::BTreeSet;

use super::ts_dom_module_mapping::{DomModuleMapping, DomModulePlan};

/// Generated Angular.ts facade file bundle.
///
/// Inputs:
/// - Produced from the real `terlan.angular.ng.*` namespace mapping.
///
/// Output:
/// - Module plan plus source, summary, and generated test text.
///
/// Transformation:
/// - Keeps Angular.ts-specific callable wrappers separate from the generic
///   TypeScript DOM binding generator.
pub(super) struct AngularNamespaceFacadeFiles {
    pub(super) plan: DomModulePlan,
    pub(super) source: String,
    pub(super) summary: String,
    pub(super) test: String,
}

/// Returns generated Angular.ts namespace facade files when the input is present.
///
/// Inputs:
/// - `mapping`: TypeScript declaration mapping for a namespace manifest.
///
/// Output:
/// - A generated `terlan.angular.Ng` facade bundle when the real Angular.ts
///   namespace input produced the core `terlan.angular.ng.*` types.
/// - `None` for normal DOM generation.
///
/// Transformation:
/// - Generates Terlan application-authoring entry points beside namespace
///   aliases instead of requiring handwritten JavaScript bootstrap glue.
pub(super) fn angular_namespace_facade_files(
    mapping: &DomModuleMapping,
) -> Option<AngularNamespaceFacadeFiles> {
    let plan = angular_namespace_facade_plan(mapping)?;
    Some(AngularNamespaceFacadeFiles {
        plan,
        source: render_facade_source(true),
        summary: render_facade_source(false),
        test: render_facade_test(),
    })
}

/// Returns the generated Angular.ts namespace facade module plan when present.
fn angular_namespace_facade_plan(mapping: &DomModuleMapping) -> Option<DomModulePlan> {
    let modules = mapping
        .modules
        .iter()
        .map(|module| module.module_path.as_str())
        .collect::<BTreeSet<_>>();
    let required = [
        "terlan.angular.ng.Angular",
        "terlan.angular.ng.NgModule",
        "terlan.angular.ng.Component",
        "terlan.angular.ng.Directive",
        "terlan.angular.ng.Scope",
        "terlan.angular.ng.TemplateCacheService",
        "terlan.angular.ng.HttpService",
        "terlan.angular.ng.HttpResponse",
        "terlan.angular.ng.Machine",
        "terlan.angular.ng.MachineConfig",
        "terlan.angular.ng.MachineSendResult",
        "terlan.angular.ng.MachineService",
        "terlan.angular.ng.MachineSnapshot",
        "terlan.angular.ng.Workflow",
        "terlan.angular.ng.WorkflowResult",
        "terlan.angular.ng.WorkflowService",
        "terlan.angular.ng.WorkflowSnapshot",
        "terlan.angular.ng.SseConfig",
        "terlan.angular.ng.SseConnection",
        "terlan.angular.ng.SseService",
        "terlan.angular.ng.WebSocketConfig",
        "terlan.angular.ng.WebSocketConnection",
        "terlan.angular.ng.WebSocketService",
        "terlan.angular.ng.WorkerConfig",
        "terlan.angular.ng.WorkerHandle",
        "terlan.angular.ng.WorkerService",
    ];
    if required.iter().all(|module| modules.contains(module)) {
        Some(DomModulePlan {
            module_path: "terlan.angular.Ng".to_string(),
            source_interface: "AngularNamespaceFacade".to_string(),
            doc: Some(
                "Generated Angular.ts facade for Terlan-owned modules, directives, templates, machines, workflows, SSE, WebSocket, and worker lifecycles."
                    .to_string(),
            ),
            type_name: "Ng".to_string(),
            type_params: Vec::new(),
            alias_target: None,
            source_path: "terlan/angular/Ng.terl".to_string(),
            interface_path: "terlan/angular/Ng.terli".to_string(),
            summary_path: "std/summaries/terlan.angular.Ng.typi".to_string(),
            test_path: "terlan/angular/NgTest.terl".to_string(),
            members: Vec::new(),
        })
    } else {
        None
    }
}

/// Renders the generated Angular.ts Terlan facade source or summary.
fn render_facade_source(include_bodies: bool) -> String {
    let mut output = String::new();
    output.push_str("module terlan.angular.Ng.\n\n");
    for (doc, signature) in [
        ("Returns the Angular.ts runtime handle.", "pub angular(): terlan.angular.ng.Angular.Angular"),
        ("Creates an Angular.ts module without dependencies.", "pub ng_module(name: std.js.String.JsString): terlan.angular.ng.NgModule.NgModule"),
        ("Creates an Angular.ts module with dependency names.", "pub ng_module_with_dependencies(name: std.js.String.JsString, dependencies: List[std.js.String.JsString]): terlan.angular.ng.NgModule.NgModule"),
        ("Registers a component on an Angular.ts module.", "pub register_component(target: terlan.angular.ng.NgModule.NgModule, name: std.js.String.JsString, component: terlan.angular.ng.Component.Component): terlan.angular.ng.NgModule.NgModule"),
        ("Registers a directive factory on an Angular.ts module.", "pub register_directive(target: terlan.angular.ng.NgModule.NgModule, name: std.js.String.JsString, factory: Dynamic): terlan.angular.ng.NgModule.NgModule"),
        ("Registers a controller factory on an Angular.ts module.", "pub register_controller(target: terlan.angular.ng.NgModule.NgModule, name: std.js.String.JsString, controller: Dynamic): terlan.angular.ng.NgModule.NgModule"),
        ("Runs an Angular.ts scope update cycle.", "pub apply_scope(scope: terlan.angular.ng.Scope.Scope): Unit"),
        ("Stores a template in the Angular.ts template cache.", "pub template_put(templates: terlan.angular.ng.TemplateCacheService.TemplateCacheService, key: std.js.String.JsString, template_source: std.js.String.JsString): terlan.angular.ng.TemplateCacheService.TemplateCacheService"),
        ("Reads a template from the Angular.ts template cache.", "pub template_get(templates: terlan.angular.ng.TemplateCacheService.TemplateCacheService, key: std.js.String.JsString): Option[std.js.String.JsString]"),
        ("Removes a template from the Angular.ts template cache.", "pub template_remove(templates: terlan.angular.ng.TemplateCacheService.TemplateCacheService, key: std.js.String.JsString): Bool"),
        ("Issues an Angular.ts HTTP GET request.", "pub http_get(http: terlan.angular.ng.HttpService.HttpService, url: std.js.String.JsString): std.js.Promise[terlan.angular.ng.HttpResponse.HttpResponse[Dynamic]]"),
        ("Creates a state machine from an explicit Angular.ts machine configuration.", "pub machine(service: terlan.angular.ng.MachineService.MachineService, config: terlan.angular.ng.MachineConfig.MachineConfig[Dynamic]): terlan.angular.ng.Machine.Machine[Dynamic]"),
        ("Sends an event and payload through a generated machine wrapper.", "pub machine_send(machine: terlan.angular.ng.Machine.Machine[Dynamic], event: std.js.String.JsString, payload: Dynamic): terlan.angular.ng.MachineSendResult.MachineSendResult[std.js.String.JsString]"),
        ("Snapshots a generated machine wrapper.", "pub machine_snapshot(machine: terlan.angular.ng.Machine.Machine[Dynamic]): terlan.angular.ng.MachineSnapshot.MachineSnapshot[Dynamic]"),
        ("Creates a workflow from an explicit Angular.ts workflow configuration.", "pub workflow(service: terlan.angular.ng.WorkflowService.WorkflowService, config: Dynamic): terlan.angular.ng.Workflow.Workflow[Dynamic]"),
        ("Runs one named workflow command with a payload.", "pub workflow_run(workflow: terlan.angular.ng.Workflow.Workflow[Dynamic], command: std.js.String.JsString, input: Dynamic): std.js.Promise[terlan.angular.ng.WorkflowResult.WorkflowResult[Dynamic]]"),
        ("Snapshots a generated workflow wrapper.", "pub workflow_snapshot(workflow: terlan.angular.ng.Workflow.Workflow[Dynamic]): terlan.angular.ng.WorkflowSnapshot.WorkflowSnapshot[Dynamic]"),
        ("Opens an SSE connection with Angular.ts defaults.", "pub sse_connect(service: terlan.angular.ng.SseService.SseService, url: std.js.String.JsString): terlan.angular.ng.SseConnection.SseConnection"),
        ("Opens an SSE connection with explicit options.", "pub sse_connect_with_config(service: terlan.angular.ng.SseService.SseService, url: std.js.String.JsString, config: terlan.angular.ng.SseConfig.SseConfig): terlan.angular.ng.SseConnection.SseConnection"),
        ("Reconnects an SSE connection while preserving its listeners.", "pub sse_reconnect(connection: terlan.angular.ng.SseConnection.SseConnection): Unit"),
        ("Closes an SSE connection and its reconnect lifecycle.", "pub sse_close(connection: terlan.angular.ng.SseConnection.SseConnection): Unit"),
        ("Opens a WebSocket connection with Angular.ts defaults.", "pub websocket_connect(service: terlan.angular.ng.WebSocketService.WebSocketService, url: std.js.String.JsString): terlan.angular.ng.WebSocketConnection.WebSocketConnection"),
        ("Opens a WebSocket connection with explicit options.", "pub websocket_connect_with_config(service: terlan.angular.ng.WebSocketService.WebSocketService, url: std.js.String.JsString, config: terlan.angular.ng.WebSocketConfig.WebSocketConfig): terlan.angular.ng.WebSocketConnection.WebSocketConnection"),
        ("Sends one value through a managed WebSocket connection.", "pub websocket_send(connection: terlan.angular.ng.WebSocketConnection.WebSocketConnection, data: Dynamic): Unit"),
        ("Closes a managed WebSocket connection.", "pub websocket_close(connection: terlan.angular.ng.WebSocketConnection.WebSocketConnection): Unit"),
        ("Starts a managed worker with Angular.ts defaults.", "pub worker_start(service: terlan.angular.ng.WorkerService.WorkerService, script_path: std.js.String.JsString): terlan.angular.ng.WorkerHandle.WorkerHandle[Dynamic, Dynamic]"),
        ("Starts a managed worker with explicit decode and restart options.", "pub worker_start_with_config(service: terlan.angular.ng.WorkerService.WorkerService, script_path: std.js.String.JsString, config: terlan.angular.ng.WorkerConfig.WorkerConfig[Dynamic]): terlan.angular.ng.WorkerHandle.WorkerHandle[Dynamic, Dynamic]"),
        ("Subscribes to worker messages and returns the unsubscribe callback.", "pub worker_on_message(worker: terlan.angular.ng.WorkerHandle.WorkerHandle[Dynamic, Dynamic], listener: Dynamic): Dynamic"),
        ("Subscribes to worker errors and returns the unsubscribe callback.", "pub worker_on_error(worker: terlan.angular.ng.WorkerHandle.WorkerHandle[Dynamic, Dynamic], listener: Dynamic): Dynamic"),
        ("Permanently terminates a managed worker and its callback lifecycle.", "pub worker_terminate(worker: terlan.angular.ng.WorkerHandle.WorkerHandle[Dynamic, Dynamic]): Unit"),
        ("Constructs a directive shape with an explicit restrict mode and link callback.", "pub directive_with_link(restrict: std.js.String.JsString, link: Dynamic): terlan.angular.ng.Directive.Directive[Dynamic]"),
    ] {
        push_facade_function(&mut output, doc, signature, include_bodies);
    }
    output
}

/// Renders the generated Angular.ts facade surface test source.
fn render_facade_test() -> String {
    let mut output = String::new();
    output.push_str("module terlan.angular.NgTest.\n\n");
    output.push_str("pub generated_angular_facade_surface_contract(): Bool ->\n    true.\n");
    output.push_str(
        "\n\
pub ng_module_typechecks(name: std.js.String.JsString): terlan.angular.ng.NgModule.NgModule ->\n\
    terlan.angular.Ng.ng_module(name).\n",
    );
    output.push_str(
        "\n\
pub component_typechecks(target: terlan.angular.ng.NgModule.NgModule, name: std.js.String.JsString, component: terlan.angular.ng.Component.Component): terlan.angular.ng.NgModule.NgModule ->\n\
    terlan.angular.Ng.register_component(target, name, component).\n",
    );
    output.push_str(
        "\n\
pub callback_lifecycle_typechecks(worker: terlan.angular.ng.WorkerHandle.WorkerHandle[Dynamic, Dynamic], listener: Dynamic): Dynamic ->\n\
    terlan.angular.Ng.worker_on_message(worker, listener).\n",
    );
    output
}

/// Appends one documented facade signature.
fn push_facade_function(output: &mut String, doc: &str, signature: &str, include_body: bool) {
    output.push_str("/**\n");
    output.push_str(" * ");
    output.push_str(doc);
    output.push_str("\n */\n");
    output.push_str(&render_signature(signature, include_body));
    output.push('\n');
}

/// Renders a signature as either a native source body or declaration.
fn render_signature(signature: &str, include_body: bool) -> String {
    if include_body {
        format!("{signature} ->\n    native.\n")
    } else {
        format!("{signature}.\n")
    }
}
