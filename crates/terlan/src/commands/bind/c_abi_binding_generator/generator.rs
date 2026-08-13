mod adapter_rendering;
mod binding_validation;
mod consumer_output;
mod input_validation;
mod safe_wrapper_rendering;
mod shape_validation;
mod worker_rendering;

use adapter_rendering::*;
use binding_validation::*;
use consumer_output::*;
use input_validation::*;
use safe_wrapper_rendering::*;
use shape_validation::*;
use worker_rendering::*;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

const C_ABI_BINDING_SCHEMA: &str = "terlan.c-abi.binding.v1";
const C_METADATA_SCHEMA: &str = "terlan.c.metadata.v1";
const SKIPPED_SYMBOLS_SCHEMA: &str = "terlan.c-abi.binding.skipped-symbols.v1";
const CC_VERSION: &str = "1.2.67";
const GETRANDOM_VERSION: &str = "0.3.4";

#[derive(Debug, Serialize, Deserialize)]
struct CAbiBindingManifest {
    schema: String,
    package: CAbiBindingPackage,
    validation: CAbiValidationContract,
    c_metadata: CMetadata,
    modules: Vec<CAbiBindingModule>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CAbiValidationContract {
    deterministic_regeneration: bool,
    warning_denied_build: bool,
    ownership_lifecycle: bool,
    error_translation: bool,
    smoke: CAbiSmokeValidation,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum CAbiSmokeValidation {
    GeneratedFixture,
    PackageOwnedLive,
}

#[derive(Debug, Serialize, Deserialize)]
struct CAbiBindingPackage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    namespace: String,
    adapter: String,
    crate_name: String,
    #[serde(default, skip_serializing_if = "is_false")]
    workspace_member: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    rust_extension: Option<CAbiRustExtension>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CAbiRustExtension {
    source: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    support_sources: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    dependencies: BTreeMap<String, String>,
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Serialize, Deserialize)]
struct CMetadata {
    schema: String,
    producer: CMetadataProducer,
    abi_version: u32,
    header: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    headers: Vec<String>,
    #[serde(default)]
    sources: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cpp_standard: Option<String>,
    #[serde(default)]
    aliases: BTreeMap<String, String>,
    #[serde(default)]
    external_link: Option<CExternalLink>,
    symbols: Vec<CSymbol>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CExternalLink {
    #[serde(default)]
    root_env: Option<String>,
    #[serde(default)]
    pkg_config: Option<CPkgConfigLink>,
    #[serde(default)]
    include_dirs: Vec<String>,
    #[serde(default)]
    library_dirs: Vec<String>,
    #[serde(default)]
    libraries: Vec<String>,
    #[serde(default)]
    runtime_library_dirs: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CPkgConfigLink {
    package: String,
    #[serde(default)]
    min_version: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    static_link: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct CMetadataProducer {
    name: String,
    version: String,
    format: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct CSymbol {
    id: String,
    c_name: String,
    kind: CSymbolKind,
    status: CSymbolStatus,
    #[serde(default)]
    ownership: Option<String>,
    #[serde(default)]
    destructor_symbol: Option<String>,
    #[serde(default)]
    thread_safety: Option<String>,
    #[serde(default)]
    returns: Option<String>,
    #[serde(default)]
    error_model: Option<CErrorModel>,
    #[serde(default)]
    success_code: Option<i32>,
    #[serde(default)]
    parameters: Vec<CParameter>,
    #[serde(default)]
    variadic: bool,
    #[serde(default)]
    callback: bool,
    #[serde(default)]
    unsupported_shape: Option<UnsupportedCShape>,
    #[serde(default)]
    detail: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum CSymbolKind {
    OpaqueStruct,
    Function,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum CSymbolStatus {
    Bind,
    Unsupported,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum CErrorModel {
    Infallible,
    StatusCode,
}

#[derive(Debug, Serialize, Deserialize)]
struct CParameter {
    name: String,
    c_type: String,
    #[serde(default)]
    direction: Option<CParameterDirection>,
    #[serde(default)]
    ownership: Option<CParameterOwnership>,
    #[serde(default)]
    borrowed_array: Option<CBorrowedArray>,
    #[serde(default)]
    input_array: Option<CInputArray>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    owned_string: Option<COwnedStringOutput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    owned_array: Option<COwnedArrayOutput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    owned_string_array: Option<COwnedStringArrayOutput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    owned_handle_array: Option<COwnedHandleArrayOutput>,
    #[serde(default)]
    fixed: Option<CFixedInput>,
}

#[derive(Debug, Serialize, Deserialize)]
struct COwnedStringOutput {
    length_parameter: String,
    destructor_symbol: String,
    copy: COwnedStringCopy,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum COwnedStringCopy {
    ImmediateUtf8,
}

#[derive(Debug, Serialize, Deserialize)]
struct COwnedArrayOutput {
    length_parameter: String,
    destructor_symbol: String,
    copy: COwnedArrayCopy,
    #[serde(default)]
    element: COwnedArrayElement,
}

#[derive(Debug, Serialize, Deserialize)]
struct COwnedStringArrayOutput {
    lengths_parameter: String,
    count_parameter: String,
    destructor_symbol: String,
    copy: COwnedStringCopy,
}

#[derive(Debug, Serialize, Deserialize)]
struct COwnedHandleArrayOutput {
    length_parameter: String,
    destructor_symbol: String,
    element_type: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum COwnedArrayCopy {
    Immediate,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum COwnedArrayElement {
    #[default]
    Int64,
    Float64,
    Bool8,
    Bytes,
}

impl COwnedArrayElement {
    fn c_pointer(self) -> &'static str {
        match self {
            Self::Int64 => "int64_t *",
            Self::Float64 => "double *",
            Self::Bool8 => "uint8_t *",
            Self::Bytes => "uint8_t *",
        }
    }

    fn c_output_pointer(self) -> &'static str {
        match self {
            Self::Int64 => "int64_t **",
            Self::Float64 => "double **",
            Self::Bool8 => "uint8_t **",
            Self::Bytes => "uint8_t **",
        }
    }

    fn rust_element(self) -> &'static str {
        match self {
            Self::Int64 => "i64",
            Self::Float64 => "f64",
            Self::Bool8 => "u8",
            Self::Bytes => "u8",
        }
    }

    fn terlan_list(self) -> &'static str {
        match self {
            Self::Int64 => "List[Int]",
            Self::Float64 => "List[Float]",
            Self::Bool8 => "List[Bool]",
            Self::Bytes => "Bytes",
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct CInputArray {
    length_parameter: String,
    #[serde(default)]
    bytes: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum CFixedInput {
    Null,
    Int32 { value: i32 },
}

#[derive(Debug, Serialize, Deserialize)]
struct CBorrowedArray {
    owner_parameter: String,
    length_symbol: String,
    copy: CBorrowedArrayCopy,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum CBorrowedArrayCopy {
    Immediate,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum CParameterDirection {
    Input,
    Output,
    InOut,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum CParameterOwnership {
    Value,
    BorrowedCall,
    TransferFull,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
enum UnsupportedCShape {
    PointerOwnershipUnknown,
    BorrowedLifetime,
    MissingDestructor,
    UnsupportedCallback,
    UnsupportedVariadicFunction,
    UnsupportedUnion,
    UnsupportedBitfield,
    AbiVersionMissing,
    ThreadLocalError,
}

#[derive(Debug, Serialize, Deserialize)]
struct CAbiBindingModule {
    module: String,
    documentation: String,
    #[serde(default)]
    types: Vec<CAbiBindingType>,
    #[serde(default)]
    functions: Vec<CAbiBindingFunction>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CAbiBindingType {
    name: String,
    c_symbol: String,
    documentation: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct CAbiBindingFunction {
    name: String,
    operation: String,
    c_symbol: String,
    role: CAbiFunctionRole,
    #[serde(default)]
    args: Vec<CAbiBindingArg>,
    returns: String,
    blocking: CAbiBlockingPolicy,
    resource: CAbiResourcePolicy,
    documentation: String,
    #[serde(default)]
    dispatcher: Option<CDispatcherBinding>,
    #[serde(default)]
    generated_smoke: CGeneratedSmokePolicy,
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum CGeneratedSmokePolicy {
    #[default]
    Execute,
    PackageOwned,
}

#[derive(Debug, Serialize, Deserialize)]
struct CDispatcherBinding {
    duplicate_handle_symbol: String,
    #[serde(default)]
    optional_value_allocator_symbol: Option<String>,
    #[serde(default)]
    optional_value_destructor_symbol: Option<String>,
    #[serde(default)]
    list_allocator_symbol: Option<String>,
    #[serde(default)]
    list_push_symbol: Option<String>,
    #[serde(default)]
    list_destructor_symbol: Option<String>,
    #[serde(default)]
    string_allocator_symbol: Option<String>,
    #[serde(default)]
    string_destructor_symbol: Option<String>,
    operator_name: String,
    overload_name: String,
    extension_abi_version: String,
    stack: Vec<CDispatcherStackValue>,
    output: CDispatcherOutput,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum CDispatcherStackValue {
    OwnedHandleCopy {
        argument: String,
    },
    OwnedOptionalHandleCopy {
        argument: String,
    },
    IntArgument {
        argument: String,
    },
    FloatArgument {
        argument: String,
    },
    BoolArgument {
        argument: String,
    },
    OwnedOptionalIntArgument {
        argument: String,
    },
    OwnedIntListArgument {
        argument: String,
    },
    OwnedOptionalIntListArgument {
        argument: String,
    },
    OwnedStringLiteral {
        value: String,
    },
    Null,
    #[serde(other)]
    Unsupported,
}

#[derive(Debug, Serialize, Deserialize)]
struct CDispatcherOutput {
    kind: CDispatcherOutputKind,
    index: usize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum CDispatcherOutputKind {
    OwnedHandle,
}

#[derive(Debug, Serialize, Deserialize)]
struct CAbiBindingArg {
    name: String,
    ty: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum CAbiFunctionRole {
    Constructor,
    ImmutableMethod,
    MutableMethod,
    FreeFunction,
    Dispose,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum CAbiBlockingPolicy {
    Fast,
    Blocking,
    Async,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum CAbiResourcePolicy {
    Value,
    OpaqueHandle,
    BorrowedHandle,
    MutableHandle,
    DisposeHandle,
    TransferableHandle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::commands::bind) struct CAbiBindingGenerationSummary {
    pub(in crate::commands::bind) module_count: usize,
    pub(in crate::commands::bind) function_count: usize,
    pub(in crate::commands::bind) skipped_symbol_count: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, PartialOrd, Ord)]
struct SkippedCSymbol {
    id: String,
    symbol: String,
    reason: String,
    detail: String,
}

#[derive(Serialize)]
struct SkippedCSymbolsManifest<'a> {
    schema: &'static str,
    metadata_producer: &'a CMetadataProducer,
    abi_version: u32,
    skipped: &'a [SkippedCSymbol],
}

/// Generates an executable package from normalized C declaration metadata.
/// The compiler consumes metadata and copies declared C inputs; it does not
/// parse C headers or allow raw pointers into the Terlan surface.
pub(in crate::commands::bind) fn generate_c_abi_bindings(
    manifest_path: &Path,
    out_dir: &Path,
) -> Result<CAbiBindingGenerationSummary, String> {
    refuse_non_empty_output(out_dir)?;
    let manifest_text = fs::read_to_string(manifest_path).map_err(|error| {
        format!(
            "failed to read C ABI binding manifest `{}`: {error}",
            manifest_path.display()
        )
    })?;
    let manifest: CAbiBindingManifest = serde_json::from_str(&manifest_text).map_err(|error| {
        format!(
            "failed to parse structured C metadata `{}`: {error}",
            manifest_path.display()
        )
    })?;
    let input_dir = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    let symbols = validate_manifest(&manifest, input_dir)?;
    let skipped = collect_skipped_symbols(&manifest.c_metadata.symbols)?;

    fs::create_dir_all(out_dir).map_err(|error| {
        format!(
            "failed to create output directory `{}`: {error}",
            out_dir.display()
        )
    })?;

    for module in &manifest.modules {
        let rendered_module_source = render_module_source(&manifest, module);
        let formatted_module_source = crate::terlan_syntax::format_source_module(
            &rendered_module_source,
        )
        .map_err(|error| {
            format!(
                "failed to validate generated Terlan module `{}`: {}",
                module.module, error.message
            )
        })?;
        let module_source = if module.functions.len() > 64 {
            rendered_module_source
        } else {
            formatted_module_source
        };
        write_file(
            &out_dir.join(module_source_path(&module.module)),
            &module_source,
        )?;
        write_file(
            &out_dir.join(module_docs_path(&module.module)),
            &render_module_docs(module, &symbols),
        )?;
    }

    copy_c_inputs(&manifest.c_metadata, input_dir, out_dir)?;
    copy_rust_extension(&manifest.package, input_dir, out_dir)?;
    write_file(
        &out_dir.join("terlan.toml"),
        &render_terlan_manifest(&manifest),
    )?;
    write_file(
        &out_dir.join("native/terlan-native.toml"),
        &render_native_boundary_metadata(&manifest)?,
    )?;
    write_file(
        &out_dir.join("native/rust/Cargo.toml"),
        &render_rust_adapter_cargo(&manifest),
    )?;
    write_file(
        &out_dir.join("native/rust/build.rs"),
        &render_c_build(&manifest.c_metadata),
    )?;
    let rust_adapter = render_rust_ffi_and_adapter(&manifest, &symbols)?;
    write_file(&out_dir.join("native/rust/src/lib.rs"), &rust_adapter.root)?;
    for (index, chunk) in rust_adapter.chunks.iter().enumerate() {
        write_file(
            &out_dir.join(format!("native/rust/src/generated_adapter_{index}.rs")),
            chunk,
        )?;
    }
    let native_helper = render_native_helper(&manifest, &symbols)?;
    write_file(
        &out_dir.join("native/rust/src/bin/native_boundary_helper.rs"),
        &native_helper.root,
    )?;
    for (index, chunk) in native_helper.dispatch_chunks.iter().enumerate() {
        write_file(
            &out_dir.join(format!(
                "native/rust/src/bin/native_boundary_helper/dispatch_{index}.rs"
            )),
            chunk,
        )?;
    }
    write_file(
        &out_dir.join(consumer_test_path(&manifest.package.namespace)),
        &render_consumer_test(&manifest)?,
    )?;
    let normalized_manifest = serde_json::to_string_pretty(&manifest)
        .map_err(|error| format!("failed to normalize C ABI binding manifest: {error}"))?;
    write_file(
        &out_dir.join("bindings/native-binding-manifest.json"),
        &(normalized_manifest + "\n"),
    )?;
    write_file(
        &out_dir.join("bindings/skipped-symbols.json"),
        &render_skipped_symbols(&manifest.c_metadata, &skipped)?,
    )?;

    Ok(CAbiBindingGenerationSummary {
        module_count: manifest.modules.len(),
        function_count: manifest
            .modules
            .iter()
            .map(|module| module.functions.len())
            .sum(),
        skipped_symbol_count: skipped.len(),
    })
}

fn validate_manifest<'a>(
    manifest: &'a CAbiBindingManifest,
    input_dir: &Path,
) -> Result<BTreeMap<&'a str, &'a CSymbol>, String> {
    if manifest.schema != C_ABI_BINDING_SCHEMA {
        return Err(format!(
            "unsupported C ABI binding schema `{}`; expected `{C_ABI_BINDING_SCHEMA}`",
            manifest.schema
        ));
    }
    if manifest.package.adapter != "c-abi" {
        return Err(format!(
            "unsupported C ABI binding adapter `{}`; expected `c-abi`",
            manifest.package.adapter
        ));
    }
    validate_identifier_path("package namespace", &manifest.package.namespace)?;
    if manifest.package.namespace.starts_with("std.native") {
        return Err("generated external packages cannot use the `std.native` namespace".into());
    }
    validate_cargo_package_name(&manifest.package.crate_name)?;
    if let Some(name) = &manifest.package.name {
        validate_cargo_package_name(name)?;
    }
    if let Some(version) = &manifest.package.version {
        if !is_pinned_cargo_version(version) {
            return Err(format!(
                "C ABI binding package version must use exact x.y.z form; found `{version}`"
            ));
        }
    }
    validate_rust_extension(&manifest.package, input_dir)?;
    validate_c_abi_contract(&manifest.validation)?;

    let metadata = &manifest.c_metadata;
    if metadata.schema != C_METADATA_SCHEMA {
        return Err(format!(
            "unsupported structured C metadata schema `{}`; expected `{C_METADATA_SCHEMA}`",
            metadata.schema
        ));
    }
    let producer_format = match metadata.producer.name.as_str() {
        "clang-libtooling" => "normalized-ast-json",
        "terlan-curated-metadata" => "reviewed-declaration-json",
        producer => {
            return Err(format!(
                "unsupported C metadata producer `{producer}`; expected maintained tooling `clang-libtooling` or `terlan-curated-metadata`"
            ));
        }
    };
    if metadata.producer.version.trim().is_empty() || metadata.producer.format != producer_format {
        return Err(format!(
            "structured C metadata from `{}` requires a producer version and `{producer_format}` format",
            metadata.producer.name
        ));
    }
    if metadata.abi_version == 0 {
        return Err("error[native_bindgen.c_abi_version_missing]: C metadata requires a positive ABI version".into());
    }
    validate_c_inputs(metadata, input_dir)?;
    if let Some(standard) = &metadata.cpp_standard {
        if !matches!(standard.as_str(), "c++17" | "c++20") {
            return Err(format!(
                "C ABI package C++ standard must be `c++17` or `c++20`; found `{standard}`"
            ));
        }
        if !metadata
            .sources
            .iter()
            .any(|source| is_cpp_adapter_source(source))
        {
            return Err("C ABI package C++ standard requires a declared C++ source".into());
        }
    }
    validate_c_aliases(metadata)?;

    let mut symbols = BTreeMap::new();
    for symbol in &metadata.symbols {
        if symbols.insert(symbol.id.as_str(), symbol).is_some() {
            return Err(format!("duplicate structured C symbol id `{}`", symbol.id));
        }
        validate_c_symbol(symbol, &metadata.aliases)?;
    }
    if symbols.is_empty() {
        return Err("structured C metadata contains no symbols".into());
    }
    for symbol in symbols.values().filter(|symbol| {
        symbol.status == CSymbolStatus::Bind && symbol.kind == CSymbolKind::OpaqueStruct
    }) {
        let destructor_id = symbol
            .destructor_symbol
            .as_deref()
            .ok_or_else(|| stable_shape_error(symbol, UnsupportedCShape::MissingDestructor))?;
        let destructor = symbols.get(destructor_id).ok_or_else(|| {
            format!(
                "opaque C symbol `{}` references unknown destructor `{destructor_id}`",
                symbol.id
            )
        })?;
        if destructor.status != CSymbolStatus::Bind || destructor.kind != CSymbolKind::Function {
            return Err(format!(
                "opaque C symbol `{}` requires a bindable destructor function",
                symbol.id
            ));
        }
    }
    validate_c_type_references(&symbols, &metadata.aliases)?;
    validate_borrowed_arrays(&symbols, &metadata.aliases)?;
    validate_owned_strings(&symbols, &metadata.aliases)?;
    validate_owned_arrays(&symbols, &metadata.aliases)?;
    validate_owned_string_arrays(&symbols, &metadata.aliases)?;
    validate_owned_handle_arrays(&symbols, &metadata.aliases)?;

    if manifest.modules.is_empty() {
        return Err("C ABI binding manifest must declare at least one module".into());
    }
    let mut operations = BTreeSet::new();
    for module in &manifest.modules {
        validate_identifier_path("module", &module.module)?;
        if module.documentation.trim().is_empty() {
            return Err(format!(
                "module `{}` documentation cannot be empty",
                module.module
            ));
        }
        for ty in &module.types {
            validate_upper_identifier("type", &ty.name)?;
            let symbol = symbols.get(ty.c_symbol.as_str()).ok_or_else(|| {
                format!(
                    "type `{}` references unknown C symbol `{}`",
                    ty.name, ty.c_symbol
                )
            })?;
            if symbol.status != CSymbolStatus::Bind || symbol.kind != CSymbolKind::OpaqueStruct {
                return Err(format!(
                    "type `{}` must reference a bindable opaque C struct",
                    ty.name
                ));
            }
        }
        for function in &module.functions {
            validate_lower_identifier("function", &function.name)?;
            validate_identifier_path("native operation", &function.operation)?;
            if !operations.insert(function.operation.as_str()) {
                return Err(format!(
                    "duplicate native operation `{}`",
                    function.operation
                ));
            }
            if function.documentation.trim().is_empty() {
                return Err(format!(
                    "function `{}` documentation cannot be empty",
                    function.name
                ));
            }
            for argument in &function.args {
                validate_lower_identifier("argument", &argument.name)?;
                reject_terlan_pointer_or_reference(&function.name, &argument.ty)?;
            }
            reject_terlan_pointer_or_reference(&function.name, &function.returns)?;
            let symbol = symbols.get(function.c_symbol.as_str()).ok_or_else(|| {
                format!(
                    "function `{}` references unknown C symbol `{}`",
                    function.name, function.c_symbol
                )
            })?;
            if symbol.status != CSymbolStatus::Bind || symbol.kind != CSymbolKind::Function {
                return Err(format!(
                    "function `{}` requires a bindable C function",
                    function.name
                ));
            }
            if let Some(dispatcher) = &function.dispatcher {
                validate_dispatcher_binding(
                    function,
                    dispatcher,
                    symbol,
                    &symbols,
                    &metadata.aliases,
                )?;
            } else {
                validate_input_array_binding(function, symbol, &metadata.aliases)?;
                validate_owned_string_binding(function, symbol)?;
                validate_owned_array_binding(function, symbol)?;
                validate_owned_string_array_binding(function, symbol)?;
                validate_owned_handle_array_binding(manifest, function, symbol)?;
            }
        }
    }
    validate_binding_roles(manifest)?;
    for (_, bound_type) in binding_types(manifest) {
        let record = symbols
            .get(bound_type.c_symbol.as_str())
            .ok_or_else(|| format!("unknown C record `{}`", bound_type.c_symbol))?;
        let dispose = dispose_for_type(manifest, &bound_type.name)?;
        if record.destructor_symbol.as_deref() != Some(dispose.c_symbol.as_str()) {
            return Err(format!(
                "dispose function `{}` for `{}` must reference destructor `{}`",
                dispose.name,
                bound_type.name,
                record.destructor_symbol.as_deref().unwrap_or("")
            ));
        }
    }
    Ok(symbols)
}

fn validate_c_abi_contract(contract: &CAbiValidationContract) -> Result<(), String> {
    for (name, enabled) in [
        (
            "deterministic_regeneration",
            contract.deterministic_regeneration,
        ),
        ("warning_denied_build", contract.warning_denied_build),
        ("ownership_lifecycle", contract.ownership_lifecycle),
        ("error_translation", contract.error_translation),
    ] {
        if !enabled {
            return Err(format!(
                "error[native_bindgen.c_abi_validation]: C ABI validation obligation `{name}` must be enabled"
            ));
        }
    }
    match contract.smoke {
        CAbiSmokeValidation::GeneratedFixture | CAbiSmokeValidation::PackageOwnedLive => Ok(()),
    }
}
