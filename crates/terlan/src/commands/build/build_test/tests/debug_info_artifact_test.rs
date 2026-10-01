use super::*;
use object::{Object, ObjectSection};

#[test]
fn debug_info_artifact_preserves_receiver_default_adapter_origins() {
    let dir = make_temp_dir("receiver_defaults_debug");
    let source_path = dir.join("receiver_defaults_debug.terl");
    let out_dir = dir.join("build");
    let source = r#"module receiver_defaults_debug.
pub struct Counter { value: Int }.
pub (counter: Counter) read(offset: Int = 2, extra: Int = 1): Int ->
    counter.value + offset + extra.
pub (mut counter: Counter) advance(offset: Int = 2, extra: Int = 1): Unit ->
    Counter { value: counter.value + offset + extra }.
pub main(): Int ->
    let counter = Counter { value: 1 };
    counter.advance(); counter.advance(4); counter.advance(5, 6);
    counter.read() + counter.read(4) + counter.read(5, 6).
"#;
    fs::write(&source_path, source).unwrap();
    assert_eq!(
        run(
            CliCommand {
                verb: Some("build".into()),
                args: vec![source_path.display().to_string()],
            },
            CliState {
                out_dir: out_dir.clone(),
                ..CliState::default()
            }
        ),
        ExitCode::SUCCESS
    );
    let records = native_debug_records(&out_dir.join("vm/receiver_defaults_debug.tvm"));
    for method in ["read", "advance"] {
        let original = records
            .iter()
            .find(|record| record.function == method && record.arity == 3)
            .unwrap();
        assert_eq!(original.source_origin, "source");
        for arity in [1, 2] {
            let adapter = records
                .iter()
                .find(|record| record.function == method && record.arity == arity)
                .unwrap();
            assert_eq!(
                adapter.source_origin,
                format!("generated:receiver_defaults_debug.{method}/3")
            );
            assert_eq!(adapter.callable_id, original.callable_id);
            assert_eq!(
                (adapter.span_start, adapter.span_end),
                (original.span_start, original.span_end)
            );
            assert_eq!(adapter.source_file, source_path.display().to_string());
        }
    }
}

/// Concrete trait bodies map to their own nested source declarations.
#[test]
fn debug_info_artifact_covers_concrete_trait_implementations() {
    let dir = make_temp_dir("concrete_trait_debug");
    let source_path = dir.join("concrete_trait_debug.terl");
    let out_dir = dir.join("build");
    let source = r#"module concrete_trait_debug.
pub trait Value[T] { value(input: T): Int. }.
impl Value[Int] for Int { value(input: Int): Int -> input + 1. }.
impl Value[Bool] for Bool { value(input: Bool): Int -> if { input -> 1; true -> 0 }. }.
pub main(): Int -> Value[Int].value(40) + Value[Bool].value(true).
"#;
    fs::write(&source_path, source).expect("write trait debug fixture");
    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".into()),
        args: vec![source_path.display().to_string()],
    };
    assert_eq!(run(cmd, state), ExitCode::SUCCESS);
    let records = native_debug_records(&out_dir.join("vm/concrete_trait_debug.tvm"));
    let mut callable_ids = std::collections::HashSet::new();
    for (ty, declaration) in [
        ("Int", "value(input: Int): Int -> input + 1"),
        (
            "Bool",
            "value(input: Bool): Int -> if { input -> 1; true -> 0 }",
        ),
    ] {
        let origin =
            format!("generated:concrete_trait_debug.concrete_trait_debug.Value[{ty}].value/1");
        let record = records
            .iter()
            .find(|record| record.source_origin == origin)
            .expect("trait implementation debug record");
        assert_eq!(record.source_file, source_path.display().to_string());
        assert_eq!(
            source[record.span_start..record.span_end].trim(),
            declaration
        );
        assert!(
            callable_ids.insert(record.callable_id),
            "distinct implementation identity"
        );
    }
}

/// Constructor adapters, defaults and specializations retain their source clause.
#[test]
fn debug_info_artifact_covers_source_constructor_helpers() {
    let dir = make_temp_dir("constructor_debug");
    let source_path = dir.join("constructor_debug.terl");
    let out_dir = dir.join("build");
    let source = r#"module constructor_debug.
pub type Adjusted = Int.
pub constructor Adjusted {
    (value: Int = 40, extra: Int = 2): Adjusted -> value + extra
}.
pub type Choice = Int.
pub constructor Choice {
    (value: Int): Choice -> value + 2;
    (flag: Bool, extra: Int): Choice -> if { flag -> extra; true -> 0 }
}.
pub type Items[T] = List[T].
pub constructor Items[T] {
    (...values: T): Items[T] -> values
}.
pub main(): Int -> Adjusted() + Choice(40) + Choice(true, 42).
pub values(): List[Int] -> Items(1, 2, 3).
"#;
    fs::write(&source_path, source).expect("write constructor debug fixture");
    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".into()),
        args: vec![source_path.display().to_string()],
    };
    assert_eq!(run(cmd, state), ExitCode::SUCCESS);
    let records = native_debug_records(&out_dir.join("vm/constructor_debug.tvm"));
    for (origin, clause) in [
        (
            "Adjusted/2",
            "(value: Int = 40, extra: Int = 2): Adjusted -> value + extra",
        ),
        ("Choice/1", "(value: Int): Choice -> value + 2"),
        (
            "Choice/2",
            "(flag: Bool, extra: Int): Choice -> if { flag -> extra; true -> 0 }",
        ),
        ("Items/1", "(...values: T): Items[T] -> values"),
    ] {
        let expected = format!("generated:constructor_debug.{origin}");
        let helpers = records
            .iter()
            .filter(|record| record.source_origin == expected)
            .collect::<Vec<_>>();
        assert!(!helpers.is_empty(), "missing constructor origin {expected}");
        for record in helpers {
            assert_eq!(record.source_file, source_path.display().to_string());
            assert_eq!(source[record.span_start..record.span_end].trim(), clause);
        }
    }
    for helper in [
        "$constructor_Adjusted_0_default_0",
        "$constructor_Adjusted_0_default_1",
        "$constructor_call_Adjusted_2",
        "$constructor_call_Items_3",
    ] {
        assert!(
            records
                .iter()
                .any(|record| record.function.contains(helper)),
            "missing generated helper {helper}"
        );
    }
}

/// Selected closure factories and lifted bodies keep their declaration spans.
#[test]
fn debug_info_artifact_covers_typed_conditional_closure_factories() {
    let dir = make_temp_dir("conditional_closure_debug");
    let source_path = dir.join("conditional_closure_debug.terl");
    let out_dir = dir.join("build");
    let source = "module conditional_closure_debug.\npub select(choice: Bool, offset: Int): Int -> let operation = if { choice -> ((value: Int) -> value + offset); true -> ((value: Int) -> value - offset) }; operation(10).\n";
    fs::write(&source_path, source).unwrap();
    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".into()),
        args: vec![source_path.display().to_string()],
    };
    assert_eq!(run(cmd, state), ExitCode::SUCCESS);
    let records = native_debug_records(&out_dir.join("vm/conditional_closure_debug.tvm"));
    let factories = records
        .iter()
        .filter(|record| record.function.starts_with("$aot_closure_factory_"))
        .collect::<Vec<_>>();
    assert_eq!(factories.len(), 2);
    for record in factories {
        assert_eq!(
            record.source_origin,
            "generated:conditional_closure_debug.select/2"
        );
        assert_eq!(record.source_file, source_path.display().to_string());
        let declaration = &source[record.span_start..record.span_end];
        assert!(
            declaration.contains("select("),
            "declaration span: {declaration}"
        );
    }
}

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
