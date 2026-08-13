//! Tests for checked actor-heap field projection from generated native code.

use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{
    terlan_hir::resolve_syntax_module_output,
    terlan_syntax::parse_module_as_syntax_output,
    terlan_typeck::{
        lower_syntax_module_output_to_core, CoreExpr, CoreModule, CoreRecordExprField,
    },
};

use super::native_object_test_support::with_dispatch_lookup_harness;
use super::{emit_native_application_object, NativeExpr, NativeModule};

/// Lowers the canonical managed-record projection fixture into CoreIR.
fn projection_core() -> CoreModule {
    let syntax = parse_module_as_syntax_output(
        "module managed_field_projection.\n\n\
         pub struct Pair {left: Int, right: Int}.\n\n\
         pub struct Profile {title: String}.\n\n\
         pub struct User {name: String, profile: Profile}.\n\n\
         pub constructor Pair {\n\
             (left: Int, right: Int): Pair -> Pair {left: left, right: right}\n\
         }.\n\n\
         pub read(pair: Pair): Int -> pair.left + pair.right.\n\n\
         pub read_record(pair: Pair): Int -> pair#Pair.left.\n\n\
         pub answer(): Int -> read(Pair(20, 22)).\n\n\
         pub record_answer(): Int -> read_record(Pair(42, 0)).\n\n\
         pub constructed_answer(): Int -> read(Pair {right: 22, left: 20}).\n\n\
         pub updated_answer(): Int -> read(Pair {left: 2, right: 0}#Pair {right: 40}).\n\n\
         pub nested_title[T => {profile: {title: String}}](value: T): String -> value.profile.title.\n\n\
         pub nested_answer(): String -> nested_title(User {name: \"Ada\", profile: Profile {title: \"Engineer\"}}).\n",
    )
    .expect("parse managed field projection source");
    let resolved = resolve_syntax_module_output(&syntax).module;
    lower_syntax_module_output_to_core(&syntax, &resolved)
}

/// Counts managed operation nodes retained in one NativeIR expression.
fn operation_count(expr: &NativeExpr) -> usize {
    match expr {
        NativeExpr::ManagedOperation { args, .. } => {
            1 + args.iter().map(operation_count).sum::<usize>()
        }
        NativeExpr::Construct { fields, .. }
        | NativeExpr::Call { args: fields, .. }
        | NativeExpr::TailCall { args: fields, .. } => fields.iter().map(operation_count).sum(),
        NativeExpr::Binary { left, right, .. } => operation_count(left) + operation_count(right),
        NativeExpr::Let { bindings, body } => {
            bindings.iter().map(operation_count).sum::<usize>() + operation_count(body)
        }
        NativeExpr::If { clauses } => clauses
            .iter()
            .map(|(condition, body)| operation_count(condition) + operation_count(body))
            .sum(),
        _ => 0,
    }
}

/// Verifies public aggregate readers retain checked projection operations and
/// execute against real actor-owned objects.
#[test]
fn public_managed_fields_execute_through_bounded_operations() {
    let core = projection_core();
    let modules = NativeModule::lower_application(&[&core]).expect("lower managed projections");
    let read = modules
        .iter()
        .flat_map(|module| &module.functions)
        .find(|function| function.name == "read")
        .expect("public field reader");
    let read_record = modules
        .iter()
        .flat_map(|module| &module.functions)
        .find(|function| function.name == "read_record")
        .expect("public record reader");
    assert_eq!(operation_count(&read.body), 2);
    assert_eq!(operation_count(&read_record.body), 1);
    let nested = modules
        .iter()
        .flat_map(|module| &module.functions)
        .find(|function| function.name == "nested_answer")
        .expect("nested projection answer");
    let nested_title = modules
        .iter()
        .flat_map(|module| &module.functions)
        .find(|function| {
            function.name.starts_with("$aot_generic_") && function.name.contains("nested_title")
        })
        .expect("specialized nested projection reader");
    let NativeExpr::ManagedOperation {
        encoded: title_operation,
        args,
    } = &nested_title.body
    else {
        panic!("nested projection must lower its outer field read");
    };
    let [NativeExpr::ManagedOperation {
        encoded: profile_operation,
        args: profile_args,
    }] = args.as_slice()
    else {
        panic!("nested projection must retain its inner field read");
    };
    assert_eq!(
        u32::from_le_bytes(profile_operation[24..28].try_into().unwrap()),
        1
    );
    assert_eq!(
        u32::from_le_bytes(title_operation[24..28].try_into().unwrap()),
        0
    );
    assert_eq!(profile_args, &[NativeExpr::Param(0)]);
    assert_eq!(operation_count(&nested.body), 0);

    let answer = modules
        .iter()
        .flat_map(|module| &module.functions)
        .find(|function| function.name == "answer")
        .expect("field answer")
        .export_id;
    let record_answer = modules
        .iter()
        .flat_map(|module| &module.functions)
        .find(|function| function.name == "record_answer")
        .expect("record answer")
        .export_id;
    let constructed = modules
        .iter()
        .flat_map(|module| &module.functions)
        .find(|function| function.name == "constructed_answer")
        .expect("record construction answer");
    assert!(matches!(
        &constructed.body,
        NativeExpr::Call { args, .. }
            if matches!(args.as_slice(), [NativeExpr::Let { bindings, body }]
                if bindings == &[NativeExpr::Int(22), NativeExpr::Int(20)]
                    && matches!(body.as_ref(), NativeExpr::Construct { fields, .. }
                        if fields == &[NativeExpr::Param(1), NativeExpr::Param(0)]))
    ));
    let constructed_answer = constructed.export_id;
    let updated = modules
        .iter()
        .flat_map(|module| &module.functions)
        .find(|function| function.name == "updated_answer")
        .expect("record update answer");
    assert_eq!(operation_count(&updated.body), 1);
    let updated_answer = updated.export_id;
    let object = emit_native_application_object("managed_field_projection", &modules)
        .expect("emit managed projection object");
    assert_managed_projection_result("managed-field-projection", &object, answer, 42);
    assert_managed_projection_result("managed-record-projection", &object, record_answer, 42);
    assert_managed_projection_result(
        "managed-record-construction",
        &object,
        constructed_answer,
        42,
    );
    assert_managed_projection_result("managed-record-update", &object, updated_answer, 42);
}

/// Verifies missing fields and explicit record-identity drift fail before
/// native object emission.
#[test]
fn managed_field_projection_rejects_invalid_identity_and_shape() {
    let mut missing = projection_core();
    let read = missing
        .functions
        .iter_mut()
        .find(|function| function.name == "read")
        .and_then(|function| function.clauses.first_mut())
        .and_then(|clause| clause.body.core_expr.as_mut())
        .expect("read body");
    let CoreExpr::BinaryOp { left, .. } = read else {
        panic!("read must contain its first field access");
    };
    let CoreExpr::FieldAccess { field, .. } = left.as_mut() else {
        panic!("read left operand must be a field access");
    };
    *field = "missing".to_string();
    let error = NativeModule::lower_application(&[&missing]).unwrap_err();
    assert!(
        error.starts_with("error[native_ir.field_missing]:"),
        "unexpected diagnostic: {error}"
    );

    let mut mismatched = projection_core();
    let read_record = mismatched
        .functions
        .iter_mut()
        .find(|function| function.name == "read_record")
        .and_then(|function| function.clauses.first_mut())
        .and_then(|clause| clause.body.core_expr.as_mut())
        .expect("record reader body");
    let CoreExpr::RecordAccess { name, .. } = read_record else {
        panic!("record reader must contain explicit record access");
    };
    *name = "Other".to_string();
    let error = NativeModule::lower_application(&[&mismatched]).unwrap_err();
    assert!(
        error.starts_with("error[native_ir.record_identity]:"),
        "unexpected diagnostic: {error}"
    );
}

/// Verifies malformed named-record construction fails before object emission.
#[test]
fn managed_record_construction_rejects_invalid_fields() {
    let mut duplicate = projection_core();
    let fields = constructed_fields(&mut duplicate);
    fields[1].key = fields[0].key.clone();
    let error = NativeModule::lower_application(&[&duplicate]).unwrap_err();
    assert!(
        error.starts_with("error[native_ir.record_field_duplicate]:"),
        "unexpected diagnostic: {error}"
    );

    let mut missing = projection_core();
    constructed_fields(&mut missing)[0].key = "missing".to_string();
    let error = NativeModule::lower_application(&[&missing]).unwrap_err();
    assert!(
        error.starts_with("error[native_ir.record_field_missing]:"),
        "unexpected diagnostic: {error}"
    );

    let mut mistyped = projection_core();
    constructed_fields(&mut mistyped)[0].value = CoreExpr::Atom("true".to_string());
    let error = NativeModule::lower_application(&[&mistyped]).unwrap_err();
    assert!(
        error.starts_with("error[native_ir.record_field_type]:"),
        "unexpected diagnostic: {error}"
    );
}

/// Verifies persistent updates reject duplicate, unknown, and mistyped fields.
#[test]
fn managed_record_update_rejects_invalid_fields() {
    let mut duplicate = projection_core();
    let fields = updated_fields(&mut duplicate);
    fields.push(fields[0].clone());
    let error = NativeModule::lower_application(&[&duplicate]).unwrap_err();
    assert!(
        error.starts_with("error[native_ir.record_update_duplicate]:"),
        "unexpected diagnostic: {error}"
    );

    let mut missing = projection_core();
    updated_fields(&mut missing)[0].key = "missing".to_string();
    let error = NativeModule::lower_application(&[&missing]).unwrap_err();
    assert!(
        error.starts_with("error[native_ir.record_update_missing]:"),
        "unexpected diagnostic: {error}"
    );

    let mut mistyped = projection_core();
    updated_fields(&mut mistyped)[0].value = CoreExpr::Atom("true".to_string());
    let error = NativeModule::lower_application(&[&mistyped]).unwrap_err();
    assert!(
        error.starts_with("error[native_ir.record_update_type]:"),
        "unexpected diagnostic: {error}"
    );

    let mut mismatched = projection_core();
    let CoreExpr::RecordUpdate { name, .. } = updated_record(&mut mismatched) else {
        unreachable!("updated record fixture shape")
    };
    *name = "Other".to_string();
    let error = NativeModule::lower_application(&[&mismatched]).unwrap_err();
    assert!(
        error.starts_with("error[native_ir.record_update_identity]:"),
        "unexpected diagnostic: {error}"
    );
}

/// Returns the fields of the public named-record construction fixture.
fn constructed_fields(module: &mut CoreModule) -> &mut Vec<CoreRecordExprField> {
    let body = module
        .functions
        .iter_mut()
        .find(|function| function.name == "constructed_answer")
        .and_then(|function| function.clauses.first_mut())
        .and_then(|clause| clause.body.core_expr.as_mut())
        .expect("record constructor body");
    let CoreExpr::Call { args, .. } = body else {
        panic!("record constructor fixture must call read");
    };
    let Some(CoreExpr::RecordConstruct { fields, .. }) = args.first_mut() else {
        panic!("read argument must be a named-record construction");
    };
    fields
}

/// Returns the changed fields of the public persistent-update fixture.
fn updated_fields(module: &mut CoreModule) -> &mut Vec<CoreRecordExprField> {
    let CoreExpr::RecordUpdate { fields, .. } = updated_record(module) else {
        panic!("read argument must be a named-record update");
    };
    fields
}

/// Returns the public persistent-update fixture expression.
fn updated_record(module: &mut CoreModule) -> &mut CoreExpr {
    let body = module
        .functions
        .iter_mut()
        .find(|function| function.name == "updated_answer")
        .and_then(|function| function.clauses.first_mut())
        .and_then(|clause| clause.body.core_expr.as_mut())
        .expect("record update body");
    let CoreExpr::Call { args, .. } = body else {
        panic!("record update fixture must call read");
    };
    args.first_mut().expect("record update call argument")
}

/// Links one object with the bounded managed-operation callback and executes
/// a zero-arity scalar export.
fn assert_managed_projection_result(label: &str, object: &[u8], export_id: u64, expected: i64) {
    let root = std::env::temp_dir().join(format!(
        "terlan-{label}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("managed projection fixture directory");
    let object_path = root.join("module.o");
    let harness_path = root.join("harness.rs");
    let executable_path = root.join("harness");
    fs::write(&object_path, object).expect("write managed projection object");
    let harness = with_dispatch_lookup_harness(MANAGED_PROJECTION_HARNESS)
        .replace("$EXPORT_ID", &export_id.to_string())
        .replace("$EXPECTED", &expected.to_string());
    fs::write(&harness_path, harness).expect("write managed projection harness");
    let compile = Command::new("rustc")
        .arg("--edition=2021")
        .arg(&harness_path)
        .arg("-C")
        .arg(format!("link-arg={}", object_path.display()))
        .arg("-o")
        .arg(&executable_path)
        .output()
        .expect("compile managed projection harness");
    assert!(
        compile.status.success(),
        "managed projection harness failed:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let run = Command::new(&executable_path)
        .output()
        .expect("run managed projection harness");
    assert!(
        run.status.success(),
        "managed projection execution failed:\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
    fs::remove_dir_all(root).expect("remove managed projection fixture");
}

const MANAGED_PROJECTION_HARNESS: &str = r#"
use std::ffi::c_void;

type Allocator = unsafe extern "C" fn(
    *mut c_void,
    *const u8,
    u64,
    *const i64,
    u64,
    *mut u64,
) -> i32;

unsafe extern "C" {
    fn terlan_native_dispatch_v3(
        context: *mut c_void,
        allocator: *const c_void,
        closure_resolver: *const c_void,
        dispatch_lookup: *const c_void,
        export_id: u64,
        arguments: *const i64,
        arity: u64,
        result: *mut i64,
        transitions: *mut i64,
        transition_capacity: u64,
        transition_len: *mut u64,
    ) -> i32;
}

#[derive(Default)]
struct Heap {
    objects: Vec<Vec<i64>>,
}

unsafe extern "C" fn managed(
    context: *mut c_void,
    layout: *const u8,
    layout_len: u64,
    fields: *const i64,
    field_count: u64,
    result: *mut u64,
) -> i32 {
    let heap = unsafe { &mut *context.cast::<Heap>() };
    let layout = unsafe { std::slice::from_raw_parts(layout, layout_len as usize) };
    let fields = unsafe { std::slice::from_raw_parts(fields, field_count as usize) };
    if layout.starts_with(b"TVMA") {
        heap.objects.push(fields.to_vec());
        unsafe { *result = heap.objects.len() as u64 };
        return 0;
    }
    if layout.starts_with(b"TVMO") && layout.len() == 28 && fields.len() == 1 {
        let handle = match usize::try_from(fields[0]) {
            Ok(handle) if handle > 0 => handle - 1,
            _ => return 21,
        };
        let field = u32::from_le_bytes(layout[24..28].try_into().unwrap()) as usize;
        let Some(value) = heap.objects.get(handle).and_then(|object| object.get(field)) else {
            return 21;
        };
        unsafe { *result = u64::from_ne_bytes(value.to_ne_bytes()) };
        return 0;
    }
    21
}

fn main() {
    let mut heap = Heap::default();
    let mut result = -1_i64;
    let mut transitions = [0_i64; 1];
    let mut transition_len = 0_u64;
    let status = unsafe {
        terlan_native_dispatch_v3(
            (&mut heap as *mut Heap).cast(),
            managed as Allocator as *const c_void,
            std::ptr::null(),
            dispatch_lookup as *const c_void,
            $EXPORT_ID,
            std::ptr::null(),
            0,
            &mut result,
            transitions.as_mut_ptr(),
            transitions.len() as u64,
            &mut transition_len,
        )
    };
    assert_eq!(status, 0);
    assert_eq!(result, $EXPECTED);
    assert_eq!(transition_len, 0);
}
"#;
