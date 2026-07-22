use std::collections::HashMap;

use crate::terlan_hir::ResolvedModule;
use crate::terlan_syntax::{
    parse_module_as_syntax_output, SyntaxDeclarationPayload, SyntaxModuleOutput,
};
use crate::terlan_typeck::type_check_syntax_module_output;

/// Source-module state required to resolve a template expression island.
#[derive(Clone, Copy)]
pub(super) struct TemplateExpressionContext<'a> {
    module: &'a SyntaxModuleOutput,
    resolved: &'a ResolvedModule,
}

impl<'a> TemplateExpressionContext<'a> {
    /// Binds expression validation to one parsed and resolved source module.
    pub(super) fn new(module: &'a SyntaxModuleOutput, resolved: &'a ResolvedModule) -> Self {
        Self { module, resolved }
    }
}

/// Result of checking one interpolation expression against one render type.
pub(super) enum TemplateExpressionCheck {
    Valid,
    Impure(String),
    Invalid,
}

/// Checks an interpolation expression against one expected render type.
///
/// With source context, the expression is appended as a generated `@pure`
/// function so normal local/import resolution and fixed-point purity analysis
/// apply. Isolated context remains available for focused validator tests.
pub(super) fn check_template_expression_as(
    expression: &str,
    expected_type: &str,
    prop_types: &HashMap<String, String>,
    struct_fields: &HashMap<String, HashMap<String, String>>,
    context: Option<TemplateExpressionContext<'_>>,
) -> TemplateExpressionCheck {
    let Some(context) = context else {
        return if isolated_expression_typechecks_as(
            expression,
            expected_type,
            prop_types,
            struct_fields,
        ) {
            TemplateExpressionCheck::Valid
        } else {
            TemplateExpressionCheck::Invalid
        };
    };

    check_expression_in_module(expression, expected_type, prop_types, context)
}

/// Checks one expression by appending a collision-free pure function.
fn check_expression_in_module(
    expression: &str,
    expected_type: &str,
    prop_types: &HashMap<String, String>,
    context: TemplateExpressionContext<'_>,
) -> TemplateExpressionCheck {
    let checker_name = expression_checker_name(context.module);
    let source = expression_check_module(
        &checker_name,
        expression,
        expected_type,
        prop_types,
        &HashMap::new(),
        true,
    );
    let Ok(mut parsed) = parse_module_as_syntax_output(&source) else {
        return TemplateExpressionCheck::Invalid;
    };
    let Some(mut declaration) = parsed.declarations.pop() else {
        return TemplateExpressionCheck::Invalid;
    };
    let mut module = context.module.clone();
    declaration.index = module.declarations.len();
    module.declarations.push(declaration);
    let resolved = crate::terlan_hir::resolve_syntax_module_output_with_interfaces(
        &module,
        &context.resolved.interface_map,
    )
    .module;
    let diagnostics = type_check_syntax_module_output(&module, &resolved);
    let purity_prefix = format!("function {checker_name} annotated @pure must be pure; found ");
    if let Some(detail) = diagnostics
        .iter()
        .find_map(|diagnostic| diagnostic.message.strip_prefix(&purity_prefix))
    {
        return TemplateExpressionCheck::Impure(detail.to_string());
    }
    if diagnostics.is_empty() {
        TemplateExpressionCheck::Valid
    } else {
        TemplateExpressionCheck::Invalid
    }
}

/// Typechecks an expression in a standalone props-and-struct module.
fn isolated_expression_typechecks_as(
    expression: &str,
    expected_type: &str,
    prop_types: &HashMap<String, String>,
    struct_fields: &HashMap<String, HashMap<String, String>>,
) -> bool {
    let source = expression_check_module(
        "render",
        expression,
        expected_type,
        prop_types,
        struct_fields,
        false,
    );
    let Ok(module) = parse_module_as_syntax_output(&source) else {
        return false;
    };
    let resolved = crate::terlan_hir::resolve_syntax_module_output(&module).module;
    type_check_syntax_module_output(&module, &resolved).is_empty()
}

/// Builds a deterministic module containing one expression-check function.
fn expression_check_module(
    function_name: &str,
    expression: &str,
    expected_type: &str,
    prop_types: &HashMap<String, String>,
    struct_fields: &HashMap<String, HashMap<String, String>>,
    pure: bool,
) -> String {
    let mut source = String::from("module template_slot_expr_check.\n\n");
    let mut structs = struct_fields.iter().collect::<Vec<_>>();
    structs.sort_by_key(|(name, _)| name.as_str());
    for (name, fields) in structs {
        source.push_str(&format!("struct {name} {{\n"));
        let mut sorted_fields = fields.iter().collect::<Vec<_>>();
        sorted_fields.sort_by_key(|(field, _)| field.as_str());
        for (field, annotation) in sorted_fields {
            source.push_str(&format!("    {field}: {annotation},\n"));
        }
        source.push_str("}.\n\n");
    }

    let mut props = prop_types.iter().collect::<Vec<_>>();
    props.sort_by_key(|(name, _)| name.as_str());
    let params = props
        .into_iter()
        .map(|(name, annotation)| format!("{name}: {annotation}"))
        .collect::<Vec<_>>()
        .join(", ");
    if pure {
        source.push_str("@pure\n");
    }
    source.push_str(&format!(
        "pub {function_name}({params}): {expected_type} ->\n    {expression}.\n"
    ));
    source
}

/// Returns a generated function name absent from the source module.
fn expression_checker_name(module: &SyntaxModuleOutput) -> String {
    let mut suffix = 0usize;
    loop {
        let candidate = format!("terlan_template_slot_check_{suffix}");
        let exists = module.declarations.iter().any(|declaration| {
            matches!(
                &declaration.payload,
                SyntaxDeclarationPayload::Function { name, .. } if name == &candidate
            )
        });
        if !exists {
            return candidate;
        }
        suffix += 1;
    }
}
