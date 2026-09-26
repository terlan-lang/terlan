use serde::{Deserialize, Serialize};

use super::*;

/// Repeated dispatcher metadata shared by a set of functions with one shape.
#[derive(Debug, Serialize, Deserialize)]
pub(super) struct CAbiBindingFunctionFamily {
    /// Prefix joined with each member operation suffix.
    operation_prefix: String,
    /// C dispatcher symbol used by every family member.
    c_symbol: String,
    /// Receiver and ownership role shared by every member.
    role: CAbiFunctionRole,
    /// Public argument list shared by every member.
    #[serde(default)]
    args: Vec<CAbiBindingArg>,
    /// Public return type shared by every member.
    returns: String,
    /// Blocking policy shared by every member.
    blocking: CAbiBlockingPolicy,
    /// Resource policy shared by every member.
    resource: CAbiResourcePolicy,
    /// Source visibility shared by every member.
    #[serde(default)]
    visibility: CAbiTerlanVisibility,
    /// Generated smoke-test policy shared by every member.
    #[serde(default)]
    generated_smoke: CGeneratedSmokePolicy,
    /// Stable dispatcher contract shared by every member.
    dispatcher: CDispatcherFunctionFamily,
    /// Function-specific identities expanded from the shared contract.
    members: Vec<CAbiBindingFunctionFamilyMember>,
}

/// Stable dispatcher fields common to one function family.
#[derive(Debug, Serialize, Deserialize)]
struct CDispatcherFunctionFamily {
    /// Symbol that creates an independently owned tensor handle.
    duplicate_handle_symbol: String,
    /// Optional StableIValue allocator symbol.
    #[serde(default)]
    optional_value_allocator_symbol: Option<String>,
    /// Optional StableIValue destructor symbol.
    #[serde(default)]
    optional_value_destructor_symbol: Option<String>,
    /// Stable list allocator symbol.
    #[serde(default)]
    list_allocator_symbol: Option<String>,
    /// Stable list append symbol.
    #[serde(default)]
    list_push_symbol: Option<String>,
    /// Stable list destructor symbol.
    #[serde(default)]
    list_destructor_symbol: Option<String>,
    /// Stable string allocator symbol.
    #[serde(default)]
    string_allocator_symbol: Option<String>,
    /// Stable string destructor symbol.
    #[serde(default)]
    string_destructor_symbol: Option<String>,
    /// Extension ABI version required by every member.
    extension_abi_version: String,
    /// StableIValue input/output stack shape shared by every member.
    stack: Vec<CDispatcherStackValue>,
    /// Result ownership contract shared by every member.
    output: CDispatcherOutput,
}

/// Function-specific identity inside one shared dispatcher family.
#[derive(Debug, Serialize, Deserialize)]
struct CAbiBindingFunctionFamilyMember {
    /// Public Terlan function name.
    name: String,
    /// Optional Rust adapter name when it differs from the public name.
    #[serde(default)]
    adapter_name: Option<String>,
    /// Optional operation suffix; defaults to the public function name.
    #[serde(default)]
    operation_suffix: Option<String>,
    /// Native dispatcher operator name.
    operator_name: String,
    /// Native dispatcher overload name; empty selects the default overload.
    #[serde(default)]
    overload_name: String,
    /// Public API documentation for the expanded function.
    documentation: String,
}

/// Expands concise dispatcher families into ordinary binding functions.
pub(super) fn expand_function_families(manifest: &mut CAbiBindingManifest) -> Result<(), String> {
    for module in &mut manifest.modules {
        for family in std::mem::take(&mut module.function_families) {
            if family.operation_prefix.is_empty() || family.operation_prefix.ends_with('.') {
                return Err(format!(
                    "error[native_bindgen.function_family]: module `{}` has invalid operation_prefix `{}`",
                    module.module, family.operation_prefix
                ));
            }
            if family.members.is_empty() {
                return Err(format!(
                    "error[native_bindgen.function_family]: module `{}` has a function family without members",
                    module.module
                ));
            }
            let mut member_names = BTreeSet::new();
            for member in family.members {
                if !member_names.insert(member.name.clone()) {
                    return Err(format!(
                        "error[native_bindgen.function_family]: module `{}` repeats family member `{}`",
                        module.module, member.name
                    ));
                }
                let operation = format!(
                    "{}.{}",
                    family.operation_prefix,
                    member
                        .operation_suffix
                        .as_deref()
                        .unwrap_or(member.name.as_str())
                );
                let dispatcher = CDispatcherBinding {
                    duplicate_handle_symbol: family.dispatcher.duplicate_handle_symbol.clone(),
                    optional_value_allocator_symbol: family
                        .dispatcher
                        .optional_value_allocator_symbol
                        .clone(),
                    optional_value_destructor_symbol: family
                        .dispatcher
                        .optional_value_destructor_symbol
                        .clone(),
                    list_allocator_symbol: family.dispatcher.list_allocator_symbol.clone(),
                    list_push_symbol: family.dispatcher.list_push_symbol.clone(),
                    list_destructor_symbol: family.dispatcher.list_destructor_symbol.clone(),
                    string_allocator_symbol: family.dispatcher.string_allocator_symbol.clone(),
                    string_destructor_symbol: family.dispatcher.string_destructor_symbol.clone(),
                    operator_name: member.operator_name,
                    overload_name: member.overload_name,
                    extension_abi_version: family.dispatcher.extension_abi_version.clone(),
                    stack: family.dispatcher.stack.clone(),
                    output: family.dispatcher.output.clone(),
                };
                module.functions.push(CAbiBindingFunction {
                    name: member.name,
                    adapter_name: member.adapter_name,
                    visibility: family.visibility,
                    operation,
                    c_symbol: family.c_symbol.clone(),
                    role: family.role,
                    args: family.args.clone(),
                    returns: family.returns.clone(),
                    blocking: family.blocking,
                    resource: family.resource,
                    documentation: member.documentation,
                    dispatcher: Some(dispatcher),
                    generated_smoke: family.generated_smoke,
                });
            }
        }
    }
    Ok(())
}
