use std::collections::BTreeMap;

use super::{c_pointer_base, is_c_identifier, resolve_c_type};

pub(super) fn rust_ffi_type(
    c_type: &str,
    aliases: &BTreeMap<String, String>,
) -> Result<String, String> {
    let resolved = resolve_c_type(c_type, aliases)?;
    let c_type = resolved.trim();
    let pointer_depth = c_type.matches('*').count();
    let is_const = c_type.starts_with("const ");
    let base = c_pointer_base(c_type);
    let mut rust_type = match base {
        "void" => "()".to_string(),
        "bool" => "bool".to_string(),
        "int8_t" => "i8".to_string(),
        "uint8_t" => "u8".to_string(),
        "int16_t" => "i16".to_string(),
        "uint16_t" => "u16".to_string(),
        "int32_t" => "i32".to_string(),
        "uint32_t" => "u32".to_string(),
        "int64_t" => "i64".to_string(),
        "uint64_t" => "u64".to_string(),
        "size_t" => "usize".to_string(),
        "float" => "f32".to_string(),
        "double" => "f64".to_string(),
        "char" => "std::ffi::c_char".to_string(),
        value if is_c_identifier(value) => value.to_string(),
        _ => {
            return Err(format!(
                "error[native_bindgen.unsupported_c_type]: `{c_type}` has no Rust FFI mapping"
            ));
        }
    };
    for depth in 0..pointer_depth {
        let qualifier = if depth == 0 && is_const {
            "*const"
        } else {
            "*mut"
        };
        rust_type = format!("{qualifier} {rust_type}");
    }
    Ok(rust_type)
}
