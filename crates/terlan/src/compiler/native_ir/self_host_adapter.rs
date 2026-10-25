//! Fail-closed ABI-1 to NativeIR adapter for the first self-hosted scalar slice.

#[path = "self_host_adapter/expressions.rs"]
mod expressions;
pub(super) use expressions::decoded_string_literal;
use expressions::*;
#[path = "self_host_adapter/regions.rs"]
mod regions;
use regions::*;
#[path = "self_host_adapter/instructions.rs"]
mod instructions;
use instructions::*;
#[path = "self_host_adapter/control_flow.rs"]
mod control_flow;
use control_flow::*;

use std::collections::{HashMap, HashSet};
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use crate::runtime::native_image::managed::{encode_closure_allocation, ManagedClosureDescriptor};
use crate::self_host_ir::ABI_MINOR;
use crate::self_host_ir::{
    BackendEnvelope, IrBlock, IrFunction, IrInstruction, IrRegion, IrStructField, IrTryRegion,
};
#[cfg(test)]
use crate::self_host_ir::{IrParameter, IrTerminator};

use super::function_lowering::{NativeFunction, NativeModule};
use super::identity::stable_export_id;
use super::model::{NativeBinaryOperator, NativeExpr, NativeType};

type ValueCatalog = HashMap<String, (usize, NativeType)>;

#[derive(Clone, Debug)]
struct LocalFunction {
    index: usize,
    capture_count: usize,
    parameters: Vec<NativeType>,
    result: NativeType,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct LocalStructField {
    pub(super) index: usize,
    pub(super) type_name: String,
}

struct FunctionCatalog {
    callables: HashMap<(String, String, usize), LocalFunction>,
    structs: HashMap<String, HashMap<String, LocalStructField>>,
    variants: Option<crate::self_host_variant_ir::VariantCatalog>,
}

impl FunctionCatalog {
    fn new() -> Self {
        Self {
            callables: HashMap::new(),
            structs: HashMap::new(),
            variants: None,
        }
    }
}

impl Deref for FunctionCatalog {
    type Target = HashMap<(String, String, usize), LocalFunction>;

    fn deref(&self) -> &Self::Target {
        &self.callables
    }
}

impl DerefMut for FunctionCatalog {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.callables
    }
}

const MAX_APPLICATION_MODULES: usize = 4_096;
const MAX_APPLICATION_FUNCTIONS: usize = 1_000_000;
const MAX_MODULE_FUNCTIONS: usize = 65_536;
const MAX_FUNCTION_BLOCKS: usize = 65_536;
const MAX_FUNCTION_INSTRUCTIONS: usize = 1_000_000;
const MAX_CFG_EXPANSIONS: usize = 4_000_000;
const MAX_INSTRUCTION_OPERANDS: usize = 65_536;
const MAX_IDENTITY_BYTES: usize = 4_096;

fn adapter_error(context: &str, message: impl AsRef<str>) -> String {
    format!(
        "error[self-host.native-adapter.{context}]: {}",
        message.as_ref()
    )
}

fn native_type(type_name: &str) -> Result<NativeType, String> {
    if let Some(native) = super::self_host_managed_layouts::native_type(type_name)? {
        return Ok(native);
    }
    match type_name {
        "Unit" => Ok(NativeType::Unit),
        "Int" => Ok(NativeType::Int),
        "Float" => Ok(NativeType::Float),
        "Bool" => Ok(NativeType::Bool),
        "Atom" => Ok(NativeType::Atom),
        "String" => Ok(NativeType::StringRef),
        "Bytes" => Ok(NativeType::BytesRef),
        "Binary" => Ok(NativeType::BinaryRef),
        "" | "Dynamic" => Err(adapter_error(
            "type",
            "unresolved or dynamic types cannot enter direct native lowering",
        )),
        other => Err(adapter_error(
            "type",
            format!("managed type `{other}` does not yet have an ABI-1 layout adapter"),
        )),
    }
}

#[cfg(test)]
#[path = "self_host_adapter_map_construct_test.rs"]
mod map_construct_tests;

#[cfg(test)]
#[path = "self_host_adapter_try_region_test.rs"]
mod try_region_tests;

fn call_instruction(opcode: &str) -> bool {
    matches!(
        opcode,
        "call-local"
            | "call-import"
            | "call-local-mut-receiver"
            | "call-import-mut-receiver"
            | "call-value"
    )
}

fn validate_actor_sites(
    function: &IrFunction,
    actor_sites: &[super::self_host_actor_adapter::NativeActorSuspensionSite],
) -> Result<(), String> {
    let calls = function
        .blocks
        .iter()
        .flat_map(|block| {
            block
                .instructions
                .iter()
                .filter(|instruction| call_instruction(&instruction.opcode))
                .map(move |instruction| (block.label.as_str(), instruction))
        })
        .collect::<Vec<_>>();
    let mut anchors = HashSet::new();
    let mut indexes = HashSet::new();
    for site in actor_sites {
        if site.source_function != function.name || site.source_arity != function.arity {
            return Err(adapter_error(
                "actor-site-owner",
                format!(
                    "site `{}:{}` does not belong to `{} {}/{}`",
                    site.block_label,
                    site.result_name,
                    function.name,
                    function.name,
                    function.arity
                ),
            ));
        }
        if !anchors.insert((site.block_label.clone(), site.result_name.clone())) {
            return Err(adapter_error(
                "actor-site",
                format!(
                    "function `{}/{}` repeats suspension anchor `{}:{}`",
                    function.name, function.arity, site.block_label, site.result_name
                ),
            ));
        }
        if !indexes.insert(site.native_call_index) {
            return Err(adapter_error(
                "actor-call-index",
                format!(
                    "function `{}/{}` repeats native call index {}",
                    function.name, function.arity, site.native_call_index
                ),
            ));
        }
        let Some((block, instruction)) = calls.get(site.native_call_index) else {
            return Err(adapter_error(
                "actor-call-index",
                format!(
                    "function `{}/{}` has no native call at index {}",
                    function.name, function.arity, site.native_call_index
                ),
            ));
        };
        if *block != site.block_label
            || instruction.result.as_deref() != Some(site.result_name.as_str())
        {
            return Err(adapter_error(
                "actor-call-anchor",
                format!(
                    "native call index {} resolves to `{}:{}` instead of `{}:{}`",
                    site.native_call_index,
                    block,
                    instruction.result.as_deref().unwrap_or("<missing>"),
                    site.block_label,
                    site.result_name
                ),
            ));
        }
        let owner_block = function
            .blocks
            .iter()
            .find(|block| block.label == site.block_label)
            .ok_or_else(|| {
                adapter_error(
                    "actor-call-anchor",
                    format!("suspension block `{}` does not exist", site.block_label),
                )
            })?;
        let is_last = owner_block
            .instructions
            .last()
            .and_then(|instruction| instruction.result.as_deref())
            == Some(site.result_name.as_str());
        if !is_last || owner_block.terminator.opcode != "unreachable" {
            return Err(adapter_error(
                "actor-continuation-split",
                format!(
                    "suspending call `{}:{}` must end an unreachable split block",
                    site.block_label, site.result_name
                ),
            ));
        }
    }
    Ok(())
}

fn lower_function(
    module_name: &str,
    function: &IrFunction,
    functions: &FunctionCatalog,
    atoms: &mut Vec<String>,
) -> Result<NativeFunction, String> {
    lower_function_with_actor_sites(module_name, function, functions, &[], atoms)
}

/// Converts one admitted ABI-1 module into the existing Cranelift-facing NativeIR.
///
/// Every unsupported representation or control-flow feature is rejected rather
/// than reconstructed through the Rust source frontend.
fn validate_envelope(envelope: &BackendEnvelope) -> Result<(), String> {
    if envelope.abi_major != 1 || envelope.abi_minor > ABI_MINOR {
        return Err(adapter_error(
            "abi",
            format!(
                "expected ABI 1.0 through 1.{ABI_MINOR}, found {}.{}",
                envelope.abi_major, envelope.abi_minor
            ),
        ));
    }
    if envelope.abi_minor < 2 && !envelope.module.structs.is_empty() {
        return Err(adapter_error(
            "layout",
            "struct layouts require managed descriptor adaptation",
        ));
    }
    if envelope.module.name.is_empty() || envelope.module.name.len() > MAX_IDENTITY_BYTES {
        return Err(adapter_error(
            "resource",
            "module identity exceeds its admission bound",
        ));
    }
    if envelope.module.functions.len() > MAX_MODULE_FUNCTIONS {
        return Err(adapter_error(
            "resource",
            format!(
                "module `{}` exceeds the {} function bound",
                envelope.module.name, MAX_MODULE_FUNCTIONS
            ),
        ));
    }
    if envelope.module.functions.is_empty() {
        return Err(adapter_error("module", "native module has no functions"));
    }
    for function in &envelope.module.functions {
        if function.capture_count > function.parameters.len() {
            return Err(adapter_error(
                "closure_captures",
                format!(
                    "function `{}` capture count exceeds its parameter count",
                    function.name
                ),
            ));
        }
        if envelope.abi_minor < 4 && function.capture_count != 0 {
            return Err(adapter_error(
                "closure_captures",
                "capture metadata requires ABI 1.4",
            ));
        }
        if function.name.is_empty() || function.name.len() > MAX_IDENTITY_BYTES {
            return Err(adapter_error(
                "resource",
                format!(
                    "module `{}` contains an invalid function identity",
                    envelope.module.name
                ),
            ));
        }
        if function.blocks.len() > MAX_FUNCTION_BLOCKS {
            return Err(adapter_error(
                "resource",
                format!("function `{}` exceeds the block bound", function.name),
            ));
        }
        let instruction_count = function
            .blocks
            .iter()
            .map(|block| block.instructions.len())
            .try_fold(0usize, usize::checked_add)
            .ok_or_else(|| adapter_error("resource", "instruction count overflow"))?;
        if instruction_count > MAX_FUNCTION_INSTRUCTIONS {
            return Err(adapter_error(
                "resource",
                format!("function `{}` exceeds the instruction bound", function.name),
            ));
        }
        if function.blocks.iter().any(|block| {
            block
                .instructions
                .iter()
                .any(|instruction| instruction.operands.len() > MAX_INSTRUCTION_OPERANDS)
                || block.terminator.operands.len() > MAX_INSTRUCTION_OPERANDS
                || block.terminator.targets.len() > MAX_INSTRUCTION_OPERANDS
        }) {
            return Err(adapter_error(
                "resource",
                format!("function `{}` exceeds the operand bound", function.name),
            ));
        }
    }
    Ok(())
}

fn application_catalog(envelopes: &[BackendEnvelope]) -> Result<FunctionCatalog, String> {
    if envelopes.len() > MAX_APPLICATION_MODULES {
        return Err(adapter_error(
            "resource",
            format!("application exceeds the {MAX_APPLICATION_MODULES} module bound"),
        ));
    }
    let function_count = envelopes
        .iter()
        .map(|envelope| envelope.module.functions.len())
        .try_fold(0usize, usize::checked_add)
        .ok_or_else(|| adapter_error("resource", "application function count overflow"))?;
    if function_count > MAX_APPLICATION_FUNCTIONS {
        return Err(adapter_error(
            "resource",
            format!("application exceeds the {MAX_APPLICATION_FUNCTIONS} function bound"),
        ));
    }
    let mut catalog = FunctionCatalog::new();
    let mut modules = HashSet::new();
    let mut index = 0;
    for envelope in envelopes {
        validate_envelope(envelope)?;
        if !modules.insert(envelope.module.name.clone()) {
            return Err(adapter_error(
                "module",
                format!("application repeats module `{}`", envelope.module.name),
            ));
        }
        for layout in &envelope.module.structs {
            let identity = if layout.name.contains('.') {
                layout.name.clone()
            } else {
                format!("{}.{}", envelope.module.name, layout.name)
            };
            let mut fields = HashMap::new();
            for (index, field) in layout.fields.iter().enumerate() {
                let IrStructField::Typed { name, type_name } = field else {
                    return Err(adapter_error(
                        "struct_layout",
                        format!("struct `{identity}` contains an untyped legacy field"),
                    ));
                };
                if fields
                    .insert(
                        name.clone(),
                        LocalStructField {
                            index,
                            type_name: type_name.clone(),
                        },
                    )
                    .is_some()
                {
                    return Err(adapter_error(
                        "struct_layout",
                        format!("struct `{identity}` repeats field `{name}`"),
                    ));
                }
            }
            if let Some(previous) = catalog.structs.insert(identity.clone(), fields.clone()) {
                if previous != fields {
                    return Err(adapter_error(
                        "struct_layout",
                        format!("application contains conflicting layout `{identity}`"),
                    ));
                }
            }
        }
        for function in &envelope.module.functions {
            let parameters = function
                .parameters
                .iter()
                .map(|parameter| native_type(&parameter.type_name))
                .collect::<Result<Vec<_>, _>>()?;
            let result = native_type(&function.return_type)?;
            let key = (
                envelope.module.name.clone(),
                function.name.clone(),
                function.arity,
            );
            if catalog
                .insert(
                    key,
                    LocalFunction {
                        index,
                        capture_count: function.capture_count,
                        parameters,
                        result,
                    },
                )
                .is_some()
            {
                return Err(adapter_error(
                    "function",
                    format!(
                        "module `{}` repeats function `{}/{}`",
                        envelope.module.name, function.name, function.arity
                    ),
                ));
            }
            index += 1;
        }
    }
    Ok(catalog)
}

fn lower_module(
    envelope: &BackendEnvelope,
    identities: &FunctionCatalog,
) -> Result<NativeModule, String> {
    let mut atoms = Vec::new();
    let functions = envelope
        .module
        .functions
        .iter()
        .map(|function| lower_function(&envelope.module.name, function, identities, &mut atoms))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(NativeModule {
        name: envelope.module.name.clone(),
        functions,
        continuations: Vec::new(),
        managed_layouts: super::self_host_managed_layouts::encoded_struct_layouts(
            std::slice::from_ref(envelope),
        )?,
        managed_collections: super::self_host_managed_layouts::encoded_collection_layouts(
            std::slice::from_ref(envelope),
        )?,
        atoms,
    })
}

fn lower_actor_continuation_body(
    envelope: &BackendEnvelope,
    identities: &FunctionCatalog,
    continuation: &crate::self_host_actor_ir::RuntimeActorContinuation,
    actor_sites: &[super::self_host_actor_adapter::NativeActorSuspensionSite],
    atoms: &mut Vec<String>,
) -> Result<NativeExpr, String> {
    let function = envelope
        .module
        .functions
        .iter()
        .find(|function| {
            function.name == continuation.source_function
                && function.arity == continuation.source_arity
        })
        .ok_or_else(|| {
            adapter_error(
                "actor-continuation-owner",
                format!(
                    "continuation {} references absent function `{}.{}/{}`",
                    continuation.id,
                    envelope.module.name,
                    continuation.source_function,
                    continuation.source_arity
                ),
            )
        })?;
    let sites = actor_sites
        .iter()
        .filter(|site| {
            site.source_function == continuation.source_function
                && site.source_arity == continuation.source_arity
        })
        .cloned()
        .collect::<Vec<_>>();
    validate_actor_sites(function, &sites)?;
    let parameter_types = continuation
        .parameter_types
        .iter()
        .map(|type_name| native_type(type_name))
        .collect::<Result<Vec<_>, _>>()?;
    let values = continuation
        .parameter_names
        .iter()
        .zip(parameter_types.iter().copied())
        .enumerate()
        .map(|(index, (name, value_type))| (name.clone(), (index, value_type)))
        .collect::<ValueCatalog>();
    let return_type = native_type(&continuation.return_type)?;
    let mut expansions = 0;
    lower_block_path(
        &continuation.entry_block,
        None,
        &function.blocks,
        &envelope.module.name,
        values,
        identities,
        &sites,
        return_type,
        atoms,
        &HashSet::new(),
        &mut expansions,
    )?
    .ok_or_else(|| {
        adapter_error(
            "actor-continuation-body",
            format!(
                "continuation {} entry block `{}` has no returning path",
                continuation.id, continuation.entry_block
            ),
        )
    })
}

fn lower_self_hosted_actor_envelope_with_catalog(
    envelope: &BackendEnvelope,
    plan: crate::self_host_actor_ir::RuntimeActorPlan,
    identities: &FunctionCatalog,
) -> Result<NativeModule, String> {
    let mut module = lower_module(envelope, identities)?;
    let actor = super::self_host_actor_adapter::materialize_actor_plan(
        plan,
        native_type,
        |continuation, sites| {
            lower_actor_continuation_body(
                envelope,
                identities,
                continuation,
                sites,
                &mut module.atoms,
            )
        },
    )?;
    module.functions = envelope
        .module
        .functions
        .iter()
        .map(|function| {
            let sites = actor
                .suspension_sites
                .iter()
                .filter(|site| {
                    site.source_function == function.name && site.source_arity == function.arity
                })
                .cloned()
                .collect::<Vec<_>>();
            lower_function_with_actor_sites(
                &envelope.module.name,
                function,
                identities,
                &sites,
                &mut module.atoms,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    module.continuations = actor.continuations;
    Ok(module)
}

fn lower_self_hosted_actor_envelope_with_variants(
    envelope: &BackendEnvelope,
    plan: crate::self_host_actor_ir::RuntimeActorPlan,
    variants: Option<crate::self_host_variant_ir::VariantCatalog>,
) -> Result<NativeModule, String> {
    let mut identities = application_catalog(std::slice::from_ref(envelope))?;
    identities.variants = variants;
    lower_self_hosted_actor_envelope_with_catalog(envelope, plan, &identities)
}

pub(crate) fn lower_self_hosted_actor_envelope(
    envelope: &BackendEnvelope,
    plan: crate::self_host_actor_ir::RuntimeActorPlan,
) -> Result<NativeModule, String> {
    lower_self_hosted_actor_envelope_with_variants(envelope, plan, None)
}

pub(crate) fn lower_self_hosted_typed_actor_envelope(
    envelope: &BackendEnvelope,
    plan: crate::self_host_actor_ir::RuntimeActorPlan,
    variants: crate::self_host_variant_ir::VariantCatalog,
) -> Result<NativeModule, String> {
    lower_self_hosted_actor_envelope_with_variants(envelope, plan, Some(variants))
}

/// Converts an ordered ABI-1 module closure into one globally indexed NativeIR application.
pub(crate) fn lower_self_hosted_application(
    envelopes: &[BackendEnvelope],
) -> Result<Vec<NativeModule>, String> {
    if envelopes.is_empty() {
        return Err(adapter_error(
            "application",
            "application has no ABI modules",
        ));
    }
    let identities = application_catalog(envelopes)?;
    envelopes
        .iter()
        .map(|envelope| lower_module(envelope, &identities))
        .collect()
}

/// Converts a typed actor-aware module closure using one shared variant and call catalog.
pub(crate) fn lower_self_hosted_typed_actor_application(
    envelopes: &[BackendEnvelope],
    plans: Vec<crate::self_host_actor_ir::RuntimeActorPlan>,
    variants: crate::self_host_variant_ir::VariantCatalog,
) -> Result<Vec<NativeModule>, String> {
    if envelopes.is_empty() {
        return Err(adapter_error(
            "typed-actor-application",
            "application has no ABI modules",
        ));
    }
    if envelopes.len() != plans.len() {
        return Err(adapter_error(
            "typed-actor-application",
            format!(
                "application has {} ABI modules but {} actor plans",
                envelopes.len(),
                plans.len()
            ),
        ));
    }
    let mut identities = application_catalog(envelopes)?;
    identities.variants = Some(variants);
    envelopes
        .iter()
        .zip(plans)
        .map(|(envelope, plan)| {
            lower_self_hosted_actor_envelope_with_catalog(envelope, plan, &identities)
        })
        .collect()
}

/// Converts one admitted ABI-1 module into the existing Cranelift-facing NativeIR.
pub(crate) fn lower_self_hosted_envelope(
    envelope: &BackendEnvelope,
) -> Result<NativeModule, String> {
    lower_self_hosted_application(std::slice::from_ref(envelope))
        .map(|mut modules| modules.remove(0))
}

#[cfg(test)]
#[path = "self_host_adapter_inline_test.rs"]
mod tests;
