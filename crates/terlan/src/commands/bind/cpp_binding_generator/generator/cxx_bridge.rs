use super::*;

pub(super) fn render_cxx_bridge(
    manifest: &NativeBindingManifest,
    symbols: &ValidatedCppSymbols<'_>,
) -> Result<String, CppBindingError> {
    let header = file_name(&manifest.cpp_metadata.header)?;
    let mut source = format!(
        "#![deny(unsafe_op_in_unsafe_fn)]\n\n#[cxx::bridge(namespace = {:?})]\npub mod ffi {{\n    unsafe extern \"C++\" {{\n        include!({:?});\n",
        manifest.cpp_metadata.namespace,
        format!("include/{header}")
    );
    if has_enum_adapters(manifest) {
        source.push_str("        include!(\"include/terlan_enum_adapters.hpp\");\n");
    }
    if has_string_adapters(manifest) {
        source.push_str("        include!(\"include/terlan_string_adapters.hpp\");\n");
    }
    if has_collection_adapters(manifest) {
        source.push_str("        include!(\"include/terlan_collection_adapters.hpp\");\n");
    }
    if has_exception_adapters(manifest) {
        source.push_str("        include!(\"include/terlan_exception_adapters.hpp\");\n");
    }
    if has_owned_value_adapters(manifest, &symbols.declarations) {
        source.push_str("        include!(\"include/terlan_owned_value_adapters.hpp\");\n");
    }
    if has_mutation_adapters(manifest) {
        source.push_str("        include!(\"include/terlan_mutation_adapters.hpp\");\n");
    }
    let mut opaque_symbols = manifest
        .modules
        .iter()
        .flat_map(|module| &module.types)
        .filter(|ty| ty.kind == NativeBindingTypeKind::OpaqueResource)
        .map(|ty| ty.cpp_symbol.as_str())
        .collect::<BTreeSet<_>>();
    for module in &manifest.modules {
        for function in module
            .functions
            .iter()
            .filter(|function| function.role == NativeFunctionRole::OwnedValueProjection)
        {
            let record = manifest
                .modules
                .iter()
                .flat_map(|owner| &owner.types)
                .find(|ty| {
                    ty.kind == NativeBindingTypeKind::ValueRecord
                        && terlan_type_matches(&function.returns, &ty.name)
                })
                .ok_or_else(|| {
                    format!(
                        "owned value projection `{}` has no returned record",
                        function.name
                    )
                })?;
            opaque_symbols.insert(record.cpp_symbol.as_str());
        }
    }
    for symbol in symbols
        .declarations
        .values()
        .filter(|symbol| opaque_symbols.contains(symbol.id.as_str()))
    {
        let cpp_namespace = symbol
            .overload_set
            .rsplit_once("::")
            .map(|(namespace, _)| namespace)
            .unwrap_or(&manifest.cpp_metadata.namespace);
        if cpp_namespace == manifest.cpp_metadata.namespace {
            source.push_str(&format!("        type {};\n", symbol.cpp_name));
        } else {
            source.push_str(&format!(
                "        #[namespace = {:?}]\n        type {};\n",
                cpp_namespace, symbol.cpp_name
            ));
        }
    }
    for (module, resource, symbol) in used_resource_list_inputs(manifest, &symbols.declarations) {
        let collector = resource_list_input_name(module, resource);
        source.push_str(&format!(
            "        type {collector};\n        fn {}() -> UniquePtr<{collector}>;\n        fn {}(values: Pin<&mut {collector}>, value: &{}) -> bool;\n",
            resource_list_input_new_name(module, resource),
            resource_list_input_push_name(module, resource),
            symbol.cpp_name,
        ));
    }
    for (module, resource, symbol) in
        used_mutable_secondary_resources(manifest, &symbols.declarations)
    {
        source.push_str(&format!(
            "        fn {}(value: &{}) -> UniquePtr<{}>;\n",
            resource_input_copy_name(module, resource),
            symbol.cpp_name,
            symbol.cpp_name,
        ));
    }
    let adapted_enum_symbols = manifest
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .filter(|function| function.role == NativeFunctionRole::EnumProjection)
        .filter_map(|function| function.cpp_symbol.as_deref())
        .collect::<BTreeSet<_>>();
    let adapted_string_symbols = manifest
        .modules
        .iter()
        .flat_map(|module| {
            let stringifiers = module
                .types
                .iter()
                .filter(|ty| ty.kind == NativeBindingTypeKind::StringValue)
                .filter_map(|ty| ty.stringifier.as_deref());
            let getters = module
                .functions
                .iter()
                .filter(|function| function.role == NativeFunctionRole::StringProjection)
                .filter_map(|function| function.cpp_symbol.as_deref());
            stringifiers.chain(getters)
        })
        .collect::<BTreeSet<_>>();
    let contained_exception_symbols = manifest
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .filter(|function| {
            matches!(
                function.role,
                NativeFunctionRole::ExceptionMethod | NativeFunctionRole::ScalarProjection
            )
        })
        .filter_map(|function| function.cpp_symbol.as_deref())
        .collect::<BTreeSet<_>>();
    let adapted_collection_symbols = manifest
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .filter(|function| {
            matches!(
                function.role,
                NativeFunctionRole::IntListProjection | NativeFunctionRole::ResourceListProjection
            )
        })
        .filter_map(|function| function.cpp_symbol.as_deref())
        .collect::<BTreeSet<_>>();
    let owned_value_symbols = manifest
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .filter(|function| {
            function_owned_value_resource(manifest, function, &symbols.declarations).is_some()
                || function_owned_value_resource_tuple(manifest, function, &symbols.declarations)
                    .is_some()
        })
        .filter_map(|function| function.cpp_symbol.as_deref())
        .collect::<BTreeSet<_>>();
    for module in &manifest.modules {
        for function in &module.functions {
            if function_owned_value_resource_tuple(manifest, function, &symbols.declarations)
                .is_some()
            {
                source.push_str(&format!(
                    "        type {};\n",
                    owned_tuple_carrier_name(module, function)
                ));
            }
        }
    }
    let mutation_symbols = manifest
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .filter(|function| function_uses_mutation_adapter(manifest, function))
        .filter_map(|function| function.cpp_symbol.as_deref())
        .collect::<BTreeSet<_>>();
    for symbol in symbols.declarations.values().filter(|symbol| {
        symbols.is_bindable(&symbol.id)
            && !matches!(symbol.kind, CppSymbolKind::Record | CppSymbolKind::Enum)
            && !adapted_enum_symbols.contains(symbol.id.as_str())
            && !adapted_string_symbols.contains(symbol.id.as_str())
            && !contained_exception_symbols.contains(symbol.id.as_str())
            && !adapted_collection_symbols.contains(symbol.id.as_str())
            && !owned_value_symbols.contains(symbol.id.as_str())
            && !mutation_symbols.contains(symbol.id.as_str())
    }) {
        source.push_str(&render_bridge_function(
            symbol,
            &manifest.cpp_metadata.namespace,
        )?);
    }
    for module in &manifest.modules {
        for function in &module.functions {
            let Some((resource, callable)) =
                function_owned_value_resource(manifest, function, &symbols.declarations)
            else {
                continue;
            };
            let resource_symbol = symbols
                .declarations
                .get(resource.cpp_symbol.as_str())
                .ok_or_else(|| format!("unknown C++ type symbol `{}`", resource.cpp_symbol))?;
            let mut args = Vec::new();
            if callable.kind == CppSymbolKind::Method {
                let receiver = callable
                    .receiver
                    .as_deref()
                    .ok_or_else(|| format!("C++ method `{}` has no receiver", callable.id))?;
                args.push(format!("value: &{}", cpp_short_name(receiver)));
            }
            for mapping in public_cpp_parameter_mappings(function, callable) {
                let parameter = &callable.parameters[mapping.cpp_parameter_index];
                if let Some(choice) = mapping.scalar_choice {
                    args.push(format!("{}: bool", choice.tag_name));
                    args.push(format!("{}: i64", choice.integer_name));
                    args.push(format!("{}: f64", choice.floating_name));
                    continue;
                }
                if let Some(presence) = mapping.presence_name {
                    args.push(format!("{presence}: bool"));
                }
                // `self` has method semantics in a CXX bridge even when Clang
                // extracted it from an ordinary free function.
                let bridge_name =
                    if callable.kind != CppSymbolKind::Method && parameter.name == "self" {
                        "self_value"
                    } else {
                        parameter.name.as_str()
                    };
                args.push(format!(
                    "{}: {}",
                    bridge_name,
                    owned_value_bridge_parameter_type(manifest, parameter, mapping.public_type,)?
                ));
            }
            let args = args.join(", ");
            source.push_str(&format!(
                "        fn {}({args}) -> UniquePtr<{}>;\n",
                owned_value_adapter_name(module, function),
                resource_symbol.cpp_name
            ));
        }
    }
    for module in &manifest.modules {
        for function in &module.functions {
            let Some((resources, callable)) =
                function_owned_value_resource_tuple(manifest, function, &symbols.declarations)
            else {
                continue;
            };
            let mut args = Vec::new();
            if callable.kind == CppSymbolKind::Method {
                let receiver = callable
                    .receiver
                    .as_deref()
                    .ok_or_else(|| format!("C++ method `{}` has no receiver", callable.id))?;
                args.push(format!("value: &{}", cpp_short_name(receiver)));
            }
            for mapping in public_cpp_parameter_mappings(function, callable) {
                let parameter = &callable.parameters[mapping.cpp_parameter_index];
                if let Some(choice) = mapping.scalar_choice {
                    args.push(format!("{}: bool", choice.tag_name));
                    args.push(format!("{}: i64", choice.integer_name));
                    args.push(format!("{}: f64", choice.floating_name));
                    continue;
                }
                if let Some(presence) = mapping.presence_name {
                    args.push(format!("{presence}: bool"));
                }
                let bridge_name =
                    if callable.kind != CppSymbolKind::Method && parameter.name == "self" {
                        "self_value"
                    } else {
                        parameter.name.as_str()
                    };
                args.push(format!(
                    "{}: {}",
                    bridge_name,
                    owned_value_bridge_parameter_type(manifest, parameter, mapping.public_type)?
                ));
            }
            let carrier = owned_tuple_carrier_name(module, function);
            source.push_str(&format!(
                "        fn {}({}) -> UniquePtr<{carrier}>;\n",
                owned_tuple_adapter_name(module, function),
                args.join(", ")
            ));
            for (index, (_, _, resource_symbol)) in resources.iter().enumerate() {
                source.push_str(&format!(
                    "        fn {}(value: Pin<&mut {carrier}>) -> UniquePtr<{}>;\n",
                    owned_tuple_take_name(module, function, index),
                    resource_symbol.cpp_name
                ));
            }
        }
    }
    for module in &manifest.modules {
        for function in module
            .functions
            .iter()
            .filter(|function| function_uses_mutation_adapter(manifest, function))
        {
            let callable = function_symbol(function, &symbols.declarations)?;
            let mut args = Vec::new();
            if callable.kind == CppSymbolKind::Method {
                let resource = function
                    .args
                    .first()
                    .and_then(|arg| {
                        manifest
                            .modules
                            .iter()
                            .flat_map(|owner| &owner.types)
                            .find(|resource| {
                                resource.kind == NativeBindingTypeKind::OpaqueResource
                                    && terlan_type_matches(&arg.ty, &resource.name)
                            })
                    })
                    .ok_or_else(|| format!("mutable method `{}` has no resource", function.name))?;
                let receiver = symbols
                    .declarations
                    .get(resource.cpp_symbol.as_str())
                    .ok_or_else(|| {
                        format!("mutable method `{}` has unknown resource", function.name)
                    })?;
                args.push(format!("self_value: Pin<&mut {}>", receiver.cpp_name));
            }
            args.extend(
                public_cpp_parameter_mappings(function, callable)
                    .into_iter()
                    .map(|mapping| {
                        let index = mapping.cpp_parameter_index;
                        let parameter = &callable.parameters[index];
                        let bridge_name = if parameter.name == "self" {
                            "self_value"
                        } else {
                            parameter.name.as_str()
                        };
                        let mutable_resource = callable.kind != CppSymbolKind::Method
                            && (index == 0
                                || function.args[mapping.public_argument_index].mutable);
                        if mutable_resource {
                            let resource = manifest
                                .modules
                                .iter()
                                .flat_map(|owner| &owner.types)
                                .find(|resource| {
                                    resource.kind == NativeBindingTypeKind::OpaqueResource
                                        && mapping.public_type.is_some_and(|ty| {
                                            terlan_type_matches(ty, &resource.name)
                                        })
                                })
                                .ok_or_else(|| {
                                    format!(
                                        "mutable free function `{}` has no mutable resource for `{}`",
                                        function.name, bridge_name
                                    )
                                })?;
                            let resource_symbol = symbols
                                .declarations
                                .get(resource.cpp_symbol.as_str())
                                .ok_or_else(|| {
                                    format!(
                                        "mutable free function `{}` has unknown mutable resource for `{}`",
                                        function.name, bridge_name
                                    )
                                })?;
                            Ok(format!(
                                "{}: Pin<&mut {}>",
                                bridge_name, resource_symbol.cpp_name
                            ))
                        } else {
                            let mut values = Vec::new();
                            if let Some(choice) = mapping.scalar_choice {
                                values.push(format!("{}: bool", choice.tag_name));
                                values.push(format!("{}: i64", choice.integer_name));
                                values.push(format!("{}: f64", choice.floating_name));
                                return Ok(values.join(", "));
                            }
                            if let Some(presence) = mapping.presence_name {
                                values.push(format!("{presence}: bool"));
                            }
                            values.push(format!(
                                "{}: {}",
                                bridge_name,
                                owned_value_bridge_parameter_type(
                                    manifest,
                                    parameter,
                                    mapping.public_type,
                                )?
                            ));
                            Ok(values.join(", "))
                        }
                    })
                    .collect::<Result<Vec<_>, String>>()?,
            );
            let args = args.join(", ");
            source.push_str(&format!(
                "        fn {}({args}) -> UniquePtr<{EXCEPTION_ENVELOPE}>;\n",
                mutation_adapter_name(module, function)
            ));
        }
    }
    for module in &manifest.modules {
        for function in module
            .functions
            .iter()
            .filter(|function| function.role == NativeFunctionRole::StringProjection)
        {
            let resource = function
                .args
                .first()
                .and_then(|arg| {
                    module.types.iter().find(|ty| {
                        ty.kind == NativeBindingTypeKind::OpaqueResource
                            && terlan_type_matches(&arg.ty, &ty.name)
                    })
                })
                .ok_or_else(|| format!("string projection `{}` has no resource", function.name))?;
            let resource_symbol = symbols
                .declarations
                .get(resource.cpp_symbol.as_str())
                .ok_or_else(|| {
                    format!("string projection `{}` has unknown resource", function.name)
                })?;
            source.push_str(&format!(
                "        fn {}(value: &{}) -> UniquePtr<CxxString>;\n",
                string_adapter_name(module, function),
                resource_symbol.cpp_name
            ));
        }
    }
    for module in &manifest.modules {
        for function in module
            .functions
            .iter()
            .filter(|function| function.role == NativeFunctionRole::IntListProjection)
        {
            let resource = function
                .args
                .first()
                .and_then(|arg| {
                    module.types.iter().find(|ty| {
                        ty.kind == NativeBindingTypeKind::OpaqueResource
                            && terlan_type_matches(&arg.ty, &ty.name)
                    })
                })
                .ok_or_else(|| {
                    format!(
                        "integer-list projection `{}` has no resource",
                        function.name
                    )
                })?;
            let resource_symbol = symbols
                .declarations
                .get(resource.cpp_symbol.as_str())
                .ok_or_else(|| {
                    format!(
                        "integer-list projection `{}` has unknown resource",
                        function.name
                    )
                })?;
            source.push_str(&format!(
                "        fn {}(value: &{}) -> UniquePtr<CxxVector<i64>>;\n",
                collection_adapter_name(module, function),
                resource_symbol.cpp_name
            ));
        }
    }
    for module in &manifest.modules {
        for function in module
            .functions
            .iter()
            .filter(|function| function.role == NativeFunctionRole::ResourceListProjection)
        {
            let (receiver, element, callable) =
                resource_list_parts(manifest, module, function, &symbols.declarations)?;
            let receiver_symbol = receiver
                .map(|receiver| {
                    symbols
                        .declarations
                        .get(receiver.cpp_symbol.as_str())
                        .ok_or_else(|| {
                            format!(
                                "resource-list projection `{}` has unknown receiver",
                                function.name
                            )
                        })
                })
                .transpose()?;
            let element_symbol = symbols
                .declarations
                .get(element.cpp_symbol.as_str())
                .ok_or_else(|| {
                    format!(
                        "resource-list projection `{}` has unknown element resource",
                        function.name
                    )
                })?;
            let result = resource_list_result_name(module, function);
            let adapter = resource_list_adapter_name(module, function);
            let mut args = receiver_symbol
                .map(|receiver| vec![format!("value: &{}", receiver.cpp_name)])
                .unwrap_or_default();
            for mapping in public_cpp_parameter_mappings(function, callable) {
                let parameter = &callable.parameters[mapping.cpp_parameter_index];
                if let Some(choice) = mapping.scalar_choice {
                    args.push(format!("{}: bool", choice.tag_name));
                    args.push(format!("{}: i64", choice.integer_name));
                    args.push(format!("{}: f64", choice.floating_name));
                    continue;
                }
                if let Some(presence) = mapping.presence_name {
                    args.push(format!("{presence}: bool"));
                }
                // CXX reserves `self` for a method receiver, while ATen also
                // uses that spelling for ordinary free parameters.
                let bridge_name = if receiver_symbol.is_none() && parameter.name == "self" {
                    "self_value"
                } else {
                    parameter.name.as_str()
                };
                args.push(format!(
                    "{}: {}",
                    bridge_name,
                    owned_value_bridge_parameter_type(manifest, parameter, mapping.public_type,)?
                ));
            }
            source.push_str(&format!(
                "        type {result};\n        fn {adapter}({}) -> UniquePtr<{result}>;\n        fn {adapter}_is_ok(result: &{result}) -> bool;\n        fn {adapter}_len(result: &{result}) -> usize;\n        fn {adapter}_element(result: &{result}, index: usize) -> UniquePtr<{}>;\n        fn {adapter}_code(result: &{result}) -> &CxxString;\n        fn {adapter}_message(result: &{result}) -> &CxxString;\n",
                args.join(", "),
                element_symbol.cpp_name,
            ));
        }
    }
    if has_exception_adapters(manifest) {
        source.push_str(&format!(
            "        type {EXCEPTION_ENVELOPE};\n        fn is_ok(self: &{EXCEPTION_ENVELOPE}) -> bool;\n        fn value(self: &{EXCEPTION_ENVELOPE}) -> i64;\n        fn float_value(self: &{EXCEPTION_ENVELOPE}) -> f64;\n        fn bool_value(self: &{EXCEPTION_ENVELOPE}) -> bool;\n        fn code(self: &{EXCEPTION_ENVELOPE}) -> &CxxString;\n        fn message(self: &{EXCEPTION_ENVELOPE}) -> &CxxString;\n"
        ));
        for module in &manifest.modules {
            for function in module.functions.iter().filter(|function| {
                matches!(
                    function.role,
                    NativeFunctionRole::ExceptionMethod | NativeFunctionRole::ScalarProjection
                )
            }) {
                let callable = function_symbol(function, &symbols.declarations)?;
                let mut args = Vec::new();
                if callable.kind == CppSymbolKind::Method {
                    let resource = function
                        .args
                        .first()
                        .and_then(|arg| {
                            manifest
                                .modules
                                .iter()
                                .flat_map(|owner| &owner.types)
                                .find(|ty| {
                                    ty.kind == NativeBindingTypeKind::OpaqueResource
                                        && terlan_type_matches(&arg.ty, &ty.name)
                                })
                        })
                        .ok_or_else(|| {
                            format!("exception method `{}` has no resource", function.name)
                        })?;
                    let resource_symbol = symbols
                        .declarations
                        .get(resource.cpp_symbol.as_str())
                        .ok_or_else(|| {
                            format!("exception method `{}` has unknown resource", function.name)
                        })?;
                    args.push(format!("value: &{}", resource_symbol.cpp_name));
                }
                for mapping in public_cpp_parameter_mappings(function, callable) {
                    let parameter = &callable.parameters[mapping.cpp_parameter_index];
                    if let Some(choice) = mapping.scalar_choice {
                        args.push(format!("{}: bool", choice.tag_name));
                        args.push(format!("{}: i64", choice.integer_name));
                        args.push(format!("{}: f64", choice.floating_name));
                        continue;
                    }
                    if let Some(presence) = mapping.presence_name {
                        args.push(format!("{presence}: bool"));
                    }
                    let bridge_name =
                        if callable.kind != CppSymbolKind::Method && parameter.name == "self" {
                            "self_value"
                        } else {
                            parameter.name.as_str()
                        };
                    args.push(format!(
                        "{}: {}",
                        bridge_name,
                        owned_value_bridge_parameter_type(
                            manifest,
                            parameter,
                            mapping.public_type,
                        )?
                    ));
                }
                source.push_str(&format!(
                    "        fn {}({}) -> UniquePtr<{EXCEPTION_ENVELOPE}>;\n",
                    exception_adapter_name(module, function),
                    args.join(", "),
                ));
            }
        }
    }
    for module in &manifest.modules {
        for function in module
            .functions
            .iter()
            .filter(|function| function.role == NativeFunctionRole::EnumProjection)
        {
            let resource = function
                .args
                .first()
                .and_then(|arg| {
                    module.types.iter().find(|ty| {
                        ty.kind == NativeBindingTypeKind::OpaqueResource
                            && terlan_type_matches(&arg.ty, &ty.name)
                    })
                })
                .ok_or_else(|| format!("enum projection `{}` has no resource", function.name))?;
            let resource_symbol = symbols
                .declarations
                .get(resource.cpp_symbol.as_str())
                .ok_or_else(|| {
                    format!("enum projection `{}` has unknown resource", function.name)
                })?;
            source.push_str(&format!(
                "        fn {}(value: &{}) -> UniquePtr<CxxString>;\n",
                enum_adapter_name(module, function),
                resource_symbol.cpp_name
            ));
        }
    }
    source.push_str("    }\n}\n");
    Ok(source)
}

pub(super) fn render_bridge_function(
    symbol: &CppSymbol,
    default_namespace: &str,
) -> Result<String, CppBindingError> {
    let mut args = Vec::new();
    if symbol.kind == CppSymbolKind::Method {
        let receiver = symbol
            .receiver
            .as_deref()
            .ok_or_else(|| format!("C++ method `{}` has no receiver", symbol.id))?;
        if symbol.receiver_mutable {
            args.push(format!("self: Pin<&mut {}>", cpp_short_name(receiver)));
        } else {
            args.push(format!("self: &{}", cpp_short_name(receiver)));
        }
    }
    for parameter in &symbol.parameters {
        args.push(format!(
            "{}: {}",
            parameter.name,
            rust_bridge_type(&parameter.ty)?
        ));
    }
    let return_text = match &symbol.returns {
        Some(returns) if returns.canonical != "void" => {
            format!(" -> {}", rust_bridge_type(returns)?)
        }
        _ => String::new(),
    };
    let namespace_attribute = if symbol.kind == CppSymbolKind::Function {
        symbol
            .overload_set
            .rsplit_once("::")
            .map(|(namespace, _)| namespace)
            .filter(|namespace| *namespace != default_namespace)
            .map(|namespace| format!("        #[namespace = {namespace:?}]\n"))
            .unwrap_or_default()
    } else {
        String::new()
    };
    Ok(format!(
        "{namespace_attribute}        fn {}({}){};\n",
        symbol.cpp_name,
        args.join(", "),
        return_text
    ))
}

pub(super) fn rust_bridge_type(cpp_type: &CppTypeMetadata) -> Result<String, CppBindingError> {
    if is_i64_type(cpp_type) {
        return Ok("i64".into());
    }
    if is_rust_str_type(cpp_type) {
        return Ok("&str".into());
    }
    if is_u8_slice_type(cpp_type) {
        return Ok("&[u8]".into());
    }
    if is_i64_slice_type(cpp_type) {
        return Ok("&[i64]".into());
    }
    if is_f64_slice_type(cpp_type) {
        return Ok("&[f64]".into());
    }
    if is_owned_string_type(cpp_type) {
        return Ok("UniquePtr<CxxString>".into());
    }
    if is_owned_u8_vector_type(cpp_type) {
        return Ok("UniquePtr<CxxVector<u8>>".into());
    }
    if is_owned_i64_vector_type(cpp_type) {
        return Ok("UniquePtr<CxxVector<i64>>".into());
    }
    if is_owned_f64_vector_type(cpp_type) {
        return Ok("UniquePtr<CxxVector<f64>>".into());
    }
    if is_owned_string_vector_type(cpp_type) {
        return Ok("UniquePtr<CxxVector<CxxString>>".into());
    }
    if let Some(record) = borrowed_const_record_name(cpp_type) {
        return Ok(format!("&{}", cpp_short_name(record)));
    }
    match cpp_type.canonical.as_str() {
        "double" => return Ok("f64".into()),
        "bool" => return Ok("bool".into()),
        _ => {}
    }
    if let Some(inner) = cpp_type
        .canonical
        .strip_prefix("std::unique_ptr<")
        .and_then(|value| value.strip_suffix('>'))
    {
        let cpp_name = cpp_short_name(inner);
        return Ok(format!("UniquePtr<{cpp_name}>"));
    }
    Err((format!(
        "error[cpp.type.unmapped]: C++ type `{}` (canonical `{}`) has no cxx bridge mapping",
        cpp_type.spelling, cpp_type.canonical
    ))
    .into())
}
