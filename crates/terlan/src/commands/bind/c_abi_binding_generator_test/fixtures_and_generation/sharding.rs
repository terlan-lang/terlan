use super::*;

#[test]
pub(super) fn large_c_surfaces_shard_ffi_free_and_owned_adapters() {
    let manifest = write_fixture_variant("large_surface", |metadata| {
        let free_function = metadata["modules"][0]["functions"]
            .as_array()
            .expect("functions")
            .iter()
            .find(|function| function["name"] == "live_count")
            .expect("free function")
            .clone();
        let free_symbol = metadata["c_metadata"]["symbols"]
            .as_array()
            .expect("symbols")
            .iter()
            .find(|symbol| symbol["id"] == "function.native_boundary_live_count")
            .expect("free symbol")
            .clone();
        let owned_function = metadata["modules"][0]["functions"]
            .as_array()
            .expect("functions")
            .iter()
            .find(|function| function["role"] == "immutable_method")
            .expect("owned function")
            .clone();

        for index in 0..200 {
            let mut function = free_function.clone();
            function["name"] = format!("large_value_{index}").into();
            function["operation"] =
                format!("c_abi_fixture.native_boundary.large_value_{index}").into();
            function["c_symbol"] = format!("function.large_value_{index}").into();
            metadata["modules"][0]["functions"]
                .as_array_mut()
                .expect("functions")
                .push(function);

            let mut symbol = free_symbol.clone();
            symbol["id"] = format!("function.large_value_{index}").into();
            symbol["c_name"] = format!("terlan_c_large_value_{index}").into();
            metadata["c_metadata"]["symbols"]
                .as_array_mut()
                .expect("symbols")
                .push(symbol);

            let mut owned = owned_function.clone();
            owned["name"] = format!("large_owned_{index}").into();
            owned["operation"] =
                format!("c_abi_fixture.native_boundary.large_owned_{index}").into();
            metadata["modules"][0]["functions"]
                .as_array_mut()
                .expect("functions")
                .push(owned);
        }
    });
    let out_dir = temp_dir("large_surface_output");

    generate_c_abi_bindings(&manifest, &out_dir).expect("generate large C ABI package");

    let source_dir = out_dir.join("native/rust/src");
    let root = fs::read_to_string(source_dir.join("lib.rs")).expect("adapter root");
    assert!(root.lines().count() < 1_000);
    assert!(root.contains("include!(\"generated_ffi_0.rs\")"));
    assert!(root.contains("include!(\"generated_free_adapter_0.rs\")"));
    assert!(!root.contains("pub fn terlan_c_large_value_59"));
    assert!(!root.contains("pub fn large_value_59"));

    let generated_sources = fs::read_dir(&source_dir)
        .expect("read generated sources")
        .map(|entry| entry.expect("generated source").path())
        .collect::<Vec<_>>();
    assert!(generated_sources.iter().any(|path| {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("generated_ffi_"))
            && fs::read_to_string(path)
                .expect("FFI shard")
                .contains("pub fn terlan_c_large_value_199")
    }));
    assert!(generated_sources.iter().any(|path| {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("generated_free_adapter_"))
            && fs::read_to_string(path)
                .expect("free adapter shard")
                .contains("pub fn large_value_199")
    }));
    let owned_adapter_paths = fs::read_dir(&source_dir)
        .expect("read generated sources")
        .map(|entry| entry.expect("generated source").path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("generated_adapter_"))
        })
        .collect::<Vec<_>>();
    assert!(owned_adapter_paths.len() > 1);
    for path in owned_adapter_paths {
        let adapter = fs::read_to_string(&path).expect("owned adapter shard");
        assert!(
            adapter.lines().count() <= 900,
            "{} has {} lines",
            path.display(),
            adapter.lines().count()
        );
    }

    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove generated outputs");
}
