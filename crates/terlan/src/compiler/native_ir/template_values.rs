//! Compiler-owned lowering for typed HTML fragment values.

use std::sync::Arc;

use crate::runtime::native_image::managed::{
    encode_string_append_operation, encode_string_escape_html_attribute_operation,
    encode_string_escape_html_text_operation, encode_string_list_join_operation,
    encode_template_render_operation, ManagedTemplateValueKind,
};
use crate::terlan_typeck::{
    CoreCaseClause, CoreExpr, CoreModule, CoreRecordExprField, CoreTemplateRenderPlan,
};

use super::{NativeExpr, NativeType};

const TEMPLATE_MODULE: &str = "std.template.Template";
const MANAGED_TEMPLATE_MODULE: &str = "$terlan.managed.template";

mod render;

/// Reports whether a checked named type is the public template HTML facade.
pub(super) fn is_template_html_type(name: &str) -> bool {
    matches!(name, "Template.Html" | "std.template.Template.Html")
}

/// Reports whether a remote-call owner names compiler-private template operations.
pub(super) fn is_managed_template_module(module: &str) -> bool {
    module == MANAGED_TEMPLATE_MODULE
}

/// Rewrites imported template fragment calls into target-owned managed operations.
pub(super) fn lower_template_values(core: &mut CoreModule) -> Result<(), String> {
    if core.templates.is_empty()
        && !core
            .imports
            .iter()
            .any(|import| import.module == TEMPLATE_MODULE)
    {
        return Ok(());
    }
    let templates = core.templates.clone();
    let types = core.types.clone();
    for function in &mut core.functions {
        for clause in &mut function.clauses {
            if let Some(body) = &mut clause.body.core_expr {
                *body = rewrite(body, &templates, &types)?;
            }
        }
    }
    Ok(())
}

/// Returns the exact result type of one compiler-private template operation.
pub(super) fn managed_template_operation_type(expr: &CoreExpr) -> Option<NativeType> {
    let CoreExpr::RemoteCall {
        module,
        function,
        args,
    } = expr
    else {
        return None;
    };
    let supported = matches!(
        (function.as_str(), args.len()),
        ("append", 2) | ("join" | "escape_text" | "escape_attribute", 1)
    ) || (args.len() == 1 && function.starts_with("render_text_"))
        || (args.len() == 2 && function.starts_with("render_attribute_"));
    (module == MANAGED_TEMPLATE_MODULE && supported).then_some(NativeType::StringRef)
}

/// Lowers one compiler-private template operation into managed NativeIR.
pub(super) fn lower_managed_template_operation(
    expr: &CoreExpr,
    mut lower: impl FnMut(&CoreExpr) -> Result<NativeExpr, String>,
) -> Result<Option<NativeExpr>, String> {
    let CoreExpr::RemoteCall {
        module,
        function,
        args,
    } = expr
    else {
        return Ok(None);
    };
    if module != MANAGED_TEMPLATE_MODULE {
        return Ok(None);
    }
    let encoded = match (function.as_str(), args.len()) {
        ("append", 2) => encode_string_append_operation(),
        ("join", 1) => encode_string_list_join_operation(),
        ("escape_text", 1) => encode_string_escape_html_text_operation(),
        ("escape_attribute", 1) => encode_string_escape_html_attribute_operation(),
        (function, 1) if function.starts_with("render_text_") => {
            encode_template_render_operation(render_value_kind(function, "render_text_")?, None)
                .map_err(|error| format!("error[native_ir.template_operation]: {error}"))?
        }
        (function, 2) if function.starts_with("render_attribute_") => {
            let attribute = core_string_literal(&args[0])?;
            encode_template_render_operation(
                render_value_kind(function, "render_attribute_")?,
                Some(&attribute),
            )
            .map_err(|error| format!("error[native_ir.template_operation]: {error}"))?
        }
        _ => {
            return Err(format!(
                "error[native_ir.template_operation]: unsupported managed template operation `{function}/{}`",
                args.len()
            ));
        }
    };
    Ok(Some(NativeExpr::ManagedOperation {
        encoded: Arc::from(encoded),
        args: args[usize::from(function.starts_with("render_attribute_"))..]
            .iter()
            .map(&mut lower)
            .collect::<Result<Vec<_>, _>>()?,
    }))
}

/// Decodes one compiler-private rendering suffix into its managed value kind.
fn render_value_kind(function: &str, prefix: &str) -> Result<ManagedTemplateValueKind, String> {
    match function.strip_prefix(prefix) {
        Some("string") => Ok(ManagedTemplateValueKind::String),
        Some("int") => Ok(ManagedTemplateValueKind::Int),
        Some("float") => Ok(ManagedTemplateValueKind::Float),
        Some("bool") => Ok(ManagedTemplateValueKind::Bool),
        Some("string_list") => Ok(ManagedTemplateValueKind::StringList),
        Some("optional_string") => Ok(ManagedTemplateValueKind::OptionalString),
        Some("optional_int") => Ok(ManagedTemplateValueKind::OptionalInt),
        Some("optional_float") => Ok(ManagedTemplateValueKind::OptionalFloat),
        Some("optional_bool") => Ok(ManagedTemplateValueKind::OptionalBool),
        Some("optional_string_list") => Ok(ManagedTemplateValueKind::OptionalStringList),
        _ => Err(format!(
            "error[native_ir.template_operation]: unsupported managed template operation `{function}`"
        )),
    }
}

/// Decodes one CoreIR managed string literal used as operation metadata.
fn core_string_literal(expr: &CoreExpr) -> Result<String, String> {
    let CoreExpr::Binary(value) = expr else {
        return Err(
            "error[native_ir.template_attribute]: attribute identity must be a string literal"
                .to_string(),
        );
    };
    serde_json::from_str(value)
        .map_err(|error| format!("error[native_ir.template_attribute]: {error}"))
}

/// Rewrites one expression after recursively normalizing all child expressions.
fn rewrite(
    expr: &CoreExpr,
    templates: &[CoreTemplateRenderPlan],
    types: &[crate::terlan_typeck::CoreTypeDecl],
) -> Result<CoreExpr, String> {
    let mut rewritten = expr.clone();
    rewrite_children(&mut rewritten, templates, types)?;
    if let CoreExpr::TemplateInstantiate { name, fields } = &rewritten {
        return render::render_template_instantiation(name, fields, templates, types);
    }
    let CoreExpr::RemoteCall {
        module,
        function,
        args,
    } = rewritten
    else {
        return Ok(rewritten);
    };
    if !matches!(module.as_str(), TEMPLATE_MODULE | "Template") {
        return Ok(CoreExpr::RemoteCall {
            module,
            function,
            args,
        });
    }
    match (function.as_str(), args.as_slice()) {
        ("trusted", [value]) => Ok(value.clone()),
        ("empty", []) => Ok(CoreExpr::Binary("\"\"".to_string())),
        ("join", [CoreExpr::List(fragments)]) => Ok(join_literal_fragments(fragments)),
        (
            "join",
            [CoreExpr::ConstructorCall {
                constructor,
                args: fragments,
                ..
            }],
        ) if constructor.rsplit('.').next() == Some("List") => {
            Ok(join_literal_fragments(fragments))
        }
        ("join", [fragments]) => Ok(managed_call("join", vec![fragments.clone()])),
        ("trusted" | "empty" | "join", _) => Err(format!(
            "error[native_ir.template_arity]: Template.{function} does not accept {} argument(s)",
            args.len()
        )),
        _ => Err(format!(
            "error[native_ir.template_function]: Template.{function}/{} is not in the managed template profile",
            args.len()
        )),
    }
}

/// Lowers one literal fragment sequence without requiring general list allocation.
pub(super) fn join_literal_fragments(fragments: &[CoreExpr]) -> CoreExpr {
    let mut fragments = fragments.iter().cloned();
    let Some(first) = fragments.next() else {
        return CoreExpr::Binary("\"\"".to_string());
    };
    fragments.fold(first, |joined, fragment| {
        managed_call("append", vec![joined, fragment])
    })
}

/// Creates one compiler-private managed template call.
pub(super) fn managed_call(function: &str, args: Vec<CoreExpr>) -> CoreExpr {
    CoreExpr::RemoteCall {
        module: MANAGED_TEMPLATE_MODULE.to_string(),
        function: function.to_string(),
        args,
    }
}

/// Rewrites every direct child while retaining the enclosing CoreIR node.
fn rewrite_children(
    expr: &mut CoreExpr,
    templates: &[CoreTemplateRenderPlan],
    types: &[crate::terlan_typeck::CoreTypeDecl],
) -> Result<(), String> {
    match expr {
        CoreExpr::Tuple(items) | CoreExpr::List(items) | CoreExpr::FixedArray(items) => {
            rewrite_many(items, templates, types)?
        }
        CoreExpr::ListCons { head, tail }
        | CoreExpr::Index {
            base: head,
            index: tail,
        } => {
            **head = rewrite(head, templates, types)?;
            **tail = rewrite(tail, templates, types)?;
        }
        CoreExpr::ListComprehension {
            expr,
            generators,
            guards,
            ..
        } => {
            **expr = rewrite(expr, templates, types)?;
            for generator in generators {
                generator.source = rewrite(&generator.source, templates, types)?;
            }
            rewrite_many(guards, templates, types)?;
        }
        CoreExpr::Let { bindings, body } => {
            for binding in bindings {
                binding.value = rewrite(&binding.value, templates, types)?;
            }
            **body = rewrite(body, templates, types)?;
        }
        CoreExpr::Map(fields) => {
            for field in fields {
                field.value = rewrite(&field.value, templates, types)?;
            }
        }
        CoreExpr::RecordConstruct { fields, .. } | CoreExpr::TemplateInstantiate { fields, .. } => {
            rewrite_fields(fields, templates, types)?
        }
        CoreExpr::FieldAccess { base, .. } | CoreExpr::RecordAccess { base, .. } => {
            **base = rewrite(base, templates, types)?
        }
        CoreExpr::RecordUpdate { base, fields, .. } => {
            **base = rewrite(base, templates, types)?;
            rewrite_fields(fields, templates, types)?;
        }
        CoreExpr::ConstructorChain { args, record, .. } => {
            rewrite_many(args, templates, types)?;
            **record = rewrite(record, templates, types)?;
        }
        CoreExpr::RemoteCall { args, .. }
        | CoreExpr::ConstructorCall { args, .. }
        | CoreExpr::Call { args, .. } => rewrite_many(args, templates, types)?,
        CoreExpr::MutableReceiverCall { receiver, args, .. } => {
            **receiver = rewrite(receiver, templates, types)?;
            rewrite_many(args, templates, types)?;
        }
        CoreExpr::FunctionCall { callee, args } => {
            **callee = rewrite(callee, templates, types)?;
            rewrite_many(args, templates, types)?;
        }
        CoreExpr::Cast { expr, .. } => **expr = rewrite(expr, templates, types)?,
        CoreExpr::Intrinsic(call) => rewrite_many(&mut call.args, templates, types)?,
        CoreExpr::SqlQuery { parameters, .. } => rewrite_many(parameters, templates, types)?,
        CoreExpr::Case { scrutinee, clauses } => {
            **scrutinee = rewrite(scrutinee, templates, types)?;
            rewrite_clauses(clauses, templates, types)?;
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            **body = rewrite(body, templates, types)?;
            rewrite_clauses(of_clauses, templates, types)?;
            rewrite_clauses(catch_clauses, templates, types)?;
            if let Some(after) = after_clause {
                *after.trigger = rewrite(&after.trigger, templates, types)?;
                *after.body = rewrite(&after.body, templates, types)?;
            }
        }
        CoreExpr::If { clauses } => {
            for clause in clauses {
                clause.condition = rewrite(&clause.condition, templates, types)?;
                clause.body = rewrite(&clause.body, templates, types)?;
            }
        }
        CoreExpr::Lam { body, .. } => **body = rewrite(body, templates, types)?,
        CoreExpr::UnaryOp { operand, .. } => **operand = rewrite(operand, templates, types)?,
        CoreExpr::BinaryOp { left, right, .. } => {
            **left = rewrite(left, templates, types)?;
            **right = rewrite(right, templates, types)?;
        }
        CoreExpr::Int(_)
        | CoreExpr::Float(_)
        | CoreExpr::Binary(_)
        | CoreExpr::Atom(_)
        | CoreExpr::Var(_)
        | CoreExpr::RemoteFunRef { .. } => {}
    }
    Ok(())
}

/// Rewrites one ordered expression sequence.
fn rewrite_many(
    expressions: &mut [CoreExpr],
    templates: &[CoreTemplateRenderPlan],
    types: &[crate::terlan_typeck::CoreTypeDecl],
) -> Result<(), String> {
    for expression in expressions {
        *expression = rewrite(expression, templates, types)?;
    }
    Ok(())
}

/// Rewrites record-like field values in source order.
fn rewrite_fields(
    fields: &mut [CoreRecordExprField],
    templates: &[CoreTemplateRenderPlan],
    types: &[crate::terlan_typeck::CoreTypeDecl],
) -> Result<(), String> {
    for field in fields {
        field.value = rewrite(&field.value, templates, types)?;
    }
    Ok(())
}

/// Rewrites guards and bodies retained by case-like clauses.
fn rewrite_clauses(
    clauses: &mut [CoreCaseClause],
    templates: &[CoreTemplateRenderPlan],
    types: &[crate::terlan_typeck::CoreTypeDecl],
) -> Result<(), String> {
    for clause in clauses {
        if let Some(guard) = &mut clause.guard {
            *guard = rewrite(guard, templates, types)?;
        }
        clause.body = rewrite(&clause.body, templates, types)?;
    }
    Ok(())
}
