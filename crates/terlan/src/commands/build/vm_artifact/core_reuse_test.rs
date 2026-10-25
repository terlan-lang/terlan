//! Behavioral coverage for verified reuse before native application lowering.

use super::super::native_debug::NativeDebugInput;
use super::super::native_image::{
    compile_owned_rooted_native_application_image, compile_rooted_native_application_image,
};
use super::*;

fn checked(source: &str) -> crate::formal_pipeline::CheckedSyntaxModuleArtifacts {
    let state = crate::CliState::default();
    crate::formal_pipeline::compile_syntax_module_through_phases_with_profile(
        "reuse.terl",
        source,
        state.diagnostic_format,
        None,
        state.native_policy,
        crate::validation::target_profile::TargetProfile::Vm,
    )
    .expect("check reuse fixture")
}

const SOURCE: &str = "module reuse. pub main(): Int -> 7.";

fn roots() -> Vec<(String, String, usize)> {
    vec![("reuse".into(), "main".into(), 0)]
}

#[test]
fn checked_reuse_identity_tracks_body_roots_policy_and_debug_sources() {
    let directory = tempfile::tempdir().unwrap();
    let first = checked(SOURCE);
    let changed = checked("module reuse. pub main(): Int -> 8.");
    let make = |core: &CoreModule, roots: &[(String, String, usize)], debug: &str, policy| {
        CoreReuse::prepare(
            directory.path(),
            "reuse",
            &[core],
            roots,
            &[debug.into()],
            policy,
        )
        .expect("prepare complete checked identity")
    };
    let mut changed_interface = first.core.clone();
    for signature in changed_interface.interface.functions.values_mut() {
        signature.return_type = "String".into();
    }
    for signatures in changed_interface.interface.function_overloads.values_mut() {
        for signature in signatures {
            signature.return_type = "String".into();
        }
    }
    let policy = NativeCodegenPolicy::Development;
    let original = make(&first.core, &roots(), "source-a", policy);
    let repeat = make(&first.core, &roots(), "source-a", policy);
    assert_eq!(original.identity, repeat.identity);
    assert_ne!(original.identity, make(&changed_interface, &roots(), "source-a", policy).identity, "HIR interface signatures are omitted by CoreModule serialization but used during native lowering");
    assert_eq!(original.stamp_path, repeat.stamp_path);
    let body = make(&changed.core, &roots(), "source-a", policy);
    assert_ne!(original.identity, body.identity);
    assert_eq!(
        original.stamp_path, body.stamp_path,
        "edits replace one index slot"
    );
    assert_ne!(
        original.identity,
        make(&first.core, &roots(), "source-b", policy).identity
    );
    assert_ne!(
        original.identity,
        make(
            &first.core,
            &roots(),
            "source-a",
            NativeCodegenPolicy::Serve
        )
        .identity
    );
    let other_roots = vec![("reuse".into(), "other".into(), 0)];
    assert_ne!(
        original.identity,
        make(&first.core, &other_roots, "source-a", policy).identity
    );
}

#[test]
fn checked_reuse_rejects_invalid_and_oversized_index_records() {
    let directory = tempfile::tempdir().unwrap();
    let compiled = checked(SOURCE);
    let reuse = CoreReuse::prepare(
        directory.path(),
        "reuse",
        &[&compiled.core],
        &roots(),
        &[],
        NativeCodegenPolicy::Development,
    )
    .unwrap();
    for bytes in [b"".as_slice(), b"{}", b"{\"payload\":", b"null"] {
        fs::write(&reuse.stamp_path, bytes).unwrap();
        assert!(reuse.read_stamp().is_none());
    }
    let file = fs::File::create(&reuse.stamp_path).unwrap();
    file.set_len(MAX_STAMP_BYTES + 1).unwrap();
    assert!(reuse.read_stamp().is_none());
    fs::remove_file(&reuse.stamp_path).unwrap();
    fs::create_dir(&reuse.stamp_path).unwrap();
    assert!(reuse.read_stamp().is_none());
}

#[test]
fn checked_reuse_stamp_rejects_redirected_and_changed_bindings() {
    let directory = tempfile::tempdir().unwrap();
    let compiled = checked(SOURCE);
    let reuse = CoreReuse::prepare(
        directory.path(),
        "reuse",
        &[&compiled.core],
        &roots(),
        &[],
        NativeCodegenPolicy::Development,
    )
    .unwrap();
    let payload = Payload {
        schema: SCHEMA.into(),
        core_identity: reuse.identity.clone(),
        native_identity: "a".repeat(64),
        projections: vec![],
    };
    let binding = hash_serialized(&payload).unwrap();
    let bytes = serde_json::to_vec(&Stamp { payload, binding }).unwrap();
    fs::write(&reuse.stamp_path, &bytes).unwrap();
    assert!(reuse.read_stamp().is_some());
    let original: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    for (key, value) in [
        ("schema", "old-schema".to_string()),
        ("core_identity", "b".repeat(64)),
        ("native_identity", "../redirected".to_string()),
        ("native_identity", "b".repeat(64)),
    ] {
        let mut altered = original.clone();
        altered["payload"][key] = value.into();
        fs::write(&reuse.stamp_path, serde_json::to_vec(&altered).unwrap()).unwrap();
        assert!(reuse.read_stamp().is_none(), "accepted modified {key}");
    }
}

#[test]
#[cfg(feature = "native-codegen")]
fn checked_reuse_verifies_objects_restores_deleted_outputs_and_matches_clean_build() {
    let directory = tempfile::tempdir().unwrap();
    let cache = directory.path().join("cache");
    let output = directory.path().join("out");
    fs::create_dir_all(&output).unwrap();
    let compiled = checked(SOURCE);
    let cores = [&compiled.core];
    let roots = roots();
    let debug = [NativeDebugInput::capture(
        "reuse.terl",
        SOURCE,
        &compiled.core,
        &compiled.syntax_output,
    )];
    let input = |incremental| RootedNativeApplicationInput {
        roots: &roots,
        debug_inputs: &debug,
        policy: NativeCodegenPolicy::Development,
        incremental,
    };
    let first =
        compile_rooted_native_application_image(&output, &cache, "reuse", &cores, input(true))
            .unwrap()
            .unwrap();
    let bytes = fs::read(output.join(&first.image_name)).unwrap();
    let reuse = CoreReuse::for_input(&cache, "reuse", &cores, &input(true)).unwrap();
    assert!(
        reuse.read_stamp().is_some(),
        "normal compilation must publish reuse evidence"
    );
    fs::remove_dir_all(&output).unwrap();
    let hit = reuse
        .load(&output)
        .unwrap()
        .expect("verified hit recreates outputs");
    assert_eq!(bytes, fs::read(output.join(&hit.image_name)).unwrap());
    assert_eq!(first.cache_input_sha256, hit.cache_input_sha256);
    assert_eq!(
        serde_json::to_value(first.aggregate_projections).unwrap(),
        serde_json::to_value(hit.aggregate_projections).unwrap()
    );
    let warm =
        compile_rooted_native_application_image(&output, &cache, "reuse", &cores, input(true))
            .unwrap()
            .unwrap();
    assert_eq!(bytes, fs::read(output.join(warm.image_name)).unwrap());
    let owned = compile_owned_rooted_native_application_image(
        &output,
        &cache,
        "reuse",
        vec![compiled.core.clone()],
        input(true),
    )
    .unwrap()
    .unwrap();
    assert_eq!(bytes, fs::read(output.join(owned.image_name)).unwrap());
    let fresh = compile_rooted_native_application_image(
        &output,
        &directory.path().join("fresh-cache"),
        "reuse",
        &cores,
        input(false),
    )
    .unwrap()
    .unwrap();
    assert_eq!(bytes, fs::read(output.join(fresh.image_name)).unwrap());
    let object = cache
        .join(&first.cache_input_sha256)
        .join(if cfg!(target_os = "windows") {
            "reuse.native.obj"
        } else {
            "reuse.native.o"
        });
    fs::write(object, b"corrupted object").unwrap();
    assert!(
        reuse.load(&output).unwrap().is_none(),
        "image reuse must verify native objects"
    );
    let repaired =
        compile_rooted_native_application_image(&output, &cache, "reuse", &cores, input(true))
            .unwrap()
            .unwrap();
    assert_eq!(bytes, fs::read(output.join(repaired.image_name)).unwrap());

    let owned_cache = directory.path().join("owned-cache");
    let owned_cold = compile_owned_rooted_native_application_image(
        &output,
        &owned_cache,
        "reuse",
        vec![compiled.core.clone()],
        input(true),
    )
    .unwrap()
    .unwrap();
    assert_eq!(bytes, fs::read(output.join(owned_cold.image_name)).unwrap());
    let owned_reuse = CoreReuse::for_input(&owned_cache, "reuse", &cores, &input(true)).unwrap();
    assert!(owned_reuse.read_stamp().is_some());
    assert!(owned_reuse.load(&output).unwrap().is_some());
}

#[test]
fn checked_reuse_tracks_every_dependency_even_outside_executable_roots() {
    let directory = tempfile::tempdir().unwrap();
    let main = checked(SOURCE);
    let dependency = checked("module dependency. pub value(): Int -> 1.");
    let changed = checked("module dependency. pub value(): Int -> 2.");
    let prepare = |dependency: &CoreModule| {
        CoreReuse::prepare(
            directory.path(),
            "reuse",
            &[&main.core, dependency],
            &roots(),
            &[],
            NativeCodegenPolicy::Development,
        )
        .unwrap()
    };
    assert_ne!(
        prepare(&dependency.core).identity,
        prepare(&changed.core).identity
    );
}

#[test]
fn checked_reuse_requires_incremental_mode_and_executable_roots() {
    let directory = tempfile::tempdir().unwrap();
    let main = checked(SOURCE);
    let cores = [&main.core];
    let roots = roots();
    for (roots, incremental) in [(roots.as_slice(), false), (&[][..], true)] {
        let input = RootedNativeApplicationInput {
            roots,
            debug_inputs: &[],
            policy: NativeCodegenPolicy::Development,
            incremental,
        };
        assert!(CoreReuse::for_input(directory.path(), "reuse", &cores, &input).is_none());
    }
}
