use super::*;

fn render_primitive_binding(argument: &CAbiBindingArg) -> String {
    match argument.abi_ty() {
        "List[Int]" => format!("arg_ints({})", argument.name),
        "List[Float]" => format!("arg_floats({})", argument.name),
        "List[Bool]" => format!("arg_bools({})", argument.name),
        "String" => format!("{}.as_str()", argument.name),
        "Bytes" => format!("{}.as_slice()", argument.name),
        _ => format!("*{}", argument.name),
    }
}

pub(super) fn render_argument_binding(
    manifest: &CAbiBindingManifest,
    argument: &CAbiBindingArg,
) -> String {
    if list_binding_type(manifest, &argument.ty).is_some() {
        format!("value_{}.as_slice()", argument.name)
    } else if binding_type(manifest, &argument.ty).is_some() {
        format!("value_{}", argument.name)
    } else {
        render_primitive_binding(argument)
    }
}

pub(super) fn render_immutable_resource_borrows(
    manifest: &CAbiBindingManifest,
    arguments: &[CAbiBindingArg],
) -> String {
    arguments
        .iter()
        .map(|argument| {
            if binding_type(manifest, &argument.ty).is_some() {
                let accessor = format!("live_{}", argument.ty.to_ascii_lowercase());
                format!(
                    "                let value_{} = match self.{accessor}({}) {{\n                    Ok(value) => value,\n                    Err(error) => return error,\n                }};\n",
                    argument.name, argument.name
                )
            } else if let Some((_, inner)) = list_binding_type(manifest, &argument.ty) {
                let accessor = format!("live_{}", inner.name.to_ascii_lowercase());
                format!(
                    "                let value_{} = {{\n                    let mut values = Vec::new();\n                    for handle in arg_handles({}) {{\n                        let value = match self.{accessor}(handle) {{\n                            Ok(value) => value,\n                            Err(error) => return error,\n                        }};\n                        values.push(value);\n                    }}\n                    values\n                }};\n",
                    argument.name, argument.name
                )
            } else {
                String::new()
            }
        })
        .collect()
}

pub(super) fn render_immutable_resource_list_borrows(
    manifest: &CAbiBindingManifest,
    arguments: &[CAbiBindingArg],
) -> String {
    arguments
        .iter()
        .filter_map(|argument| {
            let (_, inner) = list_binding_type(manifest, &argument.ty)?;
            let accessor = format!("live_{}", inner.name.to_ascii_lowercase());
            Some(format!(
                "                let value_{} = {{\n                    let mut values = Vec::new();\n                    for handle in arg_handles({}) {{\n                        let value = match self.{accessor}(handle) {{\n                            Ok(value) => value,\n                            Err(error) => return error,\n                        }};\n                        values.push(value);\n                    }}\n                    values\n                }};\n",
                argument.name, argument.name
            ))
        })
        .collect()
}

pub(super) fn render_mutable_resource_list_borrows(
    manifest: &CAbiBindingManifest,
    arguments: &[CAbiBindingArg],
) -> Result<(String, String), String> {
    let scalar_resources = arguments
        .iter()
        .filter(|argument| binding_type(manifest, &argument.ty).is_some())
        .collect::<Vec<_>>();
    let receiver = scalar_resources.first().ok_or_else(|| {
        "error[native_bindgen.unsupported_wrapper_shape]: mutable method has no receiver"
            .to_string()
    })?;
    let (_, receiver_ty) = binding_type(manifest, &receiver.ty).expect("matched receiver type");
    let mut rendered = String::new();
    for argument in &scalar_resources {
        let qualified = qualified_type_name(manifest, &argument.ty)?;
        rendered.push_str(&format!(
            "                if let Err(error) = self.validate({}, {qualified:?}) {{\n                    return error;\n                }}\n",
            argument.name
        ));
        if argument.name != receiver.name {
            rendered.push_str(&format!(
                "                if {}.id == {}.id {{\n                    return protocol_error(\"aliased_mutable_handle\", \"mutable resource calls require the receiver to be distinct from borrowed resources\");\n                }}\n",
                receiver.name, argument.name
            ));
        }
    }
    for argument in arguments {
        let Some((_, inner)) = list_binding_type(manifest, &argument.ty) else {
            continue;
        };
        let qualified = qualified_type_name(manifest, &inner.name)?;
        rendered.push_str(&format!(
            "                for handle in arg_handles({}) {{\n                    if let Err(error) = self.validate(handle, {qualified:?}) {{\n                        return error;\n                    }}\n                    if handle.id == {}.id {{\n                        return protocol_error(\"aliased_mutable_handle\", \"mutable resource calls cannot borrow the receiver through a resource list\");\n                    }}\n                }}\n",
            argument.name, receiver.name
        ));
    }
    rendered.push_str(&format!(
        "                let mut entry_{} = self.handles.remove(&{}.id).expect(\"validated mutable receiver\");\n",
        receiver.name, receiver.name
    ));
    if binding_types(manifest).len() == 1 {
        rendered.push_str(&format!(
            "                let HandleValue::{}(value_{}) = &mut entry_{}.value;\n",
            receiver_ty.name, receiver.name, receiver.name
        ));
    } else {
        rendered.push_str(&format!(
            "                let value_{} = match &mut entry_{}.value {{\n                    HandleValue::{}(value) => value,\n                    _ => unreachable!(\"validated mutable receiver type\"),\n                }};\n",
            receiver.name, receiver.name, receiver_ty.name
        ));
    }
    for argument in scalar_resources.iter().skip(1) {
        let (_, ty) = binding_type(manifest, &argument.ty).expect("matched scalar resource");
        if binding_types(manifest).len() == 1 {
            rendered.push_str(&format!(
                "                let HandleValue::{}(value_{}) = &self.handles.get(&{}.id).expect(\"validated borrowed resource\").value;\n",
                ty.name, argument.name, argument.name
            ));
        } else {
            rendered.push_str(&format!(
                "                let value_{} = match &self.handles.get(&{}.id).expect(\"validated borrowed resource\").value {{\n                    HandleValue::{}(value) => value,\n                    _ => unreachable!(\"validated borrowed resource type\"),\n                }};\n",
                argument.name, argument.name, ty.name
            ));
        }
    }
    for argument in arguments {
        let Some((_, inner)) = list_binding_type(manifest, &argument.ty) else {
            continue;
        };
        if binding_types(manifest).len() == 1 {
            rendered.push_str(&format!(
                "                let value_{} = arg_handles({}).iter().map(|handle| {{\n                    let HandleValue::{}(value) = &self.handles.get(&handle.id).expect(\"validated borrowed list resource\").value;\n                    value\n                }}).collect::<Vec<_>>();\n",
                argument.name, argument.name, inner.name
            ));
        } else {
            rendered.push_str(&format!(
                "                let value_{} = arg_handles({}).iter().map(|handle| match &self.handles.get(&handle.id).expect(\"validated borrowed list resource\").value {{\n                    HandleValue::{}(value) => value,\n                    _ => unreachable!(\"validated borrowed list resource type\"),\n                }}).collect::<Vec<_>>();\n",
                argument.name, argument.name, inner.name
            ));
        }
    }
    let restore = format!(
        "                let previous = self.handles.insert({}.id, entry_{});\n                debug_assert!(previous.is_none());\n",
        receiver.name, receiver.name
    );
    Ok((rendered, restore))
}
