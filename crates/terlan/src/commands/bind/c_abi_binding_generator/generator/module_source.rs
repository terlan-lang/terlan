//! Terlan declarations and documentation for validated C bindings.

use super::*;

fn referenced_opaque_imports(
    manifest: &CAbiBindingManifest,
    module: &CAbiBindingModule,
) -> BTreeMap<String, BTreeSet<String>> {
    let referenced_tokens = module
        .functions
        .iter()
        .flat_map(|function| {
            function
                .args
                .iter()
                .map(|argument| argument.ty.as_str())
                .chain(std::iter::once(function.returns.as_str()))
        })
        .flat_map(|ty| {
            ty.split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
                .filter(|token| !token.is_empty())
        })
        .collect::<BTreeSet<_>>();
    let mut imports = BTreeMap::<String, BTreeSet<String>>::new();
    for owner in &manifest.modules {
        if owner.module == module.module {
            continue;
        }
        for ty in &owner.types {
            if referenced_tokens.contains(ty.name.as_str()) {
                imports
                    .entry(owner.module.clone())
                    .or_default()
                    .insert(ty.name.clone());
            }
        }
    }
    imports
}

pub(super) fn render_module_source(
    manifest: &CAbiBindingManifest,
    module: &CAbiBindingModule,
) -> String {
    let mut source = format!(
        "/**\n * {}\n */\n\nmodule {}.\n\n",
        module.documentation, module.module
    );
    let mut imports = referenced_opaque_imports(manifest, module);
    for name in module
        .functions
        .iter()
        .flat_map(|function| {
            std::iter::once(function.returns.as_str())
                .chain(function.args.iter().map(|arg| arg.ty.as_str()))
        })
        .flat_map(|ty| ty.split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_' && ch != '.'))
        .filter(|name| matches!(*name, "Option" | "Result"))
        .filter(|name| !module.types.iter().any(|ty| ty.name == *name))
    {
        imports
            .entry("std.core".into())
            .or_default()
            .insert(name.into());
    }
    for import in &module.imports {
        imports
            .entry(import.module.clone())
            .or_default()
            .extend(import.names.iter().cloned());
    }
    for (owner, types) in imports {
        source.push_str(&format!(
            "import {}.{{{}}}.\n",
            owner,
            types.into_iter().collect::<Vec<_>>().join(", ")
        ));
    }
    if source.ends_with("}.\n") {
        source.push('\n');
    }
    for ty in &module.types {
        source.push_str(&format!(
            "/** {} */\npub opaque type {}.\n\n",
            ty.documentation, ty.name
        ));
    }
    for function in &module.functions {
        let visibility = if function.is_public() { "pub " } else { "" };
        source.push_str(&format!(
            "/** {} */\n{}\n{visibility}{}({}): {}.\n",
            function.documentation,
            crate::compiler::native_bindings::annotation(
                &module.module,
                &function.name,
                &function.operation
            ),
            function.name,
            render_args(&function.args),
            crate::terlan_hir::public_type_text(&function.returns)
        ));
    }
    source
}

pub(super) fn render_module_docs(
    module: &CAbiBindingModule,
    symbols: &BTreeMap<&str, &CSymbol>,
) -> String {
    let mut docs = format!("# {}\n\n{}\n\n", module.module, module.documentation);
    for function in module
        .functions
        .iter()
        .filter(|function| function.is_public())
    {
        docs.push_str(&format!(
            "## `{}`\n\n{}\n\n",
            function.name, function.documentation
        ));
        if let Some(symbol) = symbols.get(function.c_symbol.as_str()) {
            docs.push_str(&format!("- C symbol: `{}`\n", symbol.c_name));
        }
        docs.push_str(&format!(
            "- NativeBoundary operation: `{}`\n- Ownership: `{}`\n\n",
            function.operation,
            resource_policy_name(&function.resource)
        ));
    }
    docs.truncate(docs.trim_end().len());
    docs.push('\n');
    docs
}
