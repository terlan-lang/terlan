use std::collections::HashMap;

use super::test_support::*;
use super::*;
use crate::terlan_hir::{
    resolve_syntax_module_output, resolve_syntax_module_output_with_interfaces,
    syntax_module_output_to_interface,
};
use crate::terlan_syntax::{
    parse_interface_module_as_syntax_output, parse_module_as_syntax_output,
};

/// Verifies declared constructor calls carry resolved CoreIR identity.
///
/// Inputs:
/// - None; constructs a syntax-output module with a declared `Ok`
///   constructor and a function body that calls `Ok(1)`.
///
/// Output:
/// - Test passes when the function body has a typed constructor-call Core
///   payload with `constructor_identity = Some("Ok")` and Lean-covered
///   proof coverage.
///
/// Transformation:
/// - Exercises the post-lowering constructor identity annotation pass that
///   consumes resolved module constructor declarations.
#[test]
fn syntax_output_lowering_to_core_resolves_declared_constructor_call_identity() {
    let module = parse_module_as_syntax_output(
        "\
module core_constructor_identity_boundary.\n\
\n\
pub constructor Ok {\n\
    (value: Int): Dynamic -> value\n\
}.\n\
\n\
pub make(): Dynamic ->\n\
    Ok(1).\n",
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
        Some(CoreExpr::ConstructorCall {
            constructor: "Ok".to_string(),
            constructor_identity: Some("Ok".to_string()),
            args: vec![CoreExpr::Int(1)],
        })
    );
    assert_eq!(
        function.clauses[0].body.proof_coverage,
        CoreProofCoverage::LeanCovered
    );
    assert_eq!(core.metadata.resolved_constructor_call_identity_count, 1);
    assert_eq!(core.metadata.resolved_constructor_chain_identity_count, 0);
    assert_eq!(core.metadata.resolved_constructor_pattern_identity_count, 0);
    assert_eq!(core.metadata.unresolved_constructor_call_candidate_count, 0);
    assert_eq!(
        core.metadata.unresolved_constructor_chain_candidate_count,
        0
    );
    assert_eq!(
        core.metadata.unresolved_constructor_pattern_candidate_count,
        0
    );
    assert!(
        core.contract_text()
            .contains("ConstructorCall(Ok;identity=Ok;Int(1))"),
        "contract text: {}",
        core.contract_text()
    );
    assert!(
        core.contract_text()
            .contains("resolved_constructor_call_identity:1"),
        "contract text: {}",
        core.contract_text()
    );
    assert!(
            core.contract_text().contains(
                "preservation=structural-core-expr(freshness=no-runtime-bindings;target=ConstructorCall(Ok;identity=Ok;Int(1)))"
            ),
            "contract text: {}",
            core.contract_text()
        );
}

/// Verifies imported public constructor calls carry qualified CoreIR
/// identity.
///
/// Inputs:
/// - A provider interface declaring public constructor `Ok`.
/// - A consumer syntax-output module importing `Ok` and calling it.
///
/// Output:
/// - Test passes when typechecking succeeds and the consumer CoreIR call is
///   annotated with `constructor_identity = Some("provider.Ok")`.
///
/// Transformation:
/// - Resolves the consumer against an explicit interface map, lowers it to
///   CoreIR, and verifies imported constructor identity metadata without
///   adding backend-specific layout assumptions.
#[test]
fn syntax_output_lowering_to_core_resolves_imported_constructor_call_identity() {
    let provider = parse_interface_module_as_syntax_output(
        "\
module provider.\n\
\n\
pub constructor Ok {\n\
    (value: Int): Dynamic -> value\n\
}.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse provider interface fixture: {:?}", err));
    let mut interfaces = HashMap::new();
    interfaces.insert(
        provider.module_name.clone(),
        syntax_module_output_to_interface(&provider),
    );
    let module = parse_module_as_syntax_output(
        "\
module imported_constructor_identity_boundary.\n\
\n\
import provider.{Ok}.\n\
\n\
pub make(): Dynamic ->\n\
    Ok(1).\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output_with_interfaces(&module, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&module, &resolved);
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "make")
        .expect("core make function");
    assert_eq!(function.clauses.len(), 1);
    assert_eq!(
        function.clauses[0].body.core_expr,
        Some(CoreExpr::ConstructorCall {
            constructor: "Ok".to_string(),
            constructor_identity: Some("provider.Ok".to_string()),
            args: vec![CoreExpr::Int(1)],
        })
    );
    assert_eq!(
        function.clauses[0].body.proof_coverage,
        CoreProofCoverage::LeanCovered
    );
    assert_eq!(core.metadata.resolved_constructor_call_identity_count, 1);
    assert_eq!(core.metadata.unresolved_constructor_call_candidate_count, 0);
    assert!(
        core.contract_text()
            .contains("ConstructorCall(Ok;identity=provider.Ok;Int(1))"),
        "contract text: {}",
        core.contract_text()
    );
    assert!(
        core.contract_text()
            .contains("resolved_constructor_call_identity:1"),
        "contract text: {}",
        core.contract_text()
    );
}

/// Verifies aliased imported public constructor calls carry source identity.
///
/// Inputs:
/// - A provider interface declaring public constructor `Ok`.
/// - A consumer syntax-output module importing `Ok as Success` and calling
///   `Success`.
///
/// Output:
/// - Test passes when typechecking succeeds, CoreIR preserves the
///   source-visible constructor head `Success`, and the constructor
///   identity remains `provider.Ok`.
///
/// Transformation:
/// - Resolves the aliased import against an explicit interface map, lowers
///   to CoreIR, and verifies constructor identity metadata is based on the
///   provider/source constructor rather than the local alias.
#[test]
fn syntax_output_lowering_to_core_resolves_aliased_imported_constructor_call_identity() {
    let provider = parse_interface_module_as_syntax_output(
        "\
module provider.\n\
\n\
pub constructor Ok {\n\
    (value: Int): Dynamic -> value\n\
}.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse provider interface fixture: {:?}", err));
    let mut interfaces = HashMap::new();
    interfaces.insert(
        provider.module_name.clone(),
        syntax_module_output_to_interface(&provider),
    );
    let module = parse_module_as_syntax_output(
        "\
module aliased_imported_constructor_identity_boundary.\n\
\n\
import provider.{Ok as Success}.\n\
\n\
pub make(): Dynamic ->\n\
    Success(1).\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output_with_interfaces(&module, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&module, &resolved);
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "make")
        .expect("core make function");
    assert_eq!(function.clauses.len(), 1);
    assert_eq!(
        function.clauses[0].body.core_expr,
        Some(CoreExpr::ConstructorCall {
            constructor: "Success".to_string(),
            constructor_identity: Some("provider.Ok".to_string()),
            args: vec![CoreExpr::Int(1)],
        })
    );
    assert_eq!(
        function.clauses[0].body.proof_coverage,
        CoreProofCoverage::LeanCovered
    );
    assert_eq!(core.metadata.resolved_constructor_call_identity_count, 1);
    assert_eq!(core.metadata.unresolved_constructor_call_candidate_count, 0);
    assert!(
        core.contract_text()
            .contains("ConstructorCall(Success;identity=provider.Ok;Int(1))"),
        "contract text: {}",
        core.contract_text()
    );
    assert!(
        core.contract_text()
            .contains("resolved_constructor_call_identity:1"),
        "contract text: {}",
        core.contract_text()
    );
}

/// Verifies eligible local type-alias constructor calls carry CoreIR
/// identity.
///
/// Inputs:
/// - None; constructs a syntax-output module with `pub type Ok[T] =
///   {:ok, value: T}` and a function body that calls `Ok(1)`.
///
/// Output:
/// - Test passes when the function body has a typed constructor-call Core
///   payload with `constructor_identity = Some("Ok")` and no unresolved
///   constructor-call candidates.
///
/// Transformation:
/// - Exercises the post-lowering constructor identity annotation pass for
///   single-shape type aliases that the typechecker already accepts as
///   constructor-like calls.
#[test]
fn syntax_output_lowering_to_core_resolves_local_alias_constructor_call_identity() {
    let module = parse_module_as_syntax_output(
        "\
module core_alias_constructor_identity_boundary.\n\
\n\
pub type Ok[T] = {:ok, value: T}.\n\
\n\
pub make(): Dynamic ->\n\
    Ok(1).\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let diagnostics = type_check_syntax_module_output(&module, &resolved);
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "make")
        .expect("core make function");
    assert_eq!(function.clauses.len(), 1);
    assert_eq!(
        function.clauses[0].body.core_expr,
        Some(CoreExpr::ConstructorCall {
            constructor: "Ok".to_string(),
            constructor_identity: Some("Ok".to_string()),
            args: vec![CoreExpr::Int(1)],
        })
    );
    assert_eq!(
        function.clauses[0].body.proof_coverage,
        CoreProofCoverage::LeanCovered
    );
    assert_eq!(core.metadata.resolved_constructor_call_identity_count, 1);
    assert_eq!(core.metadata.unresolved_constructor_call_candidate_count, 0);
    assert!(
        core.contract_text()
            .contains("ConstructorCall(Ok;identity=Ok;Int(1))"),
        "contract text: {}",
        core.contract_text()
    );
}

/// Verifies eligible directly imported type-alias constructor calls carry
/// qualified CoreIR identity.
///
/// Inputs:
/// - A provider interface declaring public alias constructor `Ok`.
/// - A consumer syntax-output module importing `Ok` directly and calling
///   `Ok(1)`.
///
/// Output:
/// - Test passes when CoreIR preserves the source-visible constructor head
///   `Ok` and resolves the identity to `provider.Ok`.
///
/// Transformation:
/// - Resolves the direct type import against an explicit interface map,
///   lowers to CoreIR, and verifies imported single-shape type-alias
///   constructor identity metadata without using a local import alias.
#[test]
fn syntax_output_lowering_to_core_resolves_direct_imported_alias_constructor_call_identity() {
    let provider = parse_interface_module_as_syntax_output(
        "\
module provider.\n\
\n\
pub type Ok[T] = {:ok, value: T}.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse provider interface fixture: {:?}", err));
    let mut interfaces = HashMap::new();
    interfaces.insert(
        provider.module_name.clone(),
        syntax_module_output_to_interface(&provider),
    );
    let module = parse_module_as_syntax_output(
        "\
module direct_imported_alias_constructor_identity_boundary.\n\
\n\
import provider.{Ok}.\n\
\n\
pub make(): Dynamic ->\n\
    Ok(1).\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output_with_interfaces(&module, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&module, &resolved);
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "make")
        .expect("core make function");
    assert_eq!(function.clauses.len(), 1);
    assert_eq!(
        function.clauses[0].body.core_expr,
        Some(CoreExpr::ConstructorCall {
            constructor: "Ok".to_string(),
            constructor_identity: Some("provider.Ok".to_string()),
            args: vec![CoreExpr::Int(1)],
        })
    );
    assert_eq!(core.metadata.resolved_constructor_call_identity_count, 1);
    assert_eq!(core.metadata.unresolved_constructor_call_candidate_count, 0);
    assert!(
        core.contract_text()
            .contains("ConstructorCall(Ok;identity=provider.Ok;Int(1))"),
        "contract text: {}",
        core.contract_text()
    );
}

/// Verifies eligible imported type-alias constructor calls carry qualified
/// CoreIR identity.
///
/// Inputs:
/// - A provider interface declaring public alias constructor `Ok`.
/// - A consumer syntax-output module importing `Ok as Success` and calling
///   `Success`.
///
/// Output:
/// - Test passes when CoreIR preserves the source-visible constructor head
///   `Success` and resolves the identity to `provider.Ok`.
///
/// Transformation:
/// - Resolves the aliased import against an explicit interface map, lowers
///   to CoreIR, and verifies single-shape type-alias constructor identity
///   metadata is based on the provider/source alias.
#[test]
fn syntax_output_lowering_to_core_resolves_imported_alias_constructor_call_identity() {
    let provider = parse_interface_module_as_syntax_output(
        "\
module provider.\n\
\n\
pub type Ok[T] = {:ok, value: T}.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse provider interface fixture: {:?}", err));
    let mut interfaces = HashMap::new();
    interfaces.insert(
        provider.module_name.clone(),
        syntax_module_output_to_interface(&provider),
    );
    let module = parse_module_as_syntax_output(
        "\
module imported_alias_constructor_identity_boundary.\n\
\n\
import provider.{Ok as Success}.\n\
\n\
pub make(): Dynamic ->\n\
    Success(1).\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output_with_interfaces(&module, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&module, &resolved);
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "make")
        .expect("core make function");
    assert_eq!(function.clauses.len(), 1);
    assert_eq!(
        function.clauses[0].body.core_expr,
        Some(CoreExpr::ConstructorCall {
            constructor: "Success".to_string(),
            constructor_identity: Some("provider.Ok".to_string()),
            args: vec![CoreExpr::Int(1)],
        })
    );
    assert_eq!(core.metadata.resolved_constructor_call_identity_count, 1);
    assert_eq!(core.metadata.unresolved_constructor_call_candidate_count, 0);
    assert!(
        core.contract_text()
            .contains("ConstructorCall(Success;identity=provider.Ok;Int(1))"),
        "contract text: {}",
        core.contract_text()
    );
}

/// Verifies undeclared uppercase calls remain visible as unresolved
/// constructor candidates.
///
/// Inputs:
/// - None; constructs a syntax-output module with `Ok(1)` and no local
///   constructor declaration.
///
/// Output:
/// - Test passes when the function body keeps its constructor-call
///   candidate payload but CoreIR metadata records it as unresolved.
///
/// Transformation:
/// - Exercises the post-lowering constructor identity pass on a module
///   where the candidate name cannot be resolved.
#[test]
fn syntax_output_lowering_to_core_counts_unresolved_constructor_call_candidate() {
    let module = parse_module_as_syntax_output(
        "\
module core_unresolved_constructor_candidate_boundary.\n\
\n\
pub make(): Dynamic ->\n\
    Ok(1).\n",
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
        Some(CoreExpr::ConstructorCall {
            constructor: "Ok".to_string(),
            constructor_identity: None,
            args: vec![CoreExpr::Int(1)],
        })
    );
    assert_eq!(core.metadata.resolved_constructor_call_identity_count, 0);
    assert_eq!(core.metadata.unresolved_constructor_call_candidate_count, 1);
    assert!(
        core.contract_text().contains("ConstructorCall(Ok;Int(1))"),
        "contract text: {}",
        core.contract_text()
    );
    assert!(
        core.contract_text()
            .contains("unresolved_constructor_call_candidate:1"),
        "contract text: {}",
        core.contract_text()
    );
}

/// Verifies eligible local type-alias constructor patterns carry CoreIR
/// identity.
///
/// Inputs:
/// - None; constructs a syntax-output module with a single-shape `Ok[T]`
///   alias and a `case` branch matching `Ok(value)`.
///
/// Output:
/// - Test passes when the Core pattern has
///   `constructor_identity = Some("Ok")` and no unresolved constructor
///   pattern candidates.
///
/// Transformation:
/// - Exercises the same post-lowering constructor identity pass for
///   single-shape type-alias patterns that typechecking already accepts.
#[test]
fn syntax_output_lowering_to_core_resolves_local_alias_constructor_pattern_identity() {
    let module = parse_module_as_syntax_output(
        "\
module core_alias_constructor_pattern_identity_boundary.\n\
\n\
pub type Ok[T] = {:ok, value: T}.\n\
\n\
pub unwrap(input: Ok[Int]): Int ->\n\
    case input {\n\
        Ok(value) -> value\n\
    }.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let diagnostics = type_check_syntax_module_output(&module, &resolved);
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "unwrap")
        .expect("core unwrap function");
    let Some(CoreExpr::Case { clauses, .. }) = &function.clauses[0].body.core_expr else {
        panic!(
            "expected case body: {:?}",
            function.clauses[0].body.core_expr
        );
    };
    let CorePattern::Constructor {
        name,
        constructor_identity,
        args,
    } = &clauses[0].pattern
    else {
        panic!("expected constructor pattern: {:?}", clauses[0].pattern);
    };

    assert_eq!(name, "Ok");
    assert_eq!(constructor_identity.as_deref(), Some("Ok"));
    assert_eq!(args.len(), 1);
    assert_eq!(core.metadata.resolved_constructor_pattern_identity_count, 1);
    assert_eq!(
        core.metadata.unresolved_constructor_pattern_candidate_count,
        0
    );
    assert!(
        core.contract_text()
            .contains("Constructor(Ok;identity=Ok;Var(value))"),
        "contract text: {}",
        core.contract_text()
    );
}

/// Verifies eligible directly imported type-alias constructor patterns
/// carry qualified CoreIR identity.
///
/// Inputs:
/// - A provider interface declaring public alias constructor `Ok`.
/// - A consumer syntax-output module importing `Ok` directly and matching
///   `Ok(value)`.
///
/// Output:
/// - Test passes when CoreIR preserves the source-visible pattern head `Ok`
///   and resolves the identity to `provider.Ok`.
///
/// Transformation:
/// - Resolves the direct type import against an explicit interface map,
///   lowers to CoreIR, and verifies imported single-shape type-alias
///   constructor-pattern identity metadata without using a local import
///   alias.
#[test]
fn syntax_output_lowering_to_core_resolves_direct_imported_alias_constructor_pattern_identity() {
    let provider = parse_interface_module_as_syntax_output(
        "\
module provider.\n\
\n\
pub type Ok[T] = {:ok, value: T}.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse provider interface fixture: {:?}", err));
    let mut interfaces = HashMap::new();
    interfaces.insert(
        provider.module_name.clone(),
        syntax_module_output_to_interface(&provider),
    );
    let module = parse_module_as_syntax_output(
        "\
module direct_imported_alias_constructor_pattern_identity_boundary.\n\
\n\
import provider.{Ok}.\n\
\n\
pub unwrap(input: Ok[Int]): Int ->\n\
    case input {\n\
        Ok(value) -> value\n\
    }.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output_with_interfaces(&module, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&module, &resolved);
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "unwrap")
        .expect("core unwrap function");
    let Some(CoreExpr::Case { clauses, .. }) = &function.clauses[0].body.core_expr else {
        panic!(
            "expected case body: {:?}",
            function.clauses[0].body.core_expr
        );
    };
    let CorePattern::Constructor {
        name,
        constructor_identity,
        args,
    } = &clauses[0].pattern
    else {
        panic!("expected constructor pattern: {:?}", clauses[0].pattern);
    };

    assert_eq!(name, "Ok");
    assert_eq!(constructor_identity.as_deref(), Some("provider.Ok"));
    assert_eq!(args.len(), 1);
    assert_eq!(core.metadata.resolved_constructor_pattern_identity_count, 1);
    assert_eq!(
        core.metadata.unresolved_constructor_pattern_candidate_count,
        0
    );
    assert!(
        core.contract_text()
            .contains("Constructor(Ok;identity=provider.Ok;Var(value))"),
        "contract text: {}",
        core.contract_text()
    );
}

/// Verifies eligible imported type-alias constructor patterns carry
/// qualified CoreIR identity.
///
/// Inputs:
/// - A provider interface declaring public alias constructor `Ok`.
/// - A consumer syntax-output module importing `Ok as Success` and matching
///   `Success(value)`.
///
/// Output:
/// - Test passes when CoreIR preserves the source-visible pattern head
///   `Success` and resolves the identity to `provider.Ok`.
///
/// Transformation:
/// - Resolves the aliased import against an explicit interface map, lowers
///   to CoreIR, and verifies single-shape type-alias constructor-pattern
///   identity metadata is based on the provider/source alias.
#[test]
fn syntax_output_lowering_to_core_resolves_imported_alias_constructor_pattern_identity() {
    let provider = parse_interface_module_as_syntax_output(
        "\
module provider.\n\
\n\
pub type Ok[T] = {:ok, value: T}.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse provider interface fixture: {:?}", err));
    let mut interfaces = HashMap::new();
    interfaces.insert(
        provider.module_name.clone(),
        syntax_module_output_to_interface(&provider),
    );
    let module = parse_module_as_syntax_output(
        "\
module imported_alias_constructor_pattern_identity_boundary.\n\
\n\
import provider.{Ok as Success}.\n\
\n\
pub unwrap(input: Success[Int]): Int ->\n\
    case input {\n\
        Success(value) -> value\n\
    }.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output_with_interfaces(&module, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&module, &resolved);
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "unwrap")
        .expect("core unwrap function");
    let Some(CoreExpr::Case { clauses, .. }) = &function.clauses[0].body.core_expr else {
        panic!(
            "expected case body: {:?}",
            function.clauses[0].body.core_expr
        );
    };
    let CorePattern::Constructor {
        name,
        constructor_identity,
        args,
    } = &clauses[0].pattern
    else {
        panic!("expected constructor pattern: {:?}", clauses[0].pattern);
    };

    assert_eq!(name, "Success");
    assert_eq!(constructor_identity.as_deref(), Some("provider.Ok"));
    assert_eq!(args.len(), 1);
    assert_eq!(core.metadata.resolved_constructor_pattern_identity_count, 1);
    assert_eq!(
        core.metadata.unresolved_constructor_pattern_candidate_count,
        0
    );
    assert!(
        core.contract_text()
            .contains("Constructor(Success;identity=provider.Ok;Var(value))"),
        "contract text: {}",
        core.contract_text()
    );
}

#[test]
fn syntax_output_lowering_to_core_resolves_declared_constructor_pattern_identity() {
    let module = parse_module_as_syntax_output(
        "\
module core_constructor_pattern_identity_boundary.\n\
\n\
pub constructor Some {\n\
    (value: Dynamic): Dynamic -> {:some, value}\n\
}.\n\
\n\
pub unwrap(input: Dynamic): Dynamic ->\n\
    case input {\n\
        Some(value) -> value\n\
    }.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "unwrap")
        .expect("core unwrap function");
    assert_eq!(function.clauses.len(), 1);
    let Some(CoreExpr::Case { clauses, .. }) = &function.clauses[0].body.core_expr else {
        panic!(
            "expected case core expr: {:?}",
            function.clauses[0].body.core_expr
        );
    };
    assert_eq!(clauses.len(), 1);
    assert_eq!(
        clauses[0].pattern,
        CorePattern::Constructor {
            name: "Some".to_string(),
            constructor_identity: Some("Some".to_string()),
            args: vec![CorePattern::Var("value".to_string())],
        }
    );
    assert_eq!(core.metadata.resolved_constructor_call_identity_count, 0);
    assert_eq!(core.metadata.resolved_constructor_chain_identity_count, 0);
    assert_eq!(core.metadata.resolved_constructor_pattern_identity_count, 1);
    assert_eq!(core.metadata.unresolved_constructor_call_candidate_count, 0);
    assert_eq!(
        core.metadata.unresolved_constructor_chain_candidate_count,
        0
    );
    assert_eq!(
        core.metadata.unresolved_constructor_pattern_candidate_count,
        0
    );
    assert!(
        core.contract_text()
            .contains("Constructor(Some;identity=Some;Var(value))"),
        "contract text: {}",
        core.contract_text()
    );
    assert!(
        core.contract_text()
            .contains("resolved_constructor_pattern_identity:1"),
        "contract text: {}",
        core.contract_text()
    );
    assert!(
        core.contract_text().contains(
            "target=Case(Var(input);Constructor(Some;identity=Some;Var(value))=>Var(value))"
        ),
        "contract text: {}",
        core.contract_text()
    );
}

/// Verifies imported public constructor patterns carry qualified CoreIR
/// identity.
///
/// Inputs:
/// - A provider interface declaring public constructor `Some`.
/// - A consumer syntax-output module importing `Some` and matching it in a
///   case expression.
///
/// Output:
/// - Test passes when the case pattern is annotated with
///   `constructor_identity = Some("provider.Some")`.
///
/// Transformation:
/// - Resolves the consumer against an explicit interface map, lowers it to
///   CoreIR, and verifies imported constructor-pattern identity metadata.
#[test]
fn syntax_output_lowering_to_core_resolves_imported_constructor_pattern_identity() {
    let provider = parse_interface_module_as_syntax_output(
        "\
module provider.\n\
\n\
pub constructor Some {\n\
    (value: Dynamic): Dynamic -> value\n\
}.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse provider interface fixture: {:?}", err));
    let mut interfaces = HashMap::new();
    interfaces.insert(
        provider.module_name.clone(),
        syntax_module_output_to_interface(&provider),
    );
    let module = parse_module_as_syntax_output(
        "\
module imported_constructor_pattern_identity_boundary.\n\
\n\
import provider.{Some}.\n\
\n\
pub unwrap(input: Dynamic): Dynamic ->\n\
    case input {\n\
        Some(value) -> value\n\
    }.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output_with_interfaces(&module, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&module, &resolved);
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "unwrap")
        .expect("core unwrap function");
    let Some(CoreExpr::Case { clauses, .. }) = &function.clauses[0].body.core_expr else {
        panic!(
            "expected case core expr: {:?}",
            function.clauses[0].body.core_expr
        );
    };
    let CorePattern::Constructor {
        name,
        constructor_identity,
        args,
    } = &clauses[0].pattern
    else {
        panic!("expected constructor pattern: {:?}", clauses[0].pattern);
    };
    assert_eq!(name, "Some");
    assert_eq!(constructor_identity.as_deref(), Some("provider.Some"));
    assert_eq!(args, &vec![CorePattern::Var("value".to_string())]);
    assert_eq!(core.metadata.resolved_constructor_pattern_identity_count, 1);
    assert_eq!(
        core.metadata.unresolved_constructor_pattern_candidate_count,
        0
    );
    assert!(
        core.contract_text()
            .contains("Constructor(Some;identity=provider.Some;Var(value))"),
        "contract text: {}",
        core.contract_text()
    );
}

/// Verifies aliased imported public constructor patterns carry source identity.
///
/// Inputs:
/// - A provider interface declaring public constructor `Some`.
/// - A consumer syntax-output module importing `Some as Maybe` and matching
///   `Maybe(value)` in a case expression.
///
/// Output:
/// - Test passes when the CoreIR pattern preserves the source-visible head
///   `Maybe` and annotates it with `constructor_identity =
///   Some("provider.Some")`.
///
/// Transformation:
/// - Resolves the aliased pattern import against an explicit interface map,
///   lowers to CoreIR, and verifies pattern identity metadata is based on
///   the provider/source constructor rather than the local alias.
#[test]
fn syntax_output_lowering_to_core_resolves_aliased_imported_constructor_pattern_identity() {
    let provider = parse_interface_module_as_syntax_output(
        "\
module provider.\n\
\n\
pub constructor Some {\n\
    (value: Dynamic): Dynamic -> value\n\
}.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse provider interface fixture: {:?}", err));
    let mut interfaces = HashMap::new();
    interfaces.insert(
        provider.module_name.clone(),
        syntax_module_output_to_interface(&provider),
    );
    let module = parse_module_as_syntax_output(
        "\
module aliased_imported_constructor_pattern_identity_boundary.\n\
\n\
import provider.{Some as Maybe}.\n\
\n\
pub unwrap(input: Dynamic): Dynamic ->\n\
    case input {\n\
        Maybe(value) -> value\n\
    }.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output_with_interfaces(&module, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&module, &resolved);
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "unwrap")
        .expect("core unwrap function");
    let Some(CoreExpr::Case { clauses, .. }) = &function.clauses[0].body.core_expr else {
        panic!(
            "expected case core expr: {:?}",
            function.clauses[0].body.core_expr
        );
    };
    let CorePattern::Constructor {
        name,
        constructor_identity,
        args,
    } = &clauses[0].pattern
    else {
        panic!("expected constructor pattern: {:?}", clauses[0].pattern);
    };
    assert_eq!(name, "Maybe");
    assert_eq!(constructor_identity.as_deref(), Some("provider.Some"));
    assert_eq!(args, &vec![CorePattern::Var("value".to_string())]);
    assert_eq!(core.metadata.resolved_constructor_pattern_identity_count, 1);
    assert_eq!(
        core.metadata.unresolved_constructor_pattern_candidate_count,
        0
    );
    assert!(
        core.contract_text()
            .contains("Constructor(Maybe;identity=provider.Some;Var(value))"),
        "contract text: {}",
        core.contract_text()
    );
}

/// Verifies constructor-chain identity states remain partial proof coverage.
///
/// Inputs:
/// - One `CoreExpr::ConstructorChain` with no resolved base constructor
///   identity.
/// - One `CoreExpr::ConstructorChain` with a resolved base constructor
///   identity.
///
/// Output:
/// - Test passes when both payloads report `Partial` coverage and remain
///   outside the current Lean-modeled expression subset.
///
/// Transformation:
/// - Exercises the named constructor-chain proof policy without parsing a
///   source fixture, keeping identity resolution and proof promotion as
///   separate compiler decisions.
#[test]
fn syntax_output_lowering_to_core_constructor_chain_policy_stays_partial_for_identity_states() {
    let unresolved_chain = CoreExpr::ConstructorChain {
        base: "User".to_string(),
        base_constructor_identity: None,
        args: vec![CoreExpr::Var("id".to_string())],
        record: Box::new(CoreExpr::RecordConstruct {
            name: "Admin".to_string(),
            fields: vec![CoreRecordExprField {
                key: "id".to_string(),
                required: true,
                value: CoreExpr::Var("id".to_string()),
            }],
        }),
    };
    let resolved_chain = CoreExpr::ConstructorChain {
        base: "User".to_string(),
        base_constructor_identity: Some("User".to_string()),
        args: vec![CoreExpr::Var("id".to_string())],
        record: Box::new(CoreExpr::RecordConstruct {
            name: "Admin".to_string(),
            fields: vec![CoreRecordExprField {
                key: "id".to_string(),
                required: true,
                value: CoreExpr::Var("id".to_string()),
            }],
        }),
    };

    for core_expr in [&unresolved_chain, &resolved_chain] {
        assert_eq!(
            constructor_chain_proof_coverage_policy(Some(core_expr)),
            CoreProofCoverage::Partial
        );
        assert!(!core_expr_is_lean_modeled(core_expr));
    }
}

#[test]
fn syntax_output_lowering_to_core_constructor_chain_expr() {
    let module = parse_module_as_syntax_output(
        "\
module core_constructor_chain_expr_boundary.\n\
\n\
pub constructor User {\n\
    (id: Int, name: Binary): Dynamic -> id\n\
}.\n\
\n\
pub make(id: Int, name: Binary): Dynamic ->\n\
    User(id, name) with Admin { id = id, name = name }.\n",
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
    let Some(CoreExpr::ConstructorChain {
        base,
        base_constructor_identity,
        args,
        record,
    }) = &function.clauses[0].body.core_expr
    else {
        panic!(
            "expected constructor chain core expr: {:?}",
            function.clauses[0].body.core_expr
        );
    };
    assert_eq!(base, "User");
    assert_eq!(base_constructor_identity.as_deref(), Some("User"));
    assert_eq!(
        args,
        &vec![
            CoreExpr::Var("id".to_string()),
            CoreExpr::Var("name".to_string())
        ]
    );
    assert_eq!(
        record.as_ref(),
        &CoreExpr::RecordConstruct {
            name: "Admin".to_string(),
            fields: vec![
                CoreRecordExprField {
                    key: "id".to_string(),
                    required: true,
                    value: CoreExpr::Var("id".to_string()),
                },
                CoreRecordExprField {
                    key: "name".to_string(),
                    required: true,
                    value: CoreExpr::Var("name".to_string()),
                },
            ],
        }
    );
    assert_eq!(
        function.clauses[0].body.proof_coverage,
        CoreProofCoverage::Partial
    );
    assert_eq!(
        constructor_chain_proof_coverage_policy(function.clauses[0].body.core_expr.as_ref()),
        CoreProofCoverage::Partial
    );
    assert_eq!(core.metadata.resolved_constructor_call_identity_count, 1);
    assert_eq!(core.metadata.resolved_constructor_chain_identity_count, 1);
    assert_eq!(core.metadata.resolved_constructor_pattern_identity_count, 0);
    assert_eq!(core.metadata.unresolved_constructor_call_candidate_count, 0);
    assert_eq!(
        core.metadata.unresolved_constructor_chain_candidate_count,
        0
    );
    assert_eq!(
        core.metadata.unresolved_constructor_pattern_candidate_count,
        0
    );
    assert!(
            core.contract_text().contains(
                "ConstructorChain(User;identity=User;Var(id),Var(name) with RecordConstruct(Admin;id=Var(id),name=Var(name)))"
            ),
            "contract text: {}",
            core.contract_text()
        );
    assert!(
        core.contract_text()
            .contains("resolved_constructor_chain_identity:1"),
        "contract text: {}",
        core.contract_text()
    );
}

/// Verifies eligible local type-alias constructor-chain bases carry CoreIR
/// identity.
///
/// Inputs:
/// - None; constructs a syntax-output module with `pub type User =
///   {:user, id: Int, name: Binary}` and uses `User(id, name)` as a
///   constructor-chain base.
///
/// Output:
/// - Test passes when the constructor-chain base has
///   `base_constructor_identity = Some("User")`, the nested constructor
///   call identity is resolved, and no unresolved chain candidates remain.
///
/// Transformation:
/// - Resolves and lowers a type-alias constructor-chain through the same
///   CoreIR identity annotation pass used for declared constructors,
///   without promoting constructor chains to Lean-covered proof status.
#[test]
fn syntax_output_lowering_to_core_resolves_local_alias_constructor_chain_identity() {
    let module = parse_module_as_syntax_output(
        "\
module core_alias_constructor_chain_identity_boundary.\n\
\n\
pub type User = {:user, id: Int, name: Binary}.\n\
\n\
pub make(id: Int, name: Binary): Dynamic ->\n\
    User(id, name) with Admin { id = id, name = name }.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    let diagnostics = type_check_syntax_module_output(&module, &resolved);
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "make")
        .expect("core make function");
    let Some(CoreExpr::ConstructorChain {
        base,
        base_constructor_identity,
        ..
    }) = &function.clauses[0].body.core_expr
    else {
        panic!(
            "expected constructor chain core expr: {:?}",
            function.clauses[0].body.core_expr
        );
    };
    assert_eq!(base, "User");
    assert_eq!(base_constructor_identity.as_deref(), Some("User"));
    assert_eq!(
        function.clauses[0].body.proof_coverage,
        CoreProofCoverage::Partial
    );
    assert_eq!(core.metadata.resolved_constructor_call_identity_count, 1);
    assert_eq!(core.metadata.resolved_constructor_chain_identity_count, 1);
    assert_eq!(
        core.metadata.unresolved_constructor_chain_candidate_count,
        0
    );
    assert!(
            core.contract_text().contains(
                "ConstructorChain(User;identity=User;Var(id),Var(name) with RecordConstruct(Admin;id=Var(id),name=Var(name)))"
            ),
            "contract text: {}",
            core.contract_text()
        );
}

/// Verifies eligible directly imported type-alias constructor-chain bases
/// carry qualified CoreIR identity.
///
/// Inputs:
/// - A provider interface declaring public alias constructor `User`.
/// - A consumer syntax-output module importing `User` directly and using
///   `User` as the constructor-chain base.
///
/// Output:
/// - Test passes when CoreIR preserves the source-visible base `User`,
///   annotates it with `base_constructor_identity = Some("provider.User")`,
///   and reports no unresolved constructor-chain candidates.
///
/// Transformation:
/// - Resolves the direct type import against an explicit interface map,
///   lowers to CoreIR, and verifies single-shape type-alias
///   constructor-chain identity metadata without using a local import alias.
#[test]
fn syntax_output_lowering_to_core_resolves_direct_imported_alias_constructor_chain_identity() {
    let provider = parse_interface_module_as_syntax_output(
        "\
module provider.\n\
\n\
pub type User = {:user, id: Int, name: Binary}.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse provider interface fixture: {:?}", err));
    let mut interfaces = HashMap::new();
    interfaces.insert(
        provider.module_name.clone(),
        syntax_module_output_to_interface(&provider),
    );
    let module = parse_module_as_syntax_output(
        "\
module direct_imported_alias_constructor_chain_identity_boundary.\n\
\n\
import provider.{User}.\n\
\n\
pub make(id: Int, name: Binary): Dynamic ->\n\
    User(id, name) with Admin { id = id, name = name }.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output_with_interfaces(&module, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&module, &resolved);
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "make")
        .expect("core make function");
    let Some(CoreExpr::ConstructorChain {
        base,
        base_constructor_identity,
        ..
    }) = &function.clauses[0].body.core_expr
    else {
        panic!(
            "expected constructor chain core expr: {:?}",
            function.clauses[0].body.core_expr
        );
    };
    assert_eq!(base, "User");
    assert_eq!(base_constructor_identity.as_deref(), Some("provider.User"));
    assert_eq!(
        function.clauses[0].body.proof_coverage,
        CoreProofCoverage::Partial
    );
    assert_eq!(core.metadata.resolved_constructor_call_identity_count, 1);
    assert_eq!(core.metadata.resolved_constructor_chain_identity_count, 1);
    assert_eq!(
        core.metadata.unresolved_constructor_chain_candidate_count,
        0
    );
    assert!(
            core.contract_text().contains(
                "ConstructorChain(User;identity=provider.User;Var(id),Var(name) with RecordConstruct(Admin;id=Var(id),name=Var(name)))"
            ),
            "contract text: {}",
            core.contract_text()
        );
}

/// Verifies imported public constructor-chain bases carry qualified CoreIR
/// identity.
///
/// Inputs:
/// - A provider interface declaring public constructor `User`.
/// - A consumer syntax-output module importing `User` and using it as a
///   constructor-chain base.
///
/// Output:
/// - Test passes when the constructor-chain base is annotated with
///   `base_constructor_identity = Some("provider.User")`.
///
/// Transformation:
/// - Resolves the consumer against an explicit interface map, lowers it to
///   CoreIR, and verifies imported constructor-chain identity metadata
///   without promoting constructor chains to Lean-covered proof status.
#[test]
fn syntax_output_lowering_to_core_resolves_imported_constructor_chain_identity() {
    let provider = parse_interface_module_as_syntax_output(
        "\
module provider.\n\
\n\
pub constructor User {\n\
    (id: Int, name: Binary): Dynamic -> id\n\
}.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse provider interface fixture: {:?}", err));
    let mut interfaces = HashMap::new();
    interfaces.insert(
        provider.module_name.clone(),
        syntax_module_output_to_interface(&provider),
    );
    let module = parse_module_as_syntax_output(
        "\
module imported_constructor_chain_identity_boundary.\n\
\n\
import provider.{User}.\n\
\n\
pub make(id: Int, name: Binary): Dynamic ->\n\
    User(id, name) with Admin { id = id, name = name }.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output_with_interfaces(&module, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&module, &resolved);
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "make")
        .expect("core make function");
    let Some(CoreExpr::ConstructorChain {
        base,
        base_constructor_identity,
        ..
    }) = &function.clauses[0].body.core_expr
    else {
        panic!(
            "expected constructor chain core expr: {:?}",
            function.clauses[0].body.core_expr
        );
    };
    assert_eq!(base, "User");
    assert_eq!(base_constructor_identity.as_deref(), Some("provider.User"));
    assert_eq!(
        function.clauses[0].body.proof_coverage,
        CoreProofCoverage::Partial
    );
    assert_eq!(core.metadata.resolved_constructor_call_identity_count, 1);
    assert_eq!(core.metadata.resolved_constructor_chain_identity_count, 1);
    assert_eq!(
        core.metadata.unresolved_constructor_chain_candidate_count,
        0
    );
    assert!(
            core.contract_text().contains(
                "ConstructorChain(User;identity=provider.User;Var(id),Var(name) with RecordConstruct(Admin;id=Var(id),name=Var(name)))"
            ),
            "contract text: {}",
            core.contract_text()
        );
}

/// Verifies aliased imported constructor-chain bases carry source identity.
///
/// Inputs:
/// - A provider interface declaring public constructor `User`.
/// - A consumer syntax-output module importing `User as Member` and using
///   `Member` as the constructor-chain base.
///
/// Output:
/// - Test passes when CoreIR preserves the source-visible base `Member`,
///   annotates it with `base_constructor_identity = Some("provider.User")`,
///   and keeps constructor-chain proof coverage partial.
///
/// Transformation:
/// - Resolves the aliased import against an explicit interface map, lowers
///   to CoreIR, and verifies constructor-chain identity metadata is based on
///   the provider/source constructor rather than the local alias.
#[test]
fn syntax_output_lowering_to_core_resolves_aliased_imported_constructor_chain_identity() {
    let provider = parse_interface_module_as_syntax_output(
        "\
module provider.\n\
\n\
pub constructor User {\n\
    (id: Int, name: Binary): Dynamic -> id\n\
}.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse provider interface fixture: {:?}", err));
    let mut interfaces = HashMap::new();
    interfaces.insert(
        provider.module_name.clone(),
        syntax_module_output_to_interface(&provider),
    );
    let module = parse_module_as_syntax_output(
        "\
module aliased_imported_constructor_chain_identity_boundary.\n\
\n\
import provider.{User as Member}.\n\
\n\
pub make(id: Int, name: Binary): Dynamic ->\n\
    Member(id, name) with Admin { id = id, name = name }.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output_with_interfaces(&module, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&module, &resolved);
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "make")
        .expect("core make function");
    let Some(CoreExpr::ConstructorChain {
        base,
        base_constructor_identity,
        ..
    }) = &function.clauses[0].body.core_expr
    else {
        panic!(
            "expected constructor chain core expr: {:?}",
            function.clauses[0].body.core_expr
        );
    };
    assert_eq!(base, "Member");
    assert_eq!(base_constructor_identity.as_deref(), Some("provider.User"));
    assert_eq!(
        function.clauses[0].body.proof_coverage,
        CoreProofCoverage::Partial
    );
    assert_eq!(core.metadata.resolved_constructor_call_identity_count, 1);
    assert_eq!(core.metadata.resolved_constructor_chain_identity_count, 1);
    assert_eq!(
        core.metadata.unresolved_constructor_chain_candidate_count,
        0
    );
    assert!(
            core.contract_text().contains(
                "ConstructorChain(Member;identity=provider.User;Var(id),Var(name) with RecordConstruct(Admin;id=Var(id),name=Var(name)))"
            ),
            "contract text: {}",
            core.contract_text()
        );
}

/// Verifies eligible imported type-alias constructor-chain bases carry
/// qualified CoreIR identity.
///
/// Inputs:
/// - A provider interface declaring public alias constructor `User`.
/// - A consumer syntax-output module importing `User as Member` and using
///   `Member` as the constructor-chain base.
///
/// Output:
/// - Test passes when CoreIR preserves the source-visible base `Member`,
///   annotates it with `base_constructor_identity = Some("provider.User")`,
///   and reports no unresolved constructor-chain candidates.
///
/// Transformation:
/// - Resolves the aliased type import against an explicit interface map,
///   lowers to CoreIR, and verifies single-shape type-alias
///   constructor-chain identity metadata is provider-qualified.
#[test]
fn syntax_output_lowering_to_core_resolves_imported_alias_constructor_chain_identity() {
    let provider = parse_interface_module_as_syntax_output(
        "\
module provider.\n\
\n\
pub type User = {:user, id: Int, name: Binary}.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse provider interface fixture: {:?}", err));
    let mut interfaces = HashMap::new();
    interfaces.insert(
        provider.module_name.clone(),
        syntax_module_output_to_interface(&provider),
    );
    let module = parse_module_as_syntax_output(
        "\
module imported_alias_constructor_chain_identity_boundary.\n\
\n\
import provider.{User as Member}.\n\
\n\
pub make(id: Int, name: Binary): Dynamic ->\n\
    Member(id, name) with Admin { id = id, name = name }.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output_with_interfaces(&module, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&module, &resolved);
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    let function = core
        .functions
        .iter()
        .find(|function| function.name == "make")
        .expect("core make function");
    let Some(CoreExpr::ConstructorChain {
        base,
        base_constructor_identity,
        ..
    }) = &function.clauses[0].body.core_expr
    else {
        panic!(
            "expected constructor chain core expr: {:?}",
            function.clauses[0].body.core_expr
        );
    };
    assert_eq!(base, "Member");
    assert_eq!(base_constructor_identity.as_deref(), Some("provider.User"));
    assert_eq!(
        function.clauses[0].body.proof_coverage,
        CoreProofCoverage::Partial
    );
    assert_eq!(core.metadata.resolved_constructor_call_identity_count, 1);
    assert_eq!(core.metadata.resolved_constructor_chain_identity_count, 1);
    assert_eq!(
        core.metadata.unresolved_constructor_chain_candidate_count,
        0
    );
    assert!(
            core.contract_text().contains(
                "ConstructorChain(Member;identity=provider.User;Var(id),Var(name) with RecordConstruct(Admin;id=Var(id),name=Var(name)))"
            ),
            "contract text: {}",
            core.contract_text()
        );
}

#[test]
fn syntax_output_declared_constructor_patterns_are_valid_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module constructor_patterns.\n\
pub constructor None {\n\
    (): Dynamic -> :none\n\
}.\n\
pub constructor Some {\n\
    (value: Dynamic): Dynamic -> {:some, value}\n\
}.\n\
pub unwrap(input: Dynamic): Dynamic ->\n\
    case input {\n\
        None -> :none;\n\
        Some(value) -> value;\n\
        :error -> :error\n\
    }.\n\
",
    );
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_raw_atom_patterns_do_not_require_constructor_declarations_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module raw_atom_patterns.\n\
pub unwrap(input: Dynamic): Dynamic ->\n\
    case input {\n\
        :none -> :none;\n\
        :empty -> :empty\n\
    }.\n\
",
    );
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_declared_constructor_calls_are_valid_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module constructor_calls.\n\
pub constructor Some {\n\
    (value: Dynamic): Dynamic -> {:some, value}\n\
}.\n\
pub make(value: Dynamic): Dynamic ->\n\
    Some(value).\n\
",
    );
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_single_shape_alias_constructor_calls_are_valid_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module alias_constructor_calls.\n\
pub type Ok[T] = {:ok, value: T}.\n\
pub make(value: Int): Dynamic ->\n\
    Ok(value).\n\
",
    );
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_single_shape_alias_constructor_patterns_are_valid_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module alias_constructor_patterns.\n\
pub type Ok[T] = {:ok, value: T}.\n\
pub unwrap(input: Ok[Int]): Int ->\n\
    case input {\n\
        Ok(value) -> value\n\
    }.\n\
",
    );
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_literal_alias_constructor_patterns_are_valid_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module alias_literal_patterns.\n\
pub type None = Atom[\"none\"].\n\
pub unwrap(input: None): Dynamic ->\n\
    case input {\n\
        None -> :ok\n\
    }.\n\
",
    );
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies atom-literal aliases compare against their literal runtime
/// value.
///
/// Inputs:
/// - A syntax-output module defining `Unit = Atom["unit"]`.
/// - A public function returning `Unit`.
/// - A comparison between the function result and `:unit`.
///
/// Output:
/// - Test passes when syntax-output typechecking accepts the comparison
///   without diagnostics.
///
/// Transformation:
/// - Runs the formal syntax-output typechecker and confirms binary
///   comparison inference expands transparent aliases before rejecting
///   otherwise distinct operand spellings.
#[test]
fn syntax_output_literal_aliases_compare_with_literal_values_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module alias_literal_comparisons.\n\
pub type Unit = Atom[\"unit\"].\n\
pub value(): Unit ->\n\
    Unit.\n\
pub matches(): Bool ->\n\
    value() == Unit.\n\
",
    );
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies atom aliases compare against canonical atom literal values.
///
/// Inputs:
/// - A singleton alias `Ready = Atom["ready"]`.
/// - A comparison between the alias value and the canonical literal.
///
/// Output:
/// - Empty typecheck diagnostics.
///
/// Transformation:
/// - Resolves the alias value to its singleton atom representation and
///   unifies it with the explicit `Atom["ready"]` expression form.
#[test]
fn syntax_output_atom_aliases_compare_with_canonical_atom_literal_values() {
    let diagnostics = check_syntax_output(
        "\
module atom_alias_literal_comparisons.\n\
pub type Ready = Atom[\"ready\"].\n\
pub value(): Ready ->\n\
    Ready.\n\
pub matches(): Bool ->\n\
    value() == Atom[\"ready\"].\n\
",
    );
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies canonical atom literals do not become unit synonyms.
///
/// Inputs:
/// - A function returning `Atom["unit"]` as an `Atom`.
///
/// Output:
/// - Empty typecheck diagnostics.
///
/// Transformation:
/// - Confirms only bare lowercase `unit` is rejected; the explicit atom
///   literal spelling remains an ordinary symbolic atom value.
#[test]
fn syntax_output_accepts_canonical_unit_named_atom_literal() {
    let diagnostics = check_syntax_output(
        "\
module canonical_unit_named_atom_literal.\n\
pub value(): Atom ->\n\
    Atom[\"unit\"].\n\
",
    );
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_literal_alias_values_are_valid_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module alias_literal_values.\n\
pub type None = Atom[\"none\"].\n\
pub none(): None ->\n\
    None.\n\
",
    );
    assert!(diagnostics.is_empty(), "diagnostics: {:?}", diagnostics);
}

#[test]
fn syntax_output_quoted_atom_alias_constructor_patterns_are_valid_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module alias_quoted_literal_patterns.\n\
pub type ModuleAtom = :'Elixir.Module'.\n\
pub unwrap(input: ModuleAtom): Dynamic ->\n\
    case input {\n\
        ModuleAtom -> :ok\n\
    }.\n\
",
    );
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_supports_constructor_chain_now() {
    let diagnostics = check_syntax_output(
        "\
module syntax_constructor_chain_expr.\n\
pub type User = Dynamic.\n\
pub constructor User {\n\
    (id: Int, name: Binary): Dynamic ->\n\
        id\n\
}.\n\
pub demo(id: Int, name: Binary): Dynamic ->\n\
    User(id, name) with Admin { id = id, name = name }.\n\
",
    );

    assert!(diagnostics.is_empty(), "diagnostics: {:?}", diagnostics);
}
