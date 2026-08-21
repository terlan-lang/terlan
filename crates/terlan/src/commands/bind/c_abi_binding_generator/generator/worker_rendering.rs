use super::*;

mod argument_rendering;
mod dispatch_rendering;
mod native_helper_template;
use argument_rendering::{
    render_argument_binding, render_immutable_resource_borrows,
    render_immutable_resource_list_borrows, render_mutable_resource_list_borrows,
};
use dispatch_rendering::{render_dispatch_calls, render_dispatch_modules};
use native_helper_template::NATIVE_HELPER_TEMPLATE;

const NATIVE_HELPER_FUNCTION_CHUNK_SIZE: usize = 16;

fn render_multi_helper_match_arm(
    manifest: &CAbiBindingManifest,
    function: &CAbiBindingFunction,
    symbol: &CSymbol,
) -> Result<String, String> {
    let is_resource = |name: &str| binding_type(manifest, name).is_some();
    let resource_list = |name: &str| list_binding_type(manifest, name);
    let patterns = function
        .args
        .iter()
        .map(|argument| match argument.abi_ty() {
            "Int" => Ok(format!("Arg::Int({})", argument.name)),
            "Float" => Ok(format!("Arg::Float({})", argument.name)),
            "Bool" => Ok(format!("Arg::Bool({})", argument.name)),
            "String" => Ok(format!("Arg::String({})", argument.name)),
            "Bytes" => Ok(format!("Arg::Bytes({})", argument.name)),
            "List[Int]" => Ok(format!(
                "{} @ (Arg::Ints(_) | Arg::EmptyList)",
                argument.name
            )),
            "List[Float]" => Ok(format!(
                "{} @ (Arg::Floats(_) | Arg::EmptyList)",
                argument.name
            )),
            "List[Bool]" => Ok(format!(
                "{} @ (Arg::Bools(_) | Arg::EmptyList)",
                argument.name
            )),
            value if resource_list(value).is_some() => Ok(format!(
                "{} @ (Arg::Handles(_) | Arg::EmptyList)",
                argument.name
            )),
            value if is_resource(value) => Ok(format!("Arg::Handle({})", argument.name)),
            value => Err(format!(
                "error[native_bindgen.unsupported_terlan_type]: helper argument `{value}`"
            )),
        })
        .collect::<Result<Vec<_>, String>>()?;
    let fallible = symbol.error_model == Some(CErrorModel::StatusCode);
    let mut arm = format!(
        "            {:?} => {{\n                let [{}] = request.args.as_slice() else {{\n                    return protocol_error(\"invalid_arguments\", {:?});\n                }};\n",
        function.operation,
        patterns.join(", "),
        format!("{} expects its declared arguments", function.name)
    );
    match function.role {
        CAbiFunctionRole::Constructor => {
            let (_, output_ty) = binding_type(manifest, &function.returns).ok_or_else(|| {
                format!(
                    "error[native_bindgen.unsupported_terlan_type]: constructor `{}` return `{}`",
                    function.name, function.returns
                )
            })?;
            let bindings = function
                .args
                .iter()
                .map(|argument| render_argument_binding(manifest, argument))
                .collect::<Vec<_>>()
                .join(", ");
            let borrows = render_immutable_resource_borrows(manifest, &function.args);
            if !fallible {
                return Err(format!(
                    "error[native_bindgen.unsupported_wrapper_shape]: constructor `{}` must report status",
                    function.name
                ));
            }
            let qualified = qualified_type_name(manifest, &output_ty.name)?;
            arm.push_str(&format!(
                "{borrows}                let value = match {}::{}({bindings}) {{\n                    Ok(value) => value,\n                    Err(error) => return native_error(&error),\n                }};\n                let (id, generation) = match self.store_handle(HandleValue::{}(value)) {{\n                    Ok(handle) => handle,\n                    Err(error) => return error,\n                }};\n                format!(\"ok_handle {{}} {{id}} {{generation}} {{}}\", STANDARD.encode(self.owner.as_bytes()), STANDARD.encode({qualified:?}))\n",
                output_ty.name, function.adapter_name(), output_ty.name
            ));
        }
        CAbiFunctionRole::ImmutableMethod | CAbiFunctionRole::MutableMethod => {
            let owner_ty = function_owner_type(manifest, function)?;
            let handle_arguments = function
                .args
                .iter()
                .filter(|argument| is_resource(&argument.ty))
                .collect::<Vec<_>>();
            if handle_arguments.is_empty() {
                return Err(format!(
                    "error[native_bindgen.unsupported_wrapper_shape]: method `{}` has no handle",
                    function.name
                ));
            }
            let has_mutable_resource_list = function.role == CAbiFunctionRole::MutableMethod
                && function
                    .args
                    .iter()
                    .any(|argument| resource_list(&argument.ty).is_some());
            let (mut borrows, restore) = if has_mutable_resource_list {
                render_mutable_resource_list_borrows(manifest, &function.args)?
            } else {
                (String::new(), String::new())
            };
            if !has_mutable_resource_list
                && function.role == CAbiFunctionRole::MutableMethod
                && handle_arguments.len() > 1
            {
                let receiver = handle_arguments[0];
                for argument in &handle_arguments {
                    let qualified = qualified_type_name(manifest, &argument.ty)?;
                    borrows.push_str(&format!(
                        "                if let Err(error) = self.validate({}, {qualified:?}) {{\n                    return error;\n                }}\n",
                        argument.name
                    ));
                }
                for (index, left) in handle_arguments.iter().enumerate() {
                    for right in &handle_arguments[index + 1..] {
                        borrows.push_str(&format!(
                            "                if {}.id == {}.id {{\n                    return protocol_error(\"aliased_mutable_handle\", \"mutable resource calls require distinct handle arguments\");\n                }}\n",
                            left.name, right.name
                        ));
                    }
                }
                let entries = handle_arguments
                    .iter()
                    .map(|argument| format!("entry_{}", argument.name))
                    .collect::<Vec<_>>()
                    .join(", ");
                let keys = handle_arguments
                    .iter()
                    .map(|argument| format!("&{}.id", argument.name))
                    .collect::<Vec<_>>()
                    .join(", ");
                borrows.push_str(&format!(
                    "                let [{entries}] = self.handles.get_disjoint_mut([{keys}]);\n"
                ));
                if binding_types(manifest).len() == 1 {
                    borrows.push_str(&format!(
                        "                let HandleValue::{}(value_{}) = &mut entry_{}.expect(\"validated disjoint handle\").value;\n",
                        owner_ty.name, receiver.name, receiver.name
                    ));
                } else {
                    let qualified = qualified_type_name(manifest, &owner_ty.name)?;
                    borrows.push_str(&format!(
                        "                let value_{} = match &mut entry_{}.expect(\"validated disjoint handle\").value {{\n                    HandleValue::{}(value) => value,\n                    _ => return protocol_error(\"handle_storage_mismatch\", {qualified:?}),\n                }};\n",
                        receiver.name, receiver.name, owner_ty.name
                    ));
                }
                for argument in &handle_arguments[1..] {
                    if binding_types(manifest).len() == 1 {
                        borrows.push_str(&format!(
                            "                let HandleValue::{}(value_{}) = &entry_{}.expect(\"validated disjoint handle\").value;\n",
                            argument.ty, argument.name, argument.name
                        ));
                    } else {
                        let qualified = qualified_type_name(manifest, &argument.ty)?;
                        borrows.push_str(&format!(
                            "                let value_{} = match &entry_{}.expect(\"validated disjoint handle\").value {{\n                    HandleValue::{}(value) => value,\n                    _ => return protocol_error(\"handle_storage_mismatch\", {qualified:?}),\n                }};\n",
                            argument.name, argument.name, argument.ty
                        ));
                    }
                }
            } else if !has_mutable_resource_list {
                for argument in &handle_arguments {
                    let accessor = if function.role == CAbiFunctionRole::MutableMethod
                        && argument.name == handle_arguments[0].name
                    {
                        format!("live_{}_mut", owner_ty.name.to_ascii_lowercase())
                    } else {
                        format!("live_{}", argument.ty.to_ascii_lowercase())
                    };
                    borrows.push_str(&format!(
                        "                let value_{} = match self.{accessor}({}) {{\n                    Ok(value) => value,\n                    Err(error) => return error,\n                }};\n",
                        argument.name, argument.name
                    ));
                }
            }
            if !has_mutable_resource_list {
                borrows.push_str(&render_immutable_resource_list_borrows(
                    manifest,
                    &function.args,
                ));
            }
            let method_arguments = function
                .args
                .iter()
                .skip(1)
                .map(|argument| render_argument_binding(manifest, argument))
                .collect::<Vec<_>>()
                .join(", ");
            let receiver = &handle_arguments[0].name;
            let direct_call = format!(
                "value_{receiver}.{}({method_arguments})",
                function.adapter_name()
            );
            let call = if has_mutable_resource_list {
                borrows.push_str(&format!(
                    "                let call_result = {direct_call};\n{restore}"
                ));
                "call_result".to_string()
            } else {
                direct_call
            };
            if let Some((_, output_ty)) = list_binding_type(manifest, &function.returns) {
                if !fallible {
                    return Err(format!(
                        "error[native_bindgen.unsupported_wrapper_shape]: handle-list-returning method `{}` must be fallible",
                        function.name
                    ));
                }
                let qualified = qualified_type_name(manifest, &output_ty.name)?;
                arm.push_str(&format!(
                    "{borrows}                let values = match {call} {{\n                    Ok(values) => values,\n                    Err(error) => return native_error(&error),\n                }};\n                let mut handles = Vec::with_capacity(values.len());\n                for value in values {{\n                    let (id, generation) = match self.store_handle(HandleValue::{}(value)) {{\n                        Ok(handle) => handle,\n                        Err(error) => return error,\n                    }};\n                    handles.push(format!(\"{{}}:{{id}}:{{generation}}:{{}}\", STANDARD.encode(self.owner.as_bytes()), STANDARD.encode({qualified:?})));\n                }}\n                if handles.is_empty() {{ \"ok_handles\".to_string() }} else {{ format!(\"ok_handles {{}}\", handles.join(\",\")) }}\n",
                    output_ty.name
                ));
            } else if let Some((_, output_ty)) = optional_binding_type(manifest, &function.returns)
            {
                if !fallible {
                    return Err(format!(
                        "error[native_bindgen.unsupported_wrapper_shape]: optional handle-returning method `{}` must be fallible",
                        function.name
                    ));
                }
                let qualified = qualified_type_name(manifest, &output_ty.name)?;
                arm.push_str(&format!(
                    "{borrows}                match {call} {{\n                    Ok(Some(value)) => {{\n                        let (id, generation) = match self.store_handle(HandleValue::{}(value)) {{\n                            Ok(handle) => handle,\n                            Err(error) => return error,\n                        }};\n                        format!(\"ok_some_handle {{}} {{id}} {{generation}} {{}}\", STANDARD.encode(self.owner.as_bytes()), STANDARD.encode({qualified:?}))\n                    }}\n                    Ok(None) => \"ok_none\".to_string(),\n                    Err(error) => native_error(&error),\n                }}\n",
                    output_ty.name
                ));
            } else if let Some((_, output_ty)) = binding_type(manifest, &function.returns) {
                if !fallible {
                    return Err(format!(
                        "error[native_bindgen.unsupported_wrapper_shape]: handle-returning method `{}` must be fallible",
                        function.name
                    ));
                }
                let qualified = qualified_type_name(manifest, &output_ty.name)?;
                arm.push_str(&format!(
                    "{borrows}                let value = match {call} {{\n                    Ok(value) => value,\n                    Err(error) => return native_error(&error),\n                }};\n                let (id, generation) = match self.store_handle(HandleValue::{}(value)) {{\n                    Ok(handle) => handle,\n                    Err(error) => return error,\n                }};\n                format!(\"ok_handle {{}} {{id}} {{generation}} {{}}\", STANDARD.encode(self.owner.as_bytes()), STANDARD.encode({qualified:?}))\n",
                    output_ty.name
                ));
            } else {
                let success = match (function.returns.as_str(), fallible) {
                    ("Int", true) => format!("match {call} {{ Ok(value) => format!(\"ok_int {{value}}\"), Err(error) => native_error(&error) }}"),
                    ("Float", true) => format!("match {call} {{ Ok(value) => format!(\"ok_float {{value}}\"), Err(error) => native_error(&error) }}"),
                    ("Bool", true) => format!("match {call} {{ Ok(value) => format!(\"ok_bool {{value}}\"), Err(error) => native_error(&error) }}"),
                    ("String", true) => format!("match {call} {{ Ok(value) => format!(\"ok_string {{}}\", STANDARD.encode(value.as_bytes())), Err(error) => native_error(&error) }}"),
                    ("Bytes", true) => format!("match {call} {{ Ok(value) if value.is_empty() => \"ok_bytes\".to_string(), Ok(value) => format!(\"ok_bytes {{}}\", STANDARD.encode(value)), Err(error) => native_error(&error) }}"),
                    ("Unit", true) => format!("match {call} {{ Ok(()) => \"ok_unit\".to_string(), Err(error) => native_error(&error) }}"),
                    ("List[Int]", true) => format!("match {call} {{ Ok(values) if values.is_empty() => \"ok_ints\".to_string(), Ok(values) => format!(\"ok_ints {{}}\", values.iter().map(i64::to_string).collect::<Vec<_>>().join(\",\")), Err(error) => native_error(&error) }}"),
                    ("List[Float]", true) => format!("match {call} {{ Ok(values) if values.is_empty() => \"ok_floats\".to_string(), Ok(values) => format!(\"ok_floats {{}}\", values.iter().map(f64::to_string).collect::<Vec<_>>().join(\",\")), Err(error) => native_error(&error) }}"),
                    ("List[Bool]", true) => format!("match {call} {{ Ok(values) if values.is_empty() => \"ok_bools\".to_string(), Ok(values) => format!(\"ok_bools {{}}\", values.iter().map(bool::to_string).collect::<Vec<_>>().join(\",\")), Err(error) => native_error(&error) }}"),
                    ("List[String]", true) => format!("match {call} {{ Ok(values) if values.is_empty() => \"ok_strings\".to_string(), Ok(values) => format!(\"ok_strings {{}}\", values.iter().map(|value| STANDARD.encode(value.as_bytes())).collect::<Vec<_>>().join(\",\")), Err(error) => native_error(&error) }}"),
                    ("Int", false) => format!("format!(\"ok_int {{}}\", {call})"),
                    ("Float", false) => format!("format!(\"ok_float {{}}\", {call})"),
                    ("Bool", false) => format!("format!(\"ok_bool {{}}\", {call})"),
                    ("String", false) => format!("format!(\"ok_string {{}}\", STANDARD.encode({call}.as_bytes()))"),
                    ("Bytes", false) => format!("{{ let value = {call}; if value.is_empty() {{ \"ok_bytes\".to_string() }} else {{ format!(\"ok_bytes {{}}\", STANDARD.encode(value)) }} }}"),
                    ("Unit", false) => format!("{{ {call}; \"ok_unit\".to_string() }}"),
                    (value, _) => return Err(format!("error[native_bindgen.unsupported_terlan_type]: helper return `{value}`")),
                };
                arm.push_str(&format!("{borrows}                {success}\n"));
            }
        }
        CAbiFunctionRole::FreeFunction => {
            let bindings = function
                .args
                .iter()
                .map(|argument| render_argument_binding(manifest, argument))
                .collect::<Vec<_>>()
                .join(", ");
            let borrows = render_immutable_resource_borrows(manifest, &function.args);
            let call = format!("{}({bindings})", function.adapter_name());
            if let Some((_, output_ty)) = binding_type(manifest, &function.returns) {
                if !fallible {
                    return Err(format!(
                        "error[native_bindgen.unsupported_wrapper_shape]: handle-returning free function `{}` must be fallible",
                        function.name
                    ));
                }
                let qualified = qualified_type_name(manifest, &output_ty.name)?;
                arm.push_str(&format!(
                    "{borrows}                let value = match {call} {{\n                    Ok(value) => value,\n                    Err(error) => return native_error(&error),\n                }};\n                let (id, generation) = match self.store_handle(HandleValue::{}(value)) {{\n                    Ok(handle) => handle,\n                    Err(error) => return error,\n                }};\n                format!(\"ok_handle {{}} {{id}} {{generation}} {{}}\", STANDARD.encode(self.owner.as_bytes()), STANDARD.encode({qualified:?}))\n",
                    output_ty.name
                ));
                arm.push_str("            }\n");
                return Ok(arm);
            }
            let success = match (function.returns.as_str(), fallible) {
                ("Int", false) => format!("format!(\"ok_int {{}}\", {call})"),
                ("Float", false) => format!("format!(\"ok_float {{}}\", {call})"),
                ("Bool", false) => format!("format!(\"ok_bool {{}}\", {call})"),
                ("String", false) => format!("format!(\"ok_string {{}}\", STANDARD.encode({call}.as_bytes()))"),
                ("Bytes", false) => format!("{{ let value = {call}; if value.is_empty() {{ \"ok_bytes\".to_string() }} else {{ format!(\"ok_bytes {{}}\", STANDARD.encode(value)) }} }}"),
                ("Int", true) => format!("match {call} {{ Ok(value) => format!(\"ok_int {{value}}\"), Err(error) => native_error(&error) }}"),
                ("Float", true) => format!("match {call} {{ Ok(value) => format!(\"ok_float {{value}}\"), Err(error) => native_error(&error) }}"),
                ("Bool", true) => format!("match {call} {{ Ok(value) => format!(\"ok_bool {{value}}\"), Err(error) => native_error(&error) }}"),
                ("String", true) => format!("match {call} {{ Ok(value) => format!(\"ok_string {{}}\", STANDARD.encode(value.as_bytes())), Err(error) => native_error(&error) }}"),
                ("Bytes", true) => format!("match {call} {{ Ok(value) if value.is_empty() => \"ok_bytes\".to_string(), Ok(value) => format!(\"ok_bytes {{}}\", STANDARD.encode(value)), Err(error) => native_error(&error) }}"),
                ("List[Int]", true) => format!("match {call} {{ Ok(values) if values.is_empty() => \"ok_ints\".to_string(), Ok(values) => format!(\"ok_ints {{}}\", values.iter().map(i64::to_string).collect::<Vec<_>>().join(\",\")), Err(error) => native_error(&error) }}"),
                ("List[Float]", true) => format!("match {call} {{ Ok(values) if values.is_empty() => \"ok_floats\".to_string(), Ok(values) => format!(\"ok_floats {{}}\", values.iter().map(f64::to_string).collect::<Vec<_>>().join(\",\")), Err(error) => native_error(&error) }}"),
                ("List[Bool]", true) => format!("match {call} {{ Ok(values) if values.is_empty() => \"ok_bools\".to_string(), Ok(values) => format!(\"ok_bools {{}}\", values.iter().map(bool::to_string).collect::<Vec<_>>().join(\",\")), Err(error) => native_error(&error) }}"),
                ("List[String]", true) => format!("match {call} {{ Ok(values) if values.is_empty() => \"ok_strings\".to_string(), Ok(values) => format!(\"ok_strings {{}}\", values.iter().map(|value| STANDARD.encode(value.as_bytes())).collect::<Vec<_>>().join(\",\")), Err(error) => native_error(&error) }}"),
                ("Unit", false) => format!("{{ {call}; \"ok_unit\".to_string() }}"),
                ("Unit", true) => format!("match {call} {{ Ok(()) => \"ok_unit\".to_string(), Err(error) => native_error(&error) }}"),
                (value, _) => return Err(format!("error[native_bindgen.unsupported_terlan_type]: helper return `{value}`")),
            };
            arm.push_str(&format!("{borrows}                {success}\n"));
        }
        CAbiFunctionRole::Dispose => {
            let handle = function.args.first().ok_or_else(|| {
                format!(
                    "error[native_bindgen.unsupported_wrapper_shape]: dispose `{}` has no handle",
                    function.name
                )
            })?;
            let qualified = qualified_type_name(manifest, &handle.ty)?;
            arm.push_str(&format!(
                "                if let Err(error) = self.validate({}, {qualified:?}) {{\n                    return error;\n                }}\n                self.release_handle({}.id);\n                \"ok_unit\".to_string()\n",
                handle.name, handle.name
            ));
        }
    }
    arm.push_str("            }\n");
    Ok(arm)
}

pub(super) struct RenderedNativeHelper {
    pub(super) root: String,
    pub(super) dispatch_chunks: Vec<String>,
}

pub(super) fn render_native_helper(
    manifest: &CAbiBindingManifest,
    symbols: &BTreeMap<&str, &CSymbol>,
) -> Result<RenderedNativeHelper, String> {
    let types = binding_types(manifest);
    let functions = manifest
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .collect::<Vec<_>>();
    let mut imports = manifest
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .filter(|function| function.role == CAbiFunctionRole::FreeFunction)
        .map(|function| function.adapter_name().to_string())
        .collect::<Vec<_>>();
    imports.push("CAbiError".to_string());
    imports.extend(types.iter().map(|(_, ty)| ty.name.clone()));
    imports.sort();
    imports.dedup();
    let mut dispatch_chunks = Vec::new();
    let mut inline_match_arms = String::new();
    if functions.len() <= 64 {
        for function in &functions {
            inline_match_arms.push_str(&render_multi_helper_match_arm(
                manifest,
                function,
                function_symbol(function, symbols)?,
            )?);
        }
    } else {
        for (chunk_index, chunk_functions) in functions
            .chunks(NATIVE_HELPER_FUNCTION_CHUNK_SIZE)
            .enumerate()
        {
            let accepted_operations = chunk_functions
                .iter()
                .map(|function| format!("{:?}", function.operation))
                .collect::<Vec<_>>()
                .join(" | ");
            let mut chunk = format!(
                "use super::*;\n\npub(super) fn accepts_chunk_{chunk_index}(operation: &str) -> bool {{\n    matches!(operation, {accepted_operations})\n}}\n\nimpl Worker {{\n"
            );
            chunk.push_str(&format!(
                "    pub(super) fn execute_chunk_{chunk_index}(&mut self, request: Request) -> String {{\n        match request.operation.as_str() {{\n"
            ));
            for function in chunk_functions {
                chunk.push_str(&render_multi_helper_match_arm(
                    manifest,
                    function,
                    function_symbol(function, symbols)?,
                )?);
            }
            chunk.push_str(
                "            _ => unreachable!(\"operation routed to the wrong generated dispatch chunk\"),\n        }\n    }\n}\n",
            );
            dispatch_chunks.push(chunk);
        }
    }
    let handle_variants = types
        .iter()
        .map(|(_, ty)| format!("    {}({}),", ty.name, ty.name))
        .collect::<Vec<_>>()
        .join("\n");
    let mut handle_type_arms = types
        .iter()
        .map(|(_, ty)| {
            Ok(format!(
                "            HandleValue::{}(_) => {:?},",
                ty.name,
                qualified_type_name(manifest, &ty.name)?
            ))
        })
        .collect::<Result<Vec<_>, String>>()?
        .join("\n");
    if types.is_empty() {
        handle_type_arms.push_str(
            "            _ => unreachable!(\"value-only helper cannot contain handles\"),",
        );
    }
    let mut handle_accessors = String::new();
    for (_, ty) in &types {
        let accessor = ty.name.to_ascii_lowercase();
        let qualified = qualified_type_name(manifest, &ty.name)?;
        let immutable_required = functions.iter().any(|function| {
            matches!(
                function.role,
                CAbiFunctionRole::ImmutableMethod | CAbiFunctionRole::MutableMethod
            ) && function.args.iter().enumerate().any(|(index, argument)| {
                argument.ty == ty.name
                    && !(function.role == CAbiFunctionRole::MutableMethod && index == 0)
            })
        });
        let mutable_required = functions.iter().any(|function| {
            function.role == CAbiFunctionRole::MutableMethod
                && function
                    .args
                    .first()
                    .is_some_and(|argument| argument.ty == ty.name)
        });
        if types.len() == 1 {
            if immutable_required {
                handle_accessors.push_str(&format!(
                    "    fn live_{accessor}(&self, handle: &HandleArg) -> Result<&{}, String> {{\n        self.validate(handle, {qualified:?})?;\n        let HandleValue::{}(value) = &self.handles.get(&handle.id).expect(\"validated handle\").value;\n        Ok(value)\n    }}\n\n",
                    ty.name, ty.name
                ));
            }
            if mutable_required {
                handle_accessors.push_str(&format!(
                    "    fn live_{accessor}_mut(&mut self, handle: &HandleArg) -> Result<&mut {}, String> {{\n        self.validate(handle, {qualified:?})?;\n        let HandleValue::{}(value) = &mut self.handles.get_mut(&handle.id).expect(\"validated handle\").value;\n        Ok(value)\n    }}\n\n",
                    ty.name, ty.name
                ));
            }
        } else {
            if immutable_required {
                handle_accessors.push_str(&format!(
                    "    fn live_{accessor}(&self, handle: &HandleArg) -> Result<&{}, String> {{\n        self.validate(handle, {qualified:?})?;\n        match &self.handles.get(&handle.id).expect(\"validated handle\").value {{\n            HandleValue::{}(value) => Ok(value),\n            _ => Err(protocol_error(\"handle_storage_mismatch\", {qualified:?})),\n        }}\n    }}\n\n",
                    ty.name, ty.name
                ));
            }
            if mutable_required {
                handle_accessors.push_str(&format!(
                    "    fn live_{accessor}_mut(&mut self, handle: &HandleArg) -> Result<&mut {}, String> {{\n        self.validate(handle, {qualified:?})?;\n        match &mut self.handles.get_mut(&handle.id).expect(\"validated handle\").value {{\n            HandleValue::{}(value) => Ok(value),\n            _ => Err(protocol_error(\"handle_storage_mismatch\", {qualified:?})),\n        }}\n    }}\n\n",
                    ty.name, ty.name
                ));
            }
        }
    }
    let template = NATIVE_HELPER_TEMPLATE;
    let crate_ident = manifest.package.crate_name.replace('-', "_");
    let resource_dead_code = if types.is_empty() {
        "#[allow(dead_code)]\n"
    } else {
        ""
    };
    let all_infallible = functions.iter().all(|function| {
        function_symbol(function, symbols)
            .is_ok_and(|symbol| symbol.error_model != Some(CErrorModel::StatusCode))
    });
    let fallible_dead_code = if all_infallible {
        "#[allow(dead_code)]\n"
    } else {
        ""
    };
    let dispatch_modules = render_dispatch_modules(dispatch_chunks.len());
    let dispatch_calls = render_dispatch_calls(dispatch_chunks.len(), &inline_match_arms);
    let root = template
        .replace("@CRATE@", &crate_ident)
        .replace("@IMPORTS@", &imports.join(", "))
        .replace("@DISPATCH_MODULES@", &dispatch_modules)
        .replace("@DISPATCH_CALLS@", &dispatch_calls)
        .replace("@HANDLE_VARIANTS@", &handle_variants)
        .replace("@HANDLE_TYPE_ARMS@", &handle_type_arms)
        .replace("@HANDLE_ACCESSORS@", &handle_accessors)
        .replace("@RESOURCE_DEAD_CODE@", resource_dead_code)
        .replace("@FALLIBLE_DEAD_CODE@", fallible_dead_code)
        .replace(
            "@MAX_FRAME_BYTES@",
            &crate::runtime::native_boundary::adapter_abi::PUBLIC_ADAPTER_MAX_FRAME_BYTES
                .to_string(),
        )
        .replace(
            "@MAX_TRANSFER_BYTES@",
            &crate::runtime::native_boundary::adapter_abi::PUBLIC_ADAPTER_MAX_TRANSFER_BYTES
                .to_string(),
        );
    Ok(RenderedNativeHelper {
        root,
        dispatch_chunks,
    })
}
