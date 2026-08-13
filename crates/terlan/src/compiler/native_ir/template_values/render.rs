//! Compile-time lowering of checked template render plans.

use std::collections::BTreeMap;

use crate::terlan_typeck::{
    CoreExpr, CoreRecordExprField, CoreTemplateRenderPlan, CoreType, CoreTypeDecl,
};

use super::{is_template_html_type, join_literal_fragments, managed_call};

const CHILDREN_SLOT: &str = "children";

/// One expression paired with its exact checked rendering type.
#[derive(Clone)]
struct RenderValue {
    expr: CoreExpr,
    ty: CoreType,
}

/// Shared immutable inputs and mutable output buffers for one render walk.
struct RenderTraversal<'a> {
    plan: &'a CoreTemplateRenderPlan,
    templates: &'a [CoreTemplateRenderPlan],
    types: &'a [CoreTypeDecl],
    values: &'a BTreeMap<String, RenderValue>,
    stack: &'a mut Vec<String>,
    static_text: &'a mut String,
    fragments: &'a mut Vec<CoreExpr>,
}

/// Compiles one checked template instantiation into managed string operations.
pub(super) fn render_template_instantiation(
    name: &str,
    fields: &[CoreRecordExprField],
    templates: &[CoreTemplateRenderPlan],
    types: &[CoreTypeDecl],
) -> Result<CoreExpr, String> {
    let plan = template_by_name(name, templates)?;
    let values = supplied_values(plan, fields)?;
    render_plan(plan, templates, types, &values, &mut Vec::new())
}

/// Renders one plan while rejecting recursive component expansion.
fn render_plan(
    plan: &CoreTemplateRenderPlan,
    templates: &[CoreTemplateRenderPlan],
    types: &[CoreTypeDecl],
    values: &BTreeMap<String, RenderValue>,
    stack: &mut Vec<String>,
) -> Result<CoreExpr, String> {
    if stack.iter().any(|name| name == &plan.name) {
        return Err(format!(
            "error[native_ir.template_component_cycle]: component expansion revisits template `{}`",
            plan.name
        ));
    }
    stack.push(plan.name.clone());
    let mut fragments = Vec::new();
    let mut static_text = String::new();
    render_nodes(
        &plan.template.nodes,
        &mut RenderTraversal {
            plan,
            templates,
            types,
            values,
            stack,
            static_text: &mut static_text,
            fragments: &mut fragments,
        },
    )?;
    stack.pop();
    flush_static(&mut static_text, &mut fragments);
    Ok(join_literal_fragments(&fragments))
}

/// Resolves supplied and defaulted top-level props into typed values.
fn supplied_values(
    plan: &CoreTemplateRenderPlan,
    fields: &[CoreRecordExprField],
) -> Result<BTreeMap<String, RenderValue>, String> {
    let mut values = BTreeMap::new();
    for field in fields {
        let prop = plan
            .props
            .iter()
            .find(|prop| prop.name == field.key)
            .ok_or_else(|| {
                format!(
                    "error[native_ir.template_prop_unknown]: template `{}` has no prop `{}`",
                    plan.name, field.key
                )
            })?;
        if values
            .insert(
                field.key.clone(),
                RenderValue {
                    expr: field.value.clone(),
                    ty: prop.ty.clone(),
                },
            )
            .is_some()
        {
            return Err(format!(
                "error[native_ir.template_prop_duplicate]: template `{}` prop `{}` is supplied more than once",
                plan.name, field.key
            ));
        }
    }
    add_defaults(plan, &mut values)?;
    Ok(values)
}

/// Adds declaration defaults and rejects missing required props.
fn add_defaults(
    plan: &CoreTemplateRenderPlan,
    values: &mut BTreeMap<String, RenderValue>,
) -> Result<(), String> {
    for prop in &plan.props {
        if values.contains_key(&prop.name) {
            continue;
        }
        let expr = prop.default.clone().ok_or_else(|| {
            format!(
                "error[native_ir.template_prop_missing]: template `{}` requires prop `{}`",
                plan.name, prop.name
            )
        })?;
        values.insert(
            prop.name.clone(),
            RenderValue {
                expr,
                ty: prop.ty.clone(),
            },
        );
    }
    Ok(())
}

/// Renders one ordered node sequence into static and dynamic fragments.
fn render_nodes(
    nodes: &[crate::terlan_html::HtmlNode],
    traversal: &mut RenderTraversal<'_>,
) -> Result<(), String> {
    for node in nodes {
        render_node(node, traversal)?;
    }
    Ok(())
}

/// Renders one validated template node without reopening source.
fn render_node(
    node: &crate::terlan_html::HtmlNode,
    traversal: &mut RenderTraversal<'_>,
) -> Result<(), String> {
    let RenderTraversal {
        plan,
        templates,
        types,
        values,
        stack,
        static_text,
        fragments,
    } = traversal;
    match node {
        crate::terlan_html::HtmlNode::Text(text) => static_text.push_str(text),
        crate::terlan_html::HtmlNode::Comment(text) => {
            static_text.push_str("<!--");
            static_text.push_str(text);
            static_text.push_str("-->");
        }
        crate::terlan_html::HtmlNode::Doctype(text) => {
            static_text.push_str("<!DOCTYPE ");
            static_text.push_str(text);
            static_text.push('>');
        }
        crate::terlan_html::HtmlNode::Slot(slot) => {
            let value = slot_value(slot, plan, values, types)?;
            push_rendered(value, None, static_text, fragments)?;
        }
        crate::terlan_html::HtmlNode::Element(element) => {
            if let Some(component) = template_by_tag(&element.name, templates) {
                flush_static(static_text, fragments);
                fragments.push(render_component(
                    element, component, plan, templates, types, values, stack,
                )?);
                return Ok(());
            }
            static_text.push('<');
            static_text.push_str(&element.name);
            for attr in &element.attrs {
                match &attr.value {
                    None => {
                        static_text.push(' ');
                        static_text.push_str(&attr.name);
                    }
                    Some(crate::terlan_html::HtmlAttrValue::Text(value)) => {
                        static_text.push(' ');
                        static_text.push_str(&attr.name);
                        static_text.push_str("=\"");
                        static_text.push_str(&crate::terlan_html::escape_html_attr(value));
                        static_text.push('"');
                    }
                    Some(crate::terlan_html::HtmlAttrValue::Slot(slot)) => {
                        let value = slot_value(slot, plan, values, types)?;
                        push_rendered(value, Some(&attr.name), static_text, fragments)?;
                    }
                }
            }
            static_text.push('>');
            render_nodes(
                &element.children,
                &mut RenderTraversal {
                    plan,
                    templates,
                    types,
                    values,
                    stack,
                    static_text,
                    fragments,
                },
            )?;
            static_text.push_str("</");
            static_text.push_str(&element.name);
            static_text.push('>');
        }
    }
    Ok(())
}

/// Inlines one checked component plan with typed props and trusted children.
fn render_component(
    element: &crate::terlan_html::HtmlElement,
    component: &CoreTemplateRenderPlan,
    owner: &CoreTemplateRenderPlan,
    templates: &[CoreTemplateRenderPlan],
    types: &[CoreTypeDecl],
    owner_values: &BTreeMap<String, RenderValue>,
    stack: &mut Vec<String>,
) -> Result<CoreExpr, String> {
    let mut values = BTreeMap::new();
    for attr in &element.attrs {
        let prop = component
            .props
            .iter()
            .find(|prop| prop.name == attr.name)
            .ok_or_else(|| {
                format!(
                    "error[native_ir.template_component_prop_unknown]: component `{}` has no prop `{}`",
                    element.name, attr.name
                )
            })?;
        let value = match &attr.value {
            Some(crate::terlan_html::HtmlAttrValue::Text(value)) => RenderValue {
                expr: CoreExpr::Binary(
                    serde_json::to_string(value).expect("String always serializes as JSON"),
                ),
                ty: CoreType::String,
            },
            Some(crate::terlan_html::HtmlAttrValue::Slot(slot)) => {
                slot_value(slot, owner, owner_values, types)?
            }
            None => {
                return Err(format!(
                    "error[native_ir.template_component_prop_value]: component `{}` prop `{}` requires a value",
                    element.name, attr.name
                ))
            }
        };
        if value.ty != prop.ty {
            return Err(format!(
                "error[native_ir.template_component_prop_type]: component `{}` prop `{}` requires `{}`, found `{}`",
                element.name,
                attr.name,
                prop.ty.contract_text(),
                value.ty.contract_text()
            ));
        }
        values.insert(attr.name.clone(), value);
    }
    add_defaults(component, &mut values)?;

    let mut child_static = String::new();
    let mut child_fragments = Vec::new();
    render_nodes(
        &element.children,
        &mut RenderTraversal {
            plan: owner,
            templates,
            types,
            values: owner_values,
            stack,
            static_text: &mut child_static,
            fragments: &mut child_fragments,
        },
    )?;
    flush_static(&mut child_static, &mut child_fragments);
    values.insert(
        CHILDREN_SLOT.to_string(),
        RenderValue {
            expr: join_literal_fragments(&child_fragments),
            ty: CoreType::Named("Template.Html".to_string()),
        },
    );
    render_plan(component, templates, types, &values, stack)
}

/// Resolves a direct path or checked expression island to one typed value.
fn slot_value(
    slot: &crate::terlan_html::HtmlSlot,
    plan: &CoreTemplateRenderPlan,
    values: &BTreeMap<String, RenderValue>,
    types: &[CoreTypeDecl],
) -> Result<RenderValue, String> {
    if slot.path.is_empty() {
        let expression = plan
            .expressions
            .iter()
            .find(|expression| expression.source == slot.expression)
            .ok_or_else(|| {
                format!(
                    "error[native_ir.template_expression_missing]: template `{}` has no checked expression `{}`",
                    plan.name, slot.expression
                )
            })?;
        return Ok(RenderValue {
            expr: substitute_expression(&expression.expr, values, types)?,
            ty: expression.ty.clone(),
        });
    }
    let root = &slot.path[0];
    let mut value = values.get(root).cloned().ok_or_else(|| {
        format!(
            "error[native_ir.template_slot_unknown]: template `{}` has no value `{root}`",
            plan.name
        )
    })?;
    for field in slot.path.iter().skip(1) {
        let ty = field_type(&value.ty, field, types).ok_or_else(|| {
            format!(
                "error[native_ir.template_slot_path]: template `{}` slot `{}` has no checked field `{field}`",
                plan.name, slot.expression
            )
        })?;
        let expr = project_inline_record_field(value.expr, field, types)?;
        value = RenderValue { expr, ty };
    }
    Ok(value)
}

/// Substitutes an inline record field or retains a managed runtime projection.
fn project_inline_record_field(
    base: CoreExpr,
    field: &str,
    types: &[CoreTypeDecl],
) -> Result<CoreExpr, String> {
    if let CoreExpr::RecordConstruct { name, fields } = &base {
        return fields
            .iter()
            .find(|candidate| candidate.key == field)
            .map(|candidate| candidate.value.clone())
            .ok_or_else(|| {
                format!(
                    "error[native_ir.template_slot_path]: record `{name}` has no supplied field `{field}`"
                )
            });
    }
    if let CoreExpr::ConstructorCall {
        constructor, args, ..
    } = &base
    {
        let fields = types
            .iter()
            .find(|decl| {
                decl.name == *constructor
                    || constructor.rsplit('.').next() == Some(decl.name.as_str())
            })
            .and_then(|decl| decl.core_body.as_ref())
            .and_then(|body| match body {
                CoreType::Struct { fields, .. } => Some(fields),
                _ => None,
            });
        if let Some(fields) = fields {
            let index = fields
                .iter()
                .position(|candidate| candidate.name == field)
                .ok_or_else(|| {
                    format!(
                        "error[native_ir.template_slot_path]: constructor `{constructor}` has no field `{field}`"
                    )
                })?;
            return args.get(index).cloned().ok_or_else(|| {
                format!(
                    "error[native_ir.template_slot_path]: constructor `{constructor}` omits field `{field}`"
                )
            });
        }
    }
    Ok(CoreExpr::FieldAccess {
        base: Box::new(base),
        field: field.to_string(),
    })
}

/// Emits one typed text or whole-attribute rendering operation.
fn push_rendered(
    value: RenderValue,
    attribute: Option<&str>,
    static_text: &mut String,
    fragments: &mut Vec<CoreExpr>,
) -> Result<(), String> {
    if let Some(rendered) = render_literal_attribute(&value, attribute)? {
        static_text.push_str(&rendered);
        return Ok(());
    }
    flush_static(static_text, fragments);
    if attribute.is_none() && is_html_type(&value.ty) {
        fragments.push(value.expr);
        return Ok(());
    }
    let suffix = render_suffix(&value.ty).ok_or_else(|| {
        format!(
            "error[native_ir.template_render_type]: `{}` has no managed template renderer",
            value.ty.contract_text()
        )
    })?;
    let function = match attribute {
        Some(_) => format!("render_attribute_{suffix}"),
        None => format!("render_text_{suffix}"),
    };
    let mut args = Vec::with_capacity(2);
    if let Some(attribute) = attribute {
        args.push(CoreExpr::Binary(
            serde_json::to_string(attribute).expect("String always serializes as JSON"),
        ));
    }
    args.push(value.expr);
    fragments.push(managed_call(&function, args));
    Ok(())
}

/// Folds one literal attribute through the shared checked attribute renderer.
fn render_literal_attribute(
    value: &RenderValue,
    attribute: Option<&str>,
) -> Result<Option<String>, String> {
    let Some(attribute) = attribute else {
        return Ok(None);
    };
    let Some(mut value) = literal_attribute_value(&value.ty, &value.expr)? else {
        return Ok(None);
    };
    if crate::terlan_html::template_attribute_slot_kind(attribute)
        != crate::terlan_html::TemplateAttributeSlotKind::Boolean
    {
        value = match value {
            crate::terlan_html::TemplateAttributeValue::Boolean(value) => {
                crate::terlan_html::TemplateAttributeValue::Scalar(value.to_string())
            }
            value => value,
        };
    }
    crate::terlan_html::render_template_attribute(attribute, value)
        .map(|rendered| {
            rendered
                .map(|value| format!(" {value}"))
                .unwrap_or_default()
        })
        .map(Some)
        .map_err(|error| format!("error[native_ir.template_attribute]: {error}"))
}

/// Converts one checked literal into the backend-neutral attribute value.
fn literal_attribute_value(
    ty: &CoreType,
    expr: &CoreExpr,
) -> Result<Option<crate::terlan_html::TemplateAttributeValue>, String> {
    use crate::terlan_html::TemplateAttributeValue;

    let value = match (ty, expr) {
        (CoreType::String, CoreExpr::Binary(value)) => {
            TemplateAttributeValue::Scalar(serde_json::from_str(value).map_err(|error| {
                format!("error[native_ir.template_literal]: invalid String: {error}")
            })?)
        }
        (CoreType::Int, CoreExpr::Int(value)) => TemplateAttributeValue::Scalar(value.to_string()),
        (CoreType::Float, CoreExpr::Float(value)) => TemplateAttributeValue::Scalar(value.clone()),
        (CoreType::Bool, CoreExpr::Atom(value)) if value == "true" => {
            TemplateAttributeValue::Boolean(true)
        }
        (CoreType::Bool, CoreExpr::Atom(value)) if value == "false" => {
            TemplateAttributeValue::Boolean(false)
        }
        (CoreType::List(item_type), CoreExpr::List(items))
            if item_type.as_ref() == &CoreType::String =>
        {
            TemplateAttributeValue::Tokens(literal_string_list(items)?)
        }
        (
            CoreType::Apply { constructor, args },
            CoreExpr::ConstructorCall {
                constructor: value_constructor,
                args: values,
                ..
            },
        ) if constructor.rsplit('.').next() == Some("Option") && args.len() == 1 => {
            match (value_constructor.rsplit('.').next(), values.as_slice()) {
                (Some("None"), []) => TemplateAttributeValue::Missing,
                (Some("Some"), [value]) => {
                    let Some(value) = literal_attribute_value(&args[0], value)? else {
                        return Ok(None);
                    };
                    value
                }
                _ => return Ok(None),
            }
        }
        (CoreType::Apply { constructor, args }, CoreExpr::Tuple(values))
            if constructor.rsplit('.').next() == Some("Option") && args.len() == 1 =>
        {
            match values.as_slice() {
                [CoreExpr::Atom(tag), value] if tag.eq_ignore_ascii_case("some") => {
                    let Some(value) = literal_attribute_value(&args[0], value)? else {
                        return Ok(None);
                    };
                    value
                }
                _ => return Ok(None),
            }
        }
        (CoreType::Apply { constructor, args }, CoreExpr::Atom(value))
            if constructor.rsplit('.').next() == Some("Option")
                && args.len() == 1
                && value.eq_ignore_ascii_case("none") =>
        {
            TemplateAttributeValue::Missing
        }
        _ => return Ok(None),
    };
    Ok(Some(value))
}

/// Decodes one literal `List[String]` without admitting general list lowering.
fn literal_string_list(items: &[CoreExpr]) -> Result<Vec<String>, String> {
    items
        .iter()
        .map(|item| {
            let CoreExpr::Binary(value) = item else {
                return Err(
                    "error[native_ir.template_token_literal]: token-list literals require String elements"
                        .to_string(),
                );
            };
            serde_json::from_str(value).map_err(|error| {
                format!("error[native_ir.template_token_literal]: invalid String: {error}")
            })
        })
        .collect()
}

/// Selects the compiler-private renderer suffix for one checked value type.
fn render_suffix(ty: &CoreType) -> Option<&'static str> {
    match ty {
        CoreType::String => Some("string"),
        CoreType::Int => Some("int"),
        CoreType::Float => Some("float"),
        CoreType::Bool => Some("bool"),
        CoreType::List(item) if item.as_ref() == &CoreType::String => Some("string_list"),
        CoreType::Apply { constructor, args }
            if constructor.rsplit('.').next() == Some("Option") && args.len() == 1 =>
        {
            match args[0] {
                CoreType::String => Some("optional_string"),
                CoreType::Int => Some("optional_int"),
                CoreType::Float => Some("optional_float"),
                CoreType::Bool => Some("optional_bool"),
                CoreType::List(ref item) if item.as_ref() == &CoreType::String => {
                    Some("optional_string_list")
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// Substitutes template props into one validated scalar expression island.
fn substitute_expression(
    expr: &CoreExpr,
    values: &BTreeMap<String, RenderValue>,
    types: &[CoreTypeDecl],
) -> Result<CoreExpr, String> {
    Ok(match expr {
        CoreExpr::Var(name) if values.contains_key(name) => values[name].expr.clone(),
        CoreExpr::FieldAccess { base, field } => {
            project_inline_record_field(substitute_expression(base, values, types)?, field, types)?
        }
        CoreExpr::RecordAccess { base, name, field } => {
            let base = substitute_expression(base, values, types)?;
            if matches!(
                base,
                CoreExpr::RecordConstruct { .. } | CoreExpr::ConstructorCall { .. }
            ) {
                project_inline_record_field(base, field, types)?
            } else {
                CoreExpr::RecordAccess {
                    base: Box::new(base),
                    name: name.clone(),
                    field: field.clone(),
                }
            }
        }
        CoreExpr::Call { function, args } => CoreExpr::Call {
            function: function.clone(),
            args: substitute_args(args, values, types)?,
        },
        CoreExpr::RemoteCall {
            module,
            function,
            args,
        } => CoreExpr::RemoteCall {
            module: module.clone(),
            function: function.clone(),
            args: substitute_args(args, values, types)?,
        },
        CoreExpr::UnaryOp { operator, operand } => CoreExpr::UnaryOp {
            operator: operator.clone(),
            operand: Box::new(substitute_expression(operand, values, types)?),
        },
        CoreExpr::BinaryOp {
            operator,
            left,
            right,
        } => CoreExpr::BinaryOp {
            operator: operator.clone(),
            left: Box::new(substitute_expression(left, values, types)?),
            right: Box::new(substitute_expression(right, values, types)?),
        },
        CoreExpr::Cast { expr, target_type } => CoreExpr::Cast {
            expr: Box::new(substitute_expression(expr, values, types)?),
            target_type: target_type.clone(),
        },
        CoreExpr::If { clauses } => CoreExpr::If {
            clauses: clauses
                .iter()
                .map(|clause| {
                    let mut clause = clause.clone();
                    clause.condition = substitute_expression(&clause.condition, values, types)?;
                    clause.body = substitute_expression(&clause.body, values, types)?;
                    Ok(clause)
                })
                .collect::<Result<Vec<_>, String>>()?,
        },
        CoreExpr::Int(_)
        | CoreExpr::Float(_)
        | CoreExpr::Binary(_)
        | CoreExpr::Atom(_)
        | CoreExpr::Var(_) => expr.clone(),
        _ => {
            return Err(format!(
                "error[native_ir.template_expression_shape]: checked expression `{}` is not admitted for substitution",
                expr.contract_text()
            ))
        }
    })
}

/// Substitutes template props through ordered expression arguments.
fn substitute_args(
    args: &[CoreExpr],
    values: &BTreeMap<String, RenderValue>,
    types: &[CoreTypeDecl],
) -> Result<Vec<CoreExpr>, String> {
    args.iter()
        .map(|arg| substitute_expression(arg, values, types))
        .collect()
}

/// Resolves one nested field type from checked struct declarations.
fn field_type(base: &CoreType, field: &str, types: &[CoreTypeDecl]) -> Option<CoreType> {
    match base {
        CoreType::Struct { fields, .. } => fields
            .iter()
            .find(|candidate| candidate.name == field)
            .map(|candidate| candidate.ty.clone()),
        CoreType::Named(name) => types
            .iter()
            .find(|decl| decl.name == *name || name.rsplit('.').next() == Some(&decl.name))
            .and_then(|decl| decl.core_body.as_ref())
            .and_then(|body| field_type(body, field, types)),
        _ => None,
    }
}

/// Finds one checked template declaration by local source name.
fn template_by_name<'a>(
    name: &str,
    templates: &'a [CoreTemplateRenderPlan],
) -> Result<&'a CoreTemplateRenderPlan, String> {
    templates
        .iter()
        .find(|plan| plan.name == name)
        .ok_or_else(|| {
            format!(
            "error[native_ir.template_plan_missing]: template `{name}` has no checked render plan"
        )
        })
}

/// Finds one checked component declaration by its external tag identity.
fn template_by_tag<'a>(
    tag: &str,
    templates: &'a [CoreTemplateRenderPlan],
) -> Option<&'a CoreTemplateRenderPlan> {
    templates
        .iter()
        .find(|plan| plan.template.tag_name.as_deref() == Some(tag))
}

/// Reports whether one checked type is trusted template HTML.
fn is_html_type(ty: &CoreType) -> bool {
    matches!(ty, CoreType::Named(name) if is_template_html_type(name))
}

/// Flushes accumulated static markup into one managed string literal.
fn flush_static(static_text: &mut String, fragments: &mut Vec<CoreExpr>) {
    if static_text.is_empty() {
        return;
    }
    fragments.push(CoreExpr::Binary(
        serde_json::to_string(static_text).expect("String always serializes as JSON"),
    ));
    static_text.clear();
}
