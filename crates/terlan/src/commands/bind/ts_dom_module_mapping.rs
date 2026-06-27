use super::ts_parser_adapter::{
    TsDeclaration, TsDeclarationFile, TsInterfaceDeclaration, TsInterfaceMember,
    TsMethodDeclaration, TsParameterDeclaration, TsPropertyDeclaration, TsUnsupportedDeclaration,
    TsUnsupportedMember,
};
use super::ts_type_mapping::{map_ts_type_to_terlan, TsTypeMapping, TsTypeSkip};
use crate::terlan_hir::source_name_to_terlan_identifier;

/// DOM binding module mapping result.
///
/// Inputs:
/// - Produced from the neutral TypeScript declaration file model.
///
/// Output:
/// - Planned DOM modules plus stable skipped-declaration diagnostics.
///
/// Transformation:
/// - Converts TypeScript interfaces into deterministic `std.js.Dom.*` module
///   plans without writing generated files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DomModuleMapping {
    pub(super) modules: Vec<DomModulePlan>,
    pub(super) skipped: Vec<DomSkippedDeclaration>,
}

/// Planned generated DOM module.
///
/// Inputs:
/// - One supported TypeScript interface declaration.
///
/// Output:
/// - Module path, generated output paths, generated test path, default type
///   name, and mapped members.
///
/// Transformation:
/// - Applies Terlan module-layout conventions before concrete file emission
///   exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DomModulePlan {
    pub(super) module_path: String,
    pub(super) source_interface: String,
    pub(super) doc: Option<String>,
    pub(super) type_name: String,
    pub(super) type_params: Vec<String>,
    pub(super) source_path: String,
    pub(super) interface_path: String,
    pub(super) summary_path: String,
    pub(super) test_path: String,
    pub(super) members: Vec<DomMemberPlan>,
}

/// Planned generated DOM module member.
///
/// Inputs:
/// - Supported TypeScript interface properties and methods.
///
/// Output:
/// - Property or method plan with Terlan and JavaScript names.
///
/// Transformation:
/// - Preserves JavaScript source names while deriving deterministic Terlan
///   `snake_case` names for the generated API surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum DomMemberPlan {
    Property(DomPropertyPlan),
    Method(DomMethodPlan),
}

/// Planned generated DOM property.
///
/// Inputs:
/// - One TypeScript interface property.
///
/// Output:
/// - Source JS name, Terlan field/method name, readonly/optional metadata, and
///   mapped Terlan type.
///
/// Transformation:
/// - Converts the TypeScript type through the T0.3 mapper before generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DomPropertyPlan {
    pub(super) js_name: String,
    pub(super) terlan_name: String,
    pub(super) doc: Option<String>,
    pub(super) readonly: bool,
    pub(super) optional: bool,
    pub(super) terlan_type: String,
}

/// Planned generated DOM method.
///
/// Inputs:
/// - One TypeScript interface method.
///
/// Output:
/// - Source JS name, Terlan method name, mapped parameters, and mapped return
///   type.
///
/// Transformation:
/// - Converts all TypeScript types through the T0.3 mapper before generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DomMethodPlan {
    pub(super) js_name: String,
    pub(super) terlan_name: String,
    pub(super) doc: Option<String>,
    pub(super) optional: bool,
    pub(super) params: Vec<DomParamPlan>,
    pub(super) return_type: String,
}

/// Planned generated DOM method parameter.
///
/// Inputs:
/// - One TypeScript method parameter.
///
/// Output:
/// - Source JS parameter name, Terlan parameter name, optional metadata, and
///   mapped Terlan type.
///
/// Transformation:
/// - Normalizes names with the same rule as generated properties/methods.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DomParamPlan {
    pub(super) js_name: String,
    pub(super) terlan_name: String,
    pub(super) optional: bool,
    pub(super) terlan_type: String,
}

/// Stable skipped DOM declaration diagnostic.
///
/// Inputs:
/// - Produced when TypeScript-to-Terlan mapping cannot safely emit a member.
///
/// Output:
/// - Source declaration path, stable reason code, and source type label.
///
/// Transformation:
/// - Gives future generation manifests deterministic skip entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DomSkippedDeclaration {
    pub(super) source: String,
    pub(super) reason: &'static str,
    pub(super) detail: String,
}

/// Maps parsed TypeScript declarations into DOM module plans.
///
/// Inputs:
/// - `declarations`: neutral TypeScript declaration file from the Oxc adapter.
///
/// Output:
/// - `DomModuleMapping` containing generated module plans and skipped members.
///
/// Transformation:
/// - Creates one `std.js.Dom.<Interface>` module per TypeScript interface and
///   maps supported members through the T0.3 type mapper.
pub(super) fn map_ts_declarations_to_dom_modules(
    declarations: &TsDeclarationFile,
) -> DomModuleMapping {
    let mut modules = Vec::new();
    let mut skipped = Vec::new();

    for declaration in &declarations.declarations {
        match declaration {
            TsDeclaration::Interface(interface) => {
                if let Some(module) = map_interface_to_module(interface, &mut skipped) {
                    modules.push(module);
                }
            }
            TsDeclaration::Unsupported(unsupported) => {
                skipped.push(map_unsupported_declaration(unsupported));
            }
        }
    }

    DomModuleMapping { modules, skipped }
}

/// Converts a parser-level unsupported top-level declaration into a DOM skip row.
///
/// Inputs:
/// - `unsupported`: parser adapter skip metadata.
///
/// Output:
/// - DOM-level skipped declaration row.
///
/// Transformation:
/// - Preserves manifest namespace and stable reason text for declarations that
///   are intentionally absent from generated std.js output.
fn map_unsupported_declaration(unsupported: &TsUnsupportedDeclaration) -> DomSkippedDeclaration {
    DomSkippedDeclaration {
        source: unsupported.source.clone(),
        reason: unsupported.reason,
        detail: unsupported.detail.clone(),
    }
}

/// Maps one TypeScript interface into a DOM module plan.
///
/// Inputs:
/// - `interface`: neutral TypeScript interface declaration.
/// - `skipped`: shared skipped-declaration accumulator.
///
/// Output:
/// - `DomModulePlan` for the interface.
///
/// Transformation:
/// - Derives module/output paths from the interface name and maps each
///   supported member independently so one skipped member does not discard the
///   entire module.
fn map_interface_to_module(
    interface: &TsInterfaceDeclaration,
    skipped: &mut Vec<DomSkippedDeclaration>,
) -> Option<DomModulePlan> {
    if reserved_bridge_interface_reason(&interface.name).is_some() {
        skipped.push(DomSkippedDeclaration {
            source: interface.name.clone(),
            reason: "ts_bindgen.reserved_bridge_interface",
            detail: "interface is owned by a hand-authored Terlan JavaScript bridge wrapper"
                .to_string(),
        });
        return None;
    }

    let mut members = Vec::new();
    for member in &interface.members {
        match member {
            TsInterfaceMember::Property(property) => {
                if let Some(plan) = map_property(&interface.name, property, skipped) {
                    members.push(DomMemberPlan::Property(plan));
                }
            }
            TsInterfaceMember::Method(method) => {
                if let Some(plan) = map_method(&interface.name, method, skipped) {
                    members.push(DomMemberPlan::Method(plan));
                }
            }
            TsInterfaceMember::Unsupported(unsupported) => {
                skipped.push(map_unsupported_member(&interface.name, unsupported));
            }
        }
    }

    let namespace = binding_namespace(&interface.namespace);
    let file_stem = source_name_to_terlan_identifier(&interface.name);
    let module_path = format!("{namespace}.{}", interface.name);
    let source_dir = namespace_to_source_dir(namespace);
    Some(DomModulePlan {
        module_path: module_path.clone(),
        source_interface: interface.name.clone(),
        doc: interface.doc.clone(),
        type_name: interface.name.clone(),
        type_params: interface.type_params.clone(),
        source_path: format!("{source_dir}/{file_stem}.terl"),
        interface_path: format!("{source_dir}/{file_stem}.terli"),
        summary_path: format!("std/summaries/{module_path}.typi"),
        test_path: format!("{source_dir}/{}Test.terl", interface.name),
        members,
    })
}

/// Returns why a TypeScript interface is reserved for a hand-authored bridge.
///
/// Inputs:
/// - `name`: TypeScript interface name.
///
/// Output:
/// - Skip reason detail when the interface must not be generated.
/// - `None` when normal generation may continue.
///
/// Transformation:
/// - Protects Terlan-owned bridge modules from full `lib.es5.d.ts` generation
///   while still recording a stable skip row for absent TypeScript interfaces.
fn reserved_bridge_interface_reason(name: &str) -> Option<&'static str> {
    match name {
        "Array" | "ArrayConstructor" | "String" | "StringConstructor" | "Number"
        | "NumberConstructor" | "Promise" | "PromiseConstructor" => {
            Some("hand-authored bridge wrapper")
        }
        _ => None,
    }
}

/// Returns the Terlan namespace used for one TypeScript binding.
///
/// Inputs:
/// - `namespace`: manifest-provided namespace, or an empty namespace for older
///   parser-only tests and fixtures.
///
/// Output:
/// - Namespace used to construct generated module paths and output paths.
///
/// Transformation:
/// - Preserves explicit manifest namespaces and falls back to the original DOM
///   namespace for direct parser fixtures that bypass the input manifest.
fn binding_namespace(namespace: &str) -> &str {
    if namespace.is_empty() {
        "std.js.Dom"
    } else {
        namespace
    }
}

/// Converts a Terlan namespace into a generated source directory.
///
/// Inputs:
/// - `namespace`: manifest-owned module namespace such as `std.js.Dom`.
///
/// Output:
/// - Repository-relative directory path.
///
/// Transformation:
/// - Maps dotted module namespace segments to the checked-in stdlib directory
///   layout used by generated TypeScript bindings.
fn namespace_to_source_dir(namespace: &str) -> String {
    namespace
        .split('.')
        .map(source_name_to_terlan_identifier)
        .collect::<Vec<_>>()
        .join("/")
}

/// Converts a parser-level unsupported member into a DOM skip row.
///
/// Inputs:
/// - `interface_name`: source TypeScript interface name.
/// - `unsupported`: parser adapter skip metadata.
///
/// Output:
/// - DOM-level skipped declaration row.
///
/// Transformation:
/// - Prefixes the member source with the interface name so generated skip
///   manifests explain every absent TypeScript member by source path.
fn map_unsupported_member(
    interface_name: &str,
    unsupported: &TsUnsupportedMember,
) -> DomSkippedDeclaration {
    DomSkippedDeclaration {
        source: format!("{interface_name}.{}", unsupported.source),
        reason: unsupported.reason,
        detail: unsupported.detail.clone(),
    }
}

/// Maps one TypeScript property into a DOM property plan.
///
/// Inputs:
/// - `interface_name`: source interface name for diagnostics.
/// - `property`: neutral property declaration.
/// - `skipped`: shared skipped-declaration accumulator.
///
/// Output:
/// - `Some(DomPropertyPlan)` when the property type maps successfully.
/// - `None` when mapping is skipped with diagnostics.
///
/// Transformation:
/// - Converts the TypeScript type and derives a Terlan `snake_case` member
///   name while preserving the original JavaScript name.
fn map_property(
    interface_name: &str,
    property: &TsPropertyDeclaration,
    skipped: &mut Vec<DomSkippedDeclaration>,
) -> Option<DomPropertyPlan> {
    let mapping = map_ts_type_to_terlan(&property.ty);
    let terlan_type = mapped_type_or_skip(
        format!("{interface_name}.{}", property.name),
        mapping,
        skipped,
    )?;

    Some(DomPropertyPlan {
        js_name: property.name.clone(),
        terlan_name: source_name_to_terlan_identifier(&property.name),
        doc: property.doc.clone(),
        readonly: property.readonly,
        optional: property.optional,
        terlan_type,
    })
}

/// Maps one TypeScript method into a DOM method plan.
///
/// Inputs:
/// - `interface_name`: source interface name for diagnostics.
/// - `method`: neutral method declaration.
/// - `skipped`: shared skipped-declaration accumulator.
///
/// Output:
/// - `Some(DomMethodPlan)` when all parameter and return types map.
/// - `None` when any type mapping is skipped with diagnostics.
///
/// Transformation:
/// - Converts method signature types and derives Terlan names without changing
///   the underlying JavaScript method name.
fn map_method(
    interface_name: &str,
    method: &TsMethodDeclaration,
    skipped: &mut Vec<DomSkippedDeclaration>,
) -> Option<DomMethodPlan> {
    let mut params = Vec::new();
    for param in &method.params {
        params.push(map_param(interface_name, &method.name, param, skipped)?);
    }

    let return_type = mapped_type_or_skip(
        format!("{interface_name}.{} return", method.name),
        map_ts_type_to_terlan(&method.return_type),
        skipped,
    )?;

    Some(DomMethodPlan {
        js_name: method.name.clone(),
        terlan_name: source_name_to_terlan_identifier(&method.name),
        doc: method.doc.clone(),
        optional: method.optional,
        params,
        return_type,
    })
}

/// Maps one TypeScript method parameter into a DOM parameter plan.
///
/// Inputs:
/// - `interface_name`: source interface name for diagnostics.
/// - `method_name`: source method name for diagnostics.
/// - `param`: neutral parameter declaration.
/// - `skipped`: shared skipped-declaration accumulator.
///
/// Output:
/// - `Some(DomParamPlan)` when the parameter type maps successfully.
/// - `None` when mapping is skipped with diagnostics.
///
/// Transformation:
/// - Converts the parameter type and normalizes the generated Terlan parameter
///   name.
fn map_param(
    interface_name: &str,
    method_name: &str,
    param: &TsParameterDeclaration,
    skipped: &mut Vec<DomSkippedDeclaration>,
) -> Option<DomParamPlan> {
    let terlan_type = mapped_type_or_skip(
        format!("{interface_name}.{method_name} parameter {}", param.name),
        map_ts_type_to_terlan(&param.ty),
        skipped,
    )?;

    Some(DomParamPlan {
        js_name: param.name.clone(),
        terlan_name: source_name_to_terlan_identifier(&param.name),
        optional: param.optional,
        terlan_type,
    })
}

/// Extracts a mapped Terlan type or records skip diagnostics.
///
/// Inputs:
/// - `source`: source declaration path for diagnostics.
/// - `mapping`: T0.3 type mapping result.
/// - `skipped`: shared skipped-declaration accumulator.
///
/// Output:
/// - `Some(String)` when a Terlan type was produced.
/// - `None` when one or more skip diagnostics were recorded.
///
/// Transformation:
/// - Converts type-level skip diagnostics into DOM declaration skip entries.
fn mapped_type_or_skip(
    source: String,
    mapping: TsTypeMapping,
    skipped: &mut Vec<DomSkippedDeclaration>,
) -> Option<String> {
    if let Some(terlan_type) = mapping.terlan_type {
        return Some(terlan_type);
    }

    for skip in mapping.skipped {
        skipped.push(skip_to_dom_skipped(source.clone(), skip));
    }
    None
}

/// Converts a type skip diagnostic into a DOM skip diagnostic.
///
/// Inputs:
/// - `source`: declaration path where the skip occurred.
/// - `skip`: type-level skip diagnostic.
///
/// Output:
/// - DOM-level skip diagnostic.
///
/// Transformation:
/// - Preserves the stable reason code and carries the source type label into
///   generated-manifest-ready detail text.
fn skip_to_dom_skipped(source: String, skip: TsTypeSkip) -> DomSkippedDeclaration {
    DomSkippedDeclaration {
        source,
        reason: skip.reason,
        detail: skip.source,
    }
}
