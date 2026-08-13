use super::*;

/// Proves a multi-module closure compiles into one deterministic native image.
#[test]
fn parallel_frontend_compilation_preserves_one_application_link() {
    let root = make_temp_dir("parallel_frontend_application_link");
    let project = root.join("project");
    let source = project.join("src/app");
    let out_dir = root.join("build");
    fs::create_dir_all(&source).expect("create source root");
    fs::write(
        project.join(TERLAN_PROJECT_MANIFEST_FILE),
        "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"terlan-vm\"\n",
    )
    .expect("write project manifest");
    fs::write(
        source.join("First.terl"),
        "module app.First.\n\npub one(): Int -> 1.\n",
    )
    .expect("write first module");
    fs::write(
        source.join("Second.terl"),
        "module app.Second.\n\npub two(): Int -> 2.\n",
    )
    .expect("write second module");
    fs::write(
        source.join("Main.terl"),
        "module app.Main.\n\nimport app.First.{one}.\nimport app.Second.{two}.\n\npub main(): Int -> one() + two().\n",
    )
    .expect("write entry module");
    let state = CliState {
        incremental: true,
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let command = CliCommand {
        verb: Some("build".to_string()),
        args: vec![project.display().to_string()],
    };

    assert_eq!(run(command.clone(), state.clone()), ExitCode::SUCCESS);
    let image_path = out_dir.join("vm/app_Main.tvm");
    let first_image = fs::read(&image_path).expect("read first native image");
    let units_dir = out_dir.join(".terlan/native-aot/units");
    let first_units = native_unit_snapshots(&units_dir);
    assert_eq!(first_units.len(), 3);
    assert_eq!(
        fs::read_dir(out_dir.join("vm"))
            .expect("read VM output")
            .filter_map(Result::ok)
            .filter(|entry| entry.path().extension().is_some_and(|value| value == "tvm"))
            .count(),
        1
    );

    assert_eq!(run(command.clone(), state.clone()), ExitCode::SUCCESS);
    assert_eq!(
        fs::read(&image_path).expect("read reused native image"),
        first_image
    );
    assert_eq!(native_unit_snapshots(&units_dir), first_units);
    assert_eq!(
        native_image_export_names(&image_path),
        vec![
            "app.First.one/0".to_string(),
            "app.Main.main/0".to_string(),
            "app.Second.two/0".to_string(),
        ]
    );

    let poisoned_identity = first_units.keys().next().expect("native unit identity");
    let object_name = if cfg!(target_os = "windows") {
        "module.obj"
    } else {
        "module.o"
    };
    fs::write(
        units_dir.join(poisoned_identity).join(object_name),
        b"poisoned native unit",
    )
    .expect("poison native unit");
    let final_cache = fs::read_dir(out_dir.join(".terlan/native-aot"))
        .expect("read native application cache")
        .map(|entry| entry.expect("read native application cache entry").path())
        .find(|path| path.is_dir() && path.file_name().is_some_and(|name| name != "units"))
        .expect("native application cache directory");
    fs::remove_file(final_cache.join("manifest.v1")).expect("invalidate final image cache");
    assert_eq!(run(command.clone(), state.clone()), ExitCode::SUCCESS);
    assert_eq!(native_unit_snapshots(&units_dir), first_units);

    fs::write(
        source.join("First.terl"),
        "module app.First.\n\npub one(): Int -> 41.\n",
    )
    .expect("edit first module implementation");
    assert_eq!(run(command, state), ExitCode::SUCCESS);
    assert_ne!(
        fs::read(&image_path).expect("read changed native image"),
        first_image
    );
    let changed_units = native_unit_snapshots(&units_dir);
    assert_eq!(changed_units.len(), 6);
    for (identity, object) in first_units {
        assert_eq!(changed_units.get(&identity), Some(&object));
    }
}

/// Reads content-addressed native module objects keyed by cache identity.
fn native_unit_snapshots(units_dir: &Path) -> std::collections::BTreeMap<String, Vec<u8>> {
    fs::read_dir(units_dir)
        .expect("read native unit cache")
        .map(|entry| {
            let directory = entry.expect("read native unit entry").path();
            let identity = directory
                .file_name()
                .and_then(|name| name.to_str())
                .expect("UTF-8 native unit identity")
                .to_string();
            let object_name = if cfg!(target_os = "windows") {
                "module.obj"
            } else {
                "module.o"
            };
            let object = fs::read(directory.join(object_name)).expect("read native unit object");
            (identity, object)
        })
        .collect()
}

/// Reads sorted public exports from one admitted native application image.
fn native_image_export_names(path: &Path) -> Vec<String> {
    let image = fs::read(path).expect("read native application image");
    let target = crate::runtime::native_image::host_tvm_target().expect("host TVM target");
    let mut exports = crate::runtime::native_image::inspect_tvm_image(&image, &target.triple)
        .expect("inspect native application image")
        .descriptor
        .exports
        .into_iter()
        .map(|export| export.name)
        .collect::<Vec<_>>();
    exports.sort();
    exports
}
