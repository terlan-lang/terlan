//! Pointer-free owned closures stored in one actor-local moving heap.

use std::fmt::Write;
use std::sync::Arc;

use sha2::{Digest, Sha256};

use crate::runtime::native_image::TvmBoundaryType;

use super::{
    ActorHeap, AllocationClass, ManagedMemoryError, ManagedTypeDescriptor, SemanticTypeId, TvmRef,
};

const MAGIC: &[u8; 8] = b"TVMCL001";
const VERSION: u16 = 1;
const FIXED_HEADER_BYTES: usize = 64;
const TYPE_WORDS: usize = 3;
const TYPE_BYTES: usize = TYPE_WORDS * size_of::<i64>();
pub(super) const MAX_CLOSURE_PARAMETERS: usize = 256;
pub(super) const MAX_CLOSURE_RESULTS: usize = 1;
pub(super) const MAX_CLOSURE_CAPTURES: usize = 256;

/// Content-addressed identity of one admitted executable-image generation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ManagedClosureImageGeneration([u8; 32]);

impl ManagedClosureImageGeneration {
    /// Creates a nonzero generation from an admitted descriptor digest.
    pub fn new(digest: [u8; 32]) -> Result<Self, ManagedMemoryError> {
        if digest == [0; 32] {
            return Err(ManagedMemoryError::InvalidClosure);
        }
        Ok(Self(digest))
    }

    /// Returns the descriptor digest carried by every closure of this generation.
    pub fn digest(self) -> [u8; 32] {
        self.0
    }
}

/// Immutable closure shape embedded into one actor-owned managed object.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManagedClosureDescriptor {
    generation: ManagedClosureImageGeneration,
    callable_id: u64,
    parameters: Box<[TvmBoundaryType]>,
    results: Box<[TvmBoundaryType]>,
    captures: Box<[TvmBoundaryType]>,
    encoded_shape: Arc<[u8]>,
    managed_type: Arc<ManagedTypeDescriptor>,
}

impl ManagedClosureDescriptor {
    /// Derives the managed identity shared by closures with one call signature.
    pub fn semantic_id_for_signature(
        parameters: &[TvmBoundaryType],
        results: &[TvmBoundaryType],
    ) -> Result<SemanticTypeId, ManagedMemoryError> {
        closure_semantic_id(parameters, results)
    }

    /// Defines one exact image-local callable and its typed capture environment.
    pub fn new(
        generation: ManagedClosureImageGeneration,
        callable_id: u64,
        parameters: Vec<TvmBoundaryType>,
        results: Vec<TvmBoundaryType>,
        captures: Vec<TvmBoundaryType>,
    ) -> Result<Self, ManagedMemoryError> {
        if callable_id == 0
            || parameters.len() > MAX_CLOSURE_PARAMETERS
            || results.is_empty()
            || results.len() > MAX_CLOSURE_RESULTS
            || captures.len() > MAX_CLOSURE_CAPTURES
            || captures
                .iter()
                .any(|capture| matches!(capture, TvmBoundaryType::Json))
        {
            return Err(ManagedMemoryError::InvalidClosure);
        }
        let encoded_shape =
            encode_shape(generation, callable_id, &parameters, &results, &captures)?;
        let capture_offset = encoded_shape.len();
        let size = capture_offset
            .checked_add(
                captures
                    .len()
                    .checked_mul(size_of::<i64>())
                    .ok_or(ManagedMemoryError::InvalidClosure)?,
            )
            .ok_or(ManagedMemoryError::InvalidClosure)?;
        let reference_offsets = captures
            .iter()
            .enumerate()
            .filter(|(_, capture)| capture.is_managed_reference())
            .map(|(index, _)| capture_offset + index * size_of::<i64>())
            .collect::<Vec<_>>();
        let semantic = closure_semantic_id(&parameters, &results)?;
        let managed_type = Arc::new(ManagedTypeDescriptor::new_specialized(
            semantic,
            size,
            align_of::<u64>(),
            reference_offsets,
            AllocationClass::Young,
            &encoded_shape,
        )?);
        Ok(Self {
            generation,
            callable_id,
            parameters: parameters.into_boxed_slice(),
            results: results.into_boxed_slice(),
            captures: captures.into_boxed_slice(),
            encoded_shape: Arc::from(encoded_shape),
            managed_type,
        })
    }

    /// Returns the executable-image generation that owns this callable.
    pub fn generation(&self) -> ManagedClosureImageGeneration {
        self.generation
    }

    /// Returns the callable identifier within the owning image.
    pub fn callable_id(&self) -> u64 {
        self.callable_id
    }

    /// Returns the ordered caller-parameter ABI types.
    pub fn parameters(&self) -> &[TvmBoundaryType] {
        &self.parameters
    }

    /// Returns the ordered result ABI types.
    pub fn results(&self) -> &[TvmBoundaryType] {
        &self.results
    }

    /// Returns the ordered capture ABI types.
    pub fn captures(&self) -> &[TvmBoundaryType] {
        &self.captures
    }

    /// Returns the content-derived managed type identity for this signature.
    pub fn semantic_id(&self) -> SemanticTypeId {
        self.managed_type.semantic_id()
    }

    /// Rejects stale generations and ABI-incompatible indirect calls.
    pub fn validate_invocation(
        &self,
        generation: ManagedClosureImageGeneration,
        parameters: &[TvmBoundaryType],
        results: &[TvmBoundaryType],
    ) -> Result<(), ManagedMemoryError> {
        if generation != self.generation {
            return Err(ManagedMemoryError::StaleClosureGeneration);
        }
        if parameters != self.parameters() || results != self.results() {
            return Err(ManagedMemoryError::ClosureSignatureMismatch);
        }
        Ok(())
    }
}

/// Marker for one immutable closure allocated in an actor heap.
#[derive(Debug)]
pub struct ManagedClosure;

/// Decoded owned closure data after actor/generation validation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManagedClosureView {
    pub generation: ManagedClosureImageGeneration,
    pub callable_id: u64,
    pub parameters: Vec<TvmBoundaryType>,
    pub results: Vec<TvmBoundaryType>,
    pub capture_types: Vec<TvmBoundaryType>,
    pub capture_words: Vec<i64>,
}

impl ManagedClosureView {
    /// Rejects a stale generation or an ABI-incompatible indirect invocation.
    pub fn validate_invocation(
        &self,
        generation: ManagedClosureImageGeneration,
        parameters: &[TvmBoundaryType],
        results: &[TvmBoundaryType],
    ) -> Result<(), ManagedMemoryError> {
        if generation != self.generation {
            return Err(ManagedMemoryError::StaleClosureGeneration);
        }
        if parameters != self.parameters || results != self.results {
            return Err(ManagedMemoryError::ClosureSignatureMismatch);
        }
        Ok(())
    }
}

impl ActorHeap {
    /// Allocates one immutable closure with a precise capture reference map.
    pub fn allocate_closure(
        &mut self,
        descriptor: &ManagedClosureDescriptor,
        capture_words: &[i64],
    ) -> Result<TvmRef<ManagedClosure>, ManagedMemoryError> {
        if capture_words.len() != descriptor.captures.len() {
            return Err(ManagedMemoryError::InvalidClosure);
        }
        let mut references = Vec::new();
        for (index, (capture_type, word)) in
            descriptor.captures.iter().zip(capture_words).enumerate()
        {
            validate_scalar_capture(capture_type, *word)?;
            if let Some(semantic) = managed_semantic_id(capture_type)? {
                let encoded = u64::from_ne_bytes(word.to_ne_bytes());
                references.push((
                    descriptor.encoded_shape.len() + index * size_of::<i64>(),
                    self.validate_abi_reference(encoded, semantic)?,
                ));
            }
        }
        let mut payload = Vec::with_capacity(descriptor.managed_type.size());
        payload.extend_from_slice(&descriptor.encoded_shape);
        for word in capture_words {
            payload.extend_from_slice(&word.to_ne_bytes());
        }
        self.allocate(descriptor.managed_type.clone(), &payload, &references)
    }

    /// Reads a closure without exposing a code, stack, thread, or worker pointer.
    pub fn closure_view(
        &self,
        closure: TvmRef<ManagedClosure>,
    ) -> Result<ManagedClosureView, ManagedMemoryError> {
        let payload = self.read(closure)?;
        let view = decode_view(payload)?;
        let semantic = closure_semantic_id(&view.parameters, &view.results)?;
        if self.descriptor(closure)?.semantic_id() != semantic {
            return Err(ManagedMemoryError::InvalidClosure);
        }
        Ok(view)
    }
}

fn encode_shape(
    generation: ManagedClosureImageGeneration,
    callable_id: u64,
    parameters: &[TvmBoundaryType],
    results: &[TvmBoundaryType],
    captures: &[TvmBoundaryType],
) -> Result<Vec<u8>, ManagedMemoryError> {
    let mut encoded = Vec::with_capacity(
        FIXED_HEADER_BYTES + (parameters.len() + results.len() + captures.len()) * TYPE_BYTES,
    );
    encoded.extend_from_slice(MAGIC);
    encoded.extend_from_slice(&VERSION.to_le_bytes());
    encoded.extend_from_slice(&[0; 6]);
    encoded.extend_from_slice(&generation.digest());
    encoded.extend_from_slice(&callable_id.to_le_bytes());
    push_count(&mut encoded, parameters.len())?;
    push_count(&mut encoded, results.len())?;
    push_count(&mut encoded, captures.len())?;
    encoded.extend_from_slice(&[0; 2]);
    debug_assert_eq!(encoded.len(), FIXED_HEADER_BYTES);
    for boundary_type in parameters.iter().chain(results).chain(captures) {
        for word in boundary_type.transition_words() {
            encoded.extend_from_slice(&word.to_le_bytes());
        }
    }
    Ok(encoded)
}

fn decode_view(payload: &[u8]) -> Result<ManagedClosureView, ManagedMemoryError> {
    if payload.len() < FIXED_HEADER_BYTES
        || payload.get(..8) != Some(MAGIC)
        || payload.get(8..10) != Some(&VERSION.to_le_bytes())
        || payload.get(10..16) != Some(&[0; 6])
    {
        return Err(ManagedMemoryError::InvalidClosure);
    }
    let generation = ManagedClosureImageGeneration::new(array(payload, 16)?)?;
    let callable_id = u64::from_le_bytes(array(payload, 48)?);
    if callable_id == 0 {
        return Err(ManagedMemoryError::InvalidClosure);
    }
    let parameter_count = count(payload, 56, MAX_CLOSURE_PARAMETERS)?;
    let result_count = count(payload, 58, MAX_CLOSURE_RESULTS)?;
    let capture_count = count(payload, 60, MAX_CLOSURE_CAPTURES)?;
    if result_count == 0 || payload.get(62..64) != Some(&[0; 2]) {
        return Err(ManagedMemoryError::InvalidClosure);
    }
    let type_count = parameter_count + result_count + capture_count;
    let capture_offset = FIXED_HEADER_BYTES
        .checked_add(type_count * TYPE_BYTES)
        .ok_or(ManagedMemoryError::InvalidClosure)?;
    let expected = capture_offset
        .checked_add(capture_count * size_of::<i64>())
        .ok_or(ManagedMemoryError::InvalidClosure)?;
    if payload.len() != expected {
        return Err(ManagedMemoryError::InvalidClosure);
    }
    let mut types = Vec::with_capacity(type_count);
    for index in 0..type_count {
        let offset = FIXED_HEADER_BYTES + index * TYPE_BYTES;
        let words = [
            i64::from_le_bytes(array(payload, offset)?),
            i64::from_le_bytes(array(payload, offset + 8)?),
            i64::from_le_bytes(array(payload, offset + 16)?),
        ];
        types.push(
            TvmBoundaryType::from_transition_words(&words)
                .map_err(|_| ManagedMemoryError::InvalidClosure)?,
        );
    }
    let results_at = parameter_count;
    let captures_at = results_at + result_count;
    let capture_words = (0..capture_count)
        .map(|index| array(payload, capture_offset + index * 8).map(i64::from_ne_bytes))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ManagedClosureView {
        generation,
        callable_id,
        parameters: types[..results_at].to_vec(),
        results: types[results_at..captures_at].to_vec(),
        capture_types: types[captures_at..].to_vec(),
        capture_words,
    })
}

fn closure_semantic_id(
    parameters: &[TvmBoundaryType],
    results: &[TvmBoundaryType],
) -> Result<SemanticTypeId, ManagedMemoryError> {
    let mut signature = Vec::new();
    signature.extend_from_slice(
        &u16::try_from(parameters.len())
            .map_err(|_| ManagedMemoryError::InvalidClosure)?
            .to_le_bytes(),
    );
    signature.extend_from_slice(
        &u16::try_from(results.len())
            .map_err(|_| ManagedMemoryError::InvalidClosure)?
            .to_le_bytes(),
    );
    for boundary_type in parameters.iter().chain(results) {
        for word in boundary_type.transition_words() {
            signature.extend_from_slice(&word.to_le_bytes());
        }
    }
    let digest = Sha256::digest(signature);
    let mut canonical = String::from("terlan.runtime.Closure.v1:");
    for byte in digest {
        write!(&mut canonical, "{byte:02x}").map_err(|_| ManagedMemoryError::InvalidClosure)?;
    }
    SemanticTypeId::from_canonical(&canonical)
}

fn managed_semantic_id(
    boundary_type: &TvmBoundaryType,
) -> Result<Option<SemanticTypeId>, ManagedMemoryError> {
    let canonical = match boundary_type {
        TvmBoundaryType::String => Some("std.core.String"),
        TvmBoundaryType::Bytes => Some("std.binary.Bytes"),
        TvmBoundaryType::Binary => Some("std.binary.Binary"),
        TvmBoundaryType::Managed(identity) => {
            return Ok(Some(SemanticTypeId::from_bytes(*identity)))
        }
        _ => None,
    };
    canonical.map(SemanticTypeId::from_canonical).transpose()
}

pub(super) fn validate_scalar_capture(
    boundary_type: &TvmBoundaryType,
    word: i64,
) -> Result<(), ManagedMemoryError> {
    match boundary_type {
        TvmBoundaryType::Unit if word != 0 => Err(ManagedMemoryError::InvalidManagedScalar),
        TvmBoundaryType::Bool if !matches!(word, 0 | 1) => {
            Err(ManagedMemoryError::InvalidManagedScalar)
        }
        TvmBoundaryType::Json => Err(ManagedMemoryError::InvalidClosure),
        _ => Ok(()),
    }
}

fn push_count(encoded: &mut Vec<u8>, count: usize) -> Result<(), ManagedMemoryError> {
    encoded.extend_from_slice(
        &u16::try_from(count)
            .map_err(|_| ManagedMemoryError::InvalidClosure)?
            .to_le_bytes(),
    );
    Ok(())
}

fn count(payload: &[u8], offset: usize, maximum: usize) -> Result<usize, ManagedMemoryError> {
    let count = u16::from_le_bytes(array(payload, offset)?) as usize;
    (count <= maximum)
        .then_some(count)
        .ok_or(ManagedMemoryError::InvalidClosure)
}

fn array<const N: usize>(payload: &[u8], offset: usize) -> Result<[u8; N], ManagedMemoryError> {
    payload
        .get(offset..offset + N)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or(ManagedMemoryError::InvalidClosure)
}
