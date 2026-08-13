use super::*;

use crate::terlan_hir::resolve_syntax_module_output;
use crate::terlan_syntax::{parse_module_as_syntax_output, SyntaxPatternFieldOutput};

#[test]
pub(super) fn syntax_output_lowering_to_core_preserves_interface_contract() {
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
            .contains("module core_boundary.\n\n@pure\npub value(): Int.\n"),
        "interface text: {}",
        core.interface_text()
    );
}

/// Verifies receiver-method implementations survive the syntax-to-CoreIR
/// callable ABI normalization.
#[test]
pub(super) fn syntax_output_lowering_to_core_preserves_receiver_method_clauses() {
    let module = parse_module_as_syntax_output(
        "\
module core_receiver_body.\n\
pub opaque type Box.\n\
pub (value: Box) identity(): Box ->\n\
    value.\n",
    )
    .expect("parse receiver method fixture");
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let method = core
        .functions
        .iter()
        .find(|function| function.name == "identity" && function.arity == 1)
        .expect("receiver method core function");
    assert_eq!(method.clauses.len(), 1);
    assert_eq!(method.clauses[0].core_patterns.len(), 1);
    assert_eq!(
        method.clauses[0].core_patterns[0],
        Some(CorePattern::Var("value".to_string()))
    );
    assert_eq!(
        method.clauses[0].body.core_expr,
        Some(CoreExpr::Var("value".to_string()))
    );
}

#[test]
pub(super) fn syntax_output_lowering_to_core_preserves_generic_receiver_methods() {
    let module = parse_module_as_syntax_output(
        "module core_generic_receiver.\n\npub struct Presenter { prefix: String }.\npub (presenter: Presenter) present[T => {name: String}](value: T): String -> value.name.\n",
    )
    .expect("parse generic receiver method fixture");
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let method = core
        .functions
        .iter()
        .find(|function| function.name == "present" && function.arity == 2)
        .expect("generic receiver method core function");
    assert_eq!(method.generic_params, ["T => {name: String}"]);
    assert_eq!(method.clauses.len(), 1);
}

#[test]
pub(super) fn syntax_output_lowering_to_core_preserves_compiler_native_operations() {
    let module = parse_module_as_syntax_output(
        "\
module core_native_receiver.\n\
pub opaque type Frame.\n\
@compiler.native {polars.dataframe.height}\n\
pub (frame: Frame) height(): Int ->\n\
    native.\n",
    )
    .expect("parse native receiver fixture");
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);
    let method = core
        .functions
        .iter()
        .find(|function| function.name == "height" && function.arity == 1)
        .expect("native receiver core function");

    assert_eq!(
        method.native_operation.as_deref(),
        Some("polars.dataframe.height")
    );
    assert_eq!(method.clauses.len(), 1);
}

#[test]
pub(super) fn syntax_output_lowering_to_core_preserves_trait_conformance_facts() {
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
pub(super) fn syntax_output_lowering_to_core_records_mutable_receiver_call_effect() {
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

/// Verifies std collection receiver calls keep executable CoreIR in sequences.
///
/// Inputs:
/// - A source module that constructs a `std.collections.List`, mutates it with
///   receiver calls, and reads its length in the final expression.
///
/// Output:
/// - Test passes when the full function body carries executable CoreIR instead
///   of becoming summary-only at the `values.length()` receiver call.
///
/// Transformation:
/// - Parses, resolves, and lowers through the formal syntax-output path so the
///   VM can execute the same CoreIR shape used by release REPL tests.
#[test]
pub(super) fn syntax_output_lowering_to_core_records_collection_receiver_call_in_sequence() {
    let module = parse_module_as_syntax_output(
        "\
module core_collection_receiver_sequence.\n\
\n\
import std.collections.List.\n\
\n\
pub run(): Bool ->\n\
    let values = List.new();\n\
    values.push(1);\n\
    values.push(2);\n\
    values.length() == 2.\n\
",
    )
    .unwrap_or_else(|err| panic!("failed to parse collection receiver fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);
    let function = core
        .functions
        .iter()
        .find(|function| function.name == "run")
        .unwrap_or_else(|| panic!("missing run function in core: {:?}", core.functions));

    assert!(
        function.clauses[0].body.core_expr.is_some(),
        "missing executable body:\n{}",
        core.contract_text()
    );
}

/// Verifies ready SQL forms survive the formal lowering boundary as CoreIR.
///
/// Inputs:
/// - A syntax-output module whose function body is a ready `sql[Row]` form
///   with ordered interpolations and a simple row projection.
///
/// Output:
/// - Test passes when CoreIR carries a `SqlQuery` payload with bound SQL,
///   ordered parameter expressions, query kind, transaction requirement,
///   cardinality, result type, and projection fields.
///
/// Transformation:
/// - Parses and resolves the module, lowers it to CoreIR, and checks that the
///   SQL wrapper plan is preserved as backend-neutral data instead of being
///   dropped as an unsupported raw macro.
#[test]
pub(super) fn syntax_output_lowering_to_core_records_sql_query_payload() {
    let module = parse_module_as_syntax_output(
        "\
module core_sql_query.\n\
\n\
pub struct UserRow {\n\
    id: Int\n\
}.\n\
\n\
pub normalize_active(active: Bool): Bool -> active.\n\
\n\
pub find_user(id: Int, active: Bool): Result[Option[UserRow], Error] ->\n\
    sql[UserRow] {SELECT id FROM users WHERE id = ${id} AND active = ${normalize_active(active)} LIMIT 1}.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse SQL CoreIR fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);
    let function = core
        .functions
        .iter()
        .find(|function| function.name == "find_user")
        .unwrap_or_else(|| panic!("missing find_user function in core: {:?}", core.functions));

    let Some(CoreExpr::SqlQuery {
        row_type,
        bound_sql,
        parameters,
        query_kind,
        transaction_requirement,
        cardinality,
        result_type,
        result_core_type: _,
        projection_fields,
    }) = &function.clauses[0].body.core_expr
    else {
        panic!(
            "expected SQL query core expr, found {:?}",
            function.clauses[0].body.core_expr
        );
    };

    assert_eq!(row_type, "UserRow");
    assert_eq!(
        bound_sql,
        "SELECT id FROM users WHERE id = $1 AND active = $2 LIMIT 1"
    );
    assert_eq!(
        parameters,
        &vec![
            CoreExpr::Var("id".to_string()),
            CoreExpr::Call {
                function: "normalize_active".to_string(),
                args: vec![CoreExpr::Var("active".to_string())]
            }
        ]
    );
    assert_eq!(query_kind, "select");
    assert_eq!(transaction_requirement, "autocommit_allowed");
    assert_eq!(cardinality, "optional_one");
    assert_eq!(result_type, "Result[Option[UserRow], Error]");
    assert_eq!(projection_fields, &vec!["id".to_string()]);
    assert!(
        function.clauses[0]
            .body
            .core_expr
            .as_ref()
            .unwrap()
            .contract_text()
            .contains("SqlQuery(row_type=UserRow;params=[Var(id),Call(normalize_active;Var(active))];kind=select;transaction=autocommit_allowed;cardinality=optional_one;result=Result[Option[UserRow], Error];projection=id;sql=SELECT id FROM users WHERE id = $1 AND active = $2 LIMIT 1)")
    );
    assert_eq!(
        function.clauses[0].body.proof_coverage,
        CoreProofCoverage::RuntimeBoundary
    );
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
pub(super) fn syntax_output_lowering_to_core_readiness_precedence_matches_metadata_contract() {
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
pub(super) fn syntax_output_lowering_to_core_readiness_includes_summary_only_type_debt() {
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
pub(super) fn syntax_output_lowering_to_core_records_function_clause_summaries() {
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
pub(super) fn syntax_output_lowering_to_core_records_record_pattern_payload() {
    let module = parse_module_as_syntax_output(
        "\
module core_expr_pattern_gap.\n\
\n\
pub bad(Point{x: 1}): Int ->\n\
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
pub(super) fn syntax_output_lowering_to_core_pattern_coverage_includes_float_payload() {
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
pub(super) fn syntax_output_lowering_to_core_pattern_coverage_includes_map_payload() {
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
pub(super) fn syntax_output_lowering_to_core_pattern_coverage_includes_string_capture_payload() {
    let pattern = SyntaxPatternOutput {
        kind: SyntaxPatternKind::StringPattern,
        arity: 2,
        text: Some("file/${id: Int}/part/${name}.txt".to_string()),
        children: vec![
            SyntaxPatternOutput {
                kind: SyntaxPatternKind::StringCapture,
                arity: 0,
                text: Some("id: Int".to_string()),
                children: Vec::new(),
                fields: Vec::new(),
            },
            SyntaxPatternOutput {
                kind: SyntaxPatternKind::StringCapture,
                arity: 0,
                text: Some("name".to_string()),
                children: Vec::new(),
                fields: Vec::new(),
            },
        ],
        fields: Vec::new(),
    };
    let core_pattern = core_pattern_from_syntax(&pattern);

    assert_eq!(
        core_pattern,
        Some(CorePattern::StringPattern(vec![
            CoreStringPatternSegment::Literal("file/".to_string()),
            CoreStringPatternSegment::Capture(CoreStringPatternCapture {
                name: "id".to_string(),
                type_annotation: Some("Int".to_string()),
            }),
            CoreStringPatternSegment::Literal("/part/".to_string()),
            CoreStringPatternSegment::Capture(CoreStringPatternCapture {
                name: "name".to_string(),
                type_annotation: None,
            }),
            CoreStringPatternSegment::Literal(".txt".to_string()),
        ]))
    );
    assert_eq!(
        core_pattern_proof_coverage(&pattern, core_pattern.as_ref()),
        CoreProofCoverage::ProofModelRequired
    );
    assert!(core_pattern
        .as_ref()
        .is_some_and(|pattern| pattern.contract_text().contains("Capture(id:Int)")));
}
