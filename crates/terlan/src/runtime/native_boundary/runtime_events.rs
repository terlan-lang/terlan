//! Bounded NativeBoundary resource lifecycle telemetry.

use std::collections::VecDeque;

use serde::Serialize;

use crate::terlan_native_boundary::{
    handle::NativeBoundaryHandle,
    metadata::{postgres_worker_manifest, NativeBoundaryWorkerClass},
    term::{NativeBoundaryReplyTerm, NativeBoundaryTerm},
};

const RESOURCE_EVENT_HISTORY_LIMIT: usize = 1_024;

/// Resource transition observed at the NativeBoundary term boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeBoundaryResourceOutcome {
    /// A successful call returned a newly registered opaque handle.
    Created,
    /// An owning process successfully disposed an opaque handle.
    Disposed,
    /// Resource validation rejected a stale, mismatched, or unauthorized handle.
    Rejected,
}

/// Inspectable NativeBoundary resource lifecycle event.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeBoundaryResourceEvent {
    /// VM process owning or attempting the resource operation.
    pub owner_process_id: u64,
    /// Native operation associated with the transition.
    pub operation: String,
    /// Opaque resource slot id when one was available.
    pub handle_id: Option<u64>,
    /// Opaque resource generation when one was available.
    pub generation: Option<u64>,
    /// Resource lifecycle outcome.
    pub outcome: NativeBoundaryResourceOutcome,
    /// Stable typed resource error code for rejected transitions.
    pub error_code: Option<String>,
}

/// Manifest-correlated NativeBoundary dispatch event.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeBoundaryDispatchEvent {
    /// VM process that attempted the native operation.
    pub owner_process_id: u64,
    /// Compiler-native operation identifier.
    pub operation: String,
    /// Manifest worker class, or `None` for an operation not yet manifested.
    pub worker_class: Option<String>,
    /// Stable error code when dispatch returned a typed failure.
    pub error_code: Option<String>,
}

/// Bounded resource event history owned by one NativeBoundary runtime.
#[derive(Debug, Default)]
pub(super) struct NativeBoundaryResourceEventLog {
    events: VecDeque<NativeBoundaryResourceEvent>,
    dispatch_events: VecDeque<NativeBoundaryDispatchEvent>,
}

impl NativeBoundaryResourceEventLog {
    pub(super) fn observe_call(
        &mut self,
        owner_process_id: u64,
        operation: &str,
        args: &[NativeBoundaryTerm],
        reply: &NativeBoundaryReplyTerm,
    ) {
        self.push_dispatch(owner_process_id, operation, reply);
        match reply {
            NativeBoundaryReplyTerm::Ok(term) => {
                let mut pending = vec![term];
                while let Some(term) = pending.pop() {
                    match term {
                        NativeBoundaryTerm::Handle { id, generation } => self.push(
                            owner_process_id,
                            operation,
                            Some(NativeBoundaryHandle {
                                id: *id,
                                generation: *generation,
                            }),
                            NativeBoundaryResourceOutcome::Created,
                            None,
                        ),
                        NativeBoundaryTerm::OptionalHandle(Some(handle)) => self.push(
                            owner_process_id,
                            operation,
                            Some(*handle),
                            NativeBoundaryResourceOutcome::Created,
                            None,
                        ),
                        NativeBoundaryTerm::List(values) => pending.extend(values.iter()),
                        _ => {}
                    }
                }
            }
            NativeBoundaryReplyTerm::Error { code, .. } if code.starts_with("resource.") => self
                .push(
                    owner_process_id,
                    operation,
                    first_handle(args),
                    NativeBoundaryResourceOutcome::Rejected,
                    Some(code.clone()),
                ),
            NativeBoundaryReplyTerm::Error { .. } => {}
        }
    }

    pub(super) fn record_dispose(
        &mut self,
        owner_process_id: u64,
        handle: NativeBoundaryHandle,
        reply: &NativeBoundaryReplyTerm,
    ) {
        let (outcome, error_code) = match reply {
            NativeBoundaryReplyTerm::Ok(_) => (NativeBoundaryResourceOutcome::Disposed, None),
            NativeBoundaryReplyTerm::Error { code, .. } => {
                (NativeBoundaryResourceOutcome::Rejected, Some(code.clone()))
            }
        };
        self.push(
            owner_process_id,
            "resource.dispose",
            Some(handle),
            outcome,
            error_code,
        );
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = &NativeBoundaryResourceEvent> {
        self.events.iter()
    }

    pub(super) fn dispatch_iter(&self) -> impl Iterator<Item = &NativeBoundaryDispatchEvent> {
        self.dispatch_events.iter()
    }

    fn push(
        &mut self,
        owner_process_id: u64,
        operation: &str,
        handle: Option<NativeBoundaryHandle>,
        outcome: NativeBoundaryResourceOutcome,
        error_code: Option<String>,
    ) {
        if self.events.len() == RESOURCE_EVENT_HISTORY_LIMIT {
            self.events.pop_front();
        }
        self.events.push_back(NativeBoundaryResourceEvent {
            owner_process_id,
            operation: operation.to_string(),
            handle_id: handle.map(|value| value.id),
            generation: handle.map(|value| value.generation),
            outcome,
            error_code,
        });
    }

    fn push_dispatch(
        &mut self,
        owner_process_id: u64,
        operation: &str,
        reply: &NativeBoundaryReplyTerm,
    ) {
        if self.dispatch_events.len() == RESOURCE_EVENT_HISTORY_LIMIT {
            self.dispatch_events.pop_front();
        }
        let worker_class = postgres_worker_manifest()
            .export_for_operation(operation)
            .map(|export| worker_class_name(export.worker_class).to_string());
        let error_code = match reply {
            NativeBoundaryReplyTerm::Ok(_) => None,
            NativeBoundaryReplyTerm::Error { code, .. } => Some(code.clone()),
        };
        self.dispatch_events.push_back(NativeBoundaryDispatchEvent {
            owner_process_id,
            operation: operation.to_string(),
            worker_class,
            error_code,
        });
    }
}

fn worker_class_name(worker_class: NativeBoundaryWorkerClass) -> &'static str {
    match worker_class {
        NativeBoundaryWorkerClass::Fast => "fast",
        NativeBoundaryWorkerClass::Blocking => "blocking",
        NativeBoundaryWorkerClass::LongRunningCancellable => "long_running_cancellable",
        NativeBoundaryWorkerClass::Sandboxed => "sandboxed",
        NativeBoundaryWorkerClass::ResourceOwning => "resource_owning",
    }
}

fn first_handle(terms: &[NativeBoundaryTerm]) -> Option<NativeBoundaryHandle> {
    let mut pending = terms.iter().collect::<Vec<_>>();
    while let Some(term) = pending.pop() {
        match term {
            NativeBoundaryTerm::Handle { id, generation } => {
                return Some(NativeBoundaryHandle {
                    id: *id,
                    generation: *generation,
                });
            }
            NativeBoundaryTerm::OptionalHandle(Some(handle)) => return Some(*handle),
            NativeBoundaryTerm::List(values) => pending.extend(values.iter()),
            _ => {}
        }
    }
    None
}
