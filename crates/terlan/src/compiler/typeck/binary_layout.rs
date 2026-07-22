use std::collections::{HashMap, HashSet};

use super::core_intrinsic_lowering::core_pure_effect_set;
use super::*;

pub(super) const MAX_INTEGER_SEGMENT_WIDTH: i64 = 63;
pub(super) const MAX_BYTE_SEGMENT_WIDTH: i64 = i64::MAX / 8;

/// Canonical descriptor carried by source-level binary layouts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum BinaryLayoutDescriptor {
    UInt(i64),
    IntBits(i64),
    Bytes(i64),
    Bits(i64),
    Utf8,
    Utf16,
    Utf32,
    Rest,
}

/// Parses a validated descriptor type into compiler-owned structure.
pub(super) fn binary_layout_descriptor(text: &str) -> Option<BinaryLayoutDescriptor> {
    let mut vars = HashMap::<String, TypeVarId>::new();
    let mut next_var = 0;
    let descriptor_types = [
        "UInt", "IntBits", "Bytes", "Bits", "Utf8", "Utf16", "Utf32", "Rest",
    ]
    .into_iter()
    .map(str::to_string)
    .collect::<HashSet<_>>();
    let parsed = parse_type_expr(text, &descriptor_types, &mut vars, &mut next_var)?;
    let Type::Named { name, args, .. } = parsed else {
        return None;
    };
    match (name.as_str(), args.as_slice()) {
        ("UInt", [Type::LiteralInt(width)]) => Some(BinaryLayoutDescriptor::UInt(*width)),
        ("IntBits", [Type::LiteralInt(width)]) => Some(BinaryLayoutDescriptor::IntBits(*width)),
        ("Bytes", [Type::LiteralInt(width)]) => Some(BinaryLayoutDescriptor::Bytes(*width)),
        ("Bits", [Type::LiteralInt(width)]) => Some(BinaryLayoutDescriptor::Bits(*width)),
        ("Utf8", []) => Some(BinaryLayoutDescriptor::Utf8),
        ("Utf16", []) => Some(BinaryLayoutDescriptor::Utf16),
        ("Utf32", []) => Some(BinaryLayoutDescriptor::Utf32),
        ("Rest", []) => Some(BinaryLayoutDescriptor::Rest),
        _ => None,
    }
}

pub(super) fn vm_named_type(module: &str, name: &str) -> Type {
    Type::Named {
        module: Some(module.to_string()),
        name: name.to_string(),
        args: Vec::new(),
    }
}

/// Lowers fixed-width binary fields to canonical VM bitstring intrinsics.
pub(super) fn core_binary_layout_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    let big_endian = match expr.text.as_deref()? {
        "big" => true,
        "little" => false,
        _ => return None,
    };
    let span = expr.span.into();
    let mut segments = expr.fields.iter().map(|field| {
        let descriptor = binary_layout_descriptor(field.value.text.as_deref()?)?;
        let (width, intrinsic) = match descriptor {
            BinaryLayoutDescriptor::UInt(width) => (
                width,
                if big_endian {
                    CorePrimitiveIntrinsic::VmBitStringFromUintBe
                } else {
                    CorePrimitiveIntrinsic::VmBitStringFromUintLe
                },
            ),
            BinaryLayoutDescriptor::IntBits(width) => (
                width,
                if big_endian {
                    CorePrimitiveIntrinsic::VmBitStringFromIntBe
                } else {
                    CorePrimitiveIntrinsic::VmBitStringFromIntLe
                },
            ),
            BinaryLayoutDescriptor::Bytes(width) => {
                return Some(core_binary_intrinsic(
                    CorePrimitiveIntrinsic::VmBitStringFromExactBytes,
                    vec![CoreExpr::Var(field.key.clone()), CoreExpr::Int(width)],
                    span,
                ));
            }
            BinaryLayoutDescriptor::Bits(width) => {
                return Some(core_binary_intrinsic(
                    CorePrimitiveIntrinsic::VmBitStringRequireExactBits,
                    vec![CoreExpr::Var(field.key.clone()), CoreExpr::Int(width)],
                    span,
                ));
            }
            BinaryLayoutDescriptor::Utf8 => {
                return Some(core_binary_intrinsic(
                    CorePrimitiveIntrinsic::VmBitStringUtf8Scalar,
                    vec![CoreExpr::Var(field.key.clone())],
                    span,
                ));
            }
            BinaryLayoutDescriptor::Utf16 => {
                return Some(core_binary_intrinsic(
                    if big_endian {
                        CorePrimitiveIntrinsic::VmBitStringUtf16BeScalar
                    } else {
                        CorePrimitiveIntrinsic::VmBitStringUtf16LeScalar
                    },
                    vec![CoreExpr::Var(field.key.clone())],
                    span,
                ));
            }
            BinaryLayoutDescriptor::Utf32 => {
                return Some(core_binary_intrinsic(
                    if big_endian {
                        CorePrimitiveIntrinsic::VmBitStringUtf32BeScalar
                    } else {
                        CorePrimitiveIntrinsic::VmBitStringUtf32LeScalar
                    },
                    vec![CoreExpr::Var(field.key.clone())],
                    span,
                ));
            }
            BinaryLayoutDescriptor::Rest => {
                return Some(core_binary_intrinsic(
                    CorePrimitiveIntrinsic::VmBitStringFromAllBytes,
                    vec![CoreExpr::Var(field.key.clone())],
                    span,
                ));
            }
        };
        Some(core_binary_intrinsic(
            intrinsic,
            vec![CoreExpr::Var(field.key.clone()), CoreExpr::Int(width)],
            span,
        ))
    });

    let first = segments.next()??;
    segments.try_fold(first, |prefix, segment| {
        Some(core_binary_intrinsic(
            CorePrimitiveIntrinsic::VmBitStringConcat,
            vec![prefix, segment?],
            span,
        ))
    })
}

/// Lowers one checked descriptor-backed binary pattern into typed CoreIR.
pub(super) fn core_binary_layout_pattern_from_syntax(
    pattern: &SyntaxPatternOutput,
) -> Option<CorePattern> {
    let endian = match pattern.text.as_deref()? {
        "big" => CoreBinaryPatternEndian::Big,
        "little" => CoreBinaryPatternEndian::Little,
        _ => return None,
    };
    let fields = pattern
        .fields
        .iter()
        .map(|field| {
            let descriptor = binary_layout_descriptor(field.value.text.as_deref()?)?;
            let descriptor = match descriptor {
                BinaryLayoutDescriptor::UInt(width) => {
                    CoreBinaryPatternDescriptor::UInt(u64::try_from(width).ok()?)
                }
                BinaryLayoutDescriptor::IntBits(width) => {
                    CoreBinaryPatternDescriptor::IntBits(u64::try_from(width).ok()?)
                }
                BinaryLayoutDescriptor::Bytes(width) => {
                    CoreBinaryPatternDescriptor::Bytes(u64::try_from(width).ok()?)
                }
                BinaryLayoutDescriptor::Bits(width) => {
                    CoreBinaryPatternDescriptor::Bits(u64::try_from(width).ok()?)
                }
                BinaryLayoutDescriptor::Utf8 => CoreBinaryPatternDescriptor::Utf8,
                BinaryLayoutDescriptor::Utf16 => CoreBinaryPatternDescriptor::Utf16,
                BinaryLayoutDescriptor::Utf32 => CoreBinaryPatternDescriptor::Utf32,
                BinaryLayoutDescriptor::Rest => CoreBinaryPatternDescriptor::Rest,
            };
            Some(CoreBinaryPatternField {
                name: field.key.clone(),
                descriptor,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    Some(CorePattern::BinaryLayout { endian, fields })
}

fn core_binary_intrinsic(
    intrinsic: CorePrimitiveIntrinsic,
    args: Vec<CoreExpr>,
    span: Span,
) -> CoreExpr {
    let return_type = core_primitive_intrinsic_return_type(&intrinsic);
    CoreExpr::Intrinsic(CoreIntrinsicCall {
        id: CoreIntrinsicId::Primitive(intrinsic),
        args,
        return_type,
        effects: core_pure_effect_set(),
        span,
    })
}
