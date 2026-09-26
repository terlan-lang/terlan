//! Generated C++ containment for explicitly selected throwing callables.

use crate::commands::bind::cpp_binding_generator::error::CppBindingError;
use std::collections::BTreeMap;
use std::path::Path;

use super::owned_value_adapter::{
    cpp_call_argument_with_presence, cpp_default_argument_expression, cpp_parameters,
    selected_overload_invocation, selected_overload_invocation_with_receiver,
};
use super::{
    cpp_name_matches, mapped_cpp_parameter_count, public_cpp_parameter_mappings,
    terlan_type_matches, CppExceptionPolicy, CppSymbol, CppSymbolKind, NativeBindingFunction,
    NativeBindingManifest, NativeBindingModule, NativeBindingTypeKind, NativeFunctionRole,
};

/// Opaque C++ result envelope shared by all contained calls in one package.
pub(super) const EXCEPTION_ENVELOPE: &str = "TerlanExceptionEnvelope";

/// Returns the deterministic bridge name for one contained C++ callable.
pub(super) fn exception_adapter_name(
    module: &NativeBindingModule,
    function: &NativeBindingFunction,
) -> String {
    let owner = module
        .module
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!("terlan_exception_{owner}_{}", function.name)
}

/// Renders the opaque result envelope and `noexcept` adapter declarations.
pub(super) fn render_exception_adapter_header(
    manifest: &NativeBindingManifest,
    symbols: &BTreeMap<&str, &CppSymbol>,
) -> Result<String, CppBindingError> {
    let header = Path::new(&manifest.cpp_metadata.header)
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "C++ metadata header requires a UTF-8 file name".to_string())?;
    let mut source = format!(
        "#pragma once\n\n#include \"{header}\"\n\n#include <cstdint>\n#include <memory>\n#include <string>\n\nnamespace {} {{\n\n",
        manifest.cpp_metadata.namespace
    );
    source.push_str(&format!(
        "class {EXCEPTION_ENVELOPE} final {{\n public:\n  {EXCEPTION_ENVELOPE}(bool ok, std::int64_t value, double float_value, bool bool_value, std::string code, std::string message) noexcept;\n  bool is_ok() const noexcept;\n  std::int64_t value() const noexcept;\n  double float_value() const noexcept;\n  bool bool_value() const noexcept;\n  const std::string& code() const noexcept;\n  const std::string& message() const noexcept;\n\n private:\n  bool ok_;\n  std::int64_t value_;\n  double float_value_;\n  bool bool_value_;\n  std::string code_;\n  std::string message_;\n}};\n\n"
    ));
    for module in &manifest.modules {
        for function in exception_functions(module) {
            let (callable, _) = exception_parts(manifest, function, symbols)?;
            source.push_str(&format!(
                "std::unique_ptr<{EXCEPTION_ENVELOPE}> {}({}) noexcept;\n",
                exception_adapter_name(module, function),
                contained_cpp_parameters(manifest, function, callable, symbols)?,
            ));
        }
    }
    source.push_str(&format!(
        "\n}}  // namespace {}\n",
        manifest.cpp_metadata.namespace
    ));
    Ok(source)
}

/// Renders catch-all wrappers that suppress every upstream exception payload.
pub(super) fn render_exception_adapter_source(
    manifest: &NativeBindingManifest,
    symbols: &BTreeMap<&str, &CppSymbol>,
) -> Result<String, CppBindingError> {
    let mut source = format!(
        "#include \"include/terlan_exception_adapters.hpp\"\n\n#include <stdexcept>\n#include <utility>\n\nnamespace {} {{\n\n",
        manifest.cpp_metadata.namespace
    );
    source.push_str(&format!(
        "{EXCEPTION_ENVELOPE}::{EXCEPTION_ENVELOPE}(bool ok, std::int64_t value, double float_value, bool bool_value, std::string code, std::string message) noexcept\n    : ok_(ok), value_(value), float_value_(float_value), bool_value_(bool_value), code_(std::move(code)), message_(std::move(message)) {{}}\n\nbool {EXCEPTION_ENVELOPE}::is_ok() const noexcept {{ return ok_; }}\nstd::int64_t {EXCEPTION_ENVELOPE}::value() const noexcept {{ return value_; }}\ndouble {EXCEPTION_ENVELOPE}::float_value() const noexcept {{ return float_value_; }}\nbool {EXCEPTION_ENVELOPE}::bool_value() const noexcept {{ return bool_value_; }}\nconst std::string& {EXCEPTION_ENVELOPE}::code() const noexcept {{ return code_; }}\nconst std::string& {EXCEPTION_ENVELOPE}::message() const noexcept {{ return message_; }}\n\n"
    ));
    for module in &manifest.modules {
        for function in exception_functions(module) {
            let (callable, policy) = exception_parts(manifest, function, symbols)?;
            let arguments = contained_cpp_arguments(manifest, function, callable);
            let (selection, invocation) = contained_invocation(callable, &arguments)?;
            let input_validation =
                contained_integer_enum_input_validation(manifest, function, callable);
            let result_validation =
                contained_integer_enum_result_validation(manifest, function, callable);
            source.push_str(&format!(
                "std::unique_ptr<{EXCEPTION_ENVELOPE}> {}({}) noexcept {{\n  try {{\n    try {{\n{input_validation}{}      const auto result = {};\n{result_validation}      return std::make_unique<{EXCEPTION_ENVELOPE}>(true, {}, \"\", \"\");\n    }} catch (...) {{\n      return std::make_unique<{EXCEPTION_ENVELOPE}>(false, 0, 0.0, false, {:?}, {:?});\n    }}\n  }} catch (...) {{\n    return nullptr;\n  }}\n}}\n\n",
                exception_adapter_name(module, function),
                contained_cpp_parameters(manifest, function, callable, symbols)?,
                selection,
                invocation,
                success_values(function)?,
                policy.error_code,
                policy.message,
            ));
        }
    }
    source.push_str(&format!(
        "}}  // namespace {}\n",
        manifest.cpp_metadata.namespace
    ));
    Ok(source)
}

/// Rejects historical integer enum values that are outside the package's
/// reviewed finite public enum before constructing the upstream C++ enum.
fn contained_integer_enum_input_validation(
    manifest: &NativeBindingManifest,
    function: &NativeBindingFunction,
    callable: &CppSymbol,
) -> String {
    public_cpp_parameter_mappings(function, callable)
        .into_iter()
        .filter_map(|mapping| {
            if mapping.public_type != Some("Int") {
                return None;
            }
            let parameter = &callable.parameters[mapping.cpp_parameter_index];
            let (cpp_enum, variants) = reviewed_integer_enum(manifest, &parameter.ty)?;
            let rejected = variants
                .iter()
                .map(|variant| {
                    format!(
                        "{} != static_cast<std::int64_t>({cpp_enum}::{})",
                        parameter.name, variant
                    )
                })
                .collect::<Vec<_>>()
                .join(" && ");
            Some(format!(
                "      if ({rejected}) {{\n        throw std::invalid_argument(\"{} is outside the reviewed enum domain\");\n      }}\n",
                parameter.name
            ))
        })
        .collect::<Vec<_>>()
        .join("")
}

/// Rejects an upstream enum sentinel that is not exposed by the package's
/// reviewed finite public enum before copying its discriminant to Terlan.
fn contained_integer_enum_result_validation(
    manifest: &NativeBindingManifest,
    function: &NativeBindingFunction,
    callable: &CppSymbol,
) -> String {
    if function.returns != "Int" {
        return String::new();
    }
    let Some(returns) = callable.returns.as_ref() else {
        return String::new();
    };
    let Some((cpp_enum, variants)) = reviewed_integer_enum(manifest, returns) else {
        return String::new();
    };
    let rejected = variants
        .iter()
        .map(|variant| format!("result != {cpp_enum}::{variant}"))
        .collect::<Vec<_>>()
        .join(" && ");
    format!(
        "      if ({rejected}) {{\n        throw std::runtime_error(\"C++ result is outside the reviewed enum domain\");\n      }}\n"
    )
}

/// Resolves one extracted enum to the exact package-selected enumerators.
fn reviewed_integer_enum<'a>(
    manifest: &'a NativeBindingManifest,
    cpp_type: &super::CppTypeMetadata,
) -> Option<(&'a str, Vec<&'a str>)> {
    if !cpp_type.enum_type {
        return None;
    }
    let symbol = manifest.cpp_metadata.symbols.iter().find(|symbol| {
        symbol.kind == CppSymbolKind::Enum
            && (cpp_name_matches(&cpp_type.canonical, &symbol.cpp_name)
                || cpp_name_matches(&cpp_type.canonical, &symbol.overload_set))
    })?;
    let binding = manifest
        .modules
        .iter()
        .flat_map(|module| &module.types)
        .find(|ty| ty.kind == NativeBindingTypeKind::Enum && ty.cpp_symbol == symbol.id)?;
    (!binding.variants.is_empty()).then(|| {
        (
            symbol.overload_set.as_str(),
            binding
                .variants
                .iter()
                .map(|variant| variant.cpp_name.as_str())
                .collect(),
        )
    })
}

/// Returns exception-contained operations from one generated module.
fn exception_functions(
    module: &NativeBindingModule,
) -> impl Iterator<Item = &NativeBindingFunction> {
    module.functions.iter().filter(|function| {
        matches!(
            function.role,
            NativeFunctionRole::ExceptionMethod | NativeFunctionRole::ScalarProjection
        )
    })
}

/// Selects the exact primitive slot used by one contained success value.
fn success_values(function: &NativeBindingFunction) -> Result<&'static str, CppBindingError> {
    match function.returns.as_str() {
        "Float" => Ok("0, result, false"),
        "Bool" => Ok("0, 0.0, result"),
        "Int" | "Result[Int, std.core.Error.Error]" => {
            Ok("static_cast<std::int64_t>(result), 0.0, false")
        }
        returns => Err((format!(
            "contained callable `{}` has unsupported primitive result `{returns}`",
            function.name
        ))
        .into()),
    }
}

/// Uses the concrete package resource for inherited methods and bridge-safe
/// generated parameter spellings for all remaining arguments.
fn contained_cpp_parameters(
    manifest: &NativeBindingManifest,
    function: &NativeBindingFunction,
    callable: &CppSymbol,
    symbols: &BTreeMap<&str, &CppSymbol>,
) -> Result<String, CppBindingError> {
    let parameters = cpp_parameters(manifest, function, callable)?;
    if callable.kind != CppSymbolKind::Method {
        return Ok(parameters);
    }
    let resource = function
        .args
        .first()
        .and_then(|arg| {
            manifest
                .modules
                .iter()
                .flat_map(|module| &module.types)
                .find(|ty| {
                    ty.kind == NativeBindingTypeKind::OpaqueResource
                        && terlan_type_matches(&arg.ty, &ty.name)
                })
        })
        .ok_or_else(|| format!("exception method `{}` has no resource", function.name))?;
    let resource_symbol = symbols
        .get(resource.cpp_symbol.as_str())
        .copied()
        .ok_or_else(|| format!("unknown C++ type symbol `{}`", resource.cpp_symbol))?;
    let receiver = callable
        .receiver
        .as_deref()
        .ok_or_else(|| format!("C++ method `{}` has no receiver", callable.id))?;
    let prefix = format!("const {receiver}& value");
    let suffix = parameters.strip_prefix(&prefix).ok_or_else(|| {
        format!(
            "C++ method `{}` has inconsistent receiver parameters",
            callable.id
        )
    })?;
    Ok(format!(
        "const {}& value{suffix}",
        resource_symbol.overload_set
    ))
}

/// Lowers all public arguments to the selected C++ parameter list, including
/// tagged Scalars, integer-compatible enums, and omitted optional defaults.
fn contained_cpp_arguments(
    manifest: &NativeBindingManifest,
    function: &NativeBindingFunction,
    callable: &CppSymbol,
) -> String {
    let mappings = public_cpp_parameter_mappings(function, callable);
    let parameter_count = mapped_cpp_parameter_count(function, callable);
    callable
        .parameters
        .iter()
        .enumerate()
        .filter_map(|(index, parameter)| {
            if let Some(mapping) = mappings
                .iter()
                .find(|mapping| mapping.cpp_parameter_index == index)
            {
                Some(cpp_call_argument_with_presence(
                    manifest,
                    &parameter.name,
                    parameter,
                    mapping.public_type,
                    mapping.presence_name,
                    mapping.scalar_choice.as_ref(),
                ))
            } else if index < parameter_count {
                Some("std::nullopt".to_string())
            } else if callable.overload_candidates > 1 {
                Some(cpp_default_argument_expression(parameter))
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Selects one exact overload and renders its call without relying on C++
/// conversion ranking.
fn contained_invocation(
    callable: &CppSymbol,
    arguments: &str,
) -> Result<(String, String), CppBindingError> {
    if callable.overload_candidates > 1 {
        return if callable.kind == CppSymbolKind::Method {
            selected_overload_invocation_with_receiver(callable, arguments, "value")
        } else {
            selected_overload_invocation(callable, arguments)
        };
    }
    if callable.kind == CppSymbolKind::Method {
        return Ok((
            String::new(),
            format!("value.{}({arguments})", callable_name(callable)),
        ));
    }
    let call_name = if callable.template_arguments.is_empty() {
        callable.overload_set.clone()
    } else {
        callable
            .overload_set
            .rsplit_once("::")
            .map(|(namespace, _)| format!("{namespace}::{}", callable_name(callable)))
            .unwrap_or_else(|| callable_name(callable))
    };
    Ok((String::new(), format!("{call_name}({arguments})")))
}

/// Renders the callable token for a concrete function-template specialization.
fn callable_name(symbol: &CppSymbol) -> String {
    if symbol.template_arguments.is_empty() {
        symbol.cpp_name.clone()
    } else {
        format!(
            "{}<{}>",
            symbol.cpp_name,
            symbol.template_arguments.join(", ")
        )
    }
}

/// Resolves one callable's extractor declaration and stable policy.
fn exception_parts<'a>(
    manifest: &'a NativeBindingManifest,
    function: &'a NativeBindingFunction,
    symbols: &'a BTreeMap<&str, &CppSymbol>,
) -> Result<(&'a CppSymbol, &'a CppExceptionPolicy), CppBindingError> {
    let symbol_id = function
        .cpp_symbol
        .as_deref()
        .ok_or_else(|| format!("contained callable `{}` has no C++ symbol", function.name))?;
    let callable = symbols
        .get(symbol_id)
        .copied()
        .ok_or_else(|| format!("unknown contained C++ callable `{symbol_id}`"))?;
    let policy = manifest
        .mapping
        .symbols
        .iter()
        .find(|policy| policy.symbol == symbol_id)
        .and_then(|policy| policy.exception.as_ref())
        .ok_or_else(|| {
            format!(
                "contained callable `{}` has no stable policy",
                function.name
            )
        })?;
    Ok((callable, policy))
}
