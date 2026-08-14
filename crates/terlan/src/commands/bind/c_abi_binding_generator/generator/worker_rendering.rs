use super::*;

mod argument_rendering;
mod dispatch_rendering;
use argument_rendering::{
    render_argument_binding, render_immutable_resource_borrows,
    render_immutable_resource_list_borrows,
};
use dispatch_rendering::{render_dispatch_calls, render_dispatch_modules};

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
        .map(|argument| match argument.ty.as_str() {
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
            if function.role == CAbiFunctionRole::MutableMethod && handle_arguments.len() > 1 {
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
            } else {
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
            if function.role == CAbiFunctionRole::MutableMethod
                && function
                    .args
                    .iter()
                    .any(|argument| resource_list(&argument.ty).is_some())
            {
                return Err(format!(
                    "error[native_bindgen.unsupported_wrapper_shape]: mutable method `{}` cannot borrow a resource list",
                    function.name
                ));
            }
            borrows.push_str(&render_immutable_resource_list_borrows(
                manifest,
                &function.args,
            ));
            let method_arguments = function
                .args
                .iter()
                .skip(1)
                .map(|argument| render_argument_binding(manifest, argument))
                .collect::<Vec<_>>()
                .join(", ");
            let receiver = &handle_arguments[0].name;
            let call = format!("value_{receiver}.{}({method_arguments})", function.name);
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
            let call = format!("{}({bindings})", function.name);
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
        .map(|function| function.name.clone())
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
        for (chunk_index, chunk_functions) in functions.chunks(32).enumerate() {
            let mut chunk = String::from("use super::*;\n\nimpl Worker {\n");
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
    let template = r##"#![forbid(unsafe_code)]

@DISPATCH_MODULES@use std::collections::HashMap;
use std::io::{self, BufRead, Read, Write};

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use @CRATE@::{@IMPORTS@};

const MAX_ADAPTER_FRAME_BYTES: usize = @MAX_FRAME_BYTES@;
const MAX_ADAPTER_TRANSFER_BYTES: usize = @MAX_TRANSFER_BYTES@;

fn main() {
    let mut worker = match Worker::new() {
        Ok(worker) => worker,
        Err(error) => {
            eprintln!("native worker identity initialization failed: {error}");
            return;
        }
    };
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

@RESOURCE_DEAD_CODE@struct Worker {
    owner: String,
    last_request_id: Option<u64>,
    next_id: u64,
    free_ids: Vec<u64>,
    generations: HashMap<u64, u64>,
    handles: HashMap<u64, HandleEntry>,
}

@RESOURCE_DEAD_CODE@struct HandleEntry {
    generation: u64,
    value: HandleValue,
}

@RESOURCE_DEAD_CODE@enum HandleValue {
@HANDLE_VARIANTS@
}

@RESOURCE_DEAD_CODE@impl HandleValue {
    fn type_name(&self) -> &'static str {
        match self {
@HANDLE_TYPE_ARMS@
        }
    }
}

@RESOURCE_DEAD_CODE@#[derive(Clone)]
struct HandleArg {
    owner: String,
    id: u64,
    generation: u64,
    type_name: String,
}

enum Arg {
    Int(i64),
    Float(f64),
    Bool(bool),
    String(String),
    Bytes(Vec<u8>),
    Ints(Vec<i64>),
    Floats(Vec<f64>),
    Bools(Vec<bool>),
    Handles(Vec<HandleArg>),
    EmptyList,
    Handle(HandleArg),
}

struct Request {
    operation: String,
    args: Vec<Arg>,
}

@RESOURCE_DEAD_CODE@impl Worker {
    fn new() -> Result<Self, getrandom::Error> {
        let mut owner = [0_u8; 32];
        getrandom::fill(&mut owner)?;
        Ok(Self {
            owner: STANDARD.encode(owner),
            last_request_id: None,
            next_id: 0,
            free_ids: Vec::new(),
            generations: HashMap::new(),
            handles: HashMap::new(),
        })
    }

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
@DISPATCH_CALLS@
    }

    fn validate(&self, handle: &HandleArg, expected_type: &str) -> Result<(), String> {
        if handle.owner != self.owner {
            return Err(protocol_error(
                "cross_owner_handle",
                "native resource belongs to another worker",
            ));
        }
        if handle.type_name != expected_type {
            return Err(protocol_error("handle_type_mismatch", &handle.type_name));
        }
        match self.handles.get(&handle.id) {
            Some(entry) if entry.generation != handle.generation => {
                Err(protocol_error("stale_handle", "NativeBoundary handle is stale"))
            }
            Some(entry) if entry.value.type_name() != expected_type => Err(protocol_error(
                "handle_storage_mismatch",
                "NativeBoundary handle resource type does not match",
            )),
            Some(_) => Ok(()),
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
    if let Some(value) = value.strip_prefix("x:") {
        let bytes = STANDARD
            .decode(value)
            .map_err(|error| protocol_error("invalid_base64", &error.to_string()))?;
        if bytes.len() > MAX_ADAPTER_TRANSFER_BYTES {
            return Err(protocol_error(
                "transfer_too_large",
                "copied byte argument exceeds the native adapter transfer bound",
            ));
        }
        return Ok(Arg::Bytes(bytes));
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
    if let Some(value) = value.strip_prefix("lh:") {
        if value.is_empty() {
            return Ok(Arg::Handles(Vec::new()));
        }
        return value
            .split(',')
            .map(parse_handle_arg)
            .collect::<Result<Vec<_>, _>>()
            .map(Arg::Handles);
    }
    if let Some(value) = value.strip_prefix("h:") {
        return parse_handle_arg(value).map(Arg::Handle);
    }
    Err(protocol_error("invalid_argument", "unsupported argument encoding"))
}

fn parse_handle_arg(value: &str) -> Result<HandleArg, String> {
    let fields = value.split(':').collect::<Vec<_>>();
    let [owner, id, generation, type_name] = fields.as_slice() else {
        return Err(protocol_error("invalid_argument", "malformed handle"));
    };
    Ok(HandleArg {
        owner: decode_text(owner)?,
        id: id.parse().map_err(|error: std::num::ParseIntError| protocol_error("invalid_argument", &error.to_string()))?,
        generation: generation.parse().map_err(|error: std::num::ParseIntError| protocol_error("invalid_argument", &error.to_string()))?,
        type_name: decode_text(type_name)?,
    })
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

fn arg_handles(value: &Arg) -> &[HandleArg] {
    match value {
        Arg::Handles(values) => values.as_slice(),
        Arg::EmptyList => &[],
        _ => unreachable!("generated argument pattern admits only resource lists"),
    }
}

fn validate_decoded_arg_shape(value: &Arg) {
    match value {
        Arg::Int(value) => { let _ = value; }
        Arg::Float(value) => { let _ = value; }
        Arg::Bool(value) => { let _ = value; }
        Arg::String(value) => { let _ = value; }
        Arg::Bytes(value) => { let _ = value; }
        Arg::Ints(_) => { let _ = arg_ints(value); }
        Arg::Floats(_) => { let _ = arg_floats(value); }
        Arg::Bools(_) => { let _ = arg_bools(value); }
        Arg::Handles(_) => { let _ = arg_handles(value); }
        Arg::EmptyList => {}
        Arg::Handle(value) => { let _ = value; }
    }
}

@FALLIBLE_DEAD_CODE@fn native_error(error: &CAbiError) -> String {
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
    let dispatch_calls = render_dispatch_calls(&functions, &dispatch_chunks, &inline_match_arms);
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
