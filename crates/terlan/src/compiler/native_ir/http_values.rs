//! Compiler-owned managed HTTP value normalization for direct AOT handlers.

use std::sync::Arc;

use crate::runtime::native_image::managed::{
    encode_aggregate_append_pair_operation, encode_aggregate_append_value_operation,
    encode_aggregate_field_operation, encode_aggregate_replace_field_operation,
    encode_collection_layout, encode_cookie_header_operation, encode_list_empty_operation,
    encode_response_cookie_jar_operation, encode_response_security_headers_operation,
    encode_session_option_is_none_operation, encode_string_append_operation,
    encode_string_equal_operation, encode_string_map_get_option_operation,
    encode_string_prepend_literal_operation, encode_string_prepend_projected_literal_operation,
    ManagedCollectionDescriptor, ManagedCookieHeaderOperation, ManagedFieldType, SemanticTypeId,
};
use crate::terlan_typeck::{CoreCaseClause, CoreExpr, CoreModule, CoreType};

use super::NativeType;

mod body_json;
mod constructors;
mod cookies;
mod core_helpers;
mod error;
mod layout;
mod option_string;
mod receiver;
mod response;
mod security;
mod session;

pub(super) use constructors::install_http_constructors;

use body_json::{
    body_json_layouts, body_json_operation_type, lower_body_json_case,
    lower_managed_body_json_operation,
};
use core_helpers::{bool_expr, core_string_runtime_value, managed_http_call};
use error::{error_call, error_method_arity, error_operation_type, lower_error_operation};
use layout::{
    cookie_jar_descriptor, encoded_descriptor, http_error_descriptor, imports,
    option_string_layouts, request_descriptor, response_descriptor, response_header_descriptor,
    semantic, session_descriptor,
};
use option_string::lower_request_option_case;
use receiver::{is_jar_expr, is_managed_string_expr, jar_method_arity, response_method_arity};
use response::{cookie_call, jar_mutation, response_call, response_constructor, response_mutation};
use security::{lower_security_constructor_args, SECURITY_CONSTRUCTOR};

const REQUEST_MODULE: &str = "std.http.Request";
const RESPONSE_MODULE: &str = "std.http.Response";
const ROUTER_MODULE: &str = "std.http.Router";
const COOKIES_MODULE: &str = "std.http.Cookies";
const ERROR_MODULE: &str = "std.http.Error";
const RESPONSE_CONSTRUCTOR_PREFIX: &str = "$terlan.http.response.";
const REQUEST_STRING_MAP: &str = "std.http.Request.StringMap";
const STRING_OPTION: &str = "Option[String]";
const MANAGED_HTTP_MODULE: &str = "$terlan.managed.http";
const RESPONSE_HEADER: &str = "std.http.Response.Header";
const RESPONSE_HEADERS: &str = "std.http.Response.Headers";
const COOKIE_JAR: &str = "Named(Jar)";
const COOKIE_MUTATIONS: &str = "std.http.Cookies.Mutations";
const SECURITY_HEADERS: &str = "Named(SecurityHeaders)";
const MIDDLEWARE_RESULT: &str = "Named(MiddlewareResult)";

/// Imported HTTP surfaces that control one recursive normalization pass.
#[derive(Clone, Copy)]
struct HttpFeatures {
    /// Request accessors and request-owned values are available.
    request: bool,
    /// Static router values and callable plans are available.
    router: bool,
    /// Portable typed HTTP errors are available.
    error: bool,
    /// VM-owned session values and lifecycle calls are available.
    session: bool,
}

/// Reports whether a remote-call owner names the compiler-private HTTP operation family.
pub(super) fn is_managed_http_module(module: &str) -> bool {
    module == MANAGED_HTTP_MODULE
}

/// Rewrites target-owned response builders into fixed managed constructors.
pub(super) fn lower_http_values(core: &mut CoreModule) -> Result<(), String> {
    let router = imports(core, ROUTER_MODULE);
    let session = imports(core, session::SESSION_MODULE);
    let request = router || session || imports(core, REQUEST_MODULE);
    let error = router || imports(core, ERROR_MODULE);
    if !request
        && !router
        && !error
        && !imports(core, RESPONSE_MODULE)
        && !imports(core, COOKIES_MODULE)
        && !session
    {
        return Ok(());
    }
    for function in &mut core.functions {
        for clause in &mut function.clauses {
            if let Some(body) = &mut clause.body.core_expr {
                *body = rewrite(
                    body,
                    HttpFeatures {
                        request,
                        router,
                        error,
                        session,
                    },
                )?;
            }
        }
    }
    Ok(())
}

/// Returns target-owned aggregate layouts required at the HTTP boundary.
pub(super) fn http_managed_layouts(core: &CoreModule) -> Result<Vec<Arc<[u8]>>, String> {
    let mut layouts = Vec::new();
    let router = imports(core, ROUTER_MODULE);
    let session = imports(core, session::SESSION_MODULE);
    let error = router || imports(core, ERROR_MODULE);
    if router || session || imports(core, REQUEST_MODULE) {
        layouts.push(encoded_descriptor(&request_descriptor()?)?);
        layouts.push(encoded_descriptor(&cookie_jar_descriptor()?)?);
        layouts.extend(option_string_layouts()?);
        layouts.extend(body_json_layouts()?);
    }
    if session {
        layouts.push(encoded_descriptor(&session_descriptor()?)?);
    }
    if router || session || imports(core, RESPONSE_MODULE) {
        layouts.push(encoded_descriptor(&response_descriptor()?)?);
        layouts.push(encoded_descriptor(&response_header_descriptor()?)?);
    }
    if error {
        layouts.push(encoded_descriptor(&http_error_descriptor()?)?);
    }
    Ok(layouts)
}

/// Returns target-owned collection schemas nested inside HTTP boundary values.
pub(super) fn http_managed_collections(core: &CoreModule) -> Result<Vec<Arc<[u8]>>, String> {
    let mut collections = Vec::new();
    let router = imports(core, ROUTER_MODULE);
    let session = imports(core, session::SESSION_MODULE);
    if router || session || imports(core, REQUEST_MODULE) {
        let string = semantic("std.core.String")?;
        let descriptor = ManagedCollectionDescriptor::map(
            REQUEST_STRING_MAP,
            ManagedFieldType::Reference(string),
            ManagedFieldType::Reference(string),
        )
        .map_err(|error| format!("error[native_ir.http_request_map]: {error}"))?;
        collections.push(Arc::from(encode_collection_layout(&descriptor).map_err(
            |error| format!("error[native_ir.http_request_map_abi]: {error}"),
        )?));
        let mutations = ManagedCollectionDescriptor::list(
            COOKIE_MUTATIONS,
            ManagedFieldType::Reference(semantic("std.core.String")?),
        )
        .map_err(|error| format!("error[native_ir.http_cookie_mutations]: {error}"))?;
        collections.push(Arc::from(encode_collection_layout(&mutations).map_err(
            |error| format!("error[native_ir.http_cookie_mutations_abi]: {error}"),
        )?));
    }
    if router || session || imports(core, RESPONSE_MODULE) {
        let descriptor = ManagedCollectionDescriptor::list(
            RESPONSE_HEADERS,
            ManagedFieldType::Reference(semantic(RESPONSE_HEADER)?),
        )
        .map_err(|error| format!("error[native_ir.http_response_headers]: {error}"))?;
        collections.push(Arc::from(encode_collection_layout(&descriptor).map_err(
            |error| format!("error[native_ir.http_response_headers_abi]: {error}"),
        )?));
    }
    Ok(collections)
}

/// Rewrites one expression after recursively normalizing its children.
fn rewrite(expr: &CoreExpr, features: HttpFeatures) -> Result<CoreExpr, String> {
    let mut rewritten = expr.clone();
    rewrite_children(&mut rewritten, features)?;
    if features.session {
        if let Some(rewritten) = session::rewrite_session_call(&rewritten)? {
            return Ok(rewritten);
        }
    }
    match rewritten {
        CoreExpr::Case { scrutinee, clauses } if features.request => {
            if let Some(lowered) = lower_body_json_case(&scrutinee, &clauses)? {
                return Ok(lowered);
            }
            if let Some(lowered) = lower_request_option_case(&scrutinee, &clauses)? {
                return Ok(lowered);
            }
            if features.session {
                if let Some(lowered) = session::lower_session_case(&scrutinee, &clauses)? {
                    return Ok(lowered);
                }
            }
            Ok(CoreExpr::Case { scrutinee, clauses })
        }
        CoreExpr::BinaryOp {
            operator,
            left,
            right,
        } if operator == "+"
            && matches!(&*left, CoreExpr::Binary(_))
            && is_managed_string_expr(&right) =>
        {
            Ok(managed_http_call(
                "string_prepend_literal",
                vec![*left, *right],
            ))
        }
        CoreExpr::BinaryOp {
            operator,
            left,
            right,
        } if operator == "+" && is_managed_string_expr(&left) && is_managed_string_expr(&right) => {
            Ok(managed_http_call("string_append", vec![*left, *right]))
        }
        CoreExpr::Var(name) if features.router && name == "Continue" => {
            Ok(CoreExpr::ConstructorCall {
                constructor: format!("{ROUTER_MODULE}.Continue"),
                constructor_identity: Some(format!("{ROUTER_MODULE}.Continue")),
                args: Vec::new(),
            })
        }
        CoreExpr::RemoteCall {
            module,
            function,
            args,
        } if module == "__receiver__" && jar_method_arity(&function, args.len(), &args) => {
            jar_receiver_call(&function, args)
        }
        CoreExpr::RemoteCall {
            module,
            function,
            args,
        } if module == "__receiver__" && response_method_arity(&function, args.len()) => {
            response_receiver_call(&function, args)
        }
        CoreExpr::RemoteCall {
            module,
            function,
            args,
        } if features.error
            && module == "__receiver__"
            && error_method_arity(&function, args.len()) =>
        {
            error_call(&function, args)
        }
        CoreExpr::RemoteCall {
            module,
            function,
            args,
        } if module == "__receiver__"
            && matches!(
                (function.as_str(), args.len()),
                (
                    "default_security_headers" | "production_security_headers",
                    1
                )
            ) =>
        {
            response_call(&function, Vec::new())
        }
        CoreExpr::RemoteCall {
            module,
            function,
            args,
        } if features.request && module == "__receiver__" => request_accessor(&function, args),
        CoreExpr::RemoteCall {
            module,
            function,
            args,
        } if module == RESPONSE_MODULE => response_call(&function, args),
        CoreExpr::RemoteCall {
            module,
            function,
            args,
        } if module == COOKIES_MODULE => cookie_call(&function, args),
        CoreExpr::RemoteCall {
            module,
            function,
            args,
        } if module == ERROR_MODULE => error_call(&function, args),
        CoreExpr::Call { function, args } if function.starts_with(RESPONSE_MODULE) => {
            let name = function
                .strip_prefix(RESPONSE_MODULE)
                .and_then(|value| value.strip_prefix('.'))
                .unwrap_or(&function);
            response_call(name, args)
        }
        CoreExpr::Call { function, args }
            if matches!(
                (function.as_str(), args.len()),
                (
                    "default_security_headers" | "production_security_headers",
                    0
                )
            ) =>
        {
            response_call(&function, args)
        }
        CoreExpr::ConstructorCall {
            constructor,
            constructor_identity: _,
            args,
        } if args.len() == 5
            && constructor
                .rsplit('.')
                .next()
                .is_some_and(|name| name == "SecurityHeaders") =>
        {
            let args = lower_security_constructor_args(args)?;
            Ok(CoreExpr::ConstructorCall {
                constructor: SECURITY_CONSTRUCTOR.to_string(),
                constructor_identity: Some(SECURITY_CONSTRUCTOR.to_string()),
                args,
            })
        }
        CoreExpr::MutableReceiverCall {
            receiver,
            method,
            args,
            effects,
        } if is_jar_expr(&receiver) => jar_mutation(*receiver, &method, args, effects),
        CoreExpr::MutableReceiverCall {
            receiver,
            method,
            args,
            effects,
        } => response_mutation(*receiver, &method, args, effects),
        other => Ok(other),
    }
}

/// Separates a receiver argument before applying normal response mutation lowering.
fn response_receiver_call(method: &str, mut args: Vec<CoreExpr>) -> Result<CoreExpr, String> {
    if args.is_empty() {
        return Err(format!(
            "error[native_ir.http_response_mutation]: {method} is missing its receiver"
        ));
    }
    let receiver = args.remove(0);
    response_mutation(
        receiver,
        method,
        args,
        crate::terlan_typeck::CoreEffectSet {
            effects: Vec::new(),
        },
    )
}

/// Separates one known jar receiver before immutable jar operation lowering.
fn jar_receiver_call(method: &str, mut args: Vec<CoreExpr>) -> Result<CoreExpr, String> {
    let receiver = args.remove(0);
    if method == "get" {
        return Ok(CoreExpr::RemoteCall {
            module: MANAGED_HTTP_MODULE.to_string(),
            function: "jar_get".to_string(),
            args: [vec![receiver], args].concat(),
        });
    }
    jar_mutation(
        receiver,
        method,
        args,
        crate::terlan_typeck::CoreEffectSet {
            effects: Vec::new(),
        },
    )
}

/// Rewrites all child expressions while preserving the parent node.
fn rewrite_children(expr: &mut CoreExpr, features: HttpFeatures) -> Result<(), String> {
    match expr {
        CoreExpr::Tuple(items) | CoreExpr::List(items) | CoreExpr::FixedArray(items) => {
            rewrite_many(items, features)?
        }
        CoreExpr::ListCons { head, tail }
        | CoreExpr::Index {
            base: head,
            index: tail,
        } => {
            **head = rewrite(head, features)?;
            **tail = rewrite(tail, features)?;
        }
        CoreExpr::ListComprehension {
            expr,
            generators,
            guards,
            ..
        } => {
            **expr = rewrite(expr, features)?;
            for generator in generators {
                generator.source = rewrite(&generator.source, features)?;
            }
            rewrite_many(guards, features)?;
        }
        CoreExpr::Let { bindings, body } => {
            for binding in bindings {
                binding.value = rewrite(&binding.value, features)?;
            }
            **body = rewrite(body, features)?;
        }
        CoreExpr::Map(fields) => {
            for field in fields {
                field.value = rewrite(&field.value, features)?;
            }
        }
        CoreExpr::RecordConstruct { fields, .. } | CoreExpr::TemplateInstantiate { fields, .. } => {
            rewrite_fields(fields, features)?
        }
        CoreExpr::FieldAccess { base, .. } | CoreExpr::RecordAccess { base, .. } => {
            **base = rewrite(base, features)?
        }
        CoreExpr::RecordUpdate { base, fields, .. } => {
            **base = rewrite(base, features)?;
            rewrite_fields(fields, features)?;
        }
        CoreExpr::ConstructorChain { args, record, .. } => {
            rewrite_many(args, features)?;
            **record = rewrite(record, features)?;
        }
        CoreExpr::RemoteCall { args, .. }
        | CoreExpr::ConstructorCall { args, .. }
        | CoreExpr::Call { args, .. } => rewrite_many(args, features)?,
        CoreExpr::MutableReceiverCall { receiver, args, .. } => {
            **receiver = rewrite(receiver, features)?;
            rewrite_many(args, features)?;
        }
        CoreExpr::FunctionCall { callee, args } => {
            **callee = rewrite(callee, features)?;
            rewrite_many(args, features)?;
        }
        CoreExpr::Cast { expr, .. } => **expr = rewrite(expr, features)?,
        CoreExpr::Intrinsic(call) => rewrite_many(&mut call.args, features)?,
        CoreExpr::SqlQuery { parameters, .. } => rewrite_many(parameters, features)?,
        CoreExpr::Case { scrutinee, clauses } => {
            **scrutinee = rewrite(scrutinee, features)?;
            rewrite_clauses(clauses, features)?;
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            **body = rewrite(body, features)?;
            rewrite_clauses(of_clauses, features)?;
            rewrite_clauses(catch_clauses, features)?;
            if let Some(after) = after_clause {
                *after.trigger = rewrite(&after.trigger, features)?;
                *after.body = rewrite(&after.body, features)?;
            }
        }
        CoreExpr::If { clauses } => {
            for clause in clauses {
                clause.condition = rewrite(&clause.condition, features)?;
                clause.body = rewrite(&clause.body, features)?;
            }
        }
        CoreExpr::Lam { body, .. } => **body = rewrite(body, features)?,
        CoreExpr::UnaryOp { operand, .. } => **operand = rewrite(operand, features)?,
        CoreExpr::BinaryOp { left, right, .. } => {
            **left = rewrite(left, features)?;
            **right = rewrite(right, features)?;
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

/// Rewrites one ordered expression list.
fn rewrite_many(expressions: &mut [CoreExpr], features: HttpFeatures) -> Result<(), String> {
    for expression in expressions {
        *expression = rewrite(expression, features)?;
    }
    Ok(())
}

/// Rewrites record-like field payloads.
fn rewrite_fields(
    fields: &mut [crate::terlan_typeck::CoreRecordExprField],
    features: HttpFeatures,
) -> Result<(), String> {
    for field in fields {
        field.value = rewrite(&field.value, features)?;
    }
    Ok(())
}

/// Rewrites guards and bodies retained by case-like clauses.
fn rewrite_clauses(clauses: &mut [CoreCaseClause], features: HttpFeatures) -> Result<(), String> {
    for clause in clauses {
        if let Some(guard) = &mut clause.guard {
            *guard = rewrite(guard, features)?;
        }
        clause.body = rewrite(&clause.body, features)?;
    }
    Ok(())
}

/// Rewrites one portable request receiver call into a compiler-private operation.
fn request_accessor(function: &str, args: Vec<CoreExpr>) -> Result<CoreExpr, String> {
    let supported = matches!(
        (function, args.len()),
        (
            "method" | "path" | "query_string" | "body_text" | "body_json" | "cookies",
            1
        ) | ("param" | "query" | "header" | "cookie", 2)
    );
    if !supported {
        return Ok(CoreExpr::RemoteCall {
            module: "__receiver__".to_string(),
            function: function.to_string(),
            args,
        });
    }
    Ok(CoreExpr::RemoteCall {
        module: MANAGED_HTTP_MODULE.to_string(),
        function: function.to_string(),
        args,
    })
}

/// Returns the exact result type of one compiler-private request operation.
pub(super) fn managed_http_operation_type(expr: &CoreExpr) -> Option<NativeType> {
    if let Some(ty) = body_json_operation_type(expr) {
        return Some(ty);
    }
    let CoreExpr::RemoteCall {
        module,
        function,
        args,
    } = expr
    else {
        return None;
    };
    if module != MANAGED_HTTP_MODULE {
        return None;
    }
    if let Some(ty) = session::operation_type(function, args.len()) {
        return Some(ty);
    }
    match (function.as_str(), args.len()) {
        (function, arity) if error_operation_type(function, arity).is_some() => {
            error_operation_type(function, arity)
        }
        ("string_equal", 2) => Some(NativeType::Bool),
        ("string_append", 2) => Some(NativeType::StringRef),
        ("string_prepend_literal", 2) => Some(NativeType::StringRef),
        ("method" | "path" | "query_string" | "body_text", 1) => Some(NativeType::StringRef),
        ("param" | "query" | "header" | "cookie", 2) => {
            semantic(STRING_OPTION).ok().map(NativeType::ManagedRef)
        }
        ("option_is_none", 1) => Some(NativeType::Bool),
        ("option_some", 1) => Some(NativeType::StringRef),
        ("empty_headers", 0) => semantic(RESPONSE_HEADERS).ok().map(NativeType::ManagedRef),
        ("cookies", 1) | ("jar_append", 2) => semantic(COOKIE_JAR).ok().map(NativeType::ManagedRef),
        ("jar_get", 2) => semantic(STRING_OPTION).ok().map(NativeType::ManagedRef),
        ("cookie_set_header" | "cookie_set_options_header" | "cookie_delete_header", _) => {
            Some(NativeType::StringRef)
        }
        ("response_status", 2)
        | ("response_header", 3)
        | ("response_cookie_jar", 2)
        | ("response_security_headers", 2) => {
            semantic(&CoreType::Named("Response".to_string()).contract_text())
                .ok()
                .map(NativeType::ManagedRef)
        }
        _ => None,
    }
}

/// Lowers one compiler-private request operation into managed NativeIR.
pub(super) fn lower_managed_http_operation(
    expr: &CoreExpr,
    mut lower: impl FnMut(&CoreExpr) -> Result<super::NativeExpr, String>,
) -> Result<Option<super::NativeExpr>, String> {
    let CoreExpr::RemoteCall {
        module,
        function,
        args,
    } = expr
    else {
        return Ok(None);
    };
    if module != MANAGED_HTTP_MODULE {
        return Ok(None);
    }
    if let Some(operation) = lower_managed_body_json_operation(function, args, &mut lower)? {
        return Ok(Some(operation));
    }
    if let Some(operation) = session::lower_operation(function, args, &mut lower)? {
        return Ok(Some(operation));
    }
    if let Some(operation) = lower_error_operation(function, args, &mut lower)? {
        return Ok(Some(operation));
    }
    if function == "option_is_none" && args.len() == 1 {
        return Ok(Some(super::NativeExpr::ManagedOperation {
            encoded: Arc::from(encode_session_option_is_none_operation(semantic(
                STRING_OPTION,
            )?)),
            args: args.iter().map(&mut lower).collect::<Result<Vec<_>, _>>()?,
        }));
    }
    if function == "option_some" && args.len() == 1 {
        return Ok(Some(super::NativeExpr::ManagedOperation {
            encoded: Arc::from(
                encode_aggregate_field_operation(semantic(STRING_OPTION)?, 0)
                    .map_err(|error| format!("error[native_ir.http_request_option]: {error}"))?,
            ),
            args: args.iter().map(&mut lower).collect::<Result<Vec<_>, _>>()?,
        }));
    }
    if function == "string_equal" && args.len() == 2 {
        return Ok(Some(super::NativeExpr::ManagedOperation {
            encoded: Arc::from(encode_string_equal_operation()),
            args: args.iter().map(&mut lower).collect::<Result<Vec<_>, _>>()?,
        }));
    }
    if function == "string_append" && args.len() == 2 {
        return Ok(Some(super::NativeExpr::ManagedOperation {
            encoded: Arc::from(encode_string_append_operation()),
            args: args.iter().map(&mut lower).collect::<Result<Vec<_>, _>>()?,
        }));
    }
    if function == "string_prepend_literal" && args.len() == 2 {
        let CoreExpr::Binary(literal) = &args[0] else {
            return Err(
                "error[native_ir.http_string_prepend]: expected a string literal prefix"
                    .to_string(),
            );
        };
        let literal = core_string_runtime_value(literal)?;
        if let CoreExpr::RemoteCall {
            module,
            function,
            args: projection_args,
        } = &args[1]
        {
            if module == MANAGED_HTTP_MODULE
                && function == "body_text"
                && projection_args.len() == 1
            {
                let request_semantic =
                    semantic(&CoreType::Named("Request".to_string()).contract_text())?;
                return Ok(Some(super::NativeExpr::ManagedOperation {
                    encoded: Arc::from(
                        encode_string_prepend_projected_literal_operation(
                            request_semantic,
                            4,
                            &literal,
                        )
                        .map_err(|error| {
                            format!("error[native_ir.http_string_prepend]: {error}")
                        })?,
                    ),
                    args: vec![lower(&projection_args[0])?],
                }));
            }
        }
        return Ok(Some(super::NativeExpr::ManagedOperation {
            encoded: Arc::from(
                encode_string_prepend_literal_operation(&literal)
                    .map_err(|error| format!("error[native_ir.http_string_prepend]: {error}"))?,
            ),
            args: vec![lower(&args[1])?],
        }));
    }
    if function == "empty_headers" && args.is_empty() {
        return Ok(Some(super::NativeExpr::ManagedOperation {
            encoded: Arc::from(encode_list_empty_operation(semantic(RESPONSE_HEADERS)?)),
            args: Vec::new(),
        }));
    }
    let response_semantic = semantic(&CoreType::Named("Response".to_string()).contract_text())?;
    if function == "response_status" && args.len() == 2 {
        return Ok(Some(super::NativeExpr::ManagedOperation {
            encoded: Arc::from(
                encode_aggregate_replace_field_operation(response_semantic, 3)
                    .map_err(|error| format!("error[native_ir.http_response_status]: {error}"))?,
            ),
            args: args.iter().map(&mut lower).collect::<Result<Vec<_>, _>>()?,
        }));
    }
    if function == "response_header" && args.len() == 3 {
        return Ok(Some(super::NativeExpr::ManagedOperation {
            encoded: Arc::from(
                encode_aggregate_append_pair_operation(
                    response_semantic,
                    semantic(RESPONSE_HEADERS)?,
                    semantic(RESPONSE_HEADER)?,
                    5,
                )
                .map_err(|error| format!("error[native_ir.http_response_header]: {error}"))?,
            ),
            args: args.iter().map(&mut lower).collect::<Result<Vec<_>, _>>()?,
        }));
    }
    let cookie_operation = match (function.as_str(), args.len()) {
        ("cookie_set_header", 5) => Some(ManagedCookieHeaderOperation::Set),
        ("cookie_set_options_header", 10) => Some(ManagedCookieHeaderOperation::SetWithOptions),
        ("cookie_delete_header", 2) => Some(ManagedCookieHeaderOperation::Delete),
        _ => None,
    };
    if let Some(operation) = cookie_operation {
        return Ok(Some(super::NativeExpr::ManagedOperation {
            encoded: Arc::from(encode_cookie_header_operation(operation)),
            args: args.iter().map(&mut lower).collect::<Result<Vec<_>, _>>()?,
        }));
    }
    if function == "jar_append" && args.len() == 2 {
        return Ok(Some(super::NativeExpr::ManagedOperation {
            encoded: Arc::from(
                encode_aggregate_append_value_operation(
                    semantic(COOKIE_JAR)?,
                    semantic(COOKIE_MUTATIONS)?,
                    1,
                )
                .map_err(|error| format!("error[native_ir.http_cookie_jar]: {error}"))?,
            ),
            args: args.iter().map(&mut lower).collect::<Result<Vec<_>, _>>()?,
        }));
    }
    if function == "response_cookie_jar" && args.len() == 2 {
        return Ok(Some(super::NativeExpr::ManagedOperation {
            encoded: Arc::from(
                encode_response_cookie_jar_operation(
                    response_semantic,
                    semantic(RESPONSE_HEADERS)?,
                    semantic(RESPONSE_HEADER)?,
                    semantic(COOKIE_JAR)?,
                    semantic(COOKIE_MUTATIONS)?,
                    5,
                    1,
                )
                .map_err(|error| format!("error[native_ir.http_cookie_jar]: {error}"))?,
            ),
            args: args.iter().map(&mut lower).collect::<Result<Vec<_>, _>>()?,
        }));
    }
    if function == "response_security_headers" && args.len() == 2 {
        return Ok(Some(super::NativeExpr::ManagedOperation {
            encoded: Arc::from(
                encode_response_security_headers_operation(
                    response_semantic,
                    semantic(RESPONSE_HEADERS)?,
                    semantic(RESPONSE_HEADER)?,
                    semantic(SECURITY_HEADERS)?,
                    5,
                )
                .map_err(|error| format!("error[native_ir.http_security_headers]: {error}"))?,
            ),
            args: args.iter().map(&mut lower).collect::<Result<Vec<_>, _>>()?,
        }));
    }
    let request_semantic = semantic(&CoreType::Named("Request".to_string()).contract_text())?;
    if function == "jar_get" && args.len() == 2 {
        return string_map_lookup(&args[0], &args[1], semantic(COOKIE_JAR)?, 0, &mut lower)
            .map(Some);
    }
    let (field, lookup) = match function.as_str() {
        "method" => (1, false),
        "path" => (2, false),
        "param" => (3, true),
        "body_text" => (4, false),
        "query_string" => (5, false),
        "query" => (6, true),
        "header" => (7, true),
        "cookie" => (8, true),
        "cookies" => (9, false),
        _ => {
            return Err(format!(
                "error[native_ir.http_request_accessor]: unsupported Request.{function} operation"
            ));
        }
    };
    let request = args.first().ok_or_else(|| {
        format!(
            "error[native_ir.http_request_accessor]: Request.{function} is missing its receiver"
        )
    })?;
    let projection = super::NativeExpr::ManagedOperation {
        encoded: Arc::from(
            encode_aggregate_field_operation(request_semantic, field)
                .map_err(|error| format!("error[native_ir.http_request_project]: {error}"))?,
        ),
        args: vec![lower(request)?],
    };
    if !lookup {
        return Ok(Some(projection));
    }
    let key = args.get(1).ok_or_else(|| {
        format!("error[native_ir.http_request_accessor]: Request.{function} is missing its key")
    })?;
    Ok(Some(super::NativeExpr::ManagedOperation {
        encoded: Arc::from(encode_string_map_get_option_operation(
            semantic(REQUEST_STRING_MAP)?,
            semantic(STRING_OPTION)?,
        )),
        args: vec![projection, lower(key)?],
    }))
}

/// Lowers one string-map projection and checked optional lookup.
fn string_map_lookup(
    owner: &CoreExpr,
    key: &CoreExpr,
    owner_semantic: SemanticTypeId,
    field: usize,
    lower: &mut impl FnMut(&CoreExpr) -> Result<super::NativeExpr, String>,
) -> Result<super::NativeExpr, String> {
    let projection = super::NativeExpr::ManagedOperation {
        encoded: Arc::from(
            encode_aggregate_field_operation(owner_semantic, field)
                .map_err(|error| format!("error[native_ir.http_map_project]: {error}"))?,
        ),
        args: vec![lower(owner)?],
    };
    Ok(super::NativeExpr::ManagedOperation {
        encoded: Arc::from(encode_string_map_get_option_operation(
            semantic(REQUEST_STRING_MAP)?,
            semantic(STRING_OPTION)?,
        )),
        args: vec![projection, lower(key)?],
    })
}
