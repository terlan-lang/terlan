//! Bounded Float operations over actor-owned managed values.

use super::super::{
    ActorHeap, ManagedFieldValue, ManagedLayoutRegistry, ManagedMemoryError, ManagedString,
    SemanticTypeId,
};

const MAGIC: &[u8; 4] = b"TVMF";
const VERSION: u16 = 1;
const HEADER_BYTES: usize = 8;
const SEMANTIC_BYTES: usize = 16;
const TO_STRING: u8 = 1;
const FROM_STRING: u8 = 2;
const LOG: u8 = 3;
const ATAN: u8 = 4;
const SQRT: u8 = 5;
const ROUND: u8 = 6;
const TRUNC: u8 = 7;
const EXP: u8 = 8;
const LOG2: u8 = 9;
const LOG10: u8 = 10;
const CBRT: u8 = 11;
const SIN: u8 = 12;
const COS: u8 = 13;
const TAN: u8 = 14;
const ASIN: u8 = 15;
const ACOS: u8 = 16;
const SINH: u8 = 17;
const COSH: u8 = 18;
const TANH: u8 = 19;
const POW: u8 = 20;
const ATAN2: u8 = 21;
const HYPOT: u8 = 22;
const ASINH: u8 = 23;
const ACOSH: u8 = 24;
const ATANH: u8 = 25;
const EXP2: u8 = 26;
const EXPM1: u8 = 27;
const LOG1P: u8 = 28;
const FROM_STRING_BYTES: usize = HEADER_BYTES + SEMANTIC_BYTES;

/// Encodes canonical finite-binary64 formatting into a managed UTF-8 string.
pub fn encode_float_to_string_operation() -> Vec<u8> {
    header(TO_STRING)
}

/// Encodes finite-binary64 parsing into the admitted `Option[Float]` layout.
pub fn encode_float_from_string_operation(option_semantic: SemanticTypeId) -> Vec<u8> {
    let mut encoded = header(FROM_STRING);
    encoded.extend_from_slice(&option_semantic.bytes());
    encoded
}

/// Encodes natural logarithm over one finite-binary64 scalar word.
pub fn encode_float_log_operation() -> Vec<u8> {
    header(LOG)
}

/// Encodes principal arctangent over one finite-binary64 scalar word.
pub fn encode_float_atan_operation() -> Vec<u8> {
    header(ATAN)
}

/// Encodes principal square root over one non-negative finite-binary64 word.
pub fn encode_float_sqrt_operation() -> Vec<u8> {
    header(SQRT)
}

macro_rules! float_operation_encoder {
    ($name:ident, $operation:ident, $docs:literal) => {
        #[doc = $docs]
        pub fn $name() -> Vec<u8> {
            header($operation)
        }
    };
}

float_operation_encoder!(
    encode_float_round_operation,
    ROUND,
    "Encodes nearest-integer rounding."
);
float_operation_encoder!(
    encode_float_trunc_operation,
    TRUNC,
    "Encodes fractional truncation."
);
float_operation_encoder!(
    encode_float_exp_operation,
    EXP,
    "Encodes the natural exponential."
);
float_operation_encoder!(
    encode_float_log2_operation,
    LOG2,
    "Encodes the base-two logarithm."
);
float_operation_encoder!(
    encode_float_log10_operation,
    LOG10,
    "Encodes the base-ten logarithm."
);
float_operation_encoder!(
    encode_float_cbrt_operation,
    CBRT,
    "Encodes the real cube root."
);
float_operation_encoder!(
    encode_float_sin_operation,
    SIN,
    "Encodes sine over radians."
);
float_operation_encoder!(
    encode_float_cos_operation,
    COS,
    "Encodes cosine over radians."
);
float_operation_encoder!(
    encode_float_tan_operation,
    TAN,
    "Encodes tangent over radians."
);
float_operation_encoder!(
    encode_float_asin_operation,
    ASIN,
    "Encodes principal arcsine."
);
float_operation_encoder!(
    encode_float_acos_operation,
    ACOS,
    "Encodes principal arccosine."
);
float_operation_encoder!(
    encode_float_sinh_operation,
    SINH,
    "Encodes hyperbolic sine."
);
float_operation_encoder!(
    encode_float_cosh_operation,
    COSH,
    "Encodes hyperbolic cosine."
);
float_operation_encoder!(
    encode_float_tanh_operation,
    TANH,
    "Encodes hyperbolic tangent."
);
float_operation_encoder!(
    encode_float_asinh_operation,
    ASINH,
    "Encodes inverse hyperbolic sine."
);
float_operation_encoder!(
    encode_float_acosh_operation,
    ACOSH,
    "Encodes inverse hyperbolic cosine."
);
float_operation_encoder!(
    encode_float_atanh_operation,
    ATANH,
    "Encodes inverse hyperbolic tangent."
);
float_operation_encoder!(
    encode_float_exp2_operation,
    EXP2,
    "Encodes the base-two exponential."
);
float_operation_encoder!(
    encode_float_expm1_operation,
    EXPM1,
    "Encodes the natural exponential minus one."
);
float_operation_encoder!(
    encode_float_log1p_operation,
    LOG1P,
    "Encodes the natural logarithm of one plus the input."
);
float_operation_encoder!(
    encode_float_pow_operation,
    POW,
    "Encodes finite real exponentiation."
);
float_operation_encoder!(
    encode_float_atan2_operation,
    ATAN2,
    "Encodes two-argument arctangent."
);
float_operation_encoder!(
    encode_float_hypot_operation,
    HYPOT,
    "Encodes two-dimensional magnitude."
);

pub(super) fn is_float_operation(encoded: &[u8]) -> bool {
    encoded.starts_with(MAGIC)
}

#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(super) fn float_operation_result_is_reference(encoded: &[u8]) -> bool {
    matches!(encoded.get(6).copied(), Some(TO_STRING | FROM_STRING))
}

pub(super) fn execute_float_operation(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    encoded: &[u8],
    words: &[i64],
) -> Result<u64, ManagedMemoryError> {
    validate_header(encoded)?;
    match (encoded[6], encoded.len(), words) {
        (TO_STRING, HEADER_BYTES, [word]) if encoded[7] == 0 => {
            let value = finite_float(*word)?;
            heap.allocate_string(&value.to_string())
                .map(|value| value.erase().encoded_abi_word())
        }
        (FROM_STRING, FROM_STRING_BYTES, [text]) if encoded[7] == 0 => {
            parse_float(heap, layouts, encoded, *text)
        }
        (operation, HEADER_BYTES, [word]) if encoded[7] == 0 && is_unary(operation) => {
            execute_unary(operation, *word)
        }
        (operation, HEADER_BYTES, [left, right]) if encoded[7] == 0 && is_binary(operation) => {
            execute_binary(operation, *left, *right)
        }
        _ => Err(ManagedMemoryError::InvalidManagedOperation),
    }
}

fn is_unary(operation: u8) -> bool {
    matches!(
        operation,
        LOG | ATAN
            | SQRT
            | ROUND
            | TRUNC
            | EXP
            | LOG2
            | LOG10
            | CBRT
            | SIN
            | COS
            | TAN
            | ASIN
            | ACOS
            | SINH
            | COSH
            | TANH
            | ASINH
            | ACOSH
            | ATANH
            | EXP2
            | EXPM1
            | LOG1P
    )
}

fn is_binary(operation: u8) -> bool {
    matches!(operation, POW | ATAN2 | HYPOT)
}

fn execute_unary(operation: u8, word: i64) -> Result<u64, ManagedMemoryError> {
    let value = finite_float(word)?;
    if operation == SQRT && value < 0.0 {
        return Err(ManagedMemoryError::InvalidManagedScalar);
    }
    let result = match operation {
        LOG => value.ln(),
        ATAN => value.atan(),
        SQRT => value.sqrt(),
        ROUND => value.round(),
        TRUNC => value.trunc(),
        EXP => value.exp(),
        LOG2 => value.log2(),
        LOG10 => value.log10(),
        CBRT => value.cbrt(),
        SIN => value.sin(),
        COS => value.cos(),
        TAN => value.tan(),
        ASIN => value.asin(),
        ACOS => value.acos(),
        SINH => value.sinh(),
        COSH => value.cosh(),
        TANH => value.tanh(),
        ASINH => value.asinh(),
        ACOSH => value.acosh(),
        ATANH => value.atanh(),
        EXP2 => value.exp2(),
        EXPM1 => value.exp_m1(),
        LOG1P => value.ln_1p(),
        _ => return Err(ManagedMemoryError::InvalidManagedOperation),
    };
    finite_result(result)
}

fn execute_binary(operation: u8, left: i64, right: i64) -> Result<u64, ManagedMemoryError> {
    let left = finite_float(left)?;
    let right = finite_float(right)?;
    let result = match operation {
        POW => left.powf(right),
        ATAN2 => left.atan2(right),
        HYPOT => left.hypot(right),
        _ => return Err(ManagedMemoryError::InvalidManagedOperation),
    };
    finite_result(result)
}

fn finite_result(value: f64) -> Result<u64, ManagedMemoryError> {
    value
        .is_finite()
        .then_some(value.to_bits())
        .ok_or(ManagedMemoryError::InvalidManagedScalar)
}

fn parse_float(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    encoded: &[u8],
    text: i64,
) -> Result<u64, ManagedMemoryError> {
    let semantic = semantic_at(encoded, HEADER_BYTES)?;
    let text = heap
        .read_string(super::reference_word(text)?.cast::<ManagedString>())?
        .to_owned();
    let parsed = parse_float_text(&text);
    let (variant, fields) = match parsed {
        Some(value) => ("Some", vec![ManagedFieldValue::Float(value)]),
        None => ("None", Vec::new()),
    };
    let layout = super::option_layout(layouts, semantic, variant, fields.len())?;
    heap.allocate_aggregate_ref(layout, &fields)
        .map(|value| value.erase().encoded_abi_word())
}

pub(super) fn parse_float_text(text: &str) -> Option<f64> {
    text.parse::<f64>().ok().filter(|value| value.is_finite())
}

fn finite_float(word: i64) -> Result<f64, ManagedMemoryError> {
    let value = f64::from_bits(u64::from_ne_bytes(word.to_ne_bytes()));
    value
        .is_finite()
        .then_some(value)
        .ok_or(ManagedMemoryError::InvalidManagedScalar)
}

fn header(operation: u8) -> Vec<u8> {
    let mut encoded = Vec::with_capacity(FROM_STRING_BYTES);
    encoded.extend_from_slice(MAGIC);
    encoded.extend_from_slice(&VERSION.to_le_bytes());
    encoded.push(operation);
    encoded.push(0);
    encoded
}

fn validate_header(encoded: &[u8]) -> Result<(), ManagedMemoryError> {
    if encoded.len() < HEADER_BYTES
        || encoded.get(..4) != Some(MAGIC)
        || encoded.get(4..6) != Some(&VERSION.to_le_bytes())
    {
        return Err(ManagedMemoryError::InvalidManagedOperation);
    }
    Ok(())
}

fn semantic_at(encoded: &[u8], offset: usize) -> Result<SemanticTypeId, ManagedMemoryError> {
    encoded
        .get(offset..offset + SEMANTIC_BYTES)
        .and_then(|bytes| <[u8; SEMANTIC_BYTES]>::try_from(bytes).ok())
        .map(SemanticTypeId::from_bytes)
        .ok_or(ManagedMemoryError::InvalidManagedOperation)
}

#[cfg(test)]
#[path = "float_test.rs"]
mod test;
