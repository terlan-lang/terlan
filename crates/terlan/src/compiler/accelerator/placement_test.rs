//! Tests for conservative whole-program accelerator placement.

use std::path::Path;

use super::*;
use crate::compiler::accelerator::{
    AcceleratorDescriptor, AcceleratorPackageDescriptor, AcceleratorPlacementOperation,
};

/// Minimal backend-neutral package metadata used by planner tests.
const PACKAGE: &str = r#"
schema = 1
backend = "cuda"
device_classes = ["gpu"]
artifact_formats = ["ptx"]
dtypes = ["f32"]
layouts = ["row-major"]
address_spaces = ["host", "device"]
resource_classes = ["buffer"]
asynchronous_operations = ["execute"]
capabilities = ["accelerator.execute"]

[[targets]]
triple = "x86_64-unknown-linux-gnu"
availability = "supported"
artifact_formats = ["ptx"]
architectures = ["sm-30"]

[[operations]]
id = "buffer.add"
effects = ["execute"]
asynchronous = true

[[operations]]
id = "buffer.multiply"
effects = ["execute"]
asynchronous = true
"#;

/// Returns one resolved package closure.
fn packages() -> AcceleratorDependencyClosure {
    AcceleratorDependencyClosure::resolve(
        vec![AcceleratorPackageDescriptor {
            package: "terlan-cuda".to_string(),
            version: "0.0.9".to_string(),
            source: "fixtures/cuda/accelerator.toml".to_string(),
            descriptor: AcceleratorDescriptor::parse(
                PACKAGE,
                Path::new("fixtures/cuda/accelerator.toml"),
            )
            .expect("parse placement package"),
        }],
        Path::new("fixtures/application/terlan.toml"),
    )
    .expect("resolve placement package")
}

/// Returns one checked f32 matrix layout.
fn layout() -> AcceleratorTensorLayout {
    AcceleratorTensorLayout::new(
        AcceleratorScalarType::F32,
        &[2, 2],
        None,
        0,
        AcceleratorTensorOrder::RowMajor,
        4,
    )
    .expect("matrix layout")
}

/// Returns one canonical actor-owned value.
fn value(
    id: &str,
    alias_class: u32,
    initial_placement: AcceleratorPlacement,
) -> AcceleratorPlacementValue {
    AcceleratorPlacementValue {
        id: id.to_string(),
        layout: layout(),
        alias_class,
        initial_placement,
        lifetime: AcceleratorValueLifetime::Actor,
        actor: 1,
    }
}

/// Returns one operation with exact deterministic semantics.
fn operation(
    id: &str,
    semantic_operation: &str,
    inputs: &[&str],
    output: &str,
    placement: AcceleratorPlacement,
) -> AcceleratorPlacementOperation {
    AcceleratorPlacementOperation {
        id: id.to_string(),
        semantic_operation: semantic_operation.to_string(),
        package: (placement == AcceleratorPlacement::Device).then(|| "terlan-cuda".to_string()),
        inputs: inputs.iter().map(|value| (*value).to_string()).collect(),
        output: output.to_string(),
        effects: if placement == AcceleratorPlacement::Device {
            BTreeSet::from([AcceleratorEffect::Execute])
        } else {
            BTreeSet::new()
        },
        pure_elementwise: placement == AcceleratorPlacement::Device,
        numerical_policy: AcceleratorNumericalPolicy::IeeeStrict,
        deterministic: true,
        error_timing_observable: false,
        cleanup_observable: false,
        maintained_library_operation: None,
        constants: BTreeMap::new(),
        launch_dimensions: (placement == AcceleratorPlacement::Device).then_some(
            AcceleratorExecutionDimensions {
                grid: [1, 1, 1],
                block: [32, 1, 1],
            },
        ),
        placement,
        source: AcceleratorIrSource {
            file: "placement_fixture.terl".to_string(),
            line: 10,
            column: 5,
        },
    }
}

/// Returns one pipeline with two fusible device operations and one host consumer.
fn program() -> AcceleratorPlacementProgram {
    let mut add = operation(
        "add",
        "buffer.add",
        &["left", "right"],
        "sum",
        AcceleratorPlacement::Device,
    );
    add.maintained_library_operation = Some("buffer.add".to_string());
    AcceleratorPlacementProgram {
        application: "placement_fixture".to_string(),
        architecture: "sm-30".to_string(),
        values: vec![
            value("left", 1, AcceleratorPlacement::Host),
            value("right", 2, AcceleratorPlacement::Host),
            value("scale", 3, AcceleratorPlacement::Device),
            value("sum", 4, AcceleratorPlacement::Host),
            value("product", 5, AcceleratorPlacement::Host),
            value("host_result", 6, AcceleratorPlacement::Host),
        ],
        operations: vec![
            add,
            operation(
                "multiply",
                "buffer.multiply",
                &["sum", "scale"],
                "product",
                AcceleratorPlacement::Device,
            ),
            operation(
                "consume",
                "host.consume",
                &["product"],
                "host_result",
                AcceleratorPlacement::Host,
            ),
        ],
    }
}

#[test]
fn planner_fuses_compatible_regions_and_keeps_values_device_resident() {
    let plan = AcceleratorPlacementPlan::build(&program(), &packages()).expect("placement plan");

    assert_eq!(plan.schema, ACCELERATOR_PLACEMENT_SCHEMA);
    assert_eq!(plan.reference_transfer_count, 4);
    assert_eq!(plan.optimized_transfer_count, 3);
    assert_eq!(plan.synchronizations.len(), 1);
    assert_eq!(plan.regions.len(), 2);
    assert_eq!(plan.regions[0].operations, ["add", "multiply"]);
    assert!(plan.regions[0].fused);
    assert_eq!(
        plan.regions[0].maintained_library_operation.as_deref(),
        Some("terlan-cuda:buffer.add")
    );
    assert_eq!(plan.regions[0].specialization.shape, [2, 2]);
    assert!(!plan
        .transfers
        .iter()
        .any(|transfer| transfer.value == "sum"));
}

#[test]
fn planner_rejects_fusion_when_numerical_or_alias_semantics_change() {
    let mut numerical = program();
    numerical.operations[1].numerical_policy =
        AcceleratorNumericalPolicy::Approximate { max_ulp: 4 };
    let plan = AcceleratorPlacementPlan::build(&numerical, &packages()).unwrap();
    assert_eq!(plan.regions.len(), 3);
    assert!(plan
        .rejections
        .iter()
        .any(|rejection| rejection.reason_code == "numerical-policy"));

    let mut aliased = program();
    aliased
        .values
        .iter_mut()
        .find(|value| value.id == "product")
        .unwrap()
        .alias_class = 4;
    let plan = AcceleratorPlacementPlan::build(&aliased, &packages()).unwrap();
    assert!(plan
        .rejections
        .iter()
        .any(|rejection| rejection.reason_code == "aliasing"));
}

#[test]
fn planner_rejects_cross_actor_values_and_undeclared_library_operations() {
    let mut cross_actor = program();
    cross_actor
        .values
        .iter_mut()
        .find(|value| value.id == "right")
        .unwrap()
        .actor = 2;
    assert!(AcceleratorPlacementPlan::build(&cross_actor, &packages())
        .unwrap_err()
        .contains("placement-isolation"));

    let mut undeclared = program();
    undeclared.operations[0].maintained_library_operation = Some("buffer.divide".to_string());
    assert!(AcceleratorPlacementPlan::build(&undeclared, &packages())
        .unwrap_err()
        .contains("placement-library"));
}
