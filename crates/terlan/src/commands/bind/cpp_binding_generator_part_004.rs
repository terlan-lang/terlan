
/// Validates the complete adapter build plan before generation starts.
fn validate_build_plan(build: &CppBuildPlan, input_dir: &Path) -> Result<(), String> {
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
            return Err(format!(
                "duplicate C++ platform condition for os={:?}, arch={:?}, env={:?}",
                selector.0, selector.1, selector.2
            ));
        }
        validate_unique_paths("conditional include root", &condition.include_roots)?;
        validate_defines(&condition.defines)?;
        validate_unique_paths(
            "conditional library search path",
            &condition.library_search_paths,
        )?;
        validate_linked_libraries(&condition.linked_libraries)?;
    }
    Ok(())
}

/// Validates adapter-relative paths and rejects duplicate entries.
fn validate_unique_paths(kind: &str, paths: &[String]) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    for value in paths {
        validate_adapter_path(kind, value)?;
        if !seen.insert(value) {
            return Err(format!("duplicate C++ {kind} `{value}`"));
        }
    }
    Ok(())
}

/// Validates one path relative to the generated adapter root.
fn validate_adapter_path(kind: &str, value: &str) -> Result<(), String> {
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
        return Err(format!(
            "C++ {kind} `{value}` must be relative to the generated adapter root"
        ));
    }
    Ok(())
}

/// Validates preprocessor names and directive-safe optional values.
fn validate_defines(defines: &BTreeMap<String, Option<String>>) -> Result<(), String> {
    for (name, value) in defines {
        if !is_identifier_segment(name) {
            return Err(format!("invalid C++ preprocessor define `{name}`"));
        }
        if value
            .as_deref()
            .is_some_and(|value| value.chars().any(|ch| matches!(ch, '\0' | '\n' | '\r')))
        {
            return Err(format!(
                "C++ preprocessor define `{name}` contains an invalid value"
            ));
        }
    }
    Ok(())
}

/// Validates linked-library names, modes, and uniqueness.
fn validate_linked_libraries(libraries: &[CppLinkedLibrary]) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    for library in libraries {
        if library.name.is_empty()
            || !library
                .name
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | '+'))
        {
            return Err(format!("invalid C++ linked library `{}`", library.name));
        }
        if !seen.insert((library.name.as_str(), cpp_link_kind_name(library.kind))) {
            return Err(format!("duplicate C++ linked library `{}`", library.name));
        }
    }
    Ok(())
}

/// Validates one Cargo target selector token.
fn validate_target_selector(kind: &str, value: &str) -> Result<(), String> {
    if value.is_empty()
        || !value
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '_' | '-'))
    {
        return Err(format!("invalid C++ target {kind} selector `{value}`"));
    }
    Ok(())
}

/// Requires every opaque resource to have a reviewed producer and one disposer.
fn validate_resource_roles(manifest: &NativeBindingManifest) -> Result<(), String> {
    let mut qualified_types = BTreeSet::new();
    for module in &manifest.modules {
        for ty in &module.types {
            let qualified = format!("{}.{}", module.module, ty.name);
            if !qualified_types.insert(qualified.clone()) {
                return Err(format!(
                    "duplicate generated C++ resource type `{qualified}`"
                ));
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
                        && (function.returns == qualified
                            || (owner.module == module.module && function.returns == ty.name))
                })
                .count();
            if producers == 0 {
                return Err(format!(
                    "resource `{qualified}` requires at least one reviewed producer; found 0"
                ));
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
                return Err(format!(
                    "resource `{qualified}` requires exactly one disposer; found {disposers}"
                ));
            }
        }
        for function in &module.functions {
            if matches!(
                function.role,
                NativeFunctionRole::ImmutableMethod
                    | NativeFunctionRole::MutableMethod
                    | NativeFunctionRole::ValueProjection
                    | NativeFunctionRole::EnumProjection
                    | NativeFunctionRole::ExceptionMethod
            ) && !function.args.first().is_some_and(|arg| {
                manifest
                    .modules
                    .iter()
                    .flat_map(|owner| &owner.types)
                    .any(|ty| terlan_type_matches(&arg.ty, &ty.name))
            }) {
                return Err(format!(
                    "C++ method `{}` requires a module-owned resource as its first argument",
                    function.name
                ));
            }
        }
    }
    Ok(())
}

/// Returns whether a public function role may create an owned resource.
fn native_role_can_produce_resource(role: NativeFunctionRole) -> bool {
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
fn terlan_type_matches(value: &str, expected: &str) -> bool {
    value == expected || value.ends_with(&format!(".{expected}"))
}

fn copy_cpp_inputs(
    cpp: &CppMetadata,
    build: &CppBuildPlan,
    input_dir: &Path,
    out_dir: &Path,
) -> Result<(), String> {
    let header_name = file_name(&cpp.header)?;
    let mut copied_header_names = BTreeSet::from([header_name.clone()]);
    copy_file(
        &input_dir.join(&cpp.header),
        &out_dir.join("native/rust/include").join(header_name),
    )?;
    for header in &build.adapter_headers {
        let name = file_name(header)?;
        if !copied_header_names.insert(name.clone()) {
            return Err(format!(
                "duplicate generated C++ adapter header filename `{name}`"
            ));
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
fn render_terlan_manifest(manifest: &NativeBindingManifest) -> String {
    format!(
        "[package]\nname = {:?}\nversion = \"0.0.0\"\nnamespace = {:?}\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"library\"\n\n[native.rust]\ncrate = {:?}\npath = \"native/rust\"\nhelper = \"native-boundary-helper\"\nhelper_env = \"TERLAN_NATIVE_BOUNDARY_HELPER_PATH\"\n",
        manifest.package.crate_name,
        manifest.package.namespace,
        manifest.package.crate_name
    )
}

fn render_module_source(module: &NativeBindingModule) -> String {
    let mut source = format!(
        "/**\n * {}\n */\n\nmodule {}.\n\n",
        module.documentation, module.module
    );
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
                        "/** {} */\npub type {}.\n\n",
                        variant.documentation, variant.name
                    ));
                }
                source.push_str(&format!(
                    "/** {} */\npub type {} = {}.\n\n",
                    ty.documentation,
                    ty.name,
                    ty.variants
                        .iter()
                        .map(|variant| variant.name.as_str())
                        .collect::<Vec<_>>()
                        .join(" | ")
                ));
            }
        }
    }
    for function in &module.functions {
        source.push_str(&format!(
            "/** {} */\n@compiler.native {{{}}}\npub {}({}): {} -> native.\n\n",
            function.documentation,
            function.operation,
            function.name,
            render_args(&function.args),
            function.returns
        ));
    }
    source
}

fn render_module_docs(
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
        docs.push_str(&format!(
            "- NativeBoundary operation: `{}`\n- Ownership: `{}`\n\n",
            function.operation,
            resource_policy_name(&function.resource)
        ));
    }
    docs
}

fn render_native_boundary_metadata(manifest: &NativeBindingManifest) -> Result<String, String> {
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
        for function in &module.functions {
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

fn render_rust_adapter_cargo(manifest: &NativeBindingManifest) -> String {
    format!(
        "[package]\nname = {:?}\nversion = \"0.0.0\"\nedition = \"2021\"\npublish = false\nbuild = \"build.rs\"\n\n[lib]\npath = \"src/lib.rs\"\n\n[[bin]]\nname = \"native-boundary-helper\"\npath = \"src/bin/native_boundary_helper.rs\"\n\n[dependencies]\nbase64 = \"0.22.1\"\ncxx = \"={CXX_VERSION}\"\ngetrandom = \"={GETRANDOM_VERSION}\"\n\n[build-dependencies]\ncxx-build = \"={CXX_VERSION}\"\n\n[workspace]\nresolver = \"2\"\n",
        manifest.package.crate_name
    )
}

/// Renders the validated plan as a `cxx-build` build script.
fn render_cxx_build(
    cpp: &CppMetadata,
    plan: &CppBuildPlan,
    enum_adapters: bool,
    exception_adapters: bool,
) -> String {
    let mut build = String::from(
        "fn main() {\n    let root = std::path::PathBuf::from(std::env::var_os(\"CARGO_MANIFEST_DIR\").expect(\"CARGO_MANIFEST_DIR\"));\n    let mut build = cxx_build::bridge(root.join(\"src/lib.rs\"));\n",
    );
    for source in &cpp.sources {
        build.push_str(&format!(
            "    build.file({:?});\n",
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
        build.push_str("    build.file(\"cpp/terlan_enum_adapters.cc\");\n");
    }
    if exception_adapters {
        build.push_str("    build.file(\"cpp/terlan_exception_adapters.cc\");\n");
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
fn has_enum_adapters(manifest: &NativeBindingManifest) -> bool {
    manifest.modules.iter().any(|module| {
        module
            .functions
            .iter()
            .any(|function| function.role == NativeFunctionRole::EnumProjection)
    })
}

/// Returns whether this package requires generated exception containment.
fn has_exception_adapters(manifest: &NativeBindingManifest) -> bool {
    manifest.modules.iter().any(|module| {
        module
            .functions
            .iter()
            .any(|function| function.role == NativeFunctionRole::ExceptionMethod)
    })
}

/// Appends deterministic `cc::Build::define` calls.
fn render_build_defines(
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
fn render_linked_library(output: &mut String, indent: &str, library: &CppLinkedLibrary) {
    output.push_str(&format!(
        "{indent}println!(\"cargo:rustc-link-lib={}={}\");\n",
        cpp_link_kind_name(library.kind),
        escape_cargo_directive(&library.name)
    ));
}

/// Returns Cargo's spelling for a native link mode.
fn cpp_link_kind_name(kind: CppLinkKind) -> &'static str {
    match kind {
        CppLinkKind::Static => "static",
        CppLinkKind::Dynamic => "dylib",
        CppLinkKind::Framework => "framework",
    }
}

/// Renders a conjunction over the condition's Cargo target selectors.
fn render_platform_predicate(condition: &CppPlatformCondition) -> String {
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
fn escape_cargo_directive(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn render_cxx_bridge(
    manifest: &NativeBindingManifest,
    symbols: &ValidatedCppSymbols<'_>,
) -> Result<String, String> {
    let header = file_name(&manifest.cpp_metadata.header)?;
    let mut source = format!(
        "#![deny(unsafe_op_in_unsafe_fn)]\n\n#[cxx::bridge(namespace = {:?})]\npub mod ffi {{\n    unsafe extern \"C++\" {{\n        include!({:?});\n",
        manifest.cpp_metadata.namespace,
        format!("include/{header}")
    );
    if has_enum_adapters(manifest) {
        source.push_str("        include!(\"include/terlan_enum_adapters.hpp\");\n");
    }
    if has_exception_adapters(manifest) {
        source.push_str("        include!(\"include/terlan_exception_adapters.hpp\");\n");
    }
    let mut opaque_symbols = manifest
        .modules
        .iter()
        .flat_map(|module| &module.types)
        .filter(|ty| ty.kind == NativeBindingTypeKind::OpaqueResource)
        .map(|ty| ty.cpp_symbol.as_str())
        .collect::<BTreeSet<_>>();
    for module in &manifest.modules {
        for function in module
            .functions
            .iter()
            .filter(|function| function.role == NativeFunctionRole::OwnedValueProjection)
        {
            let record = manifest
                .modules
                .iter()
                .flat_map(|owner| &owner.types)
                .find(|ty| {
                    ty.kind == NativeBindingTypeKind::ValueRecord
                        && terlan_type_matches(&function.returns, &ty.name)
                })
                .ok_or_else(|| {
                    format!(
                        "owned value projection `{}` has no returned record",
                        function.name
                    )
                })?;
            opaque_symbols.insert(record.cpp_symbol.as_str());
        }
    }
    for symbol in symbols
        .declarations
        .values()
        .filter(|symbol| opaque_symbols.contains(symbol.id.as_str()))
    {
        source.push_str(&format!("        type {};\n", symbol.cpp_name));
    }
    let adapted_enum_symbols = manifest
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .filter(|function| function.role == NativeFunctionRole::EnumProjection)
        .filter_map(|function| function.cpp_symbol.as_deref())
        .collect::<BTreeSet<_>>();
    let contained_exception_symbols = manifest
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .filter(|function| function.role == NativeFunctionRole::ExceptionMethod)
        .filter_map(|function| function.cpp_symbol.as_deref())
        .collect::<BTreeSet<_>>();
    for symbol in symbols.declarations.values().filter(|symbol| {
        symbols.is_bindable(&symbol.id)
            && !matches!(symbol.kind, CppSymbolKind::Record | CppSymbolKind::Enum)
            && !adapted_enum_symbols.contains(symbol.id.as_str())
            && !contained_exception_symbols.contains(symbol.id.as_str())
    }) {
        source.push_str(&render_bridge_function(symbol)?);
    }
    if has_exception_adapters(manifest) {
        source.push_str(&format!(
            "        type {EXCEPTION_ENVELOPE};\n        fn is_ok(self: &{EXCEPTION_ENVELOPE}) -> bool;\n        fn value(self: &{EXCEPTION_ENVELOPE}) -> i64;\n        fn code(self: &{EXCEPTION_ENVELOPE}) -> &CxxString;\n        fn message(self: &{EXCEPTION_ENVELOPE}) -> &CxxString;\n"
        ));
        for module in &manifest.modules {
            for function in module
                .functions
                .iter()
                .filter(|function| function.role == NativeFunctionRole::ExceptionMethod)
            {
                let resource = function
                    .args
                    .first()
                    .and_then(|arg| {
                        module.types.iter().find(|ty| {
                            ty.kind == NativeBindingTypeKind::OpaqueResource
                                && terlan_type_matches(&arg.ty, &ty.name)
                        })
                    })
                    .ok_or_else(|| {
                        format!("exception method `{}` has no resource", function.name)
                    })?;
                let resource_symbol = symbols
                    .declarations
                    .get(resource.cpp_symbol.as_str())
                    .ok_or_else(|| {
                        format!("exception method `{}` has unknown resource", function.name)
                    })?;
                let args = std::iter::once(format!("value: &{}", resource_symbol.cpp_name))
                    .chain(
                        function
                            .args
                            .iter()
                            .skip(1)
                            .map(|arg| format!("{}: i64", arg.name)),
                    )
                    .collect::<Vec<_>>()
                    .join(", ");
                source.push_str(&format!(
                    "        fn {}({args}) -> UniquePtr<{EXCEPTION_ENVELOPE}>;\n",
                    exception_adapter_name(module, function)
                ));
            }
        }
    }
    for module in &manifest.modules {
        for function in module
            .functions
            .iter()
            .filter(|function| function.role == NativeFunctionRole::EnumProjection)
        {
            let resource = function
                .args
                .first()
                .and_then(|arg| {
                    module.types.iter().find(|ty| {
                        ty.kind == NativeBindingTypeKind::OpaqueResource
                            && terlan_type_matches(&arg.ty, &ty.name)
                    })
                })
                .ok_or_else(|| format!("enum projection `{}` has no resource", function.name))?;
            let resource_symbol = symbols
                .declarations
                .get(resource.cpp_symbol.as_str())
                .ok_or_else(|| {
                    format!("enum projection `{}` has unknown resource", function.name)
                })?;
            source.push_str(&format!(
                "        fn {}(value: &{}) -> UniquePtr<CxxString>;\n",
                enum_adapter_name(module, function),
                resource_symbol.cpp_name
            ));
        }
    }
    source.push_str("    }\n}\n");
    Ok(source)
}

fn render_bridge_function(symbol: &CppSymbol) -> Result<String, String> {
    let mut args = Vec::new();
    if symbol.kind == CppSymbolKind::Method {
        let receiver = symbol
            .receiver
            .as_deref()
            .ok_or_else(|| format!("C++ method `{}` has no receiver", symbol.id))?;
        if symbol.receiver_mutable {
            args.push(format!("self: Pin<&mut {}>", cpp_short_name(receiver)));
        } else {
            args.push(format!("self: &{}", cpp_short_name(receiver)));
        }
    }
    for parameter in &symbol.parameters {
        args.push(format!(
            "{}: {}",
            parameter.name,
            rust_bridge_type(&parameter.ty)?
        ));
    }
    let return_text = match &symbol.returns {
        Some(returns) if returns.canonical != "void" => {
            format!(" -> {}", rust_bridge_type(returns)?)
        }
        _ => String::new(),
    };
    Ok(format!(
        "        fn {}({}){};\n",
        symbol.cpp_name,
        args.join(", "),
        return_text
    ))
}

fn rust_bridge_type(cpp_type: &CppTypeMetadata) -> Result<String, String> {
    if is_i64_type(cpp_type) {
        return Ok("i64".into());
    }
    if is_rust_str_type(cpp_type) {
        return Ok("&str".into());
    }
    if is_u8_slice_type(cpp_type) {
        return Ok("&[u8]".into());
    }
    if is_i64_slice_type(cpp_type) {
        return Ok("&[i64]".into());
    }
    if is_f64_slice_type(cpp_type) {
        return Ok("&[f64]".into());
    }
    if is_owned_string_type(cpp_type) {
        return Ok("UniquePtr<CxxString>".into());
    }
    if is_owned_u8_vector_type(cpp_type) {
        return Ok("UniquePtr<CxxVector<u8>>".into());
    }
    if is_owned_i64_vector_type(cpp_type) {
        return Ok("UniquePtr<CxxVector<i64>>".into());
    }
    if is_owned_f64_vector_type(cpp_type) {
        return Ok("UniquePtr<CxxVector<f64>>".into());
    }
    if let Some(record) = borrowed_const_record_name(cpp_type) {
        return Ok(format!("&{}", cpp_short_name(record)));
    }
    match cpp_type.canonical.as_str() {
        "double" => return Ok("f64".into()),
        "bool" => return Ok("bool".into()),
        _ => {}
    }
    if let Some(inner) = cpp_type
        .canonical
        .strip_prefix("std::unique_ptr<")
        .and_then(|value| value.strip_suffix('>'))
    {
        let cpp_name = cpp_short_name(inner);
        return Ok(format!("UniquePtr<{cpp_name}>"));
    }
    Err(format!(
        "error[cpp.type.unmapped]: C++ type `{}` (canonical `{}`) has no cxx bridge mapping",
        cpp_type.spelling, cpp_type.canonical
    ))
}
