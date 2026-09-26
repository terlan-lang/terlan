use super::*;

/// Renders owned integer and opaque-resource lists for one dispatcher call.
pub(super) fn render_dispatcher_lists(
    function: &CAbiBindingFunction,
    dispatcher: &CDispatcherBinding,
    symbols: &BTreeMap<&str, &CSymbol>,
    record: &CSymbol,
    duplicate: &CSymbol,
    indent: &str,
) -> String {
    let mut body = String::new();
    for value in &dispatcher.stack {
        let argument = match value {
            CDispatcherStackValue::OwnedIntListArgument { argument }
            | CDispatcherStackValue::OwnedOptionalIntListArgument { argument }
            | CDispatcherStackValue::OwnedHandleListArgument { argument } => argument,
            _ => continue,
        };
        let allocator = symbols
            .get(
                dispatcher
                    .list_allocator_symbol
                    .as_deref()
                    .expect("validated list allocator"),
            )
            .copied()
            .expect("validated list allocator symbol");
        let push = symbols
            .get(
                dispatcher
                    .list_push_symbol
                    .as_deref()
                    .expect("validated list push"),
            )
            .copied()
            .expect("validated list push symbol");
        let destructor = symbols
            .get(
                dispatcher
                    .list_destructor_symbol
                    .as_deref()
                    .expect("validated list destructor"),
            )
            .copied()
            .expect("validated list destructor symbol");
        let raw = format!("dispatcher_list_{argument}_raw");
        let guard = format!("dispatcher_list_{argument}");
        let allocate_operation = format!("{}.list.{argument}.allocate", function.operation);
        let push_operation = format!("{}.list.{argument}.push", function.operation);
        body.push_str(&format!(
            "{indent}    let mut {raw}: *mut () = std::ptr::null_mut();\n{indent}    // SAFETY: the reviewed allocator returns one exclusive dispatcher list.\n{indent}    let status = unsafe {{ ffi::{}({argument}.len(), &mut {raw}) }};\n{indent}    check_status({allocate_operation:?}, status, {})?;\n{indent}    let {guard} = DispatcherListGuard::new({raw}, ffi::{}, {}).ok_or(CAbiError {{ operation: {:?}, status: -1 }})?;\n",
            allocator.c_name,
            allocator.success_code.unwrap_or(0),
            destructor.c_name,
            destructor.success_code.unwrap_or(0),
            function.operation,
        ));
        match value {
            CDispatcherStackValue::OwnedIntListArgument { .. }
            | CDispatcherStackValue::OwnedOptionalIntListArgument { .. } => {
                body.push_str(&format!(
                    "{indent}    for element in {argument} {{\n{indent}        // SAFETY: the armed list guard owns the destination and integer StableIValues are copied by value.\n{indent}        let status = unsafe {{ ffi::{}({guard}.as_ptr(), *element as u64) }};\n{indent}        check_status({push_operation:?}, status, {})?;\n{indent}    }}\n",
                    push.c_name,
                    push.success_code.unwrap_or(0),
                ));
            }
            CDispatcherStackValue::OwnedHandleListArgument { .. } => {
                let duplicate_operation =
                    format!("{}.list.{argument}.duplicate", function.operation);
                body.push_str(&format!(
                    "{indent}    for element in {argument} {{\n{indent}        let mut element_raw: *mut ffi::{} = std::ptr::null_mut();\n{indent}        // SAFETY: every borrowed list element is duplicated into an independent StableIValue owner.\n{indent}        let status = unsafe {{ ffi::{}(element.raw.as_ptr(), &mut element_raw) }};\n{indent}        check_status({duplicate_operation:?}, status, {})?;\n{indent}        let element_guard = DispatcherInputGuard::new(element_raw).ok_or(CAbiError {{ operation: {:?}, status: -1 }})?;\n{indent}        let element_value = element_guard.into_stable_ivalue();\n{indent}        // SAFETY: the list takes ownership of the duplicated StableIValue on success.\n{indent}        let status = unsafe {{ ffi::{}({guard}.as_ptr(), element_value) }};\n{indent}        if status != {} {{\n{indent}            let _failed_element_guard = DispatcherInputGuard::new(element_value as usize as *mut ffi::{}).expect(\"duplicated dispatcher list element is non-null\");\n{indent}            check_status({push_operation:?}, status, {})?;\n{indent}        }}\n{indent}    }}\n",
                    record.c_name,
                    duplicate.c_name,
                    duplicate.success_code.unwrap_or(0),
                    function.operation,
                    push.c_name,
                    push.success_code.unwrap_or(0),
                    record.c_name,
                    push.success_code.unwrap_or(0),
                ));
            }
            _ => unreachable!("filtered dispatcher list stack value"),
        }
    }
    body
}
