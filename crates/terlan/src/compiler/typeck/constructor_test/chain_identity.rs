use std::collections::HashMap;

use super::*;
use crate::terlan_hir::{
    resolve_syntax_module_output, resolve_syntax_module_output_with_interfaces,
    syntax_module_output_to_interface,
};
use crate::terlan_syntax::{
    parse_interface_module_as_syntax_output, parse_module_as_syntax_output,
};

#[test]
fn syntax_output_lowering_to_core_resolves_declared_constructor_pattern_identity() {
    let module = parse_module_as_syntax_output(
        "\
module core_constructor_pattern_identity_boundary.\n\
\n\
pub constructor Some {\n\
    (value: Dynamic): Dynamic -> {Atom[\"some\"], value}\n\
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
    User(id, name) with Admin { id: id, name: name }.\n",
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
///   {Atom["user"], id: Int, name: Binary}` and uses `User(id, name)` as a
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
pub type User = {Atom[\"user\"], id: Int, name: Binary}.\n\
\n\
pub make(id: Int, name: Binary): Dynamic ->\n\
    User(id, name) with Admin { id: id, name: name }.\n",
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
pub type User = {Atom[\"user\"], id: Int, name: Binary}.\n",
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
    User(id, name) with Admin { id: id, name: name }.\n",
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
    User(id, name) with Admin { id: id, name: name }.\n",
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
    Member(id, name) with Admin { id: id, name: name }.\n",
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
pub type User = {Atom[\"user\"], id: Int, name: Binary}.\n",
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
    Member(id, name) with Admin { id: id, name: name }.\n",
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

#[test]
fn syntax_output_declared_constructor_patterns_are_valid_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module constructor_patterns.\n\
pub constructor None {\n\
    (): Dynamic -> Atom[\"none\"]\n\
}.\n\
pub constructor Some {\n\
    (value: Dynamic): Dynamic -> {Atom[\"some\"], value}\n\
}.\n\
pub unwrap(input: Dynamic): Dynamic ->\n\
    case input {\n\
        None -> Atom[\"none\"];\n\
        Some(value) -> value;\n\
        Atom[\"error\"] -> Atom[\"error\"]\n\
    }.\n\
",
    );
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}
