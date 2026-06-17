use super::*;

/// Lowers resolved formal compiler state to the current core boundary.
///
/// Inputs:
/// - `resolved` compiler module produced by resolution and typechecking.
///
/// Output:
/// - Deterministic backend-neutral `CoreModule` payload.
///
/// Transformation:
/// - Copies the resolver interface into the core artifact and retains the
///   canonical module name.
/// - This function intentionally does not include backend-specific calls,
///   Erlang syntax, or JVM/JS encoding assumptions.
pub fn lower_resolved_module_to_core(resolved: &ResolvedModule) -> CoreModule {
    let imports = lower_core_imports(resolved);
    let exports = lower_core_exports(&resolved.interface);
    let types = lower_core_types(&resolved.interface);
    let functions = lower_core_functions(&resolved.interface);
    let constructors = lower_core_constructors(&resolved.interface);
    let metadata = core_module_metadata(&functions, &types, &constructors);

    CoreModule {
        schema: CORE_IR_SCHEMA.to_string(),
        module: resolved.name.clone(),
        source: CoreSourceIdentity {
            source_kind: "resolved_module".to_string(),
            syntax_contract_fingerprint: None,
        },
        imports,
        exports,
        types,
        functions,
        constructors,
        trait_conformances: Vec::new(),
        metadata,
        interface: resolved.interface.clone(),
    }
}

/// Lowers syntax-output plus resolved formal compiler state to CoreIR.
///
/// Inputs:
/// - `module`: compiler-facing syntax output produced from the canonical syntax
///   contract.
/// - `resolved`: resolver artifact after formal typechecking.
///
/// Output:
/// - Deterministic backend-neutral `CoreModule` payload with function clause
///   and expression summaries.
///
/// Transformation:
/// - Starts from the resolver/interface Core boundary, attaches syntax contract
///   identity, and overlays syntax-output function clauses as Core summaries
///   without encoding backend syntax or emitted Erlang forms.
pub fn lower_syntax_module_output_to_core(
    module: &SyntaxModuleOutput,
    resolved: &ResolvedModule,
) -> CoreModule {
    let mut core = lower_resolved_module_to_core(resolved);
    let expanded_interface = syntax_module_output_to_interface(module);
    core.interface.struct_fields = expanded_interface.struct_fields;
    core.source = CoreSourceIdentity {
        source_kind: format!("{:?}", module.source_kind),
        syntax_contract_fingerprint: Some(module.syntax_contract.fingerprint.clone()),
    };
    core.imports = core_syntax_imports(module);
    merge_core_imports(&mut core.imports, core_resolved_imported_modules(resolved));
    core.trait_conformances = core_syntax_trait_conformances(module);
    let syntax_struct_bodies = core_syntax_struct_type_bodies(module);
    for type_decl in &mut core.types {
        if let Some(core_body) = syntax_struct_bodies.get(&type_decl.name) {
            type_decl.core_body = Some(core_body.clone());
        }
    }

    let receiver_methods = core_receiver_method_dispatch_signatures(module, resolved);
    let mut function_clauses = core_syntax_function_clauses(module, &receiver_methods);
    let constructor_identities = core_constructor_identities(module, resolved, &core.constructors);
    resolve_constructor_identities_in_function_clauses(
        &mut function_clauses,
        &constructor_identities,
    );
    refresh_core_evidence_in_function_clauses(&mut function_clauses);
    for function in &mut core.functions {
        if let Some(clauses) = function_clauses.get(&(function.name.clone(), function.arity)) {
            function.clauses = clauses.clone();
        }
    }
    core.metadata = core_module_metadata(&core.functions, &core.types, &core.constructors);
    core
}

use super::core_interface::*;
use super::core_proof::*;
