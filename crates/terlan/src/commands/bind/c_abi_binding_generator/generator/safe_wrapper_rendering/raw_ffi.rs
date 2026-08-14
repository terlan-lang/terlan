use super::*;

pub(in super::super) fn render_raw_ffi_function(
    symbol: &CSymbol,
    aliases: &BTreeMap<String, String>,
) -> Result<String, String> {
    let args = symbol
        .parameters
        .iter()
        .map(|parameter| {
            Ok(format!(
                "{}: {}",
                parameter.name,
                rust_ffi_type(&parameter.c_type, aliases)?
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let returns = rust_ffi_type(symbol.returns.as_deref().unwrap_or("void"), aliases)?;
    let return_text = if returns == "()" {
        String::new()
    } else {
        format!(" -> {returns}")
    };
    Ok(format!(
        "        pub fn {}({}){};\n",
        symbol.c_name,
        args.join(", "),
        return_text
    ))
}
