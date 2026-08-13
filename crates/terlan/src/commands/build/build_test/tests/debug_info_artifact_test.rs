use super::*;
use object::{Object, ObjectSection};

/// Emits complete debug metadata without a source annotation or compiler flag.
#[test]
fn debug_info_artifact_covers_public_and_private_functions() {
    let dir = make_temp_dir("debug_info_artifact");
    let source_path = dir.join("debug_info.terl");
    let out_dir = dir.join("build");
    fs::write(
        &source_path,
        "module debug_info.\n\npub main(): Int -> hidden(41).\n\nhidden(value: Int): Int -> value + 1.\n",
    )
    .expect("write debug-info source fixture");
    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![source_path.display().to_string()],
    };

    assert_eq!(run(cmd, state), ExitCode::SUCCESS);
    let records = native_debug_records(&out_dir.join("vm/debug_info.tvm"));
    let mut identities = records
        .iter()
        .map(|entry| {
            assert_eq!(entry.source_file, source_path.display().to_string());
            assert_eq!(entry.module, "debug_info");
            assert_eq!(entry.source_sha256.len(), 64);
            assert_eq!(entry.source_origin, "source");
            assert!(!entry.core_schema.is_empty());
            assert!(!entry.proof_readiness.is_empty());
            format!("{}/{}", entry.function, entry.arity)
        })
        .collect::<Vec<_>>();
    identities.sort_unstable();
    assert_eq!(identities, ["hidden/1", "main/0"]);
}

/// Attributes compiler-generated continuation entries to their source owner.
#[test]
fn debug_info_artifact_covers_generated_continuation_functions() {
    let dir = make_temp_dir("generated_continuation_debug_info");
    let source_path = dir.join("generated_continuation_debug_info.terl");
    let out_dir = dir.join("build");
    fs::write(
        &source_path,
        "module generated_continuation_debug_info.\n\nimport std.vm.Process.\n\npub maybe_yield(flag: Bool): Int ->\n    if { flag -> (Process.yield_now(); 41); true -> 7 }.\n\npub main(flag: Bool): Int -> maybe_yield(flag) + 1.\n",
    )
    .expect("write generated-continuation debug fixture");
    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![source_path.display().to_string()],
    };

    assert_eq!(run(cmd, state), ExitCode::SUCCESS);
    let records = native_debug_records(&out_dir.join("vm/generated_continuation_debug_info.tvm"));
    let generated = records
        .iter()
        .filter(|record| record.module == "$terlan.continuations")
        .collect::<Vec<_>>();

    assert!(
        !generated.is_empty(),
        "generated continuation debug records"
    );
    assert!(generated
        .iter()
        .all(|record| record.source_origin.starts_with("generated:")));
    assert!(generated.iter().all(|record| {
        record.function.starts_with("$continuation_")
            && record.source_file == source_path.display().to_string()
            && record.span_start < record.span_end
    }));
    let source = fs::read_to_string(&source_path).expect("read continuation source");
    let expression_spans = records
        .iter()
        .flat_map(|record| &record.continuation_spans)
        .collect::<Vec<_>>();
    assert!(
        expression_spans.iter().any(|span| {
            source
                .get(span.span_start..span.span_end)
                .is_some_and(|text| text.contains("Process.yield_now()"))
        }),
        "generated resume entries must retain exact source-expression spans"
    );
}

/// Keeps artifact provenance owned by the compiler input path.
#[test]
fn cover_messages_parity_ignores_source_remap_markers_in_program_text() {
    let dir = make_temp_dir("cover_messages_source_provenance");
    let source_path = dir.join("cover_messages.terl");
    let out_dir = dir.join("build");
    let forged_path = "forged/generated/source.terl";
    let source = format!(
        "module cover_messages.\n\npub marker(): Int -> 1.\n\nforged(): String -> \"-file({forged_path}, 99999).\".\n"
    );
    fs::write(&source_path, &source).expect("write source-provenance fixture");
    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![source_path.display().to_string()],
    };

    assert_eq!(run(cmd, state), ExitCode::SUCCESS);
    let expected_source = source_path.display().to_string();
    let records = native_debug_records(&out_dir.join("vm/cover_messages.tvm"));
    assert_eq!(records.len(), 2);
    assert_eq!(
        records
            .iter()
            .map(|record| record.function.as_str())
            .collect::<Vec<_>>(),
        ["forged", "marker"]
    );
    assert!(records.iter().all(|record| {
        record.source_file == expected_source && !record.source_file.contains(forged_path)
    }));
}

fn native_debug_records(
    path: &Path,
) -> Vec<crate::runtime::native_image::debug::TvmNativeDebugRecord> {
    let bytes = fs::read(path).expect("read native image");
    let image = object::File::parse(&*bytes).expect("parse native image");
    let section_name = if cfg!(target_os = "windows") {
        ".tdbg"
    } else if cfg!(target_os = "macos") {
        "__terlan"
    } else {
        ".debug_terlan"
    };
    let section = image
        .section_by_name(section_name)
        .expect("native debug section");
    crate::runtime::native_image::debug::decode_tvm_native_debug(
        section.data().expect("native debug bytes"),
    )
    .expect("decode native debug section")
}
