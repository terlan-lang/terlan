use std::collections::BTreeMap;

use crate::terlan_syntax::{
    SyntaxExprFieldOutput, SyntaxExprKind, SyntaxExprOutput, SyntaxModuleOutput,
};

use super::{find_syntax_template_props, StaticSyntaxRenderError};

/// Converts a known template call into template-instantiation fields.
///
/// Inputs:
/// - `module`: syntax-output module containing template declarations.
/// - `expr`: syntax-output expression that may be a call.
///
/// Output:
/// - Template name and generated prop fields when the call target is a known
///   template.
/// - `None` when the expression is not a direct template call.
///
/// Transformation:
/// - Uses positional arguments for declaration-order props, named arguments for
///   exact prop names, and fills omitted trailing props from template defaults.
pub(super) fn syntax_static_template_call_fields(
    module: &SyntaxModuleOutput,
    expr: &SyntaxExprOutput,
) -> Result<Option<(String, Vec<SyntaxExprFieldOutput>)>, StaticSyntaxRenderError> {
    let Some(name) = syntax_static_template_call_name(module, expr) else {
        return Ok(None);
    };
    let template_props = find_syntax_template_props(module, name).ok_or_else(|| {
        StaticSyntaxRenderError::Invalid(format!("unknown static template `{}`", name))
    })?;
    let args = expr.children.iter().skip(1).collect::<Vec<_>>();

    let mut fields = Vec::new();
    let mut seen = BTreeMap::<String, ()>::new();
    let mut next_positional_index = 0;
    for (index, arg) in args.into_iter().enumerate() {
        let key = template_call_arg_key(
            name,
            template_props,
            expr,
            index,
            &mut next_positional_index,
        )?;

        if seen.insert(key.clone(), ()).is_some() {
            return Err(StaticSyntaxRenderError::Invalid(format!(
                "static template `{}` prop `{}` was provided more than once",
                name, key
            )));
        }
        fields.push(SyntaxExprFieldOutput {
            key,
            required: true,
            value: Box::new(arg.clone()),
        });
    }

    append_template_call_default_fields(name, template_props, &seen, &mut fields)?;

    Ok(Some((name.to_string(), fields)))
}

/// Resolves the prop key for one template-call argument.
///
/// Inputs:
/// - `template_name`: template name for diagnostics.
/// - `template_props`: declaration-order template prop list.
/// - `expr`: original call expression containing argument-name metadata.
/// - `index`: zero-based argument index.
/// - `next_positional_index`: mutable declaration prop cursor for positional
///   arguments.
///
/// Output:
/// - Prop key assigned to the call argument.
///
/// Transformation:
/// - Uses explicit call-site names when present, otherwise consumes the next
///   declaration-order prop name.
fn template_call_arg_key(
    template_name: &str,
    template_props: &[crate::terlan_syntax::SyntaxTemplatePropOutput],
    expr: &SyntaxExprOutput,
    index: usize,
    next_positional_index: &mut usize,
) -> Result<String, StaticSyntaxRenderError> {
    if let Some(arg_name) = expr.arg_names.get(index).and_then(Option::as_ref) {
        if !template_props.iter().any(|prop| prop.name == *arg_name) {
            return Err(StaticSyntaxRenderError::Invalid(format!(
                "static template `{}` has no prop `{}`",
                template_name, arg_name
            )));
        }
        return Ok(arg_name.clone());
    }

    let Some(prop) = template_props.get(*next_positional_index) else {
        return Err(StaticSyntaxRenderError::Invalid(format!(
            "static template `{}` expected at most {} arguments, got {}",
            template_name,
            template_props.len(),
            index + 1
        )));
    };
    *next_positional_index += 1;
    Ok(prop.name.clone())
}

/// Appends defaulted fields for omitted template-call props.
///
/// Inputs:
/// - `template_name`: template name for diagnostics.
/// - `template_props`: declaration-order template prop list.
/// - `seen`: props already supplied by the call.
/// - `fields`: generated field list to extend.
///
/// Output:
/// - `Ok(())` after all required/defaulted props are represented.
///
/// Transformation:
/// - Rejects missing required props and clones default expressions for omitted
///   defaulted props.
fn append_template_call_default_fields(
    template_name: &str,
    template_props: &[crate::terlan_syntax::SyntaxTemplatePropOutput],
    seen: &BTreeMap<String, ()>,
    fields: &mut Vec<SyntaxExprFieldOutput>,
) -> Result<(), StaticSyntaxRenderError> {
    for prop in template_props {
        if seen.contains_key(&prop.name) {
            continue;
        }
        let Some(default) = &prop.default else {
            return Err(StaticSyntaxRenderError::Invalid(format!(
                "static template `{}` is missing required prop `{}`",
                template_name, prop.name
            )));
        };
        fields.push(SyntaxExprFieldOutput {
            key: prop.name.clone(),
            required: true,
            value: Box::new(default.clone()),
        });
    }
    Ok(())
}

/// Returns the template name referenced by a direct template call.
///
/// Inputs:
/// - `module`: syntax-output module containing template declarations.
/// - `expr`: syntax-output expression to inspect.
///
/// Output:
/// - Template name when `expr` is a direct `Name(...)` call to a declared
///   template, otherwise `None`.
///
/// Transformation:
/// - Rejects remote and function-value calls and treats only a variable callee
///   matching a template declaration as a generated template function.
fn syntax_static_template_call_name<'a>(
    module: &SyntaxModuleOutput,
    expr: &'a SyntaxExprOutput,
) -> Option<&'a str> {
    if expr.kind != SyntaxExprKind::Call || expr.remote.is_some() {
        return None;
    }
    let callee = expr.children.first()?;
    if callee.kind != SyntaxExprKind::Var {
        return None;
    }
    let name = callee.text.as_deref()?;
    find_syntax_template_props(module, name)?;
    Some(name)
}
