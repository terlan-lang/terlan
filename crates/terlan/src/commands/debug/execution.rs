//! Scripted control of one admitted native debugger execution shard.

mod control;
mod inspection;
mod source;

use std::collections::BTreeSet;

use crate::runtime::native_image::control::TvmTransitionOperation;
use crate::runtime::native_image::debug::TvmNativeDebugRecord;
use crate::runtime::vm::debugger_control::{VmDebuggerControlCommand, VmDebuggerScheduleControl};
use crate::runtime::vm::process::VmProcessId;
use crate::runtime::vm::pure_native::{
    PureNativeExecution, PureNativeExecutionShard, PureNativeSuspension,
};

use super::script::DebugScriptCommand;
use super::session::DebugBreakpointResolution;
use super::DebugCliError;

const MAX_CONTINUE_TRANSITIONS: usize = 1_048_576;
const MAX_MAILBOX_MESSAGES: usize = 128;

use super::presentation::{
    render_bounded, render_capture_values, render_native_slots, source_location, state_name,
};
use super::script::required_argument;
use source::{continuation_local_names, source_for_continuation, BreakpointAction};

/// Observable result of executing a debugger script against one AOT shard.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct NativeDebuggerExecutionReport {
    pub(super) live_execution: bool,
    pub(super) execution_state: String,
    pub(super) events: Vec<String>,
    pub(super) result: Option<String>,
    pub(super) process_snapshots: Vec<String>,
    pub(super) resource_snapshots: Vec<String>,
    pub(super) timer_snapshots: Vec<String>,
    pub(super) mailbox_snapshots: Vec<String>,
}

impl NativeDebuggerExecutionReport {
    pub(super) fn admitted() -> Self {
        Self {
            live_execution: false,
            execution_state: "running".to_string(),
            events: Vec::new(),
            result: None,
            process_snapshots: Vec::new(),
            resource_snapshots: Vec::new(),
            timer_snapshots: Vec::new(),
            mailbox_snapshots: Vec::new(),
        }
    }
}

struct ActiveCall {
    owner: VmProcessId,
    function: String,
    source: TvmNativeDebugRecord,
    instruction_offset: usize,
    state: ActiveCallState,
}

enum ActiveCallState {
    Ready,
    Suspended(Box<PureNativeSuspension>),
}

pub(super) struct NativeDebuggerRuntime<'a> {
    shard: &'a mut PureNativeExecutionShard,
    source_records: &'a [TvmNativeDebugRecord],
    entry_hint: Option<&'a str>,
    breakpoints: Vec<RuntimeBreakpoint>,
    control: VmDebuggerScheduleControl,
    active: Option<ActiveCall>,
    selected_process: Option<VmProcessId>,
    trace_filters: BTreeSet<String>,
    report: NativeDebuggerExecutionReport,
}

#[derive(Clone)]
struct RuntimeBreakpoint {
    id: usize,
    resolution: DebugBreakpointResolution,
    enabled: bool,
}

/// Executes validated commands through one VM-owned native shard.
pub(super) fn execute_debug_script(
    shard: &mut PureNativeExecutionShard,
    source_records: &[TvmNativeDebugRecord],
    initial_breakpoints: &[DebugBreakpointResolution],
    commands: Option<&[DebugScriptCommand]>,
    entry_hint: Option<&str>,
) -> Result<NativeDebuggerExecutionReport, DebugCliError> {
    let Some(commands) = commands else {
        return Ok(NativeDebuggerExecutionReport::admitted());
    };
    let mut runtime =
        NativeDebuggerRuntime::new(shard, source_records, initial_breakpoints, entry_hint);
    for command in commands {
        if let Err(message) = runtime.execute(command) {
            let _ = runtime.abort("debugger command failed");
            return Err(DebugCliError {
                code: "debug_script_execution_failed",
                message: format!("line {}: {message}", command.line),
            });
        }
        if command.name == "quit" {
            break;
        }
    }
    runtime.finish()
}

impl<'a> NativeDebuggerRuntime<'a> {
    pub(super) fn new(
        shard: &'a mut PureNativeExecutionShard,
        source_records: &'a [TvmNativeDebugRecord],
        initial_breakpoints: &[DebugBreakpointResolution],
        entry_hint: Option<&'a str>,
    ) -> Self {
        let breakpoints = initial_breakpoints
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, resolution)| RuntimeBreakpoint {
                id: index + 1,
                resolution,
                enabled: true,
            })
            .collect();
        Self {
            shard,
            source_records,
            entry_hint,
            breakpoints,
            control: VmDebuggerScheduleControl::running(),
            active: None,
            selected_process: None,
            trace_filters: BTreeSet::new(),
            report: NativeDebuggerExecutionReport {
                live_execution: true,
                ..NativeDebuggerExecutionReport::admitted()
            },
        }
    }

    pub(super) fn event_count(&self) -> usize {
        self.report.events.len()
    }

    pub(super) fn events_from(&self, start: usize) -> &[String] {
        &self.report.events[start..]
    }

    pub(super) fn execute(&mut self, command: &DebugScriptCommand) -> Result<(), DebugCliError> {
        match command.name.as_str() {
            "run" => self.run(),
            "pause" => self.apply_control(VmDebuggerControlCommand::Pause, "paused"),
            "continue" => self.continue_execution(),
            "step" | "next" => self.step(&command.name),
            "finish" => self.finish_frame(),
            "break" => self.add_breakpoint(required_argument(command)?),
            "list" => {
                self.list_breakpoints();
                Ok(())
            }
            "remove" => {
                self.update_breakpoint(required_argument(command)?, BreakpointAction::Remove)
            }
            "enable" => {
                self.update_breakpoint(required_argument(command)?, BreakpointAction::Enable)
            }
            "disable" => {
                self.update_breakpoint(required_argument(command)?, BreakpointAction::Disable)
            }
            "processes" => {
                self.capture_processes();
                Ok(())
            }
            "resources" => {
                self.capture_resources();
                Ok(())
            }
            "trace" => self.update_trace(required_argument(command)?, true),
            "untrace" => self.update_trace(required_argument(command)?, false),
            "mailbox" => self.capture_mailbox(),
            "bt" | "frame" | "locals" | "args" | "process" | "restarts" => self.inspect(command),
            "print" | "eval" => self.evaluate(required_argument(command)?),
            "restart" => self.restart(required_argument(command)?),
            "use" => self.use_value(required_argument(command)?),
            "abort" => self.abort("debugger abort"),
            "quit" => self.abort("debugger quit"),
            "help" => {
                self.report.events.push("help".to_string());
                Ok(())
            }
            name => Err(format!(
                "error[vm.debugger.command]: unsupported debugger command `{name}`"
            )
            .into()),
        }
    }

    fn run(&mut self) -> Result<(), DebugCliError> {
        if self.active.is_some() {
            return Err(
                "error[vm.debugger.run_active]: a debug actor is already active"
                    .to_string()
                    .into(),
            );
        }
        let record = self
            .source_records
            .iter()
            .find(|record| {
                self.entry_hint.is_some_and(|entry| {
                    record.arity == 0 && format!("{}.{}", record.module, record.function) == entry
                })
            })
            .or_else(|| {
                self.source_records
                    .iter()
                    .find(|record| record.function == "main" && record.arity == 0)
            })
            .or_else(|| self.source_records.iter().find(|record| record.arity == 0))
            .ok_or_else(|| {
                "error[vm.debugger.entry]: native image has no zero-arity debug entry".to_string()
            })?;
        if !self.shard.has_export(&record.function, record.arity) {
            return Err(format!(
                "error[vm.debugger.entry]: `{}` is not an admitted native export",
                record.function
            )
            .into());
        }
        let owner = self
            .shard
            .spawn_fixed_owner_actor(&record.function, record.arity)?;
        self.active = Some(ActiveCall {
            owner,
            function: record.function.clone(),
            source: record.clone(),
            instruction_offset: 0,
            state: ActiveCallState::Ready,
        });
        self.selected_process = Some(owner);
        self.report.events.push(format!(
            "process_started:{}:{}.{}",
            owner.as_u64(),
            record.module,
            record.function
        ));
        if super::tracing::event_enabled(
            &self.trace_filters,
            "processes",
            owner.as_u64(),
            &record.module,
            &record.function,
        ) {
            self.report
                .events
                .push(format!("trace:process:{}:started", owner.as_u64()));
        }
        if super::tracing::event_enabled(
            &self.trace_filters,
            "calls",
            owner.as_u64(),
            &record.module,
            &record.function,
        ) {
            self.report.events.push(format!(
                "trace:call:{}:{}.{}",
                owner.as_u64(),
                record.module,
                record.function
            ));
        }
        if self.breakpoint_matches(record)? {
            self.apply_control(VmDebuggerControlCommand::Pause, "breakpoint")?;
            self.report.events.push(format!(
                "stopped:breakpoint:{}:{}.{}",
                owner.as_u64(),
                record.module,
                record.function
            ));
            return Ok(());
        }
        self.continue_execution()
    }

    fn continue_execution(&mut self) -> Result<(), DebugCliError> {
        self.apply_control(VmDebuggerControlCommand::Continue, "continued")?;
        for _ in 0..MAX_CONTINUE_TRANSITIONS {
            if self.active.is_none() {
                return Ok(());
            }
            let suspended = self.advance_once()?;
            if let Some(operation) = suspended {
                self.control.apply(VmDebuggerControlCommand::Pause)?;
                self.report.execution_state = "paused".to_string();
                self.report
                    .events
                    .push(format!("stopped:transition:{operation:?}"));
                return Ok(());
            }
        }
        Err(
            "error[vm.debugger.transition_limit]: continue transition limit exceeded"
                .to_string()
                .into(),
        )
    }

    fn step(&mut self, command: &str) -> Result<(), DebugCliError> {
        self.control
            .apply(VmDebuggerControlCommand::Step { slices: 1 })?;
        let _permit = self
            .control
            .claim_runnable_slice()
            .ok_or_else(|| "error[vm.debugger.step]: no step permit was issued".to_string())?;
        let suspended = self.advance_once()?;
        self.report.execution_state = state_name(self.control.snapshot().state).to_string();
        self.report.events.push(match suspended {
            Some(operation) => format!("{command}:transition:{operation:?}"),
            None => format!("{command}:complete"),
        });
        Ok(())
    }

    fn finish_frame(&mut self) -> Result<(), DebugCliError> {
        self.apply_control(VmDebuggerControlCommand::Continue, "finish")?;
        for _ in 0..MAX_CONTINUE_TRANSITIONS {
            if self.active.is_none() {
                self.report.events.push("finish:complete".to_string());
                return Ok(());
            }
            if matches!(
                self.advance_once()?,
                Some(TvmTransitionOperation::Debug | TvmTransitionOperation::Failure)
            ) {
                self.control.apply(VmDebuggerControlCommand::Pause)?;
                self.report.execution_state = "paused".to_string();
                self.report.events.push("finish:condition".to_string());
                return Ok(());
            }
        }
        Err(
            "error[vm.debugger.transition_limit]: finish transition limit exceeded"
                .to_string()
                .into(),
        )
    }

    fn advance_once(&mut self) -> Result<Option<TvmTransitionOperation>, DebugCliError> {
        let active = self
            .active
            .take()
            .ok_or_else(|| "error[vm.debugger.no_process]: no active debug actor".to_string())?;
        let ActiveCall {
            owner,
            function,
            source,
            instruction_offset,
            state,
        } = active;
        let execution = match state {
            ActiveCallState::Ready => self.shard.begin_debug_call(owner, &function, &[])?,
            ActiveCallState::Suspended(suspension)
                if suspension.operation() == TvmTransitionOperation::Capability =>
            {
                let wait = self.shard.begin_capability_call(owner, &suspension)?;
                let reply = crate::runtime::vm::package_native_helper::dispatch_vm_capability(
                    wait.request(),
                )
                .map_err(String::from)?;
                self.shard
                    .resume_capability_call(owner, *suspension, wait, reply)?
            }
            ActiveCallState::Suspended(suspension) => self.shard.resume_call(owner, *suspension)?,
        };
        self.accept_execution(
            ActiveCall {
                owner,
                function,
                source,
                instruction_offset,
                state: ActiveCallState::Ready,
            },
            execution,
        )
    }

    fn accept_execution(
        &mut self,
        active: ActiveCall,
        execution: PureNativeExecution,
    ) -> Result<Option<TvmTransitionOperation>, DebugCliError> {
        match execution {
            PureNativeExecution::Complete(value) => {
                self.shard.finish_completed_call(active.owner)?;
                self.report.result = Some(render_bounded(&value.render()));
                self.report
                    .events
                    .push(format!("process_exited:{}:normal", active.owner.as_u64()));
                if super::tracing::event_enabled(
                    &self.trace_filters,
                    "processes",
                    active.owner.as_u64(),
                    &active.source.module,
                    &active.source.function,
                ) {
                    self.report.events.push(format!(
                        "trace:process:{}:exited:normal",
                        active.owner.as_u64()
                    ));
                }
                if super::tracing::event_enabled(
                    &self.trace_filters,
                    "returns",
                    active.owner.as_u64(),
                    &active.source.module,
                    &active.source.function,
                ) {
                    self.report.events.push(format!(
                        "trace:return:{}:{}",
                        active.owner.as_u64(),
                        self.report.result.as_deref().unwrap_or("Unit")
                    ));
                }
                Ok(None)
            }
            PureNativeExecution::HttpResponse(_) => {
                if super::tracing::event_enabled(
                    &self.trace_filters,
                    "http",
                    active.owner.as_u64(),
                    &active.source.module,
                    &active.source.function,
                ) {
                    self.report
                        .events
                        .push(format!("trace:http:{}:response", active.owner.as_u64()));
                }
                self.shard.cancel_call(
                    active.owner,
                    "HTTP response cannot be rendered as a debugger expression result",
                )?;
                Err(
                    "error[vm.debugger.result]: HTTP response returned through debugger value entry"
                        .to_string()
                        .into(),
                )
            }
            PureNativeExecution::Suspended(suspension) => {
                let operation = suspension.operation();
                let continuation_id = suspension.continuation_id();
                let source =
                    source_for_continuation(self.source_records, &active.source, continuation_id);
                let process_source = crate::runtime::vm::process::VmProcessSource::new(
                    &source.module,
                    &source.function,
                    source.arity,
                )
                .with_source_path(&source.source_file);
                self.shard.debugger_set_location(
                    active.owner,
                    process_source,
                    usize::try_from(continuation_id).unwrap_or(usize::MAX),
                )?;
                self.active = Some(ActiveCall {
                    state: ActiveCallState::Suspended(suspension),
                    source: source.clone(),
                    instruction_offset: usize::try_from(continuation_id).unwrap_or(usize::MAX),
                    ..active
                });
                if super::tracing::event_enabled(
                    &self.trace_filters,
                    "transitions",
                    active.owner.as_u64(),
                    &source.module,
                    &source.function,
                ) {
                    self.report.events.push(format!(
                        "trace:transition:{}:{operation:?}:{continuation_id}",
                        active.owner.as_u64()
                    ));
                }
                self.report.events.extend(super::tracing::transition_events(
                    &self.trace_filters,
                    operation.clone(),
                    active.owner.as_u64(),
                    &source.module,
                    &source.function,
                    self.active
                        .as_ref()
                        .and_then(|active| match &active.state {
                            ActiveCallState::Suspended(suspension) => Some(suspension.arguments()),
                            ActiveCallState::Ready => None,
                        })
                        .unwrap_or(&[]),
                ));
                Ok(Some(operation))
            }
        }
    }

    fn apply_control(
        &mut self,
        command: VmDebuggerControlCommand,
        event: &str,
    ) -> Result<(), DebugCliError> {
        let snapshot = self.control.apply(command)?;
        self.report.execution_state = state_name(snapshot.state).to_string();
        self.report.events.push(format!(
            "{event}:{}:{}",
            self.report.execution_state, snapshot.remaining_step_slices
        ));
        Ok(())
    }

    fn breakpoint_matches(&self, record: &TvmNativeDebugRecord) -> Result<bool, DebugCliError> {
        let prefix = format!("{}.{}/", record.module, record.function);
        for breakpoint in &self.breakpoints {
            if !breakpoint.enabled
                || !breakpoint
                    .resolution
                    .functions
                    .iter()
                    .any(|function| function.starts_with(&prefix))
            {
                continue;
            }
            let condition = breakpoint
                .resolution
                .spec
                .split_once(" where ")
                .map(|(_, condition)| condition.trim());
            match condition {
                None => return Ok(true),
                Some(condition) => {
                    let value = super::evaluation::evaluate_frame_expression(condition)
                        .map_err(|message| {
                            format!(
                                "error[vm.debugger.condition_unsupported]: conditional breakpoint `{condition}` failed: {message}"
                            )
                        })?;
                    match value.as_str() {
                        "true" => return Ok(true),
                        "false" => continue,
                        _ => {
                            return Err(format!(
                                "error[vm.debugger.condition_type]: conditional breakpoint `{condition}` returned `{value}`, expected Bool"
                            )
                            .into());
                        }
                    }
                }
            }
        }
        Ok(false)
    }

    fn add_breakpoint(&mut self, spec: &str) -> Result<(), DebugCliError> {
        let resolution = super::session::resolve_breakpoint(spec, self.source_records)
            .map_err(|error| format!("error[{}]: {}", error.code, error.message))?;
        let id = self
            .breakpoints
            .last()
            .map_or(1, |breakpoint| breakpoint.id + 1);
        self.breakpoints.push(RuntimeBreakpoint {
            id,
            resolution,
            enabled: true,
        });
        self.report.events.push(format!("breakpoint_added:{id}"));
        Ok(())
    }

    fn list_breakpoints(&mut self) {
        for breakpoint in &self.breakpoints {
            self.report.events.push(format!(
                "breakpoint:{}:{}:{}",
                breakpoint.id,
                if breakpoint.enabled {
                    "enabled"
                } else {
                    "disabled"
                },
                breakpoint.resolution.spec
            ));
        }
    }

    fn update_breakpoint(
        &mut self,
        selector: &str,
        action: BreakpointAction,
    ) -> Result<(), DebugCliError> {
        let selected = selector.parse::<usize>().ok();
        let breakpoint = self
            .breakpoints
            .iter_mut()
            .find(|breakpoint| {
                selected == Some(breakpoint.id) || breakpoint.resolution.spec == selector
            })
            .ok_or_else(|| {
                format!(
                    "error[vm.debugger.breakpoint_missing]: breakpoint `{selector}` was not found"
                )
            })?;
        let id = breakpoint.id;
        let event = match action {
            BreakpointAction::Enable => {
                breakpoint.enabled = true;
                "breakpoint_enabled"
            }
            BreakpointAction::Disable => {
                breakpoint.enabled = false;
                "breakpoint_disabled"
            }
            BreakpointAction::Remove => {
                self.breakpoints.retain(|candidate| candidate.id != id);
                "breakpoint_removed"
            }
        };
        self.report.events.push(format!("{event}:{id}"));
        Ok(())
    }

    fn inspect(&mut self, command: &DebugScriptCommand) -> Result<(), DebugCliError> {
        let Some(active) = &self.active else {
            return Err("error[vm.debugger.no_process]: no active debug actor"
                .to_string()
                .into());
        };
        let selected = self.selected_process.unwrap_or(active.owner);
        let snapshot = self
            .shard
            .debugger_process_snapshots()
            .into_iter()
            .find(|snapshot| snapshot.pid == selected)
            .ok_or_else(|| {
                format!(
                    "error[vm.debugger.process_missing]: process {} exited during debug pause",
                    selected.as_u64()
                )
            })?;
        let detail = match command.name.as_str() {
            "bt" => format!(
                "{}:{snapshot:?}:{:?}",
                source_location(&active.source, active.instruction_offset),
                self.shard.debugger_failure_snapshot(selected)?
            ),
            "frame" => {
                if command.argument.as_deref() != Some("1") {
                    return Err(
                        "error[vm.debugger.frame_missing]: native actor has only frame 1"
                            .to_string()
                            .into(),
                    );
                }
                source_location(&active.source, active.instruction_offset)
            }
            "args" => match &active.state {
                ActiveCallState::Ready => "[]".to_string(),
                ActiveCallState::Suspended(suspension) => {
                    render_native_slots("arg", suspension.arguments())
                }
            },
            "locals" => match &active.state {
                ActiveCallState::Ready => "[]".to_string(),
                ActiveCallState::Suspended(suspension) => {
                    let captures = self
                        .shard
                        .debugger_capture_values(active.owner, suspension)?;
                    render_capture_values(
                        &captures,
                        continuation_local_names(&active.source, active.instruction_offset),
                    )
                }
            },
            "process" => {
                let selected = required_argument(command)?.parse::<u64>().map_err(|_| {
                    "error[vm.debugger.process_selector]: process id must be positive".to_string()
                })?;
                let selected_pid = VmProcessId::from_native_owner(selected).map_err(|_| {
                    "error[vm.debugger.process_selector]: process id must be positive".to_string()
                })?;
                let snapshot = self
                    .shard
                    .debugger_process_snapshots()
                    .into_iter()
                    .find(|snapshot| snapshot.pid == selected_pid)
                    .ok_or_else(|| {
                        format!(
                            "error[vm.debugger.process_missing]: process {selected} does not exist"
                        )
                    })?;
                self.selected_process = Some(selected_pid);
                format!("{snapshot:?}")
            }
            "restarts" => match &active.state {
                ActiveCallState::Suspended(suspension)
                    if suspension.operation() == TvmTransitionOperation::Failure =>
                {
                    "[retry, skip, use Unit, abort_process, restart_process]".to_string()
                }
                ActiveCallState::Suspended(suspension)
                    if suspension.operation() == TvmTransitionOperation::Debug =>
                {
                    "[skip, use Unit, abort_process]".to_string()
                }
                _ => "[]".to_string(),
            },
            _ => format!("{snapshot:?}"),
        };
        self.report
            .events
            .push(format!("{}:{detail}", command.name));
        Ok(())
    }

    fn capture_mailbox(&mut self) -> Result<(), DebugCliError> {
        let active = self
            .active
            .as_ref()
            .ok_or_else(|| "error[vm.debugger.no_process]: no active debug actor".to_string())?;
        let selected = self.selected_process.unwrap_or(active.owner);
        let snapshot = self
            .shard
            .debugger_mailbox_snapshot(selected, MAX_MAILBOX_MESSAGES)?;
        self.report.mailbox_snapshots = snapshot
            .messages
            .iter()
            .map(|message| {
                format!(
                    "{}:sequence={}:sender={}:kind={}:priority={:?}:bytes={}:{}",
                    message.id,
                    message.publication_sequence,
                    message.sender.as_u64(),
                    if message.managed { "managed" } else { "value" },
                    message.priority,
                    message.accounted_bytes,
                    render_bounded(&message.payload.render())
                )
            })
            .collect();
        self.report.events.push(format!(
            "mailbox:{}:cursor={}:omitted={}",
            self.report.mailbox_snapshots.len(),
            snapshot.selective_receive_cursor,
            snapshot.omitted_messages
        ));
        if super::tracing::event_enabled(
            &self.trace_filters,
            "mailbox",
            selected.as_u64(),
            &active.source.module,
            &active.source.function,
        ) {
            self.report.events.push(format!(
                "trace:mailbox:{}:messages={}:omitted={}",
                selected.as_u64(),
                snapshot.messages.len(),
                snapshot.omitted_messages
            ));
        }
        Ok(())
    }

    fn evaluate(&mut self, expression: &str) -> Result<(), DebugCliError> {
        let Some(active) = self.active.as_ref() else {
            return Err("error[vm.debugger.no_process]: no active debug actor"
                .to_string()
                .into());
        };
        let captures = match &active.state {
            ActiveCallState::Ready => Vec::new(),
            ActiveCallState::Suspended(suspension) => self
                .shard
                .debugger_capture_values(active.owner, suspension)?,
        };
        let literals = captures
            .iter()
            .map(|value| value.render())
            .collect::<Vec<_>>();
        let expression = super::evaluation::bind_frame_locals(
            expression,
            continuation_local_names(&active.source, active.instruction_offset),
            &literals,
        )?;
        let value = super::evaluation::evaluate_frame_expression(&expression)?;
        self.report
            .events
            .push(format!("eval:{}", render_bounded(&value)));
        Ok(())
    }
}
