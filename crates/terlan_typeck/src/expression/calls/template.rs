use super::*;
use terlan_syntax::SyntaxExprFieldOutput;

/// Infers a direct call to a declared template as a generated template function.
///
/// Inputs:
/// - `function_name`: direct call callee name.
/// - `expr`: syntax-output call expression.
/// - `locals`, `ctx`, `subst`, and `errors`: active expression inference state.
///
/// Output:
/// - `Some(Type)` when `function_name` resolves to a declared template.
/// - `None` when the call is not a template call.
///
/// Transformation:
/// - Rewrites `Page("Home")` and `Page(title = "Home")` into a synthetic
///   template-instantiation expression, preserving default-property behavior
///   and existing template prop diagnostics.
pub(super) fn infer_syntax_template_call(
    function_name: &str,
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Option<Type> {
    let template = ctx.templates.get(function_name)?;
    let Some(fields) = syntax_template_call_fields(function_name, template, expr, errors) else {
        return Some(Type::Dynamic);
    };
    let mut template_expr = expr.clone();
    template_expr.kind = SyntaxExprKind::TemplateInstantiate;
    template_expr.text = Some(function_name.to_string());
    template_expr.remote = None;
    template_expr.arg_names.clear();
    template_expr.children.clear();
    template_expr.fields = fields;
    template_expr.arity = template_expr.fields.len();

    Some(infer_syntax_template_instantiation(
        &template_expr,
        locals,
        ctx,
        subst,
        errors,
    ))
}

/// Converts a direct template call into synthetic template fields.
///
/// Inputs:
/// - `template_name`: direct template call target.
/// - `template`: declared template scheme with prop order and expected types.
/// - `expr`: syntax-output call expression.
/// - `errors`: diagnostics sink for arity failures.
///
/// Output:
/// - Ordered synthetic fields, or `None` when positional arguments exceed the
///   generated template function's declared arity.
///
/// Transformation:
/// - Maps positional args to declaration-order props and named args to exact
///   prop names. Missing/defaulted props are left for the existing
///   template-instantiation checker.
fn syntax_template_call_fields(
    template_name: &str,
    template: &TemplateScheme,
    expr: &SyntaxExprOutput,
    errors: &mut Vec<String>,
) -> Option<Vec<SyntaxExprFieldOutput>> {
    let args = expr.children.iter().skip(1).collect::<Vec<_>>();
    if args.len() > template.prop_order.len() {
        errors.push(format!(
            "template `{}` expected at most {} arguments, found {}",
            template_name,
            template.prop_order.len(),
            args.len()
        ));
        return None;
    }

    let mut fields = Vec::with_capacity(args.len());
    let mut next_positional_index = 0;
    for (index, arg) in args.into_iter().enumerate() {
        let key = if let Some(arg_name) = expr.arg_names.get(index).and_then(Option::as_ref) {
            arg_name.clone()
        } else {
            let Some(prop_name) = template.prop_order.get(next_positional_index) else {
                errors.push(format!(
                    "template `{}` expected at most {} arguments, found {}",
                    template_name,
                    template.prop_order.len(),
                    index + 1
                ));
                return None;
            };
            next_positional_index += 1;
            prop_name.clone()
        };
        fields.push(SyntaxExprFieldOutput {
            key,
            required: true,
            value: Box::new(arg.clone()),
        });
    }

    Some(fields)
}
