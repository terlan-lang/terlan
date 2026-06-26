use super::*;
use terlan_hir::resolve_syntax_module_output;
use terlan_syntax::parse_module_as_syntax_output;

#[test]
fn syntax_output_lowering_to_core_marks_remote_call_proof_model_required() {
    let module = parse_module_as_syntax_output(
        "\
module core_remote_call_boundary.\n\
\n\
pub call_remote(): Int ->\n\
    math:inc(1).\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "call_remote")
        .expect("core call_remote function");
    assert_eq!(function.clauses.len(), 1);
    assert_eq!(
        function.clauses[0].body.core_expr,
        Some(CoreExpr::RemoteCall {
            module: "math".to_string(),
            function: "inc".to_string(),
            args: vec![CoreExpr::Int(1)],
        })
    );
    assert_eq!(
        function.clauses[0].body.proof_coverage,
        CoreProofCoverage::ProofModelRequired
    );
    assert_eq!(
        core.metadata.proof_readiness,
        CoreProofReadiness::ProofModelRequired
    );
    assert_eq!(core.metadata.proof_model_required_expr_count, 1);
    assert!(core.metadata.lean_covered_expr_count >= 1);
    assert!(core.metadata.checked_preservation_expr_count >= 1);
    assert!(core.metadata.checked_preservation_expr_count >= core.metadata.lean_covered_expr_count);
    assert_eq!(core.metadata.typed_core_pattern_count, 0);
    assert_eq!(core.metadata.summary_only_pattern_count, 0);
    assert_eq!(core.metadata.checked_preservation_pattern_count, 0);
    assert!(
            core.contract_text().contains(
                "Call:core=RemoteCall(math:inc;Int(1)):preservation=structural-core-expr(freshness=no-runtime-bindings;target=RemoteCall(math:inc;Int(1))):proof=proof-model-required:remote=math"
            ),
            "contract text: {}",
            core.contract_text()
        );
}

#[test]
fn syntax_output_lowering_to_core_rejects_remote_fun_ref_source_syntax() {
    let parsed = parse_module_as_syntax_output(
        "\
module core_remote_fun_ref_boundary.\n\
\n\
pub reference(): Dynamic ->\n\
    fun erlang:abs/1.\n",
    );

    assert!(
        parsed.is_err(),
        "remote fun references are backend output syntax, not canonical Terlan source"
    );
}

#[test]
fn syntax_output_lowering_to_core_float_literal() {
    let module = parse_module_as_syntax_output(
        "\
module core_float_literal.\n\
\n\
pub value(): Float ->\n\
    1.5.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "value")
        .expect("core value function");
    assert_eq!(function.clauses.len(), 1);
    assert_eq!(
        function.clauses[0].body.core_expr,
        Some(CoreExpr::Float("1.5".to_string()))
    );
    assert_eq!(
        function.clauses[0].body.proof_coverage,
        CoreProofCoverage::ProofModelRequired
    );
    assert!(
        core.contract_text().contains("Float(1.5)"),
        "contract text: {}",
        core.contract_text()
    );
}

#[test]
fn syntax_output_lowering_to_core_binary_literal() {
    let module = parse_module_as_syntax_output(
        "\
module core_binary_literal.\n\
\n\
pub value(): Binary ->\n\
    \"hello\".\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "value")
        .expect("core value function");
    assert_eq!(function.clauses.len(), 1);
    let body = &function.clauses[0].body;
    let Some(CoreExpr::Binary(value)) = &body.core_expr else {
        panic!(
            "expected typed binary literal core expr: {:?}",
            body.core_expr
        );
    };
    assert!(
        value.contains("hello"),
        "binary literal should preserve source text: {value}"
    );
    assert_eq!(body.proof_coverage, CoreProofCoverage::LeanCovered);
    assert!(
        core.contract_text().contains("Binary("),
        "contract text: {}",
        core.contract_text()
    );
}

/// Verifies cast expressions are preserved as typed CoreIR boundaries.
///
/// Inputs:
/// - A module with an assignment-compatible `as` expression.
///
/// Output:
/// - Test passes when CoreIR contains `CoreExpr::Cast` with the lowered child
///   expression and typed target payload.
///
/// Transformation:
/// - Parses source through syntax output, lowers it to CoreIR, and checks that
///   the conversion boundary remains visible without selecting backend
///   coercion semantics.
#[test]
fn syntax_output_lowering_to_core_cast_boundary() {
    let module = parse_module_as_syntax_output(
        "\
module core_cast_boundary.\n\
\n\
pub value(): Int ->\n\
    1 as Int.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "value")
        .expect("core value function");
    assert_eq!(function.clauses.len(), 1);
    assert_eq!(
        function.clauses[0].body.core_expr,
        Some(CoreExpr::Cast {
            expr: Box::new(CoreExpr::Int(1)),
            target_type: CoreType::Int,
        })
    );
    assert_eq!(
        function.clauses[0].body.proof_coverage,
        CoreProofCoverage::ProofModelRequired
    );
    assert!(
        core.contract_text().contains("Cast(Int(1) as Int)"),
        "contract text: {}",
        core.contract_text()
    );
}

#[test]
fn syntax_output_lowering_to_core_binary_op() {
    let module = parse_module_as_syntax_output(
        "\
module core_binary_op_boundary.\n\
\n\
pub add(): Int ->\n\
    1 + 2.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "add")
        .expect("core add function");
    assert_eq!(function.clauses.len(), 1);
    assert_eq!(
        function.clauses[0].body.core_expr,
        Some(CoreExpr::BinaryOp {
            operator: "+".to_string(),
            left: Box::new(CoreExpr::Int(1)),
            right: Box::new(CoreExpr::Int(2)),
        })
    );
    assert_eq!(
        function.clauses[0].body.proof_coverage,
        CoreProofCoverage::LeanCovered
    );
    assert!(
        core.contract_text().contains("BinaryOp(+;Int(1), Int(2))"),
        "contract text: {}",
        core.contract_text()
    );
}

#[test]
fn syntax_output_lowering_to_core_unary_op() {
    let module = parse_module_as_syntax_output(
        "\
module core_unary_op_boundary.\n\
\n\
pub negate(value: Int): Int ->\n\
    -value.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "negate")
        .expect("core negate function");
    assert_eq!(function.clauses.len(), 1);
    assert_eq!(
        function.clauses[0].body.core_expr,
        Some(CoreExpr::UnaryOp {
            operator: "-".to_string(),
            operand: Box::new(CoreExpr::Var("value".to_string())),
        })
    );
    assert_eq!(
        function.clauses[0].body.proof_coverage,
        CoreProofCoverage::LeanCovered
    );
    assert!(
        core.contract_text().contains("UnaryOp(-;Var(value))"),
        "contract text: {}",
        core.contract_text()
    );
}

#[test]
fn syntax_output_lowering_to_core_map_expr() {
    let module = parse_module_as_syntax_output(
        "\
module core_map_expr_boundary.\n\
\n\
pub props(): Map ->\n\
    #{a := 1, b => 2}.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "props")
        .expect("core props function");
    assert_eq!(function.clauses.len(), 1);
    assert_eq!(
        function.clauses[0].body.core_expr,
        Some(CoreExpr::Map(vec![
            CoreMapExprField {
                key: "a".to_string(),
                required: true,
                value: CoreExpr::Int(1),
            },
            CoreMapExprField {
                key: "b".to_string(),
                required: false,
                value: CoreExpr::Int(2),
            },
        ]))
    );
    assert_eq!(
        function.clauses[0].body.proof_coverage,
        CoreProofCoverage::ProofModelRequired
    );
    assert!(
            core.contract_text()
                .contains("Map:core=Map(a:=Int(1),b=>Int(2)):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Map(a:=Int(1),b=>Int(2))):proof=proof-model-required"),
            "contract text: {}",
            core.contract_text()
        );
}

#[test]
fn syntax_output_lowering_to_core_list_cons_expr() {
    let module = parse_module_as_syntax_output(
        "\
module core_list_cons_expr_boundary.\n\
\n\
pub prepend(head: Int, tail: List[Int]): List[Int] ->\n\
    [head | tail].\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "prepend")
        .expect("core prepend function");
    assert_eq!(function.clauses.len(), 1);
    assert_eq!(
        function.clauses[0].body.core_expr,
        Some(CoreExpr::ListCons {
            head: Box::new(CoreExpr::Var("head".to_string())),
            tail: Box::new(CoreExpr::Var("tail".to_string())),
        })
    );
    assert_eq!(
        function.clauses[0].body.proof_coverage,
        CoreProofCoverage::LeanCovered
    );
    assert!(
        core.contract_text()
            .contains("ListCons(Var(head)|Var(tail))"),
        "contract text: {}",
        core.contract_text()
    );
}

#[test]
fn syntax_output_lowering_to_core_index_expr_uses_index_get_call() {
    let module = parse_module_as_syntax_output(
        "\
module core_index_expr_boundary.\n\
\n\
pub first(values: List[Int]): Dynamic ->\n\
    values[0].\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "first")
        .expect("core first function");
    assert_eq!(function.clauses.len(), 1);
    assert_eq!(
        function.clauses[0].body.core_expr,
        Some(CoreExpr::Call {
            function: "IndexGet.get_at".to_string(),
            args: vec![CoreExpr::Var("values".to_string()), CoreExpr::Int(0)],
        })
    );
    assert_eq!(
        function.clauses[0].body.proof_coverage,
        CoreProofCoverage::ProofModelRequired
    );
    assert!(
        core.contract_text()
            .contains("Call(IndexGet.get_at;Var(values),Int(0))"),
        "contract text: {}",
        core.contract_text()
    );
}

#[test]
fn syntax_output_lowering_to_core_index_assignment_uses_index_set_call() {
    let module = parse_module_as_syntax_output(
        "\
module core_index_assignment_boundary.\n\
\n\
pub update(values: List[Int]): Dynamic ->\n\
    values[0] = 1.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "update")
        .expect("core update function");
    assert_eq!(function.clauses.len(), 1);
    assert_eq!(
        function.clauses[0].body.core_expr,
        Some(CoreExpr::Call {
            function: "IndexSet.set_at".to_string(),
            args: vec![
                CoreExpr::Var("values".to_string()),
                CoreExpr::Int(0),
                CoreExpr::Int(1),
            ],
        })
    );
    assert_eq!(
        function.clauses[0].body.proof_coverage,
        CoreProofCoverage::ProofModelRequired
    );
    assert!(
        core.contract_text()
            .contains("Call(IndexSet.set_at;Var(values),Int(0),Int(1))"),
        "contract text: {}",
        core.contract_text()
    );
}

#[test]
fn syntax_output_lowering_to_core_fixed_array_expr() {
    let module = parse_module_as_syntax_output(
        "\
module core_fixed_array_expr_boundary.\n\
\n\
pub rgb(): FixedArray[3, Int] ->\n\
    #[1, 2, 3].\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "rgb")
        .expect("core rgb function");
    assert_eq!(function.clauses.len(), 1);
    assert_eq!(
        function.clauses[0].body.core_expr,
        Some(CoreExpr::FixedArray(vec![
            CoreExpr::Int(1),
            CoreExpr::Int(2),
            CoreExpr::Int(3),
        ]))
    );
    assert_eq!(
        function.clauses[0].body.proof_coverage,
        CoreProofCoverage::ProofModelRequired
    );
    assert!(
        core.contract_text()
            .contains("FixedArray(Int(1),Int(2),Int(3))"),
        "contract text: {}",
        core.contract_text()
    );
}

#[test]
fn syntax_output_lowering_to_core_list_comprehension_expr() {
    let module = parse_module_as_syntax_output(
        "\
module core_list_comprehension_expr_boundary.\n\
\n\
pub values(items: List[Int]): List[Int] ->\n\
    [value | value <- items].\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "values")
        .expect("core values function");
    assert_eq!(function.clauses.len(), 1);
    assert_eq!(
        function.clauses[0].body.core_expr,
        Some(CoreExpr::ListComprehension {
            expr: Box::new(CoreExpr::Var("value".to_string())),
            pattern: CorePattern::Var("value".to_string()),
            source: Box::new(CoreExpr::Var("items".to_string())),
            guard: None,
        })
    );
    assert_eq!(
        function.clauses[0].body.proof_coverage,
        CoreProofCoverage::ProofModelRequired
    );
    assert!(
        core.contract_text()
            .contains("ListComprehension(Var(value)|Var(value)<-Var(items))"),
        "contract text: {}",
        core.contract_text()
    );
}

#[test]
fn syntax_output_lowering_to_core_record_construct_expr() {
    let module = parse_module_as_syntax_output(
        "\
module core_record_construct_expr_boundary.\n\
\n\
pub make(): Dynamic ->\n\
    #Point { x = 1, y = 2 }.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "make")
        .expect("core make function");
    assert_eq!(function.clauses.len(), 1);
    assert_eq!(
        function.clauses[0].body.core_expr,
        Some(CoreExpr::RecordConstruct {
            name: "Point".to_string(),
            fields: vec![
                CoreRecordExprField {
                    key: "x".to_string(),
                    required: true,
                    value: CoreExpr::Int(1),
                },
                CoreRecordExprField {
                    key: "y".to_string(),
                    required: true,
                    value: CoreExpr::Int(2),
                },
            ],
        })
    );
    assert_eq!(
        function.clauses[0].body.proof_coverage,
        CoreProofCoverage::ProofModelRequired
    );
    assert!(
        core.contract_text()
            .contains("RecordConstruct(Point;x=Int(1),y=Int(2))"),
        "contract text: {}",
        core.contract_text()
    );
}

#[test]
fn syntax_output_lowering_to_core_field_access_expr() {
    let module = parse_module_as_syntax_output(
        "\
module core_field_access_expr_boundary.\n\
\n\
pub read(point: Point): Dynamic ->\n\
    point.x.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "read")
        .expect("core read function");
    assert_eq!(function.clauses.len(), 1);
    assert_eq!(
        function.clauses[0].body.core_expr,
        Some(CoreExpr::FieldAccess {
            base: Box::new(CoreExpr::Var("point".to_string())),
            field: "x".to_string(),
        })
    );
    assert_eq!(
        function.clauses[0].body.proof_coverage,
        CoreProofCoverage::LeanCovered
    );
    assert!(
        core.contract_text().contains("FieldAccess(Var(point).x)"),
        "contract text: {}",
        core.contract_text()
    );
}

#[test]
fn syntax_output_lowering_to_core_let_expr() {
    let module = parse_module_as_syntax_output(
        "\
module core_let_expr_boundary.\n\
\n\
pub with_body(x: Int): Int ->\n\
    let y = x + 1; z = y * 2; z + y.\n\
\n\
pub final_value(x: Int): Int ->\n\
    let y = x + 1; z = y * 2; z.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let with_body = core
        .functions
        .iter()
        .find(|function| function.name == "with_body")
        .expect("core with_body function");
    assert_eq!(with_body.clauses.len(), 1);
    assert_eq!(
        with_body.clauses[0].body.core_expr,
        Some(CoreExpr::Let {
            bindings: vec![
                CoreLetBinding {
                    pattern: CorePattern::Var("y".to_string()),
                    value: CoreExpr::BinaryOp {
                        operator: "+".to_string(),
                        left: Box::new(CoreExpr::Var("x".to_string())),
                        right: Box::new(CoreExpr::Int(1)),
                    },
                },
                CoreLetBinding {
                    pattern: CorePattern::Var("z".to_string()),
                    value: CoreExpr::BinaryOp {
                        operator: "*".to_string(),
                        left: Box::new(CoreExpr::Var("y".to_string())),
                        right: Box::new(CoreExpr::Int(2)),
                    },
                },
            ],
            body: Box::new(CoreExpr::BinaryOp {
                operator: "+".to_string(),
                left: Box::new(CoreExpr::Var("z".to_string())),
                right: Box::new(CoreExpr::Var("y".to_string())),
            }),
        })
    );
    assert_eq!(
        with_body.clauses[0].body.proof_coverage,
        CoreProofCoverage::ProofModelRequired
    );

    let final_value = core
        .functions
        .iter()
        .find(|function| function.name == "final_value")
        .expect("core final_value function");
    assert_eq!(
        final_value.clauses[0].body.core_expr,
        Some(CoreExpr::Let {
            bindings: vec![
                CoreLetBinding {
                    pattern: CorePattern::Var("y".to_string()),
                    value: CoreExpr::BinaryOp {
                        operator: "+".to_string(),
                        left: Box::new(CoreExpr::Var("x".to_string())),
                        right: Box::new(CoreExpr::Int(1)),
                    },
                },
                CoreLetBinding {
                    pattern: CorePattern::Var("z".to_string()),
                    value: CoreExpr::BinaryOp {
                        operator: "*".to_string(),
                        left: Box::new(CoreExpr::Var("y".to_string())),
                        right: Box::new(CoreExpr::Int(2)),
                    },
                },
            ],
            body: Box::new(CoreExpr::Var("z".to_string())),
        })
    );
    assert!(
        core.contract_text()
            .contains("Let(Var(y)=BinaryOp(+;Var(x), Int(1));Var(z)=BinaryOp(*;Var(y), Int(2));"),
        "contract text: {}",
        core.contract_text()
    );
}

#[test]
fn syntax_output_lowering_to_core_record_access_expr() {
    let module = parse_module_as_syntax_output(
        "\
module core_record_access_expr_boundary.\n\
\n\
pub read(point: Point): Dynamic ->\n\
    point#Point.x.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "read")
        .expect("core read function");
    assert_eq!(function.clauses.len(), 1);
    assert_eq!(
        function.clauses[0].body.core_expr,
        Some(CoreExpr::RecordAccess {
            base: Box::new(CoreExpr::Var("point".to_string())),
            name: "Point".to_string(),
            field: "x".to_string(),
        })
    );
    assert_eq!(
        function.clauses[0].body.proof_coverage,
        CoreProofCoverage::ProofModelRequired
    );
    assert!(
        core.contract_text()
            .contains("RecordAccess(Var(point)#Point.x)"),
        "contract text: {}",
        core.contract_text()
    );
}

#[test]
fn syntax_output_lowering_to_core_record_update_expr() {
    let module = parse_module_as_syntax_output(
        "\
module core_record_update_expr_boundary.\n\
\n\
pub update(point: Point): Dynamic ->\n\
    point#Point { x = 1, y = point.y }.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "update")
        .expect("core update function");
    assert_eq!(function.clauses.len(), 1);
    assert_eq!(
        function.clauses[0].body.core_expr,
        Some(CoreExpr::RecordUpdate {
            base: Box::new(CoreExpr::Var("point".to_string())),
            name: "Point".to_string(),
            fields: vec![
                CoreRecordExprField {
                    key: "x".to_string(),
                    required: true,
                    value: CoreExpr::Int(1),
                },
                CoreRecordExprField {
                    key: "y".to_string(),
                    required: true,
                    value: CoreExpr::FieldAccess {
                        base: Box::new(CoreExpr::Var("point".to_string())),
                        field: "y".to_string(),
                    },
                },
            ],
        })
    );
    assert_eq!(
        function.clauses[0].body.proof_coverage,
        CoreProofCoverage::ProofModelRequired
    );
    assert!(
        core.contract_text()
            .contains("RecordUpdate(Var(point)#Point;x=Int(1),y=FieldAccess(Var(point).y))"),
        "contract text: {}",
        core.contract_text()
    );
}

#[test]
fn syntax_output_lowering_to_core_template_instantiate_expr() {
    let module = parse_module_as_syntax_output(
        "\
module core_template_instantiate_expr_boundary.\n\
\n\
pub make(): Dynamic ->\n\
    UserCard{ name = \"Ada\" }.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "make")
        .expect("core make function");
    assert_eq!(function.clauses.len(), 1);
    let Some(CoreExpr::TemplateInstantiate { name, fields }) = &function.clauses[0].body.core_expr
    else {
        panic!(
            "expected template instantiation core expr: {:?}",
            function.clauses[0].body.core_expr
        );
    };
    assert_eq!(name, "UserCard");
    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].key, "name");
    assert!(matches!(fields[0].value, CoreExpr::Binary(_)));
    assert_eq!(
        function.clauses[0].body.proof_coverage,
        CoreProofCoverage::ProofModelRequired
    );
    assert!(
        core.contract_text()
            .contains("TemplateInstantiate(UserCard;name=Binary("),
        "contract text: {}",
        core.contract_text()
    );
}

/// Verifies generated template function calls lower to Core template nodes.
///
/// Inputs:
/// - Syntax output containing a template declaration and a direct named
///   template call.
///
/// Output:
/// - Test passes when the function body Core expression is
///   `CoreExpr::TemplateInstantiate`.
///
/// Transformation:
/// - Confirms CoreIR lowering uses template declaration context before treating
///   uppercase call names as constructor-call candidates.
#[test]
fn syntax_output_lowering_to_core_template_call_expr() {
    let module = parse_module_as_syntax_output(
        "\
module core_template_call_expr_boundary.\n\
template Page from \"./templates/page.terl.html\" {\n\
    title: Binary\n\
}.\n\
\n\
pub make(): Html[Dynamic] ->\n\
    Page(title = \"Ada\").\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "make")
        .expect("core make function");
    let Some(CoreExpr::TemplateInstantiate { name, fields }) = &function.clauses[0].body.core_expr
    else {
        panic!(
            "expected generated template call to lower to template instantiation: {:?}",
            function.clauses[0].body.core_expr
        );
    };
    assert_eq!(name, "Page");
    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].key, "title");
    assert!(matches!(fields[0].value, CoreExpr::Binary(_)));
}
