//! Admitted image-local dispatch for pointer-free managed closures.

use std::collections::BTreeMap;

use crate::runtime::native_image::{TvmBoundaryType, TvmCallableDescriptor};

use super::closures::{
    validate_scalar_capture, MAX_CLOSURE_CAPTURES, MAX_CLOSURE_PARAMETERS, MAX_CLOSURE_RESULTS,
};
use super::{
    ActorHeap, ManagedClosure, ManagedClosureDescriptor, ManagedClosureImageGeneration,
    ManagedClosureView, ManagedMemoryError, TvmRef,
};

/// Closed callable membership admitted with one sealed executable generation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManagedClosureDispatchTable {
    generation: ManagedClosureImageGeneration,
    callables: BTreeMap<u64, TvmCallableDescriptor>,
}

impl ManagedClosureDispatchTable {
    /// Admits a sorted, unique, bounded callable table for one exact generation.
    pub fn admit(
        generation: ManagedClosureImageGeneration,
        callables: &[TvmCallableDescriptor],
    ) -> Result<Self, ManagedMemoryError> {
        let mut admitted = BTreeMap::new();
        let mut previous = None;
        for callable in callables {
            if callable.id == 0
                || previous.is_some_and(|id| id >= callable.id)
                || callable.parameters.len() > MAX_CLOSURE_PARAMETERS
                || callable.results.len() != MAX_CLOSURE_RESULTS
                || callable.captures.len() > MAX_CLOSURE_CAPTURES
                || callable
                    .parameters
                    .iter()
                    .chain(&callable.results)
                    .chain(&callable.captures)
                    .any(|boundary_type| matches!(boundary_type, TvmBoundaryType::Json))
            {
                return Err(ManagedMemoryError::InvalidClosure);
            }
            previous = Some(callable.id);
            admitted.insert(callable.id, callable.clone());
        }
        Ok(Self {
            generation,
            callables: admitted,
        })
    }

    /// Returns the generation whose sealed descriptor owns this table.
    pub fn generation(&self) -> ManagedClosureImageGeneration {
        self.generation
    }

    /// Builds an allocation descriptor only for a callable admitted by this image.
    pub fn closure_descriptor(
        &self,
        callable_id: u64,
    ) -> Result<ManagedClosureDescriptor, ManagedMemoryError> {
        let callable = self
            .callables
            .get(&callable_id)
            .ok_or(ManagedMemoryError::UnknownClosureCallable)?;
        ManagedClosureDescriptor::new(
            self.generation,
            callable.id,
            callable.parameters.clone(),
            callable.results.clone(),
            callable.captures.clone(),
        )
    }

    /// Resolves a decoded closure through exact generation, signature, and capture checks.
    pub fn resolve(
        &self,
        closure: &ManagedClosureView,
        generation: ManagedClosureImageGeneration,
        parameters: &[TvmBoundaryType],
        results: &[TvmBoundaryType],
    ) -> Result<ManagedClosureTarget, ManagedMemoryError> {
        if generation != self.generation || closure.generation != self.generation {
            return Err(ManagedMemoryError::StaleClosureGeneration);
        }
        let callable = self
            .callables
            .get(&closure.callable_id)
            .ok_or(ManagedMemoryError::UnknownClosureCallable)?;
        closure.validate_invocation(generation, parameters, results)?;
        if callable.parameters != parameters || callable.results != results {
            return Err(ManagedMemoryError::ClosureSignatureMismatch);
        }
        if callable.captures != closure.capture_types {
            return Err(ManagedMemoryError::ClosureCaptureMismatch);
        }
        Ok(ManagedClosureTarget {
            callable_id: callable.id,
            parameter_count: callable.parameters.len(),
            capture_count: callable.captures.len(),
        })
    }
}

/// Validated image-local native target without a code pointer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ManagedClosureTarget {
    callable_id: u64,
    parameter_count: usize,
    capture_count: usize,
}

impl ManagedClosureTarget {
    pub fn callable_id(self) -> u64 {
        self.callable_id
    }

    pub fn parameter_count(self) -> usize {
        self.parameter_count
    }

    pub fn capture_count(self) -> usize {
        self.capture_count
    }
}

/// Fully validated dispatch request passed to the existing image dispatch symbol.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManagedClosureInvocation {
    target: ManagedClosureTarget,
    words: Vec<i64>,
}

impl ManagedClosureInvocation {
    pub fn target(&self) -> ManagedClosureTarget {
        self.target
    }

    /// Captures followed by caller arguments in the lifted function's ABI order.
    pub fn words(&self) -> &[i64] {
        &self.words
    }
}

impl ActorHeap {
    /// Validates and prepares one managed closure call before native dispatch.
    pub fn prepare_closure_invocation(
        &self,
        closure: TvmRef<ManagedClosure>,
        table: &ManagedClosureDispatchTable,
        generation: ManagedClosureImageGeneration,
        parameter_types: &[TvmBoundaryType],
        parameter_words: &[i64],
        result_types: &[TvmBoundaryType],
    ) -> Result<ManagedClosureInvocation, ManagedMemoryError> {
        if parameter_types.len() != parameter_words.len() {
            return Err(ManagedMemoryError::ClosureSignatureMismatch);
        }
        let closure = self.closure_view(closure)?;
        let target = table.resolve(&closure, generation, parameter_types, result_types)?;
        for (boundary_type, word) in parameter_types.iter().zip(parameter_words) {
            self.validate_closure_word(boundary_type, *word)?;
        }
        for (boundary_type, word) in closure.capture_types.iter().zip(&closure.capture_words) {
            self.validate_closure_word(boundary_type, *word)?;
        }
        let mut words = Vec::with_capacity(closure.capture_words.len() + parameter_words.len());
        words.extend_from_slice(&closure.capture_words);
        words.extend_from_slice(parameter_words);
        Ok(ManagedClosureInvocation { target, words })
    }

    fn validate_closure_word(
        &self,
        boundary_type: &TvmBoundaryType,
        word: i64,
    ) -> Result<(), ManagedMemoryError> {
        validate_scalar_capture(boundary_type, word)?;
        let semantic = match boundary_type {
            TvmBoundaryType::String => {
                Some(super::SemanticTypeId::from_canonical("std.core.String")?)
            }
            TvmBoundaryType::Bytes => {
                Some(super::SemanticTypeId::from_canonical("std.binary.Bytes")?)
            }
            TvmBoundaryType::Binary => {
                Some(super::SemanticTypeId::from_canonical("std.binary.Binary")?)
            }
            TvmBoundaryType::Managed(identity) => {
                Some(super::SemanticTypeId::from_bytes(*identity))
            }
            _ => None,
        };
        if let Some(semantic) = semantic {
            self.validate_abi_reference(u64::from_ne_bytes(word.to_ne_bytes()), semantic)?;
        }
        Ok(())
    }
}
