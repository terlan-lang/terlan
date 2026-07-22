# `std.random`

`std.random` contains VM-owned random generator contracts.

The module keeps deterministic and nondeterministic randomness explicit:
seeded generators are reproducible and suitable for tests, while entropy
generators are seeded from OS randomness through maintained Rust crates.
Drawing a value returns the next generator state alongside the value so callers
do not depend on hidden global RNG state.
