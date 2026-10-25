use super::*;
use crate::self_host_ir::{IrModule, IrStructLayout};

fn envelope(field: IrStructField) -> BackendEnvelope {
    BackendEnvelope {
        abi_major: 1,
        abi_minor: 2,
        target: "native".into(),
        module: IrModule {
            name: "sample.geometry".into(),
            structs: vec![IrStructLayout {
                name: "sample.geometry.Point".into(),
                fields: vec![field],
            }],
            functions: Vec::new(),
        },
    }
}

#[test]
fn typed_layout_lowers_to_decodable_managed_metadata() {
    let layouts = encoded_struct_layouts(&[envelope(IrStructField::Typed {
        name: "x".into(),
        type_name: "Int".into(),
    })])
    .expect("typed struct layout");

    let descriptor = crate::runtime::native_image::managed::decode_aggregate_layout(&layouts[0])
        .expect("decode struct layout");
    assert_eq!(descriptor.canonical_type(), "Named(sample.geometry.Point)");
    assert_eq!(descriptor.fields()[0].name(), Some("x"));
}

#[test]
fn legacy_layout_is_rejected_by_native_lowering() {
    let error = encoded_struct_layouts(&[envelope(IrStructField::Legacy("x".into()))])
        .expect_err("legacy layout must not reach current native lowering");

    assert!(error.contains("legacy_struct_layout"), "{error}");
}

#[test]
fn conflicting_application_layouts_are_rejected() {
    let left = envelope(IrStructField::Typed {
        name: "x".into(),
        type_name: "Int".into(),
    });
    let right = envelope(IrStructField::Typed {
        name: "x".into(),
        type_name: "Float".into(),
    });

    let error = encoded_struct_layouts(&[left, right])
        .expect_err("conflicting semantic layouts must not be admitted");

    assert!(error.contains("conflicting_struct_layout"), "{error}");
}

#[test]
fn nested_collection_types_emit_each_required_schema() {
    let mut source = envelope(IrStructField::Typed {
        name: "index".into(),
        type_name: "Map[String, List[Int]]".into(),
    });
    source.module.structs.clear();
    source
        .module
        .functions
        .push(crate::self_host_ir::IrFunction {
            module_name: "sample.geometry".into(),
            name: "index".into(),
            arity: 1,
            capture_count: 0,
            receiver_mutable: false,
            receiver_command: false,
            parameters: vec![crate::self_host_ir::IrParameter {
                name: "value".into(),
                type_name: "Map[String, List[Int]]".into(),
            }],
            return_type: "Map[String, List[Int]]".into(),
            blocks: Vec::new(),
            native_symbol: Some("sample_index".into()),
        });

    let layouts = encoded_collection_layouts(&[source]).expect("collection layouts");
    let mut canonical = layouts
        .iter()
        .map(|layout| {
            crate::runtime::native_image::managed::decode_collection_layout(layout)
                .expect("decode collection")
                .canonical_type()
                .to_owned()
        })
        .collect::<Vec<_>>();
    canonical.sort();

    assert_eq!(
        canonical,
        vec![
            "Apply(List;Int)".to_string(),
            "Apply(Map;String,Apply(List;Int))".to_string(),
        ]
    );
}

#[test]
fn closed_managed_abi_types_map_to_native_references() {
    let list = native_type("List[Int]")
        .expect("closed list type")
        .expect("native list type");
    let nominal = native_type("sample.geometry.Point")
        .expect("qualified nominal type")
        .expect("native nominal type");

    assert!(matches!(list, NativeType::ManagedRef(_)));
    assert!(matches!(nominal, NativeType::ManagedRef(_)));
    assert_eq!(native_type("Point").expect("bare type syntax"), None);
    assert_eq!(
        native_type("List[Point]").expect("generic bare type syntax"),
        None
    );
}

#[test]
fn abi_list_constructors_lower_to_managed_operations() {
    let list = collection_expr(
        "put-list",
        "List[Int]",
        vec![NativeExpr::Int(1), NativeExpr::Int(2)],
    )
    .expect("list lowering")
    .expect("list operation");
    let cons = collection_expr(
        "put-list-cons",
        "List[Int]",
        vec![NativeExpr::Int(0), list.clone()],
    )
    .expect("list-cons lowering")
    .expect("list-cons operation");

    assert!(matches!(list, NativeExpr::ManagedOperation { .. }));
    assert!(matches!(cons, NativeExpr::ManagedOperation { .. }));
}

#[test]
fn abi_list_cons_requires_head_and_tail() {
    let error = collection_expr("put-list-cons", "List[Int]", vec![NativeExpr::Int(1)])
        .expect_err("invalid list-cons arity");

    assert!(error.contains("list_cons_arity"), "{error}");
}

#[test]
fn abi_tuple_constructor_lowers_to_managed_allocation() {
    let tuple = aggregate_expr(
        "put-tuple",
        "(Int, Int)",
        vec![NativeExpr::Int(1), NativeExpr::Int(2)],
    )
    .expect("tuple lowering")
    .expect("tuple expression");

    assert!(matches!(tuple, NativeExpr::Construct { fields, .. } if fields.len() == 2));
}

#[test]
fn tuple_types_are_admitted_to_native_image_metadata() {
    let mut source = envelope(IrStructField::Typed {
        name: "pair".into(),
        type_name: "(Int, String)".into(),
    });
    source.module.structs[0].name = "sample.geometry.Box".into();

    let layouts = encoded_struct_layouts(&[source]).expect("aggregate layouts");
    let canonical = layouts
        .iter()
        .map(|layout| {
            crate::runtime::native_image::managed::decode_aggregate_layout(layout)
                .expect("decode aggregate layout")
                .canonical_type()
                .to_owned()
        })
        .collect::<Vec<_>>();

    assert!(canonical.contains(&"Tuple(Int,String)".to_string()));
}

#[test]
fn typed_tuple_projection_selects_scalar_managed_operation() {
    let instruction = IrInstruction {
        opcode: "get-tuple-element".into(),
        result: Some("value".into()),
        operands: vec!["pair".into(), "1".into()],
        operand_types: vec![Some("(String, Int)".into()), Some("Int".into())],
        type_name: "Int".into(),
        continuation_ordinal: None,
        try_region: None,
    };

    let expression =
        managed_instruction_expr(&instruction, vec![NativeExpr::Param(0), NativeExpr::Int(1)])
            .expect("tuple projection")
            .expect("managed projection");

    assert!(
        matches!(expression, NativeExpr::ManagedOperation { args, .. } if args == vec![NativeExpr::Param(0)])
    );
}

#[test]
fn root_binding_pattern_uses_managed_environment_projections() {
    let prepare = IrInstruction {
        opcode: "prepare-pattern".into(),
        result: Some("environment".into()),
        operands: vec![
            "subject".into(),
            "binding-pattern".into(),
            "value".into(),
            "0".into(),
        ],
        operand_types: vec![Some("Int".into()), None, None, Some("Int".into())],
        type_name: "PatternMatch".into(),
        continuation_ordinal: None,
        try_region: None,
    };
    let (environment, environment_type) =
        prepare_pattern_expr(&prepare, NativeExpr::Param(0), NativeType::Int)
            .expect("prepare root binding");
    assert!(matches!(environment, NativeExpr::Construct { .. }));

    let matched = IrInstruction {
        opcode: "pattern-matched".into(),
        result: Some("matched".into()),
        operands: vec!["environment".into()],
        operand_types: vec![Some("PatternMatch".into())],
        type_name: "Bool".into(),
        continuation_ordinal: None,
        try_region: None,
    };
    let (_, matched_type) =
        pattern_environment_expr(&matched, NativeExpr::Param(1), environment_type)
            .expect("matched projection")
            .expect("matched expression");
    assert_eq!(matched_type, NativeType::Bool);

    let binding = IrInstruction {
        opcode: "get-pattern-binding".into(),
        result: Some("value".into()),
        operands: vec!["environment".into(), "0".into()],
        operand_types: vec![Some("PatternMatch".into()), Some("Int".into())],
        type_name: "Int".into(),
        continuation_ordinal: None,
        try_region: None,
    };
    let (_, binding_type) =
        pattern_environment_expr(&binding, NativeExpr::Param(1), environment_type)
            .expect("binding projection")
            .expect("binding expression");
    assert_eq!(binding_type, NativeType::Int);
}

#[test]
fn v2_list_pattern_defers_aggregate_binding_projection() {
    let operands = vec![
        "subject".into(),
        "pattern-v2".into(),
        "list-cons-pattern".into(),
        "|".into(),
        "2".into(),
        "binding-pattern".into(),
        "head".into(),
        "0".into(),
        "wildcard-pattern".into(),
        "_".into(),
        "0".into(),
    ];
    let prepare = IrInstruction {
        opcode: "prepare-pattern".into(),
        result: Some("environment".into()),
        operand_types: (0..operands.len())
            .map(|index| (index == 0).then(|| "List[app.Widget]".into()))
            .collect(),
        operands,
        type_name: "PatternMatch".into(),
        continuation_ordinal: None,
        try_region: None,
    };
    let source_type = native_type("List[app.Widget]")
        .expect("list source type")
        .expect("native list source type");
    let (environment, environment_type) =
        prepare_pattern_expr(&prepare, NativeExpr::Param(0), source_type)
            .expect("deferred aggregate pattern environment");
    let NativeExpr::Construct { fields, .. } = environment else {
        panic!("v2 pattern environment must be managed");
    };
    assert_eq!(fields.len(), 2, "matched flag plus original subject");

    let binding = IrInstruction {
        opcode: "get-pattern-binding".into(),
        result: Some("head".into()),
        operands: vec![
            "environment".into(),
            "0".into(),
            "List[app.Widget]".into(),
            "list-cons-pattern".into(),
            "|".into(),
            "2".into(),
            "binding-pattern".into(),
            "head".into(),
            "0".into(),
            "wildcard-pattern".into(),
            "_".into(),
            "0".into(),
        ],
        operand_types: Vec::new(),
        type_name: "app.Widget".into(),
        continuation_ordinal: None,
        try_region: None,
    };
    let (_, binding_type) =
        pattern_environment_expr(&binding, NativeExpr::Param(1), environment_type)
            .expect("deferred aggregate binding projection")
            .expect("binding expression");
    assert!(matches!(binding_type, NativeType::ManagedRef(_)));
}
