use std::collections::HashMap;
use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::runtime::native_image::managed::SemanticTypeId;
use crate::terlan_hir::resolve_syntax_module_output;
use crate::terlan_syntax::parse_module_as_syntax_output;
use crate::terlan_typeck::{
    lower_syntax_module_output_to_core, CoreExpr, CoreTupleTypeElem, CoreType,
};

use super::super::{
    NativeExpr, NativeFunction, NativeModule, NativeTransitionOperation, NativeType,
};
use super::emit_native_application_object;

#[test]
fn managed_core_types_map_to_closed_pointer_width_native_kinds() {
    let cases = [
        (CoreType::String, "String", NativeType::StringRef),
        (CoreType::Binary, "Binary", NativeType::BinaryRef),
        (CoreType::Atom, "Atom", NativeType::Atom),
        (
            CoreType::AtomLiteral("ready".to_owned()),
            "Atom[ready]",
            NativeType::Atom,
        ),
        (
            CoreType::Named("Bytes".to_owned()),
            "Bytes",
            NativeType::BytesRef,
        ),
        (
            CoreType::Named("BitString".to_owned()),
            "BitString",
            NativeType::BinaryRef,
        ),
        (
            CoreType::Apply {
                constructor: "Process".to_owned(),
                args: vec![CoreType::Int],
            },
            "Process[Int]",
            NativeType::Int,
        ),
        (
            CoreType::Apply {
                constructor: "Entry".to_owned(),
                args: vec![CoreType::Int],
            },
            "Entry[Int]",
            NativeType::Int,
        ),
        (
            CoreType::Apply {
                constructor: "Monitor".to_owned(),
                args: vec![CoreType::Int],
            },
            "Monitor[Int]",
            NativeType::Int,
        ),
        (
            CoreType::Apply {
                constructor: "ResourceKind".to_owned(),
                args: vec![CoreType::Int],
            },
            "ResourceKind[Int]",
            NativeType::Int,
        ),
        (
            CoreType::Apply {
                constructor: "Resource".to_owned(),
                args: vec![CoreType::Int],
            },
            "Resource[Int]",
            NativeType::Int,
        ),
        (
            CoreType::Named("Timer".to_owned()),
            "Timer",
            NativeType::Int,
        ),
        (
            CoreType::Named("ExitReason".to_owned()),
            "ExitReason",
            NativeType::Int,
        ),
        (
            CoreType::Named("SchedulingClass".to_owned()),
            "SchedulingClass",
            NativeType::Int,
        ),
        (
            CoreType::Apply {
                constructor: "Message".to_owned(),
                args: vec![CoreType::String],
            },
            "Message[String]",
            NativeType::StringRef,
        ),
    ];

    for (core, text, expected) in cases {
        assert_eq!(super::super::native_type(Some(&core), text), Some(expected));
    }
    assert!(NativeType::StringRef.is_managed_reference());
    assert!(NativeType::BytesRef.is_managed_reference());
    assert!(NativeType::BinaryRef.is_managed_reference());
    assert!(!NativeType::Atom.is_managed_reference());

    let tuple = CoreType::Tuple(vec![
        CoreTupleTypeElem::Type(CoreType::Int),
        CoreTupleTypeElem::Type(CoreType::String),
    ]);
    let expected = SemanticTypeId::from_canonical(&tuple.contract_text()).expect("semantic type");
    assert_eq!(
        super::super::native_type(Some(&tuple), "{Int, String}"),
        Some(NativeType::ManagedRef(expected))
    );
    assert!(NativeType::ManagedRef(expected).is_managed_reference());

    let list = CoreType::List(Box::new(CoreType::Int));
    let expected = SemanticTypeId::from_canonical(&list.contract_text()).expect("list semantic");
    assert_eq!(
        super::super::native_type(Some(&list), "List[Int]"),
        Some(NativeType::ManagedRef(expected))
    );

    let named = CoreType::Named("projection.Pair".to_owned());
    let expected = SemanticTypeId::from_canonical(&named.contract_text()).expect("named semantic");
    assert_eq!(
        super::super::native_type(Some(&named), "projection.Pair"),
        Some(NativeType::ManagedRef(expected))
    );
}

#[test]
fn typed_public_lifecycle_operations_lower_to_existing_vm_transitions() {
    let syntax = parse_module_as_syntax_output(concat!(
        "module typed_lifecycle.\n\nimport std.vm.Process.\n",
        "import type std.vm.Process.{Entry, ExitReason, Monitor, Process, Resource, ResourceKind, SchedulingClass, Timer}.\n\n",
        "pub spawn(entry: Entry[Int]): Process[Int] -> Process.spawn[Int](entry).\n",
        "pub sleep(timer: Timer): Unit -> Process.sleep(timer).\n",
        "pub link(peer: Process[Int]): Unit -> Process.link[Int](peer).\n",
        "pub monitor(peer: Process[Int]): Monitor[Int] -> Process.monitor[Int](peer).\n",
        "pub acquire(kind: ResourceKind[Int]): Resource[Int] -> Process.acquire[Int](kind).\n",
        "pub cancel(target: Process[Int]): Unit -> Process.cancel[Int](target).\n",
        "pub fail(reason: ExitReason): Unit -> Process.fail(reason).\n",
        "pub schedule(class: SchedulingClass): Unit -> Process.schedule(class).\n",
    ))
    .expect("typed lifecycle source");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules = NativeModule::lower_application(&[&core]).expect("typed native lifecycle module");
    let functions = &modules[0].functions;
    for (name, operation, result) in [
        ("spawn", NativeTransitionOperation::Spawn, NativeType::Int),
        ("sleep", NativeTransitionOperation::Timer, NativeType::Unit),
        ("link", NativeTransitionOperation::Link, NativeType::Unit),
        (
            "monitor",
            NativeTransitionOperation::Monitor,
            NativeType::Int,
        ),
        (
            "acquire",
            NativeTransitionOperation::Resource,
            NativeType::Int,
        ),
        (
            "cancel",
            NativeTransitionOperation::Cancellation,
            NativeType::Unit,
        ),
        ("fail", NativeTransitionOperation::Failure, NativeType::Unit),
        (
            "schedule",
            NativeTransitionOperation::Scheduling,
            NativeType::Unit,
        ),
    ] {
        let function = functions
            .iter()
            .find(|function| function.name == name)
            .unwrap_or_else(|| panic!("{name} function"));
        assert_eq!(function.params, [NativeType::Int], "{name}");
        assert_eq!(function.return_type, result, "{name}");
        assert!(
            matches!(
                function.body,
                NativeExpr::Suspend {
                    operation: actual,
                    ref arguments,
                    ..
                } if actual == operation && arguments == &[NativeExpr::Param(0)]
            ),
            "{name}: {:?}",
            function.body
        );
    }
}

#[test]
fn managed_parameters_and_returns_emit_as_native_reference_slots() {
    let module = NativeModule {
        name: "ManagedIdentity".to_owned(),
        functions: vec![NativeFunction {
            export_id: 41,
            name: "identity".to_owned(),
            public: true,
            arity: 1,
            callable_captures: Vec::new(),
            params: vec![NativeType::StringRef],
            return_type: NativeType::StringRef,
            body: NativeExpr::Param(0),
        }],
        continuations: vec![],
        managed_layouts: vec![],
        managed_collections: vec![],
        atoms: vec![],
    };

    let object = emit_native_application_object("managed_identity", &[module])
        .expect("managed-reference native object");
    assert!(!object.is_empty());
}

#[test]
fn managed_content_equality_is_not_lowered_as_pointer_identity() {
    let variables = HashMap::from([
        ("left".to_owned(), NativeType::StringRef),
        ("right".to_owned(), NativeType::StringRef),
    ]);
    let equality = CoreExpr::BinaryOp {
        operator: "==".to_owned(),
        left: Box::new(CoreExpr::Var("left".to_owned())),
        right: Box::new(CoreExpr::Var("right".to_owned())),
    };

    assert_eq!(
        super::super::infer_native_type(&equality, &variables, &HashMap::new()),
        None
    );
}

#[test]
fn typed_public_mailbox_operations_lower_to_fixed_native_transition_frames() {
    let syntax = parse_module_as_syntax_output(
        concat!(
            "module typed_mailbox.\n\nimport std.vm.Process.\nimport std.vm.Message.\nimport type std.vm.Process.{Process}.\nimport type std.vm.Message.{Message}.\n\n",
            "pub struct Pair {left: Int, right: Int}.\n",
            "pub send_string(recipient: Int, payload: String): Unit -> Process.send_string(recipient, payload).\n",
            "pub receive_string(): String -> Process.receive_string().\n",
            "pub send_bytes(recipient: Int, payload: Bytes): Unit -> Process.send_bytes(recipient, payload).\n",
            "pub receive_bytes(): Bytes -> Process.receive_bytes().\n",
            "pub send_binary(recipient: Int, payload: Binary): Unit -> Process.send_binary(recipient, payload).\n",
            "pub receive_binary(): Binary -> Process.receive_binary().\n",
            "pub send_atom(recipient: Int, payload: Atom): Unit -> Process.send_atom(recipient, payload).\n",
            "pub receive_atom(): Atom -> Process.receive_atom().\n",
            "pub send_pair(recipient: Process[Pair], payload: Message[Pair]): Unit -> Process.send[Pair](recipient, payload).\n",
            "pub receive_pair(): Message[Pair] -> Process.receive[Pair]().\n",
            "pub wrap_pair(payload: Pair): Message[Pair] -> Message.wrap[Pair](payload).\n",
            "pub unwrap_pair(message: Message[Pair]): Pair -> Message.unwrap[Pair](message).\n",
            "pub identity(value: String): String -> value.\n",
            "pub composed_receive(): String -> identity(receive_string()).\n",
        ),
    )
    .expect("typed mailbox source");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules = NativeModule::lower_application(&[&core]).expect("typed native mailbox module");
    let functions = &modules[0].functions;
    for (suffix, tag) in [("string", 5), ("bytes", 9), ("binary", 4), ("atom", 8)] {
        let send_name = format!("send_{suffix}");
        let send = functions
            .iter()
            .find(|function| function.name == send_name)
            .unwrap_or_else(|| panic!("{send_name} function"));
        assert!(matches!(
            send.body,
            NativeExpr::Suspend {
                operation: NativeTransitionOperation::SendTyped,
                ref arguments,
                ..
            } if arguments.len() == 5
                && arguments[1..4]
                    == [NativeExpr::Int(tag), NativeExpr::Int(0), NativeExpr::Int(0)]
        ));
        let receive_name = format!("receive_{suffix}");
        let receive = functions
            .iter()
            .find(|function| function.name == receive_name)
            .unwrap_or_else(|| panic!("{receive_name} function"));
        assert!(matches!(
            receive.body,
            NativeExpr::Suspend {
                operation: NativeTransitionOperation::ReceiveTyped,
                ref arguments,
                ..
            } if arguments == &[NativeExpr::Int(tag), NativeExpr::Int(0), NativeExpr::Int(0)]
        ));
    }
    let pair_type = functions
        .iter()
        .find(|function| function.name == "send_pair")
        .and_then(|function| function.params.get(1))
        .copied()
        .expect("qualified Pair parameter type");
    assert!(matches!(pair_type, NativeType::ManagedRef(_)));
    let pair_transition_words = pair_type.boundary_type().transition_words();
    let pair_words = pair_transition_words.map(NativeExpr::Int);
    for (name, operation, expected_arguments) in [
        ("send_pair", NativeTransitionOperation::SendTyped, 5),
        ("receive_pair", NativeTransitionOperation::ReceiveTyped, 3),
    ] {
        let function = functions
            .iter()
            .find(|function| function.name == name)
            .unwrap_or_else(|| panic!("{name} function"));
        let NativeExpr::Suspend {
            operation: actual,
            arguments,
            ..
        } = &function.body
        else {
            panic!("{name} must suspend");
        };
        assert_eq!(*actual, operation, "{name}");
        assert_eq!(arguments.len(), expected_arguments, "{name}");
        let metadata_start = usize::from(name == "send_pair");
        assert_eq!(
            arguments[metadata_start..metadata_start + 3],
            pair_words,
            "{name}"
        );
    }
    let composed = functions
        .iter()
        .find(|function| function.name == "composed_receive")
        .expect("composed typed receive function");
    assert!(matches!(
        composed.body,
        NativeExpr::CallThen { .. } | NativeExpr::Let { .. } | NativeExpr::TailCall { .. }
    ));
    let composed_export = composed.export_id;
    for name in ["wrap_pair", "unwrap_pair"] {
        let function = functions
            .iter()
            .find(|function| function.name == name)
            .unwrap_or_else(|| panic!("{name} function"));
        assert_eq!(function.body, NativeExpr::Param(0), "{name}");
    }
    let pair_receive_export = functions
        .iter()
        .find(|function| function.name == "receive_pair")
        .expect("typed public receive export")
        .export_id;
    let object = emit_native_application_object("typed_mailbox", &modules)
        .expect("composed typed receive must emit native code");
    assert_typed_receive_frame(&object, composed_export, [5, 0, 0]);
    assert_typed_receive_frame(&object, pair_receive_export, pair_transition_words);
}

/// Links and invokes one generated composed Receive entry.
fn assert_typed_receive_frame(object: &[u8], export_id: u64, type_words: [i64; 3]) {
    let root = std::env::temp_dir().join(format!(
        "terlan-typed-mailbox-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("typed mailbox fixture directory");
    let object_path = root.join("typed_mailbox.o");
    let harness_path = root.join("harness.rs");
    let executable_path = root.join("harness");
    fs::write(&object_path, object).expect("typed mailbox object");
    let type_words = type_words.map(|word| word.to_string()).join(", ");
    fs::write(
        &harness_path,
        TYPED_RECEIVE_HARNESS
            .replace("$EXPORT_ID", &export_id.to_string())
            .replace("$TYPE_WORDS", &type_words),
    )
    .expect("typed mailbox harness");
    let compile = Command::new("rustc")
        .arg("--edition=2021")
        .arg(&harness_path)
        .arg("-C")
        .arg(format!("link-arg={}", object_path.display()))
        .arg("-o")
        .arg(&executable_path)
        .output()
        .expect("compile typed mailbox harness");
    assert!(
        compile.status.success(),
        "typed mailbox harness failed:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let run = Command::new(&executable_path)
        .output()
        .expect("run typed mailbox harness");
    assert!(
        run.status.success(),
        "typed mailbox frame was malformed:\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
    fs::remove_dir_all(root).expect("remove typed mailbox fixture");
}

/// Linked probe for the exact typed Receive operation-argument count.
const TYPED_RECEIVE_HARNESS: &str = r#"
use std::ffi::c_void;

unsafe extern "C" {
    fn terlan_native_dispatch_v2(
        context: *mut c_void,
        allocator: *const c_void,
        closure_resolver: *const c_void,
        export_id: u64,
        arguments: *const i64,
        arity: u64,
        result: *mut i64,
        transitions: *mut i64,
        transition_capacity: u64,
        transition_len: *mut u64,
    ) -> i32;
}

fn main() {
    let mut result = 0_i64;
    let mut transitions = [0_i64; 8];
    let mut transition_len = 0_u64;
    let status = unsafe {
        terlan_native_dispatch_v2(
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
            $EXPORT_ID,
            std::ptr::null(),
            0,
            &mut result,
            transitions.as_mut_ptr(),
            transitions.len() as u64,
            &mut transition_len,
        )
    };
    assert_eq!(status, 23);
    assert_ne!(result, 0);
    assert_eq!(transition_len, 3);
    assert_eq!(&transitions[..3], &[$TYPE_WORDS]);
}
"#;
