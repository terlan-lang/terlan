use std::fs;
use std::path::Path;

use serde_json::json;

use crate::commands::artifacts::{fingerprint, DependencyManifest};
use crate::validation::phase_manifest::current_syntax_contract_identity;

use super::ts_dom_module_mapping::{
    map_ts_declarations_to_dom_modules, DomMemberPlan, DomMethodPlan, DomModuleMapping,
    DomModulePlan, DomParamPlan, DomPropertyPlan, DomSkippedDeclaration,
};
use super::ts_input_manifest::{load_ts_input_manifest, safe_repo_relative_path, TsInputManifest};
use super::ts_parser_adapter::{parse_ts_declaration_file, TsDeclaration, TsDeclarationFile};

/// Dynamic generated file emitted by the TypeScript DOM binding generator.
///
/// Inputs:
/// - Created from DOM module plans and binding manifest metadata.
///
/// Output:
/// - Repository-relative output path plus deterministic contents.
///
/// Transformation:
/// - Separates generation planning from filesystem writes so tests can inspect
///   generated artifacts before materialization.
#[derive(Debug, Clone, PartialEq, Eq)]
struct GeneratedBindingFile {
    path: String,
    contents: String,
}

/// Generates JS DOM bindings from a pinned TypeScript input manifest.
///
/// Inputs:
/// - `repo_root`: repository root used to resolve manifest inputs.
/// - `manifest_path`: manifest path, absolute or relative to `repo_root`.
/// - `out_dir`: empty destination directory for generated files.
///
/// Output:
/// - `Ok(())` when generated files are written.
/// - `Err(String)` when manifest validation, parsing, mapping, or filesystem
///   writing fails.
///
/// Transformation:
/// - Loads the pinned input manifest, parses each `.d.ts` input with Oxc,
///   builds `std.js.Dom.*` module plans, emits source/interface/summary files,
///   and writes a generated binding manifest.
pub(super) fn generate_js_dom_bindings(
    repo_root: &Path,
    manifest_path: &Path,
    out_dir: &Path,
) -> Result<(), String> {
    let manifest = load_ts_input_manifest(repo_root, manifest_path)?;
    let declarations = parse_manifest_inputs(repo_root, &manifest)?;
    let mapping = map_ts_declarations_to_dom_modules(&declarations);
    let files = generated_files(&manifest, manifest_path, &mapping)?;
    write_generated_files(out_dir, &files)
}

/// Parses all TypeScript declaration inputs from a validated manifest.
///
/// Inputs:
/// - `repo_root`: repository root used to resolve manifest input paths.
/// - `manifest`: validated TypeScript input manifest.
///
/// Output:
/// - `Ok(TsDeclarationFile)` containing all parsed declarations.
/// - `Err(String)` when an input cannot be read or parsed.
///
/// Transformation:
/// - Reads each pinned declaration input and appends its parsed declarations in
///   manifest order.
fn parse_manifest_inputs(
    repo_root: &Path,
    manifest: &TsInputManifest,
) -> Result<TsDeclarationFile, String> {
    let mut declarations = Vec::new();
    for input in &manifest.inputs {
        let relative_path = safe_repo_relative_path(&input.path)?;
        let input_path = repo_root.join(relative_path);
        let source = fs::read_to_string(&input_path).map_err(|err| {
            format!(
                "ts_bindgen.read_input_failed: `{}`: {err}",
                input_path.display()
            )
        })?;
        let parsed = parse_ts_declaration_file(&source).map_err(|err| {
            format!(
                "{}: `{}`: {}",
                err.reason,
                input_path.display(),
                err.message
            )
        })?;
        declarations.extend(
            parsed
                .declarations
                .into_iter()
                .map(|declaration| declaration_with_namespace(declaration, &input.namespace)),
        );
    }
    Ok(TsDeclarationFile { declarations })
}

/// Applies a manifest namespace to one parsed TypeScript declaration.
///
/// Inputs:
/// - `declaration`: parsed declaration without repository namespace metadata.
/// - `namespace`: manifest-owned Terlan namespace for the input file.
///
/// Output:
/// - Declaration carrying the namespace used for generated module paths.
///
/// Transformation:
/// - Keeps Oxc parsing independent from repository layout while allowing one
///   manifest to generate both `std.js.*` and `std.js.Dom.*` surfaces.
fn declaration_with_namespace(declaration: TsDeclaration, namespace: &str) -> TsDeclaration {
    match declaration {
        TsDeclaration::Interface(mut interface) => {
            interface.namespace = namespace.to_string();
            TsDeclaration::Interface(interface)
        }
        TsDeclaration::Unsupported(mut unsupported) => {
            unsupported.source = format!("{namespace}.{}", unsupported.source);
            TsDeclaration::Unsupported(unsupported)
        }
    }
}

/// Builds all generated files for a DOM module mapping.
///
/// Inputs:
/// - `manifest`: validated TypeScript input manifest.
/// - `manifest_path`: user-supplied manifest path used for provenance.
/// - `mapping`: DOM module mapping result.
///
/// Output:
/// - `Ok(Vec<GeneratedBindingFile>)` with source, interface, summary, test,
///   and binding manifest files.
/// - `Err(String)` when JSON manifest rendering fails.
///
/// Transformation:
/// - Renders deterministic text files from module plans without touching the
///   filesystem.
fn generated_files(
    manifest: &TsInputManifest,
    manifest_path: &Path,
    mapping: &DomModuleMapping,
) -> Result<Vec<GeneratedBindingFile>, String> {
    let mut files = Vec::new();
    for module in &mapping.modules {
        let source = render_module_source(module, manifest, manifest_path);
        let interface = render_module_interface(module, manifest, manifest_path);
        let summary = render_module_summary(module, manifest, manifest_path);
        let test = render_module_test(module, manifest, manifest_path);
        files.push(GeneratedBindingFile {
            path: module.source_path.clone(),
            contents: source.clone(),
        });
        files.push(GeneratedBindingFile {
            path: module.interface_path.clone(),
            contents: interface,
        });
        files.push(GeneratedBindingFile {
            path: module.summary_path.clone(),
            contents: summary.clone(),
        });
        files.push(GeneratedBindingFile {
            path: format!("{}.deps", module.summary_path),
            contents: render_module_summary_deps(module, &source, &summary)?,
        });
        files.push(GeneratedBindingFile {
            path: module.test_path.clone(),
            contents: test,
        });
    }
    files.push(GeneratedBindingFile {
        path: "std/js/manifests/std_js_bindings.json".to_string(),
        contents: render_binding_manifest(manifest, manifest_path, mapping)?,
    });
    files.push(GeneratedBindingFile {
        path: "std/js/manifests/std_js_skipped.json".to_string(),
        contents: render_skipped_manifest(manifest, manifest_path, mapping)?,
    });
    Ok(files)
}

/// Renders generated summary dependency metadata for one module.
///
/// Inputs:
/// - `module`: generated module plan.
/// - `source`: generated source text.
/// - `summary`: generated summary text.
///
/// Output:
/// - Encoded `.typi.deps` manifest text.
///
/// Transformation:
/// - Produces the cache metadata required by std summary inventory checks
///   without running generated TypeScript bindings back through the full
///   compiler pipeline.
fn render_module_summary_deps(
    module: &DomModulePlan,
    source: &str,
    summary: &str,
) -> Result<String, String> {
    let syntax_contract_identity = current_syntax_contract_identity()?;
    Ok(DependencyManifest {
        module: module.module_path.clone(),
        syntax_contract_identity,
        source_hash: fingerprint(source.as_bytes()),
        interface_hash: fingerprint(summary.as_bytes()),
        interface_doc_hash: fingerprint(summary.as_bytes()),
        dependencies: Vec::new(),
    }
    .encode())
}

/// Renders generated Terlan source for one DOM module.
///
/// Inputs:
/// - `module`: planned DOM module.
/// - `manifest`: validated input manifest.
/// - `manifest_path`: user-supplied manifest path used for provenance.
///
/// Output:
/// - Generated `.terl` source text.
///
/// Transformation:
/// - Emits a deterministic generated header, module declaration, opaque default
///   type, and receiver signatures for planned members.
fn render_module_source(
    module: &DomModulePlan,
    manifest: &TsInputManifest,
    manifest_path: &Path,
) -> String {
    render_module_contract(module, manifest, manifest_path, "source", true)
}

/// Renders generated Terlan interface text for one DOM module.
///
/// Inputs:
/// - `module`: planned DOM module.
/// - `manifest`: validated input manifest.
/// - `manifest_path`: user-supplied manifest path used for provenance.
///
/// Output:
/// - Generated `.terli` text.
///
/// Transformation:
/// - Uses the same first-slice contract representation as `.terl` until the
///   emitter distinguishes implementation from interface bodies.
fn render_module_interface(
    module: &DomModulePlan,
    manifest: &TsInputManifest,
    manifest_path: &Path,
) -> String {
    render_module_contract(module, manifest, manifest_path, "interface", false)
}

/// Renders generated summary text for one DOM module.
///
/// Inputs:
/// - `module`: planned DOM module.
/// - `manifest`: validated input manifest.
/// - `manifest_path`: user-supplied manifest path used for provenance.
///
/// Output:
/// - Generated `.typi` summary text.
///
/// Transformation:
/// - Reuses the generated interface contract so import/typecheck wiring can
///   converge on the normal summary format in later gates.
fn render_module_summary(
    module: &DomModulePlan,
    manifest: &TsInputManifest,
    manifest_path: &Path,
) -> String {
    render_module_contract(module, manifest, manifest_path, "summary", false)
}

/// Renders generated Terlan test text for one DOM module.
///
/// Inputs:
/// - `module`: planned DOM module.
/// - `manifest`: validated input manifest.
/// - `manifest_path`: user-supplied manifest path used for provenance.
///
/// Output:
/// - Generated `.terl` test source text.
///
/// Transformation:
/// - Emits deterministic API-shape tests that compile once the generated
///   `std.js` support types are available, without executing browser APIs.
fn render_module_test(
    module: &DomModulePlan,
    manifest: &TsInputManifest,
    manifest_path: &Path,
) -> String {
    let mut output = render_module_header(module, manifest, manifest_path, "test");
    output.push_str(&format!("module {}Test.\n\n", module.module_path));
    output.push_str(&format!(
        "import type {}.{}.\n\n",
        module.module_path, module.type_name
    ));
    output.push_str("@test\npub generated_binding_surface_exists(): Bool ->\n    true.\n");
    for member in &module.members {
        output.push('\n');
        output.push_str(&render_member_test(module, member));
    }
    output
}

/// Renders the shared generated contract text for one module.
///
/// Inputs:
/// - `module`: planned DOM module.
/// - `manifest`: validated input manifest.
/// - `manifest_path`: user-supplied manifest path used for provenance.
/// - `kind`: generated artifact kind label.
/// - `include_bodies`: whether declarations need source bodies.
///
/// Output:
/// - Terlan contract text for source, interface, or summary output.
///
/// Transformation:
/// - Converts mapped module members into receiver-style declarations, adding
///   `native` placeholder bodies only for `.terl` source artifacts.
fn render_module_contract(
    module: &DomModulePlan,
    manifest: &TsInputManifest,
    manifest_path: &Path,
    kind: &str,
    include_bodies: bool,
) -> String {
    let mut output = render_module_header(module, manifest, manifest_path, kind);
    output.push_str(&format!("module {}.\n\n", module.module_path));
    if let Some(doc) = &module.doc {
        output.push_str(&render_doc_block(doc));
    }
    output.push_str(&format!(
        "pub opaque type {}.\n",
        render_type_declaration_name(module)
    ));
    for member in &module.members {
        output.push('\n');
        output.push_str(&render_member(module, member, include_bodies));
    }
    output
}

/// Renders the generated provenance header for one module artifact.
///
/// Inputs:
/// - `module`: planned DOM module.
/// - `manifest`: validated input manifest.
/// - `manifest_path`: user-supplied manifest path used for provenance.
/// - `kind`: generated artifact kind label.
///
/// Output:
/// - Header text ending with one blank line.
///
/// Transformation:
/// - Converts manifest and module provenance into compact comment metadata
///   shared by generated sources, interfaces, summaries, and tests.
fn render_module_header(
    module: &DomModulePlan,
    manifest: &TsInputManifest,
    manifest_path: &Path,
    kind: &str,
) -> String {
    let mut output = String::new();
    output.push_str("/**\n");
    output.push_str(" * @generated true\n");
    output.push_str(" * @do-not-edit true\n");
    output.push_str(" * @generator terlc\n");
    output.push_str(&format!(
        " * @generator-version {}\n",
        manifest.generator.version
    ));
    output.push_str(&format!(
        " * @generator-profile {}\n",
        manifest.generator.profile
    ));
    output.push_str(&format!(" * @artifact-kind {kind}\n"));
    output.push_str(&format!(" * @input-manifest {}\n", manifest_path.display()));
    output.push_str(&format!(
        " * @source-package {}@{}\n",
        manifest.source_package.name, manifest.source_package.version
    ));
    for input in &manifest.inputs {
        output.push_str(&format!(
            " * @source-input {} sha256={}\n",
            input.path, input.sha256
        ));
    }
    output.push_str(&format!(
        " * @source-interface {}\n",
        module.source_interface
    ));
    output.push_str(" */\n\n");
    output
}

/// Renders one DOM module member declaration.
///
/// Inputs:
/// - `module`: containing DOM module plan.
/// - `member`: planned DOM member.
///
/// Output:
/// - Terlan receiver declaration text.
///
/// Transformation:
/// - Dispatches property and method members to their declaration renderers.
fn render_member(module: &DomModulePlan, member: &DomMemberPlan, include_body: bool) -> String {
    match member {
        DomMemberPlan::Property(property) => render_property(module, property, include_body),
        DomMemberPlan::Method(method) => render_method(module, method, include_body),
    }
}

/// Renders one DOM property as a receiver getter declaration.
///
/// Inputs:
/// - `module`: containing DOM module plan.
/// - `property`: planned DOM property.
///
/// Output:
/// - Terlan receiver declaration text.
///
/// Transformation:
/// - Represents properties as receiver getters for the first generated DOM
///   surface, preserving mutating setters for later gates.
fn render_property(
    module: &DomModulePlan,
    property: &DomPropertyPlan,
    include_body: bool,
) -> String {
    let mut output = String::new();
    if let Some(doc) = &property.doc {
        output.push_str(&render_doc_block(doc));
    }
    let signature = format!(
        "pub (value: {}) {}(): {}",
        render_type_reference_name(module),
        property.terlan_name,
        property.terlan_type
    );
    output.push_str(&render_signature(signature, include_body));
    output
}

/// Renders one DOM method as a receiver method declaration.
///
/// Inputs:
/// - `module`: containing DOM module plan.
/// - `method`: planned DOM method.
///
/// Output:
/// - Terlan receiver declaration text.
///
/// Transformation:
/// - Emits normalized Terlan parameter names while retaining original JS names
///   in the module plan for later backend lowering.
fn render_method(module: &DomModulePlan, method: &DomMethodPlan, include_body: bool) -> String {
    let mut output = String::new();
    if let Some(doc) = &method.doc {
        output.push_str(&render_doc_block(doc));
    }
    let signature = format!(
        "pub (value: {}) {}({}): {}",
        render_type_reference_name(module),
        method.terlan_name,
        render_params(&method.params),
        method.return_type
    );
    output.push_str(&render_signature(signature, include_body));
    output
}

/// Renders TypeScript-sourced documentation as a Terlan doc block.
///
/// Inputs:
/// - `doc`: normalized JSDoc body without comment delimiters.
///
/// Output:
/// - Terlan block documentation ending with one blank line.
///
/// Transformation:
/// - Re-wraps TypeScript documentation in Terlan's block-comment style while
///   preserving `@param`, `@returns`, and free-form text lines.
fn render_doc_block(doc: &str) -> String {
    let mut output = String::new();
    output.push_str("/**\n");
    for line in doc.lines() {
        if line.is_empty() {
            output.push_str(" *\n");
        } else {
            output.push_str(" * ");
            output.push_str(&line.replace("*/", "* /"));
            output.push('\n');
        }
    }
    output.push_str(" */\n");
    output
}

/// Renders a generated type declaration name.
///
/// Inputs:
/// - `module`: generated DOM module plan.
///
/// Output:
/// - Type name with Terlan type parameters when the source interface is
///   generic.
///
/// Transformation:
/// - Converts TypeScript angle-bracket interface parameters into Terlan's
///   bracketed type parameter syntax.
fn render_type_declaration_name(module: &DomModulePlan) -> String {
    render_type_name(&module.type_name, &module.type_params)
}

/// Renders a generated type reference name.
///
/// Inputs:
/// - `module`: generated DOM module plan.
///
/// Output:
/// - Receiver type reference using the same type parameter names as the
///   generated type declaration.
///
/// Transformation:
/// - Keeps generated receiver methods generic whenever their source interface
///   is generic.
fn render_type_reference_name(module: &DomModulePlan) -> String {
    render_type_name(&module.type_name, &module.type_params)
}

/// Renders a Terlan type name with optional type parameters.
///
/// Inputs:
/// - `name`: base type name.
/// - `type_params`: source type parameter names.
///
/// Output:
/// - `Name` for non-generic types or `Name[T, U]` for generic types.
///
/// Transformation:
/// - Applies Terlan generic syntax without interpreting TypeScript constraints
///   or defaults.
fn render_type_name(name: &str, type_params: &[String]) -> String {
    if type_params.is_empty() {
        name.to_string()
    } else {
        format!("{name}[{}]", type_params.join(", "))
    }
}

/// Renders a signature as either a source body or declaration.
///
/// Inputs:
/// - `signature`: function signature without trailing punctuation.
/// - `include_body`: whether to emit a `native` placeholder body.
///
/// Output:
/// - Complete Terlan declaration text.
///
/// Transformation:
/// - Keeps interface/summary artifacts declaration-only while making generated
///   `.terl` source parseable by adding the required arrow body.
fn render_signature(signature: String, include_body: bool) -> String {
    if include_body {
        format!("{signature} ->\n    native.\n")
    } else {
        format!("{signature}.\n")
    }
}

/// Renders one generated test helper for a DOM member.
///
/// Inputs:
/// - `module`: containing DOM module plan.
/// - `member`: planned DOM member.
///
/// Output:
/// - Terlan function that typechecks one generated receiver API shape.
///
/// Transformation:
/// - Converts properties and methods into parameterized helper functions so
///   generated tests cover signatures without constructing DOM runtime values.
fn render_member_test(module: &DomModulePlan, member: &DomMemberPlan) -> String {
    match member {
        DomMemberPlan::Property(property) => render_property_test(module, property),
        DomMemberPlan::Method(method) => render_method_test(module, method),
    }
}

/// Renders a generated property-shape test helper.
///
/// Inputs:
/// - `module`: containing DOM module plan.
/// - `property`: planned DOM property.
///
/// Output:
/// - Parameterized Terlan function returning the property getter type.
///
/// Transformation:
/// - Calls the generated receiver getter so source review can see the exact
///   property method and mapped return type.
fn render_property_test(module: &DomModulePlan, property: &DomPropertyPlan) -> String {
    format!(
        "pub {}_typechecks(receiver: {}): {} ->\n    receiver.{}().\n",
        property.terlan_name,
        render_type_reference_name(module),
        property.terlan_type,
        property.terlan_name
    )
}

/// Renders a generated method-shape test helper.
///
/// Inputs:
/// - `module`: containing DOM module plan.
/// - `method`: planned DOM method.
///
/// Output:
/// - Parameterized Terlan function returning the method call type.
///
/// Transformation:
/// - Reuses generated parameter names where possible while reserving the
///   receiver helper name so source JavaScript parameters named `value` cannot
///   shadow the method-call receiver in generated tests.
fn render_method_test(module: &DomModulePlan, method: &DomMethodPlan) -> String {
    let receiver_param = format!("receiver: {}", render_type_reference_name(module));
    let mut params = vec![receiver_param];
    let argument_names = collision_free_test_argument_names(&method.params);
    params.extend(
        method
            .params
            .iter()
            .zip(argument_names.iter())
            .map(|(param, name)| format!("{}: {}", name, param.terlan_type)),
    );
    format!(
        "pub {}_typechecks({}): {} ->\n    receiver.{}({}).\n",
        method.terlan_name,
        params.join(", "),
        method.return_type,
        method.terlan_name,
        argument_names.join(", ")
    )
}

/// Builds collision-free generated test argument names.
///
/// Inputs:
/// - `params`: planned DOM method parameters.
///
/// Output:
/// - Parameter names safe to use beside the generated `receiver` binding.
///
/// Transformation:
/// - Preserves each generated parameter name unless it collides with the
///   receiver binding or an earlier parameter, appending a stable numeric
///   suffix for collisions.
fn collision_free_test_argument_names(params: &[DomParamPlan]) -> Vec<String> {
    let mut used = vec!["receiver".to_string()];
    params
        .iter()
        .map(|param| unique_test_argument_name(&param.terlan_name, &mut used))
        .collect()
}

/// Selects one generated helper argument name.
///
/// Inputs:
/// - `base`: preferred generated parameter name.
/// - `used`: names already reserved in the helper declaration.
///
/// Output:
/// - Unique helper parameter name.
///
/// Transformation:
/// - Returns `base` when unused, otherwise appends `_2`, `_3`, and so on until
///   the name is unique, then records that name in `used`.
fn unique_test_argument_name(base: &str, used: &mut Vec<String>) -> String {
    if !used.iter().any(|name| name == base) {
        used.push(base.to_string());
        return base.to_string();
    }

    for suffix in 2.. {
        let candidate = format!("{base}_{suffix}");
        if !used.iter().any(|name| name == &candidate) {
            used.push(candidate.clone());
            return candidate;
        }
    }

    unreachable!("unbounded suffix search should always find a unique name")
}

/// Renders method parameters.
///
/// Inputs:
/// - `params`: planned DOM parameters.
///
/// Output:
/// - Comma-separated Terlan parameter list.
///
/// Transformation:
/// - Converts parameter plans into `name: Type` text.
fn render_params(params: &[DomParamPlan]) -> String {
    params
        .iter()
        .map(|param| format!("{}: {}", param.terlan_name, param.terlan_type))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Renders the generated binding manifest.
///
/// Inputs:
/// - `manifest`: validated TypeScript input manifest.
/// - `manifest_path`: user-supplied manifest path used for provenance.
/// - `mapping`: DOM module mapping result.
///
/// Output:
/// - Pretty JSON generated binding manifest.
///
/// Transformation:
/// - Records pinned inputs, generated outputs, target profile, generator
///   version, and skipped declarations in the roadmap-defined shape.
fn render_binding_manifest(
    manifest: &TsInputManifest,
    manifest_path: &Path,
    mapping: &DomModuleMapping,
) -> Result<String, String> {
    let outputs = mapping
        .modules
        .iter()
        .map(|module| {
            json!({
                "module": module.module_path,
                "source": module.source_path,
                "interface": module.interface_path,
                "summary": module.summary_path,
                "test": module.test_path,
            })
        })
        .collect::<Vec<_>>();
    let skipped = mapping
        .skipped
        .iter()
        .map(skipped_manifest_entry)
        .collect::<Vec<_>>();
    let inputs = manifest
        .inputs
        .iter()
        .map(|input| {
            json!({
                "package": manifest.source_package.name,
                "package_version": manifest.source_package.version,
                "path": input.path,
                "sha256": input.sha256,
                "kind": input.kind,
                "namespace": input.namespace,
            })
        })
        .collect::<Vec<_>>();

    serde_json::to_string_pretty(&json!({
        "schema": "terlan.std.js.bindings.v1",
        "generator": manifest.generator.name,
        "generator_version": manifest.generator.version,
        "generator_profile": manifest.generator.profile,
        "input_manifest": manifest_path.display().to_string(),
        "target_profile": manifest.target_profile,
        "inputs": inputs,
        "outputs": outputs,
        "skipped_manifest": "std/js/manifests/std_js_skipped.json",
        "skipped": skipped,
    }))
    .map(|json| format!("{json}\n"))
    .map_err(|err| format!("ts_bindgen.binding_manifest_render_failed: {err}"))
}

/// Renders the skipped-declarations manifest.
///
/// Inputs:
/// - `manifest`: validated TypeScript input manifest.
/// - `manifest_path`: user-supplied manifest path used for provenance.
/// - `mapping`: DOM module mapping result.
///
/// Output:
/// - Pretty JSON skipped-declarations manifest.
///
/// Transformation:
/// - Emits a standalone review artifact for skipped declarations even when the
///   current fixture has no skipped declarations.
fn render_skipped_manifest(
    manifest: &TsInputManifest,
    manifest_path: &Path,
    mapping: &DomModuleMapping,
) -> Result<String, String> {
    let skipped = mapping
        .skipped
        .iter()
        .map(skipped_manifest_entry)
        .collect::<Vec<_>>();
    serde_json::to_string_pretty(&json!({
        "schema": "terlan.std.js.skipped-declarations.v1",
        "generator": manifest.generator.name,
        "generator_version": manifest.generator.version,
        "input_manifest": manifest_path.display().to_string(),
        "target_profile": manifest.target_profile,
        "skipped": skipped,
    }))
    .map(|json| format!("{json}\n"))
    .map_err(|err| format!("ts_bindgen.skipped_manifest_render_failed: {err}"))
}

/// Converts a skipped declaration into JSON.
///
/// Inputs:
/// - `skipped`: skipped DOM declaration diagnostic.
///
/// Output:
/// - JSON value suitable for the generated binding manifest.
///
/// Transformation:
/// - Preserves source, reason, and detail fields exactly.
fn skipped_manifest_entry(skipped: &DomSkippedDeclaration) -> serde_json::Value {
    json!({
        "source": skipped.source,
        "reason": skipped.reason,
        "detail": skipped.detail,
    })
}

/// Writes generated files into an empty output directory.
///
/// Inputs:
/// - `out_dir`: destination directory.
/// - `files`: generated file list.
///
/// Output:
/// - `Ok(())` when all files are written.
/// - `Err(String)` when the output directory is non-empty or writing fails.
///
/// Transformation:
/// - Refuses to overwrite existing output and materializes parent directories
///   for every generated file.
fn write_generated_files(out_dir: &Path, files: &[GeneratedBindingFile]) -> Result<(), String> {
    ensure_empty_output_dir(out_dir)?;
    for file in files {
        let path = out_dir.join(&file.path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                format!("failed to create directory `{}`: {err}", parent.display())
            })?;
        }
        fs::write(&path, &file.contents)
            .map_err(|err| format!("failed to write generated file `{}`: {err}", path.display()))?;
    }
    Ok(())
}

/// Ensures an output directory exists and is empty.
///
/// Inputs:
/// - `out_dir`: destination directory.
///
/// Output:
/// - `Ok(())` when the directory exists and contains no entries.
/// - `Err(String)` when an existing directory is non-empty or filesystem
///   inspection fails.
///
/// Transformation:
/// - Creates missing output directories and refuses to overwrite existing
///   generated artifacts.
fn ensure_empty_output_dir(out_dir: &Path) -> Result<(), String> {
    if out_dir.exists() {
        let mut entries = fs::read_dir(out_dir).map_err(|err| {
            format!(
                "failed to read output directory `{}`: {err}",
                out_dir.display()
            )
        })?;
        if entries
            .next()
            .transpose()
            .map_err(|err| {
                format!(
                    "failed to inspect output directory `{}`: {err}",
                    out_dir.display()
                )
            })?
            .is_some()
        {
            return Err(format!(
                "refusing to generate into non-empty output directory `{}`",
                out_dir.display()
            ));
        }
    } else {
        fs::create_dir_all(out_dir).map_err(|err| {
            format!(
                "failed to create output directory `{}`: {err}",
                out_dir.display()
            )
        })?;
    }
    Ok(())
}
