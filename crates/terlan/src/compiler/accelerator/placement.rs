//! Whole-program accelerator placement, fusion, and transfer planning.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::{
    AcceleratorDependencyClosure, AcceleratorEffect, AcceleratorError,
    AcceleratorExecutionDimensions, AcceleratorIrSource, AcceleratorResult, AcceleratorScalarType,
    AcceleratorTensorLayout, AcceleratorTensorOrder,
};

/// Stable explainable placement-plan schema.
pub const ACCELERATOR_PLACEMENT_SCHEMA: &str = "terlan.accelerator-placement.v1";

/// Host or accelerator residence selected for a value or operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AcceleratorPlacement {
    /// Native host execution and host-visible memory.
    Host,
    /// Accelerator execution and device-resident memory.
    Device,
}

/// Lifetime boundary that constrains device residence and borrowing.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AcceleratorValueLifetime {
    /// Value is consumed within one operation.
    Operation,
    /// Value remains actor-owned across suspension.
    Actor,
}

/// Floating-point and integer behavior required by an operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AcceleratorNumericalPolicy {
    /// Exact integer or Boolean semantics.
    Exact,
    /// Ordered IEEE behavior without reassociation or contraction.
    IeeeStrict,
    /// Explicit approximate behavior bounded by maximum ULP distance.
    Approximate { max_ulp: u32 },
}

/// Canonical value evidence used by placement and alias analysis.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceleratorPlacementValue {
    /// Stable SSA-like value identity.
    pub id: String,
    /// Canonical checked tensor layout.
    pub layout: AcceleratorTensorLayout,
    /// Nonzero alias class; equal classes may alias.
    pub alias_class: u32,
    /// Initial value residence before the first operation.
    pub initial_placement: AcceleratorPlacement,
    /// Borrow and residence lifetime.
    pub lifetime: AcceleratorValueLifetime,
    /// Actor owner used to preserve isolation.
    pub actor: u64,
}

/// One explicit package or compiler-generated operation in program order.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceleratorPlacementOperation {
    /// Stable operation identity.
    pub id: String,
    /// Package semantic operation ID or generated-kernel identity.
    pub semantic_operation: String,
    /// Package owning the reference operation, when any.
    pub package: Option<String>,
    /// Ordered input value identities.
    pub inputs: Vec<String>,
    /// Output value identity.
    pub output: String,
    /// Package-declared effects.
    pub effects: BTreeSet<AcceleratorEffect>,
    /// Whether the operation is pure and elementwise.
    pub pure_elementwise: bool,
    /// Required observable numerical behavior.
    pub numerical_policy: AcceleratorNumericalPolicy,
    /// Whether operation results must be deterministic.
    pub deterministic: bool,
    /// Whether failure timing is observable and must not move.
    pub error_timing_observable: bool,
    /// Whether cleanup timing is observable and must not move.
    pub cleanup_observable: bool,
    /// Maintained library operation proven equivalent by package metadata.
    pub maintained_library_operation: Option<String>,
    /// Statically specialized scalar constants.
    pub constants: BTreeMap<String, String>,
    /// Statically selected launch bounds for generated device work.
    pub launch_dimensions: Option<AcceleratorExecutionDimensions>,
    /// Explicit preferred execution placement.
    pub placement: AcceleratorPlacement,
    /// Source location explaining the decision.
    pub source: AcceleratorIrSource,
}

/// Whole-program operation graph after CoreIR application analysis.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceleratorPlacementProgram {
    /// Source application identity.
    pub application: String,
    /// Target accelerator architecture.
    pub architecture: String,
    /// Canonical values keyed independently by stable IDs.
    pub values: Vec<AcceleratorPlacementValue>,
    /// Topologically ordered operations.
    pub operations: Vec<AcceleratorPlacementOperation>,
}

/// Concrete specialization selected for one device region.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceleratorPlacementSpecialization {
    /// Scalar dtype.
    pub dtype: AcceleratorScalarType,
    /// Static tensor rank.
    pub rank: usize,
    /// Static tensor dimensions.
    pub shape: Vec<u64>,
    /// Static tensor order.
    pub order: AcceleratorTensorOrder,
    /// Static byte alignment.
    pub alignment: u64,
    /// Selected architecture.
    pub architecture: String,
    /// Statically specialized scalar constants.
    pub constants: BTreeMap<String, String>,
    /// Statically selected launch bounds.
    pub launch_dimensions: Option<AcceleratorExecutionDimensions>,
}

/// One explicit host/device transfer retained by dependency analysis.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceleratorTransferDecision {
    /// Value being transferred.
    pub value: String,
    /// Source residence.
    pub from: AcceleratorPlacement,
    /// Destination residence.
    pub to: AcceleratorPlacement,
    /// Consumer requiring the transfer.
    pub before_operation: String,
    /// Explainable dependency reason.
    pub reason: String,
}

/// One explicit synchronization retained by host dependency analysis.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceleratorSynchronizationDecision {
    /// Device-produced value requiring completion.
    pub value: String,
    /// Host operation waiting for completion.
    pub before_operation: String,
    /// Explainable dependency reason.
    pub reason: String,
}

/// One fused or standalone executable placement region.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceleratorPlacementRegion {
    /// Stable deterministic region identity.
    pub id: String,
    /// Selected execution residence.
    pub placement: AcceleratorPlacement,
    /// Ordered source operations.
    pub operations: Vec<String>,
    /// Concrete static specialization.
    pub specialization: AcceleratorPlacementSpecialization,
    /// Selected maintained library operation, when preferable.
    pub maintained_library_operation: Option<String>,
    /// Whether more than one operation was fused.
    pub fused: bool,
    /// Explainable selection reason.
    pub reason: String,
    /// Source location of the first operation.
    pub source: AcceleratorIrSource,
}

/// One rejected optimization and its semantic reason.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceleratorOptimizationRejection {
    /// Earlier operation considered for fusion or elision.
    pub operation: String,
    /// Following operation considered with it.
    pub next_operation: Option<String>,
    /// Stable rejection class.
    pub reason_code: String,
    /// Human-readable semantic reason.
    pub reason: String,
}

/// Deterministic explainable whole-program placement output.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceleratorPlacementPlan {
    /// Stable plan schema.
    pub schema: &'static str,
    /// Source application identity.
    pub application: String,
    /// Selected target architecture.
    pub architecture: String,
    /// Ordered executable regions.
    pub regions: Vec<AcceleratorPlacementRegion>,
    /// Required host/device transfers after elision.
    pub transfers: Vec<AcceleratorTransferDecision>,
    /// Required host waits after elision.
    pub synchronizations: Vec<AcceleratorSynchronizationDecision>,
    /// Conservative optimization rejections.
    pub rejections: Vec<AcceleratorOptimizationRejection>,
    /// Transfers that would occur without residence analysis.
    pub reference_transfer_count: usize,
    /// Transfers retained after residence analysis.
    pub optimized_transfer_count: usize,
}

impl AcceleratorPlacementPlan {
    /// Plans one topologically ordered whole-program operation graph.
    pub fn build(
        program: &AcceleratorPlacementProgram,
        packages: &AcceleratorDependencyClosure,
    ) -> AcceleratorResult<Self> {
        validate_program(program)?;
        let values = program
            .values
            .iter()
            .map(|value| (value.id.as_str(), value))
            .collect::<BTreeMap<_, _>>();
        let mut residence = program
            .values
            .iter()
            .map(|value| (value.id.clone(), value.initial_placement))
            .collect::<BTreeMap<_, _>>();
        let mut transfers = Vec::new();
        let mut synchronizations = Vec::new();
        let mut reference_transfer_count = 0usize;
        for operation in &program.operations {
            for input in &operation.inputs {
                let current = residence[input];
                if operation.placement == AcceleratorPlacement::Device {
                    reference_transfer_count += 1;
                }
                if current != operation.placement {
                    transfers.push(AcceleratorTransferDecision {
                        value: input.clone(),
                        from: current,
                        to: operation.placement,
                        before_operation: operation.id.clone(),
                        reason: "consumer placement requires value residence".to_string(),
                    });
                    if current == AcceleratorPlacement::Device {
                        synchronizations.push(AcceleratorSynchronizationDecision {
                            value: input.clone(),
                            before_operation: operation.id.clone(),
                            reason: "host consumer depends on device completion".to_string(),
                        });
                    }
                    residence.insert(input.clone(), operation.placement);
                }
            }
            residence.insert(operation.output.clone(), operation.placement);
        }
        let (regions, rejections) = build_regions(program, packages, &values)?;
        Ok(Self {
            schema: ACCELERATOR_PLACEMENT_SCHEMA,
            application: program.application.clone(),
            architecture: program.architecture.clone(),
            regions,
            optimized_transfer_count: transfers.len(),
            transfers,
            synchronizations,
            rejections,
            reference_transfer_count,
        })
    }
}

/// Validates graph identity, static layouts, ownership, and topological order.
fn validate_program(program: &AcceleratorPlacementProgram) -> AcceleratorResult<()> {
    if program.application.trim().is_empty() || program.architecture.trim().is_empty() {
        return Err(
            "error[accelerator.placement-identity]: empty application or target"
                .to_string()
                .into(),
        );
    }
    let mut values = BTreeMap::new();
    for value in &program.values {
        value.layout.validate().map_err(|error| {
            format!(
                "error[accelerator.placement-layout]: {}: {error:?}",
                value.id
            )
        })?;
        if value.id.trim().is_empty() || value.alias_class == 0 || value.actor == 0 {
            return Err(format!(
                "error[accelerator.placement-value]: invalid value `{}`",
                value.id
            )
            .into());
        }
        if values.insert(value.id.as_str(), value).is_some() {
            return Err(format!(
                "error[accelerator.placement-value]: duplicate value `{}`",
                value.id
            )
            .into());
        }
    }
    let mut operations = BTreeSet::new();
    let mut producers = BTreeSet::new();
    let mut operation_lifetime_uses = BTreeMap::<&str, usize>::new();
    for operation in &program.operations {
        if !operations.insert(operation.id.as_str()) || operation.inputs.is_empty() {
            return Err(format!(
                "error[accelerator.placement-operation]: invalid operation `{}`",
                operation.id
            )
            .into());
        }
        let output = values.get(operation.output.as_str()).ok_or_else(|| {
            format!(
                "error[accelerator.placement-output]: `{}`",
                operation.output
            )
        })?;
        producers.insert(operation.output.as_str());
        if let Some(dimensions) = operation.launch_dimensions {
            dimensions
                .validate()
                .map_err(|error| format!("error[accelerator.placement-launch]: {error}"))?;
        }
        for input in &operation.inputs {
            let input_value = values
                .get(input.as_str())
                .ok_or_else(|| format!("error[accelerator.placement-input]: `{input}`"))?;
            if input_value.actor != output.actor {
                return Err(format!(
                    "error[accelerator.placement-isolation]: `{input}` crosses actor ownership"
                )
                .into());
            }
            if input_value.lifetime == AcceleratorValueLifetime::Operation {
                if producers.contains(input.as_str()) {
                    return Err(format!(
                        "error[accelerator.placement-lifetime]: operation-borrowed `{input}` escaped its producer"
                    ).into());
                }
                let uses = operation_lifetime_uses.entry(input.as_str()).or_default();
                *uses += 1;
                if *uses > 1 {
                    return Err(format!(
                        "error[accelerator.placement-lifetime]: operation-borrowed `{input}` was reused"
                    ).into());
                }
            }
        }
    }
    Ok(())
}

/// Builds executable regions and conservative fusion rejections.
fn build_regions(
    program: &AcceleratorPlacementProgram,
    packages: &AcceleratorDependencyClosure,
    values: &BTreeMap<&str, &AcceleratorPlacementValue>,
) -> AcceleratorResult<(
    Vec<AcceleratorPlacementRegion>,
    Vec<AcceleratorOptimizationRejection>,
)> {
    let mut regions: Vec<AcceleratorPlacementRegion> = Vec::new();
    let mut rejections = Vec::new();
    for operation in &program.operations {
        let output = values[operation.output.as_str()];
        let library = select_library(operation, packages)?;
        let specialization = specialization(output, operation, &program.architecture);
        let can_fuse = regions
            .last()
            .and_then(|region| region.operations.last())
            .and_then(|operation| {
                program
                    .operations
                    .iter()
                    .find(|value| value.id == *operation)
            })
            .map(|previous| fusion_rejection(previous, operation, values));
        if let (Some(region), Some(None)) = (regions.last_mut(), can_fuse.as_ref()) {
            region.operations.push(operation.id.clone());
            region.fused = true;
            region.reason = "compatible pure elementwise operations fused".to_string();
            continue;
        }
        if let Some(Some((code, reason))) = can_fuse {
            rejections.push(AcceleratorOptimizationRejection {
                operation: regions
                    .last()
                    .and_then(|region| region.operations.last())
                    .cloned()
                    .unwrap_or_default(),
                next_operation: Some(operation.id.clone()),
                reason_code: code,
                reason,
            });
        }
        regions.push(AcceleratorPlacementRegion {
            id: format!("region-{}", regions.len()),
            placement: operation.placement,
            operations: vec![operation.id.clone()],
            specialization,
            maintained_library_operation: library,
            fused: false,
            reason: if operation.placement == AcceleratorPlacement::Device {
                "explicit accelerator package operation".to_string()
            } else {
                "host operation retained".to_string()
            },
            source: operation.source.clone(),
        });
    }
    Ok((regions, rejections))
}

/// Returns a rejection reason or admits fusion with `None`.
fn fusion_rejection(
    previous: &AcceleratorPlacementOperation,
    next: &AcceleratorPlacementOperation,
    values: &BTreeMap<&str, &AcceleratorPlacementValue>,
) -> Option<(String, String)> {
    if previous.placement != AcceleratorPlacement::Device
        || next.placement != AcceleratorPlacement::Device
        || !previous.pure_elementwise
        || !next.pure_elementwise
    {
        return Some((
            "not-pure-elementwise".to_string(),
            "both operations must be pure device elementwise operations".to_string(),
        ));
    }
    if previous.output != next.inputs[0] {
        return Some((
            "dependency-order".to_string(),
            "next operation does not consume the preceding output first".to_string(),
        ));
    }
    let previous_output = values[previous.output.as_str()];
    let next_output = values[next.output.as_str()];
    if previous_output.layout != next_output.layout {
        return Some((
            "layout-mismatch".to_string(),
            "static dtype, shape, strides, or alignment differ".to_string(),
        ));
    }
    if previous_output.alias_class == next_output.alias_class {
        return Some((
            "aliasing".to_string(),
            "intermediate and output may alias".to_string(),
        ));
    }
    if previous.numerical_policy != next.numerical_policy
        || previous.deterministic != next.deterministic
    {
        return Some((
            "numerical-policy".to_string(),
            "fusion could alter ordering, rounding, or determinism".to_string(),
        ));
    }
    if previous.error_timing_observable
        || next.error_timing_observable
        || previous.cleanup_observable
        || next.cleanup_observable
    {
        return Some((
            "observable-effects".to_string(),
            "failure or cleanup timing is observable".to_string(),
        ));
    }
    let allowed = BTreeSet::from([AcceleratorEffect::Execute]);
    if !previous.effects.is_subset(&allowed) || !next.effects.is_subset(&allowed) {
        return Some((
            "effect-contract".to_string(),
            "package effects exceed pure execution".to_string(),
        ));
    }
    None
}

/// Selects a maintained package operation only when metadata contains the semantic ID.
fn select_library(
    operation: &AcceleratorPlacementOperation,
    packages: &AcceleratorDependencyClosure,
) -> AcceleratorResult<Option<String>> {
    let Some(library) = &operation.maintained_library_operation else {
        return Ok(None);
    };
    let package_name = operation.package.as_deref().ok_or_else(|| {
        AcceleratorError::message(
            "select accelerator library",
            format!(
                "error[accelerator.placement-library]: `{}` has no package owner",
                operation.id
            ),
        )
    })?;
    let package = packages
        .packages
        .iter()
        .find(|package| package.package == package_name)
        .ok_or_else(|| {
            AcceleratorError::message(
                "select accelerator library",
                format!("error[accelerator.placement-library]: unknown package `{package_name}`"),
            )
        })?;
    if !package
        .descriptor
        .operations
        .iter()
        .any(|candidate| candidate.id == *library)
    {
        return Err(format!(
            "error[accelerator.placement-library]: package `{package_name}` does not declare `{library}`"
        ).into());
    }
    Ok(Some(format!("{package_name}:{library}")))
}

/// Builds one static specialization from canonical output evidence.
fn specialization(
    value: &AcceleratorPlacementValue,
    operation: &AcceleratorPlacementOperation,
    architecture: &str,
) -> AcceleratorPlacementSpecialization {
    AcceleratorPlacementSpecialization {
        dtype: value.layout.dtype,
        rank: value.layout.rank(),
        shape: value.layout.dimensions.clone(),
        order: value.layout.order,
        alignment: value.layout.alignment,
        architecture: architecture.to_string(),
        constants: operation.constants.clone(),
        launch_dimensions: operation.launch_dimensions,
    }
}

#[cfg(test)]
#[path = "placement_test.rs"]
mod placement_test;
