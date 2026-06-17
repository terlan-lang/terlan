use super::test_support::*;
use super::*;
use terlan_hir::resolve_syntax_module_output;
use terlan_syntax::{parse_module_as_syntax_output, SyntaxPatternFieldOutput};

#[test]
fn syntax_output_lowering_to_core_preserves_interface_contract() {
    let module = parse_module_as_syntax_output(
        "\
module core_boundary.\n\
pub value(): Int ->\n\
    1.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_resolved_module_to_core(&resolved);

    assert_eq!(core.schema, CORE_IR_SCHEMA);
    assert_eq!(core.module, "core_boundary");
    assert_eq!(core.source.source_kind, "resolved_module");
    assert_eq!(core.functions.len(), 1);
    assert_eq!(core.functions[0].name, "value");
    assert_eq!(core.functions[0].arity, 0);
    assert!(core.functions[0].public);
    assert_eq!(core.functions[0].return_type, "Int");
    assert_eq!(core.functions[0].core_return_type, Some(CoreType::Int));
    assert!(core.exports.iter().any(|export| {
        export.name == "value" && matches!(export.kind, CoreExportKind::Function { arity: 0 })
    }));
    assert_eq!(core.metadata.interface_function_count, 1);
    assert_eq!(
        core.metadata.proof_readiness,
        CoreProofReadiness::NoExpressions
    );
    assert_eq!(core.metadata.lean_covered_expr_count, 0);
    assert_eq!(core.metadata.proof_model_required_expr_count, 0);
    assert_eq!(core.metadata.lean_covered_pattern_count, 0);
    assert_eq!(core.metadata.proof_model_required_pattern_count, 0);
    assert_eq!(core.metadata.typed_core_expr_count, 0);
    assert_eq!(core.metadata.summary_only_expr_count, 0);
    assert_eq!(core.metadata.typed_core_pattern_count, 0);
    assert_eq!(core.metadata.summary_only_pattern_count, 0);
    assert_eq!(core.metadata.checked_preservation_expr_count, 0);
    assert_eq!(core.metadata.checked_preservation_pattern_count, 0);
    assert_eq!(core.metadata.typed_core_type_count, 1);
    assert_eq!(core.metadata.summary_only_type_count, 0);
    assert!(
        core.contract_text()
            .contains("schema=terlan.core_ir.v1\nmodule=core_boundary"),
        "contract text: {}",
        core.contract_text()
    );
    assert!(
        core.interface_text()
            .contains("module core_boundary.\n\npub value(): Int.\n"),
        "interface text: {}",
        core.interface_text()
    );
}

#[test]
fn syntax_output_lowering_to_core_preserves_trait_conformance_facts() {
    let module = parse_module_as_syntax_output(
        "\
module core_trait_conformance.\n\
pub trait Show[T] {\n\
    show(value: T): String.\n\
}.\n\
\n\
pub struct User implements Show[User] {\n\
    name: String\n\
}.\n\
\n\
pub (user: User) show(): String ->\n\
    user.name.\n\
\n\
pub impl Show[Int] for Int {\n\
    show(value: Int): String ->\n\
        \"int\".\n\
}.\n\
",
    )
    .unwrap_or_else(|err| panic!("failed to parse conformance fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    assert!(core.trait_conformances.iter().any(|conformance| {
        conformance.trait_ref == "Show[User]"
            && conformance.for_type == "User"
            && conformance.source == CoreTraitConformanceSource::Implements
            && conformance.public
    }));
    assert!(core.trait_conformances.iter().any(|conformance| {
        conformance.trait_ref == "Show[Int]"
            && conformance.for_type == "Int"
            && conformance.source == CoreTraitConformanceSource::ExplicitImpl
            && conformance.public
    }));
    assert!(
        core.contract_text()
            .contains("trait_conformance=Show[Int] for=Int source=ExplicitImpl public=true"),
        "contract text: {}",
        core.contract_text()
    );
}

/// Verifies mutable receiver calls are explicit effectful CoreIR nodes.
///
/// Inputs:
/// - A syntax-output module with a declared mutable receiver method and a
///   function body that calls it as `map.put()`.
///
/// Output:
/// - Test passes when formal CoreIR lowering records
///   `CoreExpr::MutableReceiverCall` with a `receiver_mutation` effect
///   instead of treating the call as an ordinary `Unit`-returning function.
///
/// Transformation:
/// - Parses and resolves the syntax-output module, lowers it through the
///   formal CoreIR path, and inspects the typed Core payload attached to the
///   caller function body.

/// Verifies mutable receiver calls are explicit effectful CoreIR nodes.
///
/// Inputs:
/// - A syntax-output module with a declared mutable receiver method and a
///   function body that calls it as `map.put()`.
///
/// Output:
/// - Test passes when formal CoreIR lowering records
///   `CoreExpr::MutableReceiverCall` with a `receiver_mutation` effect
///   instead of treating the call as an ordinary `Unit`-returning function.
///
/// Transformation:
/// - Parses and resolves the syntax-output module, lowers it through the
///   formal CoreIR path, and inspects the typed Core payload attached to the
///   caller function body.
#[test]
fn syntax_output_lowering_to_core_records_mutable_receiver_call_effect() {
    let module = parse_module_as_syntax_output(
        "\
module core_mutable_receiver_effect.\n\
\n\
pub struct Map {\n\
    size: Int\n\
}.\n\
\n\
pub (mut map: Map) put(): Unit ->\n\
    map.\n\
\n\
pub run(map: Map): Unit ->\n\
    map.put().\n\
",
    )
    .unwrap_or_else(|err| panic!("failed to parse mutable receiver fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);
    let function = core
        .functions
        .iter()
        .find(|function| function.name == "run")
        .unwrap_or_else(|| panic!("missing run function in core: {:?}", core.functions));

    let Some(CoreExpr::MutableReceiverCall {
        receiver,
        method,
        args,
        effects,
    }) = &function.clauses[0].body.core_expr
    else {
        panic!(
            "expected mutable receiver call core expr, found {:?}",
            function.clauses[0].body.core_expr
        );
    };

    assert_eq!(**receiver, CoreExpr::Var("map".to_string()));
    assert_eq!(method, "put");
    assert!(args.is_empty());
    assert_eq!(effects, &core_receiver_mutation_effect_set());
    assert!(function.clauses[0]
        .body
        .core_expr
        .as_ref()
        .unwrap()
        .contract_text()
        .contains("MutableReceiverCall(Var(map).put;args=;effects=Effects(receiver_mutation))"));
}

/// Verifies CoreIR proof-readiness precedence remains stable.
///
/// Inputs:
/// - None; constructs in-memory Core proof coverage counters for each
///   precedence boundary.
///
/// Output:
/// - Test passes when readiness follows runtime-boundary, partial,
///   proof-model-required, artifact-only, lean-covered, and no-expressions
///   order.
///
/// Transformation:
/// - Exercises producer-side readiness derivation directly without parsing
///   source or building a full Core module.

/// Verifies CoreIR proof-readiness precedence remains stable.
///
/// Inputs:
/// - None; constructs in-memory Core proof coverage counters for each
///   precedence boundary.
///
/// Output:
/// - Test passes when readiness follows runtime-boundary, partial,
///   proof-model-required, artifact-only, lean-covered, and no-expressions
///   order.
///
/// Transformation:
/// - Exercises producer-side readiness derivation directly without parsing
///   source or building a full Core module.
#[test]
fn syntax_output_lowering_to_core_readiness_precedence_matches_metadata_contract() {
    let cases = [
        (
            CoreProofCoverageCounts {
                runtime_boundary: 1,
                partial: 1,
                proof_model_required: 1,
                artifact_only: 1,
                lean_covered: 1,
            },
            CoreProofReadiness::RuntimeBoundary,
        ),
        (
            CoreProofCoverageCounts {
                partial: 1,
                proof_model_required: 1,
                artifact_only: 1,
                lean_covered: 1,
                ..CoreProofCoverageCounts::default()
            },
            CoreProofReadiness::Partial,
        ),
        (
            CoreProofCoverageCounts {
                proof_model_required: 1,
                artifact_only: 1,
                lean_covered: 1,
                ..CoreProofCoverageCounts::default()
            },
            CoreProofReadiness::ProofModelRequired,
        ),
        (
            CoreProofCoverageCounts {
                artifact_only: 1,
                lean_covered: 1,
                ..CoreProofCoverageCounts::default()
            },
            CoreProofReadiness::ArtifactOnly,
        ),
        (
            CoreProofCoverageCounts {
                lean_covered: 1,
                ..CoreProofCoverageCounts::default()
            },
            CoreProofReadiness::LeanCovered,
        ),
        (
            CoreProofCoverageCounts::default(),
            CoreProofReadiness::NoExpressions,
        ),
    ];

    for (coverage, expected) in cases {
        assert_eq!(core_proof_readiness(&coverage), expected);
    }
}

/// Verifies summary-only CoreType payloads contribute proof-model debt.
///
/// Inputs:
/// - None; constructs in-memory proof coverage and type payload counters.
///
/// Output:
/// - Test passes when summary-only type payloads promote otherwise covered
///   or expression-free modules to proof-model-required readiness.
///
/// Transformation:
/// - Exercises module-level readiness derivation without parsing source or
///   building a full Core module.

/// Verifies summary-only CoreType payloads contribute proof-model debt.
///
/// Inputs:
/// - None; constructs in-memory proof coverage and type payload counters.
///
/// Output:
/// - Test passes when summary-only type payloads promote otherwise covered
///   or expression-free modules to proof-model-required readiness.
///
/// Transformation:
/// - Exercises module-level readiness derivation without parsing source or
///   building a full Core module.
#[test]
fn syntax_output_lowering_to_core_readiness_includes_summary_only_type_debt() {
    let lean_coverage = CoreProofCoverageCounts {
        lean_covered: 1,
        ..CoreProofCoverageCounts::default()
    };
    let expression_free_coverage = CoreProofCoverageCounts::default();
    let typed_types = CoreTypePayloadCounts {
        typed_core_type: 1,
        ..CoreTypePayloadCounts::default()
    };
    let summary_types = CoreTypePayloadCounts {
        summary_only_type: 1,
        ..CoreTypePayloadCounts::default()
    };

    assert_eq!(
        core_module_proof_readiness(&lean_coverage, &summary_types),
        CoreProofReadiness::ProofModelRequired
    );
    assert_eq!(
        core_module_proof_readiness(&expression_free_coverage, &summary_types),
        CoreProofReadiness::ProofModelRequired
    );
    assert_eq!(
        core_module_proof_readiness(&expression_free_coverage, &typed_types),
        CoreProofReadiness::NoExpressions
    );
}

#[test]
fn syntax_output_lowering_to_core_records_function_clause_summaries() {
    let module = parse_module_as_syntax_output(
        "\
module core_expr_boundary.\n\
\n\
pub add(x: Int): Int ->\n\
    x + 1.\n",
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
    assert_eq!(function.params[0].core_ty, Some(CoreType::Int));
    assert_eq!(function.core_return_type, Some(CoreType::Int));
    assert_eq!(
        function.clauses[0].core_patterns,
        vec![Some(CorePattern::Var("x".to_string()))]
    );
    assert_eq!(
        function.clauses[0].pattern_proof_coverage,
        vec![CoreProofCoverage::LeanCovered]
    );
    assert_eq!(
        function.clauses[0].pattern_checked_preservation_evidence,
        vec![Some(CoreCheckedPreservationEvidence {
            kind: CoreCheckedPreservationEvidenceKind::StructuralCorePattern,
            freshness: CoreSubstitutionFreshnessEvidence::RuntimeBindingsRequired,
            target: "Var(x)".to_string(),
        })]
    );
    assert_eq!(function.clauses[0].body.kind, "BinaryOp");
    assert_eq!(function.clauses[0].body.operator.as_deref(), Some("+"));
    assert_eq!(
        function.clauses[0].body.children[0].core_expr,
        Some(CoreExpr::Var("x".to_string()))
    );
    assert_eq!(
        function.clauses[0].body.children[1].core_expr,
        Some(CoreExpr::Int(1))
    );
    assert_eq!(
        function.clauses[0].body.checked_preservation_evidence,
        Some(CoreCheckedPreservationEvidence {
            kind: CoreCheckedPreservationEvidenceKind::StructuralCoreExpr,
            freshness: CoreSubstitutionFreshnessEvidence::NoRuntimeBindings,
            target: "BinaryOp(+;Var(x), Int(1))".to_string(),
        })
    );
    assert_eq!(
        core.metadata.proof_readiness,
        CoreProofReadiness::LeanCovered
    );
    assert_eq!(core.metadata.lean_covered_expr_count, 3);
    assert_eq!(core.metadata.proof_model_required_expr_count, 0);
    assert_eq!(core.metadata.lean_covered_pattern_count, 1);
    assert_eq!(core.metadata.proof_model_required_pattern_count, 0);
    assert_eq!(core.metadata.typed_core_expr_count, 3);
    assert_eq!(core.metadata.summary_only_expr_count, 0);
    assert_eq!(core.metadata.typed_core_pattern_count, 1);
    assert_eq!(core.metadata.summary_only_pattern_count, 0);
    assert_eq!(core.metadata.checked_preservation_expr_count, 3);
    assert_eq!(core.metadata.checked_preservation_pattern_count, 1);
    assert_eq!(core.metadata.checked_preservation_expr_structural_count, 3);
    assert_eq!(
        core.metadata.checked_preservation_pattern_structural_count,
        1
    );
    assert_eq!(
        core.metadata
            .checked_preservation_expr_no_runtime_bindings_count,
        3
    );
    assert_eq!(
        core.metadata
            .checked_preservation_pattern_no_runtime_bindings_count,
        0
    );
    assert_eq!(
        core.metadata
            .checked_preservation_expr_runtime_bindings_required_count,
        0
    );
    assert_eq!(
        core.metadata
            .checked_preservation_pattern_runtime_bindings_required_count,
        1
    );
    assert_eq!(core.metadata.typed_core_type_count, 2);
    assert_eq!(core.metadata.summary_only_type_count, 0);
    assert!(
        core.contract_text().contains("function_clause=add/1#0"),
        "contract text: {}",
        core.contract_text()
    );
    assert!(
        core.contract_text().contains("core_patterns=Var(x)"),
        "contract text: {}",
        core.contract_text()
    );
    assert!(
        core.contract_text().contains("pattern_proof=lean-covered"),
        "contract text: {}",
        core.contract_text()
    );
    assert!(
            core.contract_text().contains(
                "body=BinaryOp:core=BinaryOp(+;Var(x), Int(1)):preservation=structural-core-expr(freshness=no-runtime-bindings;target=BinaryOp(+;Var(x), Int(1))):proof=lean-covered:op=+:"
            ),
            "contract text: {}",
            core.contract_text()
        );
    assert!(
            core.contract_text().contains(
                "Var:core=Var(x):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Var(x)):proof=lean-covered"
            ) && core.contract_text().contains(
                "Int:core=Int(1):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Int(1)):proof=lean-covered"
            ),
            "contract text: {}",
            core.contract_text()
        );
}

#[test]
fn syntax_output_lowering_to_core_records_record_pattern_payload() {
    let module = parse_module_as_syntax_output(
        "\
module core_expr_pattern_gap.\n\
\n\
pub bad(#Point { x = 1 }): Int ->\n\
    1.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "bad")
        .expect("core bad function");
    assert_eq!(function.clauses.len(), 1);
    assert_eq!(
        function.clauses[0].core_patterns,
        vec![Some(CorePattern::Record {
            name: "Point".to_string(),
            fields: vec![CoreRecordPatternField {
                key: "x".to_string(),
                required: true,
                value: CorePattern::Int(1),
            }],
        })]
    );
    assert_eq!(
        function.clauses[0].pattern_proof_coverage,
        vec![CoreProofCoverage::ProofModelRequired]
    );
    assert_eq!(
        function.clauses[0].pattern_checked_preservation_evidence,
        vec![Some(CoreCheckedPreservationEvidence {
            kind: CoreCheckedPreservationEvidenceKind::StructuralCorePattern,
            freshness: CoreSubstitutionFreshnessEvidence::NoRuntimeBindings,
            target: "Record(Point;x=Int(1))".to_string(),
        })]
    );
    assert_eq!(
        core.metadata.proof_readiness,
        CoreProofReadiness::ProofModelRequired
    );
    assert_eq!(core.metadata.lean_covered_pattern_count, 0);
    assert_eq!(core.metadata.proof_model_required_pattern_count, 1);
    assert_eq!(core.metadata.typed_core_pattern_count, 1);
    assert_eq!(core.metadata.summary_only_pattern_count, 0);
    assert_eq!(core.metadata.checked_preservation_pattern_count, 1);
    assert!(
        core.contract_text()
            .contains("core_patterns=Record(Point;x=Int(1))"),
        "contract text: {}",
        core.contract_text()
    );
    assert_eq!(core.metadata.checked_preservation_pattern_count, 1);
}

#[test]
fn syntax_output_lowering_to_core_pattern_coverage_includes_float_payload() {
    let pattern = SyntaxPatternOutput {
        kind: SyntaxPatternKind::Float,
        arity: 1,
        text: Some("1.0".to_string()),
        children: Vec::new(),
        fields: Vec::new(),
    };
    let core_pattern = core_pattern_from_syntax(&pattern);

    assert_eq!(core_pattern, Some(CorePattern::Float("1.0".to_string())));
    assert_eq!(
        core_pattern_proof_coverage(&pattern, core_pattern.as_ref()),
        CoreProofCoverage::ProofModelRequired
    );
}

#[test]
fn syntax_output_lowering_to_core_pattern_coverage_includes_map_payload() {
    let pattern = SyntaxPatternOutput {
        kind: SyntaxPatternKind::Map,
        arity: 1,
        text: None,
        children: Vec::new(),
        fields: vec![SyntaxPatternFieldOutput {
            key: "a".to_string(),
            required: true,
            value: Box::new(SyntaxPatternOutput {
                kind: SyntaxPatternKind::Int,
                arity: 1,
                text: Some("1".to_string()),
                children: Vec::new(),
                fields: Vec::new(),
            }),
        }],
    };
    let core_pattern = core_pattern_from_syntax(&pattern);

    assert_eq!(
        core_pattern,
        Some(CorePattern::Map(vec![CoreMapPatternField {
            key: "a".to_string(),
            required: true,
            value: CorePattern::Int(1),
        }]))
    );
    assert_eq!(
        core_pattern_proof_coverage(&pattern, core_pattern.as_ref()),
        CoreProofCoverage::ProofModelRequired
    );
}

#[test]
fn syntax_output_lowering_to_core_pattern_coverage_includes_list_cons_payload() {
    let pattern = SyntaxPatternOutput {
        kind: SyntaxPatternKind::ListCons,
        arity: 2,
        text: None,
        children: vec![
            SyntaxPatternOutput {
                kind: SyntaxPatternKind::Int,
                arity: 1,
                text: Some("1".to_string()),
                children: Vec::new(),
                fields: Vec::new(),
            },
            SyntaxPatternOutput {
                kind: SyntaxPatternKind::Var,
                arity: 1,
                text: Some("rest".to_string()),
                children: Vec::new(),
                fields: Vec::new(),
            },
        ],
        fields: Vec::new(),
    };
    let core_pattern = core_pattern_from_syntax(&pattern);

    assert_eq!(
        core_pattern,
        Some(CorePattern::ListCons {
            head: Box::new(CorePattern::Int(1)),
            tail: Box::new(CorePattern::Var("rest".to_string())),
        })
    );
    assert_eq!(
        core_pattern_proof_coverage(&pattern, core_pattern.as_ref()),
        CoreProofCoverage::ProofModelRequired
    );
}

/// Verifies structural patterns require Lean-modeled child patterns before
/// they are reported as Lean-covered.
///
/// Inputs:
/// - None; constructs a tuple pattern containing a float child pattern.
///
/// Output:
/// - Test passes when the tuple still carries a typed CorePattern payload
///   but reports proof-model-required coverage.
///
/// Transformation:
/// - Exercises recursive Lean-shape validation for structural CorePattern
///   payloads.
#[test]
fn syntax_output_lowering_to_core_pattern_coverage_requires_covered_tuple_children() {
    let pattern = SyntaxPatternOutput {
        kind: SyntaxPatternKind::Tuple,
        arity: 1,
        text: None,
        children: vec![SyntaxPatternOutput {
            kind: SyntaxPatternKind::Float,
            arity: 1,
            text: Some("1.0".to_string()),
            children: Vec::new(),
            fields: Vec::new(),
        }],
        fields: Vec::new(),
    };
    let core_pattern = core_pattern_from_syntax(&pattern);

    assert_eq!(
        core_pattern,
        Some(CorePattern::Tuple(vec![CorePattern::Float(
            "1.0".to_string()
        )]))
    );
    assert_eq!(
        core_pattern_proof_coverage(&pattern, core_pattern.as_ref()),
        CoreProofCoverage::ProofModelRequired
    );
}

/// Verifies list patterns require Lean-modeled child patterns before they
/// are reported as Lean-covered.
///
/// Inputs:
/// - None; constructs a list pattern containing a float child pattern.
///
/// Output:
/// - Test passes when the list still carries a typed CorePattern payload
///   but reports proof-model-required coverage.
///
/// Transformation:
/// - Exercises recursive Lean-shape validation for list CorePattern
///   payloads.
#[test]
fn syntax_output_lowering_to_core_pattern_coverage_requires_covered_list_children() {
    let pattern = SyntaxPatternOutput {
        kind: SyntaxPatternKind::List,
        arity: 1,
        text: None,
        children: vec![SyntaxPatternOutput {
            kind: SyntaxPatternKind::Float,
            arity: 1,
            text: Some("1.0".to_string()),
            children: Vec::new(),
            fields: Vec::new(),
        }],
        fields: Vec::new(),
    };
    let core_pattern = core_pattern_from_syntax(&pattern);

    assert_eq!(
        core_pattern,
        Some(CorePattern::List(vec![CorePattern::Float(
            "1.0".to_string()
        )]))
    );
    assert_eq!(
        core_pattern_proof_coverage(&pattern, core_pattern.as_ref()),
        CoreProofCoverage::ProofModelRequired
    );
}

/// Verifies constructor patterns require Lean-modeled argument patterns
/// before they are reported as Lean-covered.
///
/// Inputs:
/// - None; constructs a constructor pattern containing a float argument
///   pattern.
///
/// Output:
/// - Test passes when the constructor still carries a typed CorePattern
///   payload but reports proof-model-required coverage.
///
/// Transformation:
/// - Exercises recursive Lean-shape validation for constructor CorePattern
///   payloads.
#[test]
fn syntax_output_lowering_to_core_pattern_coverage_requires_covered_constructor_args() {
    let pattern = SyntaxPatternOutput {
        kind: SyntaxPatternKind::Constructor,
        arity: 1,
        text: Some("Some".to_string()),
        children: vec![SyntaxPatternOutput {
            kind: SyntaxPatternKind::Float,
            arity: 1,
            text: Some("1.0".to_string()),
            children: Vec::new(),
            fields: Vec::new(),
        }],
        fields: Vec::new(),
    };
    let core_pattern = core_pattern_from_syntax(&pattern);

    assert_eq!(
        core_pattern,
        Some(CorePattern::Constructor {
            name: "Some".to_string(),
            constructor_identity: None,
            args: vec![CorePattern::Float("1.0".to_string())],
        })
    );
    assert_eq!(
        core_pattern_proof_coverage(&pattern, core_pattern.as_ref()),
        CoreProofCoverage::ProofModelRequired
    );
}

#[test]
fn syntax_output_lowering_to_core_pattern_coverage_requires_map_field_payload() {
    let pattern = SyntaxPatternOutput {
        kind: SyntaxPatternKind::MapField,
        arity: 1,
        text: Some("a".to_string()),
        children: Vec::new(),
        fields: vec![SyntaxPatternFieldOutput {
            key: "a".to_string(),
            required: true,
            value: Box::new(SyntaxPatternOutput {
                kind: SyntaxPatternKind::Int,
                arity: 1,
                text: Some("1".to_string()),
                children: Vec::new(),
                fields: Vec::new(),
            }),
        }],
    };
    let core_pattern = core_pattern_from_syntax(&pattern);

    assert_eq!(core_pattern, None);
    assert_eq!(
        core_pattern_proof_coverage(&pattern, core_pattern.as_ref()),
        CoreProofCoverage::ProofModelRequired
    );
}

#[test]
fn syntax_output_lowering_to_core_pattern_coverage_includes_compat_wildcards() {
    for kind in [SyntaxPatternKind::Ignore, SyntaxPatternKind::Placeholder] {
        let pattern = SyntaxPatternOutput {
            kind,
            arity: 0,
            text: None,
            children: Vec::new(),
            fields: Vec::new(),
        };
        let core_pattern = core_pattern_from_syntax(&pattern);

        assert_eq!(core_pattern, Some(CorePattern::Wildcard));
        assert_eq!(
            core_pattern_proof_coverage(&pattern, core_pattern.as_ref()),
            CoreProofCoverage::LeanCovered
        );
    }
}

#[test]
fn syntax_output_lowering_to_core_records_local_call_core_expr() {
    let module = parse_module_as_syntax_output(
        "\
module core_call_boundary.\n\
\n\
identity(x: Int): Int ->\n\
    x.\n\
\n\
pub call_it(): Int ->\n\
    identity(1).\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "call_it")
        .expect("core call_it function");
    assert_eq!(function.clauses.len(), 1);
    assert_eq!(
        function.clauses[0].body.core_expr,
        Some(CoreExpr::Call {
            function: "identity".to_string(),
            args: vec![CoreExpr::Int(1)],
        })
    );
    assert_eq!(
        function.clauses[0].body.proof_coverage,
        CoreProofCoverage::LeanCovered
    );
    assert!(
            core.contract_text()
                .contains("Call:core=Call(identity;Int(1)):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Call(identity;Int(1))):proof=lean-covered"),
            "contract text: {}",
            core.contract_text()
        );
}

/// Verifies dedicated function-value invocation remains distinct in CoreIR.
///
/// Inputs:
/// - A syntax-output module whose function body uses `f.(value)`.
///
/// Output:
/// - Test passes when the formal CoreIR payload is `CoreExpr::FunctionCall`
///   with a variable callee and one argument.
///
/// Transformation:
/// - Parses, resolves, and lowers source through the syntax-output path,
///   then inspects the backend-neutral CoreIR expression.
#[test]
fn syntax_output_lowering_to_core_records_function_value_call_core_expr() {
    let module = parse_module_as_syntax_output(
        "\
module core_function_call_boundary.\n\
\n\
pub apply(value: Int, f: (Int) -> Int): Int ->\n\
    f.(value).\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "apply")
        .expect("core apply function");
    assert_eq!(function.clauses.len(), 1);
    assert_eq!(
        function.clauses[0].body.core_expr,
        Some(CoreExpr::FunctionCall {
            callee: Box::new(CoreExpr::Var("f".to_string())),
            args: vec![CoreExpr::Var("value".to_string())],
        })
    );
    assert!(
        core.contract_text()
            .contains("FunctionCall(Var(f);Var(value))"),
        "contract text: {}",
        core.contract_text()
    );
}

/// Verifies pipe-forward can target dedicated function-value invocation.
///
/// Inputs:
/// - A syntax-output module using `value |> f.()`.
///
/// Output:
/// - Test passes when the function typechecks without diagnostics.
///
/// Transformation:
/// - Exercises the pipe rule that prepends the left operand to a
///   `FunctionCall` argument list before checking the callee function type.
#[test]
fn syntax_output_typechecks_pipe_into_function_value_call() {
    let diagnostics = check_syntax_output(
        "\
module pipe_to_function_value_call.\n\
\n\
pub apply(value: Int, f: (Int) -> Int): Int ->\n\
    value |> f.().\n",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies compound type annotations lower to CoreType.
///
/// Inputs:
/// - None; exercises `core_type_from_text` with nested type text.
///
/// Output:
/// - Test passes when supported atom literal, list, tuple, parameterized
///   named, function, and union annotations produce typed CoreType
///   payloads.
///
/// Transformation:
/// - Parses type text directly without constructing a full module.
#[test]
fn syntax_output_lowering_to_core_records_compound_core_type_payloads() {
    assert_eq!(
        core_type_from_text("List[Int]"),
        Some(CoreType::List(Box::new(CoreType::Int)))
    );
    assert_eq!(core_type_from_text("String"), Some(CoreType::String));
    assert_eq!(core_type_from_text("Text"), Some(CoreType::Binary));
    assert_eq!(
        core_type_from_text("Atom[\"none\"]"),
        Some(CoreType::AtomLiteral("none".to_string()))
    );
    assert_eq!(
        core_type_from_text("Atom[\"Elixir.Module\"]"),
        Some(CoreType::AtomLiteral("Elixir.Module".to_string()))
    );
    assert_eq!(
        core_type_from_text(": none"),
        Some(CoreType::AtomLiteral("none".to_string()))
    );
    assert_eq!(
        core_type_from_text(":'Elixir.Module'"),
        Some(CoreType::AtomLiteral("Elixir.Module".to_string()))
    );
    assert_eq!(
        core_type_from_text("{Int, Bool}"),
        Some(CoreType::Tuple(vec![
            CoreTupleTypeElem::Type(CoreType::Int),
            CoreTupleTypeElem::Type(CoreType::Bool),
        ]))
    );
    assert_eq!(
        core_type_from_text("List[{Int, Bool}]"),
        Some(CoreType::List(Box::new(CoreType::Tuple(vec![
            CoreTupleTypeElem::Type(CoreType::Int),
            CoreTupleTypeElem::Type(CoreType::Bool),
        ]))))
    );
    assert_eq!(
        core_type_from_text("{Atom[\"ok\"], value: T, _: Int}"),
        Some(CoreType::Tuple(vec![
            CoreTupleTypeElem::Type(CoreType::AtomLiteral("ok".to_string())),
            CoreTupleTypeElem::Field {
                name: "value".to_string(),
                ty: CoreType::Named("T".to_string()),
            },
            CoreTupleTypeElem::Field {
                name: "_".to_string(),
                ty: CoreType::Int,
            },
        ]))
    );
    assert_eq!(
        core_type_from_text("#{name := Binary}"),
        Some(CoreType::Map(vec![CoreMapTypeField {
            key: "name".to_string(),
            operator: ":=".to_string(),
            value: CoreType::Binary,
        }]))
    );
    assert_eq!(
        core_type_from_text("# {name := Binary}"),
        Some(CoreType::Map(vec![CoreMapTypeField {
            key: "name".to_string(),
            operator: ":=".to_string(),
            value: CoreType::Binary,
        }]))
    );
    assert_eq!(
        core_type_from_text("#{:ok => {:ok, value: T}}"),
        Some(CoreType::Map(vec![CoreMapTypeField {
            key: ":ok".to_string(),
            operator: "=>".to_string(),
            value: CoreType::Tuple(vec![
                CoreTupleTypeElem::Type(CoreType::AtomLiteral("ok".to_string())),
                CoreTupleTypeElem::Field {
                    name: "value".to_string(),
                    ty: CoreType::Named("T".to_string()),
                },
            ]),
        }]))
    );
    assert_eq!(
        core_type_from_text("Result[Int]"),
        Some(CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![CoreType::Int],
        })
    );
    assert_eq!(
        core_type_from_text("List[Result[{Int, Bool}]]"),
        Some(CoreType::List(Box::new(CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![CoreType::Tuple(vec![
                CoreTupleTypeElem::Type(CoreType::Int),
                CoreTupleTypeElem::Type(CoreType::Bool),
            ])],
        })))
    );
    assert_eq!(
        core_type_from_text("(Int) -> Bool"),
        Some(CoreType::Arrow {
            params: vec![CoreType::Int],
            return_type: Box::new(CoreType::Bool),
        })
    );
    assert_eq!(
        core_type_from_text("(Int, Result[Bool]) -> List[Int]"),
        Some(CoreType::Arrow {
            params: vec![
                CoreType::Int,
                CoreType::Apply {
                    constructor: "Result".to_string(),
                    args: vec![CoreType::Bool],
                },
            ],
            return_type: Box::new(CoreType::List(Box::new(CoreType::Int))),
        })
    );
    assert_eq!(
        core_type_from_text("Result[(Int) -> Bool]"),
        Some(CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![CoreType::Arrow {
                params: vec![CoreType::Int],
                return_type: Box::new(CoreType::Bool),
            }],
        })
    );
    assert_eq!(
        core_type_from_text("Int | Bool"),
        Some(CoreType::Union(vec![CoreType::Int, CoreType::Bool]))
    );
    assert_eq!(
        core_type_from_text("List[Int | Bool]"),
        Some(CoreType::List(Box::new(CoreType::Union(vec![
            CoreType::Int,
            CoreType::Bool,
        ]))))
    );
    assert_eq!(
        core_type_from_text("(Int) -> Bool | Never"),
        Some(CoreType::Union(vec![
            CoreType::Arrow {
                params: vec![CoreType::Int],
                return_type: Box::new(CoreType::Bool),
            },
            CoreType::Never,
        ]))
    );
    assert_eq!(
        core_type_from_text(":none | :empty"),
        Some(CoreType::Union(vec![
            CoreType::AtomLiteral("none".to_string()),
            CoreType::AtomLiteral("empty".to_string()),
        ]))
    );
    assert_eq!(core_type_from_text("Int | "), None);
    assert_eq!(core_type_from_text("none"), None);
    assert_eq!(core_type_from_text("result[Int]"), None);
}

/// Verifies type declaration bodies carry optional typed CoreType payloads.
///
/// Inputs:
/// - None; constructs a syntax-output module with supported and
///   unsupported type declaration bodies.
///
/// Output:
/// - Test passes when supported aliases, including atom-literal aliases,
///   have typed `core_body` payloads.
///
/// Transformation:
/// - Lowers resolved module interface type declarations into CoreIR type
///   declarations without emitting backend-specific type syntax.
#[test]
fn syntax_output_lowering_to_core_records_type_decl_core_body_payloads() {
    let module = parse_module_as_syntax_output(
        "\
module core_type_decl_boundary.\n\
\n\
pub type Text = Binary.\n\
pub type MaybeInt = Int | Never.\n\
pub type Items[T] = List[T].\n\
pub type None = :none.\n\
pub type Ok[T] = {:ok, value: T}.\n\
pub type Props = #{name := Binary}.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_resolved_module_to_core(&resolved);

    let text = core
        .types
        .iter()
        .find(|decl| decl.name == "Text")
        .expect("Text core type declaration");
    assert_eq!(text.core_body, Some(CoreType::Binary));

    let maybe_int = core
        .types
        .iter()
        .find(|decl| decl.name == "MaybeInt")
        .expect("MaybeInt core type declaration");
    assert_eq!(
        maybe_int.core_body,
        Some(CoreType::Union(vec![CoreType::Int, CoreType::Never]))
    );

    let items = core
        .types
        .iter()
        .find(|decl| decl.name == "Items")
        .expect("Items core type declaration");
    assert_eq!(
        items.core_body,
        Some(CoreType::List(Box::new(CoreType::Named("T".to_string()))))
    );

    let none = core
        .types
        .iter()
        .find(|decl| decl.name == "None")
        .expect("None core type declaration");
    assert_eq!(
        none.core_body,
        Some(CoreType::AtomLiteral("none".to_string()))
    );

    let ok = core
        .types
        .iter()
        .find(|decl| decl.name == "Ok")
        .expect("Ok core type declaration");
    assert_eq!(
        ok.core_body,
        Some(CoreType::Tuple(vec![
            CoreTupleTypeElem::Type(CoreType::AtomLiteral("ok".to_string())),
            CoreTupleTypeElem::Field {
                name: "value".to_string(),
                ty: CoreType::Named("T".to_string()),
            },
        ]))
    );

    let props = core
        .types
        .iter()
        .find(|decl| decl.name == "Props")
        .expect("Props core type declaration");
    assert_eq!(
        props.core_body,
        Some(CoreType::Map(vec![CoreMapTypeField {
            key: "name".to_string(),
            operator: ":=".to_string(),
            value: CoreType::Binary,
        }]))
    );
    assert_eq!(core.metadata.typed_core_type_count, 6);
    assert_eq!(core.metadata.summary_only_type_count, 0);
}

/// Verifies unsupported type declaration bodies count as summary-only
/// CoreType payloads.
///
/// Inputs:
/// - None; constructs a syntax-output module with a public struct
///   declaration whose structural body is not yet represented as CoreType.
///
/// Output:
/// - Test passes when the type declaration has no `core_body`, and metadata
///   records one summary-only type payload.
///
/// Transformation:
/// - Lowers a resolved struct declaration through CoreIR metadata
///   construction without backend-specific type encoding.
#[test]
fn syntax_output_lowering_to_core_counts_summary_only_type_decl_payloads() {
    let module = parse_module_as_syntax_output(
        "\
module core_summary_type_decl_boundary.\n\
\n\
pub struct Point {\n\
    x: Int,\n\
    y: Int\n\
}.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_resolved_module_to_core(&resolved);

    let point = core
        .types
        .iter()
        .find(|decl| decl.name == "Point")
        .expect("Point core type declaration");
    assert_eq!(point.core_body, None);
    assert_eq!(
        core.metadata.proof_readiness,
        CoreProofReadiness::ProofModelRequired
    );
    assert_eq!(core.metadata.typed_core_type_count, 0);
    assert_eq!(core.metadata.summary_only_type_count, 1);
}

/// Verifies uppercase constructor-like calls lower as CoreIR candidates.
///
/// Inputs:
/// - None; constructs a syntax-output call expression for `Ok(1)`.
///
/// Output:
/// - Test passes when the expression has a typed `CoreExpr::ConstructorCall`
///   payload and is classified as partial.
///
/// Transformation:
/// - Exercises the named-call lowering rule without invoking resolver
///   behavior for constructor aliases.
#[test]
fn syntax_output_lowering_to_core_records_constructor_call_candidate() {
    let expr = SyntaxExprOutput {
        kind: SyntaxExprKind::Call,
        arity: 1,
        text: None,
        span: Default::default(),
        raw: None,
        operator: None,
        remote: None,
        children: vec![
            SyntaxExprOutput {
                kind: SyntaxExprKind::Var,
                arity: 0,
                text: Some("Ok".to_string()),
                span: Default::default(),
                raw: None,
                operator: None,
                remote: None,
                children: Vec::new(),
                patterns: Vec::new(),
                fields: Vec::new(),
                clauses: Vec::new(),
                catch_clauses: Vec::new(),
                try_after: None,
                html_nodes: Vec::new(),
            },
            SyntaxExprOutput {
                kind: SyntaxExprKind::Int,
                arity: 0,
                text: Some("1".to_string()),
                span: Default::default(),
                raw: None,
                operator: None,
                remote: None,
                children: Vec::new(),
                patterns: Vec::new(),
                fields: Vec::new(),
                clauses: Vec::new(),
                catch_clauses: Vec::new(),
                try_after: None,
                html_nodes: Vec::new(),
            },
        ],
        patterns: Vec::new(),
        fields: Vec::new(),
        clauses: Vec::new(),
        catch_clauses: Vec::new(),
        try_after: None,
        html_nodes: Vec::new(),
    };
    let core_expr = core_expr_from_syntax(&expr);

    assert_eq!(
        core_expr,
        Some(CoreExpr::ConstructorCall {
            constructor: "Ok".to_string(),
            constructor_identity: None,
            args: vec![CoreExpr::Int(1)],
        })
    );
    assert_eq!(
        core_expr_proof_coverage(&expr, core_expr.as_ref()),
        CoreProofCoverage::Partial
    );
}

/// Verifies the remote-call proof policy switch remains conservative.
///
/// Inputs:
/// - A typed `CoreExpr::RemoteCall` value matching the formal remote-call
///   payload shape.
/// - The summary-only `None` path used when coverage is requested without a
///   typed Core payload.
///
/// Output:
/// - The test passes when both paths report `ProofModelRequired`, and the
///   promotion helper still prevents remote calls from counting as
///   Lean-modeled.
///
/// Transformation:
/// - Exercises the named compiler-side promotion policy without lowering a
///   source fixture, so future remote-dispatch promotion must update this
///   explicit policy guard.
#[test]
fn syntax_output_lowering_to_core_remote_call_policy_switch_stays_proof_model_required() {
    let remote_call = CoreExpr::RemoteCall {
        module: "Eq".to_string(),
        function: "equal".to_string(),
        args: vec![
            CoreExpr::Var("Left".to_string()),
            CoreExpr::Var("Right".to_string()),
        ],
    };

    assert_eq!(
        remote_call_proof_coverage_policy(Some(&remote_call)),
        CoreProofCoverage::ProofModelRequired
    );
    assert_eq!(
        remote_call_proof_coverage_policy(None),
        CoreProofCoverage::ProofModelRequired
    );
    assert!(!remote_call_is_promoted_to_lean_covered());
    assert!(!core_expr_is_lean_modeled(&remote_call));
}
