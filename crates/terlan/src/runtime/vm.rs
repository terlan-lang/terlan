pub(crate) mod accelerator_operation;
pub(crate) mod acme_worker;
pub(crate) mod actor;
pub(crate) mod aot_metadata;
pub(crate) mod bitstring;
#[cfg(test)]
pub(crate) mod call_count;
#[cfg(test)]
pub(crate) mod call_memory;
#[cfg(test)]
pub(crate) mod call_metric;
#[cfg(test)]
pub(crate) mod call_time;
vm_capability_component! {
    pub(crate) mod capability_worker;
}
#[cfg(test)]
pub(crate) mod checksum;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) mod code_server;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) mod code_server_compiler;
#[cfg(test)]
pub(crate) mod coordination;
pub(crate) mod debugger_control;
#[cfg(test)]
pub(crate) mod debugger_transport;
#[cfg(test)]
pub(crate) mod distributed_scheduler;
#[cfg(test)]
pub(crate) mod distributed_state;
#[cfg(test)]
pub(crate) mod distributed_storage;
#[cfg(test)]
pub(crate) mod driver;
#[cfg(test)]
pub(crate) mod dynamic_module;
vm_capability_component! {
    #[cfg(test)]
    pub(crate) mod epmd;
}
pub(crate) mod execution_shard_epoch;
pub(crate) mod execution_shard_protocol;
pub(crate) mod execution_shard_supervisor;
pub(crate) mod failure;
pub(crate) mod fatal_diagnostics;
pub(crate) mod fixed_scheduler_control;
pub(crate) mod fixed_scheduler_telemetry;
pub(crate) mod framing;
#[cfg(test)]
pub(crate) mod http;
#[cfg(test)]
pub(crate) mod http_metrics;
mod http_response_value;
pub(crate) mod http_router;
pub(crate) mod http_session;
#[cfg(test)]
pub(crate) mod http_static;
#[cfg(test)]
pub(crate) mod io_diagnostics;
#[cfg(test)]
pub(crate) mod io_reactor;
pub(crate) mod io_runtime_boundary;
pub(crate) mod iovec;
#[cfg(test)]
pub(crate) mod live_template_protocol;
pub(crate) mod local_trace;
pub(crate) mod protocol_task_executor;
#[cfg(any(test, feature = "benchmark-tools"))]
pub(crate) use super::map_layout;
pub(crate) mod actor_directory;
#[cfg(any(test, feature = "benchmark-tools"))]
pub(crate) mod map_value;
pub(crate) mod memory;
pub(crate) mod meta_trace;
pub(crate) mod model_sync;
pub(crate) mod multicore_replay;
pub(crate) mod native_boundary;
pub(crate) mod native_callable;
pub(crate) mod native_exchange;
pub(crate) mod native_image_diagnostics;
pub(crate) mod package_native_helper;
#[cfg(test)]
pub(crate) mod package_transport;
#[cfg(test)]
pub(crate) mod packet;
#[cfg(any(test, feature = "benchmark-tools"))]
pub(crate) mod persistent_actor_adapter;
#[cfg(any(test, feature = "benchmark-tools"))]
pub(crate) mod persistent_actor_compaction;
pub(crate) mod persistent_actor_performance;
pub(crate) mod persistent_actor_policy;
pub(crate) mod persistent_actor_restore;
#[cfg(any(test, feature = "benchmark-tools"))]
pub(crate) mod persistent_actor_schema;
pub(crate) mod persistent_actor_store;
#[cfg(test)]
pub(crate) mod persistent_actor_telemetry;
#[cfg(test)]
pub(crate) mod persistent_actor_telemetry_aggregation;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) mod postgres;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) mod postgres_command;
pub(crate) mod process;
pub(crate) mod process_alias;
#[cfg(test)]
pub(crate) mod process_environment;
pub(crate) mod pure_native;
pub(crate) mod reference;
pub(crate) mod resource;
pub(crate) mod restart_backoff;
pub(crate) mod scheduler;
pub(crate) mod scheduler_topology;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) mod source_reload;
pub(crate) mod sse;
#[cfg(test)]
mod statistics;
pub mod supervision;
pub(crate) mod support_bundle;
#[cfg(test)]
mod system_information;
#[cfg(test)]
mod system_profile;
pub(crate) mod table;
pub(crate) mod tcp;
pub(crate) mod tcp_scheduler;
#[cfg(test)]
pub(crate) mod term_format;
pub(crate) mod time;
pub(crate) mod timer;
#[cfg(test)]
pub(crate) mod tls;
#[cfg(test)]
pub(crate) mod tls_test_support;
#[cfg(test)]
pub(crate) mod udp;
mod value;
pub(crate) use http_response_value::{VmAotHttpResponse, VmHttpCallResult};
pub(crate) use value::ReplValue;
pub(crate) mod websocket;
pub(crate) mod work_stealing;

/// Links the production multicore ownership modules into sanitizer harnesses.
pub fn link_multicore_sanitizer_surface() {
    let sizes = (
        std::mem::size_of::<actor_directory::VmActorDirectory<(), ()>>(),
        std::mem::size_of::<fixed_scheduler_control::VmFixedSchedulerControl<()>>(),
        std::mem::size_of::<process::VmProcessId>(),
        std::mem::size_of::<scheduler_topology::VmSchedulerTopology>(),
    );
    std::hint::black_box(sizes);
}

/// Runs the production fixed-scheduler memory-model stress under sanitizer builds.
#[cfg(feature = "multicore-tsan-harness")]
pub fn run_multicore_sanitizer_stress() {
    fixed_scheduler_control::run_multicore_sanitizer_stress();
}

/// Runs one watchdog child selected by the sanitizer harness environment.
#[cfg(feature = "multicore-tsan-harness")]
pub fn run_multicore_sanitizer_seed() {
    fixed_scheduler_control::run_multicore_sanitizer_seed();
}

/// Measurements from the production actor continuation park/wakeup path.
#[cfg(feature = "benchmark-tools")]
#[derive(Clone, Debug, serde::Serialize)]
pub struct VmAcceleratorSchedulingBenchmark {
    /// Number of actors rotated through the benchmark.
    pub actors: usize,
    /// Number of complete park/wakeup cycles measured.
    pub iterations: usize,
    /// Median actor suspension latency in nanoseconds.
    pub suspension_median_ns: u64,
    /// 95th-percentile actor suspension latency in nanoseconds.
    pub suspension_p95_ns: u64,
    /// Median actor wakeup latency in nanoseconds.
    pub wakeup_median_ns: u64,
    /// 95th-percentile actor wakeup latency in nanoseconds.
    pub wakeup_p95_ns: u64,
    /// Complete suspension/wakeup cycles per second.
    pub concurrent_actor_throughput_per_second: f64,
}

/// Typed failure shared by VM runtime subsystems that previously erased
/// operational context into bare strings.
#[derive(Eq, PartialEq)]
pub(crate) struct VmRuntimeError(terlan_runtime_abi::BoundaryError);

impl VmRuntimeError {
    pub(crate) fn message(rendered: impl Into<String>) -> Self {
        Self(terlan_runtime_abi::BoundaryError::message(
            terlan_runtime_abi::ErrorDomain::VmRuntime,
            "execute VM runtime operation",
            rendered,
        ))
    }
}

impl std::fmt::Debug for VmRuntimeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

impl std::fmt::Display for VmRuntimeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

impl std::error::Error for VmRuntimeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.source()
    }
}

impl std::ops::Deref for VmRuntimeError {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.0.context()
    }
}

impl From<String> for VmRuntimeError {
    fn from(rendered: String) -> Self {
        Self::message(rendered)
    }
}

impl From<&str> for VmRuntimeError {
    fn from(rendered: &str) -> Self {
        rendered.to_owned().into()
    }
}

impl From<terlan_runtime_abi::BoundaryError> for VmRuntimeError {
    fn from(error: terlan_runtime_abi::BoundaryError) -> Self {
        Self(error)
    }
}

impl From<VmRuntimeError> for String {
    fn from(error: VmRuntimeError) -> Self {
        error.to_string()
    }
}

pub(crate) type VmRuntimeResult<T> = Result<T, VmRuntimeError>;

/// Typed failure from the accelerator continuation scheduling benchmark.
#[cfg(feature = "benchmark-tools")]
pub struct VmAcceleratorSchedulingError(terlan_runtime_abi::BoundaryError);

#[cfg(feature = "benchmark-tools")]
impl From<String> for VmAcceleratorSchedulingError {
    fn from(message: String) -> Self {
        Self(terlan_runtime_abi::BoundaryError::message(
            terlan_runtime_abi::ErrorDomain::VmRuntime,
            "benchmark accelerator scheduling",
            message,
        ))
    }
}

#[cfg(feature = "benchmark-tools")]
impl std::fmt::Debug for VmAcceleratorSchedulingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

#[cfg(feature = "benchmark-tools")]
impl std::fmt::Display for VmAcceleratorSchedulingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

#[cfg(feature = "benchmark-tools")]
impl std::error::Error for VmAcceleratorSchedulingError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.source()
    }
}

#[cfg(feature = "benchmark-tools")]
impl From<VmAcceleratorSchedulingError> for String {
    fn from(error: VmAcceleratorSchedulingError) -> Self {
        error.to_string()
    }
}

/// Measures the same continuation suspension and scheduler wakeup path used by
/// asynchronous accelerator operations.
#[cfg(feature = "benchmark-tools")]
pub fn benchmark_accelerator_scheduling(
    iterations: usize,
    actors: usize,
) -> Result<VmAcceleratorSchedulingBenchmark, VmAcceleratorSchedulingError> {
    use std::time::Instant;

    use actor::VmActorRuntime;
    use process::VmProcessSource;

    if iterations == 0 || actors == 0 {
        return Err(
            "accelerator scheduling benchmark dimensions must be positive"
                .to_string()
                .into(),
        );
    }
    let mut runtime = VmActorRuntime::default();
    let owners = (0..actors)
        .map(|index| {
            runtime.spawn_root(VmProcessSource::new(
                "accelerator.performance",
                format!("owner_{index}"),
                0,
            ))
        })
        .collect::<Vec<_>>();
    let mut suspension_samples = Vec::with_capacity(iterations);
    let mut wakeup_samples = Vec::with_capacity(iterations);
    let benchmark_started = Instant::now();
    for index in 0..iterations {
        let owner = owners[index % owners.len()];
        let identity = u64::try_from(index)
            .ok()
            .and_then(|value| value.checked_add(1))
            .ok_or_else(|| "accelerator scheduling benchmark identity overflow".to_string())?;
        let started = Instant::now();
        runtime.park_native_continuation(owner.as_u64(), identity, identity)?;
        suspension_samples.push(nanoseconds(started.elapsed().as_nanos()));
        let started = Instant::now();
        runtime.resume_native_continuation(owner.as_u64(), identity, identity)?;
        wakeup_samples.push(nanoseconds(started.elapsed().as_nanos()));
    }
    let elapsed = benchmark_started.elapsed().as_secs_f64();
    Ok(VmAcceleratorSchedulingBenchmark {
        actors,
        iterations,
        suspension_median_ns: percentile_ns(&mut suspension_samples, 50),
        suspension_p95_ns: percentile_ns(&mut suspension_samples, 95),
        wakeup_median_ns: percentile_ns(&mut wakeup_samples, 50),
        wakeup_p95_ns: percentile_ns(&mut wakeup_samples, 95),
        concurrent_actor_throughput_per_second: iterations as f64 / elapsed,
    })
}

#[cfg(feature = "benchmark-tools")]
fn nanoseconds(value: u128) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

#[cfg(feature = "benchmark-tools")]
fn percentile_ns(samples: &mut [u64], percentile: usize) -> u64 {
    samples.sort_unstable();
    let index = samples
        .len()
        .saturating_mul(percentile)
        .div_ceil(100)
        .saturating_sub(1)
        .min(samples.len().saturating_sub(1));
    samples[index]
}
