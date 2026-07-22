/// Parses `terlan-vm run` arguments.
fn parse_run_args(args: &[String]) -> VmCommand {
    let mut source = None;
    let mut entry = "main".to_string();
    let mut test_eval = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--help" | "-h" => return VmCommand::Help,
            "--test-eval" => {
                test_eval = true;
                index += 1;
            }
            "--entry" => {
                let Some(value) = args.get(index + 1) else {
                    return VmCommand::Error("missing value for --entry".to_string());
                };
                entry = value.clone();
                index += 2;
            }
            arg if arg.starts_with('-') => {
                return VmCommand::Error(format!("unknown terlan-vm run option: {arg}"));
            }
            path => {
                if source.is_some() {
                    return VmCommand::Error(
                        "terlan-vm run accepts exactly one source file".to_string(),
                    );
                }
                source = Some(PathBuf::from(path));
                index += 1;
            }
        }
    }

    let Some(source) = source else {
        return VmCommand::Error("terlan-vm run requires a source file".to_string());
    };
    VmCommand::Run {
        source,
        entry,
        test_eval,
    }
}

/// Builds the public persistent actor export manifest printed by the CLI.
fn render_persistent_actor_export_manifest(
    args: &VmPersistentActorExportCommandArgs,
) -> Result<String, String> {
    let export = build_persistent_actor_export(
        &args.actor_id,
        &args.schema_id,
        args.schema_version,
        &args.source_adapter_kind,
        args.snapshot_generation,
        args.last_event_sequence,
        args.retained_event_count,
        &args.redacted_fields,
        &args.resource_handles,
        false,
    )?;
    let envelope = build_cross_machine_actor_export(&export, &args.source_machine_id)
        .map_err(|error| format!("error[vm_persistent_actor_export]: {error:?}"))?;
    Ok(envelope.render_manifest())
}

/// Builds the public persistent actor restore validation report printed by the CLI.
fn render_persistent_actor_restore_plan(
    args: &VmPersistentActorRestoreCommandArgs,
) -> Result<String, String> {
    let export = build_persistent_actor_export(
        &args.actor_id,
        &args.schema_id,
        args.schema_version,
        &args.source_adapter_kind,
        args.snapshot_generation,
        args.last_event_sequence,
        args.retained_event_count,
        &args.redacted_fields,
        &args.resource_handles,
        args.compacted,
    )?;
    let actor_id = VmPersistentActorId::new(args.actor_id.clone())?;
    let schema = VmPersistentActorSchema::new(args.schema_id.clone(), args.schema_version)?;
    let mut target = VmPersistentActorRestoreTarget::new(
        actor_id,
        schema,
        args.available_resource_handles.clone(),
        VmPersistentActorRestoreCapabilities::full(),
    )
    .with_adapter_kind(args.target_adapter_kind.clone());
    if args.allow_cross_adapter_restore {
        target = target.allow_cross_adapter_restore();
    }
    let plan = plan_persistent_actor_restore(&export, &target)
        .map_err(|error| format!("error[vm_persistent_actor_restore]: {error:?}"))?;
    let replay_fixture = generate_minimal_actor_replay_fixture(&export, &target)
        .map_err(|error| format!("error[vm_persistent_actor_restore]: {error:?}"))?;
    Ok(format!(
        "restore_plan=accepted;actor={};schema={}:{};source_adapter={};target_adapter={};snapshot_generation={};events={};resources={};redactions={};compacted={};replay_fixture={}",
        args.actor_id,
        args.schema_id,
        args.schema_version,
        args.source_adapter_kind,
        args.target_adapter_kind,
        plan.snapshot_generation,
        plan.restored_event_sequences
            .iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join(","),
        plan.restored_resource_handles.join(","),
        plan.redacted_fields.join(","),
        args.compacted,
        replay_fixture.render_manifest(),
    ))
}

fn build_persistent_actor_export(
    actor_id: &str,
    schema_id: &str,
    schema_version: u64,
    source_adapter_kind: &str,
    snapshot_generation: u64,
    last_event_sequence: u64,
    retained_event_count: usize,
    redacted_fields: &[String],
    resource_handles: &[String],
    compacted: bool,
) -> Result<VmPersistentActorExport, String> {
    let actor_id = VmPersistentActorId::new(actor_id.to_string())?;
    let schema = VmPersistentActorSchema::new(schema_id.to_string(), schema_version)?;
    let snapshot = VmPersistentActorSnapshot::new(
        actor_id.clone(),
        schema.clone(),
        snapshot_generation,
        ReplValue::Atom("redacted_state".to_string()),
        Vec::new(),
        Vec::new(),
        resource_handles.to_vec(),
        last_event_sequence,
    )?;
    let mut events = Vec::with_capacity(retained_event_count);
    for offset in 0..retained_event_count {
        let sequence = last_event_sequence
            .checked_add(offset as u64)
            .and_then(|value| value.checked_add(1))
            .ok_or_else(|| {
                "error[vm_persistent_actor_export]: retained event sequence overflow".to_string()
            })?;
        events.push(VmPersistentActorEvent::new(
            actor_id.clone(),
            schema.clone(),
            sequence,
            ReplValue::Atom("redacted_event".to_string()),
        )?);
    }
    Ok(
        VmPersistentActorExport::new(snapshot, events, redacted_fields.to_vec(), compacted)
            .map_err(|error| format!("error[vm_persistent_actor_export]: {error:?}"))?
            .with_source_adapter_kind(source_adapter_kind.to_string()),
    )
}

/// Parses `terlan-vm load` arguments.
fn parse_load_args(args: &[String]) -> VmCommand {
    match args {
        [flag] if matches!(flag.as_str(), "--help" | "-h") => VmCommand::Help,
        [path] => VmCommand::Load {
            artifact: PathBuf::from(path),
        },
        [] => VmCommand::Error("terlan-vm load requires an artifact file".to_string()),
        _ => VmCommand::Error("terlan-vm load accepts exactly one artifact file".to_string()),
    }
}

/// Parses `terlan-vm package-image-metadata` arguments.
fn parse_package_image_metadata_args(args: &[String]) -> VmCommand {
    let mut image = None;
    let mut package_path = "runtime/release-self-test.tvm".to_string();
    let mut entry = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--help" | "-h" => return VmCommand::Help,
            "--package-path" => {
                let Some(value) = args.get(index + 1) else {
                    return VmCommand::Error("missing value for --package-path".to_string());
                };
                package_path = value.clone();
                index += 2;
            }
            "--entry" => {
                let Some(value) = args.get(index + 1) else {
                    return VmCommand::Error("missing value for --entry".to_string());
                };
                entry = Some(value.clone());
                index += 2;
            }
            value if value.starts_with('-') => {
                return VmCommand::Error(format!(
                    "unknown terlan-vm package-image-metadata option: {value}"
                ));
            }
            value => {
                if image.is_some() {
                    return VmCommand::Error(
                        "terlan-vm package-image-metadata accepts exactly one image".to_string(),
                    );
                }
                image = Some(PathBuf::from(value));
                index += 1;
            }
        }
    }
    let Some(image) = image else {
        return VmCommand::Error(
            "terlan-vm package-image-metadata requires a .tvm image".to_string(),
        );
    };
    let Some(entry) = entry else {
        return VmCommand::Error("terlan-vm package-image-metadata requires --entry".to_string());
    };
    VmCommand::PackageImageMetadata {
        image,
        package_path,
        entry,
    }
}

/// Parses `terlan-vm validate-package` arguments.
fn parse_validate_package_args(args: &[String]) -> VmCommand {
    match args {
        [flag] if matches!(flag.as_str(), "--help" | "-h") => VmCommand::Help,
        [root] => VmCommand::ValidatePackage {
            root: PathBuf::from(root),
        },
        [] => VmCommand::Error("terlan-vm validate-package requires a package root".to_string()),
        _ => VmCommand::Error(
            "terlan-vm validate-package accepts exactly one package root".to_string(),
        ),
    }
}

/// Parses `terlan-vm support-bundle` arguments.
fn parse_support_bundle_args(args: &[String]) -> VmCommand {
    match args {
        [flag] if matches!(flag.as_str(), "--help" | "-h") => VmCommand::Help,
        [image] => VmCommand::SupportBundle {
            image: PathBuf::from(image),
        },
        [] => VmCommand::Error("terlan-vm support-bundle requires a .tvm image".to_string()),
        _ => {
            VmCommand::Error("terlan-vm support-bundle accepts exactly one .tvm image".to_string())
        }
    }
}

/// Parses `terlan-vm benchmark-http-handler` arguments.
fn parse_benchmark_http_handler_args(args: &[String]) -> VmCommand {
    let mut iterations = 10_000usize;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--help" | "-h" => return VmCommand::Help,
            "--iterations" => {
                let Some(value) = args.get(index + 1) else {
                    return VmCommand::Error("missing value for --iterations".to_string());
                };
                let Ok(parsed) = value.parse::<usize>() else {
                    return VmCommand::Error(format!(
                        "terlan-vm benchmark-http-handler --iterations expects a positive integer, got `{value}`"
                    ));
                };
                if parsed == 0 {
                    return VmCommand::Error(
                        "terlan-vm benchmark-http-handler --iterations must be greater than 0"
                            .to_string(),
                    );
                }
                iterations = parsed;
                index += 2;
            }
            arg if arg.starts_with('-') => {
                return VmCommand::Error(format!(
                    "unknown terlan-vm benchmark-http-handler option: {arg}"
                ));
            }
            arg => {
                return VmCommand::Error(format!(
                    "terlan-vm benchmark-http-handler does not accept positional argument `{arg}`"
                ));
            }
        }
    }
    VmCommand::BenchmarkHttpHandler { iterations }
}

/// Parses `terlan-vm benchmark-http-stack` arguments.
fn parse_benchmark_http_stack_args(args: &[String]) -> VmCommand {
    let mut iterations = 10_000usize;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--help" | "-h" => return VmCommand::Help,
            "--iterations" => {
                let Some(value) = args.get(index + 1) else {
                    return VmCommand::Error("missing value for --iterations".to_string());
                };
                let Ok(parsed) = value.parse::<usize>() else {
                    return VmCommand::Error(format!(
                        "terlan-vm benchmark-http-stack --iterations expects a positive integer, got `{value}`"
                    ));
                };
                if parsed == 0 {
                    return VmCommand::Error(
                        "terlan-vm benchmark-http-stack --iterations must be greater than 0"
                            .to_string(),
                    );
                }
                iterations = parsed;
                index += 2;
            }
            arg if arg.starts_with('-') => {
                return VmCommand::Error(format!(
                    "unknown terlan-vm benchmark-http-stack option: {arg}"
                ));
            }
            arg => {
                return VmCommand::Error(format!(
                    "terlan-vm benchmark-http-stack does not accept positional argument `{arg}`"
                ));
            }
        }
    }
    VmCommand::BenchmarkHttpStack { iterations }
}

/// Parses `terlan-vm benchmark-in-memory-framing` arguments.
fn parse_benchmark_in_memory_framing_args(args: &[String]) -> VmCommand {
    let mut iterations = 10_000usize;
    let mut payload_bytes = 128usize;
    let mut workload = BenchmarkFramingWorkload::Roundtrip;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--help" | "-h" => return VmCommand::Help,
            "--iterations" => {
                let Some(value) = args.get(index + 1) else {
                    return VmCommand::Error("missing value for --iterations".to_string());
                };
                let Ok(parsed) = value.parse::<usize>() else {
                    return VmCommand::Error(format!(
                        "terlan-vm benchmark-in-memory-framing --iterations expects a positive integer, got `{value}`"
                    ));
                };
                if parsed == 0 {
                    return VmCommand::Error(
                        "terlan-vm benchmark-in-memory-framing --iterations must be greater than 0"
                            .to_string(),
                    );
                }
                iterations = parsed;
                index += 2;
            }
            "--payload-bytes" => {
                let Some(value) = args.get(index + 1) else {
                    return VmCommand::Error("missing value for --payload-bytes".to_string());
                };
                let Ok(parsed) = value.parse::<usize>() else {
                    return VmCommand::Error(format!(
                        "terlan-vm benchmark-in-memory-framing --payload-bytes expects a non-negative integer, got `{value}`"
                    ));
                };
                if parsed > 1024 * 1024 {
                    return VmCommand::Error(
                        "terlan-vm benchmark-in-memory-framing --payload-bytes must not exceed 1048576"
                            .to_string(),
                    );
                }
                payload_bytes = parsed;
                index += 2;
            }
            "--workload" => {
                let Some(value) = args.get(index + 1) else {
                    return VmCommand::Error("missing value for --workload".to_string());
                };
                let parsed = match BenchmarkFramingWorkload::parse(value) {
                    Ok(parsed) => parsed,
                    Err(error) => return VmCommand::Error(error),
                };
                workload = parsed;
                index += 2;
            }
            arg if arg.starts_with('-') => {
                return VmCommand::Error(format!(
                    "unknown terlan-vm benchmark-in-memory-framing option: {arg}"
                ));
            }
            arg => {
                return VmCommand::Error(format!(
                    "terlan-vm benchmark-in-memory-framing does not accept positional argument `{arg}`"
                ));
            }
        }
    }
    VmCommand::BenchmarkInMemoryFraming {
        iterations,
        payload_bytes,
        workload,
    }
}

/// Parses `terlan-vm benchmark-http-vm-stream` arguments.
fn parse_benchmark_http_vm_stream_args(args: &[String]) -> VmCommand {
    let mut iterations = 100usize;
    let mut payload_bytes = 7usize;
    let mut requests_per_connection = 1usize;
    let mut request_mix = BenchmarkHttpRequestMix::Single;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--help" | "-h" => return VmCommand::Help,
            "--iterations" => {
                let Some(value) = args.get(index + 1) else {
                    return VmCommand::Error("missing value for --iterations".to_string());
                };
                let Ok(parsed) = value.parse::<usize>() else {
                    return VmCommand::Error(format!(
                        "terlan-vm benchmark-http-vm-stream --iterations expects a positive integer, got `{value}`"
                    ));
                };
                if parsed == 0 {
                    return VmCommand::Error(
                        "terlan-vm benchmark-http-vm-stream --iterations must be greater than 0"
                            .to_string(),
                    );
                }
                iterations = parsed;
                index += 2;
            }
            "--payload-bytes" => {
                let Some(value) = args.get(index + 1) else {
                    return VmCommand::Error("missing value for --payload-bytes".to_string());
                };
                let Ok(parsed) = value.parse::<usize>() else {
                    return VmCommand::Error(format!(
                        "terlan-vm benchmark-http-vm-stream --payload-bytes expects a non-negative integer, got `{value}`"
                    ));
                };
                if parsed > 1024 * 1024 {
                    return VmCommand::Error(
                        "terlan-vm benchmark-http-vm-stream --payload-bytes must not exceed 1048576"
                            .to_string(),
                    );
                }
                payload_bytes = parsed;
                index += 2;
            }
            "--requests-per-connection" => {
                let Some(value) = args.get(index + 1) else {
                    return VmCommand::Error(
                        "missing value for --requests-per-connection".to_string(),
                    );
                };
                let Ok(parsed) = value.parse::<usize>() else {
                    return VmCommand::Error(format!(
                        "terlan-vm benchmark-http-vm-stream --requests-per-connection expects a positive integer, got `{value}`"
                    ));
                };
                if parsed == 0 {
                    return VmCommand::Error(
                        "terlan-vm benchmark-http-vm-stream --requests-per-connection must be greater than 0"
                            .to_string(),
                    );
                }
                requests_per_connection = parsed;
                index += 2;
            }
            "--request-mix" => {
                let Some(value) = args.get(index + 1) else {
                    return VmCommand::Error("missing value for --request-mix".to_string());
                };
                request_mix = match BenchmarkHttpRequestMix::parse(value) {
                    Ok(parsed) => parsed,
                    Err(message) => return VmCommand::Error(message),
                };
                index += 2;
            }
            arg if arg.starts_with('-') => {
                return VmCommand::Error(format!(
                    "unknown terlan-vm benchmark-http-vm-stream option: {arg}"
                ));
            }
            arg => {
                return VmCommand::Error(format!(
                    "terlan-vm benchmark-http-vm-stream does not accept positional argument `{arg}`"
                ));
            }
        }
    }
    VmCommand::BenchmarkHttpVmStream {
        iterations,
        payload_bytes,
        requests_per_connection,
        request_mix,
    }
}

/// Parses `terlan-vm benchmark-http-socket` arguments.
fn parse_benchmark_http_socket_args(args: &[String]) -> VmCommand {
    let mut iterations = 100usize;
    let mut concurrency = 1usize;
    let mut queue_capacity = None;
    let mut warmup_requests = 0usize;
    let mut handler_delay_ms = 0u64;
    let mut requests_per_connection = 1usize;
    let mut payload_bytes = 7usize;
    let mut request_mix = BenchmarkHttpRequestMix::Single;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--help" | "-h" => return VmCommand::Help,
            "--iterations" => {
                let Some(value) = args.get(index + 1) else {
                    return VmCommand::Error("missing value for --iterations".to_string());
                };
                let Ok(parsed) = value.parse::<usize>() else {
                    return VmCommand::Error(format!(
                        "terlan-vm benchmark-http-socket --iterations expects a positive integer, got `{value}`"
                    ));
                };
                if parsed == 0 {
                    return VmCommand::Error(
                        "terlan-vm benchmark-http-socket --iterations must be greater than 0"
                            .to_string(),
                    );
                }
                iterations = parsed;
                index += 2;
            }
            "--concurrency" => {
                let Some(value) = args.get(index + 1) else {
                    return VmCommand::Error("missing value for --concurrency".to_string());
                };
                let Ok(parsed) = value.parse::<usize>() else {
                    return VmCommand::Error(format!(
                        "terlan-vm benchmark-http-socket --concurrency expects a positive integer, got `{value}`"
                    ));
                };
                if parsed == 0 {
                    return VmCommand::Error(
                        "terlan-vm benchmark-http-socket --concurrency must be greater than 0"
                            .to_string(),
                    );
                }
                concurrency = parsed;
                index += 2;
            }
            "--queue-capacity" => {
                let Some(value) = args.get(index + 1) else {
                    return VmCommand::Error("missing value for --queue-capacity".to_string());
                };
                let Ok(parsed) = value.parse::<usize>() else {
                    return VmCommand::Error(format!(
                        "terlan-vm benchmark-http-socket --queue-capacity expects a positive integer, got `{value}`"
                    ));
                };
                if parsed == 0 {
                    return VmCommand::Error(
                        "terlan-vm benchmark-http-socket --queue-capacity must be greater than 0"
                            .to_string(),
                    );
                }
                queue_capacity = Some(parsed);
                index += 2;
            }
            "--handler-delay-ms" => {
                let Some(value) = args.get(index + 1) else {
                    return VmCommand::Error("missing value for --handler-delay-ms".to_string());
                };
                let Ok(parsed) = value.parse::<u64>() else {
                    return VmCommand::Error(format!(
                        "terlan-vm benchmark-http-socket --handler-delay-ms expects a non-negative integer, got `{value}`"
                    ));
                };
                handler_delay_ms = parsed;
                index += 2;
            }
            "--warmup-requests" => {
                let Some(value) = args.get(index + 1) else {
                    return VmCommand::Error("missing value for --warmup-requests".to_string());
                };
                let Ok(parsed) = value.parse::<usize>() else {
                    return VmCommand::Error(format!(
                        "terlan-vm benchmark-http-socket --warmup-requests expects a non-negative integer, got `{value}`"
                    ));
                };
                warmup_requests = parsed;
                index += 2;
            }
            "--requests-per-connection" => {
                let Some(value) = args.get(index + 1) else {
                    return VmCommand::Error(
                        "missing value for --requests-per-connection".to_string(),
                    );
                };
                let Ok(parsed) = value.parse::<usize>() else {
                    return VmCommand::Error(format!(
                        "terlan-vm benchmark-http-socket --requests-per-connection expects a positive integer, got `{value}`"
                    ));
                };
                if parsed == 0 {
                    return VmCommand::Error(
                        "terlan-vm benchmark-http-socket --requests-per-connection must be greater than 0"
                            .to_string(),
                    );
                }
                requests_per_connection = parsed;
                index += 2;
            }
            "--payload-bytes" => {
                let Some(value) = args.get(index + 1) else {
                    return VmCommand::Error("missing value for --payload-bytes".to_string());
                };
                let Ok(parsed) = value.parse::<usize>() else {
                    return VmCommand::Error(format!(
                        "terlan-vm benchmark-http-socket --payload-bytes expects a non-negative integer, got `{value}`"
                    ));
                };
                if parsed > 1024 * 1024 {
                    return VmCommand::Error(
                        "terlan-vm benchmark-http-socket --payload-bytes must not exceed 1048576"
                            .to_string(),
                    );
                }
                payload_bytes = parsed;
                index += 2;
            }
            "--request-mix" => {
                let Some(value) = args.get(index + 1) else {
                    return VmCommand::Error("missing value for --request-mix".to_string());
                };
                request_mix = match BenchmarkHttpRequestMix::parse(value) {
                    Ok(parsed) => parsed,
                    Err(message) => return VmCommand::Error(message),
                };
                index += 2;
            }
            arg if arg.starts_with('-') => {
                return VmCommand::Error(format!(
                    "unknown terlan-vm benchmark-http-socket option: {arg}"
                ));
            }
            arg => {
                return VmCommand::Error(format!(
                    "terlan-vm benchmark-http-socket does not accept positional argument `{arg}`"
                ));
            }
        }
    }
    VmCommand::BenchmarkHttpSocket {
        iterations,
        concurrency,
        queue_capacity: queue_capacity.unwrap_or(concurrency),
        warmup_requests,
        handler_delay_ms,
        requests_per_connection,
        payload_bytes,
        request_mix,
    }
}

/// Parses `terlan-vm inspect` arguments.
fn parse_inspect_args(args: &[String]) -> VmCommand {
    match args {
        [flag] if matches!(flag.as_str(), "--help" | "-h") => VmCommand::Help,
        [subject] if subject == "processes" => VmCommand::Inspect {
            subject: VmInspectSubject::Processes,
        },
        [subject] if subject == "supervisors" => VmCommand::Inspect {
            subject: VmInspectSubject::Supervisors,
        },
        [subject] if subject == "resources" => VmCommand::Inspect {
            subject: VmInspectSubject::Resources,
        },
        [subject, pid] if subject == "process" => VmCommand::Inspect {
            subject: VmInspectSubject::Process { pid: pid.clone() },
        },
        [] => VmCommand::Error("terlan-vm inspect requires a subject".to_string()),
        _ => VmCommand::Error(
            "terlan-vm inspect accepts processes, supervisors, resources, or process <pid>"
                .to_string(),
        ),
    }
}

/// Compiles, loads, and executes one Terlan source file in the standalone VM.
fn run_path(
    source: &Path,
    entry: &str,
    test_eval: bool,
    output: &mut dyn FnMut(&str),
) -> Result<(), String> {
    if is_tvm_image_path(source) {
        run_tvm_image(source, entry, test_eval)
    } else if is_vm_artifact_path(source) {
        Err(tvm_json_runtime_removed_error(source))
    } else {
        run_source_file(source, entry, test_eval, output)
    }
}

/// Returns the stable rejection for the deleted public serialized-artifact lane.
fn tvm_json_runtime_removed_error(path: &Path) -> String {
    format!(
        "error[tvm_json_runtime_removed]: serialized VM artifact `{}` is not executable; build and run its native `.tvm` image",
        path.display()
    )
}

/// Returns whether a path is a Terlan VM JSON artifact.
fn is_vm_artifact_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(".tvm.json"))
}
