use std::collections::BTreeMap;

use super::super::collection_adapter::{
    collection_adapter_name, resource_list_adapter_name, resource_list_parts,
};
use super::super::enum_adapter::enum_adapter_name;
use super::super::exception_adapter::exception_adapter_name;
use super::super::mutation_adapter::{function_uses_mutation_adapter, mutation_adapter_name};
use super::super::owned_value_adapter::{
    function_owned_value_resource, function_owned_value_resource_tuple, owned_tuple_adapter_name,
    owned_tuple_take_name, owned_value_adapter_name, resource_input_copy_name,
    resource_list_input_new_name, resource_list_input_push_name, resource_list_input_resource,
};
use super::super::string_adapter::string_adapter_name;
use super::super::{
    function_symbol, public_cpp_parameter_mappings, CppSymbol, NativeBindingArg,
    NativeBindingFunction, NativeBindingManifest, NativeBindingModule, NativeBindingType,
    NativeBindingTypeKind, NativeFunctionRole,
};
use super::template::HELPER_TEMPLATE;

/// Renders one helper capable of storing every opaque resource in the package.
pub(in crate::commands::bind::cpp_binding_generator) fn render_native_helper(
    manifest: &NativeBindingManifest,
    symbols: &BTreeMap<&str, &CppSymbol>,
) -> Result<String, String> {
    let mut variants = String::new();
    for module in &manifest.modules {
        for ty in module
            .types
            .iter()
            .filter(|ty| ty.kind == NativeBindingTypeKind::OpaqueResource)
        {
            variants.push_str(&format!(
                "    {}(cxx::UniquePtr<ffi::{}>),\n",
                resource_variant(module, ty),
                cpp_type(ty, symbols)?
            ));
        }
    }
    let mut arms = String::new();
    for module in &manifest.modules {
        for function in module
            .functions
            .iter()
            .filter(|function| function.terlan_body.is_none())
        {
            arms.push_str(&render_operation_arm(manifest, module, function, symbols)?);
        }
    }
    let null_failure = render_null_failure(manifest, symbols)?;
    let live_mut = if arms.contains("self.live_mut(") {
        "    fn live_mut(\n        &mut self,\n        handle: &HandleArg,\n        expected_type: &str,\n    ) -> Result<&mut HandleEntry, String> {\n        self.validate(handle, expected_type)?;\n        Ok(self.handles.get_mut(&handle.id).expect(\"validated handle\"))\n    }"
    } else {
        ""
    };
    let arg_handles = if arms.contains("arg_handles(") {
        "fn arg_handles(value: &Arg) -> &[HandleArg] {\n    match value {\n        Arg::Handles(values) => values,\n        Arg::EmptyList => &[],\n        _ => unreachable!(\"generated operation pattern validates resource-list arguments\"),\n    }\n}"
    } else {
        ""
    };

    let crate_ident = manifest.package.crate_name.replace('-', "_");
    Ok(HELPER_TEMPLATE
        .replace("@CRATE@", &crate_ident)
        .replace("@HANDLE_VARIANTS@", &variants)
        .replace("@OPERATION_ARMS@", &arms)
        .replace("@NULL_FAILURE@", &null_failure)
        .replace("@LIVE_MUT@", live_mut)
        .replace("@ARG_HANDLES@", arg_handles)
        .replace(
            "@MAX_FRAME_BYTES@",
            &crate::runtime::native_boundary::adapter_abi::PUBLIC_ADAPTER_MAX_FRAME_BYTES
                .to_string(),
        )
        .replace(
            "@MAX_TRANSFER_BYTES@",
            &crate::runtime::native_boundary::adapter_abi::PUBLIC_ADAPTER_MAX_TRANSFER_BYTES
                .to_string(),
        ))
}

/// Renders a finite package-owned classifier for null native results.
fn render_null_failure(
    manifest: &NativeBindingManifest,
    symbols: &BTreeMap<&str, &CppSymbol>,
) -> Result<String, String> {
    let Some(policy) = &manifest.null_failure else {
        return Ok("fn native_null_failure(default_message: &str) -> String {\n    protocol_error(\"native_null_handle\", default_message)\n}\n".into());
    };
    let probe = symbols
        .get(policy.probe_symbol.as_str())
        .ok_or_else(|| format!("unknown C++ null failure probe `{}`", policy.probe_symbol))?;
    let cases = policy
        .cases
        .iter()
        .map(|case| {
            format!(
                "        {} => protocol_error({:?}, {:?}),\n",
                case.value, case.failure.code, case.failure.message
            )
        })
        .collect::<String>();
    Ok(format!(
        "fn native_null_failure(_default_message: &str) -> String {{\n    match ffi::{}() {{\n{cases}        _ => protocol_error({:?}, {:?}),\n    }}\n}}\n",
        probe.cpp_name, policy.fallback.code, policy.fallback.message
    ))
}

/// Renders one operation dispatch arm from the function's role and types.
fn render_operation_arm(
    manifest: &NativeBindingManifest,
    module: &NativeBindingModule,
    function: &NativeBindingFunction,
    symbols: &BTreeMap<&str, &CppSymbol>,
) -> Result<String, String> {
    let pattern = render_arg_pattern(manifest, &function.args)?;
    let header = format!(
        "            {:?} => {{\n                let {pattern} = request.args.as_slice() else {{\n                    return protocol_error(\"invalid_arguments\", {:?});\n                }};\n",
        function.operation,
        format!("{} received invalid arguments", function.name)
    );
    let body = match function.role {
        NativeFunctionRole::Constructor => {
            let (owner, ty) = find_return_resource(manifest, function)?;
            let symbol = function_symbol(function, symbols)?;
            let call_name =
                if function_owned_value_resource_tuple(manifest, function, symbols).is_some() {
                    owned_tuple_adapter_name(module, function)
                } else if function_owned_value_resource(manifest, function, symbols).is_some() {
                    owned_value_adapter_name(module, function)
                } else {
                    symbol.cpp_name.clone()
                };
            let call = render_ffi_call(manifest, &call_name, &function.args, 0)?;
            let borrowed = render_borrowed_resource_bindings(manifest, &function.args, 0)?;
            let resource_lists = render_resource_list_input_bindings(manifest, function, symbol)?;
            let variant = resource_variant(owner, ty);
            let type_name = resource_type_name(owner, ty);
            format!(
                "{borrowed}{resource_lists}                let value = {call};\n                if value.is_null() {{\n                    return native_null_failure(\"constructor returned null\");\n                }}\n                self.next_id += 1;\n                let id = self.next_id;\n                self.handles.insert(id, HandleEntry {{ generation: 1, type_name: {type_name:?}, value: HandleValue::{variant}(value) }});\n                format!(\"ok_handle {{}} {{id}} 1 {{}}\", STANDARD.encode(self.owner.as_bytes()), STANDARD.encode({type_name:?}))\n"
            )
        }
        NativeFunctionRole::ImmutableMethod | NativeFunctionRole::MutableMethod => {
            let (owner, ty, handle_index) = find_handle_resource(manifest, function)?;
            if handle_index != 0 {
                return Err(format!(
                    "C++ method `{}` requires its resource handle as the first argument",
                    function.name
                ));
            }
            let symbol = function_symbol(function, symbols)?;
            let variant = resource_variant(owner, ty);
            let type_name = resource_type_name(owner, ty);
            let call_args = render_call_args(manifest, &function.args, 1)?;
            let borrowed = render_borrowed_resource_bindings(manifest, &function.args, 1)?;
            let resource_lists = render_resource_list_input_bindings(manifest, function, symbol)?;
            let receiver = if function.role == NativeFunctionRole::MutableMethod
                && function_uses_mutation_adapter(manifest, function)
            {
                return render_contained_mutable_method(
                    manifest, module, function, symbol, owner, ty,
                )
                .map(|body| format!("{header}{body}            }}\n"));
            } else if function.role == NativeFunctionRole::ImmutableMethod
                && (function_owned_value_resource(manifest, function, symbols).is_some()
                    || function_owned_value_resource_tuple(manifest, function, symbols).is_some())
            {
                let adapter =
                    if function_owned_value_resource_tuple(manifest, function, symbols).is_some() {
                        owned_tuple_adapter_name(module, function)
                    } else {
                        owned_value_adapter_name(module, function)
                    };
                let suffix = if call_args.is_empty() {
                    String::new()
                } else {
                    format!(", {call_args}")
                };
                format!(
                    "                let entry = match self.live(arg_0, {type_name:?}) {{ Ok(entry) => entry, Err(error) => return error }};\n                let HandleValue::{variant}(value) = &entry.value else {{ return protocol_error(\"handle_type_mismatch\", {type_name:?}); }};\n{borrowed}{resource_lists}                let result = ffi::{adapter}(value.as_ref().expect(\"validated non-null handle\"){suffix});\n"
                )
            } else if function.role == NativeFunctionRole::MutableMethod {
                format!(
                    "                let entry = match self.live_mut(arg_0, {type_name:?}) {{ Ok(entry) => entry, Err(error) => return error }};\n                let HandleValue::{variant}(value) = &mut entry.value else {{ return protocol_error(\"handle_type_mismatch\", {type_name:?}); }};\n{borrowed}{resource_lists}                let result = value.pin_mut().{}({call_args});\n",
                    symbol.cpp_name
                )
            } else {
                format!(
                    "                let entry = match self.live(arg_0, {type_name:?}) {{ Ok(entry) => entry, Err(error) => return error }};\n                let HandleValue::{variant}(value) = &entry.value else {{ return protocol_error(\"handle_type_mismatch\", {type_name:?}); }};\n{borrowed}{resource_lists}                let result = value.as_ref().expect(\"validated non-null handle\").{}({call_args});\n",
                    symbol.cpp_name
                )
            };
            receiver + &render_function_result(manifest, module, function, symbols)?
        }
        NativeFunctionRole::FreeFunction => {
            let symbol = function_symbol(function, symbols)?;
            let call_name =
                if function_owned_value_resource_tuple(manifest, function, symbols).is_some() {
                    owned_tuple_adapter_name(module, function)
                } else if function_owned_value_resource(manifest, function, symbols).is_some() {
                    owned_value_adapter_name(module, function)
                } else {
                    symbol.cpp_name.clone()
                };
            let call = render_ffi_call(manifest, &call_name, &function.args, 0)?;
            let borrowed = render_borrowed_resource_bindings(manifest, &function.args, 0)?;
            let resource_lists = render_resource_list_input_bindings(manifest, function, symbol)?;
            format!(
                "{borrowed}{resource_lists}                let result = {call};\n{}",
                render_function_result(manifest, module, function, symbols)?
            )
        }
        NativeFunctionRole::MutableFreeFunction => {
            render_mutable_free_function(manifest, module, function, symbols)?
        }
        NativeFunctionRole::ValueProjection => {
            render_value_projection(manifest, module, function, symbols)?
        }
        NativeFunctionRole::OwnedValueProjection => {
            render_owned_value_projection(manifest, module, function, symbols)?
        }
        NativeFunctionRole::EnumProjection => render_enum_projection(manifest, module, function)?,
        NativeFunctionRole::StringProjection => {
            render_string_projection(manifest, module, function)?
        }
        NativeFunctionRole::ScalarProjection => {
            render_scalar_projection(manifest, module, function, symbols)?
        }
        NativeFunctionRole::IntListProjection => {
            render_int_list_projection(manifest, module, function)?
        }
        NativeFunctionRole::ResourceListProjection => {
            render_resource_list_projection(manifest, module, function, symbols)?
        }
        NativeFunctionRole::ExceptionMethod => render_exception_method(manifest, module, function)?,
        NativeFunctionRole::Dispose => {
            let (owner, ty, handle_index) = find_handle_resource(manifest, function)?;
            if handle_index != 0 || function.args.len() != 1 {
                return Err(format!(
                    "dispose function `{}` requires exactly one resource handle",
                    function.name
                ));
            }
            let type_name = resource_type_name(owner, ty);
            format!(
                "                if let Err(error) = self.validate(arg_0, {type_name:?}) {{ return error; }}\n                self.handles.remove(&arg_0.id);\n                \"ok_unit\".to_string()\n"
            )
        }
    };
    Ok(format!("{header}{body}            }}\n"))
}

/// Renders a method mutation through the same compiler-generated containment
/// and pre-borrow resource-copy path used for mutable free functions.
fn render_contained_mutable_method(
    manifest: &NativeBindingManifest,
    module: &NativeBindingModule,
    function: &NativeBindingFunction,
    symbol: &CppSymbol,
    owner: &NativeBindingModule,
    resource: &NativeBindingType,
) -> Result<String, String> {
    let type_name = resource_type_name(owner, resource);
    let variant = resource_variant(owner, resource);
    let copied_resources = render_copied_resource_bindings(manifest, &function.args, 1)?;
    let resource_lists = render_resource_list_input_bindings(manifest, function, symbol)?;
    let args = render_call_args(manifest, &function.args, 1)?;
    let suffix = if args.is_empty() {
        String::new()
    } else {
        format!(", {args}")
    };
    Ok(format!(
        "{copied_resources}{resource_lists}                let entry = match self.live_mut(arg_0, {type_name:?}) {{ Ok(entry) => entry, Err(error) => return error }};\n                let HandleValue::{variant}(value) = &mut entry.value else {{ return protocol_error(\"handle_type_mismatch\", {type_name:?}); }};\n                let envelope = ffi::{}(value.pin_mut(){suffix});\n                let Some(envelope) = envelope.as_ref() else {{ return protocol_error(\"native_exception_envelope\", {:?}); }};\n                if envelope.is_ok() {{\n                    \"ok_unit\".to_string()\n                }} else {{\n                    protocol_error(envelope.code().to_str().unwrap_or(\"native_mutation_failed\"), envelope.message().to_str().unwrap_or(\"native mutation failed\"))\n                }}\n",
        mutation_adapter_name(module, function),
        format!("{} could not allocate a contained result", function.name),
    ))
}

/// Renders a contained free function whose first argument is the sole mutable resource.
fn render_mutable_free_function(
    manifest: &NativeBindingManifest,
    module: &NativeBindingModule,
    function: &NativeBindingFunction,
    symbols: &BTreeMap<&str, &CppSymbol>,
) -> Result<String, String> {
    if function
        .args
        .iter()
        .skip(1)
        .any(|argument| argument.mutable)
    {
        return render_multi_target_mutable_free_function(manifest, module, function, symbols);
    }
    let (owner, resource, handle_index) = find_handle_resource(manifest, function)?;
    if handle_index != 0 {
        return Err(format!(
            "mutable free function `{}` requires its resource target first",
            function.name
        ));
    }
    let symbol = function_symbol(function, symbols)?;
    let type_name = resource_type_name(owner, resource);
    let variant = resource_variant(owner, resource);
    let copied_resources = render_copied_resource_bindings(manifest, &function.args, 1)?;
    let resource_lists = render_resource_list_input_bindings(manifest, function, symbol)?;
    let args = render_call_args(manifest, &function.args, 1)?;
    let suffix = if args.is_empty() {
        String::new()
    } else {
        format!(", {args}")
    };
    Ok(format!(
        "{copied_resources}{resource_lists}                let entry = match self.live_mut(arg_0, {type_name:?}) {{ Ok(entry) => entry, Err(error) => return error }};\n                let HandleValue::{variant}(value) = &mut entry.value else {{ return protocol_error(\"handle_type_mismatch\", {type_name:?}); }};\n                let envelope = ffi::{}(value.pin_mut(){suffix});\n                let Some(envelope) = envelope.as_ref() else {{ return protocol_error(\"native_exception_envelope\", {:?}); }};\n                if envelope.is_ok() {{\n                    \"ok_unit\".to_string()\n                }} else {{\n                    protocol_error(envelope.code().to_str().unwrap_or(\"native_mutation_failed\"), envelope.message().to_str().unwrap_or(\"native mutation failed\"))\n                }}\n",
        mutation_adapter_name(module, function),
        format!("{} could not allocate a contained result", function.name),
    ))
}

/// Renders a contained mutation with multiple independently retained targets.
/// All inputs are copied and all handles are validated before targets are
/// temporarily removed from the table, allowing safe simultaneous mutable CXX
/// borrows without unsafe Rust or aliasing the same public handle twice.
fn render_multi_target_mutable_free_function(
    manifest: &NativeBindingManifest,
    module: &NativeBindingModule,
    function: &NativeBindingFunction,
    symbols: &BTreeMap<&str, &CppSymbol>,
) -> Result<String, String> {
    let symbol = function_symbol(function, symbols)?;
    let mutable = function
        .args
        .iter()
        .enumerate()
        .filter(|(index, argument)| *index == 0 || argument.mutable)
        .map(|(index, argument)| {
            let (owner, resource) =
                find_resource_type(manifest, &argument.ty).ok_or_else(|| {
                    format!(
                        "mutable free function `{}` target `{}` is not an opaque resource",
                        function.name, argument.name
                    )
                })?;
            Ok((
                index,
                resource_type_name(owner, resource),
                resource_variant(owner, resource),
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let copied_resources = render_copied_resource_bindings(manifest, &function.args, 1)?;
    let resource_lists = render_resource_list_input_bindings(manifest, function, symbol)?;
    let distinct = mutable
        .iter()
        .enumerate()
        .flat_map(|(left_index, (left, _, _))| {
            mutable
                .iter()
                .skip(left_index + 1)
                .map(move |(right, _, _)| {
                    format!(
                        "                if arg_{left}.id == arg_{right}.id {{ return protocol_error(\"mutable_handle_alias\", \"mutable output handles must be distinct\"); }}\n"
                    )
                })
        })
        .collect::<String>();
    let validations = mutable
        .iter()
        .map(|(index, type_name, _)| {
            format!(
                "                if let Err(error) = self.validate(arg_{index}, {type_name:?}) {{ return error; }}\n"
            )
        })
        .collect::<String>();
    let removals = mutable
        .iter()
        .map(|(index, _, _)| {
            format!(
                "                let mut arg_{index}_entry = self.handles.remove(&arg_{index}.id).expect(\"validated mutable handle\");\n"
            )
        })
        .collect::<String>();
    let value_bindings = mutable
        .iter()
        .map(|(index, type_name, variant)| {
            format!(
                "                    let HandleValue::{variant}(arg_{index}_value) = &mut arg_{index}_entry.value else {{ unreachable!(\"validated {type_name} handle variant\") }};\n"
            )
        })
        .collect::<String>();
    let reinserts = mutable
        .iter()
        .map(|(index, _, _)| {
            format!("                self.handles.insert(arg_{index}.id, arg_{index}_entry);\n")
        })
        .collect::<String>();
    let args = render_call_args(manifest, &function.args, 1)?;
    let suffix = if args.is_empty() {
        String::new()
    } else {
        format!(", {args}")
    };
    Ok(format!(
        "{copied_resources}{resource_lists}{distinct}{validations}{removals}                let envelope = {{\n{value_bindings}                    ffi::{}(arg_0_value.pin_mut(){suffix})\n                }};\n{reinserts}                let Some(envelope) = envelope.as_ref() else {{ return protocol_error(\"native_exception_envelope\", {:?}); }};\n                if envelope.is_ok() {{\n                    \"ok_unit\".to_string()\n                }} else {{\n                    protocol_error(envelope.code().to_str().unwrap_or(\"native_mutation_failed\"), envelope.message().to_str().unwrap_or(\"native mutation failed\"))\n                }}\n",
        mutation_adapter_name(module, function),
        format!("{} could not allocate a contained result", function.name),
    ))
}

/// Copies secondary resources before a mutable handle-table borrow. This also
/// preserves calls where a secondary input aliases the mutation target.
fn render_copied_resource_bindings(
    manifest: &NativeBindingManifest,
    args: &[NativeBindingArg],
    skip: usize,
) -> Result<String, String> {
    let mut source = String::new();
    for (index, arg) in args.iter().enumerate().skip(skip) {
        if arg.cpp_ignore {
            continue;
        }
        if arg.mutable {
            continue;
        }
        let Some((module, ty)) = find_resource_type(manifest, &arg.ty) else {
            continue;
        };
        let type_name = resource_type_name(module, ty);
        let variant = resource_variant(module, ty);
        let copy = resource_input_copy_name(module, ty);
        source.push_str(&format!(
            "                let arg_{index}_copy = {{\n                    let entry = match self.live(arg_{index}, {type_name:?}) {{ Ok(entry) => entry, Err(error) => return error }};\n                    let HandleValue::{variant}(value) = &entry.value else {{ return protocol_error(\"handle_type_mismatch\", {type_name:?}); }};\n                    ffi::{copy}(value.as_ref().expect(\"validated non-null handle\"))\n                }};\n                if arg_{index}_copy.is_null() {{ return protocol_error(\"native_resource_copy\", \"could not copy resource argument\"); }}\n                let arg_{index}_ref = arg_{index}_copy.as_ref().expect(\"validated non-null resource copy\");\n"
        ));
    }
    Ok(source)
}

/// Renders one contained throwing method into the stable `Result` protocol.
fn render_exception_method(
    manifest: &NativeBindingManifest,
    module: &NativeBindingModule,
    function: &NativeBindingFunction,
) -> Result<String, String> {
    let (owner, resource, handle_index) = find_handle_resource(manifest, function)?;
    if handle_index != 0 {
        return Err(format!(
            "exception method `{}` requires its resource handle first",
            function.name
        ));
    }
    let variant = resource_variant(owner, resource);
    let type_name = resource_type_name(owner, resource);
    let args = render_call_args(manifest, &function.args, 1)?;
    let suffix = if args.is_empty() {
        String::new()
    } else {
        format!(", {args}")
    };
    Ok(format!(
        "                let entry = match self.live(arg_0, {type_name:?}) {{ Ok(entry) => entry, Err(error) => return error }};\n                let HandleValue::{variant}(value) = &entry.value else {{ return protocol_error(\"handle_type_mismatch\", {type_name:?}); }};\n                let envelope = ffi::{}(value.as_ref().expect(\"validated non-null handle\"){suffix});\n                let Some(envelope) = envelope.as_ref() else {{ return protocol_error(\"native_exception_envelope\", {:?}); }};\n                if envelope.is_ok() {{\n                    format!(\"result_ok_int {{}}\", envelope.value())\n                }} else {{\n                    format!(\"result_err {{}} {{}}\", STANDARD.encode(envelope.code().as_bytes()), STANDARD.encode(envelope.message().as_bytes()))\n                }}\n",
        exception_adapter_name(module, function),
        format!("{} could not allocate a contained result", function.name)
    ))
}

/// Renders one symbolic enum getter through its generated C++ adapter.
fn render_enum_projection(
    manifest: &NativeBindingManifest,
    module: &NativeBindingModule,
    function: &NativeBindingFunction,
) -> Result<String, String> {
    let (owner, resource, handle_index) = find_handle_resource(manifest, function)?;
    if handle_index != 0 || function.args.len() != 1 {
        return Err(format!(
            "enum projection `{}` requires exactly one resource handle",
            function.name
        ));
    }
    let variant = resource_variant(owner, resource);
    let type_name = resource_type_name(owner, resource);
    Ok(format!(
        "                let entry = match self.live(arg_0, {type_name:?}) {{ Ok(entry) => entry, Err(error) => return error }};\n                let HandleValue::{variant}(value) = &entry.value else {{ return protocol_error(\"handle_type_mismatch\", {type_name:?}); }};\n                let result = ffi::{}(value.as_ref().expect(\"validated non-null handle\"));\n                let Some(result) = result.as_ref() else {{ return protocol_error(\"native_unknown_enum\", {:?}); }};\n                format!(\"ok_atom {{}}\", STANDARD.encode(result.as_bytes()))\n",
        enum_adapter_name(module, function),
        format!("{} returned an unselected enum value", function.name)
    ))
}

/// Renders a copied C++ value through its generated string adapter.
fn render_string_projection(
    manifest: &NativeBindingManifest,
    module: &NativeBindingModule,
    function: &NativeBindingFunction,
) -> Result<String, String> {
    let (owner, resource, handle_index) = find_handle_resource(manifest, function)?;
    if handle_index != 0 || function.args.len() != 1 {
        return Err(format!(
            "string projection `{}` requires exactly one resource handle",
            function.name
        ));
    }
    let variant = resource_variant(owner, resource);
    let type_name = resource_type_name(owner, resource);
    Ok(format!(
        "                let entry = match self.live(arg_0, {type_name:?}) {{ Ok(entry) => entry, Err(error) => return error }};\n                let HandleValue::{variant}(value) = &entry.value else {{ return protocol_error(\"handle_type_mismatch\", {type_name:?}); }};\n                let result = ffi::{}(value.as_ref().expect(\"validated non-null handle\"));\n                let Some(result) = result.as_ref() else {{ return protocol_error(\"native_string_projection\", {:?}); }};\n                format!(\"ok_string {{}}\", STANDARD.encode(result.as_bytes()))\n",
        string_adapter_name(module, function),
        format!("{} could not be converted to a string", function.name)
    ))
}

/// Renders a contained integer getter into an ordinary public Int response.
fn render_scalar_projection(
    manifest: &NativeBindingManifest,
    module: &NativeBindingModule,
    function: &NativeBindingFunction,
    symbols: &BTreeMap<&str, &CppSymbol>,
) -> Result<String, String> {
    let callable = function_symbol(function, symbols)?;
    let borrowed = render_borrowed_resource_bindings(manifest, &function.args, 0)?;
    let resource_lists = render_resource_list_input_bindings(manifest, function, callable)?;
    let args = render_call_args(manifest, &function.args, 0)?;
    let success = match function.returns.as_str() {
        "Int" => "format!(\"ok_int {}\", envelope.value())",
        "Float" => "format!(\"ok_float {}\", envelope.float_value())",
        "Bool" => "format!(\"ok_bool {}\", envelope.bool_value())",
        returns => {
            return Err(format!(
                "scalar projection `{}` has unsupported result `{returns}`",
                function.name
            ));
        }
    };
    Ok(format!(
        "{borrowed}{resource_lists}                let envelope = ffi::{}({args});\n                let Some(envelope) = envelope.as_ref() else {{ return protocol_error(\"native_exception_envelope\", {:?}); }};\n                if envelope.is_ok() {{\n                    {success}\n                }} else {{\n                    protocol_error(&envelope.code().to_string_lossy(), &envelope.message().to_string_lossy())\n                }}\n",
        exception_adapter_name(module, function),
        format!("{} could not allocate a contained result", function.name)
    ))
}

/// Copies a borrowed C++ integer collection through its generated vector adapter.
fn render_int_list_projection(
    manifest: &NativeBindingManifest,
    module: &NativeBindingModule,
    function: &NativeBindingFunction,
) -> Result<String, String> {
    let (owner, resource, handle_index) = find_handle_resource(manifest, function)?;
    if handle_index != 0 || function.args.len() != 1 {
        return Err(format!(
            "integer-list projection `{}` requires exactly one resource handle",
            function.name
        ));
    }
    let variant = resource_variant(owner, resource);
    let type_name = resource_type_name(owner, resource);
    Ok(format!(
        "                let entry = match self.live(arg_0, {type_name:?}) {{ Ok(entry) => entry, Err(error) => return error }};\n                let HandleValue::{variant}(value) = &entry.value else {{ return protocol_error(\"handle_type_mismatch\", {type_name:?}); }};\n                let result = ffi::{}(value.as_ref().expect(\"validated non-null handle\"));\n{}",
        collection_adapter_name(module, function),
        render_result(manifest, "List[Int]")?
    ))
}

/// Copies a returned C++ vector into independently owned handles. Every native
/// element is copied successfully before any handle is published, preserving
/// failure atomicity at the Terlan boundary.
fn render_resource_list_projection(
    manifest: &NativeBindingManifest,
    module: &NativeBindingModule,
    function: &NativeBindingFunction,
    symbols: &BTreeMap<&str, &CppSymbol>,
) -> Result<String, String> {
    let (receiver, element, callable) = resource_list_parts(manifest, module, function, symbols)?;
    let element_owner = manifest
        .modules
        .iter()
        .find(|candidate| candidate.types.iter().any(|ty| std::ptr::eq(ty, element)))
        .ok_or_else(|| {
            format!(
                "resource-list projection `{}` has no element owner",
                function.name
            )
        })?;
    let element_variant = resource_variant(element_owner, element);
    let element_type = resource_type_name(element_owner, element);
    let adapter = resource_list_adapter_name(module, function);
    let argument_skip = usize::from(callable.kind == super::super::CppSymbolKind::Method);
    let args = render_call_args(manifest, &function.args, argument_skip)?;
    let suffix = if args.is_empty() {
        String::new()
    } else {
        format!(", {args}")
    };
    let borrowed = render_borrowed_resource_bindings(manifest, &function.args, argument_skip)?;
    let resource_lists = render_resource_list_input_bindings(manifest, function, callable)?;
    let invoke = if let Some(receiver) = receiver {
        let (owner, resource, handle_index) = find_handle_resource(manifest, function)?;
        if handle_index != 0 || !type_matches(&function.args[0].ty, receiver) {
            return Err(format!(
                "resource-list projection `{}` requires its receiver handle first",
                function.name
            ));
        }
        let receiver_variant = resource_variant(owner, resource);
        let receiver_type = resource_type_name(owner, resource);
        format!(
            "                    let entry = match self.live(arg_0, {receiver_type:?}) {{ Ok(entry) => entry, Err(error) => return error }};\n                    let HandleValue::{receiver_variant}(value) = &entry.value else {{ return protocol_error(\"handle_type_mismatch\", {receiver_type:?}); }};\n{borrowed}{resource_lists}                    let envelope = ffi::{adapter}(value.as_ref().expect(\"validated non-null handle\"){suffix});\n"
        )
    } else {
        format!(
            "{borrowed}{resource_lists}                    let envelope = ffi::{adapter}({args});\n"
        )
    };
    Ok(format!(
        "                let values = {{\n{invoke}                    let Some(envelope) = envelope.as_ref() else {{ return protocol_error(\"native_resource_list_envelope\", {:?}); }};\n                    if !ffi::{adapter}_is_ok(envelope) {{\n                        return protocol_error(&ffi::{adapter}_code(envelope).to_string_lossy(), &ffi::{adapter}_message(envelope).to_string_lossy());\n                    }}\n                    let mut values = Vec::with_capacity(ffi::{adapter}_len(envelope));\n                    for index in 0..ffi::{adapter}_len(envelope) {{\n                        let value = ffi::{adapter}_element(envelope, index);\n                        if value.is_null() {{ return protocol_error(\"native_resource_list_element\", {:?}); }}\n                        values.push(value);\n                    }}\n                    values\n                }};\n                let mut handles = Vec::with_capacity(values.len());\n                for value in values {{\n                    self.next_id += 1;\n                    let id = self.next_id;\n                    let generation = 1;\n                    self.handles.insert(id, HandleEntry {{ generation, type_name: {element_type:?}, value: HandleValue::{element_variant}(value) }});\n                    handles.push(format!(\"{{}}:{{id}}:{{generation}}:{{}}\", STANDARD.encode(self.owner.as_bytes()), STANDARD.encode({element_type:?})));\n                }}\n                if handles.is_empty() {{ \"ok_handles\".to_string() }} else {{ format!(\"ok_handles {{}}\", handles.join(\",\")) }}\n",
        format!("{} could not allocate a contained result", function.name),
        format!("{} could not copy a returned resource", function.name),
    ))
}

/// Renders a copied record assembled from reviewed primitive getter symbols.
fn render_value_projection(
    manifest: &NativeBindingManifest,
    module: &NativeBindingModule,
    function: &NativeBindingFunction,
    symbols: &BTreeMap<&str, &CppSymbol>,
) -> Result<String, String> {
    let record = module
        .types
        .iter()
        .find(|ty| {
            ty.kind == NativeBindingTypeKind::ValueRecord && type_matches(&function.returns, ty)
        })
        .ok_or_else(|| format!("unknown value record `{}`", function.returns))?;
    let (owner, resource, handle_index) = find_handle_resource(manifest, function)?;
    if handle_index != 0 || function.args.len() != 1 {
        return Err(format!(
            "value projection `{}` requires exactly one resource handle",
            function.name
        ));
    }
    let variant = resource_variant(owner, resource);
    let type_name = resource_type_name(owner, resource);
    let mut source = format!(
        "                let entry = match self.live(arg_0, {type_name:?}) {{ Ok(entry) => entry, Err(error) => return error }};\n                let HandleValue::{variant}(value) = &entry.value else {{ return protocol_error(\"handle_type_mismatch\", {type_name:?}); }};\n                let value = value.as_ref().expect(\"validated non-null handle\");\n"
    );
    let mut encoded_fields = Vec::new();
    let mut format_args = Vec::new();
    for (index, projection) in function.projections.iter().enumerate() {
        let symbol = symbols
            .get(projection.cpp_symbol.as_str())
            .ok_or_else(|| format!("unknown C++ projection symbol `{}`", projection.cpp_symbol))?;
        source.push_str(&format!(
            "                let field_{index} = value.{}();\n",
            symbol.cpp_name
        ));
        let field = record
            .fields
            .iter()
            .find(|field| field.name == projection.field)
            .ok_or_else(|| {
                format!(
                    "record `{}` has no projected field `{}`",
                    record.name, projection.field
                )
            })?;
        let kind = match field.ty.as_str() {
            "Int" => "i",
            "Float" => "f",
            "Bool" => "b",
            _ => {
                return Err(format!(
                    "native helper cannot encode copied record field type `{}`",
                    field.ty
                ));
            }
        };
        encoded_fields.push(format!("{{}}:{kind}:{{}}"));
        format_args.push(format!("STANDARD.encode({:?})", projection.field));
        format_args.push(format!("field_{index}"));
    }
    source.push_str(&format!(
        "                format!({:?}, STANDARD.encode({:?}), {})\n",
        format!("ok_record {{}} {}", encoded_fields.join(",")),
        record.name,
        format_args.join(", ")
    ));
    Ok(source)
}

/// Renders a copied record projected from one temporary uniquely owned C++ value.
fn render_owned_value_projection(
    manifest: &NativeBindingManifest,
    _module: &NativeBindingModule,
    function: &NativeBindingFunction,
    symbols: &BTreeMap<&str, &CppSymbol>,
) -> Result<String, String> {
    let record = find_value_record_type(manifest, &function.returns)
        .ok_or_else(|| format!("unknown value record `{}`", function.returns))?;
    let producer = function_symbol(function, symbols)?;
    let call = render_ffi_call(manifest, &producer.cpp_name, &function.args, 0)?;
    let borrowed = render_borrowed_resource_bindings(manifest, &function.args, 0)?;
    let mut source = format!(
        "{borrowed}                let result = {call};\n                if result.is_null() {{\n                    return native_null_failure(\"owned value projection returned null\");\n                }}\n                let value = result.as_ref().expect(\"validated non-null owned value\");\n"
    );
    let mut encoded_fields = Vec::new();
    let mut format_args = Vec::new();
    for (index, projection) in function.projections.iter().enumerate() {
        let symbol = symbols
            .get(projection.cpp_symbol.as_str())
            .ok_or_else(|| format!("unknown C++ projection symbol `{}`", projection.cpp_symbol))?;
        source.push_str(&format!(
            "                let field_{index} = value.{}();\n",
            symbol.cpp_name
        ));
        let field = record
            .fields
            .iter()
            .find(|field| field.name == projection.field)
            .ok_or_else(|| {
                format!(
                    "record `{}` has no projected field `{}`",
                    record.name, projection.field
                )
            })?;
        let kind = match field.ty.as_str() {
            "Int" => "i",
            "Float" => "f",
            "Bool" => "b",
            _ => {
                return Err(format!(
                    "native helper cannot encode copied record field type `{}`",
                    field.ty
                ));
            }
        };
        encoded_fields.push(format!("{{}}:{kind}:{{}}"));
        format_args.push(format!("STANDARD.encode({:?})", projection.field));
        format_args.push(format!("field_{index}"));
    }
    source.push_str(&format!(
        "                format!({:?}, STANDARD.encode({:?}), {})\n",
        format!("ok_record {{}} {}", encoded_fields.join(",")),
        record.name,
        format_args.join(", ")
    ));
    Ok(source)
}

/// Renders a slice pattern that preserves argument positions without using names.
fn render_arg_pattern(
    manifest: &NativeBindingManifest,
    args: &[NativeBindingArg],
) -> Result<String, String> {
    let mut patterns = Vec::new();
    for (index, arg) in args.iter().enumerate() {
        let binding = if arg.cpp_ignore {
            format!("_arg_{index}")
        } else {
            format!("arg_{index}")
        };
        let pattern = match arg.ty.as_str() {
            "Int" => format!("Arg::Int({binding})"),
            "Float" => format!("Arg::Float({binding})"),
            "Bool" => format!("Arg::Bool({binding})"),
            "String" => format!("Arg::String({binding})"),
            "std.vm.Bytes.Bytes" | "Bytes" => format!("Arg::Bytes({binding})"),
            "List[Int]" => format!("{binding} @ (Arg::Ints(_) | Arg::EmptyList)"),
            "List[Float]" => format!("{binding} @ (Arg::Floats(_) | Arg::EmptyList)"),
            _ if find_resource_list_type(manifest, &arg.ty).is_some() => {
                format!("{binding} @ (Arg::Handles(_) | Arg::EmptyList)")
            }
            _ if find_enum_type(manifest, &arg.ty).is_some() => {
                // Public compatibility surfaces may expose reviewed C++ enum
                // values as their stable integer codes (for example the
                // legacy Tensor factory helpers), while newer typed surfaces
                // use the generated enum atoms. Accept both wire forms and
                // normalize them at the call site.
                format!("{binding} @ (Arg::Atom(_) | Arg::Int(_))")
            }
            _ if find_string_value_type(manifest, &arg.ty).is_some() => {
                format!("Arg::String({binding})")
            }
            _ if find_value_record_type(manifest, &arg.ty).is_some() => {
                format!("Arg::Record({binding})")
            }
            _ if is_resource_type(&arg.ty) => format!("Arg::Handle({binding})"),
            _ => {
                return Err(format!(
                    "native helper cannot decode argument type `{}` for `{}`",
                    arg.ty, arg.name
                ));
            }
        };
        patterns.push(pattern);
    }
    Ok(format!("[{}]", patterns.join(", ")))
}

/// Renders a direct free-function or constructor call.
fn render_ffi_call(
    manifest: &NativeBindingManifest,
    name: &str,
    args: &[NativeBindingArg],
    skip: usize,
) -> Result<String, String> {
    Ok(format!(
        "ffi::{name}({})",
        render_call_args(manifest, args, skip)?
    ))
}

/// Renders call arguments after any receiver handle.
fn render_call_args(
    manifest: &NativeBindingManifest,
    args: &[NativeBindingArg],
    skip: usize,
) -> Result<String, String> {
    args.iter()
        .enumerate()
        .skip(skip)
        .map(|(index, arg)| {
            if arg.cpp_ignore {
                return Ok(Vec::new());
            }
            if args
                .get(index + 1)
                .is_some_and(|next| next.prepend_resource)
            {
                return Ok(Vec::new());
            }
            if let Some(ty) = find_enum_type(manifest, &arg.ty) {
                return render_enum_argument(manifest, ty, index, &arg.name)
                    .map(|value| vec![value]);
            }
            if find_string_value_type(manifest, &arg.ty).is_some() {
                return Ok(vec![format!("arg_{index}.as_str()")]);
            }
            if let Some(ty) = find_value_record_type(manifest, &arg.ty) {
                return render_record_arguments(ty, arg, index);
            }
            if find_resource_type(manifest, &arg.ty).is_some() {
                return Ok(vec![if arg.mutable {
                    format!("arg_{index}_value.pin_mut()")
                } else {
                    format!("arg_{index}_ref")
                }]);
            }
            if find_resource_list_type(manifest, &arg.ty).is_some() {
                return Ok(vec![format!("arg_{index}_list_ref")]);
            }
            match arg.ty.as_str() {
                "Int" | "Float" | "Bool" => Ok(vec![format!("*arg_{index}")]),
                "String" => Ok(vec![format!("arg_{index}.as_str()")]),
                "std.vm.Bytes.Bytes" | "Bytes" => Ok(vec![format!("arg_{index}.as_slice()")]),
                "List[Int]" => Ok(vec![format!("arg_ints(arg_{index})")]),
                "List[Float]" => Ok(vec![format!("arg_floats(arg_{index})")]),
                _ => Err(format!(
                    "native helper cannot pass argument type `{}` for `{}`",
                    arg.ty, arg.name
                )),
            }
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|args| args.into_iter().flatten().collect::<Vec<_>>().join(", "))
}

/// Validates and borrows secondary opaque-resource arguments for one call.
fn render_borrowed_resource_bindings(
    manifest: &NativeBindingManifest,
    args: &[NativeBindingArg],
    skip: usize,
) -> Result<String, String> {
    let mut source = String::new();
    for (index, arg) in args.iter().enumerate().skip(skip) {
        if arg.cpp_ignore {
            continue;
        }
        if args
            .get(index + 1)
            .is_some_and(|next| next.prepend_resource)
        {
            continue;
        }
        if arg.mutable {
            continue;
        }
        let Some((module, ty)) = find_resource_type(manifest, &arg.ty) else {
            continue;
        };
        let type_name = resource_type_name(module, ty);
        let variant = resource_variant(module, ty);
        source.push_str(&format!(
            "                let arg_{index}_entry = match self.live(arg_{index}, {type_name:?}) {{ Ok(entry) => entry, Err(error) => return error }};\n                let HandleValue::{variant}(arg_{index}_value) = &arg_{index}_entry.value else {{ return protocol_error(\"handle_type_mismatch\", {type_name:?}); }};\n                let arg_{index}_ref = arg_{index}_value.as_ref().expect(\"validated non-null handle\");\n"
        ));
    }
    Ok(source)
}

/// Copies validated handles into generated C++ collectors. The C++ values are
/// independent of the handle table before the upstream callable is entered.
fn render_resource_list_input_bindings(
    manifest: &NativeBindingManifest,
    function: &NativeBindingFunction,
    symbol: &CppSymbol,
) -> Result<String, String> {
    let mut source = String::new();
    for mapping in public_cpp_parameter_mappings(function, symbol) {
        let parameter = &symbol.parameters[mapping.cpp_parameter_index];
        let argument_index = mapping.public_argument_index;
        let argument = &function.args[argument_index];
        let Some((owner, resource, _)) =
            resource_list_input_resource(manifest, Some(&argument.ty), &parameter.ty)
        else {
            continue;
        };
        let type_name = resource_type_name(owner, resource);
        let variant = resource_variant(owner, resource);
        let new = resource_list_input_new_name(owner, resource);
        let push = resource_list_input_push_name(owner, resource);
        source.push_str(&format!(
            "                let mut arg_{argument_index}_list = ffi::{new}();\n                if arg_{argument_index}_list.is_null() {{ return protocol_error(\"native_resource_list_input\", \"could not allocate resource-list argument\"); }}\n"
        ));
        if argument.prepend_resource {
            let prefix_index = argument_index
                .checked_sub(1)
                .expect("validated prepended resource argument");
            source.push_str(&format!(
                "                let prefix_entry = match self.live(arg_{prefix_index}, {type_name:?}) {{ Ok(entry) => entry, Err(error) => return error }};\n                let HandleValue::{variant}(prefix_value) = &prefix_entry.value else {{ return protocol_error(\"handle_type_mismatch\", {type_name:?}); }};\n                if !ffi::{push}(arg_{argument_index}_list.pin_mut(), prefix_value.as_ref().expect(\"validated non-null handle\")) {{ return protocol_error(\"native_resource_list_input\", \"could not copy prepended resource-list argument\"); }}\n"
            ));
        }
        source.push_str(&format!(
            "                for handle in arg_handles(arg_{argument_index}) {{\n                    let entry = match self.live(handle, {type_name:?}) {{ Ok(entry) => entry, Err(error) => return error }};\n                    let HandleValue::{variant}(value) = &entry.value else {{ return protocol_error(\"handle_type_mismatch\", {type_name:?}); }};\n                    if !ffi::{push}(arg_{argument_index}_list.pin_mut(), value.as_ref().expect(\"validated non-null handle\")) {{ return protocol_error(\"native_resource_list_input\", \"could not copy resource-list argument\"); }}\n                }}\n                let arg_{argument_index}_list_ref = arg_{argument_index}_list.as_ref().expect(\"validated non-null resource-list collector\");\n"
        ));
    }
    Ok(source)
}

/// Expands one copied record argument into reviewed scalar C++ call arguments.
fn render_record_arguments(
    ty: &NativeBindingType,
    argument: &NativeBindingArg,
    index: usize,
) -> Result<Vec<String>, String> {
    argument
        .fields
        .iter()
        .map(|mapping| {
            let field = ty
                .fields
                .iter()
                .find(|field| field.name == mapping.field)
                .ok_or_else(|| {
                    format!(
                        "record argument `{}` references unknown field `{}`",
                        argument.name, mapping.field
                    )
                })?;
            let accessor = match field.ty.as_str() {
                "Int" => "int",
                "Float" => "float",
                "Bool" => "bool",
                _ => {
                    return Err(format!(
                        "native helper cannot copy record field type `{}` for `{}.{}`",
                        field.ty, argument.name, field.name
                    ));
                }
            };
            Ok(format!(
                "match arg_{index}.{accessor}({:?}, {:?}) {{ Ok(value) => value, Err(error) => return error }}",
                ty.name, field.name
            ))
        })
        .collect()
}

/// Converts one reviewed Terlan enum atom into its package-owned integer code.
fn render_enum_argument(
    manifest: &NativeBindingManifest,
    ty: &NativeBindingType,
    index: usize,
    argument_name: &str,
) -> Result<String, String> {
    if ty.variants.is_empty() {
        return Err(format!("enum `{}` has no reviewed variants", ty.name));
    }
    let symbol = manifest
        .cpp_metadata
        .symbols
        .iter()
        .find(|symbol| symbol.id == ty.cpp_symbol)
        .ok_or_else(|| format!("unknown C++ enum symbol `{}`", ty.cpp_symbol))?;
    let arms = ty
        .variants
        .iter()
        .map(|variant| {
            let value = symbol
                .enum_values
                .iter()
                .find(|value| value.name == variant.cpp_name)
                .ok_or_else(|| {
                    format!(
                        "enum `{}` variant `{}` has no extracted value",
                        ty.name, variant.cpp_name
                    )
                })?
                .value
                .parse::<i64>()
                .map_err(|_| {
                    format!(
                        "enum `{}` variant `{}` is outside the helper integer range",
                        ty.name, variant.cpp_name
                    )
                })?;
            Ok(format!("{:?} => {value}_i64", variant.atom))
        })
        .collect::<Result<Vec<_>, String>>()?
        .join(", ");
    Ok(format!(
        "match arg_{index} {{ Arg::Atom(value) => match value.as_str() {{ {arms}, _ => return protocol_error(\"invalid_enum_value\", {:?}) }}, Arg::Int(value) => *value, _ => unreachable!(\"generated enum argument pattern validated the wire type\") }}",
        format!("{argument_name} received an unselected enum value")
    ))
}

/// Converts one supported result into the stable helper protocol.
fn render_result(manifest: &NativeBindingManifest, ty: &str) -> Result<String, String> {
    match ty {
        "Int" => Ok("                format!(\"ok_int {result}\")\n".into()),
        "Float" => Ok("                format!(\"ok_float {result}\")\n".into()),
        "Bool" => Ok("                format!(\"ok_bool {result}\")\n".into()),
        "String" => Ok(render_owned_copy(
            manifest,
            "string",
            "format!(\"ok_string {}\", STANDARD.encode(result.as_bytes()))",
        )),
        "std.vm.Bytes.Bytes" | "Bytes" => Ok(render_owned_copy(
            manifest,
            "byte buffer",
            "format!(\"ok_bytes {}\", STANDARD.encode(result.as_slice()))",
        )),
        "List[Int]" => Ok(render_owned_copy(
            manifest,
            "integer list",
            "if result.is_empty() { \"ok_ints\".to_string() } else { format!(\"ok_ints {}\", result.iter().map(i64::to_string).collect::<Vec<_>>().join(\",\")) }",
        )),
        "List[Float]" => Ok(render_owned_copy(
            manifest,
            "float list",
            "if result.is_empty() { \"ok_floats\".to_string() } else { format!(\"ok_floats {}\", result.iter().map(f64::to_string).collect::<Vec<_>>().join(\",\")) }",
        )),
        "List[Bool]" => Ok(render_owned_copy(
            manifest,
            "boolean list",
            "if result.is_empty() { \"ok_bools\".to_string() } else { format!(\"ok_bools {}\", result.iter().map(|value| (*value != 0).to_string()).collect::<Vec<_>>().join(\",\")) }",
        )),
        "List[String]" => Ok(render_owned_copy(
            manifest,
            "string list",
            "if result.is_empty() { \"ok_strings\".to_string() } else { format!(\"ok_strings {}\", result.iter().map(|value| STANDARD.encode(value.as_bytes())).collect::<Vec<_>>().join(\",\")) }",
        )),
        "Unit" => {
            Ok("                let _ = result;\n                \"ok_unit\".to_string()\n".into())
        }
        _ => Err(format!("native helper cannot encode result type `{ty}`")),
    }
}

/// Renders either an ordinary copied result or a newly owned resource handle.
fn render_function_result(
    manifest: &NativeBindingManifest,
    module: &NativeBindingModule,
    function: &NativeBindingFunction,
    symbols: &BTreeMap<&str, &CppSymbol>,
) -> Result<String, String> {
    if let Some((resources, _)) = function_owned_value_resource_tuple(manifest, function, symbols) {
        let takes = resources
            .iter()
            .enumerate()
            .map(|(index, _)| {
                format!(
                    "                let value_{index} = ffi::{}(result.pin_mut());\n",
                    owned_tuple_take_name(module, function, index)
                )
            })
            .collect::<String>();
        let null_checks = resources
            .iter()
            .enumerate()
            .map(|(index, _)| {
                format!(
                    "                if value_{index}.is_null() {{ return native_null_failure(\"native tuple element returned null\"); }}\n"
                )
            })
            .collect::<String>();
        let stores = resources
            .iter()
            .enumerate()
            .map(|(index, (owner, resource, _))| {
                let variant = resource_variant(owner, resource);
                let type_name = resource_type_name(owner, resource);
                format!(
                    "                self.next_id += 1;\n                let id_{index} = self.next_id;\n                self.handles.insert(id_{index}, HandleEntry {{ generation: 1, type_name: {type_name:?}, value: HandleValue::{variant}(value_{index}) }});\n                encoded.push(format!(\"{{}}:{{}}:1:{{}}\", STANDARD.encode(self.owner.as_bytes()), id_{index}, STANDARD.encode({type_name:?})));\n"
                )
            })
            .collect::<String>();
        return Ok(format!(
            "                if result.is_null() {{ return native_null_failure(\"native tuple result returned null\"); }}\n                let mut result = result;\n{takes}{null_checks}                let mut encoded = Vec::with_capacity({});\n{stores}                format!(\"ok_tuple_handles {{}}\", encoded.join(\",\"))\n",
            resources.len()
        ));
    }
    if let Some((owner, resource)) = find_resource_type(manifest, &function.returns) {
        let variant = resource_variant(owner, resource);
        let type_name = resource_type_name(owner, resource);
        return Ok(format!(
            "                if result.is_null() {{ return native_null_failure(\"native operation returned null\"); }}\n                self.next_id += 1;\n                let id = self.next_id;\n                self.handles.insert(id, HandleEntry {{ generation: 1, type_name: {type_name:?}, value: HandleValue::{variant}(result) }});\n                format!(\"ok_handle {{}} {{id}} 1 {{}}\", STANDARD.encode(self.owner.as_bytes()), STANDARD.encode({type_name:?}))\n"
        ));
    }
    if let Some((owner, resource)) = find_optional_resource_type(manifest, &function.returns) {
        let variant = resource_variant(owner, resource);
        let type_name = resource_type_name(owner, resource);
        return Ok(format!(
            "                if result.is_null() {{\n                    \"ok_none\".to_string()\n                }} else {{\n                    self.next_id += 1;\n                    let id = self.next_id;\n                    self.handles.insert(id, HandleEntry {{ generation: 1, type_name: {type_name:?}, value: HandleValue::{variant}(result) }});\n                    format!(\"ok_some_handle {{}} {{id}} 1 {{}}\", STANDARD.encode(self.owner.as_bytes()), STANDARD.encode({type_name:?}))\n                }}\n"
        ));
    }
    render_result(manifest, &function.returns)
}

/// Copies a non-null owned C++ standard-library value into the helper reply.
fn render_owned_copy(manifest: &NativeBindingManifest, kind: &str, expression: &str) -> String {
    if manifest.null_failure.is_some() {
        return format!(
            "                let Some(result) = result.as_ref() else {{ return native_null_failure(\"{kind} result was null\"); }};\n                {expression}\n"
        );
    }
    format!(
        "                let Some(result) = result.as_ref() else {{ return protocol_error(\"native_null_value\", \"{kind} result was null\"); }};\n                {expression}\n"
    )
}

/// Finds the resource returned by a constructor.
fn find_return_resource<'a>(
    manifest: &'a NativeBindingManifest,
    function: &NativeBindingFunction,
) -> Result<(&'a NativeBindingModule, &'a NativeBindingType), String> {
    find_resource_type(manifest, &function.returns).ok_or_else(|| {
        format!(
            "constructor `{}` must return a package-owned resource",
            function.name
        )
    })
}

/// Finds the resource-handle argument used by a method or disposer.
fn find_handle_resource<'a>(
    manifest: &'a NativeBindingManifest,
    function: &NativeBindingFunction,
) -> Result<(&'a NativeBindingModule, &'a NativeBindingType, usize), String> {
    for (index, arg) in function.args.iter().enumerate() {
        if let Some((module, ty)) = find_resource_type(manifest, &arg.ty) {
            return Ok((module, ty, index));
        }
    }
    Err(format!(
        "function `{}` requires a package-owned resource",
        function.name
    ))
}

/// Resolves a local or fully qualified Terlan type to its owning resource module.
fn find_resource_type<'a>(
    manifest: &'a NativeBindingManifest,
    value: &str,
) -> Option<(&'a NativeBindingModule, &'a NativeBindingType)> {
    manifest.modules.iter().find_map(|module| {
        module
            .types
            .iter()
            .find(|ty| ty.kind == NativeBindingTypeKind::OpaqueResource && type_matches(value, ty))
            .map(|ty| (module, ty))
    })
}

/// Resolves an optional opaque resource return (`Option[Resource]`). The
/// generated CXX wrapper represents the option as a nullable unique pointer;
/// the helper turns that pointer into the stable optional-handle protocol.
fn find_optional_resource_type<'a>(
    manifest: &'a NativeBindingManifest,
    value: &str,
) -> Option<(&'a NativeBindingModule, &'a NativeBindingType)> {
    value
        .strip_prefix("Option[")
        .and_then(|inner| inner.strip_suffix(']'))
        .and_then(|inner| find_resource_type(manifest, inner.trim()))
}

fn find_resource_list_type<'a>(
    manifest: &'a NativeBindingManifest,
    value: &str,
) -> Option<(&'a NativeBindingModule, &'a NativeBindingType)> {
    value
        .strip_prefix("List[")
        .and_then(|element| element.strip_suffix(']'))
        .and_then(|element| find_resource_type(manifest, element.trim()))
}

/// Resolves a local or fully qualified Terlan type to a generated finite enum.
fn find_enum_type<'a>(
    manifest: &'a NativeBindingManifest,
    value: &str,
) -> Option<&'a NativeBindingType> {
    manifest
        .modules
        .iter()
        .flat_map(|module| &module.types)
        .find(|ty| ty.kind == NativeBindingTypeKind::Enum && type_matches(value, ty))
}

/// Resolves a local or fully qualified Terlan type to a copied value record.
fn find_value_record_type<'a>(
    manifest: &'a NativeBindingManifest,
    value: &str,
) -> Option<&'a NativeBindingType> {
    manifest
        .modules
        .iter()
        .flat_map(|module| &module.types)
        .find(|ty| ty.kind == NativeBindingTypeKind::ValueRecord && type_matches(value, ty))
}

/// Resolves a transparent String mapped to a constructed C++ value.
fn find_string_value_type<'a>(
    manifest: &'a NativeBindingManifest,
    value: &str,
) -> Option<&'a NativeBindingType> {
    manifest
        .modules
        .iter()
        .flat_map(|module| &module.types)
        .find(|ty| ty.kind == NativeBindingTypeKind::StringValue && type_matches(value, ty))
}

/// Returns whether a Terlan type name denotes a generated resource.
fn type_matches(value: &str, ty: &NativeBindingType) -> bool {
    value == ty.name || value.ends_with(&format!(".{}", ty.name))
}

/// Returns whether a helper argument is syntactically a generated type.
fn is_resource_type(value: &str) -> bool {
    value
        .split('.')
        .next_back()
        .and_then(|name| name.chars().next())
        .is_some_and(char::is_uppercase)
        && !matches!(value, "Int" | "Bool" | "String" | "Unit")
}

/// Resolves a generated resource to its C++ type name.
fn cpp_type<'a>(
    ty: &NativeBindingType,
    symbols: &'a BTreeMap<&str, &CppSymbol>,
) -> Result<&'a str, String> {
    symbols
        .get(ty.cpp_symbol.as_str())
        .map(|symbol| symbol.cpp_name.as_str())
        .ok_or_else(|| format!("unknown C++ type symbol `{}`", ty.cpp_symbol))
}

/// Creates a collision-resistant Rust enum variant for a module-owned resource.
fn resource_variant(module: &NativeBindingModule, ty: &NativeBindingType) -> String {
    module
        .module
        .split('.')
        .chain(std::iter::once(ty.name.as_str()))
        .map(upper_camel_identifier)
        .collect()
}

/// Converts one validated source identifier into a warning-clean Rust variant segment.
fn upper_camel_identifier(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut uppercase_next = true;
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            result.push(if uppercase_next {
                ch.to_ascii_uppercase()
            } else {
                ch
            });
            uppercase_next = false;
        } else {
            uppercase_next = true;
        }
    }
    result
}

/// Returns the wire-visible fully qualified Terlan resource type.
fn resource_type_name(module: &NativeBindingModule, ty: &NativeBindingType) -> String {
    format!("{}.{}", module.module, ty.name)
}
