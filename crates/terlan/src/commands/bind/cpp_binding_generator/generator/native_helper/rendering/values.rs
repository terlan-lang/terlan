//! Native helper argument conversion and result projection.

use super::*;
use crate::commands::bind::cpp_binding_generator::error::CppBindingError;

/// Renders a slice pattern that preserves argument positions without using names.
pub(super) fn render_arg_pattern(
    manifest: &NativeBindingManifest,
    args: &[NativeBindingArg],
) -> Result<String, CppBindingError> {
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
                return Err((format!(
                    "native helper cannot decode argument type `{}` for `{}`",
                    arg.ty, arg.name
                ))
                .into());
            }
        };
        patterns.push(pattern);
    }
    Ok(format!("[{}]", patterns.join(", ")))
}

/// Renders a direct free-function or constructor call.
pub(super) fn render_ffi_call(
    manifest: &NativeBindingManifest,
    name: &str,
    args: &[NativeBindingArg],
    skip: usize,
) -> Result<String, CppBindingError> {
    Ok(format!(
        "ffi::{name}({})",
        render_call_args(manifest, args, skip)?
    ))
}

/// Renders call arguments after any receiver handle.
pub(super) fn render_call_args(
    manifest: &NativeBindingManifest,
    args: &[NativeBindingArg],
    skip: usize,
) -> Result<String, CppBindingError> {
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
                _ => Err((format!(
                    "native helper cannot pass argument type `{}` for `{}`",
                    arg.ty, arg.name
                ))
                .into()),
            }
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|args| args.into_iter().flatten().collect::<Vec<_>>().join(", "))
}

/// Validates and borrows secondary opaque-resource arguments for one call.
pub(super) fn render_borrowed_resource_bindings(
    manifest: &NativeBindingManifest,
    args: &[NativeBindingArg],
    skip: usize,
) -> Result<String, CppBindingError> {
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
pub(super) fn render_resource_list_input_bindings(
    manifest: &NativeBindingManifest,
    function: &NativeBindingFunction,
    symbol: &CppSymbol,
) -> Result<String, CppBindingError> {
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
pub(super) fn render_record_arguments(
    ty: &NativeBindingType,
    argument: &NativeBindingArg,
    index: usize,
) -> Result<Vec<String>, CppBindingError> {
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
                    return Err((format!(
                        "native helper cannot copy record field type `{}` for `{}.{}`",
                        field.ty, argument.name, field.name
                    )).into());
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
pub(super) fn render_enum_argument(
    manifest: &NativeBindingManifest,
    ty: &NativeBindingType,
    index: usize,
    argument_name: &str,
) -> Result<String, CppBindingError> {
    if ty.variants.is_empty() {
        return Err((format!("enum `{}` has no reviewed variants", ty.name)).into());
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
pub(super) fn render_result(
    manifest: &NativeBindingManifest,
    ty: &str,
) -> Result<String, CppBindingError> {
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
        _ => Err((format!("native helper cannot encode result type `{ty}`")).into()),
    }
}

/// Renders either an ordinary copied result or a newly owned resource handle.
pub(super) fn render_function_result(
    manifest: &NativeBindingManifest,
    module: &NativeBindingModule,
    function: &NativeBindingFunction,
    symbols: &BTreeMap<&str, &CppSymbol>,
) -> Result<String, CppBindingError> {
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
pub(super) fn render_owned_copy(
    manifest: &NativeBindingManifest,
    kind: &str,
    expression: &str,
) -> String {
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
pub(super) fn find_return_resource<'a>(
    manifest: &'a NativeBindingManifest,
    function: &NativeBindingFunction,
) -> Result<(&'a NativeBindingModule, &'a NativeBindingType), CppBindingError> {
    Ok(
        find_resource_type(manifest, &function.returns).ok_or_else(|| {
            format!(
                "constructor `{}` must return a package-owned resource",
                function.name
            )
        })?,
    )
}

/// Finds the resource-handle argument used by a method or disposer.
pub(super) fn find_handle_resource<'a>(
    manifest: &'a NativeBindingManifest,
    function: &NativeBindingFunction,
) -> Result<(&'a NativeBindingModule, &'a NativeBindingType, usize), CppBindingError> {
    for (index, arg) in function.args.iter().enumerate() {
        if let Some((module, ty)) = find_resource_type(manifest, &arg.ty) {
            return Ok((module, ty, index));
        }
    }
    Err((format!(
        "function `{}` requires a package-owned resource",
        function.name
    ))
    .into())
}

/// Resolves a local or fully qualified Terlan type to its owning resource module.
pub(super) fn find_resource_type<'a>(
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
pub(super) fn find_optional_resource_type<'a>(
    manifest: &'a NativeBindingManifest,
    value: &str,
) -> Option<(&'a NativeBindingModule, &'a NativeBindingType)> {
    value
        .strip_prefix("Option[")
        .and_then(|inner| inner.strip_suffix(']'))
        .and_then(|inner| find_resource_type(manifest, inner.trim()))
}

pub(super) fn find_resource_list_type<'a>(
    manifest: &'a NativeBindingManifest,
    value: &str,
) -> Option<(&'a NativeBindingModule, &'a NativeBindingType)> {
    value
        .strip_prefix("List[")
        .and_then(|element| element.strip_suffix(']'))
        .and_then(|element| find_resource_type(manifest, element.trim()))
}

/// Resolves a local or fully qualified Terlan type to a generated finite enum.
pub(super) fn find_enum_type<'a>(
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
pub(super) fn find_value_record_type<'a>(
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
pub(super) fn find_string_value_type<'a>(
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
pub(super) fn type_matches(value: &str, ty: &NativeBindingType) -> bool {
    value == ty.name || value.ends_with(&format!(".{}", ty.name))
}

/// Returns whether a helper argument is syntactically a generated type.
pub(super) fn is_resource_type(value: &str) -> bool {
    value
        .split('.')
        .next_back()
        .and_then(|name| name.chars().next())
        .is_some_and(char::is_uppercase)
        && !matches!(value, "Int" | "Bool" | "String" | "Unit")
}

/// Resolves a generated resource to its C++ type name.
pub(super) fn cpp_type<'a>(
    ty: &NativeBindingType,
    symbols: &'a BTreeMap<&str, &CppSymbol>,
) -> Result<&'a str, CppBindingError> {
    Ok(symbols
        .get(ty.cpp_symbol.as_str())
        .map(|symbol| symbol.cpp_name.as_str())
        .ok_or_else(|| format!("unknown C++ type symbol `{}`", ty.cpp_symbol))?)
}

/// Creates a collision-resistant Rust enum variant for a module-owned resource.
pub(super) fn resource_variant(module: &NativeBindingModule, ty: &NativeBindingType) -> String {
    module
        .module
        .split('.')
        .chain(std::iter::once(ty.name.as_str()))
        .map(upper_camel_identifier)
        .collect()
}

/// Converts one validated source identifier into a warning-clean Rust variant segment.
pub(super) fn upper_camel_identifier(value: &str) -> String {
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
pub(super) fn resource_type_name(module: &NativeBindingModule, ty: &NativeBindingType) -> String {
    format!("{}.{}", module.module, ty.name)
}
