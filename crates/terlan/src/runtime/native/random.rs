//! Random adapter operations for `std.random.Random`.
//!
//! This module owns Terlan's portable random-number generator implementation.
//! It delegates deterministic and OS-seeded randomness to the maintained
//! `rand` and `rand_chacha` crates and keeps generator state explicit.

use std::sync::atomic::{AtomicU64, Ordering};

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha12Rng;

static ENTROPY_GENERATOR_ID: AtomicU64 = AtomicU64::new(1);

/// Explicit random generator state owned by the VM/native adapter.
#[derive(Clone, Debug)]
pub struct Generator {
    rng: ChaCha12Rng,
    identity: GeneratorIdentity,
    steps: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum GeneratorIdentity {
    Seed(u64),
    Entropy(u64),
}

impl Generator {
    /// Builds a deterministic generator from unsigned seed material.
    pub fn from_seed(seed: u64) -> Self {
        Self {
            rng: ChaCha12Rng::seed_from_u64(seed),
            identity: GeneratorIdentity::Seed(seed),
            steps: 0,
        }
    }

    /// Builds a generator seeded from OS randomness.
    pub fn from_entropy() -> Self {
        Self {
            rng: ChaCha12Rng::from_os_rng(),
            identity: GeneratorIdentity::Entropy(
                ENTROPY_GENERATOR_ID.fetch_add(1, Ordering::Relaxed),
            ),
            steps: 0,
        }
    }

    /// Returns a stable fingerprint for hashing and equality.
    pub fn fingerprint(&self) -> String {
        match self.identity {
            GeneratorIdentity::Seed(seed) => format!("seed:{seed}:{}", self.steps),
            GeneratorIdentity::Entropy(id) => format!("entropy:{id}:{}", self.steps),
        }
    }

    fn advance(&mut self) {
        self.steps = self.steps.saturating_add(1);
    }
}

impl PartialEq for Generator {
    fn eq(&self, other: &Self) -> bool {
        self.identity == other.identity && self.steps == other.steps
    }
}

impl Eq for Generator {}

/// Portable random error returned by random operations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RandomError {
    code: &'static str,
    message: String,
    offset: usize,
}

impl RandomError {
    /// Builds a portable random error.
    pub fn new(code: &'static str, message: impl Into<String>, offset: usize) -> Self {
        Self {
            code,
            message: message.into(),
            offset,
        }
    }

    /// Returns the stable machine-readable error code.
    pub fn code(&self) -> &'static str {
        self.code
    }

    /// Returns the human-readable diagnostic message.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns the source offset associated with the error.
    pub fn offset(&self) -> usize {
        self.offset
    }
}

/// Creates a deterministic generator from Terlan `Int` seed material.
pub fn seed(seed: i64) -> Result<Generator, RandomError> {
    let seed = u64::try_from(seed).map_err(|_| {
        RandomError::new(
            "random.invalid_seed",
            "seed must be a non-negative integer",
            0,
        )
    })?;
    Ok(Generator::from_seed(seed))
}

/// Creates a generator seeded from OS randomness.
pub fn entropy() -> Generator {
    Generator::from_entropy()
}

/// Draws one unbounded integer and returns advanced generator state.
pub fn int(generator: &Generator) -> (Generator, i64) {
    let mut next = generator.clone();
    let value = next.rng.random::<i64>();
    next.advance();
    (next, value)
}

/// Draws one bounded integer in `[min, max)`.
pub fn bounded_int(
    generator: &Generator,
    min: i64,
    max: i64,
) -> Result<(Generator, i64), RandomError> {
    if min >= max {
        return Err(RandomError::new(
            "random.invalid_bounds",
            "bounded_int expects min to be less than max",
            0,
        ));
    }
    let mut next = generator.clone();
    let value = next.rng.random_range(min..max);
    next.advance();
    Ok((next, value))
}

/// Draws one finite float in `[0.0, 1.0)`.
pub fn float(generator: &Generator) -> (Generator, f64) {
    let mut next = generator.clone();
    let value = next.rng.random::<f64>();
    next.advance();
    (next, value)
}

/// Draws one boolean value.
pub fn bool(generator: &Generator) -> (Generator, bool) {
    let mut next = generator.clone();
    let value = next.rng.random::<bool>();
    next.advance();
    (next, value)
}

/// Chooses one index from a non-empty collection length.
pub fn choice_index(generator: &Generator, len: usize) -> Result<(Generator, usize), RandomError> {
    if len == 0 {
        return Err(RandomError::new(
            "random.empty_choice",
            "choice expects at least one value",
            0,
        ));
    }
    let mut next = generator.clone();
    let value = next.rng.random_range(0..len);
    next.advance();
    Ok((next, value))
}

/// Chooses one index from a non-empty collection length.
pub fn choice(generator: &Generator, len: usize) -> Result<(Generator, usize), RandomError> {
    choice_index(generator, len)
}

/// Builds a shuffled index order for a collection length.
pub fn shuffled_indices(generator: &Generator, len: usize) -> (Generator, Vec<usize>) {
    let mut next = generator.clone();
    let mut indices = (0..len).collect::<Vec<_>>();
    use rand::seq::SliceRandom;
    indices.shuffle(&mut next.rng);
    next.advance();
    (next, indices)
}

/// Builds a shuffled index order for a collection length.
pub fn shuffle(generator: &Generator, len: usize) -> (Generator, Vec<usize>) {
    shuffled_indices(generator, len)
}

/// Builds sample indices for a collection length without replacement.
pub fn sample_indices(
    generator: &Generator,
    len: usize,
    count: i64,
) -> Result<(Generator, Vec<usize>), RandomError> {
    let count = usize::try_from(count).map_err(|_| {
        RandomError::new(
            "random.invalid_sample_size",
            "sample size must be non-negative",
            0,
        )
    })?;
    if count > len {
        return Err(RandomError::new(
            "random.sample_too_large",
            "sample size must be less than or equal to the input length",
            0,
        ));
    }
    let (next, indices) = shuffled_indices(generator, len);
    Ok((next, indices.into_iter().take(count).collect()))
}

/// Builds sample indices for a collection length without replacement.
pub fn sample(
    generator: &Generator,
    len: usize,
    count: i64,
) -> Result<(Generator, Vec<usize>), RandomError> {
    sample_indices(generator, len, count)
}

#[cfg(test)]
#[path = "random_test.rs"]
mod random_test;
