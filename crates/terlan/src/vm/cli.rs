use super::*;

/// Parsed standalone VM command.
pub(super) enum VmCommand {
    Help,
    Version,
    Inspect {
        subject: VmInspectSubject,
    },
    Run {
        source: PathBuf,
        entry: String,
        result_mode: RunResultMode,
        program_arguments: Vec<String>,
    },
    Load {
        artifact: PathBuf,
    },
    PackageImageMetadata {
        image: PathBuf,
        package_path: String,
        entry: String,
    },
    ValidatePackage {
        root: PathBuf,
    },
    SupportBundle {
        image: PathBuf,
    },
    ExportPersistentActor {
        args: VmPersistentActorExportCommandArgs,
    },
    RestorePersistentActor {
        args: VmPersistentActorRestoreCommandArgs,
    },
    BenchmarkHttpHandler {
        iterations: usize,
    },
    BenchmarkHttpStack {
        iterations: usize,
    },
    BenchmarkInMemoryFraming {
        iterations: usize,
        payload_bytes: usize,
        workload: BenchmarkFramingWorkload,
    },
    BenchmarkHttpVmStream {
        iterations: usize,
        payload_bytes: usize,
        requests_per_connection: usize,
        request_mix: BenchmarkHttpRequestMix,
    },
    BenchmarkHttpSocket {
        iterations: usize,
        concurrency: usize,
        queue_capacity: usize,
        warmup_requests: usize,
        handler_delay_ms: u64,
        requests_per_connection: usize,
        payload_bytes: usize,
        request_mix: BenchmarkHttpRequestMix,
    },
    Error(String),
}

/// Public CLI arguments for deterministic persistent actor export manifests.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct VmPersistentActorExportCommandArgs {
    pub(super) actor_id: String,
    pub(super) schema_id: String,
    pub(super) schema_version: u64,
    pub(super) source_machine_id: String,
    pub(super) source_adapter_kind: String,
    pub(super) snapshot_generation: u64,
    pub(super) last_event_sequence: u64,
    pub(super) retained_event_count: usize,
    pub(super) redacted_fields: Vec<String>,
    pub(super) resource_handles: Vec<String>,
}

impl Default for VmPersistentActorExportCommandArgs {
    fn default() -> Self {
        Self {
            actor_id: String::new(),
            schema_id: String::new(),
            schema_version: 0,
            source_machine_id: String::new(),
            source_adapter_kind: "force_local".to_string(),
            snapshot_generation: 1,
            last_event_sequence: 0,
            retained_event_count: 0,
            redacted_fields: Vec::new(),
            resource_handles: Vec::new(),
        }
    }
}

/// Public CLI arguments for validating persistent actor restore manifests.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct VmPersistentActorRestoreCommandArgs {
    pub(super) actor_id: String,
    pub(super) schema_id: String,
    pub(super) schema_version: u64,
    pub(super) source_adapter_kind: String,
    pub(super) target_adapter_kind: String,
    pub(super) snapshot_generation: u64,
    pub(super) last_event_sequence: u64,
    pub(super) retained_event_count: usize,
    pub(super) redacted_fields: Vec<String>,
    pub(super) resource_handles: Vec<String>,
    pub(super) available_resource_handles: Vec<String>,
    pub(super) allow_cross_adapter_restore: bool,
    pub(super) compacted: bool,
}

impl Default for VmPersistentActorRestoreCommandArgs {
    fn default() -> Self {
        Self {
            actor_id: String::new(),
            schema_id: String::new(),
            schema_version: 0,
            source_adapter_kind: "force_local".to_string(),
            target_adapter_kind: "force_local".to_string(),
            snapshot_generation: 1,
            last_event_sequence: 0,
            retained_event_count: 0,
            redacted_fields: Vec::new(),
            resource_handles: Vec::new(),
            available_resource_handles: Vec::new(),
            allow_cross_adapter_restore: false,
            compacted: false,
        }
    }
}

/// Request shape selection for VM HTTP socket benchmarks.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum BenchmarkHttpRequestMix {
    Single,
    Crud,
    Add,
    LargeStatic,
    SlowClient,
    Streaming,
    SyntheticHandlers,
}

impl BenchmarkHttpRequestMix {
    /// Parses a socket benchmark request mix name.
    pub(super) fn parse(value: &str) -> Result<Self, String> {
        match value {
            "single" => Ok(Self::Single),
            "crud" => Ok(Self::Crud),
            "add" => Ok(Self::Add),
            "large-static" => Ok(Self::LargeStatic),
            "slow-client" => Ok(Self::SlowClient),
            "streaming" => Ok(Self::Streaming),
            "synthetic-handlers" => Ok(Self::SyntheticHandlers),
            _ => Err(format!(
                "terlan-vm benchmark-http-socket --request-mix expects `single`, `crud`, `add`, `large-static`, `slow-client`, `streaming`, or `synthetic-handlers`, got `{value}`"
            )),
        }
    }
}

/// Read-only VM inspection subject selected from the standalone CLI.
pub(super) enum VmInspectSubject {
    Processes,
    Supervisors,
    Resources,
    Process { pid: String },
}

/// Standalone Terlan VM executable entrypoint.
///
/// Inputs:
/// - Process arguments after `terlan-vm`.
///
/// Output:
/// - Exit code for help, version, compile/load/run success, or usage/runtime
///   failure.
///
/// Transformation:
/// - Builds a complete VM artifact that can compile one Terlan source file to
///   CoreIR, load it into the Rust VM, and execute a zero-arity entrypoint
///   without going through the `terlc` CLI.
pub(super) fn run() -> ExitCode {
    match parse_args(std::env::args().skip(1).collect()) {
        VmCommand::Help => {
            print_usage();
            ExitCode::SUCCESS
        }
        VmCommand::Version => {
            println!("terlan-vm {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        VmCommand::Inspect { subject } => match inspect_local_vm(subject) {
            Ok(text) => {
                print!("{text}");
                ExitCode::SUCCESS
            }
            Err(message) => {
                eprintln!("{message}");
                ExitCode::from(1)
            }
        },
        VmCommand::Run {
            source,
            entry,
            result_mode,
            program_arguments,
        } => {
            let mut output = |line: &str| println!("{line}");
            match run_path(
                &source,
                &entry,
                result_mode,
                &program_arguments,
                &mut output,
            ) {
                Ok(()) => ExitCode::SUCCESS,
                Err(message) => {
                    eprintln!("{message}");
                    ExitCode::from(1)
                }
            }
        }
        VmCommand::Load { artifact } => {
            if is_vm_artifact_path(&artifact) {
                eprintln!("{}", tvm_json_runtime_removed_error(&artifact));
                ExitCode::from(1)
            } else {
                match PureNativeExecutionShard::load_image(&artifact) {
                    Ok(_shard) => {
                        println!("loaded native TVM image {}", artifact.display());
                        ExitCode::SUCCESS
                    }
                    Err(message) => {
                        eprintln!("{message}");
                        ExitCode::from(1)
                    }
                }
            }
        }
        VmCommand::PackageImageMetadata {
            image,
            package_path,
            entry,
        } => match describe_packaged_tvm_image(&image, &package_path, &entry).and_then(|metadata| {
            serde_json::to_string_pretty(&metadata).map_err(|error| {
                terlan_runtime_abi::BoundaryError::sourced(
                    terlan_runtime_abi::ErrorDomain::CommandExecution,
                    "tvm.package.metadata_json",
                    "render package image metadata",
                    format!("error[tvm.package.metadata_json]: {error}"),
                    error,
                )
            })
        }) {
            Ok(metadata) => {
                println!("{metadata}");
                ExitCode::SUCCESS
            }
            Err(message) => {
                eprintln!("{message}");
                ExitCode::from(1)
            }
        },
        VmCommand::ValidatePackage { root } => {
            match validate_and_execute_release_package(&root).and_then(|report| {
                serde_json::to_string_pretty(&report).map_err(|error| {
                    terlan_runtime_abi::BoundaryError::sourced(
                        terlan_runtime_abi::ErrorDomain::CommandExecution,
                        "tvm.package.report_json",
                        "render package validation report",
                        format!("error[tvm.package.report_json]: {error}"),
                        error,
                    )
                })
            }) {
                Ok(report) => {
                    println!("{report}");
                    ExitCode::SUCCESS
                }
                Err(message) => {
                    eprintln!("{message}");
                    ExitCode::from(1)
                }
            }
        }
        VmCommand::SupportBundle { image } => match render_tvm_support_bundle(&image) {
            Ok(bundle) => {
                print!("{bundle}");
                ExitCode::SUCCESS
            }
            Err(message) => {
                eprintln!("{message}");
                ExitCode::from(1)
            }
        },
        VmCommand::ExportPersistentActor { args } => {
            match render_persistent_actor_export_manifest(&args) {
                Ok(manifest) => {
                    println!("{manifest}");
                    ExitCode::SUCCESS
                }
                Err(message) => {
                    eprintln!("{message}");
                    ExitCode::from(1)
                }
            }
        }
        VmCommand::RestorePersistentActor { args } => {
            match render_persistent_actor_restore_plan(&args) {
                Ok(report) => {
                    println!("{report}");
                    ExitCode::SUCCESS
                }
                Err(message) => {
                    eprintln!("{message}");
                    ExitCode::from(1)
                }
            }
        }
        VmCommand::BenchmarkHttpHandler { iterations } => {
            match benchmark_http_handler(iterations) {
                Ok(report) => {
                    println!("{report}");
                    ExitCode::SUCCESS
                }
                Err(message) => {
                    eprintln!("{message}");
                    ExitCode::from(1)
                }
            }
        }
        VmCommand::BenchmarkHttpStack { iterations } => match benchmark_http_stack(iterations) {
            Ok(report) => {
                println!("{report}");
                ExitCode::SUCCESS
            }
            Err(message) => {
                eprintln!("{message}");
                ExitCode::from(1)
            }
        },
        VmCommand::BenchmarkInMemoryFraming {
            iterations,
            payload_bytes,
            workload,
        } => match benchmark_in_memory_framing(iterations, payload_bytes, workload) {
            Ok(report) => {
                println!("{report}");
                ExitCode::SUCCESS
            }
            Err(message) => {
                eprintln!("{message}");
                ExitCode::from(1)
            }
        },
        VmCommand::BenchmarkHttpVmStream {
            iterations,
            payload_bytes,
            requests_per_connection,
            request_mix,
        } => match benchmark_http_vm_stream(
            iterations,
            payload_bytes,
            requests_per_connection,
            request_mix,
        ) {
            Ok(report) => {
                println!("{report}");
                ExitCode::SUCCESS
            }
            Err(message) => {
                eprintln!("{message}");
                ExitCode::from(1)
            }
        },
        VmCommand::BenchmarkHttpSocket {
            iterations,
            concurrency,
            queue_capacity,
            warmup_requests,
            handler_delay_ms,
            requests_per_connection,
            payload_bytes,
            request_mix,
        } => match benchmark_http_socket(HttpSocketBenchmarkOptions {
            iterations,
            concurrency,
            queue_capacity,
            warmup_requests,
            handler_delay_ms,
            requests_per_connection,
            payload_bytes,
            request_mix,
        }) {
            Ok(report) => {
                println!("{report}");
                ExitCode::SUCCESS
            }
            Err(message) => {
                eprintln!("{message}");
                ExitCode::from(1)
            }
        },
        VmCommand::Error(message) => {
            eprintln!("{message}");
            print_usage();
            ExitCode::from(2)
        }
    }
}

/// Parses standalone VM arguments.
pub(super) fn parse_args(args: Vec<String>) -> VmCommand {
    match args.as_slice() {
        [] => VmCommand::Error("terlan-vm requires a command".to_string()),
        [flag] if matches!(flag.as_str(), "--help" | "-h" | "help") => VmCommand::Help,
        [flag] if matches!(flag.as_str(), "--version" | "-V" | "version") => VmCommand::Version,
        [command, rest @ ..] if command == "run" => parse_run_args(rest),
        [command, rest @ ..] if command == "load" => parse_load_args(rest),
        [command, rest @ ..] if command == "package-image-metadata" => {
            parse_package_image_metadata_args(rest)
        }
        [command, rest @ ..] if command == "validate-package" => parse_validate_package_args(rest),
        [command, rest @ ..] if command == "support-bundle" => parse_support_bundle_args(rest),
        [command, rest @ ..] if command == "export-persistent-actor" => {
            parse_export_persistent_actor_args(rest)
        }
        [command, rest @ ..] if command == "restore-persistent-actor" => {
            parse_restore_persistent_actor_args(rest)
        }
        [command, rest @ ..] if command == "inspect" => parse_inspect_args(rest),
        [command, rest @ ..] if command == "benchmark-http-handler" => {
            parse_benchmark_http_handler_args(rest)
        }
        [command, rest @ ..] if command == "benchmark-http-stack" => {
            parse_benchmark_http_stack_args(rest)
        }
        [command, rest @ ..] if command == "benchmark-in-memory-framing" => {
            parse_benchmark_in_memory_framing_args(rest)
        }
        [command, rest @ ..] if command == "benchmark-http-vm-stream" => {
            parse_benchmark_http_vm_stream_args(rest)
        }
        [command, rest @ ..] if command == "benchmark-http-socket" => {
            parse_benchmark_http_socket_args(rest)
        }
        [command, ..] => VmCommand::Error(format!("unknown terlan-vm command: {command}")),
    }
}

/// Parses `terlan-vm export-persistent-actor` arguments.
pub(super) fn parse_export_persistent_actor_args(args: &[String]) -> VmCommand {
    let mut parsed = VmPersistentActorExportCommandArgs::default();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--help" | "-h" => return VmCommand::Help,
            "--actor" => {
                let Some(value) = args.get(index + 1) else {
                    return VmCommand::Error("missing value for --actor".to_string());
                };
                parsed.actor_id = value.clone();
                index += 2;
            }
            "--schema" => {
                let Some(value) = args.get(index + 1) else {
                    return VmCommand::Error("missing value for --schema".to_string());
                };
                parsed.schema_id = value.clone();
                index += 2;
            }
            "--schema-version" => {
                let Some(value) = args.get(index + 1) else {
                    return VmCommand::Error("missing value for --schema-version".to_string());
                };
                parsed.schema_version = match parse_positive_u64(
                    value,
                    "export-persistent-actor",
                    "--schema-version",
                ) {
                    Ok(value) => value,
                    Err(message) => return VmCommand::Error(message),
                };
                index += 2;
            }
            "--source-machine" => {
                let Some(value) = args.get(index + 1) else {
                    return VmCommand::Error("missing value for --source-machine".to_string());
                };
                parsed.source_machine_id = value.clone();
                index += 2;
            }
            "--source-adapter" => {
                let Some(value) = args.get(index + 1) else {
                    return VmCommand::Error("missing value for --source-adapter".to_string());
                };
                parsed.source_adapter_kind = value.clone();
                index += 2;
            }
            "--snapshot-generation" => {
                let Some(value) = args.get(index + 1) else {
                    return VmCommand::Error("missing value for --snapshot-generation".to_string());
                };
                parsed.snapshot_generation = match parse_positive_u64(
                    value,
                    "export-persistent-actor",
                    "--snapshot-generation",
                ) {
                    Ok(value) => value,
                    Err(message) => return VmCommand::Error(message),
                };
                index += 2;
            }
            "--last-sequence" => {
                let Some(value) = args.get(index + 1) else {
                    return VmCommand::Error("missing value for --last-sequence".to_string());
                };
                parsed.last_event_sequence = match value.parse::<u64>() {
                    Ok(value) => value,
                    Err(_) => {
                        return VmCommand::Error(format!(
                            "terlan-vm export-persistent-actor --last-sequence expects a non-negative integer, got `{value}`"
                        ));
                    }
                };
                index += 2;
            }
            "--event-count" => {
                let Some(value) = args.get(index + 1) else {
                    return VmCommand::Error("missing value for --event-count".to_string());
                };
                parsed.retained_event_count = match value.parse::<usize>() {
                    Ok(value) => value,
                    Err(_) => {
                        return VmCommand::Error(format!(
                            "terlan-vm export-persistent-actor --event-count expects a non-negative integer, got `{value}`"
                        ));
                    }
                };
                index += 2;
            }
            "--redact" => {
                let Some(value) = args.get(index + 1) else {
                    return VmCommand::Error("missing value for --redact".to_string());
                };
                parsed.redacted_fields.push(value.clone());
                index += 2;
            }
            "--resource" => {
                let Some(value) = args.get(index + 1) else {
                    return VmCommand::Error("missing value for --resource".to_string());
                };
                parsed.resource_handles.push(value.clone());
                index += 2;
            }
            arg if arg.starts_with('-') => {
                return VmCommand::Error(format!(
                    "unknown terlan-vm export-persistent-actor option: {arg}"
                ));
            }
            arg => {
                return VmCommand::Error(format!(
                    "terlan-vm export-persistent-actor does not accept positional argument `{arg}`"
                ));
            }
        }
    }

    if parsed.actor_id.is_empty() {
        return VmCommand::Error("terlan-vm export-persistent-actor requires --actor".to_string());
    }
    if parsed.schema_id.is_empty() {
        return VmCommand::Error("terlan-vm export-persistent-actor requires --schema".to_string());
    }
    if parsed.schema_version == 0 {
        return VmCommand::Error(
            "terlan-vm export-persistent-actor requires --schema-version".to_string(),
        );
    }
    if parsed.source_machine_id.is_empty() {
        return VmCommand::Error(
            "terlan-vm export-persistent-actor requires --source-machine".to_string(),
        );
    }

    VmCommand::ExportPersistentActor { args: parsed }
}

/// Parses `terlan-vm restore-persistent-actor` arguments.
pub(super) fn parse_restore_persistent_actor_args(args: &[String]) -> VmCommand {
    let mut parsed = VmPersistentActorRestoreCommandArgs::default();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--help" | "-h" => return VmCommand::Help,
            "--actor" => {
                let Some(value) = args.get(index + 1) else {
                    return VmCommand::Error("missing value for --actor".to_string());
                };
                parsed.actor_id = value.clone();
                index += 2;
            }
            "--schema" => {
                let Some(value) = args.get(index + 1) else {
                    return VmCommand::Error("missing value for --schema".to_string());
                };
                parsed.schema_id = value.clone();
                index += 2;
            }
            "--schema-version" => {
                let Some(value) = args.get(index + 1) else {
                    return VmCommand::Error("missing value for --schema-version".to_string());
                };
                parsed.schema_version =
                    match parse_positive_u64(value, "restore-persistent-actor", "--schema-version")
                    {
                        Ok(value) => value,
                        Err(message) => return VmCommand::Error(message),
                    };
                index += 2;
            }
            "--source-adapter" => {
                let Some(value) = args.get(index + 1) else {
                    return VmCommand::Error("missing value for --source-adapter".to_string());
                };
                parsed.source_adapter_kind = value.clone();
                index += 2;
            }
            "--target-adapter" => {
                let Some(value) = args.get(index + 1) else {
                    return VmCommand::Error("missing value for --target-adapter".to_string());
                };
                parsed.target_adapter_kind = value.clone();
                index += 2;
            }
            "--snapshot-generation" => {
                let Some(value) = args.get(index + 1) else {
                    return VmCommand::Error("missing value for --snapshot-generation".to_string());
                };
                parsed.snapshot_generation = match parse_positive_u64(
                    value,
                    "restore-persistent-actor",
                    "--snapshot-generation",
                ) {
                    Ok(value) => value,
                    Err(message) => return VmCommand::Error(message),
                };
                index += 2;
            }
            "--last-sequence" => {
                let Some(value) = args.get(index + 1) else {
                    return VmCommand::Error("missing value for --last-sequence".to_string());
                };
                parsed.last_event_sequence = match value.parse::<u64>() {
                    Ok(value) => value,
                    Err(_) => {
                        return VmCommand::Error(format!(
                            "terlan-vm restore-persistent-actor --last-sequence expects a non-negative integer, got `{value}`"
                        ));
                    }
                };
                index += 2;
            }
            "--event-count" => {
                let Some(value) = args.get(index + 1) else {
                    return VmCommand::Error("missing value for --event-count".to_string());
                };
                parsed.retained_event_count = match value.parse::<usize>() {
                    Ok(value) => value,
                    Err(_) => {
                        return VmCommand::Error(format!(
                            "terlan-vm restore-persistent-actor --event-count expects a non-negative integer, got `{value}`"
                        ));
                    }
                };
                index += 2;
            }
            "--redact" => {
                let Some(value) = args.get(index + 1) else {
                    return VmCommand::Error("missing value for --redact".to_string());
                };
                parsed.redacted_fields.push(value.clone());
                index += 2;
            }
            "--resource" => {
                let Some(value) = args.get(index + 1) else {
                    return VmCommand::Error("missing value for --resource".to_string());
                };
                parsed.resource_handles.push(value.clone());
                index += 2;
            }
            "--available-resource" => {
                let Some(value) = args.get(index + 1) else {
                    return VmCommand::Error("missing value for --available-resource".to_string());
                };
                parsed.available_resource_handles.push(value.clone());
                index += 2;
            }
            "--allow-cross-adapter" => {
                parsed.allow_cross_adapter_restore = true;
                index += 1;
            }
            "--compacted" => {
                parsed.compacted = true;
                index += 1;
            }
            arg if arg.starts_with('-') => {
                return VmCommand::Error(format!(
                    "unknown terlan-vm restore-persistent-actor option: {arg}"
                ));
            }
            arg => {
                return VmCommand::Error(format!(
                    "terlan-vm restore-persistent-actor does not accept positional argument `{arg}`"
                ));
            }
        }
    }

    if parsed.actor_id.is_empty() {
        return VmCommand::Error("terlan-vm restore-persistent-actor requires --actor".to_string());
    }
    if parsed.schema_id.is_empty() {
        return VmCommand::Error(
            "terlan-vm restore-persistent-actor requires --schema".to_string(),
        );
    }
    if parsed.schema_version == 0 {
        return VmCommand::Error(
            "terlan-vm restore-persistent-actor requires --schema-version".to_string(),
        );
    }

    VmCommand::RestorePersistentActor { args: parsed }
}

pub(super) fn parse_positive_u64(value: &str, command: &str, flag: &str) -> Result<u64, String> {
    let parsed = value.parse::<u64>().map_err(|_| {
        format!("terlan-vm {command} {flag} expects a positive integer, got `{value}`")
    })?;
    if parsed == 0 {
        return Err(format!("terlan-vm {command} {flag} must be greater than 0"));
    }
    Ok(parsed)
}
