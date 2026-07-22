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
///   {Atom["ok"], value: T}` and a function body that calls `Ok(1)`.
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
pub type Ok[T] = {Atom[\"ok\"], value: T}.\n\
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
pub type Ok[T] = {Atom[\"ok\"], value: T}.\n",
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
pub type Ok[T] = {Atom[\"ok\"], value: T}.\n",
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
pub type Ok[T] = {Atom[\"ok\"], value: T}.\n\
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
pub type Ok[T] = {Atom[\"ok\"], value: T}.\n",
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
pub type Ok[T] = {Atom[\"ok\"], value: T}.\n",
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
