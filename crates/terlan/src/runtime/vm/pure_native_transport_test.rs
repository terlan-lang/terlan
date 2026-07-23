//! Managed continuation transport projection tests.

use std::sync::Arc;

use crate::runtime::native_image::control::TvmTransitionOperation;
use crate::runtime::native_image::managed::{
    ManagedAggregateDescriptor, ManagedFieldType, ManagedFieldValue,
};

use super::*;

/// Deterministic backend for a complete typed Send-to-Receive lifecycle.
#[derive(Debug)]
struct TypedMailboxBackend;

impl NativeImageBackend for TypedMailboxBackend {
    /// Starts by sending one backend-owned String word to the calling actor.
    fn call_frame(
        &mut self,
        context: &mut PureNativeExecutionContext<'_>,
        request_id: u64,
        _export_id: u64,
        _args: &[ReplValue],
    ) -> Result<TvmControlFrame, String> {
        let owner_id = context.owner_id();
        let [tag, low, high] = TvmBoundaryType::String.transition_words();
        Ok(TvmControlFrame::Transition {
            request_id,
            owner_id,
            continuation_id: 901,
            operation: TvmTransitionOperation::Send,
            arguments: vec![owner_id as i64, tag, low, high, 37],
            values: Vec::new(),
        })
    }

    /// Requests typed receive after Send, then returns the injected native word.
    fn resume_frame(
        &mut self,
        context: &mut PureNativeExecutionContext<'_>,
        request_id: u64,
        continuation_id: u64,
        values: Vec<i64>,
    ) -> Result<TvmControlFrame, String> {
        let owner_id = context.owner_id();
        match continuation_id {
            901 if values.is_empty() => Ok(TvmControlFrame::Transition {
                request_id,
                owner_id,
                continuation_id: 907,
                operation: TvmTransitionOperation::Receive,
                arguments: TvmBoundaryType::String.transition_words().to_vec(),
                values: Vec::new(),
            }),
            907 if values == [41] => Ok(TvmControlFrame::Success {
                request_id,
                owner_id,
                value: 41,
            }),
            _ => Err("unexpected typed mailbox continuation".to_string()),
        }
    }

    /// Decodes the completed receiver-owned String word.
    fn decode_result(
        &self,
        _context: &PureNativeExecutionContext<'_>,
        result_type: &TvmBoundaryType,
        value: i64,
        projection: NativeResultProjection,
    ) -> Result<NativeDecodedResult, String> {
        if projection != NativeResultProjection::PublicValue {
            return Err("unexpected typed mailbox result projection".to_string());
        }
        match (result_type, value) {
            (TvmBoundaryType::String, 41) => Ok(NativeDecodedResult::Value(ReplValue::String(
                "mailbox".to_string(),
            ))),
            _ => Err("unexpected typed mailbox result".to_string()),
        }
    }

    /// Materializes the sender-owned word before mailbox publication.
    fn decode_transition_value(
        &self,
        _context: &PureNativeExecutionContext<'_>,
        boundary_type: &TvmBoundaryType,
        value: i64,
    ) -> Result<ReplValue, String> {
        match (boundary_type, value) {
            (TvmBoundaryType::String, 37) => Ok(ReplValue::String("mailbox".to_string())),
            _ => Err("unexpected typed send value".to_string()),
        }
    }

    /// Allocates the selected mailbox value into receiver-owned storage.
    fn encode_transition_value(
        &mut self,
        _context: &mut PureNativeExecutionContext<'_>,
        boundary_type: &TvmBoundaryType,
        value: &ReplValue,
    ) -> Result<i64, String> {
        match (boundary_type, value) {
            (TvmBoundaryType::String, ReplValue::String(value)) if value == "mailbox" => Ok(41),
            _ => Err("unexpected typed receive value".to_string()),
        }
    }

    /// Releases no external state.
    fn shutdown(&mut self) -> Result<(), String> {
        Ok(())
    }

    /// Produces an independent deterministic backend.
    fn fork_box(&self) -> Result<Box<dyn NativeImageBackend>, String> {
        Ok(Box::new(Self))
    }
}

/// Builds the canonical typed mailbox boundary used by suspension tests.
fn typed_mailbox_boundary() -> PureNativeBoundary {
    PureNativeBoundary {
        artifact: Some(ResolvedPureArtifact {
            image_identity: "transport-image".to_string(),
            descriptor_digest: [3; 32],
            exports: vec![PureNativeExportSpec {
                id: 887,
                module: "typed.Mailbox".to_string(),
                function: "round_trip".to_string(),
                arity: 0,
                parameters: Vec::new(),
                result: TvmBoundaryType::String,
            }],
            continuations: vec![
                TvmContinuationDescriptor {
                    id: 901,
                    parameters: Vec::new(),
                    results: vec![TvmBoundaryType::String],
                },
                TvmContinuationDescriptor {
                    id: 907,
                    parameters: vec![TvmBoundaryType::String],
                    results: vec![TvmBoundaryType::String],
                },
            ],
        }),
        backend: Some(Box::new(TypedMailboxBackend)),
        call_cache: None,
    }
}

#[test]
fn typed_mailbox_full_cycle_sends_receives_and_returns_receiver_owned_value() {
    let mut actors = crate::runtime::vm::actor::VmActorRuntime::default();
    let owner = actors.spawn_root(crate::runtime::vm::process::VmProcessSource::new(
        "typed.Mailbox",
        "round_trip",
        0,
    ));
    let mut boundary = typed_mailbox_boundary();
    let mut execution = PureNativeExecutionRuntime::runtime_default().expect("execution runtime");
    let mut context = PureNativeExecutionContext::new(owner, &mut execution);

    assert_eq!(
        boundary
            .call_for_actor(&mut actors, &mut context, "round_trip", &[])
            .expect("typed mailbox lifecycle"),
        ReplValue::String("mailbox".to_string())
    );
    assert_eq!(
        actors
            .processes()
            .get(owner)
            .expect("typed mailbox owner")
            .mailbox_len(),
        0
    );
    assert_eq!(actors.pending_native_continuation_count(), 0);
}

/// Moves each parked continuation across an OS thread before exact resume.
#[test]
fn parked_native_continuation_resumes_after_thread_transfer() {
    let mut actors = crate::runtime::vm::actor::VmActorRuntime::default();
    let owner = actors.spawn_root(crate::runtime::vm::process::VmProcessSource::new(
        "typed.Mailbox",
        "round_trip",
        0,
    ));
    let mut boundary = typed_mailbox_boundary();
    let mut execution = PureNativeExecutionRuntime::runtime_default().expect("execution runtime");
    let mut context = PureNativeExecutionContext::new(owner, &mut execution);

    let PureNativeExecution::Suspended(first) = boundary
        .begin_call_for_actor(&mut actors, &mut context, "round_trip", &[])
        .expect("begin typed mailbox call")
    else {
        panic!("typed mailbox call must suspend for send");
    };
    let first = std::thread::spawn(move || first)
        .join()
        .expect("transfer send continuation");
    let PureNativeExecution::Suspended(second) = boundary
        .resume_transition_for_actor(&mut actors, &mut context, first)
        .expect("resume typed send")
    else {
        panic!("typed send must suspend for receive");
    };
    let second = std::thread::spawn(move || second)
        .join()
        .expect("transfer receive continuation");
    let PureNativeExecution::Complete(value) = boundary
        .resume_transition_for_actor(&mut actors, &mut context, second)
        .expect("resume typed receive")
    else {
        panic!("typed receive must complete");
    };

    assert_eq!(value, ReplValue::String("mailbox".to_string()));
    assert_eq!(actors.pending_native_continuation_count(), 0);
}

/// Rejects a continuation resume through another actor's execution context.
#[test]
fn execution_context_rejects_foreign_actor_before_transition_service() {
    let mut actors = crate::runtime::vm::actor::VmActorRuntime::default();
    let owner = actors.spawn_root(crate::runtime::vm::process::VmProcessSource::new(
        "typed.Mailbox",
        "round_trip",
        0,
    ));
    let foreign = actors.spawn_root(crate::runtime::vm::process::VmProcessSource::new(
        "typed.Mailbox",
        "foreign",
        0,
    ));
    let mut boundary = typed_mailbox_boundary();
    let mut execution = PureNativeExecutionRuntime::runtime_default().expect("execution runtime");
    let suspension = {
        let mut owner_context = PureNativeExecutionContext::new(owner, &mut execution);
        let PureNativeExecution::Suspended(suspension) = boundary
            .begin_call_for_actor(&mut actors, &mut owner_context, "round_trip", &[])
            .expect("begin owner call")
        else {
            panic!("typed mailbox call must suspend");
        };
        suspension
    };
    let mut foreign_context = PureNativeExecutionContext::new(foreign, &mut execution);

    let error = boundary
        .resume_transition_for_actor(&mut actors, &mut foreign_context, suspension)
        .expect_err("foreign context must not resume owner continuation");

    assert!(error.contains("cannot resume owner"));
    assert_eq!(actors.pending_native_continuation_count(), 1);
    assert_eq!(
        actors.processes().get(owner).expect("owner").mailbox_len(),
        0
    );
}

/// Direct managed backend used to prove mailbox transfer never materializes a public value.
#[derive(Debug)]
struct ManagedMailboxBackend {
    descriptor: Arc<ManagedAggregateDescriptor>,
    recipient: Option<u64>,
}

impl ManagedMailboxBackend {
    /// Creates one backend around a canonical Pair descriptor.
    fn pair() -> Self {
        Self {
            descriptor: Arc::new(
                ManagedAggregateDescriptor::tuple(
                    "typed.Mailbox.Pair",
                    vec![ManagedFieldType::Int, ManagedFieldType::Bool],
                )
                .expect("Pair descriptor"),
            ),
            recipient: None,
        }
    }

    /// Routes the generated send to one distinct receiver actor.
    fn with_recipient(mut self, recipient: u64) -> Self {
        self.recipient = Some(recipient);
        self
    }

    /// Returns the exact native boundary identity for the Pair graph.
    fn boundary_type(&self) -> TvmBoundaryType {
        TvmBoundaryType::Managed(self.descriptor.managed().semantic_id().bytes())
    }
}

impl NativeImageBackend for ManagedMailboxBackend {
    /// Allocates one sender-owned Pair and yields a typed self-send.
    fn call_frame(
        &mut self,
        context: &mut PureNativeExecutionContext<'_>,
        request_id: u64,
        _export_id: u64,
        _args: &[ReplValue],
    ) -> Result<TvmControlFrame, String> {
        let owner_id = context.owner_id();
        let descriptor = Arc::clone(&self.descriptor);
        let pair = context
            .managed()
            .with_public_allocation(owner_id, |heap, _| {
                heap.allocate_aggregate(
                    descriptor,
                    &[ManagedFieldValue::Int(53), ManagedFieldValue::Bool(true)],
                )
                .map_err(|error| error.to_string())
            })?;
        let [tag, low, high] = self.boundary_type().transition_words();
        Ok(TvmControlFrame::Transition {
            request_id,
            owner_id,
            continuation_id: 911,
            operation: TvmTransitionOperation::Send,
            arguments: vec![
                self.recipient.unwrap_or(owner_id) as i64,
                tag,
                low,
                high,
                i64::from_ne_bytes(pair.encoded_abi_word().to_ne_bytes()),
            ],
            values: Vec::new(),
        })
    }

    /// Requests typed receive and then returns its injected managed word.
    fn resume_frame(
        &mut self,
        context: &mut PureNativeExecutionContext<'_>,
        request_id: u64,
        continuation_id: u64,
        values: Vec<i64>,
    ) -> Result<TvmControlFrame, String> {
        let owner_id = context.owner_id();
        match continuation_id {
            911 if values.is_empty() => Ok(TvmControlFrame::Transition {
                request_id,
                owner_id,
                continuation_id: 919,
                operation: TvmTransitionOperation::Receive,
                arguments: self.boundary_type().transition_words().to_vec(),
                values: Vec::new(),
            }),
            919 if values.len() == 1 => Ok(TvmControlFrame::Success {
                request_id,
                owner_id,
                value: values[0],
            }),
            _ => Err("unexpected managed mailbox continuation".to_string()),
        }
    }

    /// Materializes only the final public export result after VM execution completes.
    fn decode_result(
        &self,
        context: &PureNativeExecutionContext<'_>,
        result_type: &TvmBoundaryType,
        value: i64,
        projection: NativeResultProjection,
    ) -> Result<NativeDecodedResult, String> {
        if projection != NativeResultProjection::PublicValue {
            return Err("unexpected managed result projection".to_string());
        }
        if result_type != &self.boundary_type() {
            return Err("unexpected managed result type".to_string());
        }
        let owner_id = context.owner_id();
        let descriptor = Arc::clone(&self.descriptor);
        context
            .managed_ref()
            .with_public_materialization(owner_id, |heap, _| {
                let reference = heap
                    .validate_abi_reference(
                        u64::from_ne_bytes(value.to_ne_bytes()),
                        descriptor.managed().semantic_id(),
                    )
                    .map_err(|error| error.to_string())?;
                let view = heap
                    .read_aggregate(reference.cast(), &descriptor)
                    .map_err(|error| error.to_string())?;
                match (view.field(0), view.field(1)) {
                    (Ok(ManagedFieldValue::Int(integer)), Ok(ManagedFieldValue::Bool(boolean))) => {
                        Ok(NativeDecodedResult::Value(ReplValue::Tuple(vec![
                            ReplValue::Int(integer),
                            ReplValue::Bool(boolean),
                        ])))
                    }
                    _ => Err("managed Pair fields did not match".to_string()),
                }
            })
    }

    /// Copies the generated word directly into receiver-owned graph storage.
    fn copy_transition_value(
        &mut self,
        context: &mut PureNativeExecutionContext<'_>,
        recipient_id: u64,
        boundary_type: &TvmBoundaryType,
        value: i64,
    ) -> Result<Option<crate::runtime::vm::process::VmManagedMailboxToken>, String> {
        let owner_id = context.owner_id();
        let fragment =
            context
                .managed()
                .copy_mailbox_value(owner_id, recipient_id, boundary_type, value)?;
        crate::runtime::vm::process::VmManagedMailboxToken::new(
            fragment.fragment_id(),
            fragment.sender().get(),
            fragment.receiver().get(),
            fragment.receiver_heap_bytes(),
        )
        .map(Some)
    }

    /// Removes a copied graph when actor mailbox admission rejects it.
    fn rollback_transition_value(
        &mut self,
        context: &mut PureNativeExecutionContext<'_>,
        fragment: crate::runtime::vm::process::VmManagedMailboxToken,
    ) -> Result<(), String> {
        context
            .managed()
            .rollback_mailbox_value(fragment.fragment_id())
    }

    /// Releases the precise root after the VM removes its mailbox token.
    fn consume_transition_value(
        &mut self,
        context: &mut PureNativeExecutionContext<'_>,
        fragment: crate::runtime::vm::process::VmManagedMailboxToken,
    ) -> Result<(), String> {
        context
            .managed()
            .consume_mailbox_value(fragment.fragment_id())
    }

    /// Injects the receiver-owned fragment root directly into native continuation code.
    fn encode_transition_message(
        &mut self,
        context: &mut PureNativeExecutionContext<'_>,
        boundary_type: &TvmBoundaryType,
        message: &crate::runtime::vm::process::VmMessage,
    ) -> Result<i64, String> {
        let fragment = message
            .managed_fragment
            .ok_or_else(|| "managed mailbox fragment is missing".to_string())?;
        let owner_id = context.owner_id();
        context
            .managed_ref()
            .mailbox_value_word(fragment.fragment_id(), owner_id, boundary_type)
    }

    /// Releases all state for this isolated test backend.
    fn shutdown(&mut self) -> Result<(), String> {
        Ok(())
    }

    /// Releases only the actor heap owned by the closing boundary.
    fn shutdown_owner(
        &mut self,
        context: &mut PureNativeExecutionContext<'_>,
    ) -> Result<(), String> {
        let owner_id = context.owner_id();
        context.managed().release_owner(owner_id);
        Ok(())
    }

    /// Forks immutable backend metadata while the shard retains mutable state.
    fn fork_box(&self) -> Result<Box<dyn NativeImageBackend>, String> {
        Ok(Box::new(Self {
            descriptor: Arc::clone(&self.descriptor),
            recipient: self.recipient,
        }))
    }
}

/// Executes a managed Send/Receive cycle without transition value materialization.
#[test]
fn managed_mailbox_full_cycle_preserves_native_graph_identity() {
    let mut actors = crate::runtime::vm::actor::VmActorRuntime::default();
    let owner = actors.spawn_root(crate::runtime::vm::process::VmProcessSource::new(
        "typed.Mailbox",
        "managed_round_trip",
        0,
    ));
    let backend = ManagedMailboxBackend::pair();
    let boundary_type = backend.boundary_type();
    let mut boundary = PureNativeBoundary {
        artifact: Some(ResolvedPureArtifact {
            image_identity: "managed-transport-image".to_string(),
            descriptor_digest: [4; 32],
            exports: vec![PureNativeExportSpec {
                id: 887,
                module: "typed.Mailbox".to_string(),
                function: "managed_round_trip".to_string(),
                arity: 0,
                parameters: Vec::new(),
                result: boundary_type.clone(),
            }],
            continuations: vec![
                TvmContinuationDescriptor {
                    id: 911,
                    parameters: Vec::new(),
                    results: vec![boundary_type.clone()],
                },
                TvmContinuationDescriptor {
                    id: 919,
                    parameters: vec![boundary_type.clone()],
                    results: vec![boundary_type],
                },
            ],
        }),
        backend: Some(Box::new(backend)),
        call_cache: None,
    };
    let mut execution = PureNativeExecutionRuntime::runtime_default().expect("execution runtime");
    let mut context = PureNativeExecutionContext::new(owner, &mut execution);

    assert_eq!(
        boundary
            .call_for_actor(&mut actors, &mut context, "managed_round_trip", &[])
            .expect("managed mailbox lifecycle"),
        ReplValue::Tuple(vec![ReplValue::Int(53), ReplValue::Bool(true)])
    );
    assert_eq!(
        actors
            .processes()
            .get(owner)
            .expect("managed mailbox owner")
            .mailbox_len(),
        0
    );
    assert_eq!(actors.pending_native_continuation_count(), 0);
}

/// Rejects an over-limit cross-owner graph without leaking receiver allocations.
#[test]
fn managed_mailbox_rejection_rolls_back_receiver_heap_and_retains_lease() {
    let limits = crate::runtime::vm::memory::VmMemoryLimits::new(1, 1).expect("tiny limits");
    let mut actors = crate::runtime::vm::actor::VmActorRuntime::with_memory_limits(limits);
    let owner = actors.spawn_root(crate::runtime::vm::process::VmProcessSource::new(
        "typed.Mailbox",
        "managed_rejected",
        0,
    ));
    let recipient = actors.spawn_root(crate::runtime::vm::process::VmProcessSource::new(
        "typed.Mailbox",
        "recipient",
        0,
    ));
    let backend = ManagedMailboxBackend::pair().with_recipient(recipient.as_u64());
    let boundary_type = backend.boundary_type();
    let mut boundary = PureNativeBoundary {
        artifact: Some(ResolvedPureArtifact {
            image_identity: "collection-transport-image".to_string(),
            descriptor_digest: [5; 32],
            exports: vec![PureNativeExportSpec {
                id: 889,
                module: "typed.Mailbox".to_string(),
                function: "managed_rejected".to_string(),
                arity: 0,
                parameters: Vec::new(),
                result: boundary_type.clone(),
            }],
            continuations: vec![TvmContinuationDescriptor {
                id: 911,
                parameters: Vec::new(),
                results: vec![boundary_type],
            }],
        }),
        backend: Some(Box::new(backend)),
        call_cache: None,
    };
    let mut execution = PureNativeExecutionRuntime::runtime_default().expect("execution runtime");
    let mut context = PureNativeExecutionContext::new(owner, &mut execution);

    let error = boundary
        .call_for_actor(&mut actors, &mut context, "managed_rejected", &[])
        .expect_err("receiver mailbox limit must reject the graph");
    drop(context);

    assert!(error.contains("mailbox memory hard limit"));
    assert_eq!(
        execution.managed_ref().heap_usage(recipient.as_u64()),
        Some((0, 0))
    );
    assert_eq!(
        actors
            .processes()
            .get(recipient)
            .expect("recipient")
            .mailbox_len(),
        0
    );
    assert_eq!(actors.pending_native_continuation_count(), 1);
}

/// Validates only scalar captures because managed roots stay in the owning shard.
#[test]
fn continuation_validation_projects_out_managed_capture_types() {
    let continuation = TvmContinuationDescriptor {
        id: 801,
        parameters: vec![
            TvmBoundaryType::Int,
            TvmBoundaryType::Managed([7; 16]),
            TvmBoundaryType::Bool,
        ],
        results: vec![TvmBoundaryType::Int],
    };

    validate_continuation(&continuation, &TvmBoundaryType::Int, &[19, 1])
        .expect("scalar projection");
    let leaked = validate_continuation(&continuation, &TvmBoundaryType::Int, &[19, 77, 1])
        .expect_err("raw managed word must not be transported");
    assert!(leaked.contains("expects 2 transported values"));
}

/// Preserves scalar validation and requires exact managed continuation results.
#[test]
fn continuation_projection_rejects_invalid_scalars_and_mismatched_results() {
    let mut continuation = TvmContinuationDescriptor {
        id: 802,
        parameters: vec![
            TvmBoundaryType::Managed([8; 16]),
            TvmBoundaryType::Float,
            TvmBoundaryType::Bool,
        ],
        results: vec![TvmBoundaryType::Int],
    };
    let invalid_float = i64::from_ne_bytes(f64::NAN.to_bits().to_ne_bytes());
    let float_error =
        validate_continuation(&continuation, &TvmBoundaryType::Int, &[invalid_float, 1])
            .expect_err("non-finite capture");
    assert!(float_error.contains("non-finite"));
    let bool_error = validate_continuation(&continuation, &TvmBoundaryType::Int, &[0, 2])
        .expect_err("invalid Bool capture");
    assert!(bool_error.contains("expected 0 or 1"));

    continuation.results = vec![TvmBoundaryType::Managed([9; 16])];
    validate_continuation(&continuation, &TvmBoundaryType::Managed([9; 16]), &[0, 1])
        .expect("matching managed result");
    let result_error =
        validate_continuation(&continuation, &TvmBoundaryType::Managed([10; 16]), &[0, 1])
            .expect_err("mismatched managed result");
    assert!(result_error.contains("does not match the declared resume signature"));
}

/// Keeps managed exports visible instead of applying the removed worker filter.
#[test]
fn export_projection_preserves_exact_managed_signatures() {
    let descriptor = TvmExecutableDescriptor {
        runtime_abi_min: 2,
        runtime_abi_max: 2,
        native_boundary_min: 1,
        native_boundary_max: 1,
        target: crate::runtime::native_image::TvmImageTarget {
            triple: "test-target".to_string(),
            architecture: "test".to_string(),
            operating_system: "test".to_string(),
            calling_convention: "terlan-native-v2".to_string(),
        },
        identity: crate::runtime::native_image::TvmImageIdentity {
            compiler: "test".to_string(),
            build: "test".to_string(),
            package: "test".to_string(),
            module: "managed".to_string(),
        },
        exports: vec![crate::runtime::native_image::TvmExportDescriptor {
            id: 91,
            name: "managed.identity/1".to_string(),
            parameters: vec![TvmBoundaryType::String],
            results: vec![TvmBoundaryType::String],
        }],
        capabilities: Vec::new(),
        resources: Vec::new(),
        dependencies: Vec::new(),
        continuations: Vec::new(),
        callables: Vec::new(),
        managed_layouts: Vec::new(),
        managed_collections: Vec::new(),
        atoms: Vec::new(),
        integrity: crate::runtime::native_image::TvmImageIntegrity {
            code_digest: [0; 32],
            immutable_data_digest: [0; 32],
        },
        signature: None,
    };

    let exports = exports_from_descriptor(&descriptor).expect("project managed export");
    assert_eq!(exports.len(), 1);
    assert_eq!(exports[0].parameters, [TvmBoundaryType::String]);
    assert_eq!(exports[0].result, TvmBoundaryType::String);
}

/// Admits only public aggregate and collection shapes to descriptor-directed parameters.
#[test]
fn managed_export_argument_precheck_defers_exact_shape_to_owning_backend() {
    let export = PureNativeExportSpec {
        id: 92,
        module: "managed".to_string(),
        function: "identity".to_string(),
        arity: 1,
        parameters: vec![TvmBoundaryType::Managed([7; 16])],
        result: TvmBoundaryType::Managed([7; 16]),
    };
    for value in [
        ReplValue::Tuple(vec![ReplValue::Int(1)]),
        ReplValue::List(vec![ReplValue::Int(1)]),
        ReplValue::Record {
            name: "Value".to_string(),
            fields: vec![("value".to_string(), ReplValue::Int(1))],
        },
        ReplValue::Map(vec![(ReplValue::Int(1), ReplValue::Bool(true))]),
        ReplValue::MapIndexed(crate::runtime::vm::map_value::VmMapValue::from_entries(
            vec![(ReplValue::Int(1), ReplValue::Bool(true))],
        )),
        ReplValue::Set(vec![ReplValue::Int(1)]),
    ] {
        validate_arguments(&export, &[value]).expect("managed public shape");
    }
    assert!(validate_arguments(&export, &[ReplValue::Int(1)])
        .expect_err("managed scalar mismatch")
        .contains("does not match its declared native ABI"));
}

/// Accepts Atom values only for descriptor-declared Atom parameters.
#[test]
fn atom_export_argument_precheck_preserves_the_typed_boundary() {
    let export = PureNativeExportSpec {
        id: 93,
        module: "atoms".to_owned(),
        function: "identity".to_owned(),
        arity: 1,
        parameters: vec![TvmBoundaryType::Atom],
        result: TvmBoundaryType::Atom,
    };
    validate_arguments(&export, &[ReplValue::Atom("ready".to_owned())])
        .expect("typed Atom argument");
    assert!(
        validate_arguments(&export, &[ReplValue::String("ready".to_owned())])
            .expect_err("String is not Atom")
            .contains("does not match its declared native ABI")
    );
}
