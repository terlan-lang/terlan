mod adapter_rendering;
mod argument_mutability;
mod binding_validation;
mod consumer_output;
mod function_family;
mod input_validation;
mod safe_wrapper_rendering;
mod shape_validation;
mod worker_rendering;

use adapter_rendering::*;
use argument_mutability::*;
use binding_validation::*;
use consumer_output::*;
use function_family::*;
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
const CC_VERSION: &str = "1.5.1";
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
    /// Maps generated module names to package-authored Terlan declarations.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    terlan_module_extensions: BTreeMap<String, String>,
    /// Package-level Terlan dependencies emitted into the generated manifest.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    terlan_dependencies: BTreeMap<String, CAbiTerlanDependency>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum CAbiTerlanDependency {
    Path { path: String },
    Git { git: String, rev: String },
    Registry { registry: String, version: String },
}

#[derive(Debug, Serialize, Deserialize)]
struct CAbiRustExtension {
    source: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    support_sources: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    dependencies: BTreeMap<String, CAbiRustDependency>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum CAbiRustDependency {
    Version(String),
    Detailed {
        version: String,
        #[serde(default)]
        features: Vec<String>,
        #[serde(default = "default_true")]
        default_features: bool,
    },
}

fn default_true() -> bool {
    true
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
    #[serde(rename = "uint64")]
    UInt64,
    Float64,
    Bool8,
    Bytes,
}

impl COwnedArrayElement {
    fn c_pointer(self) -> &'static str {
        match self {
            Self::Int64 => "int64_t *",
            Self::UInt64 => "uint64_t *",
            Self::Float64 => "double *",
            Self::Bool8 => "uint8_t *",
            Self::Bytes => "uint8_t *",
        }
    }

    fn c_output_pointer(self) -> &'static str {
        match self {
            Self::Int64 => "int64_t **",
            Self::UInt64 => "uint64_t **",
            Self::Float64 => "double **",
            Self::Bool8 => "uint8_t **",
            Self::Bytes => "uint8_t **",
        }
    }

    fn rust_element(self) -> &'static str {
        match self {
            Self::Int64 => "i64",
            Self::UInt64 => "u64",
            Self::Float64 => "f64",
            Self::Bool8 => "u8",
            Self::Bytes => "u8",
        }
    }

    fn terlan_list(self) -> &'static str {
        match self {
            Self::Int64 => "List[Int]",
            Self::UInt64 => "C/Rust-only UInt64",
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    element_type: Option<String>,
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
    imports: Vec<CAbiBindingImport>,
    #[serde(default)]
    types: Vec<CAbiBindingType>,
    #[serde(default)]
    functions: Vec<CAbiBindingFunction>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    function_families: Vec<CAbiBindingFunctionFamily>,
}

/// One explicit selective import required by generated module extensions.
#[derive(Debug, Serialize, Deserialize)]
struct CAbiBindingImport {
    /// Dotted Terlan module path that owns the imported declarations.
    module: String,
    /// Declaration names merged into the generated selective import.
    names: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CAbiBindingType {
    name: String,
    c_symbol: String,
    documentation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CAbiBindingFunction {
    name: String,
    #[serde(default)]
    adapter_name: Option<String>,
    #[serde(default)]
    visibility: CAbiTerlanVisibility,
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

/// Source-level visibility of one generated Terlan native declaration.
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum CAbiTerlanVisibility {
    /// Exposes the declaration as package API.
    #[default]
    Public,
    /// Keeps the declaration callable only from its generated module.
    Private,
}

impl CAbiBindingFunction {
    /// Returns whether this declaration belongs to the public Terlan API.
    fn is_public(&self) -> bool {
        self.visibility == CAbiTerlanVisibility::Public
    }
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum CGeneratedSmokePolicy {
    #[default]
    Execute,
    PackageOwned,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    OwnedOptionalFloatArgument {
        argument: String,
    },
    OwnedIntListArgument {
        argument: String,
    },
    OwnedOptionalIntListArgument {
        argument: String,
    },
    OwnedHandleListArgument {
        argument: String,
    },
    OwnedStringArgument {
        argument: String,
    },
    OwnedOptionalStringArgument {
        argument: String,
    },
    OwnedStringLiteral {
        value: String,
    },
    Null,
    #[serde(other)]
    Unsupported,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum CDispatcherOutput {
    OwnedHandle { index: usize },
    OwnedHandleTuple { indices: Vec<usize> },
    DiscardOwnedHandle { index: usize },
    DiscardOwnedHandleTuple { indices: Vec<usize> },
}

impl CDispatcherOutput {
    /// Returns the StableIValue stack slots transferred into the public result.
    fn indices(&self) -> Vec<usize> {
        match self {
            Self::OwnedHandle { index } | Self::DiscardOwnedHandle { index } => vec![*index],
            Self::OwnedHandleTuple { indices } | Self::DiscardOwnedHandleTuple { indices } => {
                indices.clone()
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CAbiBindingArg {
    name: String,
    ty: String,
    #[serde(default)]
    abi_ty: Option<String>,
    #[serde(default)]
    default: Option<String>,
    /// Borrows this non-receiver opaque resource mutably in generated Rust.
    #[serde(default, skip_serializing_if = "is_false")]
    mutable: bool,
}

impl CAbiBindingArg {
    /// Returns the concrete boundary representation for this public argument.
    fn abi_ty(&self) -> &str {
        self.abi_ty.as_deref().unwrap_or(&self.ty)
    }
}

impl CAbiBindingFunction {
    /// Returns the Rust adapter identifier used behind the public Terlan name.
    fn adapter_name(&self) -> &str {
        self.adapter_name.as_deref().unwrap_or(&self.name)
    }
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum CAbiBlockingPolicy {
    Fast,
    Blocking,
    Async,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
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
    let mut manifest: CAbiBindingManifest =
        serde_json::from_str(&manifest_text).map_err(|error| {
            format!(
                "failed to parse structured C metadata `{}`: {error}",
                manifest_path.display()
            )
        })?;
    expand_function_families(&mut manifest)?;
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
        let mut rendered_module_source = render_module_source(&manifest, module);
        append_terlan_module_extension(
            &mut rendered_module_source,
            &manifest.package,
            &module.module,
            input_dir,
        )?;
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
    for (index, chunk) in rust_adapter.ffi_chunks.iter().enumerate() {
        write_file(
            &out_dir.join(format!("native/rust/src/generated_ffi_{index}.rs")),
            chunk,
        )?;
    }
    for (index, chunk) in rust_adapter.owned_chunks.iter().enumerate() {
        write_file(
            &out_dir.join(format!("native/rust/src/generated_adapter_{index}.rs")),
            chunk,
        )?;
    }
    for (index, chunk) in rust_adapter.free_chunks.iter().enumerate() {
        write_file(
            &out_dir.join(format!("native/rust/src/generated_free_adapter_{index}.rs")),
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

#[path = "generator/manifest_validation.rs"]
mod manifest_validation;
use manifest_validation::validate_manifest;
