use std::path::Path;
use std::time::Instant;

use serde::Serialize;

use crate::runtime::vm::pure_native::PureNativeExecutionShard;
use crate::runtime::vm::ReplValue;

use super::evaluate_test_result;

/// Raw in-process timing evidence for one generated zero-arity export.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct NativeImageBenchmarkReport {
    schema: &'static str,
    entry: String,
    warmup_iterations: usize,
    samples: usize,
    operations_per_sample: usize,
    sample_ns: Vec<u128>,
    result_kind: String,
    completed_calls: u64,
    retained_generation_references: usize,
}

/// Returns whether a path names a target-native TVM executable image.
pub(super) fn is_tvm_image_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension == "tvm")
}

/// Statically admits and executes one zero-arity export from a native image.
pub(super) fn run_tvm_image(path: &Path, entry: &str, test_eval: bool) -> Result<(), String> {
    let mut shard = PureNativeExecutionShard::load_image(path)?;
    let result: Result<ReplValue, String> = shard.call(entry, &[]);
    let shutdown = shard.shutdown();
    let value = result?;
    shutdown?;
    if test_eval {
        evaluate_test_result(value)?;
    }
    Ok(())
}

/// Measures generated code and its actor/runtime ABI path inside one loaded shard.
pub(super) fn benchmark_tvm_image(
    path: &Path,
    entry: &str,
    warmup_iterations: usize,
    samples: usize,
    operations_per_sample: usize,
) -> Result<NativeImageBenchmarkReport, String> {
    if samples == 0 || operations_per_sample == 0 {
        return Err("native image benchmark requires non-zero samples and operations".to_string());
    }
    let mut shard = PureNativeExecutionShard::load_image(path)?;
    let mut last = ReplValue::Unit;
    for _ in 0..warmup_iterations {
        last = shard.call(entry, &[])?;
    }
    let mut sample_ns = Vec::with_capacity(samples);
    for _ in 0..samples {
        let started = Instant::now();
        for _ in 0..operations_per_sample {
            last = shard.call(entry, &[])?;
        }
        sample_ns.push(started.elapsed().as_nanos());
    }
    let completed_calls = shard.completed_call_count();
    let references = shard.generation_references();
    let retained_generation_references = references.total();
    let result_kind = match last {
        ReplValue::Unit => "Unit",
        ReplValue::Int(_) => "Int",
        ReplValue::Float(_) => "Float",
        ReplValue::String(_) => "String",
        ReplValue::Bytes(_) => "Bytes",
        ReplValue::BitString(_) => "BitString",
        ReplValue::Atom(_) => "Atom",
        ReplValue::Bool(_) => "Bool",
        #[cfg(test)]
        ReplValue::RandomGenerator(_) => "RandomGenerator",
        ReplValue::Type(_) => "Type",
        ReplValue::Tuple(_) => "Tuple",
        ReplValue::Record { .. } => "Record",
        ReplValue::List(_) => "List",
        ReplValue::Map(_) => "Map",
        #[cfg(test)]
        ReplValue::MapIndexed(_) => "MapIndexed",
        ReplValue::Set(_) => "Set",
        #[cfg(test)]
        ReplValue::Iterator { .. } => "Iterator",
    }
    .to_string();
    shard.shutdown()?;
    Ok(NativeImageBenchmarkReport {
        schema: "terlan-native-image-generated-benchmark-v1",
        entry: entry.to_string(),
        warmup_iterations,
        samples,
        operations_per_sample,
        sample_ns,
        result_kind,
        completed_calls,
        retained_generation_references,
    })
}

/// Captures deterministic structural support metadata for one admitted image.
pub(super) fn render_tvm_support_bundle(path: &Path) -> Result<String, String> {
    let mut shard = PureNativeExecutionShard::load_image(path)?;
    let bundle = shard.native_support_bundle()?;
    let bytes = bundle.serialized_bytes()?;
    shard.shutdown()?;
    String::from_utf8(bytes).map_err(|error| format!("error[tvm.support_bundle.utf8]: {error}"))
}
