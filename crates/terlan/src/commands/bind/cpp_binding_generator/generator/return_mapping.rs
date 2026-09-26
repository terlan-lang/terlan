use super::*;

/// Requires each public result type to match the extracted C++ ownership and
/// container shape before any bridge source is emitted.
pub(super) fn validate_function_return_mapping(
    function: &NativeBindingFunction,
    symbol: &CppSymbol,
    modules: &[NativeBindingModule],
    symbols: &ValidatedCppSymbols<'_>,
) -> Result<(), CppBindingError> {
    let returns = symbol.returns.as_ref();
    let canonical = returns
        .map(|returns| returns.canonical.as_str())
        .unwrap_or("void");
    let compatible =
        resource_tuple_return_resources(modules, &function.returns, symbol, &symbols.declarations)
            .is_some()
            || match function.returns.as_str() {
                "Unit" => {
                    canonical == "void"
                        || function.role == NativeFunctionRole::MutableFreeFunction
                            && returns
                                .and_then(|returns| {
                                    mutable_record_reference_name(returns)
                                        .or_else(|| borrowed_const_record_name(returns))
                                })
                                .is_some()
                        || function.role == NativeFunctionRole::MutableFreeFunction
                            && mutable_resource_tuple_return_matches(
                                function, symbol, modules, symbols,
                            )
                        || function.role == NativeFunctionRole::MutableMethod
                            && symbols
                                .policies
                                .get(symbol.id.as_str())
                                .is_some_and(|policy| policy.exception.is_some())
                            && returns
                                .and_then(|returns| {
                                    mutable_record_reference_name(returns)
                                        .or_else(|| borrowed_const_record_name(returns))
                                })
                                .zip(symbol.receiver.as_deref())
                                .is_some_and(|(returned, receiver)| {
                                    cpp_name_matches(returned, receiver)
                                })
                }
                "Int" => returns.is_some_and(is_i64_type),
                "Float" => canonical == "double",
                "Bool" => canonical == "bool",
                "String" => returns.is_some_and(is_owned_string_type),
                "std.vm.Bytes.Bytes" | "Bytes" => returns.is_some_and(is_owned_u8_vector_type),
                "List[Int]" => returns.is_some_and(is_owned_i64_vector_type),
                "List[Float]" => returns.is_some_and(is_owned_f64_vector_type),
                // Bool lists use an owned byte vector at the CXX boundary;
                // this avoids the specialised `std::vector<bool>` layout
                // while preserving the public boolean-list contract.
                "List[Bool]" => returns.is_some_and(is_owned_u8_vector_type),
                "List[String]" => returns.is_some_and(is_owned_string_vector_type),
                option
                    if option
                        .strip_prefix("Option[")
                        .and_then(|inner| inner.strip_suffix(']'))
                        .and_then(|inner| find_resource_type_in_modules(modules, inner.trim()))
                        .is_some_and(|resource| {
                            symbols
                                .declarations
                                .get(resource.cpp_symbol.as_str())
                                .is_some_and(|resource_symbol| {
                                    owned_unique_ptr_name(
                                        symbol.returns.as_ref().expect("known return"),
                                    )
                                    .is_some_and(|name| {
                                        cpp_name_matches(name, &resource_symbol.cpp_name)
                                    })
                                })
                        }) =>
                {
                    true
                }
                returns => modules.iter().flat_map(|module| &module.types).any(|ty| {
                    if ty.kind != NativeBindingTypeKind::OpaqueResource
                        || !terlan_type_matches(returns, &ty.name)
                    {
                        return false;
                    }
                    let Some(resource_symbol) = symbols.declarations.get(ty.cpp_symbol.as_str())
                    else {
                        return false;
                    };
                    owned_unique_ptr_name(symbol.returns.as_ref().expect("known return"))
                        .is_some_and(|name| cpp_name_matches(name, &resource_symbol.cpp_name))
                        || matches!(
                            function.role,
                            NativeFunctionRole::Constructor
                                | NativeFunctionRole::FreeFunction
                                | NativeFunctionRole::ImmutableMethod
                        ) && cpp_name_matches(canonical, &resource_symbol.cpp_name)
                }),
            };
    if !compatible {
        return Err((format!(
            "error[cpp.type.mapping_mismatch]: function `{}` maps C++ result `{canonical}` to incompatible Terlan type `{}`",
            function.name, function.returns
        )).into());
    }
    if matches!(
        function.resource,
        NativeResourcePolicy::OwnedHandle | NativeResourcePolicy::NullableHandle
    ) && !modules.iter().flat_map(|module| &module.types).any(|ty| {
        if ty.kind != NativeBindingTypeKind::OpaqueResource
            || !terlan_type_matches(&function.returns, &ty.name)
        {
            return false;
        }
        let Some(resource_symbol) = symbols.declarations.get(ty.cpp_symbol.as_str()) else {
            return false;
        };
        returns.is_some_and(|returns| {
            owned_unique_ptr_name(returns)
                .is_some_and(|name| cpp_name_matches(name, &resource_symbol.cpp_name))
                || matches!(
                    function.role,
                    NativeFunctionRole::Constructor
                        | NativeFunctionRole::FreeFunction
                        | NativeFunctionRole::ImmutableMethod
                ) && cpp_name_matches(&returns.canonical, &resource_symbol.cpp_name)
        })
    }) {
        return Err((format!(
            "error[cpp.ownership.handle_result]: function `{}` classifies `{canonical}` as `{}` but must return a package-owned resource by value or std::unique_ptr",
            function.name,
            resource_policy_name(&function.resource)
        )).into());
    }
    Ok(())
}

/// Proves a discarded C++ tuple result aliases every retained mutable resource
/// in public argument order. Generated mutation adapters never expose the alias
/// tuple, but this evidence prevents unrelated by-value tuples from being
/// mislabeled as `Unit`.
pub(super) fn mutable_resource_tuple_return_matches(
    function: &NativeBindingFunction,
    symbol: &CppSymbol,
    modules: &[NativeBindingModule],
    symbols: &ValidatedCppSymbols<'_>,
) -> bool {
    let Some(elements) = symbol
        .returns
        .as_ref()
        .and_then(|returns| cpp_std_tuple_elements(&returns.canonical))
    else {
        return false;
    };
    let mutable_arguments = function
        .args
        .iter()
        .enumerate()
        .filter(|(index, argument)| *index == 0 || argument.mutable)
        .collect::<Vec<_>>();
    if elements.len() != mutable_arguments.len() {
        return false;
    }
    elements
        .into_iter()
        .zip(mutable_arguments)
        .all(|(element, (_, argument))| {
            let canonical = element
                .trim()
                .strip_prefix("const ")
                .unwrap_or(element.trim())
                .trim_end_matches('&')
                .trim();
            find_resource_type_in_modules(modules, &argument.ty)
                .and_then(|resource| symbols.declarations.get(resource.cpp_symbol.as_str()))
                .is_some_and(|resource| {
                    cpp_name_matches(canonical, &resource.cpp_name)
                        || cpp_name_matches(canonical, &resource.overload_set)
                })
        })
}

pub(super) fn is_supported_cxx_type(cpp_type: &CppTypeMetadata) -> bool {
    let canonical = cpp_type.canonical.as_str();
    canonical == "void"
        || is_i64_type(cpp_type)
        || canonical == "double"
        || canonical == "bool"
        || is_rust_str_type(cpp_type)
        || is_cpp_string_view_type(cpp_type)
        || is_u8_slice_type(cpp_type)
        || is_i64_slice_type(cpp_type)
        || is_f64_slice_type(cpp_type)
        || borrowed_const_record_name(cpp_type).is_some()
        || cpp_array_ref_element(cpp_type).is_some()
        || canonical
            .strip_prefix("std::unique_ptr<")
            .and_then(|value| value.strip_suffix('>'))
            .is_some_and(|inner| !inner.trim().is_empty())
}

/// Returns the element name from a supported immutable ATen/c10 list view.
pub(super) fn cpp_array_ref_element(cpp_type: &CppTypeMetadata) -> Option<&str> {
    if cpp_type.pointer_depth != 0 {
        return None;
    }
    let canonical = cpp_type.canonical.trim();
    let list = if cpp_type.reference == CppReferenceKind::None {
        canonical
    } else if cpp_type.reference == CppReferenceKind::Lvalue && cpp_type.is_const {
        canonical.strip_prefix("const ")?.strip_suffix('&')?.trim()
    } else {
        return None;
    };
    list.strip_prefix("c10::ArrayRef<")
        .or_else(|| list.strip_prefix("at::ArrayRef<"))
        .or_else(|| list.strip_prefix("c10::IListRef<"))
        .or_else(|| list.strip_prefix("at::IListRef<"))
        .and_then(|value| value.strip_suffix('>'))
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// Recognizes an immutable integer view constructible from `(data, size)`.
/// Clang canonicalizes ATen's `IntArrayRef` alias to `c10::ArrayRef<long>`.
pub(super) fn is_i64_array_ref_type(cpp_type: &CppTypeMetadata) -> bool {
    if cpp_type.pointer_depth != 0 || cpp_type.reference != CppReferenceKind::None {
        return false;
    }
    let spelling = compact_cpp_spelling(&cpp_type.spelling);
    let canonical = compact_cpp_spelling(&cpp_type.canonical);
    if canonical.contains("OptionalArrayRef<") {
        return false;
    }
    matches!(
        spelling.as_str(),
        "at::IntArrayRef" | "c10::IntArrayRef" | "c10::ArrayRef<std::int64_t>"
    ) || ["long", "longlong", "std::int64_t", "int64_t"]
        .iter()
        .any(|element| canonical.ends_with(&format!("ArrayRef<{element}>")))
}

/// Recognizes c10's optional immutable integer view. CXX cannot bridge this
/// template directly, so generated adapters receive a Rust slice and construct
/// the exact optional ArrayRef on the C++ side.
pub(super) fn is_optional_i64_array_ref_type(cpp_type: &CppTypeMetadata) -> bool {
    if cpp_type.pointer_depth != 0 || cpp_type.reference != CppReferenceKind::None {
        return false;
    }
    let spelling = compact_cpp_spelling(&cpp_type.spelling);
    let canonical = compact_cpp_spelling(&cpp_type.canonical);
    matches!(
        spelling.as_str(),
        "at::OptionalIntArrayRef" | "c10::OptionalIntArrayRef" | "OptionalIntArrayRef"
    ) || ["long", "longlong", "std::int64_t", "int64_t"]
        .iter()
        .any(|element| canonical.ends_with(&format!("OptionalArrayRef<{element}>")))
}

/// Recognizes ATen/c10's dynamically typed numeric scalar value.
pub(super) fn is_cpp_scalar_type(cpp_type: &CppTypeMetadata) -> bool {
    let canonical = compact_cpp_spelling(&cpp_type.canonical);
    matches!(
        canonical.as_str(),
        "c10::Scalar" | "constc10::Scalar&" | "at::Scalar" | "constat::Scalar&"
    )
}

/// Recognizes an optional ATen/c10 dynamically typed numeric scalar.
///
/// Historical package surfaces commonly expose separate `Int` and `Float`
/// overloads while generated ATen declarations receive
/// `std::optional<Scalar>`. The generated adapter constructs the present
/// optional value; an absent public argument continues to lower to
/// `std::nullopt` through the ordinary optional-default path.
pub(super) fn is_optional_cpp_scalar_type(cpp_type: &CppTypeMetadata) -> bool {
    optional_cpp_inner_type(cpp_type).is_some_and(|inner| {
        matches!(
            compact_cpp_spelling(inner).as_str(),
            "c10::Scalar" | "at::Scalar"
        )
    })
}

/// Returns the type wrapped by a by-value or immutable-reference
/// `std::optional<T>` declaration. CXX adapters copy the optional's contained
/// value before entering the selected upstream callable.
pub(super) fn optional_cpp_inner_type(cpp_type: &CppTypeMetadata) -> Option<&str> {
    if cpp_type.pointer_depth != 0 {
        return None;
    }
    let canonical = match cpp_type.reference {
        CppReferenceKind::None => cpp_type.canonical.trim(),
        CppReferenceKind::Lvalue if cpp_type.is_const => cpp_type
            .canonical
            .trim()
            .strip_prefix("const ")?
            .strip_suffix('&')?
            .trim(),
        _ => return None,
    };
    canonical
        .strip_prefix("std::optional<")
        .and_then(|value| value.strip_suffix('>'))
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// One extracted parameter's aligned public representation.
pub(super) struct PublicCppParameterMapping<'a> {
    /// Index of the corresponding parameter in the extracted C++ declaration.
    pub(super) cpp_parameter_index: usize,
    /// Public scalar type used to select a bridge-safe lowering.
    pub(super) public_type: Option<&'a str>,
    /// Optional public presence flag for `(has_value, value)` conventions.
    pub(super) presence_name: Option<&'a str>,
    /// Index of the first corresponding argument in the complete public call.
    pub(super) public_argument_index: usize,
    /// Optional tagged integer/floating representation of one C++ Scalar.
    pub(super) scalar_choice: Option<PublicScalarChoice<'a>>,
}

/// Public fields used to construct one exact C++ numeric Scalar.
pub(super) struct PublicScalarChoice<'a> {
    pub(super) tag_name: &'a str,
    pub(super) integer_name: &'a str,
    pub(super) floating_name: &'a str,
}
