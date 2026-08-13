#[path = "integration_test/manifest_and_arguments.rs"]
mod manifest_and_arguments;
pub(crate) use manifest_and_arguments::run;
#[path = "integration_test/phase_execution.rs"]
mod phase_execution;
