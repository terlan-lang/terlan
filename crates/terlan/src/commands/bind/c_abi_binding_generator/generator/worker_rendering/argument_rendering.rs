use super::*;

fn render_primitive_binding(argument: &CAbiBindingArg) -> String {
    match argument.ty.as_str() {
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
