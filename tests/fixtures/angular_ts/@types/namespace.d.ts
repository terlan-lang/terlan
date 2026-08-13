import type { Angular as TAngular } from "./angular.ts";

declare global {
  export namespace ng {
    type Angular = TAngular;
    type NgModule = { name: string };
    type Component = { template: string };
    type Directive<TController = unknown> = { controller?: TController };
    type Scope = { $id: number };
    type HttpService = string;
    type HttpResponse<T> = { data: T; status: number };
    type SseConfig = TSseConfig;
    type SseConnection = TSseConnection;
    type SseService = TSseService;
    type RealtimeProtocolEventDetail<T = unknown, TSource = unknown> = { data: T; source: TSource };
    type RealtimeProtocolMessage = { type: string; data?: unknown };
    type TemplateCacheService = Map<string, string>;
    type Machine<TContract = unknown> = TMachine<TContract>;
    type MachineConfig<TContract = unknown> = TMachineConfig<TContract>;
    type MachineSendResult<TState = string> = TMachineSendResult<TState>;
    type MachineService = TMachineService;
    type MachineSnapshot<TContract = unknown> = TMachineSnapshot<TContract>;
    type Workflow<TContract = unknown> = TWorkflow<TContract>;
    type WorkflowResult<TOutput = unknown> = TWorkflowResult<TOutput>;
    type WorkflowService = TWorkflowService;
    type WorkflowSnapshot<TContract = unknown> = TWorkflowSnapshot<TContract>;
    type WebSocketConfig = TWebSocketConfig;
    type WebSocketConnection = TWebSocketConnection;
    type WebSocketService = TWebSocketService;
    type WorkerConfig<TReceive = unknown> = TWorkerConfig<TReceive>;
    type WorkerHandle<TSend = unknown, TReceive = unknown> = TWorkerHandle<TSend, TReceive>;
    type WorkerService = TWorkerService;
  }
}
