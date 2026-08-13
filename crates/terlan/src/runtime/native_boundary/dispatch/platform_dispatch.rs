//! Typed host-platform projections for the NativeBoundary dispatcher.

use crate::terlan_native::platform;

use super::{args::unknown_operation, DispatchError, NativeBoundaryValue};

pub(super) fn dispatch(operation: &str) -> Result<NativeBoundaryValue, DispatchError> {
    match operation {
        "std.system.platform.current" => {
            let host = platform::current();
            Ok(NativeBoundaryValue::Record {
                name: "Host".to_string(),
                fields: vec![
                    (
                        "operating_system".to_string(),
                        NativeBoundaryValue::Text(host.operating_system),
                    ),
                    (
                        "architecture".to_string(),
                        NativeBoundaryValue::Text(host.architecture),
                    ),
                    (
                        "path_separator".to_string(),
                        NativeBoundaryValue::Text(host.path_separator),
                    ),
                    (
                        "executable_suffix".to_string(),
                        NativeBoundaryValue::Text(host.executable_suffix),
                    ),
                ],
            })
        }
        "std.system.platform.current_metrics" => {
            let metrics = platform::current_metrics();
            Ok(NativeBoundaryValue::Record {
                name: "HostMetrics".to_string(),
                fields: vec![
                    (
                        "available".to_string(),
                        NativeBoundaryValue::Bool(metrics.available),
                    ),
                    (
                        "message".to_string(),
                        NativeBoundaryValue::Text(metrics.message),
                    ),
                    (
                        "kernel".to_string(),
                        NativeBoundaryValue::Text(metrics.kernel),
                    ),
                    (
                        "operating_system".to_string(),
                        NativeBoundaryValue::Text(metrics.operating_system),
                    ),
                    (
                        "cpu_model".to_string(),
                        NativeBoundaryValue::Text(metrics.cpu_model),
                    ),
                    (
                        "memory_bytes".to_string(),
                        NativeBoundaryValue::Int(metrics.memory_bytes),
                    ),
                    (
                        "available_memory_bytes".to_string(),
                        NativeBoundaryValue::Int(metrics.available_memory_bytes),
                    ),
                    (
                        "cpu_affinity".to_string(),
                        NativeBoundaryValue::List(
                            metrics
                                .cpu_affinity
                                .into_iter()
                                .map(NativeBoundaryValue::Int)
                                .collect(),
                        ),
                    ),
                    (
                        "cpu_governor".to_string(),
                        NativeBoundaryValue::Text(metrics.cpu_governor),
                    ),
                    (
                        "load_1m".to_string(),
                        NativeBoundaryValue::Float(metrics.load_1m),
                    ),
                    (
                        "load_5m".to_string(),
                        NativeBoundaryValue::Float(metrics.load_5m),
                    ),
                    (
                        "load_15m".to_string(),
                        NativeBoundaryValue::Float(metrics.load_15m),
                    ),
                ],
            })
        }
        _ => Err(unknown_operation(operation)),
    }
}
