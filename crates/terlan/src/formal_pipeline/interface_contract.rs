//! Independent handwritten interface contracts at source compilation boundaries.

use std::collections::HashMap;
use std::path::Path;

use crate::terlan_hir::{
    resolve_syntax_module_output_with_interfaces, syntax_module_output_to_interface,
    ModuleInterface,
};
use crate::terlan_syntax::{parse_interface_module_as_syntax_output, SyntaxModuleOutput};
use crate::terlan_typeck::{
    check_module_interface_contract, expand_syntax_includes, type_check_syntax_module_output,
    DiagSeverity, Diagnostic,
};

/// Expands source-visible interface shapes to a fixed point before consumers
/// resolve inherited fields. Declaration diagnostics remain owned by the full
/// checker; an invalid expansion is never installed in this graph.
pub(crate) fn expand_interface_graph(
    modules: &[SyntaxModuleOutput],
    interfaces: &mut HashMap<String, ModuleInterface>,
) {
    for _ in 0..modules.len() {
        let mut changed = false;
        for module in modules {
            let (mut expanded, errors) = crate::terlan_typeck::expand_syntax_macros_with_interfaces(
                module.clone(),
                interfaces,
            );
            if !errors.is_empty()
                || crate::terlan_hir::expand_syntax_shape_imports(&mut expanded, interfaces)
                    .is_err()
            {
                continue;
            }
            let resolved =
                resolve_syntax_module_output_with_interfaces(&expanded, interfaces).module;
            let (expanded, errors) = expand_syntax_includes(expanded, &resolved);
            if !errors.is_empty() {
                continue;
            }
            let interface = syntax_module_output_to_interface(&expanded);
            changed |= interfaces.get(&module.module_name).is_none_or(|current| {
                current.to_terlan_interface_type_text() != interface.to_terlan_interface_type_text()
            });
            interfaces.insert(module.module_name.clone(), interface);
        }
        if !changed {
            break;
        }
    }
}

/// Reads the sibling contract strictly. Generated `.typi` summaries are never
/// contracts, and malformed or unreadable `.terli` files cannot be ignored.
pub(crate) fn read_declared_interface(path: &Path) -> std::io::Result<Option<SyntaxModuleOutput>> {
    let contract_path = path.with_extension("terli");
    let source = match std::fs::read_to_string(&contract_path) {
        Ok(source) => source,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(std::io::Error::new(
                error.kind(),
                format!("cannot read interface {}: {error}", contract_path.display()),
            ));
        }
    };
    parse_interface_module_as_syntax_output(&source)
        .map(Some)
        .map_err(|error| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("invalid interface {}: {error:?}", contract_path.display()),
            )
        })
}

/// Prepares and validates a declaration using the same macro, shape, include,
/// resolver, and typechecker rules used by the main frontend.
pub(super) fn prepare_interface_module(
    module: SyntaxModuleOutput,
    interfaces: &HashMap<String, ModuleInterface>,
) -> (
    SyntaxModuleOutput,
    crate::terlan_hir::ResolvedModule,
    Vec<Diagnostic>,
) {
    let (mut module, mut diagnostics) =
        crate::terlan_typeck::expand_syntax_macros_with_interfaces(module, interfaces);
    if let Err(error) = crate::terlan_hir::expand_syntax_shape_imports(&mut module, interfaces) {
        diagnostics.push(Diagnostic {
            span: module.span.into(),
            message: format!("interface shape expansion failed: {error:?}"),
            severity: DiagSeverity::Error,
        });
    }
    let resolved = resolve_syntax_module_output_with_interfaces(&module, interfaces).module;
    diagnostics.extend(resolved.diagnostics.iter().map(|diagnostic| Diagnostic {
        span: diagnostic.span,
        message: diagnostic.message.clone(),
        severity: DiagSeverity::Error,
    }));
    let (module, include_errors) = expand_syntax_includes(module, &resolved);
    diagnostics.extend(include_errors);
    diagnostics.extend(type_check_syntax_module_output(&module, &resolved));
    (module, resolved, diagnostics)
}

/// Returns the exact surface validated against the implementation, so later
/// lowering does not reopen a possibly changed contract to determine exports.
/// Cache hits also revalidate this contract before reusing implementation code.
pub(crate) fn check_declared_interface_and_surface(
    path: &Path,
    module: &SyntaxModuleOutput,
    interfaces: &HashMap<String, ModuleInterface>,
) -> (Vec<Diagnostic>, Option<ModuleInterface>) {
    if path.extension().and_then(|extension| extension.to_str()) != Some("terl") {
        return (Vec::new(), None);
    }
    let contract = match read_declared_interface(path) {
        Ok(Some(contract)) => contract,
        Ok(None) => return (Vec::new(), None),
        Err(message) => {
            return (
                vec![Diagnostic {
                    span: module.span.into(),
                    message: message.to_string(),
                    severity: DiagSeverity::Error,
                }],
                None,
            );
        }
    };
    let (contract, resolved, mut diagnostics) = prepare_interface_module(contract, interfaces);
    let mut surface = None;
    if !diagnostics
        .iter()
        .any(|diagnostic| matches!(diagnostic.severity, DiagSeverity::Error))
    {
        let expected = syntax_module_output_to_interface(&contract);
        let implementation =
            resolve_syntax_module_output_with_interfaces(module, interfaces).module;
        diagnostics.extend(check_module_interface_contract(
            module,
            &expected,
            &resolved,
            &implementation,
        ));
        surface = Some(expected);
    }
    // The CLI is compiling the implementation here. Keep spans in that file
    // and identify the sibling contract in every diagnostic message.
    for diagnostic in &mut diagnostics {
        diagnostic.span = module.span.into();
        diagnostic.message = format!(
            "{}: {}",
            path.with_extension("terli").display(),
            diagnostic.message
        );
    }
    (diagnostics, surface)
}

/// Keeps compiled helper bodies available for local calls while restricting
/// backend exports and generated public summaries to the admitted contract.
pub(crate) fn restrict_module_exports(
    core: &mut crate::terlan_typeck::CoreModule,
    expected: &ModuleInterface,
) {
    use crate::terlan_typeck::CoreVisibility;
    for function in &mut core.functions {
        let key = (function.name.clone(), function.arity);
        if core.interface.functions.contains_key(&key) {
            function.public &= expected
                .function_overloads
                .get(&key)
                .is_some_and(|signatures| signatures.iter().any(|signature| signature.public));
        }
    }
    for (key, signature) in &mut core.interface.functions {
        if let Some(declared) = expected.functions.get(key) {
            *signature = declared.clone();
        } else {
            signature.public = false;
        }
    }
    for (key, signatures) in &mut core.interface.function_overloads {
        if let Some(declared) = expected.function_overloads.get(key) {
            *signatures = declared.clone();
        } else {
            for signature in signatures {
                signature.public = false;
            }
        }
    }
    for ty in &mut core.types {
        ty.visibility = if !expected.public_types.contains(&ty.name) {
            CoreVisibility::Private
        } else if core.interface.opaque_types.contains(&ty.name) {
            CoreVisibility::Opaque
        } else {
            CoreVisibility::Public
        };
    }
    for name in core
        .interface
        .public_types
        .difference(&expected.public_types)
    {
        core.interface.private_types.insert(name.clone());
    }
    core.interface
        .public_types
        .retain(|name| expected.public_types.contains(name));
    // Publish the declared representation of transparent public types. Local
    // CoreIR retains the implementation's aliases for code generation.
    for name in &expected.public_types {
        if let Some(params) = expected.type_params.get(name) {
            core.interface
                .type_params
                .insert(name.clone(), params.clone());
        }
        if expected.opaque_types.contains(name) {
            continue;
        }
        if let Some(body) = expected.type_bodies.get(name) {
            core.interface
                .type_bodies
                .insert(name.clone(), body.clone());
        }
        if let Some(fields) = expected.struct_fields.get(name) {
            core.interface
                .struct_fields
                .insert(name.clone(), fields.clone());
        }
        if let Some(union) = expected.valued_unions.get(name) {
            core.interface
                .valued_unions
                .insert(name.clone(), union.clone());
        }
    }
    for constructor in &mut core.constructors {
        constructor.public &= expected
            .constructors
            .get(&constructor.name)
            .is_some_and(|signatures| signatures.iter().any(|signature| signature.public));
    }
    for (name, signatures) in &mut core.interface.constructors {
        if !expected.constructors.contains_key(name) {
            for signature in signatures {
                signature.public = false;
            }
        }
    }
    core.interface
        .constants
        .retain(|key, _| expected.constants.contains_key(key));
    core.interface
        .const_functions
        .retain(|key, _| expected.const_functions.contains_key(key));
    core.interface
        .expression_macros
        .retain(|key, _| expected.expression_macros.contains_key(key));
    core.interface
        .shapes
        .retain(|key, _| expected.shapes.contains_key(key));
    core.interface
        .traits
        .retain(|key, _| expected.traits.contains_key(key));
    core.interface
        .trait_conformances
        .retain(|item| expected.trait_conformances.contains(item));
    core.interface
        .associated_constants
        .retain(|key, _| expected.associated_constants.contains_key(key));
    core.exports = crate::terlan_typeck::core_interface::lower_core_exports(&core.interface);
}
