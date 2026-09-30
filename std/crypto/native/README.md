# Crypto Native Ownership

The package owns maintained-provider calls for Ed25519 and pure SHA-256
operations. `std.crypto.Hash.sha256` is an explicit native declaration, not a
compiler intrinsic. Text, bytes, and framed-field exports use the shared
copied-value binding registry; compiler and VM execution do not select hashing
behavior from source function names or a dedicated digest opcode.

SHA-256 remains implemented by `sha2`. Package code supplies only the documented
UTF-8, field-length, domain, and NUL-separator framing. The shared ABI validates
borrowed bytes and typed lists without payload coercion.

Filesystem hashes, artifact inventories, and their path-admission policy still
live in the legacy host dispatcher. Their migration is not complete and must
preserve streaming I/O, cancellation, and filesystem authority rather than
registering blocking file work as a nonblocking copied-value operation.

Tests cover standard vectors, binary and Unicode inputs, ordered framing,
wrong argument shapes, source-body authority, compiled Terlan package calls,
and rejection of the retired managed digest opcode.
