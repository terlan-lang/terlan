mod http_response;
#[path = "std_runtime/module_support.rs"]
mod module_support;
#[path = "std_runtime/operation_support.rs"]
mod operation_support;

pub(in crate::validation::target_profile) use module_support::{
    std_call_heads, std_module_import_violation, target_profile_supports_task_operation,
    target_profile_supports_vm_mutable_receiver_call, target_profile_supports_vm_std_remote_call,
    validate_core_imports, StdCallHeads,
};
pub(in crate::validation::target_profile) use operation_support::{
    target_profile_supports_vm_intrinsic, target_profile_supports_vm_native_bridge_operation,
    validate_std_runtime_operation_summary_support, validate_std_runtime_operation_support,
    StdRuntimeOperationPolicy,
};
