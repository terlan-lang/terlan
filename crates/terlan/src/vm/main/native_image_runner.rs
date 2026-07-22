use std::path::Path;

use crate::runtime::vm::pure_native::PureNativeExecutionShard;
use crate::runtime::vm::ReplValue;

use super::evaluate_test_result;

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

/// Captures deterministic structural support metadata for one admitted image.
pub(super) fn render_tvm_support_bundle(path: &Path) -> Result<String, String> {
    let mut shard = PureNativeExecutionShard::load_image(path)?;
    let bundle = shard.native_support_bundle()?;
    let bytes = bundle.serialized_bytes()?;
    shard.shutdown()?;
    String::from_utf8(bytes).map_err(|error| format!("error[tvm.support_bundle.utf8]: {error}"))
}
