use super::*;
use terlan_hir::FunctionSignature;
use terlan_syntax::parse_expr_as_syntax_output;

/// Collects imported module-member function value targets.
///
/// Inputs:
/// - `module_aliases`: source-visible imported module aliases mapped to their
///   resolved module names.
/// - `interfaces`: loaded provider interfaces keyed by resolved module name.
///
/// Output:
/// - Function targets keyed by `(module_alias, member_name)` for member
///   references such as `Users.index`.
///
/// Transformation:
/// - Scans each imported module interface and records only public functions
///   whose name has exactly one public signature. Overloaded functions are
///   intentionally omitted until expected-type-directed function reference
///   resolution is implemented.
pub(in crate::emit::syntax) fn collect_imported_module_member_functions(
    module_aliases: &BTreeMap<String, String>,
    interfaces: &BTreeMap<String, ModuleInterface>,
) -> BTreeMap<(String, String), SyntaxImportedFunctionTarget> {
    let mut targets = BTreeMap::new();
    for (module_alias, module_name) in module_aliases {
        let Some(interface) = interfaces.get(module_name) else {
            continue;
        };
        for (function, signature) in unique_public_function_signatures(interface) {
            targets.insert(
                (module_alias.clone(), function.clone()),
                syntax_imported_function_target_from_signature(module_name, function, signature),
            );
        }
    }
    targets
}

/// Returns non-overloaded public function signatures from an interface.
///
/// Inputs:
/// - `interface`: loaded module interface.
///
/// Output:
/// - `(function_name, signature)` pairs where the function has exactly one
///   public callable signature.
///
/// Transformation:
/// - Builds a temporary public signature index from overload metadata when
///   available and from the compatibility function map otherwise, then filters
///   out ambiguous names.
fn unique_public_function_signatures(
    interface: &ModuleInterface,
) -> Vec<(&String, &FunctionSignature)> {
    let mut signatures = BTreeMap::<&String, Vec<&FunctionSignature>>::new();
    for ((name, _arity), overloads) in &interface.function_overloads {
        signatures
            .entry(name)
            .or_default()
            .extend(overloads.iter().filter(|signature| signature.public));
    }
    if signatures.is_empty() {
        for ((name, _arity), signature) in &interface.functions {
            if signature.public {
                signatures.entry(name).or_default().push(signature);
            }
        }
    }

    signatures
        .into_iter()
        .filter_map(|(name, entries)| {
            let [signature] = entries.as_slice() else {
                return None;
            };
            Some((name, *signature))
        })
        .collect()
}

/// Converts an interface signature into imported-function lowering metadata.
///
/// Inputs:
/// - `module`: resolved provider module name.
/// - `function`: provider function name.
/// - `signature`: public provider signature.
///
/// Output:
/// - Imported function target with arity, parameter names, and default
///   expression metadata.
///
/// Transformation:
/// - Preserves enough function metadata for direct remote function references
///   and later selected-import call lowering to agree on arity/defaults.
fn syntax_imported_function_target_from_signature(
    module: &str,
    function: &str,
    signature: &FunctionSignature,
) -> SyntaxImportedFunctionTarget {
    SyntaxImportedFunctionTarget {
        module: module.to_string(),
        function: function.to_string(),
        fixed_arity: signature.params.len(),
        min_arity: signature
            .params
            .iter()
            .filter(|param| param.default_text.is_none())
            .count(),
        param_names: signature
            .params
            .iter()
            .map(|param| param.name.clone())
            .collect(),
        defaults: signature
            .params
            .iter()
            .map(|param| {
                param
                    .default_text
                    .as_ref()
                    .and_then(|text| parse_expr_as_syntax_output(text).ok())
            })
            .collect(),
    }
}
/// Adds inherited local receiver-method targets for including structs.
///
/// Inputs:
/// - `module`: syntax-output module containing struct and receiver-method
///   declarations.
/// - `receiver_methods`: backend receiver dispatch map keyed by method/arity
///   and receiver type.
///
/// Output:
/// - None; `receiver_methods` is updated in place.
///
/// Transformation:
/// - For each local `struct Child includes Parent`, copies parent receiver
///   method targets to the child receiver type unless the child already has an
///   explicit method target. The inherited target still lowers to the original
///   receiver-first function body.
pub(in crate::emit::syntax) fn extend_receiver_methods_with_local_struct_includes(
    module: &SyntaxModuleOutput,
    receiver_methods: &mut BTreeMap<(String, usize), BTreeMap<String, SyntaxReceiverMethodTarget>>,
) {
    let local_structs = module
        .declarations
        .iter()
        .filter_map(|decl| match &decl.payload {
            SyntaxDeclarationPayload::Struct { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    let include_edges = module
        .declarations
        .iter()
        .filter_map(|decl| match &decl.payload {
            SyntaxDeclarationPayload::Struct { name, includes, .. } => Some((name, includes)),
            _ => None,
        })
        .flat_map(|(child, includes)| {
            let local_structs = &local_structs;
            includes
                .iter()
                .filter(move |parent| local_structs.contains(*parent))
                .map(move |parent| (child.clone(), parent.clone()))
        })
        .collect::<Vec<_>>();

    let inherited = include_edges
        .iter()
        .flat_map(|(child, parent)| {
            receiver_methods.iter().filter_map(move |(key, receivers)| {
                receivers
                    .get(parent)
                    .cloned()
                    .map(|target| (key.clone(), child.clone(), target))
            })
        })
        .collect::<Vec<_>>();

    for (key, child, target) in inherited {
        receiver_methods
            .entry(key)
            .or_default()
            .entry(child)
            .or_insert(target);
    }
}

/// Collects typed trait-method wrapper names for explicit impl declarations.
///
/// Inputs:
/// - `module`: syntax-output module containing zero or more
///   `impl Trait[...] for Type` declarations.
///
/// Output:
/// - Map from trait name, method name, and concrete implementation type to the
///   generated Erlang wrapper function name.
///
/// Transformation:
/// - Reads only structured syntax output, extracts the head trait name and
///   normalized `for` type, and assigns deterministic wrapper names so
///   `Trait.method(value)` dispatch can resolve before ordinary remote calls.
pub(in crate::emit::syntax) fn collect_syntax_typed_trait_method_wrappers(
    module: &SyntaxModuleOutput,
) -> BTreeMap<String, BTreeMap<String, BTreeMap<String, String>>> {
    let mut wrappers = BTreeMap::new();

    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::TraitImpl {
            trait_ref,
            for_type,
            methods,
            ..
        } = &declaration.payload
        else {
            continue;
        };
        let Some(trait_name) = syntax_type_head_name(&trait_ref.text) else {
            continue;
        };
        let type_arg = normalize_trait_type_text(&for_type.text);

        for method in methods {
            wrappers
                .entry(trait_name.clone())
                .or_insert_with(BTreeMap::new)
                .entry(method.name.clone())
                .or_insert_with(BTreeMap::new)
                .insert(
                    type_arg.clone(),
                    typed_trait_method_wrapper_name(&trait_name, &method.name, &type_arg),
                );
        }
    }

    wrappers
}

/// Collects local functions that can be captured as first-class values.
///
/// Inputs:
/// - `module`: syntax-output module containing function declarations.
///
/// Output:
/// - Map from source function name to its single source arity.
///
/// Transformation:
/// - Scans non-generic local functions and keeps only names that appear with
///   exactly one arity. Multi-clause functions with the same arity remain
///   capturable, while overloaded names and generic-bound functions are left
///   out so backend lowering cannot pick the wrong BEAM function reference.
pub(in crate::emit::syntax) fn collect_syntax_local_function_values(
    module: &SyntaxModuleOutput,
) -> BTreeMap<String, usize> {
    let mut arities_by_name = BTreeMap::<String, BTreeSet<usize>>::new();
    for decl in &module.declarations {
        let SyntaxDeclarationPayload::Function {
            name,
            params,
            generic_bounds,
            ..
        } = &decl.payload
        else {
            continue;
        };
        if !generic_bounds.is_empty() {
            continue;
        }
        arities_by_name
            .entry(name.clone())
            .or_default()
            .insert(params.len());
    }

    arities_by_name
        .into_iter()
        .filter_map(|(name, arities)| {
            if arities.len() == 1 {
                arities.iter().next().copied().map(|arity| (name, arity))
            } else {
                None
            }
        })
        .collect()
}

/// Infers a conservative concrete type key for syntax-output trait dispatch.
///
/// Inputs:
/// - `expr`: syntax-output expression used as the trait method's value
///   argument.
/// - `env`: local lowering environment containing parameter annotations.
///
/// Output:
/// - `Some(type_name)` for simple literal or annotated-local expressions.
/// - `None` when the expression needs full type-checker annotation before
///   dispatch can be selected.
///
/// Transformation:
/// - Maps primitive literal shapes and annotated locals to the normalized type
///   names used by typed trait-wrapper lookup.
pub(in crate::emit::syntax) fn infer_syntax_trait_dispatch_type(
    expr: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<String> {
    match expr.kind {
        SyntaxExprKind::Int => Some("Int".to_string()),
        SyntaxExprKind::Float => Some("Float".to_string()),
        SyntaxExprKind::Binary => Some("String".to_string()),
        SyntaxExprKind::List => Some("List".to_string()),
        SyntaxExprKind::Atom => expr.text.as_deref().and_then(|text| match text {
            "unit" => Some("Unit".to_string()),
            "lt" | "eq" | "gt" => Some("Comparison".to_string()),
            _ => None,
        }),
        SyntaxExprKind::Var => {
            let name = expr.text.as_deref()?;
            if is_bool_literal_name(name) {
                Some("Bool".to_string())
            } else {
                env.value_types
                    .get(name)
                    .map(|type_text| normalize_syntax_trait_dispatch_type_key(type_text))
            }
        }
        SyntaxExprKind::RecordConstruct => expr.text.clone(),
        SyntaxExprKind::Call => infer_syntax_known_call_return_type(expr, ctx, env),
        SyntaxExprKind::BinaryOp => infer_syntax_binary_op_return_type(expr, ctx, env),
        _ => None,
    }
}

/// Infers simple binary operator result types for syntax-bridge lowering.
///
/// Inputs:
/// - `expr`: syntax-output binary operator expression.
/// - `ctx`, `env`: active lowering context and local type environment.
///
/// Output:
/// - Source-level result type text for primitive operator shapes.
/// - `None` when either operand type is unavailable or the operator is not
///   relevant to receiver/dispatch decisions.
///
/// Transformation:
/// - Mirrors the typechecker's scalar operator rules closely enough for the
///   direct Erlang bridge to select backend operators such as integer `div`
///   for `Int / Int` expressions.
fn infer_syntax_binary_op_return_type(
    expr: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<String> {
    let left = expr.children.first()?;
    let right = expr.children.get(1)?;
    let left_type = infer_syntax_trait_dispatch_type(left, ctx, env)?;
    let right_type = infer_syntax_trait_dispatch_type(right, ctx, env)?;
    match expr.operator.as_deref()? {
        "+" if left_type == "String" || right_type == "String" => Some("String".to_string()),
        "+" | "-" | "*" | "/"
            if is_syntax_int_type_text(&left_type) && is_syntax_int_type_text(&right_type) =>
        {
            Some("Int".to_string())
        }
        "+" | "-" | "*" | "/" => Some("Number".to_string()),
        "div" | "rem" => Some("Int".to_string()),
        "==" | "!=" | "/=" | "=:=" | "=/=" | ">=" | ">" | "<=" | "<" | "and" | "or" | "&&"
        | "||" => Some("Bool".to_string()),
        _ => None,
    }
}

/// Returns whether type text denotes an integer-shaped Terlan value.
///
/// Inputs:
/// - `type_text`: source or normalized type text from syntax metadata.
///
/// Output:
/// - `true` for `Int` and qualified `std.core.Int.Int` spellings.
///
/// Transformation:
/// - Normalizes imported core aliases before comparing the final type shape so
///   backend operator selection is independent of import spelling.
pub(in crate::emit::syntax) fn is_syntax_int_type_text(type_text: &str) -> bool {
    matches!(
        normalize_syntax_trait_dispatch_type_key(type_text).as_str(),
        "Int" | "std.core.Int.Int"
    )
}

/// Infers selected compiler-known call return types for direct syntax lowering.
///
/// Inputs:
/// - `expr`: syntax-output call expression.
///
/// Output:
/// - Source-level return type name for closed, compiler-known calls.
/// - `None` for ordinary user calls and unknown std calls.
///
/// Transformation:
/// - Provides a narrow bridge for direct Erlang lowering before full CoreIR
///   annotations are available. The rule is intentionally closed to
///   `std.http.Response` builders and `std.http.Router` builders so chained
///   handler/router code can lower through the same receiver helpers as named
///   values.
fn infer_syntax_known_call_return_type(
    expr: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<String> {
    let callee = expr.children.first()?;
    if callee.kind == SyntaxExprKind::FieldAccess {
        let function = callee.text.as_deref()?;
        let receiver = callee.children.first()?;
        if let Some(receiver_type) = infer_syntax_trait_dispatch_type(receiver, ctx, env) {
            if let Some(return_type) = primitive_receiver_method_return_type(
                &receiver_type,
                function,
                expr.children.len() - 1,
            ) {
                return Some(return_type);
            }
        }
        if is_router_builder_function(function) {
            if infer_syntax_known_call_return_type(receiver, ctx, env).as_deref() == Some("Router")
            {
                return Some("Router".to_string());
            }
        }
        return None;
    }

    let function = syntax_expr_name(callee)?;
    if let Some(remote) = expr.remote.as_deref() {
        let response_module = matches!(remote, "Response" | "std.http.Response");
        if response_module
            && matches!(
                function,
                "json" | "json_text" | "text" | "html" | "file" | "redirect"
            )
        {
            return Some("Response".to_string());
        }
        let router_module = matches!(remote, "Router" | "std.http.Router");
        if router_module && is_router_builder_function(function) {
            return Some("Router".to_string());
        }
        return None;
    }
    let arity = expr.children.len().saturating_sub(1);
    let args = &expr.children[1..];
    if let Some(target) = ctx.imported_constructor_target(function, arity) {
        return infer_syntax_remote_constructor_return_type(target, args, ctx, env);
    }
    if let Some(target) = ctx.alias_constructor_call_target(function, arity) {
        return infer_syntax_alias_constructor_return_type(function, target, args, ctx, env);
    }
    if !ctx.constructors.contains_key(function) && ctx.struct_field_types.contains_key(function) {
        return Some(function.to_string());
    }
    if let Some(target) = ctx.local_function_target(function, expr.children.len().saturating_sub(1))
    {
        return Some(normalize_trait_type_text(&target.return_type));
    }
    None
}

/// Infers a source-visible return type for an imported constructor call.
///
/// Inputs:
/// - `target`: imported constructor metadata selected by call head and arity.
/// - `args`: source argument expressions excluding the callee.
/// - `ctx`, `env`: active syntax lowering context and local type environment.
///
/// Output:
/// - Constructor return type text with simple vararg element substitution when
///   the argument type is visible.
///
/// Transformation:
/// - Preserves imported constructor return metadata, but narrows common
///   collection shorthand such as `Vector(1, 2)` to `Vector[Int]` so downstream
///   receiver dispatch can keep the element type.
fn infer_syntax_remote_constructor_return_type(
    target: &SyntaxRemoteConstructorTarget,
    args: &[SyntaxExprOutput],
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<String> {
    let head = receiver_type_head(&target.return_type);
    if target.varargs && target.fixed_arity == 0 {
        if let Some(first_arg_type) = args
            .first()
            .and_then(|arg| infer_syntax_trait_dispatch_type(arg, ctx, env))
        {
            return Some(format!("{}[{}]", head, first_arg_type));
        }
    }
    Some(normalize_trait_type_text(&target.return_type))
}

/// Infers a source-visible return type for an alias-constructor call.
///
/// Inputs:
/// - `name`: source constructor name.
/// - `target`: alias-constructor metadata selected for the call.
/// - `args`: source argument expressions excluding the callee.
/// - `ctx`, `env`: active syntax lowering context and local type environment.
///
/// Output:
/// - Alias constructor type text, parameterized by inferred argument types when
///   available.
///
/// Transformation:
/// - Treats alias constructors as producing their constructor alias rather than
///   only the lowered runtime tuple. For `Some(Vector(1))`, this returns
///   `Some[Vector[Int]]`, giving `case` branch pattern bindings enough type
///   information to lower receiver calls on the payload.
fn infer_syntax_alias_constructor_return_type(
    name: &str,
    target: &SyntaxAliasConstructorTarget,
    args: &[SyntaxExprOutput],
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<String> {
    if target.params.is_empty() {
        return Some(name.to_string());
    }
    let arg_types = args
        .iter()
        .map(|arg| {
            infer_syntax_trait_dispatch_type(arg, ctx, env).unwrap_or_else(|| "Dynamic".to_string())
        })
        .collect::<Vec<_>>();
    Some(format!("{}[{}]", name, arg_types.join(", ")))
}

/// Infers the return type for a recognized primitive receiver intrinsic.
///
/// Inputs:
/// - `receiver_type`: normalized primitive receiver type text.
/// - `method`: receiver method name.
/// - `arg_count`: number of explicit call arguments.
///
/// Output:
/// - Terlan type text for the intrinsic return value when recognized.
///
/// Transformation:
/// - Maps receiver intrinsic metadata to the structural type emitted into
///   syntax metadata.
fn primitive_receiver_method_return_type(
    receiver_type: &str,
    method: &str,
    arg_count: usize,
) -> Option<String> {
    match primitive_receiver_method_intrinsic(receiver_type, method, arg_count)? {
        CorePrimitiveIntrinsic::StringEqual
        | CorePrimitiveIntrinsic::StringContains
        | CorePrimitiveIntrinsic::StringStartsWith
        | CorePrimitiveIntrinsic::StringEndsWith
        | CorePrimitiveIntrinsic::StringIsEmpty
        | CorePrimitiveIntrinsic::MapContainsKey
        | CorePrimitiveIntrinsic::SetContains => Some("Bool".to_string()),
        CorePrimitiveIntrinsic::StringCompare => Some("Comparison".to_string()),
        CorePrimitiveIntrinsic::StringLength | CorePrimitiveIntrinsic::StringByteSize => {
            Some("Int".to_string())
        }
        CorePrimitiveIntrinsic::StringToString
        | CorePrimitiveIntrinsic::StringAppend
        | CorePrimitiveIntrinsic::StringReplace
        | CorePrimitiveIntrinsic::StringTrim
        | CorePrimitiveIntrinsic::StringTrimStart
        | CorePrimitiveIntrinsic::StringTrimEnd
        | CorePrimitiveIntrinsic::StringLowercase
        | CorePrimitiveIntrinsic::StringUppercase
        | CorePrimitiveIntrinsic::IntToString
        | CorePrimitiveIntrinsic::FloatToString
        | CorePrimitiveIntrinsic::BoolToString => Some("String".to_string()),
        CorePrimitiveIntrinsic::StringSplit => Some("List[String]".to_string()),
        CorePrimitiveIntrinsic::StringSplitOnce => Some("Option[{String, String}]".to_string()),
        _ => None,
    }
}

/// Returns whether a function name belongs to the router builder API.
///
/// Inputs:
/// - `function`: candidate function or receiver method name.
///
/// Output:
/// - `true` when the name is part of `std.http.Router` construction.
///
/// Transformation:
/// - Uses a fixed release-owned allow-list to identify router builder calls.
fn is_router_builder_function(function: &str) -> bool {
    matches!(
        function,
        "new"
            | "get"
            | "post"
            | "put"
            | "patch"
            | "delete"
            | "head"
            | "options"
            | "use"
            | "fallback"
            | "error"
            | "group"
    )
}

/// Normalizes an annotated local type into the wrapper-map key used for trait dispatch.
///
/// Inputs:
/// - `type_text`: source or imported-qualified type annotation from the
///   lowering environment.
///
/// Output:
/// - Dispatch key used by typed trait wrapper lookup.
///
/// Transformation:
/// - Preserves ordinary user types exactly.
/// - Collapses compiler-known core aliases imported from their owning modules,
///   such as `std.core.Unit.Unit`, back to the source-level key used by
///   `impl Ordering[Unit]`.
pub(in crate::emit::syntax) fn normalize_syntax_trait_dispatch_type_key(type_text: &str) -> String {
    match normalize_trait_type_text(type_text).as_str() {
        "std.core.Unit.Unit" => "Unit".to_string(),
        "std.core.Ordering.Comparison" => "Comparison".to_string(),
        other => other.to_string(),
    }
}

/// Builds a lowering environment for a function or method body.
///
/// Inputs:
/// - `params`: source parameters for the callable.
/// - `ctx`: module lowering context.
/// - `generic_bounds`: generic trait bounds declared on the callable.
///
/// Output:
/// - Local lowering environment for the callable body.
///
/// Transformation:
/// - Records value locals, qualified parameter types, struct-typed locals, and
///   hidden generic-bound dictionary parameter names.
pub(in crate::emit::syntax) fn lower_syntax_function_env(
    params: &[SyntaxParamOutput],
    ctx: &SyntaxLowerCtx,
    generic_bounds: &[String],
) -> SyntaxLowerEnv {
    let value_locals = params.iter().map(|param| param.name.clone()).collect();
    let value_types = params
        .iter()
        .map(|param| {
            (
                param.name.clone(),
                qualify_imported_type_text(
                    &normalize_trait_type_text(&param.annotation.text),
                    &ctx.imported_type_refs,
                ),
            )
        })
        .collect();
    let trait_bound_dicts = generic_bounds
        .iter()
        .zip(generic_bound_param_names(generic_bounds))
        .filter_map(|(bound, param_name)| {
            let bound = parse_syntax_generic_function_bound(bound)?;
            if bound.type_args.len() != 1 {
                return None;
            }
            Some(((bound.trait_name, bound.type_args[0].clone()), param_name))
        })
        .collect();
    SyntaxLowerEnv {
        value_locals,
        value_types,
        trait_bound_dicts,
        value_replacements: BTreeMap::new(),
        current_constructor_target: None,
    }
}

/// Builds a lowering environment for a constructor clause body.
///
/// Inputs:
/// - `params`: constructor clause parameters.
/// - `ctx`: module lowering context.
///
/// Output:
/// - Local lowering environment for the constructor body.
///
/// Transformation:
/// - Records constructor value locals, qualified parameter types, and
///   struct-typed locals without generic-bound dictionaries.
pub(in crate::emit::syntax) fn lower_syntax_constructor_clause_env(
    constructor_target: &str,
    params: &[SyntaxConstructorParamOutput],
    ctx: &SyntaxLowerCtx,
) -> SyntaxLowerEnv {
    let value_locals = params.iter().map(|param| param.name.clone()).collect();
    let value_types = params
        .iter()
        .map(|param| {
            (
                param.name.clone(),
                qualify_imported_type_text(
                    &normalize_trait_type_text(&param.annotation.text),
                    &ctx.imported_type_refs,
                ),
            )
        })
        .collect();
    SyntaxLowerEnv {
        value_locals,
        value_types,
        trait_bound_dicts: BTreeMap::new(),
        value_replacements: BTreeMap::new(),
        current_constructor_target: Some(constructor_target.to_string()),
    }
}

/// Resolves a local struct name from a simple type annotation.
///
/// Inputs:
/// - `annotation`: source type annotation text.
/// - `ctx`: module lowering context containing local struct fields.
///
/// Output:
/// - Local struct name when the annotation directly names a local struct.
///
/// Transformation:
/// - Rejects generic and qualified annotations, then checks the local struct
///   field table.
fn syntax_struct_name_from_type_annotation(
    annotation: &str,
    ctx: &SyntaxLowerCtx,
) -> Option<String> {
    let trimmed = annotation.trim();
    if trimmed.contains('[') || trimmed.contains('.') {
        return None;
    }
    ctx.struct_field_types
        .contains_key(trimmed)
        .then(|| trimmed.to_string())
}

/// Lowers a syntax-output expression without pre-existing local state.
///
/// Inputs:
/// - `expr`: syntax-output expression tree.
/// - `ctx`: module lowering context.
///
/// Output:
/// - Erlang render expression when this bridge supports the expression shape.
///
/// Transformation:
/// - Delegates to environment-aware lowering with an empty local environment.
pub(in crate::emit::syntax) fn lower_syntax_expr(
    expr: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
) -> Option<ErlExpr> {
    lower_syntax_expr_with_env(expr, ctx, &SyntaxLowerEnv::default())
}
/// Resolves record name for field-access sugar.
///
/// Inputs:
/// - `value`: expression on the left side of `.field`.
/// - `ctx`: module lowering context containing local struct field metadata.
/// - `env`: local lowering environment containing struct-typed locals.
///
/// Output:
/// - Struct record name when the value is a known local struct value.
///
/// Transformation:
/// - Uses parameter/local type metadata and local struct field metadata to
///   turn `user.name` and `plan.package.name` into record access for the
///   correct Erlang record at each path segment.
pub(in crate::emit::syntax) fn resolve_syntax_field_access_struct(
    value: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<String> {
    let type_text = resolve_syntax_field_access_type(value, ctx, env)?;
    syntax_struct_name_from_type_annotation(&type_text, ctx)
}

/// Resolves the source type of an expression used as a field-access receiver.
///
/// Inputs:
/// - `value`: expression whose resulting type is needed.
/// - `ctx`: module lowering context containing local struct fields.
/// - `env`: local lowering environment containing parameter/local types.
///
/// Output:
/// - Source type text for local variables and nested local struct fields.
///
/// Transformation:
/// - Walks field-access chains through `struct_field_types`, allowing the
///   syntax bridge to select the right BEAM record name for nested access.
fn resolve_syntax_field_access_type(
    value: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<String> {
    match value.kind {
        SyntaxExprKind::Var => env.value_types.get(value.text.as_deref()?).cloned(),
        SyntaxExprKind::FieldAccess => {
            let field = value.text.as_deref()?;
            let receiver = value.children.first()?;
            let receiver_type = resolve_syntax_field_access_type(receiver, ctx, env)?;
            let receiver_struct = syntax_struct_name_from_type_annotation(&receiver_type, ctx)?;
            ctx.struct_field_types
                .get(&receiver_struct)
                .and_then(|fields| fields.get(field))
                .cloned()
        }
        _ => None,
    }
}
