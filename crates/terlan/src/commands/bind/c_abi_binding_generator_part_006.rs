
#[allow(dead_code)]
fn render_helper_match_arm(
    function: &CAbiBindingFunction,
    symbol: &CSymbol,
    ty: &CAbiBindingType,
) -> Result<String, String> {
    let patterns = function
        .args
        .iter()
        .map(|argument| match argument.ty.as_str() {
            "Int" => Ok(format!("Arg::Int({})", argument.name)),
            "Float" => Ok(format!("Arg::Float({})", argument.name)),
            "Bool" => Ok(format!("Arg::Bool({})", argument.name)),
            "String" => Ok(format!("Arg::String({})", argument.name)),
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
            value if value == ty.name => Ok(format!("Arg::Handle({})", argument.name)),
            value => Err(format!(
                "error[native_bindgen.unsupported_terlan_type]: helper argument `{value}`"
            )),
        })
        .collect::<Result<Vec<_>, String>>()?;
    let bindings = function
        .args
        .iter()
        .filter(|argument| {
            matches!(
                argument.ty.as_str(),
                "Int" | "Float" | "Bool" | "String" | "List[Int]" | "List[Float]" | "List[Bool]"
            )
        })
        .map(|argument| {
            if argument.ty == "List[Int]" {
                format!("arg_ints({})", argument.name)
            } else if argument.ty == "List[Float]" {
                format!("arg_floats({})", argument.name)
            } else if argument.ty == "List[Bool]" {
                format!("arg_bools({})", argument.name)
            } else if argument.ty == "String" {
                format!("{}.as_str()", argument.name)
            } else {
                format!("*{}", argument.name)
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    let fallible = symbol.error_model == Some(CErrorModel::StatusCode);
    let invalid = format!("{} expects its declared arguments", function.name);
    let mut arm = format!(
        "            {:?} => {{\n                let [{}] = request.args.as_slice() else {{\n                    return protocol_error(\"invalid_arguments\", {:?});\n                }};\n",
        function.operation,
        patterns.join(", "),
        invalid
    );
    match function.role {
        CAbiFunctionRole::Constructor => {
            let call = format!("{}::{}({bindings})", ty.name, function.name);
            if !fallible {
                return Err(format!(
                    "error[native_bindgen.unsupported_wrapper_shape]: constructor `{}` must report status",
                    function.name
                ));
            }
            arm.push_str(&format!(
                "                let value = match {call} {{\n                    Ok(value) => value,\n                    Err(error) => return native_error(&error),\n                }};\n                self.next_id += 1;\n                let id = self.next_id;\n                self.handles.insert(id, HandleEntry {{ generation: 1, value }});\n                format!(\"ok_handle {{id}} 1 {{}}\", STANDARD.encode(HANDLE_TYPE))\n"
            ));
        }
        CAbiFunctionRole::ImmutableMethod | CAbiFunctionRole::MutableMethod => {
            let handle = function
                .args
                .iter()
                .find(|argument| argument.ty == ty.name)
                .ok_or_else(|| {
                    format!(
                        "error[native_bindgen.unsupported_wrapper_shape]: method `{}` has no handle",
                        function.name
                    )
                })?;
            let accessor = if function.role == CAbiFunctionRole::MutableMethod {
                "live_mut"
            } else {
                "live"
            };
            let call = format!("entry.value.{}({bindings})", function.name);
            if function.returns == ty.name {
                if !fallible {
                    return Err(format!(
                        "error[native_bindgen.unsupported_wrapper_shape]: handle-returning method `{}` must be fallible",
                        function.name
                    ));
                }
                let handle_arguments = function
                    .args
                    .iter()
                    .filter(|argument| argument.ty == ty.name)
                    .collect::<Vec<_>>();
                let mut borrows = String::new();
                for argument in &handle_arguments {
                    borrows.push_str(&format!(
                        "                    let entry_{} = match self.live({}) {{\n                        Ok(entry) => entry,\n                        Err(error) => return error,\n                    }};\n",
                        argument.name, argument.name
                    ));
                }
                let method_arguments = function
                    .args
                    .iter()
                    .skip(1)
                    .map(|argument| {
                        if argument.ty == ty.name {
                            format!("&entry_{}.value", argument.name)
                        } else if argument.ty == "List[Int]" {
                            format!("arg_ints({})", argument.name)
                        } else if argument.ty == "List[Float]" {
                            format!("arg_floats({})", argument.name)
                        } else if argument.ty == "String" {
                            format!("{}.as_str()", argument.name)
                        } else {
                            format!("*{}", argument.name)
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                let receiver = &handle_arguments[0].name;
                let handle_call = format!(
                    "entry_{receiver}.value.{}({method_arguments})",
                    function.name
                );
                arm.push_str(&format!(
                    "                let value = {{\n{borrows}                    match {handle_call} {{\n                        Ok(value) => value,\n                        Err(error) => return native_error(&error),\n                    }}\n                }};\n                self.next_id += 1;\n                let id = self.next_id;\n                self.handles.insert(id, HandleEntry {{ generation: 1, value }});\n                format!(\"ok_handle {{id}} 1 {{}}\", STANDARD.encode(HANDLE_TYPE))\n"
                ));
                arm.push_str("            }\n");
                return Ok(arm);
            }
            let success = match (function.returns.as_str(), fallible) {
                ("Int", true) => format!(
                    "match {call} {{\n                        Ok(value) => format!(\"ok_int {{value}}\"),\n                        Err(error) => native_error(&error),\n                    }}"
                ),
                ("Float", true) => format!(
                    "match {call} {{\n                        Ok(value) => format!(\"ok_float {{value}}\"),\n                        Err(error) => native_error(&error),\n                    }}"
                ),
                ("Bool", true) => format!(
                    "match {call} {{\n                        Ok(value) => format!(\"ok_bool {{value}}\"),\n                        Err(error) => native_error(&error),\n                    }}"
                ),
                ("String", true) => format!(
                    "match {call} {{\n                        Ok(value) => format!(\"ok_string {{}}\", STANDARD.encode(value.as_bytes())),\n                        Err(error) => native_error(&error),\n                    }}"
                ),
                ("Unit", true) => format!(
                    "match {call} {{\n                        Ok(()) => \"ok_unit\".to_string(),\n                        Err(error) => native_error(&error),\n                    }}"
                ),
                ("List[Int]", true) => format!(
                    "match {call} {{\n                        Ok(values) if values.is_empty() => \"ok_ints\".to_string(),\n                        Ok(values) => format!(\"ok_ints {{}}\", values.iter().map(i64::to_string).collect::<Vec<_>>().join(\",\")),\n                        Err(error) => native_error(&error),\n                    }}"
                ),
                ("List[Float]", true) => format!(
                    "match {call} {{\n                        Ok(values) if values.is_empty() => \"ok_floats\".to_string(),\n                        Ok(values) => format!(\"ok_floats {{}}\", values.iter().map(f64::to_string).collect::<Vec<_>>().join(\",\")),\n                        Err(error) => native_error(&error),\n                    }}"
                ),
                ("List[Bool]", true) => format!(
                    "match {call} {{\n                        Ok(values) if values.is_empty() => \"ok_bools\".to_string(),\n                        Ok(values) => format!(\"ok_bools {{}}\", values.iter().map(bool::to_string).collect::<Vec<_>>().join(\",\")),\n                        Err(error) => native_error(&error),\n                    }}"
                ),
                ("List[String]", true) => format!(
                    "match {call} {{\n                        Ok(values) if values.is_empty() => \"ok_strings\".to_string(),\n                        Ok(values) => format!(\"ok_strings {{}}\", values.iter().map(|value| STANDARD.encode(value.as_bytes())).collect::<Vec<_>>().join(\",\")),\n                        Err(error) => native_error(&error),\n                    }}"
                ),
                ("Int", false) => format!("format!(\"ok_int {{}}\", {call})"),
                ("Float", false) => format!("format!(\"ok_float {{}}\", {call})"),
                ("Bool", false) => format!("format!(\"ok_bool {{}}\", {call})"),
                ("String", false) => format!(
                    "format!(\"ok_string {{}}\", STANDARD.encode({call}.as_bytes()))"
                ),
                ("Unit", false) => format!("{{ {call}; \"ok_unit\".to_string() }}"),
                (value, _) => {
                    return Err(format!(
                        "error[native_bindgen.unsupported_terlan_type]: helper return `{value}`"
                    ));
                }
            };
            arm.push_str(&format!(
                "                match self.{accessor}({}) {{\n                    Ok(entry) => {success},\n                    Err(error) => error,\n                }}\n",
                handle.name
            ));
        }
        CAbiFunctionRole::FreeFunction => {
            let call = format!("{}({bindings})", function.name);
            let success = match (function.returns.as_str(), fallible) {
                ("Int", false) => format!("format!(\"ok_int {{}}\", {call})"),
                ("Float", false) => format!("format!(\"ok_float {{}}\", {call})"),
                ("Bool", false) => format!("format!(\"ok_bool {{}}\", {call})"),
                ("String", false) => format!("format!(\"ok_string {{}}\", STANDARD.encode({call}.as_bytes()))"),
                ("Int", true) => format!(
                    "match {call} {{ Ok(value) => format!(\"ok_int {{value}}\"), Err(error) => native_error(&error) }}"
                ),
                ("Float", true) => format!(
                    "match {call} {{ Ok(value) => format!(\"ok_float {{value}}\"), Err(error) => native_error(&error) }}"
                ),
                ("Bool", true) => format!(
                    "match {call} {{ Ok(value) => format!(\"ok_bool {{value}}\"), Err(error) => native_error(&error) }}"
                ),
                ("String", true) => format!("match {call} {{ Ok(value) => format!(\"ok_string {{}}\", STANDARD.encode(value.as_bytes())), Err(error) => native_error(&error) }}"),
                ("Unit", false) => format!("{{ {call}; \"ok_unit\".to_string() }}"),
                ("Unit", true) => format!(
                    "match {call} {{ Ok(()) => \"ok_unit\".to_string(), Err(error) => native_error(&error) }}"
                ),
                ("List[Int]", true) => format!(
                    "match {call} {{ Ok(values) if values.is_empty() => \"ok_ints\".to_string(), Ok(values) => format!(\"ok_ints {{}}\", values.iter().map(i64::to_string).collect::<Vec<_>>().join(\",\")), Err(error) => native_error(&error) }}"
                ),
                ("List[Float]", true) => format!(
                    "match {call} {{ Ok(values) if values.is_empty() => \"ok_floats\".to_string(), Ok(values) => format!(\"ok_floats {{}}\", values.iter().map(f64::to_string).collect::<Vec<_>>().join(\",\")), Err(error) => native_error(&error) }}"
                ),
                ("List[Bool]", true) => format!(
                    "match {call} {{ Ok(values) if values.is_empty() => \"ok_bools\".to_string(), Ok(values) => format!(\"ok_bools {{}}\", values.iter().map(bool::to_string).collect::<Vec<_>>().join(\",\")), Err(error) => native_error(&error) }}"
                ),
                ("List[String]", true) => format!(
                    "match {call} {{ Ok(values) if values.is_empty() => \"ok_strings\".to_string(), Ok(values) => format!(\"ok_strings {{}}\", values.iter().map(|value| STANDARD.encode(value.as_bytes())).collect::<Vec<_>>().join(\",\")), Err(error) => native_error(&error) }}"
                ),
                (value, _) => {
                    return Err(format!(
                        "error[native_bindgen.unsupported_terlan_type]: helper return `{value}`"
                    ));
                }
            };
            arm.push_str(&format!("                {success}\n"));
        }
        CAbiFunctionRole::Dispose => {
            let handle = function.args.first().ok_or_else(|| {
                format!(
                    "error[native_bindgen.unsupported_wrapper_shape]: dispose `{}` has no handle",
                    function.name
                )
            })?;
            arm.push_str(&format!(
                "                if let Err(error) = self.validate({}) {{\n                    return error;\n                }}\n                self.handles.remove(&{}.id);\n                \"ok_unit\".to_string()\n",
                handle.name, handle.name
            ));
        }
    }
    arm.push_str("            }\n");
    Ok(arm)
}

fn render_multi_helper_match_arm(
    manifest: &CAbiBindingManifest,
    function: &CAbiBindingFunction,
    symbol: &CSymbol,
) -> Result<String, String> {
    let is_resource = |name: &str| binding_type(manifest, name).is_some();
    let patterns = function
        .args
        .iter()
        .map(|argument| match argument.ty.as_str() {
            "Int" => Ok(format!("Arg::Int({})", argument.name)),
            "Float" => Ok(format!("Arg::Float({})", argument.name)),
            "Bool" => Ok(format!("Arg::Bool({})", argument.name)),
            "String" => Ok(format!("Arg::String({})", argument.name)),
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
            value if is_resource(value) => Ok(format!("Arg::Handle({})", argument.name)),
            value => Err(format!(
                "error[native_bindgen.unsupported_terlan_type]: helper argument `{value}`"
            )),
        })
        .collect::<Result<Vec<_>, String>>()?;
    let primitive_binding = |argument: &CAbiBindingArg| {
        if argument.ty == "List[Int]" {
            format!("arg_ints({})", argument.name)
        } else if argument.ty == "List[Float]" {
            format!("arg_floats({})", argument.name)
        } else if argument.ty == "List[Bool]" {
            format!("arg_bools({})", argument.name)
        } else if argument.ty == "String" {
            format!("{}.as_str()", argument.name)
        } else {
            format!("*{}", argument.name)
        }
    };
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
                .map(primitive_binding)
                .collect::<Vec<_>>()
                .join(", ");
            if !fallible {
                return Err(format!(
                    "error[native_bindgen.unsupported_wrapper_shape]: constructor `{}` must report status",
                    function.name
                ));
            }
            let qualified = qualified_type_name(manifest, &output_ty.name)?;
            arm.push_str(&format!(
                "                let value = match {}::{}({bindings}) {{\n                    Ok(value) => value,\n                    Err(error) => return native_error(&error),\n                }};\n                let (id, generation) = match self.store_handle(HandleValue::{}(value)) {{\n                    Ok(handle) => handle,\n                    Err(error) => return error,\n                }};\n                format!(\"ok_handle {{id}} {{generation}} {{}}\", STANDARD.encode({qualified:?}))\n",
                output_ty.name, function.name, output_ty.name
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
            let mut borrows = String::new();
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
            let method_arguments = function
                .args
                .iter()
                .skip(1)
                .map(|argument| {
                    if is_resource(&argument.ty) {
                        format!("value_{}", argument.name)
                    } else {
                        primitive_binding(argument)
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            let receiver = &handle_arguments[0].name;
            let call = format!("value_{receiver}.{}({method_arguments})", function.name);
            if let Some((_, output_ty)) = binding_type(manifest, &function.returns) {
                if !fallible {
                    return Err(format!(
                        "error[native_bindgen.unsupported_wrapper_shape]: handle-returning method `{}` must be fallible",
                        function.name
                    ));
                }
                let qualified = qualified_type_name(manifest, &output_ty.name)?;
                arm.push_str(&format!(
                    "{borrows}                let value = match {call} {{\n                    Ok(value) => value,\n                    Err(error) => return native_error(&error),\n                }};\n                let (id, generation) = match self.store_handle(HandleValue::{}(value)) {{\n                    Ok(handle) => handle,\n                    Err(error) => return error,\n                }};\n                format!(\"ok_handle {{id}} {{generation}} {{}}\", STANDARD.encode({qualified:?}))\n",
                    output_ty.name
                ));
            } else {
                let success = match (function.returns.as_str(), fallible) {
                    ("Int", true) => format!("match {call} {{ Ok(value) => format!(\"ok_int {{value}}\"), Err(error) => native_error(&error) }}"),
                    ("Float", true) => format!("match {call} {{ Ok(value) => format!(\"ok_float {{value}}\"), Err(error) => native_error(&error) }}"),
                    ("Bool", true) => format!("match {call} {{ Ok(value) => format!(\"ok_bool {{value}}\"), Err(error) => native_error(&error) }}"),
                    ("String", true) => format!("match {call} {{ Ok(value) => format!(\"ok_string {{}}\", STANDARD.encode(value.as_bytes())), Err(error) => native_error(&error) }}"),
                    ("Unit", true) => format!("match {call} {{ Ok(()) => \"ok_unit\".to_string(), Err(error) => native_error(&error) }}"),
                    ("List[Int]", true) => format!("match {call} {{ Ok(values) if values.is_empty() => \"ok_ints\".to_string(), Ok(values) => format!(\"ok_ints {{}}\", values.iter().map(i64::to_string).collect::<Vec<_>>().join(\",\")), Err(error) => native_error(&error) }}"),
                    ("List[Float]", true) => format!("match {call} {{ Ok(values) if values.is_empty() => \"ok_floats\".to_string(), Ok(values) => format!(\"ok_floats {{}}\", values.iter().map(f64::to_string).collect::<Vec<_>>().join(\",\")), Err(error) => native_error(&error) }}"),
                    ("List[Bool]", true) => format!("match {call} {{ Ok(values) if values.is_empty() => \"ok_bools\".to_string(), Ok(values) => format!(\"ok_bools {{}}\", values.iter().map(bool::to_string).collect::<Vec<_>>().join(\",\")), Err(error) => native_error(&error) }}"),
                    ("List[String]", true) => format!("match {call} {{ Ok(values) if values.is_empty() => \"ok_strings\".to_string(), Ok(values) => format!(\"ok_strings {{}}\", values.iter().map(|value| STANDARD.encode(value.as_bytes())).collect::<Vec<_>>().join(\",\")), Err(error) => native_error(&error) }}"),
                    ("Int", false) => format!("format!(\"ok_int {{}}\", {call})"),
                    ("Float", false) => format!("format!(\"ok_float {{}}\", {call})"),
                    ("Bool", false) => format!("format!(\"ok_bool {{}}\", {call})"),
                    ("String", false) => format!("format!(\"ok_string {{}}\", STANDARD.encode({call}.as_bytes()))"),
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
                .map(primitive_binding)
                .collect::<Vec<_>>()
                .join(", ");
            let call = format!("{}({bindings})", function.name);
            let success = match (function.returns.as_str(), fallible) {
                ("Int", false) => format!("format!(\"ok_int {{}}\", {call})"),
                ("Float", false) => format!("format!(\"ok_float {{}}\", {call})"),
                ("Bool", false) => format!("format!(\"ok_bool {{}}\", {call})"),
                ("String", false) => format!("format!(\"ok_string {{}}\", STANDARD.encode({call}.as_bytes()))"),
                ("Int", true) => format!("match {call} {{ Ok(value) => format!(\"ok_int {{value}}\"), Err(error) => native_error(&error) }}"),
                ("Float", true) => format!("match {call} {{ Ok(value) => format!(\"ok_float {{value}}\"), Err(error) => native_error(&error) }}"),
                ("Bool", true) => format!("match {call} {{ Ok(value) => format!(\"ok_bool {{value}}\"), Err(error) => native_error(&error) }}"),
                ("String", true) => format!("match {call} {{ Ok(value) => format!(\"ok_string {{}}\", STANDARD.encode(value.as_bytes())), Err(error) => native_error(&error) }}"),
                ("List[Int]", true) => format!("match {call} {{ Ok(values) if values.is_empty() => \"ok_ints\".to_string(), Ok(values) => format!(\"ok_ints {{}}\", values.iter().map(i64::to_string).collect::<Vec<_>>().join(\",\")), Err(error) => native_error(&error) }}"),
                ("List[Float]", true) => format!("match {call} {{ Ok(values) if values.is_empty() => \"ok_floats\".to_string(), Ok(values) => format!(\"ok_floats {{}}\", values.iter().map(f64::to_string).collect::<Vec<_>>().join(\",\")), Err(error) => native_error(&error) }}"),
                ("List[Bool]", true) => format!("match {call} {{ Ok(values) if values.is_empty() => \"ok_bools\".to_string(), Ok(values) => format!(\"ok_bools {{}}\", values.iter().map(bool::to_string).collect::<Vec<_>>().join(\",\")), Err(error) => native_error(&error) }}"),
                ("List[String]", true) => format!("match {call} {{ Ok(values) if values.is_empty() => \"ok_strings\".to_string(), Ok(values) => format!(\"ok_strings {{}}\", values.iter().map(|value| STANDARD.encode(value.as_bytes())).collect::<Vec<_>>().join(\",\")), Err(error) => native_error(&error) }}"),
                ("Unit", false) => format!("{{ {call}; \"ok_unit\".to_string() }}"),
                ("Unit", true) => format!("match {call} {{ Ok(()) => \"ok_unit\".to_string(), Err(error) => native_error(&error) }}"),
                (value, _) => return Err(format!("error[native_bindgen.unsupported_terlan_type]: helper return `{value}`")),
            };
            arm.push_str(&format!("                {success}\n"));
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

fn render_native_helper(
    manifest: &CAbiBindingManifest,
    symbols: &BTreeMap<&str, &CSymbol>,
) -> Result<String, String> {
    let types = binding_types(manifest);
    let mut imports = manifest
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .filter(|function| function.role == CAbiFunctionRole::FreeFunction)
        .map(|function| function.name.clone())
        .collect::<Vec<_>>();
    imports.push("CAbiError".to_string());
    imports.extend(types.iter().map(|(_, ty)| ty.name.clone()));
    imports.sort();
    imports.dedup();
    let mut match_arms = String::new();
    for function in manifest.modules.iter().flat_map(|module| &module.functions) {
        match_arms.push_str(&render_multi_helper_match_arm(
            manifest,
            function,
            function_symbol(function, symbols)?,
        )?);
    }
    let handle_variants = types
        .iter()
        .map(|(_, ty)| format!("    {}({}),", ty.name, ty.name))
        .collect::<Vec<_>>()
        .join("\n");
    let handle_type_arms = types
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
    let mut handle_accessors = String::new();
    for (_, ty) in &types {
        let accessor = ty.name.to_ascii_lowercase();
        let qualified = qualified_type_name(manifest, &ty.name)?;
        if types.len() == 1 {
            handle_accessors.push_str(&format!(
                "    fn live_{accessor}(&self, handle: &HandleArg) -> Result<&{}, String> {{\n        self.validate(handle, {qualified:?})?;\n        let HandleValue::{}(value) = &self.handles.get(&handle.id).expect(\"validated handle\").value;\n        Ok(value)\n    }}\n\n    #[allow(dead_code)]\n    fn live_{accessor}_mut(&mut self, handle: &HandleArg) -> Result<&mut {}, String> {{\n        self.validate(handle, {qualified:?})?;\n        let HandleValue::{}(value) = &mut self.handles.get_mut(&handle.id).expect(\"validated handle\").value;\n        Ok(value)\n    }}\n\n",
                ty.name, ty.name, ty.name, ty.name
            ));
        } else {
            handle_accessors.push_str(&format!(
                "    fn live_{accessor}(&self, handle: &HandleArg) -> Result<&{}, String> {{\n        self.validate(handle, {qualified:?})?;\n        match &self.handles.get(&handle.id).expect(\"validated handle\").value {{\n            HandleValue::{}(value) => Ok(value),\n            _ => Err(protocol_error(\"handle_storage_mismatch\", {qualified:?})),\n        }}\n    }}\n\n    #[allow(dead_code)]\n    fn live_{accessor}_mut(&mut self, handle: &HandleArg) -> Result<&mut {}, String> {{\n        self.validate(handle, {qualified:?})?;\n        match &mut self.handles.get_mut(&handle.id).expect(\"validated handle\").value {{\n            HandleValue::{}(value) => Ok(value),\n            _ => Err(protocol_error(\"handle_storage_mismatch\", {qualified:?})),\n        }}\n    }}\n\n",
                ty.name, ty.name, ty.name, ty.name
            ));
        }
    }
    let template = r##"#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::io::{self, BufRead, Read, Write};

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use @CRATE@::{@IMPORTS@};

const MAX_ADAPTER_FRAME_BYTES: usize = @MAX_FRAME_BYTES@;

fn main() {
    let mut worker = Worker::default();
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let mut stdout = io::stdout().lock();
    loop {
        let mut frame = Vec::new();
        let read = Read::by_ref(&mut input)
            .take((MAX_ADAPTER_FRAME_BYTES + 1) as u64)
            .read_until(b'\n', &mut frame);
        let (payload, terminate) = match read {
            Ok(0) => break,
            Ok(_) if frame.len() > MAX_ADAPTER_FRAME_BYTES => (
                protocol_error("frame_too_large", "native adapter frame exceeds its bound"),
                true,
            ),
            Ok(_) => match String::from_utf8(frame) {
                Ok(line) => (
                    worker.execute_line(line.trim_end_matches(['\r', '\n'])),
                    false,
                ),
                Err(error) => (protocol_error("invalid_utf8", &error.to_string()), false),
            },
            Err(error) => (protocol_error("native_read_error", &error.to_string()), true),
        };
        if writeln!(stdout, "{payload}").is_err() || stdout.flush().is_err() || terminate {
            break;
        }
    }
}

#[derive(Default)]
struct Worker {
    last_request_id: Option<u64>,
    next_id: u64,
    free_ids: Vec<u64>,
    generations: HashMap<u64, u64>,
    handles: HashMap<u64, HandleEntry>,
}

struct HandleEntry {
    generation: u64,
    value: HandleValue,
}

enum HandleValue {
@HANDLE_VARIANTS@
}

impl HandleValue {
    fn type_name(&self) -> &'static str {
        match self {
@HANDLE_TYPE_ARMS@
        }
    }
}

#[derive(Clone)]
struct HandleArg {
    id: u64,
    generation: u64,
    type_name: String,
}

enum Arg {
    Int(i64),
    Float(f64),
    Bool(bool),
    String(String),
    Ints(Vec<i64>),
    Floats(Vec<f64>),
    Bools(Vec<bool>),
    EmptyList,
    Handle(HandleArg),
}

struct Request {
    operation: String,
    args: Vec<Arg>,
}

impl Worker {
    fn store_handle(&mut self, value: HandleValue) -> Result<(u64, u64), String> {
        while let Some(id) = self.free_ids.pop() {
            let previous = self.generations.get(&id).copied().unwrap_or_default();
            if let Some(generation) = previous.checked_add(1) {
                self.generations.insert(id, generation);
                self.handles.insert(id, HandleEntry { generation, value });
                return Ok((id, generation));
            }
        }
        let Some(id) = self.next_id.checked_add(1) else {
            return Err(protocol_error(
                "resource_table_exhausted",
                "NativeBoundary handle table exhausted",
            ));
        };
        self.next_id = id;
        let generation = 1;
        self.generations.insert(id, generation);
        self.handles.insert(id, HandleEntry { generation, value });
        Ok((id, generation))
    }

    fn release_handle(&mut self, id: u64) {
        if self.handles.remove(&id).is_some() {
            self.free_ids.push(id);
        }
    }

    fn execute_line(&mut self, line: &str) -> String {
        let Some(request_id) = request_id(line) else {
            return match parse_request(line) {
                Ok(_) => protocol_error("invalid_request", "request id is missing"),
                Err(error) => error,
            };
        };
        if self
            .last_request_id
            .is_some_and(|last_request_id| request_id <= last_request_id)
        {
            return format!(
                "reply {request_id} 1 {}",
                protocol_error(
                    "request_not_monotonic",
                    "native adapter request id was already completed"
                )
            );
        }
        self.last_request_id = Some(request_id);
        let request = match parse_request(line) {
            Ok(request) => request,
            Err(error) => return format!("reply {request_id} 1 {error}"),
        };
        let payload = self.execute(request);
        format!("reply {request_id} 1 {payload}")
    }

    fn execute(&mut self, request: Request) -> String {
        match request.operation.as_str() {
@MATCH_ARMS@
            _ => protocol_error("unknown_operation", &request.operation),
        }
    }

    fn validate(&self, handle: &HandleArg, expected_type: &str) -> Result<(), String> {
        if handle.type_name != expected_type {
            return Err(protocol_error("handle_type_mismatch", &handle.type_name));
        }
        match self.handles.get(&handle.id) {
            Some(entry)
                if entry.generation == handle.generation
                    && entry.value.type_name() == expected_type => Ok(()),
            Some(_) => Err(protocol_error(
                "handle_storage_mismatch",
                "NativeBoundary handle resource type does not match",
            )),
            _ => Err(protocol_error("stale_handle", "NativeBoundary handle is stale")),
        }
    }

@HANDLE_ACCESSORS@
}

fn request_id(line: &str) -> Option<u64> {
    let mut fields = line.split_whitespace();
    (fields.next() == Some("call"))
        .then(|| fields.next()?.parse::<u64>().ok())
        .flatten()
}

fn parse_request(line: &str) -> Result<Request, String> {
    let mut fields = line.split_whitespace();
    if fields.next() != Some("call") {
        return Err(protocol_error("invalid_request", "expected call request"));
    }
    let _request_id = fields
        .next()
        .ok_or_else(|| protocol_error("invalid_request", "missing request id"))?
        .parse::<u64>()
        .map_err(|error| protocol_error("invalid_request", &error.to_string()))?;
    let operation = decode_text(fields.next().ok_or_else(|| {
        protocol_error("invalid_request", "missing encoded operation")
    })?)?;
    let args = fields.map(parse_arg).collect::<Result<Vec<_>, _>>()?;
    for argument in &args {
        validate_decoded_arg_shape(argument);
    }
    Ok(Request { operation, args })
}

fn parse_arg(value: &str) -> Result<Arg, String> {
    // The VM uses `ls:` for an empty list. Preserve the empty value here and
    // resolve it against each generated operation's declared list type.
    if value == "ls:" {
        return Ok(Arg::EmptyList);
    }
    if let Some(value) = value.strip_prefix("i:") {
        return value
            .parse::<i64>()
            .map(Arg::Int)
            .map_err(|error| protocol_error("invalid_argument", &error.to_string()));
    }
    if let Some(value) = value.strip_prefix("f:") {
        return value
            .parse::<f64>()
            .map(Arg::Float)
            .map_err(|error| protocol_error("invalid_argument", &error.to_string()));
    }
    if let Some(value) = value.strip_prefix("b:") {
        return value
            .parse::<bool>()
            .map(Arg::Bool)
            .map_err(|error| protocol_error("invalid_argument", &error.to_string()));
    }
    if let Some(value) = value.strip_prefix("s:") {
        return decode_text(value).map(Arg::String);
    }
    if let Some(value) = value.strip_prefix("li:") {
        if value.is_empty() {
            return Ok(Arg::Ints(Vec::new()));
        }
        return value
            .split(',')
            .map(|value| {
                value.parse::<i64>().map_err(|error| {
                    protocol_error("invalid_argument", &error.to_string())
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Arg::Ints);
    }
    if let Some(value) = value.strip_prefix("lf:") {
        if value.is_empty() {
            return Ok(Arg::Floats(Vec::new()));
        }
        return value
            .split(',')
            .map(|value| {
                value.parse::<f64>().map_err(|error| {
                    protocol_error("invalid_argument", &error.to_string())
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Arg::Floats);
    }
    if let Some(value) = value.strip_prefix("lb:") {
        if value.is_empty() {
            return Ok(Arg::Bools(Vec::new()));
        }
        return value
            .split(',')
            .map(|value| {
                value.parse::<bool>().map_err(|error| {
                    protocol_error("invalid_argument", &error.to_string())
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Arg::Bools);
    }
    if let Some(value) = value.strip_prefix("h:") {
        let fields = value.split(':').collect::<Vec<_>>();
        let [id, generation, type_name] = fields.as_slice() else {
            return Err(protocol_error("invalid_argument", "malformed handle"));
        };
        return Ok(Arg::Handle(HandleArg {
            id: id.parse().map_err(|error: std::num::ParseIntError| protocol_error("invalid_argument", &error.to_string()))?,
            generation: generation.parse().map_err(|error: std::num::ParseIntError| protocol_error("invalid_argument", &error.to_string()))?,
            type_name: decode_text(type_name)?,
        }));
    }
    Err(protocol_error("invalid_argument", "unsupported argument encoding"))
}

fn decode_text(value: &str) -> Result<String, String> {
    let bytes = STANDARD
        .decode(value)
        .map_err(|error| protocol_error("invalid_base64", &error.to_string()))?;
    String::from_utf8(bytes)
        .map_err(|error| protocol_error("invalid_utf8", &error.to_string()))
}

fn arg_ints(value: &Arg) -> &[i64] {
    match value {
        Arg::Ints(values) => values.as_slice(),
        Arg::EmptyList => &[],
        _ => unreachable!("generated argument pattern admits only integer lists"),
    }
}

fn arg_floats(value: &Arg) -> &[f64] {
    match value {
        Arg::Floats(values) => values.as_slice(),
        Arg::EmptyList => &[],
        _ => unreachable!("generated argument pattern admits only float lists"),
    }
}

fn arg_bools(value: &Arg) -> &[bool] {
    match value {
        Arg::Bools(values) => values.as_slice(),
        Arg::EmptyList => &[],
        _ => unreachable!("generated argument pattern admits only boolean lists"),
    }
}

fn validate_decoded_arg_shape(value: &Arg) {
    match value {
        Arg::Int(value) => { let _ = value; }
        Arg::Float(value) => { let _ = value; }
        Arg::Bool(value) => { let _ = value; }
        Arg::String(value) => { let _ = value; }
        Arg::Ints(_) => { let _ = arg_ints(value); }
        Arg::Floats(_) => { let _ = arg_floats(value); }
        Arg::Bools(_) => { let _ = arg_bools(value); }
        Arg::EmptyList => {}
        Arg::Handle(value) => { let _ = value; }
    }
}

fn native_error(error: &CAbiError) -> String {
    protocol_error(
        &format!("c_abi_status_{}", error.status),
        error.operation,
    )
}

fn protocol_error(code: &str, message: &str) -> String {
    format!("err {} {}", STANDARD.encode(code), STANDARD.encode(message))
}
"##;
    let crate_ident = manifest.package.crate_name.replace('-', "_");
    Ok(template
        .replace("@CRATE@", &crate_ident)
        .replace("@IMPORTS@", &imports.join(", "))
        .replace("@MATCH_ARMS@", &match_arms)
        .replace("@HANDLE_VARIANTS@", &handle_variants)
        .replace("@HANDLE_TYPE_ARMS@", &handle_type_arms)
        .replace("@HANDLE_ACCESSORS@", &handle_accessors)
        .replace(
            "@MAX_FRAME_BYTES@",
            &crate::runtime::native_boundary::adapter_abi::PUBLIC_ADAPTER_MAX_FRAME_BYTES
                .to_string(),
        ))
}
