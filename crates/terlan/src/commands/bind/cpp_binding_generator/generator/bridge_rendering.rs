use super::*;

/// Validates the complete adapter build plan before generation starts.
pub(super) fn validate_build_plan(
    build: &CppBuildPlan,
    input_dir: &Path,
) -> Result<(), CppBindingError> {
    validate_unique_paths("adapter header", &build.adapter_headers)?;
    for header in &build.adapter_headers {
        validate_input_path(input_dir, header)?;
    }
    if build.include_roots.is_empty() {
        return Err("C++ build plan requires at least one adapter include root".into());
    }
    validate_unique_paths("include root", &build.include_roots)?;
    validate_defines(&build.defines)?;
    validate_unique_paths("library search path", &build.library_search_paths)?;
    validate_linked_libraries(&build.linked_libraries)?;
    validate_unique_paths("rebuild input", &build.rebuild_inputs)?;
    if build.rebuild_inputs.is_empty() {
        return Err("C++ build plan requires at least one rebuild input".into());
    }

    let mut selectors = BTreeSet::new();
    for condition in &build.platform_conditions {
        if condition.target_os.is_none()
            && condition.target_arch.is_none()
            && condition.target_env.is_none()
        {
            return Err("C++ platform condition requires at least one target selector".into());
        }
        for (kind, selector) in [
            ("operating system", condition.target_os.as_deref()),
            ("architecture", condition.target_arch.as_deref()),
            ("environment", condition.target_env.as_deref()),
        ] {
            if let Some(selector) = selector {
                validate_target_selector(kind, selector)?;
            }
        }
        let selector = (
            condition.target_os.as_deref(),
            condition.target_arch.as_deref(),
            condition.target_env.as_deref(),
        );
        if !selectors.insert(selector) {
            return Err((format!(
                "duplicate C++ platform condition for os={:?}, arch={:?}, env={:?}",
                selector.0, selector.1, selector.2
            ))
            .into());
        }
        validate_unique_paths("conditional include root", &condition.include_roots)?;
        validate_defines(&condition.defines)?;
        validate_unique_paths(
            "conditional library search path",
            &condition.library_search_paths,
        )?;
        validate_linked_libraries(&condition.linked_libraries)?;
    }
    let mut external_envs = BTreeSet::new();
    for external in &build.external_roots {
        if !is_environment_name(&external.env) {
            return Err((format!(
                "invalid external C++ SDK environment variable `{}`",
                external.env
            ))
            .into());
        }
        if !external_envs.insert(external.env.as_str()) {
            return Err((format!(
                "duplicate external C++ SDK environment variable `{}`",
                external.env
            ))
            .into());
        }
        if external.include_roots.is_empty() {
            return Err((format!(
                "external C++ SDK `{}` requires at least one include root",
                external.env
            ))
            .into());
        }
        validate_unique_paths("external include root", &external.include_roots)?;
        validate_unique_paths(
            "external library search path",
            &external.library_search_paths,
        )?;
        validate_linked_libraries(&external.linked_libraries)?;
        validate_unique_paths("external rebuild input", &external.rebuild_inputs)?;
    }
    Ok(())
}

/// Restricts build-time environment lookups to conventional constant names.
fn is_environment_name(value: &str) -> bool {
    !value.is_empty()
        && value.chars().all(|character| {
            character.is_ascii_uppercase() || character.is_ascii_digit() || character == '_'
        })
        && value
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_uppercase())
}

/// Validates adapter-relative paths and rejects duplicate entries.
pub(super) fn validate_unique_paths(kind: &str, paths: &[String]) -> Result<(), CppBindingError> {
    let mut seen = BTreeSet::new();
    for value in paths {
        validate_adapter_path(kind, value)?;
        if !seen.insert(value) {
            return Err((format!("duplicate C++ {kind} `{value}`")).into());
        }
    }
    Ok(())
}

/// Validates one path relative to the generated adapter root.
pub(super) fn validate_adapter_path(kind: &str, value: &str) -> Result<(), CppBindingError> {
    if value == "." {
        return Ok(());
    }
    let path = Path::new(value);
    if value.trim().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        || value.chars().any(|ch| matches!(ch, '\0' | '\n' | '\r'))
    {
        return Err((format!(
            "C++ {kind} `{value}` must be relative to the generated adapter root"
        ))
        .into());
    }
    Ok(())
}

/// Validates preprocessor names and directive-safe optional values.
pub(super) fn validate_defines(
    defines: &BTreeMap<String, Option<String>>,
) -> Result<(), CppBindingError> {
    for (name, value) in defines {
        if !is_identifier_segment(name) {
            return Err((format!("invalid C++ preprocessor define `{name}`")).into());
        }
        if value
            .as_deref()
            .is_some_and(|value| value.chars().any(|ch| matches!(ch, '\0' | '\n' | '\r')))
        {
            return Err(
                (format!("C++ preprocessor define `{name}` contains an invalid value")).into(),
            );
        }
    }
    Ok(())
}

/// Validates linked-library names, modes, and uniqueness.
pub(super) fn validate_linked_libraries(
    libraries: &[CppLinkedLibrary],
) -> Result<(), CppBindingError> {
    let mut seen = BTreeSet::new();
    for library in libraries {
        if library.name.is_empty()
            || !library
                .name
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | '+'))
        {
            return Err((format!("invalid C++ linked library `{}`", library.name)).into());
        }
        if !seen.insert((library.name.as_str(), cpp_link_kind_name(library.kind))) {
            return Err((format!("duplicate C++ linked library `{}`", library.name)).into());
        }
    }
    Ok(())
}

/// Validates one Cargo target selector token.
pub(super) fn validate_target_selector(kind: &str, value: &str) -> Result<(), CppBindingError> {
    if value.is_empty()
        || !value
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '_' | '-'))
    {
        return Err((format!("invalid C++ target {kind} selector `{value}`")).into());
    }
    Ok(())
}

/// Requires every opaque resource to have a reviewed producer and one disposer.
pub(super) fn validate_resource_roles(
    manifest: &NativeBindingManifest,
) -> Result<(), CppBindingError> {
    let mut qualified_types = BTreeSet::new();
    for module in &manifest.modules {
        for ty in &module.types {
            let qualified = format!("{}.{}", module.module, ty.name);
            if !qualified_types.insert(qualified.clone()) {
                return Err(
                    (format!("duplicate generated C++ resource type `{qualified}`")).into(),
                );
            }
            if ty.kind != NativeBindingTypeKind::OpaqueResource {
                continue;
            }
            let producers = manifest
                .modules
                .iter()
                .flat_map(|owner| {
                    owner
                        .functions
                        .iter()
                        .map(move |function| (owner, function))
                })
                .filter(|(owner, function)| {
                    native_role_can_produce_resource(function.role)
                        && (terlan_type_matches(&function.returns, &qualified)
                            || (owner.module == module.module
                                && terlan_type_matches(&function.returns, &ty.name))
                            || (function.role == NativeFunctionRole::ResourceListProjection
                                && function
                                    .returns
                                    .strip_prefix("List[")
                                    .and_then(|value| value.strip_suffix(']'))
                                    .is_some_and(|element| {
                                        terlan_type_matches(element, &qualified)
                                            || (owner.module == module.module
                                                && terlan_type_matches(element, &ty.name))
                                    })))
                })
                .count();
            if producers == 0 {
                return Err((format!(
                    "resource `{qualified}` requires at least one reviewed producer; found 0"
                ))
                .into());
            }
            let disposers = module
                .functions
                .iter()
                .filter(|function| {
                    function.role == NativeFunctionRole::Dispose
                        && function.args.len() == 1
                        && terlan_type_matches(&function.args[0].ty, &ty.name)
                })
                .count();
            if disposers != 1 {
                return Err((format!(
                    "resource `{qualified}` requires exactly one disposer; found {disposers}"
                ))
                .into());
            }
        }
        for function in &module.functions {
            if matches!(
                function.role,
                NativeFunctionRole::ImmutableMethod
                    | NativeFunctionRole::MutableMethod
                    | NativeFunctionRole::ValueProjection
                    | NativeFunctionRole::EnumProjection
                    | NativeFunctionRole::StringProjection
                    | NativeFunctionRole::IntListProjection
                    | NativeFunctionRole::ExceptionMethod
            ) && !function.args.first().is_some_and(|arg| {
                manifest
                    .modules
                    .iter()
                    .flat_map(|owner| &owner.types)
                    .any(|ty| terlan_type_matches(&arg.ty, &ty.name))
            }) {
                return Err((format!(
                    "C++ method `{}` requires a module-owned resource as its first argument",
                    function.name
                ))
                .into());
            }
        }
    }
    Ok(())
}

/// Returns whether a public function role may create an owned resource.
pub(super) fn native_role_can_produce_resource(role: NativeFunctionRole) -> bool {
    matches!(
        role,
        NativeFunctionRole::Constructor
            | NativeFunctionRole::ImmutableMethod
            | NativeFunctionRole::MutableMethod
            | NativeFunctionRole::FreeFunction
            | NativeFunctionRole::ExceptionMethod
    )
}

/// Matches local and fully qualified Terlan type spellings.
pub(super) fn terlan_type_matches(value: &str, expected: &str) -> bool {
    value == expected || value.ends_with(&format!(".{expected}"))
}

pub(super) fn copy_cpp_inputs(
    cpp: &CppMetadata,
    build: &CppBuildPlan,
    input_dir: &Path,
    out_dir: &Path,
) -> Result<(), CppBindingError> {
    let header_name = file_name(&cpp.header)?;
    let mut copied_header_names = BTreeSet::from([header_name.clone()]);
    copy_file(
        &input_dir.join(&cpp.header),
        &out_dir.join("native/rust/include").join(header_name),
    )?;
    for header in &build.adapter_headers {
        let name = file_name(header)?;
        if !copied_header_names.insert(name.clone()) {
            return Err(
                (format!("duplicate generated C++ adapter header filename `{name}`")).into(),
            );
        }
        copy_file(
            &input_dir.join(header),
            &out_dir.join("native/rust/include").join(name),
        )?;
    }
    for source in &cpp.sources {
        copy_file(
            &input_dir.join(source),
            &out_dir.join("native/rust/cpp").join(file_name(source)?),
        )?;
    }
    Ok(())
}

/// Renders package, source-root, and native-helper metadata for external use.
pub(super) fn render_terlan_manifest(manifest: &NativeBindingManifest) -> String {
    let helper_env = format!(
        "TERLAN_{}_NATIVE_BOUNDARY_HELPER_PATH",
        manifest.package.namespace.to_ascii_uppercase()
    );
    format!(
        "[package]\nname = {:?}\nversion = \"0.0.0\"\nnamespace = {:?}\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"library\"\n\n[native.rust]\ncrate = {:?}\npath = \"native/rust\"\nhelper = \"native-boundary-helper\"\nhelper_env = {:?}\n",
        manifest.package.crate_name,
        manifest.package.namespace,
        manifest.package.crate_name,
        helper_env,
    )
}

pub(super) fn render_module_source(module: &NativeBindingModule) -> String {
    let mut source = format!(
        "/**\n * {}\n */\n\nmodule {}.\n\n",
        module.documentation, module.module
    );
    for imported in &module.type_imports {
        source.push_str(&format!("import type {imported}.\n"));
    }
    if !module.type_imports.is_empty() {
        source.push('\n');
    }
    for ty in &module.types {
        match ty.kind {
            NativeBindingTypeKind::OpaqueResource => source.push_str(&format!(
                "/** {} */\npub opaque type {}.\n\n",
                ty.documentation, ty.name
            )),
            NativeBindingTypeKind::ValueRecord => {
                source.push_str(&format!(
                    "/** {} */\npub struct {} {{\n",
                    ty.documentation, ty.name
                ));
                for (index, field) in ty.fields.iter().enumerate() {
                    let suffix = if index + 1 == ty.fields.len() {
                        ""
                    } else {
                        ","
                    };
                    source.push_str(&format!("    {}: {}{suffix}\n", field.name, field.ty));
                }
                source.push_str("}.\n\n");
                let constructor = lower_type_name(&ty.name);
                let args = ty
                    .fields
                    .iter()
                    .map(|field| format!("{}: {}", field.name, field.ty))
                    .collect::<Vec<_>>()
                    .join(", ");
                let fields = ty
                    .fields
                    .iter()
                    .map(|field| format!("{}: {}", field.name, field.name))
                    .collect::<Vec<_>>()
                    .join(", ");
                source.push_str(&format!(
                    "/** Constructs one copied {} value. */\npub {constructor}({args}): {} ->\n    {} {{{fields}}}.\n\n",
                    ty.name, ty.name, ty.name
                ));
            }
            NativeBindingTypeKind::Enum => {
                for variant in &ty.variants {
                    source.push_str(&format!(
                        "/** {} */\npub type {} = Atom[{:?}].\n\n",
                        variant.documentation, variant.name, variant.atom
                    ));
                }
                source.push_str(&format!(
                    "/** {} */\npub type {} = {}.\n\n",
                    ty.documentation,
                    ty.name,
                    ty.variants
                        .iter()
                        .map(|variant| format!("Atom[{value:?}]", value = variant.atom))
                        .collect::<Vec<_>>()
                        .join(" | ")
                ));
            }
            NativeBindingTypeKind::StringValue => source.push_str(&format!(
                "/** {} */\npub type {} = String.\n\n",
                ty.documentation, ty.name
            )),
        }
    }
    for function in &module.functions {
        let visibility = if function.visibility.is_public() {
            "pub "
        } else {
            ""
        };
        if let Some(body) = function.terlan_body.as_deref() {
            let body = body
                .lines()
                .map(|line| {
                    if line.is_empty() {
                        String::new()
                    } else {
                        format!("    {line}")
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            source.push_str(&format!(
                "/** {} */\n{visibility}{}({}): {} ->\n{body}.\n\n",
                function.documentation,
                function.name,
                render_args(&function.args),
                function.returns
            ));
        } else {
            source.push_str(&format!(
                "/** {} */\n@compiler.native {{{}}}\n{visibility}{}({}): {} -> native.\n\n",
                function.documentation,
                function.operation,
                function.name,
                render_args(&function.args),
                function.returns
            ));
        }
    }
    source
}

pub(super) fn render_module_docs(
    module: &NativeBindingModule,
    symbols: &BTreeMap<&str, &CppSymbol>,
) -> String {
    let mut docs = format!("# {}\n\n{}\n\n", module.module, module.documentation);
    for function in &module.functions {
        docs.push_str(&format!(
            "## `{}`\n\n{}\n\n",
            function.name, function.documentation
        ));
        if let Some(id) = function.cpp_symbol.as_deref() {
            if let Some(symbol) = symbols.get(id) {
                docs.push_str(&format!("- C++ symbol: `{}`\n", symbol.cpp_name));
            }
        }
        if function.terlan_body.is_some() {
            docs.push_str("- Implementation: generated Terlan composition\n\n");
        } else {
            docs.push_str(&format!(
                "- NativeBoundary operation: `{}`\n- Ownership: `{}`\n\n",
                function.operation,
                resource_policy_name(&function.resource)
            ));
        }
    }
    docs
}

pub(super) fn render_native_boundary_metadata(
    manifest: &NativeBindingManifest,
) -> Result<String, CppBindingError> {
    let null_failure_transport = if manifest.null_failure.is_some() {
        "finite_status_probe"
    } else {
        "generic_null"
    };
    let target = &manifest.cpp_metadata.compile.target_triple;
    let calling_convention =
        crate::runtime::native_boundary::adapter_abi::calling_convention_for_target(target)?;
    let adapter = crate::runtime::native_boundary::adapter_abi::NativeAdapterAbiContract::current()
        .render_metadata(target, calling_convention)?;
    let mut metadata = format!(
        "[package]\nnamespace = {:?}\nadapter = \"cxx\"\ncrate = {:?}\n\n[cpp_metadata]\nschema = {:?}\nproducer = {:?}\nformat = {:?}\ntarget = {:?}\nlanguage_standard = {:?}\nmapping_schema = {:?}\n\n[public_adapter]\n{}handle_scope = \"worker_random_256\"\ncross_owner = \"reject\"\nraw_pointers = false\nexceptions_cross_boundary = false\nnull_failure = {:?}\nnative_failure_payloads = false\n\n",
        manifest.package.namespace,
        manifest.package.crate_name,
        manifest.cpp_metadata.schema,
        manifest.cpp_metadata.producer.name,
        manifest.cpp_metadata.producer.format,
        manifest.cpp_metadata.compile.target_triple,
        manifest.cpp_metadata.compile.language_standard,
        manifest.mapping.schema,
        adapter,
        null_failure_transport
    );
    for module in &manifest.modules {
        for function in module
            .functions
            .iter()
            .filter(|function| function.terlan_body.is_none())
        {
            metadata.push_str(&format!(
                "[functions.{:?}]\noperation = {:?}\narity = {}\nreturns = {:?}\nblocking = {:?}\nresource = {:?}\n\n",
                format!("{}.{}", module.module, function.name),
                function.operation,
                function.args.len(),
                function.returns,
                blocking_policy_name(&function.blocking),
                resource_policy_name(&function.resource)
            ));
        }
    }
    Ok(metadata)
}

pub(super) fn render_rust_adapter_cargo(manifest: &NativeBindingManifest) -> String {
    format!(
        "[package]\nname = {:?}\nversion = \"0.0.0\"\nedition = \"2021\"\npublish = false\nbuild = \"build.rs\"\n\n[lib]\npath = \"src/lib.rs\"\n\n[[bin]]\nname = \"native-boundary-helper\"\npath = \"src/bin/native_boundary_helper.rs\"\n\n[dependencies]\nbase64 = \"0.22.1\"\ncxx = \"={CXX_VERSION}\"\ngetrandom = \"={GETRANDOM_VERSION}\"\n\n[build-dependencies]\ncxx-build = \"={CXX_VERSION}\"\n\n[workspace]\nresolver = \"2\"\n",
        manifest.package.crate_name
    )
}

/// Adapter families whose generated sources must be included in the C++ build.
pub(super) struct CxxBuildAdapters {
    pub(super) enum_adapters: bool,
    pub(super) string_adapters: bool,
    pub(super) collection_adapters: bool,
    pub(super) exception_adapters: bool,
    pub(super) owned_value_adapters: bool,
    pub(super) mutation_adapters: bool,
}

/// Renders the validated plan as a `cxx-build` build script.
pub(super) fn render_cxx_build(
    cpp: &CppMetadata,
    plan: &CppBuildPlan,
    adapters: CxxBuildAdapters,
) -> String {
    let CxxBuildAdapters {
        enum_adapters,
        string_adapters,
        collection_adapters,
        exception_adapters,
        owned_value_adapters,
        mutation_adapters,
    } = adapters;
    let mut build = String::from(
        "fn main() {\n    let root = std::path::PathBuf::from(std::env::var_os(\"CARGO_MANIFEST_DIR\").expect(\"CARGO_MANIFEST_DIR\"));\n    let mut build = cxx_build::bridge(root.join(\"src/lib.rs\"));\n    build.include(&root);\n",
    );
    for source in &cpp.sources {
        build.push_str(&format!(
            "    build.file(root.join({:?}));\n",
            format!(
                "cpp/{}",
                Path::new(source)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
            )
        ));
    }
    if enum_adapters {
        build.push_str("    build.file(root.join(\"cpp/terlan_enum_adapters.cc\"));\n");
    }
    if string_adapters {
        build.push_str("    build.file(root.join(\"cpp/terlan_string_adapters.cc\"));\n");
    }
    if collection_adapters {
        build.push_str("    build.file(root.join(\"cpp/terlan_collection_adapters.cc\"));\n");
    }
    if exception_adapters {
        build.push_str("    build.file(root.join(\"cpp/terlan_exception_adapters.cc\"));\n");
    }
    if owned_value_adapters {
        build.push_str("    build.file(root.join(\"cpp/terlan_owned_value_adapters.cc\"));\n");
    }
    if mutation_adapters {
        build.push_str("    build.file(root.join(\"cpp/terlan_mutation_adapters.cc\"));\n");
    }
    for root in &plan.include_roots {
        build.push_str(&format!("    build.include(root.join({root:?}));\n"));
    }
    render_build_defines(&mut build, "    ", &plan.defines);
    for path in &plan.library_search_paths {
        build.push_str(&format!(
            "    println!(\"cargo:rustc-link-search=native={{}}\", root.join({:?}).display());\n",
            escape_cargo_directive(path)
        ));
    }
    for library in &plan.linked_libraries {
        render_linked_library(&mut build, "    ", library);
    }
    for condition in &plan.platform_conditions {
        let predicate = render_platform_predicate(condition);
        build.push_str(&format!("    if {predicate} {{\n"));
        for root in &condition.include_roots {
            build.push_str(&format!("        build.include(root.join({root:?}));\n"));
        }
        render_build_defines(&mut build, "        ", &condition.defines);
        for path in &condition.library_search_paths {
            build.push_str(&format!(
                "        println!(\"cargo:rustc-link-search=native={{}}\", root.join({:?}).display());\n",
                escape_cargo_directive(path)
            ));
        }
        for library in &condition.linked_libraries {
            render_linked_library(&mut build, "        ", library);
        }
        build.push_str("    }\n");
    }
    for (index, external) in plan.external_roots.iter().enumerate() {
        let variable = format!("external_root_{index}");
        build.push_str(&format!(
            "    println!(\"cargo:rerun-if-env-changed={}\");\n    let {variable} = std::path::PathBuf::from(std::env::var_os({:?}).expect({:?}));\n",
            external.env,
            external.env,
            format!("{} must point at the external C++ SDK root", external.env)
        ));
        for root in &external.include_roots {
            build.push_str(&format!("    build.include({variable}.join({root:?}));\n"));
        }
        for path in &external.library_search_paths {
            build.push_str(&format!(
                "    println!(\"cargo:rustc-link-search=native={{}}\", {variable}.join({:?}).display());\n",
                escape_cargo_directive(path)
            ));
        }
        for library in &external.linked_libraries {
            render_linked_library(&mut build, "    ", library);
        }
        for input in &external.rebuild_inputs {
            build.push_str(&format!(
                "    println!(\"cargo:rerun-if-changed={{}}\", {variable}.join({:?}).display());\n",
                escape_cargo_directive(input)
            ));
        }
    }
    build.push_str(&format!(
        "    build.std({:?}).compile(\"terlan_native_boundary_cxx\");\n",
        cpp.compile.language_standard
    ));
    for input in &plan.rebuild_inputs {
        build.push_str(&format!(
            "    println!(\"cargo:rerun-if-changed={}\");\n",
            escape_cargo_directive(input)
        ));
    }
    build.push_str("}\n");
    build
}

/// Returns whether this package requires generated symbolic enum adapters.
pub(super) fn has_enum_adapters(manifest: &NativeBindingManifest) -> bool {
    manifest.modules.iter().any(|module| {
        module
            .functions
            .iter()
            .any(|function| function.role == NativeFunctionRole::EnumProjection)
    })
}

/// Returns whether this package requires generated copied-value string adapters.
pub(super) fn has_string_adapters(manifest: &NativeBindingManifest) -> bool {
    manifest.modules.iter().any(|module| {
        module
            .functions
            .iter()
            .any(|function| function.role == NativeFunctionRole::StringProjection)
    })
}

/// Returns whether this package copies borrowed C++ collections into Terlan.
pub(super) fn has_collection_adapters(manifest: &NativeBindingManifest) -> bool {
    manifest.modules.iter().any(|module| {
        module.functions.iter().any(|function| {
            matches!(
                function.role,
                NativeFunctionRole::IntListProjection | NativeFunctionRole::ResourceListProjection
            )
        })
    })
}

/// Returns whether this package requires generated exception containment.
pub(super) fn has_exception_adapters(manifest: &NativeBindingManifest) -> bool {
    manifest.modules.iter().any(|module| {
        module.functions.iter().any(|function| {
            matches!(
                function.role,
                NativeFunctionRole::ExceptionMethod
                    | NativeFunctionRole::ScalarProjection
                    | NativeFunctionRole::MutableFreeFunction
            )
        })
    })
}

/// Appends deterministic `cc::Build::define` calls.
pub(super) fn render_build_defines(
    output: &mut String,
    indent: &str,
    defines: &BTreeMap<String, Option<String>>,
) {
    for (name, value) in defines {
        match value {
            Some(value) => output.push_str(&format!(
                "{indent}build.define({name:?}, Some({value:?}));\n"
            )),
            None => output.push_str(&format!("{indent}build.define({name:?}, None::<&str>);\n")),
        }
    }
}

/// Appends one typed Cargo native-link directive.
pub(super) fn render_linked_library(output: &mut String, indent: &str, library: &CppLinkedLibrary) {
    output.push_str(&format!(
        "{indent}println!(\"cargo:rustc-link-lib={}={}\");\n",
        cpp_link_kind_name(library.kind),
        escape_cargo_directive(&library.name)
    ));
}

/// Returns Cargo's spelling for a native link mode.
pub(super) fn cpp_link_kind_name(kind: CppLinkKind) -> &'static str {
    match kind {
        CppLinkKind::Static => "static",
        CppLinkKind::Dynamic => "dylib",
        CppLinkKind::Framework => "framework",
    }
}

/// Renders a conjunction over the condition's Cargo target selectors.
pub(super) fn render_platform_predicate(condition: &CppPlatformCondition) -> String {
    let mut predicates = Vec::new();
    if let Some(value) = &condition.target_os {
        predicates.push(format!(
            "std::env::var(\"CARGO_CFG_TARGET_OS\").as_deref() == Ok({value:?})"
        ));
    }
    if let Some(value) = &condition.target_arch {
        predicates.push(format!(
            "std::env::var(\"CARGO_CFG_TARGET_ARCH\").as_deref() == Ok({value:?})"
        ));
    }
    if let Some(value) = &condition.target_env {
        predicates.push(format!(
            "std::env::var(\"CARGO_CFG_TARGET_ENV\").as_deref() == Ok({value:?})"
        ));
    }
    predicates.join(" && ")
}

/// Escapes a validated value for a generated Rust string literal.
pub(super) fn escape_cargo_directive(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}
