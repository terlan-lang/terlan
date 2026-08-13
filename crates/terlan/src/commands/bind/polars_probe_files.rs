#[path = "polars_probe_files/bridge.rs"]
mod bridge;
#[path = "polars_probe_files/contracts.rs"]
mod contracts;

pub(in crate::commands::bind) use bridge::POLARS_FILES;
pub(in crate::commands::bind) use contracts::GeneratedFile;
