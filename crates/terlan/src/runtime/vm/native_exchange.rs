//! VM-owned one-shot exchange for package-native owned buffers.
//!
//! Native helpers are isolated processes, so pointer-bearing formats such as
//! DLPack cannot cross the helper transport. Producers copy a validated tensor
//! into this owner-thread registry; consumers atomically claim the owned
//! payload and reconstruct their package-native representation locally.

use std::collections::BTreeMap;

const MAX_RANK: usize = 32;
const MAX_EXCHANGE_BYTES: usize = 16_777_216;
const TENSOR_PACKET_HEADER_BYTES: usize = 16;
const TENSOR_PACKET_MAGIC: &[u8; 4] = b"TNXP";
const TOKEN_MAGIC: &[u8; 4] = b"TNXT";

/// Opaque, authenticated identity for one native exchange.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct NativeExchangeToken {
    id: u64,
    generation: u64,
    authentication: u64,
}

impl NativeExchangeToken {
    /// Returns the fields used by the VM term codec.
    #[cfg(test)]
    pub(crate) fn fields(self) -> (u64, u64, u64) {
        (self.id, self.generation, self.authentication)
    }

    /// Reconstructs a token supplied by the VM term codec.
    pub(crate) fn from_fields(id: u64, generation: u64, authentication: u64) -> Self {
        Self {
            id,
            generation,
            authentication,
        }
    }
}

/// Pointer-free tensor descriptor copied between native helpers.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NativeTensorExchange {
    /// DLPack ABI major version admitted by the producer.
    pub(crate) version: u32,
    /// DLPack device type; baseline exchange supports CPU (`1`).
    pub(crate) device_type: i32,
    /// Device ordinal; baseline CPU exchange requires zero.
    pub(crate) device_id: i32,
    /// DLPack dtype code.
    pub(crate) dtype_code: u8,
    /// Bits per scalar lane.
    pub(crate) dtype_bits: u8,
    /// Scalar lane count; baseline exchange requires one.
    pub(crate) dtype_lanes: u16,
    /// Nonnegative logical dimensions.
    pub(crate) shape: Vec<i64>,
    /// Optional element strides. When present they must be row-major.
    pub(crate) strides: Option<Vec<i64>>,
    /// Byte offset into `data`; baseline exchange requires zero.
    pub(crate) byte_offset: u64,
    /// Owned, pointer-free tensor bytes.
    pub(crate) data: Vec<u8>,
}

/// Payload admitted into the package-neutral exchange registry.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum NativeExchangePayload {
    /// A validated baseline tensor copied from package-native DLPack storage.
    Tensor(NativeTensorExchange),
}

impl NativeExchangePayload {
    fn kind(&self) -> &'static str {
        match self {
            Self::Tensor(_) => "tensor.dlpack.v1",
        }
    }

    fn validate(&self) -> Result<(), NativeExchangeError> {
        match self {
            Self::Tensor(tensor) => validate_tensor(tensor),
        }
    }
}

/// Stable native exchange failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NativeExchangeError {
    code: &'static str,
    message: String,
}

impl NativeExchangeError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    /// Returns the stable diagnostic code.
    pub(crate) fn code(&self) -> &'static str {
        self.code
    }

    /// Returns the human-readable diagnostic.
    pub(crate) fn message(&self) -> &str {
        &self.message
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NativeExchangeState {
    Available,
    Claimed,
    Closed,
}

#[derive(Clone, Debug)]
struct NativeExchangeSlot {
    generation: u64,
    authentication: u64,
    owner_process_id: u64,
    producer: String,
    consumer: String,
    state: NativeExchangeState,
    payload: Option<NativeExchangePayload>,
    cleanup_events: u8,
}

/// Owner-thread registry for one-shot cross-helper native payloads.
#[derive(Debug)]
pub(crate) struct NativeExchangeBroker {
    next_id: u64,
    authentication_seed: u64,
    slots: BTreeMap<u64, NativeExchangeSlot>,
}

impl Default for NativeExchangeBroker {
    fn default() -> Self {
        Self::new()
    }
}

impl NativeExchangeBroker {
    /// Creates an empty broker with a process-random authentication seed.
    pub(crate) fn new() -> Self {
        Self {
            next_id: 1,
            authentication_seed: rand::random(),
            slots: BTreeMap::new(),
        }
    }

    /// Publishes one validated payload for an exact consumer package.
    pub(crate) fn publish(
        &mut self,
        owner_process_id: u64,
        producer: &str,
        consumer: &str,
        payload: NativeExchangePayload,
    ) -> Result<NativeExchangeToken, NativeExchangeError> {
        validate_identity("producer", producer)?;
        validate_identity("consumer", consumer)?;
        payload.validate()?;
        let id = self.next_id;
        self.next_id = id.checked_add(1).ok_or_else(|| {
            NativeExchangeError::new(
                "native_exchange.id_overflow",
                "native exchange id allocation overflowed",
            )
        })?;
        let generation = 1;
        let authentication = authenticate(self.authentication_seed, id, generation);
        self.slots.insert(
            id,
            NativeExchangeSlot {
                generation,
                authentication,
                owner_process_id,
                producer: producer.to_string(),
                consumer: consumer.to_string(),
                state: NativeExchangeState::Available,
                payload: Some(payload),
                cleanup_events: 0,
            },
        );
        Ok(NativeExchangeToken {
            id,
            generation,
            authentication,
        })
    }

    /// Publishes one validated pointer-free tensor packet and returns an
    /// authenticated token encoded as VM-owned bytes.
    pub(crate) fn publish_tensor_packet(
        &mut self,
        owner_process_id: u64,
        producer: &str,
        consumer: &str,
        packet: &[u8],
    ) -> Result<Vec<u8>, NativeExchangeError> {
        let tensor = decode_tensor_packet(packet)?;
        let token = self.publish(
            owner_process_id,
            producer,
            consumer,
            NativeExchangePayload::Tensor(tensor),
        )?;
        Ok(encode_token(token))
    }

    /// Claims an authenticated token byte string for one consumer and returns
    /// the validated pointer-free tensor packet sent to that helper.
    pub(crate) fn claim_tensor_packet(
        &mut self,
        token_bytes: &[u8],
        owner_process_id: u64,
        consumer: &str,
    ) -> Result<Option<(NativeExchangeToken, Vec<u8>)>, NativeExchangeError> {
        let Some(token) = decode_token(token_bytes)? else {
            return Ok(None);
        };
        let payload = self.claim(token, owner_process_id, consumer, "tensor.dlpack.v1")?;
        let NativeExchangePayload::Tensor(tensor) = payload;
        Ok(Some((token, encode_tensor_packet(&tensor))))
    }

    /// Atomically claims an available payload for its exact owner and consumer.
    pub(crate) fn claim(
        &mut self,
        token: NativeExchangeToken,
        owner_process_id: u64,
        consumer: &str,
        expected_kind: &str,
    ) -> Result<NativeExchangePayload, NativeExchangeError> {
        let slot = self.live_slot_mut(token)?;
        if slot.owner_process_id != owner_process_id {
            return Err(NativeExchangeError::new(
                "native_exchange.owner_mismatch",
                "native exchange belongs to a different VM process",
            ));
        }
        if slot.consumer != consumer {
            return Err(NativeExchangeError::new(
                "native_exchange.consumer_mismatch",
                format!(
                    "native exchange consumer is `{}`, not `{consumer}`",
                    slot.consumer
                ),
            ));
        }
        if slot.state != NativeExchangeState::Available {
            return Err(NativeExchangeError::new(
                "native_exchange.already_claimed",
                "native exchange is not available",
            ));
        }
        let kind = slot
            .payload
            .as_ref()
            .map(NativeExchangePayload::kind)
            .unwrap_or("closed");
        if kind != expected_kind {
            return Err(NativeExchangeError::new(
                "native_exchange.kind_mismatch",
                format!("native exchange kind is `{kind}`, not `{expected_kind}`"),
            ));
        }
        slot.state = NativeExchangeState::Claimed;
        slot.payload.take().ok_or_else(|| {
            NativeExchangeError::new(
                "native_exchange.state",
                "available native exchange has no payload",
            )
        })
    }

    /// Closes a claimed token after consumer acceptance or rejection.
    pub(crate) fn close_claim(
        &mut self,
        token: NativeExchangeToken,
    ) -> Result<(), NativeExchangeError> {
        let slot = self.live_slot_mut(token)?;
        if slot.state != NativeExchangeState::Claimed {
            return Err(NativeExchangeError::new(
                "native_exchange.not_claimed",
                "native exchange has not been claimed",
            ));
        }
        close_slot(slot);
        Ok(())
    }

    /// Closes all exchanges owned by an exiting actor.
    pub(crate) fn close_owner(&mut self, owner_process_id: u64) {
        for slot in self.slots.values_mut() {
            if slot.owner_process_id == owner_process_id {
                close_slot(slot);
            }
        }
    }

    /// Closes all exchanges published by a failed native helper.
    pub(crate) fn close_producer(&mut self, producer: &str) {
        for slot in self.slots.values_mut() {
            if slot.producer == producer {
                close_slot(slot);
            }
        }
    }

    /// Closes every exchange during VM shutdown.
    pub(crate) fn shutdown(&mut self) {
        for slot in self.slots.values_mut() {
            close_slot(slot);
        }
    }

    fn live_slot_mut(
        &mut self,
        token: NativeExchangeToken,
    ) -> Result<&mut NativeExchangeSlot, NativeExchangeError> {
        let slot = self.slots.get_mut(&token.id).ok_or_else(|| {
            NativeExchangeError::new("native_exchange.stale", "native exchange token is not live")
        })?;
        if slot.generation != token.generation || slot.authentication != token.authentication {
            return Err(NativeExchangeError::new(
                "native_exchange.stale",
                "native exchange token is stale or forged",
            ));
        }
        if slot.state == NativeExchangeState::Closed {
            return Err(NativeExchangeError::new(
                "native_exchange.already_claimed",
                "native exchange token has already been consumed",
            ));
        }
        Ok(slot)
    }

    #[cfg(test)]
    fn cleanup_events(&self, token: NativeExchangeToken) -> u8 {
        self.slots
            .get(&token.id)
            .map_or(0, |slot| slot.cleanup_events)
    }
}

fn encode_token(token: NativeExchangeToken) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(28);
    bytes.extend_from_slice(TOKEN_MAGIC);
    bytes.extend_from_slice(&token.id.to_le_bytes());
    bytes.extend_from_slice(&token.generation.to_le_bytes());
    bytes.extend_from_slice(&token.authentication.to_le_bytes());
    bytes
}

fn decode_token(bytes: &[u8]) -> Result<Option<NativeExchangeToken>, NativeExchangeError> {
    if !bytes.starts_with(TOKEN_MAGIC) {
        return Ok(None);
    }
    if bytes.len() != 28 {
        return Err(NativeExchangeError::new(
            "native_exchange.token",
            "native exchange token has an invalid length",
        ));
    }
    let read = |offset| {
        let mut field = [0u8; 8];
        field.copy_from_slice(&bytes[offset..offset + 8]);
        u64::from_le_bytes(field)
    };
    Ok(Some(NativeExchangeToken::from_fields(
        read(4),
        read(12),
        read(20),
    )))
}

fn decode_tensor_packet(packet: &[u8]) -> Result<NativeTensorExchange, NativeExchangeError> {
    if packet.len() < TENSOR_PACKET_HEADER_BYTES
        || &packet[..4] != TENSOR_PACKET_MAGIC
        || packet[4] != 1
        || packet[5] != native_endian_code()
    {
        return Err(tensor_failure(
            "version",
            "tensor packet version, magic, or endianness is unsupported",
        ));
    }
    let dtype_lanes = u16::from_le_bytes([packet[8], packet[9]]);
    let rank = u32::from_le_bytes([packet[12], packet[13], packet[14], packet[15]]) as usize;
    if rank > MAX_RANK {
        return Err(tensor_failure(
            "rank",
            "tensor packet rank exceeds the VM exchange limit",
        ));
    }
    let metadata_bytes =
        TENSOR_PACKET_HEADER_BYTES
            .checked_add(rank.checked_mul(16).ok_or_else(|| {
                tensor_failure("overflow", "tensor packet metadata size overflowed")
            })?)
            .ok_or_else(|| tensor_failure("overflow", "tensor packet metadata size overflowed"))?;
    if packet.len() < metadata_bytes {
        return Err(tensor_failure(
            "shape",
            "tensor packet is truncated before shape and strides",
        ));
    }
    let read_i64 = |offset| {
        let mut field = [0u8; 8];
        field.copy_from_slice(&packet[offset..offset + 8]);
        i64::from_le_bytes(field)
    };
    let shape = (0..rank)
        .map(|index| read_i64(TENSOR_PACKET_HEADER_BYTES + index * 8))
        .collect::<Vec<_>>();
    let strides = (0..rank)
        .map(|index| read_i64(TENSOR_PACKET_HEADER_BYTES + rank * 8 + index * 8))
        .collect::<Vec<_>>();
    let tensor = NativeTensorExchange {
        version: 1,
        device_type: i32::from(packet[10]),
        device_id: i32::from(packet[11]),
        dtype_code: packet[6],
        dtype_bits: packet[7],
        dtype_lanes,
        shape,
        strides: Some(strides),
        byte_offset: 0,
        data: packet[metadata_bytes..].to_vec(),
    };
    validate_tensor(&tensor)?;
    Ok(tensor)
}

fn encode_tensor_packet(tensor: &NativeTensorExchange) -> Vec<u8> {
    let rank = tensor.shape.len();
    let metadata_bytes = TENSOR_PACKET_HEADER_BYTES + rank * 16;
    let mut packet = vec![0u8; metadata_bytes + tensor.data.len()];
    packet[..4].copy_from_slice(TENSOR_PACKET_MAGIC);
    packet[4] = 1;
    packet[5] = native_endian_code();
    packet[6] = tensor.dtype_code;
    packet[7] = tensor.dtype_bits;
    packet[8..10].copy_from_slice(&tensor.dtype_lanes.to_le_bytes());
    packet[10] = u8::try_from(tensor.device_type).unwrap_or(0);
    packet[11] = u8::try_from(tensor.device_id).unwrap_or(0);
    packet[12..16].copy_from_slice(&(rank as u32).to_le_bytes());
    for (index, dimension) in tensor.shape.iter().enumerate() {
        let offset = TENSOR_PACKET_HEADER_BYTES + index * 8;
        packet[offset..offset + 8].copy_from_slice(&dimension.to_le_bytes());
    }
    for (index, stride) in tensor.strides.as_ref().into_iter().flatten().enumerate() {
        let offset = TENSOR_PACKET_HEADER_BYTES + rank * 8 + index * 8;
        packet[offset..offset + 8].copy_from_slice(&stride.to_le_bytes());
    }
    packet[metadata_bytes..].copy_from_slice(&tensor.data);
    packet
}

fn native_endian_code() -> u8 {
    if cfg!(target_endian = "little") {
        1
    } else {
        2
    }
}

fn close_slot(slot: &mut NativeExchangeSlot) {
    if slot.state == NativeExchangeState::Closed {
        return;
    }
    slot.payload = None;
    slot.state = NativeExchangeState::Closed;
    slot.cleanup_events = slot.cleanup_events.saturating_add(1);
}

fn authenticate(seed: u64, id: u64, generation: u64) -> u64 {
    let mut value = seed ^ id.rotate_left(17) ^ generation.rotate_left(41);
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value.wrapping_mul(0x94d0_49bb_1331_11eb) ^ (value >> 31)
}

fn validate_identity(kind: &str, value: &str) -> Result<(), NativeExchangeError> {
    let mut chars = value.chars();
    let valid_start = chars.next().is_some_and(|ch| ch.is_ascii_lowercase());
    let valid_rest = chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_');
    if valid_start && valid_rest {
        Ok(())
    } else {
        Err(NativeExchangeError::new(
            "native_exchange.identity",
            format!("native exchange {kind} identity `{value}` is not canonical"),
        ))
    }
}

fn validate_tensor(tensor: &NativeTensorExchange) -> Result<(), NativeExchangeError> {
    if tensor.version != 1 {
        return tensor_error("version", "only DLPack major version 1 is supported");
    }
    if tensor.device_type != 1 || tensor.device_id != 0 {
        return tensor_error("device", "only CPU device 0 is supported");
    }
    if tensor.dtype_lanes != 1 {
        return tensor_error("dtype", "only scalar DLPack lanes are supported");
    }
    let element_bytes = match (tensor.dtype_code, tensor.dtype_bits) {
        (1, 8) | (6, 8) => 1usize,
        (0, 32) | (2, 32) => 4usize,
        (0, 64) | (2, 64) => 8usize,
        _ => {
            return tensor_error(
                "dtype",
                "supported DLPack dtypes are bool8, uint8, int32, int64, float32, and float64",
            )
        }
    };
    if tensor.shape.len() > MAX_RANK {
        return tensor_error("rank", "tensor rank exceeds the VM exchange limit");
    }
    if tensor.byte_offset != 0 {
        return tensor_error("byte_offset", "nonzero tensor byte offsets are unsupported");
    }
    let element_count = tensor.shape.iter().try_fold(1usize, |count, dimension| {
        let dimension = usize::try_from(*dimension)
            .map_err(|_| tensor_failure("shape", "tensor dimensions must be nonnegative"))?;
        count
            .checked_mul(dimension)
            .ok_or_else(|| tensor_failure("overflow", "tensor element count overflowed"))
    })?;
    let expected_bytes = element_count
        .checked_mul(element_bytes)
        .ok_or_else(|| tensor_failure("overflow", "tensor byte count overflowed"))?;
    if expected_bytes > MAX_EXCHANGE_BYTES {
        return tensor_error("size", "tensor exceeds the native exchange byte limit");
    }
    if tensor.data.len() != expected_bytes {
        return tensor_error(
            "byte_count",
            "tensor byte count does not match shape and dtype",
        );
    }
    if let Some(strides) = &tensor.strides {
        if strides.len() != tensor.shape.len() {
            return tensor_error("strides", "tensor stride rank does not match shape");
        }
        let mut expected = 1i64;
        for (dimension, stride) in tensor.shape.iter().zip(strides).rev() {
            if *stride != expected {
                return tensor_error("layout", "only contiguous row-major tensors are supported");
            }
            expected = expected
                .checked_mul(*dimension)
                .ok_or_else(|| tensor_failure("overflow", "tensor contiguous stride overflowed"))?;
        }
    }
    Ok(())
}

fn tensor_error<T>(suffix: &'static str, message: &'static str) -> Result<T, NativeExchangeError> {
    Err(tensor_failure(suffix, message))
}

fn tensor_failure(suffix: &'static str, message: &'static str) -> NativeExchangeError {
    NativeExchangeError::new(
        match suffix {
            "version" => "native_exchange.tensor.version",
            "device" => "native_exchange.tensor.device",
            "dtype" => "native_exchange.tensor.dtype",
            "rank" => "native_exchange.tensor.rank",
            "shape" => "native_exchange.tensor.shape",
            "byte_offset" => "native_exchange.tensor.byte_offset",
            "overflow" => "native_exchange.tensor.overflow",
            "size" => "native_exchange.tensor.size",
            "byte_count" => "native_exchange.tensor.byte_count",
            "strides" => "native_exchange.tensor.strides",
            "layout" => "native_exchange.tensor.layout",
            _ => "native_exchange.tensor.invalid",
        },
        message,
    )
}

#[cfg(test)]
#[path = "native_exchange_test.rs"]
#[cfg(test)]
mod native_exchange_test;
