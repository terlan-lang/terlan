use super::*;

pub(super) fn infer_map_put(name: &str, expr: &CoreExpr) -> Option<CoreType> {
    match expr {
        CoreExpr::MutableReceiverCall {
            receiver,
            method,
            args,
            ..
        } if method == "put"
            && matches!(receiver.as_ref(), CoreExpr::Var(receiver) if receiver == name) =>
        {
            let [key, value] = args.as_slice() else {
                return None;
            };
            Some(CoreType::Apply {
                constructor: "Map".to_string(),
                args: vec![literal_type(key)?, literal_type(value)?],
            })
        }
        CoreExpr::Let { bindings, body } => bindings
            .iter()
            .find_map(|binding| infer_map_put(name, &binding.value))
            .or_else(|| infer_map_put(name, body)),
        _ => None,
    }
}

pub(super) fn infer_set_add(name: &str, expr: &CoreExpr) -> Option<CoreType> {
    match expr {
        CoreExpr::MutableReceiverCall {
            receiver,
            method,
            args,
            ..
        } if method == "add"
            && matches!(receiver.as_ref(), CoreExpr::Var(receiver) if receiver == name) =>
        {
            let [element] = args.as_slice() else {
                return None;
            };
            Some(CoreType::Apply {
                constructor: "Set".to_string(),
                args: vec![literal_type(element)?],
            })
        }
        CoreExpr::Let { bindings, body } => bindings
            .iter()
            .find_map(|binding| infer_set_add(name, &binding.value))
            .or_else(|| infer_set_add(name, body)),
        _ => None,
    }
}

fn literal_type(expr: &CoreExpr) -> Option<CoreType> {
    match expr {
        CoreExpr::Int(_) => Some(CoreType::Int),
        CoreExpr::Float(_) => Some(CoreType::Float),
        CoreExpr::Binary(_) => Some(CoreType::String),
        CoreExpr::Atom(value) if matches!(value.as_str(), "true" | "false") => Some(CoreType::Bool),
        CoreExpr::Atom(_) => Some(CoreType::Atom),
        _ => None,
    }
}

pub(super) fn map_receiver_intrinsic(method: &str, arity: usize) -> Option<CorePrimitiveIntrinsic> {
    match (method, arity) {
        ("is_empty", 1) => Some(CorePrimitiveIntrinsic::MapIsEmpty),
        ("size", 1) => Some(CorePrimitiveIntrinsic::MapSize),
        ("get", 2) => Some(CorePrimitiveIntrinsic::MapGet),
        ("contains_key", 2) => Some(CorePrimitiveIntrinsic::MapContainsKey),
        ("iterator", 1) => Some(CorePrimitiveIntrinsic::MapIterator),
        ("put", 3) => Some(CorePrimitiveIntrinsic::MapPut),
        ("remove", 2) => Some(CorePrimitiveIntrinsic::MapRemove),
        ("clear", 1) => Some(CorePrimitiveIntrinsic::MapClear),
        _ => None,
    }
}

pub(super) fn set_receiver_intrinsic(method: &str, arity: usize) -> Option<CorePrimitiveIntrinsic> {
    match (method, arity) {
        ("is_empty", 1) => Some(CorePrimitiveIntrinsic::SetIsEmpty),
        ("size", 1) => Some(CorePrimitiveIntrinsic::SetSize),
        ("contains", 2) => Some(CorePrimitiveIntrinsic::SetContains),
        ("iterator", 1) => Some(CorePrimitiveIntrinsic::SetIterator),
        ("add", 2) => Some(CorePrimitiveIntrinsic::SetAdd),
        ("remove", 2) => Some(CorePrimitiveIntrinsic::SetRemove),
        ("clear", 1) => Some(CorePrimitiveIntrinsic::SetClear),
        _ => None,
    }
}

pub(super) fn list_receiver_intrinsic(
    method: &str,
    arity: usize,
) -> Option<CorePrimitiveIntrinsic> {
    match (method, arity) {
        ("is_empty", 1) => Some(CorePrimitiveIntrinsic::ListIsEmpty),
        ("length", 1) => Some(CorePrimitiveIntrinsic::ListLength),
        ("first", 1) => Some(CorePrimitiveIntrinsic::ListFirst),
        ("rest", 1) => Some(CorePrimitiveIntrinsic::ListRest),
        ("iterator", 1) => Some(CorePrimitiveIntrinsic::ListIterator),
        ("push", 2) => Some(CorePrimitiveIntrinsic::ListPush),
        ("clear", 1) => Some(CorePrimitiveIntrinsic::ListClear),
        _ => None,
    }
}

pub(super) fn bytes_receiver_intrinsic(
    method: &str,
    arity: usize,
) -> Option<CorePrimitiveIntrinsic> {
    match (method, arity) {
        ("to_list", 1) => Some(CorePrimitiveIntrinsic::VmBytesToList),
        ("length", 1) => Some(CorePrimitiveIntrinsic::VmBytesLength),
        ("starts_with", 2) => Some(CorePrimitiveIntrinsic::VmBytesStartsWith),
        ("contains", 2) => Some(CorePrimitiveIntrinsic::VmBytesContains),
        ("first_non_ascii_whitespace", 1) => {
            Some(CorePrimitiveIntrinsic::VmBytesFirstNonAsciiWhitespace)
        }
        ("slice", 3) => Some(CorePrimitiveIntrinsic::VmBytesSlice),
        ("read_uint_be", 3) => Some(CorePrimitiveIntrinsic::VmBytesReadUintBe),
        ("read_int_be", 3) => Some(CorePrimitiveIntrinsic::VmBytesReadIntBe),
        ("read_uint_le", 3) => Some(CorePrimitiveIntrinsic::VmBytesReadUintLe),
        ("read_int_le", 3) => Some(CorePrimitiveIntrinsic::VmBytesReadIntLe),
        _ => None,
    }
}

pub(super) fn bitstring_receiver_intrinsic(
    method: &str,
    arity: usize,
) -> Option<CorePrimitiveIntrinsic> {
    match (method, arity) {
        ("to_utf8_scalar", 1) => Some(CorePrimitiveIntrinsic::VmBitStringToUtf8Scalar),
        ("to_utf16_be_scalar", 1) => Some(CorePrimitiveIntrinsic::VmBitStringToUtf16BeScalar),
        ("to_utf16_le_scalar", 1) => Some(CorePrimitiveIntrinsic::VmBitStringToUtf16LeScalar),
        ("to_utf32_be_scalar", 1) => Some(CorePrimitiveIntrinsic::VmBitStringToUtf32BeScalar),
        ("to_utf32_le_scalar", 1) => Some(CorePrimitiveIntrinsic::VmBitStringToUtf32LeScalar),
        ("bit_length", 1) => Some(CorePrimitiveIntrinsic::VmBitStringBitLength),
        ("byte_length", 1) => Some(CorePrimitiveIntrinsic::VmBitStringByteLength),
        ("is_byte_aligned", 1) => Some(CorePrimitiveIntrinsic::VmBitStringIsByteAligned),
        ("slice", 3) => Some(CorePrimitiveIntrinsic::VmBitStringSlice),
        ("concat", 2) => Some(CorePrimitiveIntrinsic::VmBitStringConcat),
        ("to_bytes", 1) => Some(CorePrimitiveIntrinsic::VmBitStringToBytes),
        ("to_uint_be", 1) => Some(CorePrimitiveIntrinsic::VmBitStringToUintBe),
        ("to_int_be", 1) => Some(CorePrimitiveIntrinsic::VmBitStringToIntBe),
        ("to_uint_le", 1) => Some(CorePrimitiveIntrinsic::VmBitStringToUintLe),
        ("to_int_le", 1) => Some(CorePrimitiveIntrinsic::VmBitStringToIntLe),
        _ => None,
    }
}

pub(super) fn bitstring_intrinsic_return_type(intrinsic: &CorePrimitiveIntrinsic) -> CoreType {
    match intrinsic {
        CorePrimitiveIntrinsic::VmBitStringBitLength
        | CorePrimitiveIntrinsic::VmBitStringByteLength
        | CorePrimitiveIntrinsic::VmBitStringToUtf8Scalar
        | CorePrimitiveIntrinsic::VmBitStringToUtf16BeScalar
        | CorePrimitiveIntrinsic::VmBitStringToUtf16LeScalar
        | CorePrimitiveIntrinsic::VmBitStringToUtf32BeScalar
        | CorePrimitiveIntrinsic::VmBitStringToUtf32LeScalar
        | CorePrimitiveIntrinsic::VmBitStringToUintBe
        | CorePrimitiveIntrinsic::VmBitStringToIntBe
        | CorePrimitiveIntrinsic::VmBitStringToUintLe
        | CorePrimitiveIntrinsic::VmBitStringToIntLe => CoreType::Int,
        CorePrimitiveIntrinsic::VmBitStringIsByteAligned => CoreType::Bool,
        CorePrimitiveIntrinsic::VmBitStringToBytes => CoreType::Named("Bytes".to_string()),
        _ => CoreType::Named("BitString".to_string()),
    }
}

pub(super) fn bytes_intrinsic_return_type(intrinsic: &CorePrimitiveIntrinsic) -> CoreType {
    match intrinsic {
        CorePrimitiveIntrinsic::VmBytesToList => CoreType::List(Box::new(CoreType::Int)),
        CorePrimitiveIntrinsic::VmBytesStartsWith | CorePrimitiveIntrinsic::VmBytesContains => {
            CoreType::Bool
        }
        CorePrimitiveIntrinsic::VmBytesLength
        | CorePrimitiveIntrinsic::VmBytesFirstNonAsciiWhitespace
        | CorePrimitiveIntrinsic::VmBytesReadUintBe
        | CorePrimitiveIntrinsic::VmBytesReadIntBe
        | CorePrimitiveIntrinsic::VmBytesReadUintLe
        | CorePrimitiveIntrinsic::VmBytesReadIntLe => CoreType::Int,
        _ => CoreType::Named("Bytes".to_string()),
    }
}

pub(crate) fn list_intrinsic_return_type(
    element: &CoreType,
    intrinsic: &CorePrimitiveIntrinsic,
) -> CoreType {
    match intrinsic {
        CorePrimitiveIntrinsic::ListIsEmpty => CoreType::Bool,
        CorePrimitiveIntrinsic::ListLength => CoreType::Int,
        CorePrimitiveIntrinsic::ListGet => element.clone(),
        CorePrimitiveIntrinsic::ListFirst => CoreType::Apply {
            constructor: "Option".to_string(),
            args: vec![element.clone()],
        },
        CorePrimitiveIntrinsic::ListRest => CoreType::Apply {
            constructor: "Option".to_string(),
            args: vec![CoreType::List(Box::new(element.clone()))],
        },
        CorePrimitiveIntrinsic::ListIterator => CoreType::Apply {
            constructor: "Iterator".to_string(),
            args: vec![element.clone()],
        },
        _ => CoreType::List(Box::new(element.clone())),
    }
}

pub(super) fn map_intrinsic_return_type(
    intrinsic: &CorePrimitiveIntrinsic,
    map: &CoreType,
) -> CoreType {
    let Some((key, value)) = map_elements(map) else {
        return map.clone();
    };
    match intrinsic {
        CorePrimitiveIntrinsic::MapIsEmpty | CorePrimitiveIntrinsic::MapContainsKey => {
            CoreType::Bool
        }
        CorePrimitiveIntrinsic::MapSize => CoreType::Int,
        CorePrimitiveIntrinsic::MapGet => option(value.clone()),
        CorePrimitiveIntrinsic::MapIterator => CoreType::Apply {
            constructor: "Iterator".to_string(),
            args: vec![CoreType::Tuple(vec![
                crate::terlan_typeck::CoreTupleTypeElem::Type(key.clone()),
                crate::terlan_typeck::CoreTupleTypeElem::Type(value.clone()),
            ])],
        },
        _ => map.clone(),
    }
}

pub(super) fn set_intrinsic_return_type(
    intrinsic: &CorePrimitiveIntrinsic,
    set: &CoreType,
) -> CoreType {
    let Some(element) = set_element(set) else {
        return set.clone();
    };
    match intrinsic {
        CorePrimitiveIntrinsic::SetIsEmpty | CorePrimitiveIntrinsic::SetContains => CoreType::Bool,
        CorePrimitiveIntrinsic::SetSize => CoreType::Int,
        CorePrimitiveIntrinsic::SetIterator => CoreType::Apply {
            constructor: "Iterator".to_string(),
            args: vec![element.clone()],
        },
        _ => set.clone(),
    }
}
