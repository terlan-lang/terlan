use super::*;

#[test]
pub(super) fn bindable_unsafe_c_shapes_fail_with_stable_diagnostic_families() {
    let cases: &[(&str, &str, fn(&mut Value))] = &[
        (
            "pointer",
            "native_bindgen.c_pointer_ownership_unknown",
            |metadata| {
                symbol_mut(metadata, "function.native_boundary_add")["parameters"][0]
                    .as_object_mut()
                    .expect("parameter")
                    .remove("ownership");
            },
        ),
        (
            "lifetime",
            "native_bindgen.c_borrowed_lifetime",
            |metadata| {
                symbol_mut(metadata, "function.native_boundary_live_count")["returns"] =
                    Value::String("const int64_t *".to_string());
            },
        ),
        (
            "destructor",
            "native_bindgen.c_missing_destructor",
            |metadata| {
                symbol_mut(metadata, "record.native_boundary")
                    .as_object_mut()
                    .expect("record")
                    .remove("destructor_symbol");
            },
        ),
        (
            "callback",
            "native_bindgen.c_unsupported_callback",
            |metadata| {
                symbol_mut(metadata, "function.native_boundary_add")["callback"] =
                    Value::Bool(true);
            },
        ),
        (
            "variadic",
            "native_bindgen.c_unsupported_variadic_function",
            |metadata| {
                symbol_mut(metadata, "function.native_boundary_add")["variadic"] =
                    Value::Bool(true);
            },
        ),
        (
            "abi_version",
            "native_bindgen.c_abi_version_missing",
            |metadata| metadata["c_metadata"]["abi_version"] = Value::Number(0.into()),
        ),
        (
            "unmapped_type",
            "native_bindgen.c_type_unmapped",
            |metadata| {
                symbol_mut(metadata, "function.native_boundary_add")["parameters"][1]["c_type"] =
                    Value::String("TerlanCMystery".to_string());
            },
        ),
        (
            "borrowed_array_metadata",
            "native_bindgen.c_borrowed_lifetime",
            |metadata| {
                symbol_mut(metadata, "function.native_boundary_samples")["parameters"][1]
                    .as_object_mut()
                    .expect("array parameter")
                    .remove("borrowed_array");
            },
        ),
        (
            "borrowed_array_owner",
            "native_bindgen.c_borrowed_array_contract",
            |metadata| {
                symbol_mut(metadata, "function.native_boundary_samples")["parameters"][1]
                    ["borrowed_array"]["owner_parameter"] =
                    Value::String("missing_owner".to_string());
            },
        ),
        (
            "borrowed_array_length",
            "native_bindgen.c_borrowed_array_contract",
            |metadata| {
                symbol_mut(metadata, "function.native_boundary_samples")["parameters"][1]
                    ["borrowed_array"]["length_symbol"] =
                    Value::String("function.missing_length".to_string());
            },
        ),
    ];

    for (name, family, mutate) in cases {
        let manifest = write_fixture_variant(name, *mutate);
        let out_dir = temp_dir(&format!("{name}_out"));
        let error = generate_c_abi_bindings(&manifest, &out_dir).expect_err("shape must fail");
        assert!(
            error.contains(&format!("error[{family}]")),
            "unexpected {name} diagnostic: {error}"
        );
        fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    }
}

#[test]
pub(super) fn structured_c_metadata_rejects_unmaintained_producer_and_unknown_skip_shape() {
    let producer_manifest = write_fixture_variant("producer", |metadata| {
        metadata["c_metadata"]["producer"]["name"] = Value::String("handwritten".into());
    });
    let error = generate_c_abi_bindings(&producer_manifest, &temp_dir("producer_out"))
        .expect_err("producer must fail");
    assert!(error.contains("expected maintained tooling `clang-libtooling`"));
    fs::remove_dir_all(producer_manifest.parent().expect("producer root"))
        .expect("remove producer variant");

    let shape_manifest = write_fixture_variant("shape", |metadata| {
        symbol_mut(metadata, "unsupported.union")["unsupported_shape"] =
            Value::String("free_form_reason".into());
    });
    let error = generate_c_abi_bindings(&shape_manifest, &temp_dir("shape_out"))
        .expect_err("unknown shape must fail");
    assert!(error.contains("unknown variant `free_form_reason`"));
    fs::remove_dir_all(shape_manifest.parent().expect("shape root")).expect("remove shape variant");
}

/// Verifies malformed or non-trailing Terlan defaults fail before source
/// generation with one stable diagnostic family.
#[test]
pub(super) fn structured_c_metadata_rejects_invalid_terlan_argument_defaults() {
    let cases: &[(&str, fn(&mut Value))] = &[
        ("empty_default", |metadata| {
            metadata["modules"][0]["functions"][6]["args"][1]["default"] =
                Value::String("  ".to_string());
        }),
        ("multiline_default", |metadata| {
            metadata["modules"][0]["functions"][6]["args"][1]["default"] =
                Value::String("0\n+ 1".to_string());
        }),
        ("required_after_default", |metadata| {
            metadata["modules"][0]["functions"][2]["args"][0]["default"] =
                Value::String("missing".to_string());
        }),
    ];

    for (name, mutate) in cases {
        let manifest = write_fixture_variant(name, *mutate);
        let out_dir = temp_dir(&format!("{name}_out"));
        let error = generate_c_abi_bindings(&manifest, &out_dir)
            .expect_err("invalid Terlan default must fail");
        assert!(
            error.contains("error[native_bindgen.terlan_default_argument]"),
            "unexpected {name} diagnostic: {error}"
        );
        fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    }
}

/// Verifies generated public and Rust adapter identities remain unique.
#[test]
pub(super) fn structured_c_metadata_rejects_binding_identity_collisions() {
    let cases: &[(&str, &str, fn(&mut Value))] = &[
        (
            "terlan_overload_collision",
            "native_bindgen.terlan_overload_collision",
            |metadata| {
                metadata["modules"][0]["functions"][1]["name"] = Value::String("new".to_string());
                metadata["modules"][0]["functions"][1]["args"][0]["ty"] =
                    Value::String("Int".to_string());
            },
        ),
        (
            "adapter_name_collision",
            "native_bindgen.adapter_name_collision",
            |metadata| {
                metadata["modules"][0]["functions"][7]["adapter_name"] =
                    Value::String("unsqueeze".to_string());
            },
        ),
    ];

    for (name, family, mutate) in cases {
        let manifest = write_fixture_variant(name, *mutate);
        let out_dir = temp_dir(&format!("{name}_out"));
        let error = generate_c_abi_bindings(&manifest, &out_dir)
            .expect_err("ambiguous binding identity must fail");
        assert!(
            error.contains(&format!("error[{family}]")),
            "unexpected {name} diagnostic: {error}"
        );
        fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    }
}

/// Verifies generated Terlan APIs retain same-name, same-arity overloads when
/// their public parameter types are distinct.
#[test]
pub(super) fn structured_c_metadata_accepts_type_distinct_terlan_overloads() {
    let manifest = write_fixture_variant("typed_overload", |metadata| {
        metadata["modules"][0]["functions"][1]["name"] = Value::String("new".to_string());
        metadata["modules"][0]["functions"][1]["adapter_name"] =
            Value::String("read_new".to_string());
    });
    let out_dir = temp_dir("typed_overload_out");
    generate_c_abi_bindings(&manifest, &out_dir).expect("typed overload must generate");
    let source = fs::read_to_string(out_dir.join("src/c_abi_fixture/NativeBoundary.terl"))
        .expect("read generated Terlan overloads");
    assert!(source.contains("pub new(value: Int): NativeBoundary"));
    assert!(source.contains("pub new(boundary: NativeBoundary): Int"));
    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

/// Verifies a nominal Terlan argument can retain its public type while using a
/// reviewed primitive boundary representation.
#[test]
pub(super) fn structured_c_metadata_separates_public_and_abi_argument_types() {
    let manifest = write_fixture_variant("public_abi_type", |metadata| {
        metadata["modules"][0]["functions"][2]["args"][1]["ty"] =
            Value::String("ExampleCode".to_string());
        metadata["modules"][0]["functions"][2]["args"][1]["abi_ty"] =
            Value::String("Int".to_string());
        metadata["modules"][0]["functions"][2]["visibility"] = Value::String("private".to_string());
        metadata["modules"][0]["imports"] = serde_json::json!([
            {"module": "std.core.Int", "names": ["to_string"]}
        ]);
    });
    let out_dir = temp_dir("public_abi_type_out");
    generate_c_abi_bindings(&manifest, &out_dir).expect("typed ABI generation");
    let module = fs::read_to_string(out_dir.join("src/c_abi_fixture/NativeBoundary.terl"))
        .expect("generated Terlan module");
    let adapter =
        fs::read_to_string(out_dir.join("native/rust/src/lib.rs")).expect("generated Rust adapter");
    let helper = fs::read_to_string(out_dir.join("native/rust/src/bin/native_boundary_helper.rs"))
        .expect("generated helper");
    assert!(module.contains("delta: ExampleCode"));
    assert!(module.contains("import std.core.Int.{to_string}."));
    assert!(module.contains("\nadd(boundary: NativeBoundary"));
    assert!(!module.contains("\npub add(boundary: NativeBoundary"));
    assert!(adapter.contains("delta: i64"));
    assert!(helper.contains("Arg::Int(delta)"));
    let consumer = fs::read_to_string(out_dir.join("tests/c_abi_fixture/NativeBoundaryTest.terl"))
        .expect("generated consumer");
    assert!(!consumer.contains("add"));
    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
}

#[test]
pub(super) fn dispatcher_metadata_rejects_ambiguous_or_unsupported_stack_contracts() {
    let cases: &[(&str, &str, fn(&mut Value))] = &[
        (
            "dispatcher_abi",
            "native_bindgen.c_dispatcher_contract",
            |metadata| {
                metadata["modules"][0]["functions"][5]["dispatcher"]["extension_abi_version"] =
                    Value::String("1".to_string());
            },
        ),
        (
            "dispatcher_overload",
            "native_bindgen.c_dispatcher_contract",
            |metadata| {
                metadata["modules"][0]["functions"][5]["dispatcher"]["overload_name"] =
                    Value::String("bad\0overload".to_string());
            },
        ),
        (
            "dispatcher_argument",
            "native_bindgen.c_dispatcher_contract",
            |metadata| {
                metadata["modules"][0]["functions"][5]["dispatcher"]["stack"][0]["argument"] =
                    Value::String("unknown".to_string());
            },
        ),
        (
            "dispatcher_stack_kind",
            "native_bindgen.c_dispatcher_contract",
            |metadata| {
                metadata["modules"][0]["functions"][5]["dispatcher"]["stack"][1]["kind"] =
                    Value::String("raw_pointer".to_string());
            },
        ),
        (
            "dispatcher_int_argument",
            "native_bindgen.c_dispatcher_contract",
            |metadata| {
                metadata["modules"][0]["functions"][6]["dispatcher"]["stack"][1]["argument"] =
                    Value::String("unknown".to_string());
            },
        ),
        (
            "dispatcher_bool_argument",
            "native_bindgen.c_dispatcher_contract",
            |metadata| {
                metadata["modules"][0]["functions"][6]["args"][1]["ty"] =
                    Value::String("Bool".to_string());
                metadata["modules"][0]["functions"][6]["dispatcher"]["stack"][1]["kind"] =
                    Value::String("bool_argument".to_string());
                metadata["modules"][0]["functions"][6]["dispatcher"]["stack"][1]["argument"] =
                    Value::String("unknown".to_string());
            },
        ),
        (
            "dispatcher_float_argument",
            "native_bindgen.c_dispatcher_contract",
            |metadata| {
                metadata["modules"][0]["functions"][6]["args"][1]["ty"] =
                    Value::String("Float".to_string());
                metadata["modules"][0]["functions"][6]["dispatcher"]["stack"][1]["kind"] =
                    Value::String("float_argument".to_string());
                metadata["modules"][0]["functions"][6]["dispatcher"]["stack"][1]["argument"] =
                    Value::String("unknown".to_string());
            },
        ),
        (
            "dispatcher_optional_allocator",
            "native_bindgen.c_dispatcher_contract",
            |metadata| {
                metadata["modules"][0]["functions"][6]["dispatcher"]["stack"][1]["kind"] =
                    Value::String("owned_optional_int_argument".to_string());
            },
        ),
        (
            "dispatcher_list_allocator",
            "native_bindgen.c_dispatcher_contract",
            |metadata| {
                metadata["modules"][0]["functions"][6]["args"][1]["ty"] =
                    Value::String("List[Int]".to_string());
                metadata["modules"][0]["functions"][6]["dispatcher"]["stack"][1]["kind"] =
                    Value::String("owned_int_list_argument".to_string());
            },
        ),
        (
            "dispatcher_string_allocator",
            "native_bindgen.c_dispatcher_contract",
            |metadata| {
                metadata["modules"][0]["functions"][5]["dispatcher"]["stack"][1] =
                    serde_json::json!({"kind": "owned_string_literal", "value": "none"});
            },
        ),
        (
            "dispatcher_duplicate_handle_argument",
            "native_bindgen.c_dispatcher_contract",
            |metadata| {
                metadata["modules"][0]["functions"][7]["dispatcher"]["stack"][1]["argument"] =
                    Value::String("left".to_string());
            },
        ),
        (
            "dispatcher_output_slot",
            "native_bindgen.c_dispatcher_contract",
            |metadata| {
                metadata["modules"][0]["functions"][7]["dispatcher"]["output"]["index"] =
                    Value::Number(1.into());
            },
        ),
    ];
    for (name, family, mutate) in cases {
        let manifest = write_fixture_variant(name, *mutate);
        let error = generate_c_abi_bindings(&manifest, &temp_dir(&format!("{name}_out")))
            .expect_err("dispatcher contract must fail");
        assert!(
            error.contains(&format!("error[{family}]")),
            "unexpected {name} diagnostic: {error}"
        );
        fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    }
}

#[test]
pub(super) fn external_c_distribution_aliases_link_without_bundled_sources() {
    let manifest = write_fixture_variant("external_link", |metadata| {
        metadata["c_metadata"]["sources"] = Value::Array(Vec::new());
        metadata["c_metadata"]["header"] = Value::String("include/native_boundary.h".to_string());
        metadata["c_metadata"]["aliases"] = serde_json::json!({
            "FixtureHandle": "TerlanCNativeBoundary *",
            "FixtureStatus": "int32_t"
        });
        metadata["c_metadata"]["external_link"] = serde_json::json!({
            "root_env": "TERLAN_C_ABI_TEST_ROOT",
            "include_dirs": ["include"],
            "library_dirs": ["lib"],
            "libraries": ["terlan_external_fixture"],
            "runtime_library_dirs": ["lib"]
        });
        for id in [
            "function.native_boundary_create",
            "function.native_boundary_value",
            "function.native_boundary_add",
            "function.native_boundary_destroy",
        ] {
            symbol_mut(metadata, id)["returns"] = Value::String("FixtureStatus".to_string());
        }
        symbol_mut(metadata, "function.native_boundary_create")["parameters"][1]["c_type"] =
            Value::String("FixtureHandle *".to_string());
        symbol_mut(metadata, "function.native_boundary_value")["parameters"][0]["c_type"] =
            Value::String("FixtureHandle".to_string());
        symbol_mut(metadata, "function.native_boundary_add")["parameters"][0]["c_type"] =
            Value::String("FixtureHandle".to_string());
        symbol_mut(metadata, "function.native_boundary_destroy")["parameters"][0]["c_type"] =
            Value::String("FixtureHandle".to_string());
    });
    let out_dir = temp_dir("external_link_out");
    let distribution = temp_dir("external_distribution");
    let library_dir = distribution.join("lib");
    let include_dir = distribution.join("include");
    fs::create_dir_all(&library_dir).expect("create external library directory");
    fs::create_dir_all(&include_dir).expect("create external include directory");
    fs::copy(
        fixture_dir().join("native_boundary.h"),
        include_dir.join("native_boundary.h"),
    )
    .expect("copy external fixture header");
    let library = library_dir.join("libterlan_external_fixture.so");
    let compile = std::process::Command::new("cc")
        .args(["-shared", "-fPIC"])
        .arg(fixture_dir().join("native_boundary.c"))
        .arg("-I")
        .arg(&distribution)
        .arg("-o")
        .arg(&library)
        .output()
        .expect("compile external fixture library");
    assert!(
        compile.status.success(),
        "external fixture compilation failed: {}",
        String::from_utf8_lossy(&compile.stderr)
    );

    generate_c_abi_bindings(&manifest, &out_dir).expect("generate external C ABI package");
    assert!(!out_dir.join("native/rust/c").exists());
    let cargo = fs::read_to_string(out_dir.join("native/rust/Cargo.toml")).expect("Cargo.toml");
    assert!(!cargo.contains("cc ="));
    let build = fs::read_to_string(out_dir.join("native/rust/build.rs")).expect("build.rs");
    assert!(build.contains("TERLAN_C_ABI_TEST_ROOT"));
    assert!(build.contains("rustc-link-lib=dylib=terlan_external_fixture"));
    let adapter = fs::read_to_string(out_dir.join("native/rust/src/lib.rs")).expect("adapter");
    assert!(adapter.contains("out_boundary: *mut *mut TerlanCNativeBoundary"));
    assert!(adapter.contains("-> i32"));

    let target_dir = temp_dir("external_link_target");
    let build_output =
        std::process::Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string()))
            .args(["build", "--manifest-path"])
            .arg(out_dir.join("native/rust/Cargo.toml"))
            .args(["--offline", "--quiet", "--bin", "native-boundary-helper"])
            .env("CARGO_TARGET_DIR", &target_dir)
            .env("TERLAN_C_ABI_TEST_ROOT", &distribution)
            .output()
            .expect("link generated adapter to external fixture");
    assert!(
        build_output.status.success(),
        "external adapter build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build_output.stdout),
        String::from_utf8_lossy(&build_output.stderr)
    );

    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
    fs::remove_dir_all(distribution).expect("remove external distribution");
    fs::remove_dir_all(target_dir).expect("remove target");
}

#[test]
pub(super) fn external_c_distribution_can_compile_package_owned_adapter_sources() {
    let manifest = write_fixture_variant("external_link_with_adapter", |metadata| {
        metadata["c_metadata"]["sources"] = serde_json::json!(["external_adapter.c"]);
        metadata["c_metadata"]["header"] = Value::String("include/native_boundary.h".to_string());
        metadata["c_metadata"]["external_link"] = serde_json::json!({
            "root_env": "TERLAN_C_ABI_TEST_ROOT",
            "include_dirs": ["include"],
            "library_dirs": ["lib"],
            "libraries": ["terlan_external_fixture"],
            "runtime_library_dirs": ["lib"]
        });
    });
    fs::write(
        manifest
            .parent()
            .expect("variant root")
            .join("external_adapter.c"),
        "/* package-owned adapter source */\n",
    )
    .expect("write adapter source");
    let out_dir = temp_dir("external_link_with_adapter_out");

    generate_c_abi_bindings(&manifest, &out_dir)
        .expect("generate externally linked package with adapter source");
    assert!(out_dir.join("native/rust/c/external_adapter.c").is_file());
    let cargo = fs::read_to_string(out_dir.join("native/rust/Cargo.toml")).expect("Cargo.toml");
    assert!(cargo.contains("cc = \"=1.2.67\""));
    let build = fs::read_to_string(out_dir.join("native/rust/build.rs")).expect("build.rs");
    assert!(build.contains("c_build.file(\"c/external_adapter.c\")"));
    assert!(build.contains("c_build.include(root.join(\"include\"))"));
    assert!(build.contains("rustc-link-lib=dylib=terlan_external_fixture"));

    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
pub(super) fn package_owned_rust_extension_is_copied_and_declares_pinned_dependencies() {
    let manifest = write_fixture_variant("rust_extension", |metadata| {
        metadata["package"]["rust_extension"] = serde_json::json!({
            "source": "package_extension.rs",
            "support_sources": ["extension_support.rs"],
            "dependencies": {"polling": "3.11.0"}
        });
    });
    fs::write(
        manifest
            .parent()
            .expect("variant root")
            .join("package_extension.rs"),
        "pub fn extension_probe() -> bool { true }\n",
    )
    .expect("write Rust extension source");
    fs::write(
        manifest
            .parent()
            .expect("variant root")
            .join("extension_support.rs"),
        "pub fn support_probe() -> bool { true }\n",
    )
    .expect("write Rust extension support source");
    let out_dir = temp_dir("rust_extension_out");

    generate_c_abi_bindings(&manifest, &out_dir).expect("generate Rust extension package");

    assert_eq!(
        fs::read_to_string(out_dir.join("native/rust/src/package_extension.rs"))
            .expect("copied Rust extension"),
        "pub fn extension_probe() -> bool { true }\n"
    );
    assert_eq!(
        fs::read_to_string(out_dir.join("native/rust/src/extension_support.rs"))
            .expect("copied Rust extension support source"),
        "pub fn support_probe() -> bool { true }\n"
    );
    let cargo = fs::read_to_string(out_dir.join("native/rust/Cargo.toml")).expect("Cargo.toml");
    assert!(cargo.contains("polling = \"3.11.0\""));
    let adapter = fs::read_to_string(out_dir.join("native/rust/src/lib.rs")).expect("adapter");
    assert!(adapter.contains("mod package_extension;"));
    assert!(adapter.contains("pub use package_extension::*;"));

    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
pub(super) fn package_owned_rust_extension_rejects_escaping_sources_and_unpinned_dependencies() {
    let escaping = write_fixture_variant("rust_extension_escape", |metadata| {
        metadata["package"]["rust_extension"] = serde_json::json!({
            "source": "../escape.rs"
        });
    });
    let error = generate_c_abi_bindings(&escaping, &temp_dir("rust_extension_escape_out"))
        .expect_err("escaping extension source must fail");
    assert!(error.contains("must be a package-relative file"), "{error}");
    fs::remove_dir_all(escaping.parent().expect("variant root")).expect("remove variant");

    let escaping_support = write_fixture_variant("rust_extension_support_escape", |metadata| {
        metadata["package"]["rust_extension"] = serde_json::json!({
            "source": "package_extension.rs",
            "support_sources": ["../escape.rs"]
        });
    });
    fs::write(
        escaping_support
            .parent()
            .expect("variant root")
            .join("package_extension.rs"),
        "pub fn extension_probe() -> bool { true }\n",
    )
    .expect("write Rust extension source");
    let error = generate_c_abi_bindings(
        &escaping_support,
        &temp_dir("rust_extension_support_escape_out"),
    )
    .expect_err("escaping support source must fail");
    assert!(error.contains("must be a package-relative file"), "{error}");
    fs::remove_dir_all(escaping_support.parent().expect("variant root")).expect("remove variant");

    let unpinned = write_fixture_variant("rust_extension_unpinned", |metadata| {
        metadata["package"]["rust_extension"] = serde_json::json!({
            "source": "package_extension.rs",
            "dependencies": {"polling": "^3.11"}
        });
    });
    fs::write(
        unpinned
            .parent()
            .expect("variant root")
            .join("package_extension.rs"),
        "pub fn extension_probe() -> bool { true }\n",
    )
    .expect("write Rust extension source");
    let error = generate_c_abi_bindings(&unpinned, &temp_dir("rust_extension_unpinned_out"))
        .expect_err("unpinned dependency must fail");
    assert!(error.contains("exact stable x.y.z version"), "{error}");
    fs::remove_dir_all(unpinned.parent().expect("variant root")).expect("remove variant");
}

#[test]
pub(super) fn pkg_config_external_c_distribution_compiles_package_owned_adapter_sources() {
    let manifest = write_fixture_variant("pkg_config_external_link", |metadata| {
        metadata["c_metadata"]["sources"] = serde_json::json!(["external_adapter.c"]);
        metadata["c_metadata"]["header"] = Value::String("native_boundary.h".to_string());
        metadata["c_metadata"]["external_link"] = serde_json::json!({
            "pkg_config": {
                "package": "libpq",
                "min_version": "14.0",
                "static_link": true
            }
        });
    });
    fs::write(
        manifest
            .parent()
            .expect("variant root")
            .join("external_adapter.c"),
        "#include <libpq-fe.h>\nint terlan_libpq_probe(void) { return PQlibVersion() > 0; }\n",
    )
    .expect("write libpq adapter source");
    let out_dir = temp_dir("pkg_config_external_link_out");

    generate_c_abi_bindings(&manifest, &out_dir).expect("generate pkg-config linked C ABI package");
    assert!(out_dir
        .join("native/rust/include/native_boundary.h")
        .is_file());
    let cargo = fs::read_to_string(out_dir.join("native/rust/Cargo.toml")).expect("Cargo.toml");
    assert!(cargo.contains("pkg-config = \"=0.3.33\""));
    let build = fs::read_to_string(out_dir.join("native/rust/build.rs")).expect("build.rs");
    assert!(build.contains("probe.statik(true)"));
    assert!(build.contains("probe.atleast_version(\"14.0\")"));
    assert!(contains_ignoring_whitespace(
        &build,
        "probe.probe(\"libpq\")"
    ));
    assert!(build.contains("library.include_paths"));
    assert!(!build.contains("PathBuf"));

    let target_dir = temp_dir("pkg_config_external_link_target");
    let build_output =
        std::process::Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string()))
            .args(["build", "--manifest-path"])
            .arg(out_dir.join("native/rust/Cargo.toml"))
            .args(["--offline", "--quiet", "--lib"])
            .env("CARGO_TARGET_DIR", &target_dir)
            .output()
            .expect("build generated libpq adapter");
    assert!(
        build_output.status.success(),
        "pkg-config adapter build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build_output.stdout),
        String::from_utf8_lossy(&build_output.stderr)
    );

    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant root");
    fs::remove_dir_all(out_dir).expect("remove output");
    fs::remove_dir_all(target_dir).expect("remove target");
}

#[test]
pub(super) fn checked_libpq_package_matches_deterministic_regeneration() {
    let package = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../std/native/libpq");
    let checked = package.join("generated");
    let regenerated = temp_dir("checked_libpq_regeneration");

    generate_c_abi_bindings(&package.join("native-binding.json"), &regenerated)
        .expect("regenerate checked libpq package");
    for relative in [
        "native/rust/build.rs",
        "native/rust/src/lib.rs",
        "native/rust/src/bin/native_boundary_helper.rs",
    ] {
        let status = std::process::Command::new("rustfmt")
            .args(["--edition", "2021"])
            .arg(regenerated.join(relative))
            .status()
            .expect("run rustfmt over regenerated libpq Rust source");
        assert!(status.success(), "rustfmt failed for {relative}");
    }
    let checked_files = generated_files(&checked);
    assert_eq!(checked_files, generated_files(&regenerated));
    for relative in checked_files {
        assert_eq!(
            fs::read(checked.join(&relative)).expect("read checked generated file"),
            fs::read(regenerated.join(&relative)).expect("read regenerated file"),
            "generated libpq drift in {}",
            relative.display()
        );
    }

    fs::remove_dir_all(regenerated).expect("remove regenerated package");
}

#[test]
pub(super) fn send_only_opaque_resources_generate_send_without_sync() {
    let manifest = write_fixture_variant("send_only_resource", |metadata| {
        let record = metadata["c_metadata"]["symbols"]
            .as_array_mut()
            .expect("symbols")
            .iter_mut()
            .find(|symbol| symbol["kind"] == "opaque_struct")
            .expect("opaque record");
        record["thread_safety"] = Value::String("send_only".to_string());
    });
    let out_dir = temp_dir("send_only_resource_out");

    generate_c_abi_bindings(&manifest, &out_dir).expect("generate send-only C ABI package");
    let adapter = fs::read_to_string(out_dir.join("native/rust/src/lib.rs")).expect("adapter");
    assert!(adapter.contains("unsafe impl Send for NativeBoundary {}"));
    assert!(!adapter.contains("impl Sync for NativeBoundary"));

    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant root");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
pub(super) fn opaque_resources_reject_unknown_thread_safety_metadata() {
    let manifest = write_fixture_variant("unknown_thread_safety", |metadata| {
        let record = metadata["c_metadata"]["symbols"]
            .as_array_mut()
            .expect("symbols")
            .iter_mut()
            .find(|symbol| symbol["kind"] == "opaque_struct")
            .expect("opaque record");
        record["thread_safety"] = Value::String("send_and_sync".to_string());
    });
    let out_dir = temp_dir("unknown_thread_safety_out");

    let error = generate_c_abi_bindings(&manifest, &out_dir)
        .expect_err("reject unsupported thread-safety claim");
    assert!(error.contains("requires `thread_confined` or `send_only`"));

    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant root");
}

#[test]
pub(super) fn external_c_distribution_splits_cpp_adapters_and_maps_borrowed_strings() {
    let manifest = write_fixture_variant("external_link_with_cpp_adapter", |metadata| {
        metadata["c_metadata"]["sources"] =
            serde_json::json!(["external_adapter.c", "model_adapter.cpp"]);
        metadata["c_metadata"]["cpp_standard"] = Value::String("c++20".to_string());
        metadata["c_metadata"]["header"] = Value::String("include/native_boundary.h".to_string());
        metadata["c_metadata"]["external_link"] = serde_json::json!({
            "root_env": "TERLAN_C_ABI_TEST_ROOT",
            "include_dirs": ["include"],
            "library_dirs": ["lib"],
            "libraries": ["terlan_external_fixture"],
            "runtime_library_dirs": ["lib"]
        });
        metadata["c_metadata"]["symbols"]
            .as_array_mut()
            .expect("symbols")
            .push(serde_json::json!({
                "id": "function.run_model",
                "c_name": "terlan_c_run_model",
                "kind": "function",
                "status": "bind",
                "returns": "int32_t",
                "error_model": "status_code",
                "success_code": 0,
                "parameters": [
                    {"name": "boundary", "c_type": "const TerlanCNativeBoundary *", "direction": "input", "ownership": "borrowed_call"},
                    {"name": "path", "c_type": "const char *", "direction": "input", "ownership": "borrowed_call"},
                    {"name": "out_boundary", "c_type": "TerlanCNativeBoundary **", "direction": "output", "ownership": "transfer_full"}
                ]
            }));
        metadata["modules"][0]["functions"]
            .as_array_mut()
            .expect("functions")
            .push(serde_json::json!({
                "name": "run_model",
                "operation": "c_abi_fixture.native_boundary.run_model",
                "c_symbol": "function.run_model",
                "role": "immutable_method",
                "args": [
                    {"name": "boundary", "ty": "NativeBoundary"},
                    {"name": "path", "ty": "String"}
                ],
                "returns": "NativeBoundary",
                "blocking": "blocking",
                "resource": "opaque_handle",
                "documentation": "Runs a package-owned C++ model adapter.",
                "generated_smoke": "package_owned"
            }));
    });
    let root = manifest.parent().expect("variant root");
    fs::write(root.join("external_adapter.c"), "/* C adapter */\n").expect("write C adapter");
    fs::write(root.join("model_adapter.cpp"), "/* C++ adapter */\n").expect("write C++ adapter");
    let out_dir = temp_dir("external_link_with_cpp_adapter_out");

    generate_c_abi_bindings(&manifest, &out_dir)
        .expect("generate externally linked package with C++ adapter source");
    let build = fs::read_to_string(out_dir.join("native/rust/build.rs")).expect("build.rs");
    assert!(build.contains("c_build.file(\"c/external_adapter.c\")"));
    assert!(build.contains("cpp_build.file(\"c/model_adapter.cpp\")"));
    assert!(build.contains("cpp_build.cpp(true)"));
    assert!(build.contains("warnings_into_errors(true)"));
    assert!(build.contains("-std=c++20"));
    let adapter = fs::read_to_string(out_dir.join("native/rust/src/lib.rs")).expect("adapter");
    assert!(adapter.contains("pub fn run_model(&self, path: &str)"));
    assert!(adapter.contains("std::ffi::CString::new(path.as_bytes())"));
    assert!(adapter.contains("path_c.as_ptr()"));
    let helper = fs::read_to_string(out_dir.join("native/rust/src/bin/native_boundary_helper.rs"))
        .expect("helper");
    assert!(helper.contains("Arg::String(path)"));
    assert!(helper.contains("path.as_str()"));
    assert!(helper.contains("strip_prefix(\"s:\")"));
    let source = fs::read_to_string(out_dir.join("src/c_abi_fixture/NativeBoundary.terl"))
        .expect("Terlan source");
    assert!(source.contains("run_model(boundary: NativeBoundary, path: String)"));

    fs::remove_dir_all(root).expect("remove variant root");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
pub(super) fn multiple_opaque_resources_generate_typed_owners_and_cross_resource_calls() {
    let manifest = write_fixture_variant("multiple_opaque_resources", |metadata| {
        let symbols = metadata["c_metadata"]["symbols"]
            .as_array_mut()
            .expect("symbols");
        symbols.push(serde_json::json!({
            "id": "record.native_model",
            "c_name": "TerlanCNativeModel",
            "kind": "opaque_struct",
            "status": "bind",
            "ownership": "owned",
            "destructor_symbol": "function.native_model_destroy",
            "thread_safety": "thread_confined"
        }));
        symbols.push(serde_json::json!({
            "id": "function.native_model_load",
            "c_name": "terlan_c_native_model_load",
            "kind": "function",
            "status": "bind",
            "returns": "int32_t",
            "error_model": "status_code",
            "success_code": 0,
            "parameters": [
                {"name": "path", "c_type": "const char *", "direction": "input", "ownership": "borrowed_call"},
                {"name": "out_model", "c_type": "TerlanCNativeModel **", "direction": "output", "ownership": "transfer_full"}
            ]
        }));
        symbols.push(serde_json::json!({
            "id": "function.native_model_apply",
            "c_name": "terlan_c_native_model_apply",
            "kind": "function",
            "status": "bind",
            "returns": "int32_t",
            "error_model": "status_code",
            "success_code": 0,
            "parameters": [
                {"name": "model", "c_type": "const TerlanCNativeModel *", "direction": "input", "ownership": "borrowed_call"},
                {"name": "boundary", "c_type": "const TerlanCNativeBoundary *", "direction": "input", "ownership": "borrowed_call"},
                {"name": "out_boundary", "c_type": "TerlanCNativeBoundary **", "direction": "output", "ownership": "transfer_full"}
            ]
        }));
        symbols.push(serde_json::json!({
            "id": "function.native_boundary_from_model",
            "c_name": "terlan_c_native_boundary_from_model",
            "kind": "function",
            "status": "bind",
            "returns": "int32_t",
            "error_model": "status_code",
            "success_code": 0,
            "parameters": [
                {"name": "model", "c_type": "const TerlanCNativeModel *", "direction": "input", "ownership": "borrowed_call"},
                {"name": "out_boundary", "c_type": "TerlanCNativeBoundary **", "direction": "output", "ownership": "transfer_full"}
            ]
        }));
        symbols.push(serde_json::json!({
            "id": "function.native_model_destroy",
            "c_name": "terlan_c_native_model_destroy",
            "kind": "function",
            "status": "bind",
            "returns": "int32_t",
            "error_model": "status_code",
            "success_code": 0,
            "parameters": [
                {"name": "model", "c_type": "TerlanCNativeModel *", "direction": "input", "ownership": "transfer_full"}
            ]
        }));

        metadata["modules"][0]["types"]
            .as_array_mut()
            .expect("types")
            .push(serde_json::json!({
                "name": "NativeModel",
                "c_symbol": "record.native_model",
                "documentation": "Opaque reusable model owned by the generated adapter."
            }));
        let functions = metadata["modules"][0]["functions"]
            .as_array_mut()
            .expect("functions");
        functions.push(serde_json::json!({
            "name": "load_model",
            "operation": "c_abi_fixture.native_boundary.load_model",
            "c_symbol": "function.native_model_load",
            "role": "constructor",
            "args": [{"name": "path", "ty": "String"}],
            "returns": "NativeModel",
            "blocking": "blocking",
            "resource": "opaque_handle",
            "documentation": "Loads a reusable native model.",
            "generated_smoke": "package_owned"
        }));
        functions.push(serde_json::json!({
            "name": "from_model",
            "operation": "c_abi_fixture.native_boundary.from_model",
            "c_symbol": "function.native_boundary_from_model",
            "role": "constructor",
            "args": [{"name": "model", "ty": "NativeModel"}],
            "returns": "NativeBoundary",
            "blocking": "fast",
            "resource": "opaque_handle",
            "documentation": "Constructs a boundary from a separately owned model.",
            "generated_smoke": "package_owned"
        }));
        let apply_model = serde_json::json!({
            "name": "apply_model",
            "operation": "c_abi_fixture.native_boundary.apply_model",
            "c_symbol": "function.native_model_apply",
            "role": "mutable_method",
            "args": [
                {"name": "model", "ty": "NativeModel"},
                {"name": "boundary", "ty": "NativeBoundary"}
            ],
            "returns": "NativeBoundary",
            "blocking": "blocking",
            "resource": "opaque_handle",
            "documentation": "Applies a model to a separately owned input.",
            "generated_smoke": "package_owned"
        });
        functions.push(serde_json::json!({
            "name": "dispose_model",
            "operation": "c_abi_fixture.native_boundary.dispose_model",
            "c_symbol": "function.native_model_destroy",
            "role": "dispose",
            "args": [{"name": "model", "ty": "NativeModel"}],
            "returns": "Unit",
            "blocking": "fast",
            "resource": "dispose_handle",
            "documentation": "Destroys the reusable model.",
            "generated_smoke": "package_owned"
        }));
        metadata["modules"]
            .as_array_mut()
            .expect("modules")
            .push(serde_json::json!({
                "module": "c_abi_fixture.Model",
                "documentation": "Cross-resource model operations.",
                "types": [],
                "functions": [apply_model]
            }));
    });
    let out_dir = temp_dir("multiple_opaque_resources_out");

    generate_c_abi_bindings(&manifest, &out_dir).expect("generate multiple opaque resources");

    let adapter = fs::read_to_string(out_dir.join("native/rust/src/lib.rs")).expect("adapter");
    assert!(adapter.contains("pub struct TerlanCNativeModel"));
    assert!(adapter.contains("pub struct NativeModel"));
    assert!(adapter.contains("impl NativeModel"));
    assert!(adapter.contains("impl Drop for NativeModel"));
    assert!(adapter.contains("pub fn load_model(path: &str)"));
    assert!(adapter.contains("pub fn apply_model(&mut self, boundary: &NativeBoundary)"));
    assert!(adapter.contains("pub fn from_model(model: &NativeModel)"));

    let helper = fs::read_to_string(out_dir.join("native/rust/src/bin/native_boundary_helper.rs"))
        .expect("helper");
    assert!(helper.contains("enum HandleValue"));
    assert!(helper.contains("NativeBoundary(NativeBoundary)"));
    assert!(helper.contains("NativeModel(NativeModel)"));
    assert!(helper.contains("fn live_nativemodel_mut("));
    assert!(helper.contains("let value_model = match self.live_nativemodel(model)"));
    assert!(helper.contains("NativeBoundary::from_model(value_model)"));
    assert!(helper.contains("c_abi_fixture.NativeBoundary.NativeModel"));
    assert!(helper.contains("HandleValue::NativeModel(value)"));
    assert!(helper.contains("#![forbid(unsafe_code)]"));
    assert!(helper.contains("self.handles.get_disjoint_mut"));
    assert!(helper.contains("aliased_mutable_handle"));
    assert!(!helper.contains("unsafe {"));
    let model_source = fs::read_to_string(out_dir.join("src/c_abi_fixture/Model.terl"))
        .expect("generated Terlan module");
    assert!(
        model_source.contains("import c_abi_fixture.NativeBoundary.{NativeBoundary, NativeModel}.")
    );

    let consumer = fs::read_to_string(out_dir.join("tests/c_abi_fixture/NativeBoundaryTest.terl"))
        .expect("generated consumer");
    assert!(consumer.contains("dispose(boundary)"));
    assert!(!consumer.contains("dispose_model(boundary)"));

    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}
