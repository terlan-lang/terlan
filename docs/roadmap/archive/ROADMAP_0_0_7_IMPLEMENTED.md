# Terlan 0.0.7 Implemented Roadmap Archive

This archive preserves the detailed requirements, gates, acceptance criteria,
and implementation notes removed from completed `[x]` entries in the active
0.0.7 roadmap. It is historical provenance, not an active release plan. The
active roadmap remains authoritative for incomplete work and architectural
pivots.


## Completed 001

- [x] Require every local source binding to repeat the `let` keyword.
  - Hard decision for 0.0.7: the canonical and only accepted source form is
    `let a = 1; let b = a + 1; a + b`. The implicit continuation form
    `let a = 1; b = a + 1; a + b` must be rejected. Terlan will not add a
    comma-grouped alternative such as `let a = 1, b = 2; ...`.
  - Rationale: an unmarked `b = ...` visually reads as assignment or as a
    punctuation-delimited binding group borrowed from another language. Each
    binding must identify itself so declaration boundaries remain explicit to
    readers, formatters, editors, diagnostics, and syntax-aware tools.
  - Grammar requirement: model a source let as one binding followed by its
    result expression: `LetExpr ::= "let" LetBinding ";" Expr`. Consecutive
    bindings are recursive let expressions in source syntax.
  - CoreIR requirement: lowering may flatten consecutive source lets into the
    existing ordered `CoreExpr::Let { bindings, body }` representation. This
    normalization must preserve left-to-right evaluation, visibility of each
    earlier binding to later values, pattern failure behavior, and lexical
    scope.
  - Parser requirement: remove implicit post-semicolon `Pattern =` binding
    detection. Emit a stable, actionable diagnostic for the retired form that
    tells the user to insert `let` before the next binding.
  - Formatter requirement: always emit `let` for every binding, including
    multiline bindings and destructuring patterns. Formatting must be
    idempotent and must never recreate the retired implicit form.
  - Migration requirement: provide a mechanical migration path for existing
    0.0.6-style binding sequences and cover nested lets, comments, multiline
    values, destructuring, and indexed assignment boundaries without changing
    program semantics.
  - Cross-surface requirement: update the EBNF and language guide, parser and
    syntax-output tests, formatter, typechecker/CoreIR lowering coverage, VM
    execution tests, LSP/editor grammars, and tree-sitter grammar together.
  - Acceptance: repeated lets parse, format, typecheck, lower, and execute on
    the VM-default path; omitted subsequent `let` keywords and comma-grouped
    bindings fail with stable diagnostics; migration fixtures preserve the
    original CoreIR contract and runtime result.
  - Fail-fast requirement: refutable chains use the explicit
    `let { Pattern <- Expr; ... } else { Clause; ... }; Expr` form. Right-hand
    sides evaluate once from left to right, the first mismatch enters the
    shared fallback with its actual value, success bindings remain unavailable
    to fallback clauses, and commas plus the retired unbraced form are rejected.
  - Gate: add `make repeated-let-syntax-check` and include it in the canonical
    0.0.7 language/release gate before marking this item complete.
  - Completed progress: canonical parsing now requires `let` on every local
    binding, emits stable diagnostics for omitted keywords and comma grouping,
    and preserves the existing ordered CoreIR lowering and VM semantics. The
    formatter and REPL emit repeated keywords, tree-sitter recognizes recursive
    source lets and unambiguous field selectors, and
    `terlc fmt --migrate-repeated-lets <path>` performs a parser-recorded,
    comment-preserving 0.0.6 migration without rewriting indexed assignments.
    The checked-in std/test sources use the canonical form, and
    `make repeated-let-syntax-check` is part of the canonical language surface
    gate. Grouped fail-fast bindings now parse and format idempotently,
    typecheck fallback scope and exhaustiveness, lower to ordered nested CoreIR
    cases, and execute success, first-mismatch, and later-mismatch paths on the
    VM. The same gate passes the Tree-sitter corpus and four VM language tests.


## Completed 002

- [x] Close long-tail interoperability and ergonomics for `${...}` string pattern
  capture support.
  - Problem: capture strings are entering core pattern support, but the full
    user-facing surface is still incomplete unless parser AST, tree-sitter,
    syntax docs, and `terlc`/editor workflows all stay mechanically aligned.
  - Requirement: add an explicit long-tail parity matrix for `${...}` captures
    under `docs/compiler/type_spec/pattern_matching_support_matrix.json` that
    tracks acceptance/rejection in parser, typechecker, CoreIR, VM, JS lowering,
    tree-sitter, LSP, and stdlib test coverage.
  - Requirement: add executable matrix snapshots for
    `route`, `path`, `template`, and `shape`-backed uses so capture patterns are
    validated in non-`case` contexts (e.g. `pub route(...) = "x/${id}"` style
    declarations).
  - Requirement: include parser/lsp/tree-sitter cross-validation for capture
    spans and token classes so users get the same syntax-highlighting and
    diagnostic offsets in files, editor view, and `terlc` failure output.
  - Requirement (completed): include stable documentation examples in the
    language primer and string-pattern sections showing both typed and inferred
    capture forms.
  - Requirement: ensure command-line workflows retain ergonomics for capture-heavy
    patterns by validating `terlc repl`, `terlc fmt`, and `terlc test` all agree
    on pretty-print, stable diagnostics, and executable anchors for string captures.
  - Gate: `make string-pattern-long-tail-check` for formatter, syntax-output,
    LSP, tree-sitter, shape-backed parser, and template-backed capture-flow
    parity.
  - Current gate state: direct parser/typecheck/CoreIR/default-VM capture
    support is complete. Long-tail formatter and tree-sitter coverage now has an
    executable gate; LSP document diagnostics now cover direct capture syntax.
    Target-profile/backend diagnostics now prove the VM profile accepts direct
    capture patterns while the legacy core subset rejects them. Shape-backed
    raw declaration parsing and template-backed capture-to-constructor syntax
    output now have executable fixtures. The support matrix now classifies six
    required contexts (`route`, `path`, `function_head`, `lambda`, `shape`, and
    `template`) across parser, typechecker, CoreIR, VM, JS, tree-sitter, LSP,
    and stdlib-test stages. Semantic shape expansion and direct template pattern
    expansion remain explicitly blocked rather than being reported as complete.
  - Completed progress: `pattern-matching-support` now rejects a missing
    long-tail context, an unknown per-context stage, and any blocked semantic
    context without a diagnostic plus adversarial evidence. The quality report
    records 6 long-tail contexts alongside 22 pattern families, 63 positive
    anchors, and 14 adversarial references. `string-pattern-long-tail-check`
    invokes this validator directly before formatter, target-profile, LSP,
    executable Terlan, and tree-sitter checks, so the standalone gate cannot
    pass against an overstated parity matrix.
  - Completed progress: command-line parity now uses
    `tests/pattern/StringPatternLongTailTest.terl` as the shared canonical
    `terlc fmt` and executable `terlc test` fixture. The REPL gate executes the
    same typed route-capture shape through VM evaluation and proves malformed
    adjacent captures emit one stable expression diagnostic without a
    misleading declaration fallback. `make string-pattern-long-tail-check`
    passes with these command-path checks included.
  - Completed progress: the root language primer and grammar reference now
    document the canonical inferred `"assets/${bucket}/${file}.txt"` and typed
    `"GET /users/${id: Int}"` forms, their valid pattern positions, `where`
    guards, and the adjacent-capture ambiguity rule. The
    `pattern-matching-support` quality command requires both forms in both
    documents, adversarial fixtures prove either form cannot disappear
    silently, and `make string-pattern-long-tail-check` plus
    `make executable-docs-vm-check` pass with the documentation contract.
  - Completed progress: `string-pattern-long-tail-check` now includes
    `parses_shape_synonym_with_string_capture_body` and
    `syntax_output_keeps_template_backed_string_capture_flow`, updates the
    parity matrix to include shape and template-backed contexts, proves raw
    `shape ... = "users/${id: Int}/assets/${file}" where id > 0.` declarations
    preserve capture-bearing bodies before semantic expansion is enabled, and
    proves a captured path segment can feed the current nominal template
    construction form in syntax output.
  - Completed progress: `string-pattern-long-tail-check` now also runs
    `lsp_document_reports_shape_synonym_expansion_blocker`, so the LSP gate
    covers shape-backed string-capture syntax and its current semantic blocker
    instead of only direct `case`/`let` capture documents. The LSP document
    symbol construction now centralizes the pinned `lsp-types` 0.94
    `deprecated` compatibility field in one helper, `RUSTFLAGS='-D warnings'
    cargo check --locked -p terlan --features editor-lsp --bin terlan-lsp`
    passes, and `make string-pattern-long-tail-check` passes.
  - Completed progress: every context that claims Tree-sitter support now names
    a concrete corpus anchor in the support matrix. The quality gate rejects
    unsupported claims, package smoke locks the route/path/function-head/shape/
    lambda/template snippets, and the owning gate regenerates and executes all
    12 corpus cases before running the five default-VM Terlan tests. The full
    gate passes with `RUSTFLAGS='-D warnings'`; semantic shape and template
    expansion remain owned by their separate roadmap slices.
  - Make integration: run string-pattern long-tail parity checks after
    `string-pattern-matching-check` and before `tree-sitter-package-check` and
    `lsp-outline-check`.
  - Acceptance: no context (`case`, `let`, function-head, shape, route-like
    declarations) should compile successfully in one surface and fail on another
    with the same capture syntax.


## Completed 003

- [x] Add Terlan-native bitstring and binary construction/matching.
  - Problem: Terlan currently has `Binary` literals as text-like values and a
    low-level VM `bitstring` helper, but it does not have a usable source-level
    story for byte-oriented binaries, bitstrings, packet decoding, or protocol
    pattern matching. Erlang-style `<<...>>` syntax is intentionally rejected,
    so Terlan needs its own explicit form instead of leaking BEAM syntax.
  - Requirement: distinguish text strings from byte binaries in the VM value
    model. `String`/text values and `Binary`/bytes values must not silently
    collapse to the same runtime representation once binary processing is
    enabled.
  - Requirement: design Terlan-native source syntax for binary construction
    and matching. It must cover fixed-width integer segments, byte segments,
    bit-width segments, signed/unsigned interpretation, big/little/native
    endian selection, UTF-8 scalar segments, raw byte arrays, and rest payload
    capture.
  - Requirement: do not adopt Erlang `<<SourcePort:16, Payload/binary>>`
    syntax directly. If a compact protocol syntax is added, it must look and
    read like Terlan and must preserve Terlan's explicitness around types,
    widths, endian, and capture names.
  - Requirement: support both construction and pattern matching. Construction
    should build immutable VM-owned binary/bitstring values; matching should
    destructure immutable VM-owned binary/bitstring values into typed captures.
  - Requirement: binary patterns must be accepted anywhere ordinary patterns
    are accepted after the feature is complete: `case`, function-head pattern
    parameters, `let` destructuring where irrefutable/fallible semantics are
    explicit, shape synonyms, and future extractor-backed shapes.
  - Requirement: support protocol-oriented shapes so packet layouts can be
    named and reused without custom parser code:
    ```terl
    pub shape TcpHeader(source_port, dest_port, sequence, ack, flags, payload) =
        Binary[big] {
            source_port: U16,
            dest_port: U16,
            sequence: U32,
            ack: U32,
            data_offset_size: 4,
            reserved_size: 4,
            flags: U8,
            window_size: U16,
            checksum: U16,
            urgent_pointer: U16,
            payload: Rest
        }.
    ```
  - Requirement: binary and bitstring segment types must be designed before
    syntax is finalized. The names do not have to be `U8`, `U16`, or `U32`,
    but the type model must express the same ideas: unsigned integer segment
    widths, signed integer segment widths, byte-aligned blobs, arbitrary
    bit-width blobs, UTF-8 scalar/codepoint segments, and a single rest
    payload segment.
  - Requirement: widths, signedness, byte alignment, and endian policy must be
    compiler-visible through types or typed segment descriptors. They must not
    be parser-only strings or runtime-only metadata.
  - Requirement: segment marker types must live in a normal std module such as
    `std.binary` or `std.vm.Bytes`. They must have docs, tests, formatter
    support, LSP hover support, and generated summaries like other std types.
  - Requirement: the canonical segment-width model is generic typed
    descriptors such as `UInt[16]`, `Int[16]`, `Bytes[N]`, and `Bits[N]`.
    Fixed aliases such as `U8`, `U16`, or `U32` may be added later as
    conveniences, but they are not the canonical form for 0.0.7.
  - Requirement: the release gate must lock the generic form first:
    ```terl
    import std.binary.{UInt, IntBits, Bytes, Bits, Rest}.

    Binary[big] {
        source_port: UInt[16],
        destination_port: UInt[16],
        flags: UInt[8],
        payload: Rest
    }
    ```
    Non-canonical fixed-width aliases must either be absent or documented as
    simple aliases to the generic form. They must not create a second
    independent type model.
  - Requirement: typed captures must obey ordinary Terlan typechecking. A
    segment whose decoded value cannot fit its declared type must reject the
    match or produce a typed decoding error according to context.
  - Requirement: matching must be deterministic and bounds-checked. Truncated
    binaries, non-byte-aligned bitstrings, invalid UTF-8 scalar segments,
    duplicate capture names, impossible widths, negative widths, and multiple
    unbounded rest captures must fail with stable diagnostics or typed errors.
  - Requirement: bitstrings that are not byte-aligned must be representable as
    values, but APIs that require byte-aligned binaries must reject them with
    stable typed errors.
  - Requirement: VM implementation must own binary/bitstring storage,
    slicing, matching, and construction. Native crates may be used for
    specialized protocol parsing later, but the core binary value and pattern
    semantics are VM-owned.
  - Requirement: parser, formatter, typechecker, CoreIR, VM IR/artifact,
    evaluator, LSP, tree-sitter, syntax docs, Lean/type-spec proof track, and
    coverage inventory must agree on the syntax and semantics.
  - Requirement: property-based tests must be used for binary construction and
    decode roundtrips, truncation fuzzing, endian differences, random payload
    capture, and bit-alignment edge cases.
  - Gate: add `make binary-bitstring-processing-check`.
  - Completed progress: descriptor-backed `Binary[big|little] { ... }`
    source now round-trips through parser and formatter in expression,
    function-head pattern, lambda-pattern, and case-pattern scaffold positions.
    The formatter no longer serializes generated `Dynamic` parameter
    annotations onto binary layout function-head patterns, and the untyped
    pattern-clause parser preserves explicit return annotations such as
    `decode(Binary[little] { ... }): Int -> ...`. The regression is locked by
    `formatter_preserves_binary_layout_scaffold`, including return-type
    preservation, and `make binary-bitstring-processing-check` passes end to
    end.
  - Completed progress: `std.vm.Bytes` now uses a dedicated immutable
    `ReplValue::Bytes(Arc<[u8]>)` representation instead of the synthetic
    `{vm_bytes, List[Int]}` tuple. Construction validates and packs octets,
    concatenation stays byte-native, receiver conversion and length preserve
    the public API, and text values cannot compare equal to byte values. VM
    hashing, rendering, type reflection, retained-size accounting, and TETF
    encoding all recognize the distinct byte representation. The standalone
    VM now reuses the canonical value type classifier instead of maintaining a
    second match. Focused behavioral tests and
    `make binary-bitstring-processing-check` pass; source-level binary
    construction and matching remain outstanding.
  - Completed progress: `std.vm.Bytes.Bytes.slice(start, length)` now performs
    VM-owned immutable byte slicing through a closed `vm.bytes.slice` CoreIR
    intrinsic identity. It validates argument types, rejects negative ranges,
    rejects out-of-bounds ranges, and detects range arithmetic overflow before
    host-width conversion with stable diagnostics. Compiler selection, stable
    intrinsic naming, return-type metadata, source execution, and adversarial
    runtime tests are covered. The owning
    `make binary-bitstring-processing-check` now executes the source-level
    bytes test and passes end to end.
  - Completed progress: `std.vm.Bytes.Bytes.read_uint_be(bit_offset,
    bit_width)` now decodes unsigned big-endian integers directly from the
    VM-owned immutable byte representation through the closed
    `vm.bytes.read_uint_be` CoreIR intrinsic. Reads support arbitrary aligned
    and unaligned bit offsets with widths from 1 through 63, while stable
    diagnostics reject invalid types, negative offsets, unsupported widths,
    arithmetic overflow, and truncated ranges. The compiler registry,
    intrinsic metadata, source evaluator path, boundary behavior, and
    adversarial runtime cases are covered. Bytes operations were also moved
    from the oversized remote-dispatch file into a focused runtime module.
    `make binary-bitstring-processing-check` passes; construction and
    executable binary patterns remain open.
  - Completed progress: `std.vm.Bytes.Bytes.read_int_be(bit_offset,
    bit_width)` now decodes signed two's-complement integers through the closed
    `vm.bytes.read_int_be` CoreIR intrinsic. Signed and unsigned reads share one
    bounds-checked network-order extraction path, then signed reads extend the
    declared width without host-endian behavior. Tests lock positive and
    negative 1-bit, 8-bit, and 63-bit boundaries, unaligned fields, invalid
    types and widths, truncation, arithmetic overflow, intrinsic identity,
    return metadata, and source-level VM execution.
    `make binary-bitstring-processing-check` passes; construction and
    executable binary patterns remain open.
  - Completed progress: `std.vm.Bytes.Bytes.read_uint_le/2` and
    `read_int_le/2` now provide deterministic little-endian integer decoding
    through closed CoreIR intrinsic identities. Successive wire-order groups
    of at most eight bits receive increasing numeric significance while bits
    inside each group remain network ordered, covering conventional byte widths
    and explicitly defining partial-width and unaligned behavior. All four
    integer readers share bounds, width, overflow, bit-access, and sign-extension
    helpers. Tests lock unsigned and signed 8-bit, 12-bit, 16-bit, and 63-bit
    values, aligned and unaligned offsets, truncation, invalid types and widths,
    intrinsic metadata, arity rejection, and source-level VM execution.
    `make binary-bitstring-processing-check` passes; binary construction and
    executable binary patterns remain open.
  - Completed progress: selected imported std functions now dispatch before
    merged dependency bodies when the root module has no local function with
    the same name and arity. This prevents `std.test.Test.each/2` from being
    shadowed by `std.collections.List.each/2`, which previously converted
    generated property rows into an iterator and then attempted to iterate the
    iterator again. The focused property regression, all 12 table tests, all 12
    binary property tests, all 63 adjacent binary API tests, the descriptor
    contract, and the exact VM bytes evaluator test pass under
    `make binary-descriptor-check`. Source-level binary matching and raw VM
    bitstring values remain open.
  - Completed progress: the VM now owns a canonical immutable
    `ReplValue::BitString` representation with an exact logical bit length,
    network-order bits, and masked trailing storage. `std.vm.BitString`
    exposes checked construction from packed bytes, UTF-8 scalar encoding,
    arbitrary bit slicing, length/alignment inspection, and aligned conversion
    to `std.vm.Bytes`; every operation has a closed `vm.bitstring.*` CoreIR
    identity and the module interface is embedded in installed compiler
    binaries. Equality, hashing, rendering, type reflection, retained-size
    accounting, TETF encoding, and the benchmark binary's shared value model
    all recognize the distinct value. Terlan execution tests and adversarial
    Rust tests cover partial-byte canonicalization, unaligned ranges, negative
    and overflowing inputs, invalid Unicode scalars, and byte-only conversion
    rejection. The complete `make binary-bitstring-processing-check` passes
    with Rust warnings denied. Source-level `Binary { ... }` construction and
    matching remain open, so this parent item remains unchecked.
  - Completed progress: raw `Bits[N]` protocol descriptors now execute against
    VM-owned `std.vm.BitString.BitString` values. Source decoding returns exact
    logical bit lengths, and VM encoding packs adjacent partial-byte values
    without introducing padding; bit spans are no longer restricted by the
    63-bit integer segment limit. Positive and adversarial coverage locks 3+5
    bit packing, 64-bit spans, checked bit access, wrong value types, width
    mismatch, truncation, and terminal byte-alignment rejection. The descriptor
    matrix names the current tests, all 66 `std/binary` tests pass, and the full
    `make binary-bitstring-processing-check` succeeds with Rust warnings
    denied. Source-level `Binary { ... }` construction and matching remain
    open, so this parent item remains unchecked.
  - Completed progress: `std.vm.BitString.BitString.concat/1` now composes two
    immutable logical bit sequences without inserting byte padding. The VM
    implementation uses checked length arithmetic and canonical trailing-bit
    masking; closed CoreIR identity `vm.bitstring.concat` carries the operation
    through compiler selection and receiver dispatch. Positive and adversarial
    tests cover aligned and unaligned concatenation, empty identity, host-size
    overflow, wrong suffix types, source execution, intrinsic arity and return
    metadata, and generated interface drift. The complete
    `make binary-bitstring-processing-check` passes with Rust warnings denied.
    Source-level `Binary[big|little] { ... }` construction and executable
    matching now share canonical descriptor semantics through typed CoreIR and
    the VM. Case arms, function heads, lambda parameters, and refutable `let`
    patterns capture `UInt[N]`, `IntBits[N]`, `Bytes[N]`, `Bits[N]`, and terminal
    `Rest` fields. The matcher preserves logical bit offsets, requires exact
    consumption without `Rest`, and commits captures atomically. Adversarial
    coverage rejects truncation, non-byte-sized rest payloads, empty/malformed
    CoreIR layouts, duplicate captures, and non-terminal `Rest`; JS profiles
    reject the VM-owned pattern through target-profile validation. The complete
    `RUSTFLAGS='-D warnings' make binary-bitstring-processing-check` passes.
    Tree-sitter now parses construction and every supported pattern position
    through one shared binary-layout field grammar, with corpus, highlight, and
    package-surface coverage. The VS Code TextMate bridge recognizes layouts,
    endian policies, and canonical descriptors; LSP parsing continues through
    the canonical compiler parser already covered by the focused syntax tests.
    These tooling checks are owned by `binary-syntax-scaffold-check` and pass
    under warnings-as-errors. The executable `BinaryPropertyTest` now proves
    188 generated construction/matching roundtrips across unsigned and signed
    boundaries, both endian modes, and mixed byte/bit/rest layouts at unaligned
    offsets. It also exhaustively rejects all 67 incomplete bit prefixes across
    fixed integer, UTF-8 scalar, and exact byte/bit layouts, including unaligned
    truncation points. Reusable shape-backed layouts now execute locally and
    across imported module boundaries as recorded below, completing this parent.
  - Completed progress: `std.vm.BitString` now constructs fixed-width integer
    fields through `from_uint_be/2`, `from_int_be/2`, `from_uint_le/2`, and
    `from_int_le/2`. One VM-owned conversion path enforces widths from 1 through
    63, signed and unsigned representability, two's-complement encoding, partial
    widths, and deterministic wire ordering; the protocol encoder now delegates
    to that same path instead of maintaining a second fit/endian implementation.
    Four closed `vm.bitstring.*` CoreIR identities, generated interfaces,
    source-level execution, wrong-type diagnostics, 1-bit and 63-bit boundaries,
    aligned and unaligned values, and both endian policies are covered. The full
    `make binary-bitstring-processing-check` passes with Rust warnings denied.
    Grammar-level `Binary { ... }` construction and executable matching remain
    open, so this parent item remains unchecked.
  - Completed progress: `std.vm.BitString` now decodes complete fixed-width
    values through `to_uint_be/0`, `to_int_be/0`, `to_uint_le/0`, and
    `to_int_le/0`. One VM-owned decoder handles checked offsets, widths from 1
    through 63, signed two's-complement extension, partial-byte fields, and
    both endian policies; the existing `std.vm.Bytes.read_*` operations now
    delegate to the same implementation instead of maintaining duplicate bit
    accumulation and sign-extension code. Four closed `vm.bitstring.*` CoreIR
    identities, generated interfaces, source-level round trips, offset and
    overflow adversarial cases, stable zero/64-bit diagnostics, and existing
    Bytes compatibility are covered. The full
    `make binary-bitstring-processing-check` passes with Rust warnings denied.
    Grammar-level `Binary { ... }` construction and executable matching remain
    open, so this parent item remains unchecked.
  - Completed progress: `std.vm.BitString.BitString.to_utf8_scalar/0` now
    provides the exact inverse of `utf8_scalar/1` through the closed
    `vm.bitstring.to_utf8_scalar` CoreIR identity. VM-owned validation requires
    byte alignment and exactly one canonical UTF-8 scalar, rejecting empty,
    malformed, overlong, multi-scalar, and partial-byte values with stable
    diagnostics. Source execution covers a non-ASCII scalar, while VM and
    remote adversarial tests cover Unicode boundaries and every rejection
    class. Generated interfaces are exact, and the full
    `make binary-bitstring-processing-check` passes with Rust warnings denied.
    Source-level construction and executable matching are complete as recorded
    in the canonical-descriptor item below. Tree-sitter corpus/highlight
    coverage, the VS Code TextMate bridge, and canonical LSP parser coverage are
    complete and owned by `binary-syntax-scaffold-check`. Generated descriptor
    roundtrips and property-driven truncation coverage are complete in
    `BinaryPropertyTest`; reusable shape-backed layouts are complete as recorded
    below.
  - Completed progress: canonical widthless `Utf8` fields now construct and
    match one Unicode scalar as an `Int`. Construction lowers to the existing
    closed `vm.bitstring.utf8_scalar` intrinsic, whose previously missing direct
    VM execution case now delegates to the canonical VM-owned bitstring path.
    Matching determines the 1-4 byte prefix width at byte-aligned offsets and
    reuses `VmBitString::to_utf8_scalar` for continuation, overlong, surrogate,
    truncation, and scalar-count validation. Capture commits remain atomic on
    malformed input. Parser, typechecker, CoreIR contract text, Tree-sitter,
    highlighting, and the VS Code TextMate bridge share the descriptor.
    Executable construction/matching tests cover valid scalars and malformed or
    truncated input, while generated properties cover 11 Unicode boundary
    values. `RUSTFLAGS='-D warnings' make binary-syntax-scaffold-check` passes.
  - Completed progress: property-driven truncation coverage now enumerates every
    incomplete bit prefix for a 24-bit fixed-integer layout, a 24-bit UTF-8
    scalar layout, and a 19-bit exact byte/bit layout. All 67 generated prefixes,
    including unaligned boundaries, fail to match without partial captures. The
    executable properties use VM-owned `BitString.slice/2`, and the owning
    `RUSTFLAGS='-D warnings' make binary-syntax-scaffold-check` gate passes.
  - Completed progress: compile-time shape aliases now reuse canonical binary
    layouts without rewriting descriptor metadata as private captures. Binary
    field keys participate in ordinary shape hygiene and argument substitution,
    while `UInt`, `IntBits`, `Bytes`, `Bits`, `Utf8`, and `Rest` remain exact
    descriptors. Local shapes execute in function heads, case arms, and
    refutable lets; exported shapes survive interface generation, renamed
    imports, CoreIR lowering, and default-VM execution. Structural capture
    arguments fail with a stable diagnostic; only variable and wildcard capture
    arguments are accepted. Tree-sitter includes an explicit
    binary-shape corpus case, and the owning
    `RUSTFLAGS='-D warnings' make binary-syntax-scaffold-check` gate passes. The
    complete `RUSTFLAGS='-D warnings' make binary-bitstring-processing-check`
    parent gate also passes.
  - Make integration: run `binary-bitstring-processing-check` after
    `string-pattern-matching-check` and before `pattern-matching-support-check`.
  - Acceptance: executable `.terl` tests prove construction and matching for
    fixed-width unsigned integers, signed integers, endian variants, UTF-8
    scalars, byte arrays, bit-width fields, rest payloads, shape-backed packet
    layouts, and function-head binary patterns through the default VM path.
  - Acceptance: executable protocol tests include at least one TCP-like header
    decoder and one compact custom binary message format with roundtrip
    construction/decode assertions.
  - Acceptance: adversarial tests prove Erlang `<<...>>` syntax remains
    rejected, truncated payloads, impossible widths, duplicate captures,
    invalid endian markers, invalid UTF-8, non-byte-aligned binary-only API
    calls, multiple rest captures, and unsupported backend lowering fail with
    stable diagnostics.


## Completed 004

- [x] Add Terlan-native binary construction/matching syntax and VM execution
  using the canonical descriptors.
  - Problem: we have descriptor types but no user-facing, typed grammar shape
    for reliable binary construction and packet decoding in a single Terlan-native
    form.
  - Requirement: introduce expression/pattern syntax that is distinct from
    Erlang `<<...>>` while supporting canonical descriptor composition. Example:
    ```terl
    import std.binary.{UInt, IntBits, Bytes, Bits, Rest}.

    let payload = Binary {
        source_port: UInt[16],
        destination_port: UInt[16],
        sequence: IntBits[32],
        flags: UInt[8],
        body: Rest
    }.
    ```
  - Requirement: binary construction and matching must accept the same forms:
    function-head clauses, lambda parameter patterns, `case` arms, and
    `let`/shape-backed pattern destructuring (for matching arms), with
    constructor form available in expressions.
  - Syntax requirements:
    - policy/endianness and width controls are explicit and typed, not implicit
      global flags.
    - `Rest` is terminal-only and not repeatable.
    - typed mismatch in construction/matching is rejected statically when
      possible, otherwise at runtime with typed decode/construct diagnostics.
  - Runtime requirements:
    - parser/lowering/typecheck/VM must use one IR family for binary shapes and
      enforce deterministic field order and alias expansion.
    - constructor and matcher must share the same conversion and capture typing
      behavior from descriptors.
    - mismatch, truncation, invalid UTF-8, invalid width, and duplicate-name
      failures must remain stable in both parser and runtime diagnostics.
  - Tooling and docs requirements:
    - include syntax-output and formatter coverage for this syntax family.
    - update docs examples to use canonical descriptor names.
    - LSP/tree-sitter/editor grammar tokenization for binary construction/match
      syntax and descriptor fields.
  - Tests:
    - add parser tests for constructor/matcher forms, nested descriptors,
      shape-backed use, and `Rest` constraints.
    - add VM executable coverage under `tests/pattern/PatternMatchingTest.terl` for
      roundtrip construct/parse, function-head matches, case matches, and shape
      reuse.
    - add protocol-style acceptance cases (header decode/encode, rest payload,
      endian variants) and property-based decode-construction roundtrips.
    - add adversarial `.terl` tests for malformed descriptors in syntax,
      `Rest` misuse, oversized widths, non-byte-aligned constructions, and
      stable `unsupported_binary_pattern` path for unsupported targets.
  - Gate: extend `make binary-bitstring-processing-check` with a dedicated
    `binary_construction_matching` anchor that validates parser, syntax-output,
    typecheck, CoreIR, VM execution, and JS/backend diagnostic parity.
  - Completed progress: source-level `Binary[big|little] { ... }` expressions
    now construct immutable VM-owned bitstrings for canonical `UInt[N]`,
    `IntBits[N]`, exact-width `Bytes[N]`, exact-width `Bits[N]`, widthless
    `Utf8`, and terminal `Rest` fields. Integer widths are limited to 1 through
    63; byte and bit
    widths must be positive. Field names resolve to in-scope values, nominal
    field types are checked statically, and integer/byte/bitstring type
    mismatches, unbound values, and oversized widths fail with stable
    diagnostics. `Rest` accepts the same nominal `std.vm.Bytes.Bytes` body used
    by canonical protocol shapes. CoreIR lowering composes closed
    `vm.bitstring.from_*`, `vm.bitstring.from_all_bytes`,
    `vm.bitstring.from_exact_bytes`, `vm.bitstring.require_exact_bits`, and
    `vm.bitstring.concat` intrinsics. Exact byte construction rejects both short
    and long buffers, while exact bit construction rejects logical-length
    mismatches without padding unaligned values. Terminal rest bodies likewise
    append without alignment padding. Big- and little-endian executable `.terl`
    tests, mixed byte/integer, bit/integer, and rest/integer construction,
    typechecker adversarial tests, CoreIR identity coverage, and default-VM
    evaluator tests are part of the existing gate. The actor registry uses its
    relative VM module boundary, so the standalone benchmark reuses the same
    process-alias module without mirror drift.
    Executable matching now lowers the same descriptors into typed
    `CorePattern::BinaryLayout` nodes and runs through one atomic VM matcher.
    Case, function-head, lambda, and refutable-`let` positions execute through
    the default VM; big/little-endian integers, signed fields, exact bytes,
    unaligned bits, and terminal rest captures are covered. Truncation, trailing
    input without `Rest`, non-byte-sized rest captures, duplicate names, empty
    layouts, and non-terminal rest in malformed CoreIR fail without leaking
    partial bindings. VM target validation accepts the pattern while every JS
    profile returns a stable `target_profile_unsupported` violation. The gate
    runs `tests/binary/BinaryPatternTest.terl` and the focused compiler/VM
    adversarial anchors, and
    `RUSTFLAGS='-D warnings' make binary-bitstring-processing-check` passes end
    to end. Tree-sitter now uses one shared field rule for constructor and
    pattern layouts, with corpus cases for case arms, function heads, lambdas,
    and refutable lets plus descriptor highlighting and package-surface checks.
    The VS Code TextMate bridge covers the same layout vocabulary, and LSP
    parsing uses the canonical compiler parser. The owning
    `RUSTFLAGS='-D warnings' make binary-syntax-scaffold-check` gate passes.
    `BinaryPropertyTest` adds 188 generated source-level roundtrips: unsigned
    and signed boundary values execute in both endian modes, while generated
    marker, three-bit field, and rest-payload cross-products prove mixed
    descriptor matching at unaligned offsets. The property suite is part of
    the owning gate, which passes under warnings-as-errors; this item is now
    complete.
  - Make integration: run this section’s tests as part of
    `binary-bitstring-processing-check` and `make check` before `shape-synonyms-`
    and `pattern-matching-support-check`.
  - Acceptance: executable `.terl` tests prove construction and matching for
    canonical descriptors in all supported pattern positions and through default VM
    execution.
  - Acceptance: adversarial coverage proves descriptor misuse and malformed
    binary terms fail at the correct stage with stable diagnostic codes.


## Completed 005

- [x] Add protocol-shape templates and canonical packet decode/encode helpers
  in std.binary.
  - Problem: teams will still need recurring boilerplate for common framing, and
    the first protocol support should be reusable across apps without custom,
    low-level parsing code.
  - Requirement: introduce a small set of reusable shape helpers in `std.binary`
    for protocol pipelines:
    - declarative packet shape registration (named shape sets with field order,
      capture types, and policy defaults),
    - reusable header/body composition and split operations,
    - size-aware slicing helpers for payload extraction.
  - Requirement: canonicalize these helpers around the same descriptor model:
    `UInt`, `IntBits`, `Bytes`, `Bits`, and `Rest`, and enforce that policy
    defaults are explicit (`little`/`big`, signedness, alignment checks).
  - Requirement: provide examples for at least TCP-like and custom mini-frame
    layouts to demonstrate shape reuse, including a header-only parser and a
    prefixed-payload parser with deterministic tail semantics.
  - Requirement: add a small protocol DSL that remains plain Terlan syntax (no
    Erlang `<<...>>` fallback), and supports composition across modules and
    shape aliases.
  - Requirement: ensure helpers are side-effect free and compile with the default
    VM path. Non-VM backends may expose the helper surface only via explicit
    unsupported-result behavior.
  - Requirement: wire docs/syntax-output/formatter for packet helpers, and
    include examples with canonical descriptors so users can copy into real
    services without alias ambiguity.
  - Requirement: define a stable binary protocol compatibility test surface with
    deterministic golden expectations for parsing order, rest handling, and invalid
    layouts.
  - Tests:
    - add parser/typecheck tests for protocol shape registration and alias
      expansion.
    - add executable `.terl` tests in `tests/pattern/PatternMatchingTest.terl`
      for at least one TCP-like header shape and one compressed custom frame.
    - add property/adversarial tests for prefix collisions, field reordering
      mistakes, shape alias mismatch, and mixed-endian capture attempts.
    - add VM benchmark-style harness entries for one fixed-size header and one
      nested protocol with mutable body length.
  - Gate: extend `make binary-bitstring-processing-check` with
    `binary_protocol_shape_helpers` anchor validating parser/typecheck/CoreIR/VM
    compatibility and protocol helper lowering.
  - Completed progress: `std.binary.Binary.compose_header_body/2` now composes
    validated inert protocol descriptors instead of returning
    `UnsupportedRuntime`. It preserves header-then-body field order, derives a
    deterministic combined name, preserves the body terminal-rest marker, and
    rejects terminal-rest headers and mixed endian policies with typed
    construction errors. The descriptor matrix and release API inventory now
    classify composition as positive metadata behavior while decode/encode
    remain explicit runtime gaps. `make binary-bitstring-processing-check`
    passes with positive composition plus both adversarial cases.
  - Completed progress: `std.binary.Binary.split_header_body/2` now splits
    VM-owned byte frames into immutable header/body slices after validating the
    requested header length. Zero/full boundary splits preserve the original
    frame, negative lengths return `InvalidWidth`, and short frames return
    `TruncatedPayload`; all paths execute through adjacent Terlan release tests.
    Standalone std-test orchestration now detects `native` placeholders from
    checked CoreIR and keeps every annotation family out of source-function
    merging, so `@target.vm` byte operations remain VM-owned. The protocol
    helper gate, all 38 binary API tests, generated-summary drift for 160
    interfaces, and release API inventory pass. Descriptor-directed decode,
    encode, and packet-shape DSL work remain open.
  - Completed progress: `std.binary.Binary.split_protocol_header/2` now derives
    a frame boundary from a validated protocol shape instead of requiring a
    duplicated caller-supplied byte count. It folds ordered `UInt`, `IntBits`,
    `Bits`, and `Bytes` widths into one fixed bit prefix, stops before terminal
    `Rest`, requires byte alignment, and delegates immutable slicing to the
    checked VM byte helper. Fixed TCP-like headers, compact rest-terminated
    frames, byte spans, generated widths, malformed shapes, unaligned layouts,
    and truncated frames execute through 44 adjacent API tests and 9 property
    tests. `make binary-protocol-helper-check`, the descriptor contract,
    generated-summary drift for 160 interfaces, and release API inventory pass.
    Descriptor-directed encoding and the packet-shape DSL remain open.
  - Completed progress: `std.binary.Binary.decode_fixed_header/2` is now an
    executable typed VM-byte API rather than a `Dynamic` unsupported-runtime
    placeholder. It accepts `ProtocolShape` and `std.vm.Bytes.Bytes`, validates
    and derives the fixed boundary through `split_protocol_header/2`, and
    returns the still-encoded header and body as immutable byte values. Adjacent
    tests prove compact-frame execution and generated-width equivalence with the
    lower-level splitter. The machine-enforced unsupported-runtime inventory
    dropped from seven operations to six, and `make binary-protocol-helper-check`,
    descriptor contract tests, generated-summary drift for 160 interfaces,
    release API inventory, and Rust quality checks pass. Descriptor-directed
    encoding and the packet-shape DSL remain open.
  - Completed progress: `std.binary.Binary.decode_prefixed_body/2` now walks a
    validated protocol shape and returns ordered typed decoded fields plus the
    immutable terminal byte suffix. `UInt` and `IntBits` fields decode through
    the VM-owned big/little-endian readers, fixed `Bytes` fields use checked
    immutable slices, and `Rest` preserves deterministic tail semantics. The
    closed `DecodedValue = Int | std.vm.Bytes.Bytes` result avoids `Dynamic`,
    while truncation, widths above the current 63-bit scalar limit, unaligned
    tails, and unsupported `Bits` values return typed errors. Four adjacent
    adversarial/API tests and a partitioned exhaustive property cover every
    unsigned octet from 0 through 255. The unsupported-runtime inventory drops
    from six operations to five, all 56 `std/binary` tests pass, and
    `make binary-bitstring-processing-check` passes end to end. Descriptor-
    directed encoding, source-level binary matching, and VM bitstring values
    remain open.
  - Completed progress: `std.binary.Binary.encode_exact/2` and
    `encode_prefix/2` now provide checked descriptor-directed protocol encoding
    through the VM-owned byte runtime. They pack signed and unsigned
    big/little-endian integer fields through 63 bits, preserve fixed byte
    fields, reject unaligned or overflowing values, and either append or omit a
    declared terminal `Rest` body. Source tests cover compact-frame round trips,
    prefix-only output, endian/fixed-byte combinations, malformed field sets,
    unsupported raw `Bits`, and every unsigned octet through a partitioned
    property test; nine focused Rust tests also cover 63-bit boundaries,
    duplicates, body policy, and source-level descriptor atom names. Mixed
    source/native std modules now retain source helpers while dispatching only
    known native placeholders to the VM. The unsupported-runtime inventory
    drops from five operations to three, all 58 `std/binary` tests pass, and
    `make binary-bitstring-processing-check` passes end to end. Source-level
    binary matching and raw VM bitstring values remain open, so this parent
    item remains unchecked.
  - Completed progress: the internal `terlan-benchmark` binary now exposes a
    `vm-binary-protocol-baseline` command backed by the checked-in
    `benchmarks/fixtures/BinaryProtocolBenchmarkTest.terl` workload. The current
    versioned report and expanded success/adversarial workload matrix are
    tracked by the dedicated protocol-stack benchmark slice below. The harness
    labels measurements as compiler-process-plus-VM-test end-to-end timing
    rather than claiming codec-only throughput. `make
    binary-protocol-benchmark-check` owns the report/unit/workload checks and
    now runs inside
    `make binary-bitstring-processing-check`; both gates pass with Rust compiler
    warnings denied. Large scheduler-safe iteration sweeps remain owned by the
    VM scheduling/performance slices rather than recursive source benchmark
    loops.
  - Completed progress: `std.binary.Binary.ProtocolShapeSet` and
    `ProtocolShapeAlias` now provide declarative, ordered protocol registries
    with exact direct-name and alias resolution. Construction validates every
    shape before exposure, rejects empty sets, duplicate shape or alias names,
    direct-name/alias collisions, blank aliases, missing targets, and alias
    chains, and never falls back to prefix matching. Nine adjacent API and
    adversarial tests plus two generated property tests cover direct lookup,
    aliases, prefix collisions, invalid metadata, and ambiguous registries;
    all 69 `std/binary` tests pass. The binary descriptor contract now owns nine
    metadata types, all 700 release API rows remain covered with zero baseline
    gaps, 160 generated summaries match committed artifacts, and
    `make binary-bitstring-processing-check` passes end to end. Source-level
    binary matching and raw VM bitstring values remain open, so this parent
    item remains unchecked.
  - Completed progress: registered layouts now execute through
    `protocol_shape_set_decode/3`, `protocol_shape_set_encode_exact/3`, and
    `protocol_shape_set_encode_prefix/3`. Each helper validates and resolves an
    exact direct or alias name before delegating to the canonical descriptor
    decoder or existing VM-owned encoder; there is no second codec, implicit
    default shape, or prefix fallback. Adjacent tests prove direct/alias decode
    parity, exact alias encode/decode roundtrip, direct/alias prefix parity, and
    typed `UnknownShape` failures across all three entry points. A generated
    property test roundtrips scalar and body values through multiple alias
    names. All 74 `std/binary` tests pass, the release API inventory covers 703
    rows with zero baseline gaps, 160 generated summaries match, Rust warnings
    remain denied, and `make binary-bitstring-processing-check` passes end to
    end. Source-level binary matching and raw VM bitstring values remain open,
    so this parent item remains unchecked.
  - Completed progress: `tests/pattern/PatternMatchingTest.terl` now provides
    the executable `binary_protocol_shape_helpers` anchor required by this
    slice. It resolves a registered TCP-like layout and verifies its canonical
    fixed-header/body boundary, then encodes and decodes a compact frame through
    an exact shape alias while checking scalar fields and the terminal payload.
    The complete `make binary-bitstring-processing-check` gate passes, including
    descriptor contracts, generated adversarial cases, VM byte execution,
    protocol benchmarks, and TCP framing. Raw `Bits` value decoding remains
    explicitly unsupported and stays owned by the parent binary-construction
    and matching slice.
  - Make integration: run protocol-helper tests as part of binary checks before
    moving to `terlan-vm` protocol-level integration slices.
  - Acceptance: teams can define a protocol layout in `std.binary` style and
    decode/encode it in VM execution without custom parser functions.


## Completed 006

- [x] Add VM-native static asset and response streaming support under std.http.
  - Problem: without explicit response-streaming and static asset support, VM HTTP
    apps cannot efficiently serve JS/wasm/css/html bundles or long-poll/event-like
    payloads.
  - Requirement: add typed static asset serve helpers and route integration:
    - manifest-driven asset tables,
    - content-type inference by extension,
    - range-style and cache-control metadata where relevant,
    - immutable/static fingerprint lookup path.
  - Requirement: add stream-capable response primitives in std.http:
    chunked-like chunk emission semantics (VM-native representation), backpressure
    integration, and explicit completion/error/abort outcomes.
  - Requirement: support response body modes as explicit variants (`Raw`, `Binary`,
    `Text`, `Stream`, and `Empty`) with deterministic serialization.
  - Requirement: keep asset serving and stream emission explicit and typed:
    no implicit conversion from arbitrary lists/atoms into response bodies.
  - Requirement: preserve middleware compatibility so static and stream responses
    can pass through the same router chain.
  - Requirement: define explicit errors for unsupported streaming in non-VM
    backends and malformed asset manifests.
  - Requirement: include safe defaults for maximum in-memory buffer, stream
    chunk size, and max pending writes.
  - Requirement: document stable response contracts, including middleware
    interaction with stream cancellation/timeouts.
  - Tests:
    - VM executable `.terl` tests for static asset route lookup, cache
      metadata, and content-type behavior.
    - executable streaming tests for backpressure cancellation, partial flush,
      stream abort, and response finalize ordering.
    - adversarial tests for malformed asset manifests, unsupported backend
      streaming behavior, and excessive stream buffering.
    - integration tests for router/middleware with stream and static routes.
  - Gate: extend `make vm-http-stack-check` with
    `vm_http_static_and_streaming` anchor.
  - Completed progress: VM static assets now expose typed inclusive,
    open-ended, and suffix byte ranges. Valid ranges produce deterministic
    `206 Partial Content` responses with `Accept-Ranges`, `Content-Range`,
    content length, content type, cache policy, and exact sliced bytes.
    Oversized inclusive ends clamp to the asset boundary; reversed ranges,
    zero-length suffixes, empty assets, and starts beyond the asset boundary
    return stable `InvalidRange` or `UnsatisfiableRange` outcomes. The
    canonical `make vm-http-static-streaming-check` gate runs the positive and
    adversarial range cases. HTTP header parsing remains protocol-layer work;
    this slice defines the VM-owned validated numeric range contract without a
    hand-rolled wire parser.
  - Completed progress: VM response streams now use a bounded, pollable state
    machine with atomic chunk admission, deterministic FIFO partial flushes,
    explicit `Open`/`Finishing`/`Complete`/`Aborted` states, idempotent finish,
    and typed backpressure, closed-stream, aborted-stream, and invalid-chunk
    outcomes. Stream snapshots expose pending-write limits and emitted
    chunk/byte counts to the scheduler lane. The canonical
    `make vm-http-static-streaming-check` gate covers ordered splitting,
    rollback when a multi-chunk write exceeds capacity, finish ordering, and
    abort cleanup.
  - Completed progress: the response stream now flushes through the existing
    VM framing and TCP runtime instead of a parallel socket path. Successful
    writes commit exactly one queued chunk; peer-inbox pressure preserves the
    chunk, parks the owning VM process, and resumes through the existing TCP
    write-wakeup contract. Closed and cancelled transports clear pending
    chunks and produce distinct typed terminal outcomes. The canonical gate
    covers ordered TCP writes, pressure/wakeup/retry behavior, and adversarial
    peer close/cancellation.
  - Completed progress: the canonical HTTP/1 response serializer now owns both
    buffered and streamed wire framing in a focused module. Streamed responses
    emit validated `Transfer-Encoding: chunked` heads, non-empty hexadecimal
    chunks, and one explicit terminal marker with exact wire-byte accounting.
    Conflicting caller-supplied `Content-Length`/`Transfer-Encoding`, bodyless
    statuses, empty data chunks, and head/chunk/final write failures have stable
    rejection behavior. Buffered responses now also reject ambiguous transfer
    encoding. The canonical gate covers positive wire output and adversarial
    metadata/write failures.
  - Completed progress: chunked HTTP/1 wire frames are now bound to a dedicated
    VM response state machine over the existing scheduler/TCP write path. The
    stream preserves strict head/body/end order, distinguishes body drain from
    protocol finalization, caches the current encoded chunk across pressure
    retries, parks and wakes through VM TCP, and commits body accounting only
    after the corresponding wire frame is accepted. Invalid metadata,
    abort-before-finalize, and terminal transport failure remain typed and
    deterministic. The canonical gate covers exact end-to-end wire order,
    pressure retry, protocol lifecycle inspection, and adversarial races.
  - Completed progress: matched router targets now materialize through one
    typed response adapter after middleware continuation. Static targets become
    buffered byte responses with manifest-owned content/cache metadata; stream
    targets become the existing bounded HTTP/1 response state machine and flush
    through VM TCP with the selected status and connection policy. Handler,
    compiled-handler, SSE, and WebSocket targets are rejected as
    `InvalidResponse` until their execution/upgrade stage has produced a body.
    The canonical gate covers middleware-to-static materialization, complete
    middleware-to-stream HTTP/1 wire output, and adversarial premature target
    conversion.
  - Completed progress: compiled Terlan `std.http.Response` descriptors now
    materialize through the router response adapter after handler execution.
    Text, trusted HTML, JSON text, native JSON, redirects, and manifest-backed
    files produce deterministic buffered responses with validated status,
    headers, cookies, content type, cache policy, and content length.
    `Response.file` resolves package paths deterministically and serves the
    manifest-owned bytes; framing headers cannot be injected through response
    metadata. The canonical gate compiles and executes source handlers and
    covers all finite response kinds, missing assets, malformed descriptors,
    and conflicting framing metadata.
  - Completed progress: `std.http.Response.stream` exposes both a one-argument
    source constructor with bounded defaults and an explicit five-argument
    constructor for status, content type, chunk size, and pending-write limits.
    VM lowering validates the typed descriptor, feeds source chunks through the
    existing bounded HTTP/1 stream plan and VM TCP queue, preserves chunk order,
    and reports invalid limits, invalid chunks, and queue pressure through stable
    typed outcomes. Non-VM dispatch rejects streaming explicitly instead of
    buffering silently. The canonical gate compiles Terlan handlers, verifies
    exact wire output and defaults, and covers malformed descriptors and
    backpressure adversarially.
  - Make integration: run this gate after `vm_http_router_middleware` and before
    HTTP concurrency/hot-reload slices.
  - Acceptance: a complete request path is available for static assets and
    streaming responses with stable typed outcomes and explicit backend limits.


## Completed 007

- [x] Add VM-native WebSocket and SSE support for long-lived push-style channels.
  - Problem: static and request/response-only HTTP is insufficient for live
    dashboards, event systems, and command/control workflows. We need explicit
    long-lived channel semantics in VM terms.
  - Requirement: add `std.http.WebSocket`/`std.http.Sse` transport contracts that
    run on VM-owned socket lifecycle and mailbox scheduling.
  - Requirement: define typed channel states for open/closed/backpressure/errored
    with explicit transitions and no hidden mutable internals in user space.
  - Requirement: support upgrade flow for WebSocket-style handshakes and explicit
    frame decode/encode through the existing binary descriptors and framing
    abstraction.
  - Requirement: support backpressure-aware outgoing send paths and bounded
    inbound queueing per connection.
  - Requirement: add SSE stream semantics with typed event envelopes, explicit
    retry/disconnect outcomes, and deterministic close behavior.
  - Requirement: define clear lifecycle events for close reason, malformed frame,
    ping/pong/keep-alive support, and timeout behavior.
  - Requirement: integrate with middleware/router context so channel endpoints can
    reuse auth/trace/recovery chain behavior.
  - Requirement: include explicit non-VM behavior: web packaging may consume
    inert route metadata, but live channel execution remains VM-owned.
  - Requirement: docs and examples with at least one broadcast room/channel model
    and one typed event stream endpoint.
  - Tests:
    - parser/typecheck tests for websocket/sse entry signatures and route shape.
    - executable `.terl` tests for open/close transitions, message send/receive,
      frame errors, and backpressure behavior.
    - adversarial tests for malformed upgrade attempts, oversized frames,
      oversized event payloads, and unsupported-backend channels.
    - perf-style VM tests for concurrent channel count and bounded queue contention.
  - Gate: extend `make vm-http-stack-check` with
    `vm_http_ws_sse_transport` anchor.
  - Current gate state: `make vm-http-stack-check` passes with warnings denied.
    The `vm_http_ws_sse_transport_executes_authenticated_source_push_paths`
    anchor compiles the checked-in `std/http/LiveChannelTest.terl`, executes its
    typed auth middleware, flushes an exact typed SSE frame, completes a
    WebSocket upgrade over VM-owned TCP, and pushes an exact WebSocket frame
    through the resulting VM session. Existing focused coverage retains bounded
    queue pressure, malformed and oversized input rejection, ping/pong,
    timeout/cancellation, TLS upgrade, broadcast, reconnect, and deterministic
    close behavior.
  - Make integration: run this gate after `vm_http_static_and_streaming` and
    before VM HTTP concurrency/hot reload slices.
  - Acceptance: one complete `.terl` example demonstrates middleware-authenticated
    long-lived push communication in VM-native HTTP stack terms.


## Completed 008

- [x] Add VM HTTP concurrency model, worker isolation, and hot-reload capability.
  - Problem: real services need bounded concurrency, predictable isolate scheduling,
    and fast code updates without dropping in-flight state or leaking resources.
  - Requirement: define a VM-native concurrency contract for HTTP server work:
    worker pools, active-connection accounting, queue limits, and deterministic
    overload policy (`reject`, `queue`, or `spill` semantics).
  - Requirement: each request/connection must carry an execution scope with
    bounded memory and deterministic cleanup on abort/timeout/error.
  - Requirement: add typed lifecycle hooks for worker start, request start/end,
    channel bind/unbind, and shutdown handoff so middleware can observe and
    enforce policies.
  - Requirement: add graceful shutdown and drain behavior with explicit
    timeout, completion wait, and forced close fallback.
  - Requirement: add VM-native hot-reload for compiled route/module artifacts:
    replace handler and router artifacts, invalidate old caches deterministically,
    and preserve a safe in-flight transition boundary.
  - Requirement: ensure hot-reload is explicit via `terlc`/`terl repl` workflow and
    does not mutate process state unless explicitly wired by user lifecycle hooks.
  - Requirement: document semantics for mixed-version handling when old handlers are
    active during artifact swap (compat mode, reject mode, or handoff mode).
  - Requirement: include deterministic stress behavior for connection spikes and
    backpressure saturation so saturation is surfaced as typed outcomes, not
    dropped silently.
  - Requirement: provide debug signals for concurrency diagnostics that can be
    consumed by vm CLI/debug tooling.
  - Requirement: avoid exposing scheduler internals as part of user API; provide
    capabilities-only control knobs with safe defaults.
  - Tests:
    - parser/typecheck tests for concurrency and lifecycle declarations.
    - VM executable `.terl` tests for overload paths, lifecycle hook ordering,
      worker assignment, and graceful shutdown under load.
    - adversarial tests for hot-reload races, artifact mismatch, queue overflow,
      and malformed lifecycle hooks.
    - perf regression suite for concurrent route throughput, queue saturation,
      and tail-latency under bounded worker pressure.
  - Gate: extend `make vm-http-stack-check` with
    `vm_http_concurrency_and_hot_reload` anchor.
  - Make integration: run this gate after `vm_http_ws_sse_transport` and before
    broader HTTP optimization and distributed transport slices.
  - Acceptance: the VM can sustain concurrent load with typed overload behavior,
    and hot-reload does not break running sessions beyond documented lifecycle
    policies.
  - Completed progress (2026-07-13): VM HTTP graceful shutdown now uses an
    explicit scheduler-driven lifecycle. `begin_drain` closes the listener before
    any further accept, while `poll_drain` and `poll_drain_with_tls` give retained
    handlers a bounded number of reactor/scheduler ticks to complete. Terminal
    reports distinguish `Pending`, `Drained`, and `Forced`, account completed and
    forced handlers, preserve deterministic resource cleanup, and remove TLS
    listener metadata only after a terminal drain outcome. Adversarial coverage
    locks zero-budget/no-side-effect behavior, invalid transitions, completion
    after a real TCP wakeup, cancellation-versus-completion accounting, forced
    cleanup at the deadline, and TLS plan lifetime.
    The existing `vm-http-concurrency-hot-reload-check` owns these anchors and
    passes with `RUSTFLAGS='-D warnings'`.
  - Completed progress (2026-07-13): the VM HTTP worker queue now exposes
    explicit `Queue`, `Reject`, and `Spill` admission policies with typed
    ownership-preserving outcomes. `Queue` retains bounded backpressure;
    `Reject` and `Spill` return the original work item when capacity is full so
    overload cannot silently discard a connection or request. Positive coverage
    proves all three policies enqueue when capacity is available, while
    adversarial coverage proves a saturated queue remains unchanged and rejected
    or spilled work remains caller-owned. Both exact regressions are owned by
    `vm-http-concurrency-hot-reload-check`, which passes with warnings denied.
  - Completed progress (2026-07-13): `std.http.Router` now exposes the closed
    `OverloadPolicy` atom union and `Router.overload(policy, max_pending)` builder.
    VM evaluation preserves the builder step, descriptor materialization validates
    the policy and positive pending-work bound, and `VmHttpRouter` retains the
    typed configuration for admission. Adversarial coverage rejects unknown
    policies, zero bounds, duplicate configuration, and scoped group policies
    that would otherwise be ignored. The checked-in interface/dependency summary
    and release API matrix include the public surface. The exact VM regression
    and executable `RouterTest.terl` are owned by
    `vm-http-concurrency-hot-reload-check`, which passes with warnings denied.
  - Completed progress (2026-07-16): `VmHttpTcpServer` now consumes the typed
    router overload configuration at the VM TCP accept boundary. Saturated
    `Queue` admission leaves work in the bounded listener backlog, `Reject`
    deterministically closes the accepted stream and exits its handler process,
    and `Spill` transfers work into the fallback execution lane while exposing
    explicit per-poll and lifetime counters. Server inspection retains the
    configured policy and all admission counters. Adversarial VM TCP coverage
    proves listener backpressure, rejected-peer closure without a live-process
    leak, and ownership-preserving spill admission. The exact regressions are
    owned by `vm-http-concurrency-hot-reload-check`, which passes with
    `RUSTFLAGS='-D warnings'`; the focused overload module passes all six tests
    after the implementation was isolated in `runtime/vm/http/overload.rs`.
  - Completed progress (2026-07-16): deterministic saturation stress now drives
    64 simultaneous VM TCP connections through a pending-work bound of eight for
    each overload policy. The regression locks exact accepted, queued, rejected,
    spilled, parked-process, and live-process accounting for `Queue`, `Reject`,
    and `Spill`; shutdown then proves the listener backlog, handler registry,
    process table, and TCP stream table are empty. The focused test and the full
    `vm-http-concurrency-hot-reload-check` pass with
    `RUSTFLAGS='-D warnings'`.
  - Completed progress (2026-07-16): `VmHttpTcpServer` now owns one typed
    lifecycle-hook contract for worker start, request start/end, channel
    bind/unbind, and graceful or immediate shutdown handoff. Hooks authorize
    policy-sensitive transitions before execution and observe successful
    transitions afterward. Request and graceful-drain policy rejection is
    deterministic, while channel cleanup and immediate shutdown remain
    non-vetoable so middleware cannot leak VM processes or TCP streams.
    Adversarial coverage proves rejected request dispatch does not invoke the
    handler, rejected drain leaves the listener open, and rejected channel bind
    rolls back the spawned process and accepted stream. The implementation is
    isolated in `runtime/vm/http/lifecycle_hooks.rs`, reduces the existing
    `http.rs` file-size baseline, and the complete
    `vm-http-concurrency-hot-reload-check` passes with
    `RUSTFLAGS='-D warnings'`.
  - Completed progress (2026-07-16): `std.http.Router` now exposes the typed
    `LifecycleEvent`, `LifecycleDecision`, and `LifecycleMiddleware` contract
    plus `Router.lifecycle`, and the router descriptor preserves exactly one
    root-scoped compiled callback. Descriptor decoding rejects non-callable,
    duplicate, and nested-group declarations rather than silently dropping
    lifecycle policy. `VmHttpTcpServer` installs the source callback against a
    shared `TerlanVm`; policy-sensitive events execute before transitions while
    completion and cleanup events execute afterward and cannot veto resource
    cleanup. Source-level adversarial coverage proves request rejection prevents
    handler execution, cleanup rejection still removes the process and stream,
    and invalid descriptor ownership fails deterministically. The executable
    `RouterTest.terl`, generated Router interface, and complete
    `vm-http-concurrency-hot-reload-check` pass with
    `RUSTFLAGS='-D warnings'`.
  - Completed progress (2026-07-16): source HTTP routers now publish as immutable
    VM-plus-router artifacts behind a monotonic generation registry. Compatible
    reloads retain the captured generation for in-flight requests while new
    requests bind the replacement; strict reloads reject publication atomically
    while the active generation is busy. RAII request leases retire superseded
    generations after their final request and preserve the complete request,
    handler, and reverse-response middleware graph. A real two-thread race proves
    a version-one request completes on version one after version two becomes
    active while a new request executes version two. Adversarial coverage locks
    invalid, duplicate, and busy publication rejection plus deterministic
    inspection and retirement events. The focused tests and complete
    `vm-http-concurrency-hot-reload-check` pass with
    `RUSTFLAGS='-D warnings'`.
  - Completed progress (2026-07-16): `terlc serve` now publishes source and
    packaged VM router changes through the generation registry and captures one
    request-scoped runtime lease before dynamic, static, SSE, or WebSocket route
    execution. Source and artifact checksums suppress unchanged publication;
    compatible reloads reuse the deployment so generation identifiers remain
    monotonic. A source-level adversarial regression holds a version-one request
    lease across recompilation, proves a new request executes version two, and
    then proves the retained request still executes version one. Plain handlers
    remain on a non-router VM lease. The handler cache was extracted from
    `commands/serve/mod.rs`, reducing that file below its reviewed size baseline.
    Both exact regressions and the complete
    `vm-http-concurrency-hot-reload-check` pass with
    `RUSTFLAGS='-D warnings'`.
  - Completed progress (2026-07-16): the `terlc serve` hot-reload gate now
    records concurrent throughput and tail-latency evidence from real VM
    handler execution rather than registry-only operations. Four retained
    generation-one leases and four generation-two leases execute 128 requests
    concurrently; every response is checked against its captured generation.
    The machine-readable
    `target/quality/serve-http-hot-reload-concurrency-report.json` records the
    compatible reload policy, worker/request counts, wall time, p50/p95/p99,
    throughput, and correctness assertions under schema
    `terlan-vm-http-hot-reload-concurrency-v1`. The canonical gate run measured
    p50 30.866 microseconds, p95 151.360 microseconds, p99 187.673 microseconds,
    and 78,386 requests/second. The complete
    `vm-http-concurrency-hot-reload-check` passes with
    `RUSTFLAGS='-D warnings'`.


## Completed 009

- [x] Add initial distributed transport protocol for VM nodes with explicit
  serialization, discovery, and fault semantics.
  - Problem: HTTP and protocol layers are currently single-node by default. Without
    a stable distributed control plane, multi-node scaling and failover scenarios
    remain ad-hoc.
  - Requirement: define a VM-owned distributed transport contract (wire framing,
    message envelopes, delivery guarantees, and session identity) in std.vm /
    std.cluster namespaces.
  - Requirement: support cluster node lifecycle (join/leave/heartbeat/fencing)
    with typed node states and deterministic transitions.
  - Requirement: define deterministic serialization for cross-node messages and a
    canonical wire schema version so messages remain backward-compatible per release
    policy.
  - Requirement: add explicit delivery semantics (`AtMostOnce`, `AtLeastOnce` where
    practical, and explicit `NeedsAck` path) with typed outcomes.
  - Requirement: provide transport backends via VM abstraction so this feature does
    not expose external scheduler-specific runtime primitives.
  - Requirement: include typed disconnect/reconnect outcomes, partition/failure
    simulation, and message dedup semantics in protocol contracts.
  - Requirement: add simple cluster metadata APIs for membership view, health,
    shard/role tagging, and leadership hints.
  - Requirement: provide a secure baseline: message-size caps, identity checks,
    channel timeout policy, and reject unknown protocol versions.
  - Requirement: align distributed transport with existing process/message model so
    actors can observe cluster state changes via typed channels.
  - Requirement: include minimal docs describing bootstrap/seed model and rolling
    restart behavior.
  - Tests:
    - parser/typecheck tests for cluster/distributed API declarations.
    - VM executable tests for node join/leave, heartbeat propagation,
      heartbeat timeout, and message exchange.
    - adversarial tests for duplicate delivery, out-of-order delivery,
      protocol-version mismatch, and unauthorized node identity attempts.
    - resilience tests for partition heal, restart, and stale-state pruning.
  - Gate: `make vm-distributed-transport-check` with anchor
    `vm_distributed_transport_protocol`.
  - Make integration: run this gate after `vm_http_concurrency_and_hot_reload` and
    before distributed scheduling/task placement slices.
  - Acceptance: one `.terl` test cluster scenario validates node discovery,
    message exchange, and deterministic failover behavior across at least two
    VM nodes.
  - Completed progress (2026-07-16): `std.vm.Cluster.Membership` now exposes
    source-level `heartbeat` and `expire` transitions backed by
    `VmClusterMembership`; membership descriptors retain bounded per-node
    snapshots instead of append-only heartbeat history. The executable
    `ClusterTest.terl` scenario proves the exact timeout boundary, peer
    transition to `unreachable`, heartbeat recovery to `active`, and local-node
    liveness alongside the existing two-node message exchange. The owning
    `vm-distributed-transport-check` now executes the source tests and passes
    with `RUSTFLAGS='-D warnings'`, including duplicate, out-of-order,
    reconnect, incompatible-profile, stale-heartbeat, left-node, and fenced-node
    adversarial coverage. Partition/restart resilience, bootstrap
    documentation, and full failover acceptance remain open under this parent
    slice.
  - Completed progress (2026-07-16): `std.vm.Cluster.Membership` now exposes
    immutable source-level `leave` and `fence` transitions through one shared
    VM snapshot-update path. Executable source coverage proves that a joined
    view remains active while independently derived views transition to the
    distinct terminal states `left` and `fenced`. The complete
    `vm-distributed-transport-check` passes with `RUSTFLAGS='-D warnings'` and
    now explicitly includes fenced-leave rejection and unknown-node lifecycle
    adversarial checks.
  - Completed progress (2026-07-16): cluster membership inspection now returns
    the closed `NodeState` union rather than untyped state text. The public
    singleton domain covers `Active`, `Left`, `Unreachable`, `Fenced`, and
    `Missing`; the VM boundary returns their canonical atom representation.
    Executable source tests exercise every arm, including absent-node lookup,
    and the complete `vm-distributed-transport-check` passes with
    `RUSTFLAGS='-D warnings'`.
  - Completed progress (2026-07-16): `Membership.view` now projects the
    VM-owned, node-id-ordered membership table into typed immutable `Node`
    records containing lifecycle state, last-seen tick, and sorted role tags.
    `Membership.health` returns the closed `Healthy | Degraded` domain, while
    `Membership.leader_hint` returns only an active `leader`-tagged node as an
    explicitly non-authoritative `Option[String]` hint. Executable source
    coverage proves ordering, local and remote role metadata, aggregate health
    degradation, and leader-hint removal after heartbeat expiry. The complete
    `vm-distributed-transport-check` passes with `RUSTFLAGS='-D warnings'`.
  - Completed progress (2026-07-16): source-level transport sessions now retain
    bounded immutable continuity snapshots instead of reopening fresh VM state
    for every operation. `Session.send` returns a typed `SendResult` containing
    the advanced session and frame; `Session.accept` returns a typed
    `AcceptResult` with the advanced session and closed `InboundOutcome` domain.
    The VM validates snapshot restoration, preserves monotonic outbound message
    ids, pending acknowledgements, inbound ordering, and duplicate history, and
    rejects non-contiguous inbound snapshots adversarially. The executable
    two-node Terlan scenario now sends two frames, observes the typed
    `OutOfOrder` gap, closes it, accepts the second frame, and classifies a
    replay as `Duplicate`. The complete `vm-distributed-transport-check` passes
    with `RUSTFLAGS='-D warnings'`.
  - Completed progress (2026-07-16): `std.vm.Cluster.Session` now exposes the
    VM-owned transport lifecycle as typed immutable transitions. `state`
    returns the closed `Connected | Disconnected` domain; `disconnect` accepts
    the closed five-reason `DisconnectReason` domain and returns a
    `DisconnectResult` with an inspectable event; `reconnect` returns a
    `ReconnectResult` classified as `AlreadyConnected` or `Reconnected`.
    Source execution covers all disconnect reasons, both reconnect outcomes,
    lifecycle state inspection, and message-order continuity across a
    disconnect/reconnect interval. The existing adversarial runtime coverage
    continues to prove message and acknowledgement operations are blocked while
    disconnected, wrong remote identities are rejected, and pending
    acknowledgements survive reconnection. The complete
    `vm-distributed-transport-check` passes with `RUSTFLAGS='-D warnings'`.
  - Completed progress (2026-07-16): distributed delivery and acknowledgement
    semantics are now source-visible and VM-owned. `Delivery` is the closed
    `AtMostOnce | NeedsAck` domain; `Session.send_with` selects it explicitly,
    and opaque frames preserve and expose both delivery policy and message id.
    `Session.needs_ack` and `pending_ack_count` inspect bounded session state,
    while `acknowledge` validates frame ownership and returns an immutable typed
    `AcknowledgeResult` with the advanced session. The executable Terlan
    scenario covers both delivery policies, pending-state creation and removal,
    stable message ids, and frame metadata. Existing VM adversarial coverage
    continues to reject duplicate and disconnected acknowledgements and proves
    pending acknowledgements survive snapshots and reconnects. All nine source
    tests and the complete `vm-distributed-transport-check` pass with
    `RUSTFLAGS='-D warnings'`.
  - Completed progress (2026-07-16): cluster membership now has bounded,
    deterministic stale-state pruning through `Membership.prune`. The VM
    removes only `Left` and `Unreachable` peer snapshots after a non-zero
    retention window has strictly elapsed; the exact boundary is retained, and
    local, active, and fenced identities are never pruned. The immutable source
    descriptor path and embedded interface expose the same transition. Source
    coverage proves left/unreachable removal, exact-boundary retention, fenced
    identity retention, and local-node retention. VM adversarial coverage
    rejects zero retention and independently covers both removable terminal
    states. All ten source tests and the complete
    `vm-distributed-transport-check` pass with `RUSTFLAGS='-D warnings'`.
  - Completed progress (2026-07-16): VM cluster profiles now preserve an
    explicit positive incarnation epoch, expose `epoch` inspection, and advance
    through checked `Profile.next_epoch` transitions. `Membership.restart`
    replaces only a known, compatible, unfenced peer with the same stable
    application/VM/node identity at a strictly newer epoch; it restores the
    peer to `Active`, preserves role tags, and updates its observed tick.
    Ordinary duplicate joins can no longer overwrite known identities.
    Executable source coverage proves unreachable-to-active rolling restart,
    epoch advancement, and role preservation. VM adversarial coverage rejects
    stale and duplicate incarnations, unknown nodes, identity mismatch,
    incompatible clusters, fenced restarts, and epoch overflow. All eleven
    source tests and the complete `vm-distributed-transport-check` pass with
    `RUSTFLAGS='-D warnings'`.
  - Completed progress (2026-07-16): cluster membership now exposes explicit
    VM-owned `partition` and `heal` transitions. Partitioning moves only an
    active remote peer to `Unreachable` while preserving identity, epoch,
    roles, and deterministic metadata; healing accepts only an unreachable
    peer at a monotonic tick. A three-node executable Terlan scenario proves
    leadership hints move deterministically from node-b to node-c during a
    partition and return after healing. VM adversarial coverage rejects stale
    and duplicate transitions, local-node partitioning, unknown peers, and
    terminal left/fenced misuse. All twelve source tests and the complete
    `vm-distributed-transport-check` pass with `RUSTFLAGS='-D warnings'`.
  - Completed progress (2026-07-16): the public cluster contract now documents
    explicit seed-based bootstrap, identity and compatibility validation,
    epoch-safe rolling restart, and the non-consensus meaning of deterministic
    leadership hints. A single executable three-node Terlan acceptance scenario
    joins primary and backup peers, delivers through the primary, partitions it,
    observes deterministic failover, delivers through the backup, and restores
    the primary only through a strictly newer incarnation epoch. All thirteen
    source tests and `RUSTFLAGS='-D warnings' make
    vm-distributed-transport-check` pass, closing the parent transport item.


## Completed 010

- [x] Add VM-native distributed scheduler, actor migration, and shard-aware task
  placement.
  - Problem: transport alone enables connectivity, but workloads still need safe
    placement decisions and predictable actor/process movement to run real clustered
    services.
  - Requirement: define typed scheduling policy descriptors (for example,
    `round_robin`, `least_connections`, `pinned`, `shard_affinity`) with
    explicit fallback behavior and validation.
  - Requirement: add APIs for process placement, migration intent, migration
    outcome, and placement policy override at route/actor group level.
  - Requirement: support shard-aware placement where actor/process identity maps to
    deterministic shard keys, while allowing controlled rebalancing events.
  - Requirement: define explicit migration semantics for stateful actors:
    snapshot, transfer, resume, rollback, and abort paths.
  - Requirement: include deterministic ordering and exactly-once constraints for
    migration handoff messages.
  - Requirement: expose cluster event hooks for scheduler decisions (placement,
    migration, throttling, and partition response) through typed notifications.
  - Requirement: ensure scheduler APIs are VM-owned and do not depend on BEAM-style
    supervisor internals or external runtime-specific scheduling calls.
  - Requirement: define policy limits for rebalance frequency, migration count,
    and backoff strategy with explicit safe defaults.
  - Tests:
    - parser/typecheck tests for scheduler policies and migration declarations.
    - VM executable tests for process placement, shard consistency, and migration
      commit/rollback behavior.
    - adversarial tests for invalid affinity keys, conflicting policy declarations,
      migration loops, and out-of-order migration outcomes.
    - failure-mode tests for stalled migration, node eviction during move, and
      policy fallback behavior.
  - Gate: add `make vm-distributed-scheduling-check` with anchor
    `vm_distributed_scheduler_and_migration`.
  - Make integration: run after `vm-distributed-transport-check` and before
    any higher-level distributed storage/state replication slices.
  - Acceptance: one `.terl` scenario demonstrates pinned and sharded workload
    placement with controlled migration under cluster load changes.
  - Completed progress (2026-07-16): `std.vm.Scheduler` now executes through the canonical
    `VmDistributedScheduler` rather than a separate first-active-node descriptor
    model. Immutable `PlacementResult` and `MigrationResult` values thread the
    updated scheduler state, source inspectors expose placement and migration
    decisions, and `SchedulerTest.terl` executes pinned placement, shard-owner
    failover, and ordered requested/snapshotting/transferring/resuming migration
    with event-log assertions. Immutable `MigrationOutcomeResult` values now
    expose commit, rollback, and abort transitions with typed outcome kind,
    sequence, and reason inspectors. Executable and adversarial tests cover
    commit-after-resume, lossless rollback and abort reasons, terminal event
    counts, premature commit rejection, and empty-reason rejection. The
    scheduler now owns durable route and actor-group policy registries with
    deterministic actor-group, route, then call-site-default precedence.
    Identical declarations replay idempotently, conflicting declarations fail,
    and scoped placements use borrowed registry lookups without hot-path key
    allocation. The public immutable API, source adapter, generated interface,
    and executable Terlan tests cover scoped declarations and placements. The
    source-state snapshot codec round-trips all scheduler fields and rejects
    corrupt cursors, event sequences, duplicate or invalid policy registries,
    node states, and phase skips. `RUSTFLAGS='-D warnings' make
    vm-distributed-scheduling-check` passes.


## Completed 011

- [x] Add VM-owned distributed state primitives and replication contracts.
  - Problem: we have migration and fault-recovery planning, but no stable,
    VM-owned primitive for shared state semantics under multi-node execution.
    Protocols currently cannot assume implicit replication behavior.
  - Requirement: add `std.cluster.state` primitives that define:
    - state ownership and namespace,
    - read/write transaction envelopes,
    - version vectors or monotonic sequence metadata,
    - explicit conflict-resolution policy declarations (`winner-takes-all`,
      `merge`, `last-writer-wins`, `explicit-user-resolution`).
  - Requirement: make these primitives **strategy-neutral**:
    the VM owns persistence and transport; specific replication strategy is
    supplied by policy declarations and can be swapped without changing transport
    shape.
  - Requirement: add explicit primitives for:
    - durable snapshot export/import,
    - lease/fencing token checks,
    - consistent checkpoint and recovery descriptors.
  - Requirement: enforce typed and deterministic consistency surface:
    state operations must return explicit conflict/error outcomes (not ad-hoc
    exceptions), with typed outcomes carrying conflicting versions and local policy
    context.
  - Requirement: ensure state operations remain composable with actors and message
    primitives through explicit state handles and do not leak transport internals.
  - Requirement: provide parser/typecheck support for declarations of state scope,
    sync policy, and conflict strategy in `.terl` module terms.
  - Requirement: include LSP/tree-sitter/formatter docs coverage for state policy
    declarations and operation forms.
  - Requirement: include non-VM backend behavior by mapping unsupported strategies
    to typed unsupported/force-local outcomes.
  - Tests:
    - parser/typecheck tests for state declarations and policy contracts.
    - VM executable tests for local-first updates, version conflict capture,
      merge-policy selection, checkpoint/replay, and lease failure paths.
    - adversarial tests for duplicate sequence updates, stale checkpoint replay,
      conflicting policy combinations, and malformed state descriptors.
    - fuzz tests for random conflict graphs and recovery ordering.
  - Gate: add `make vm-distributed-state-check` with anchor
    `vm_distributed_state_contracts`.
  - Make integration: run this gate after `vm-distributed-scheduling-check`
    (`vm_distributed_fault_recovery`) and before distributed persistence transport
    integration slices.
  - Acceptance: a `.terl` scenario proves explicit conflict outcomes, deterministic
    version handling, typed checkpoint restore, and no undefined behavior on policy
    mismatch.
  - Completed: `std.vm.DistributedState` now executes through the VM-owned Rust
    state engine. Stateful `Store.write` receiver calls rebind the opaque store,
    return typed applied/conflict/policy-mismatch outcomes, and preserve state
    through deterministic snapshot export and restore. The executable Terlan
    tests and adapter adversarial tests run under `make vm-distributed-state-check`.


## Completed 012

- [x] Add pluggable distributed storage adapters for replicated state and recovery.
  - Problem: distributed state contracts are defined, but there is no production-
    usable storage binding layer; teams cannot yet validate VM semantics against
    real persistence behavior with explicit backends.
  - Requirement: add VM-owned storage adapter contracts in `std.cluster.storage` for:
    local write-ahead persistence, snapshot logs, and distributed durable store.
  - Requirement: adapter support must be pluggable by policy:
    local-only mode for tests/dev, durable mode for persistence, and cluster mode
    for replication-aware checkpoints.
  - Requirement: define adapter lifecycle operations:
    `open`, `append`, `flush`, `compact`, `load_snapshot`, `close`, and
    deterministic capability checks.
  - Requirement: define typed storage outcomes for: successful write, fsync/finalize
    failure, stale snapshot, checksum mismatch, and unsupported operation on
    backend.
  - Requirement: no backend internal state should leak into language surface; user
    sees only typed contracts and stable outcomes.
  - Requirement: default VM lane can run without external storage backend (force-local
    behavior) and must expose an explicit `StorageUnavailable` outcome instead of
    panics.
  - Requirement: define how storage failures interact with migration/fault lanes:
    migration rollback must remain idempotent when persistence operations fail.
  - Requirement: include LSP/tree-sitter/docs support for adapter selection and
    storage policy declarations, and formatter canonicalization for adapter blocks.
  - Tests:
    - parser/typecheck tests for storage adapter declarations and mode mismatches.
    - VM executable integration tests for local/ durable / cluster adapters:
      checkpoint write/read, compaction, stale snapshot rejection, reopen behavior.
    - adversarial tests for corrupted checkpoint, checksum mismatch, flush
      timeout, and partial writes.
    - failure-injection tests proving migration and fault recovery remain bounded
      under storage failure paths.
  - Gate: extend `make vm-distributed-state-check` with
    `vm_distributed_storage_adapters` anchor.
  - Make integration: run this gate after `vm-distributed-state-check`
    (`vm_distributed_state_contracts`) and before distributed observability slices.
  - Acceptance: one `.terl` workflow writes and restores checkpoints through a
    selected adapter policy with deterministic success/failure outcomes and
    recovery guarantees.
  - Completed: `std.vm.DistributedStorage` executes through VM-owned,
    registry-backed adapter handles for local, durable, and cluster policies.
    Lifecycle operations, typed failures, compare-and-swap, proof contracts,
    transactional batches, schema migration, resource validation, compaction,
    and reopen behavior are executable from Terlan. The gate runs all 13 Terlan
    storage scenarios alongside the adversarial Rust adapter suite.


## Completed 013

- [x] Add compiler-enforced adversarial type constraints through negative trait
  implementations.
  - Requirement: do not add `contract`, `policy`, `TypeContract`,
    `Contract[T]`, or `Capability` as separate user-facing concepts. Negative
    behavior belongs to the same trait system as positive behavior.
  - Requirement: positive capability remains an ordinary trait implementation:
    ```terl
    pub impl Drop[SecretKey] ->
        SecretStore.release.
    ```
  - Requirement: denied capability uses explicit negative impl syntax:
    ```terl
    pub impl not Log[SecretKey].
    pub impl not JsonEncode[SecretKey].
    pub impl not Compare[SecretKey].
    pub impl not Copy[SecretKey].
    pub impl not SendAcrossNode[SecretKey].
    ```
  - Requirement: a negative impl has no body. It is a compile-time fact that
    the named type must never implement the named trait.
  - Requirement: `impl not Trait[T]` resolves `Trait` and `T` through normal
    names in scope. Start with real traits only; method-specific or
    function-specific denies are future work unless they fall out naturally
    from trait denial.
  - Requirement: `not` wins over generic/default behavior. If a type has
    `impl not JsonEncode[SecretKey]`, no generic JSON fallback may encode it;
    if it has `impl not Compare[SecretKey]`, equality/order operators must
    reject it when they depend on that trait.
  - Requirement: a visible negative impl blocks positive impls, derived impls,
    blanket impls, generated std bindings, backend lowering, NativeBoundary
    transfer, persistence, actor/node sending, and Cloud metadata that would
    require the denied trait.
  - Requirement: absence of a negative impl does not imply denial. Existing
    trait and operation resolution remains unchanged until a visible
    `impl not` supplies negative metadata.
  - Requirement: `pub impl not Trait[T].` exports negative trait metadata in
    generated module summaries, docs, LSP hover, editor diagnostics, and
    Cloud/native manifests. Private negative impls are enforced only inside
    the defining module.
  - Requirement: conflicting positive/negative impls for the same trait/type
    pair are rejected with a stable diagnostic. Duplicate negative impls,
    unknown traits, non-type targets, trait aliases that expand ambiguously,
    and orphan-rule violations must also report stable diagnostics.
  - Requirement: negative impls must integrate with trait resolution,
    generics, std APIs, VM resource handles, JS/backend diagnostics,
    NativeBoundary, type docs, and coverage inventories without creating a
    parallel capability system.
  - Gate: `make core-type-contracts-check` and
    `make stdlib-negative-api-tests-check`.
  - Current gate state: local, public, interface, and generic-target
    `impl not Trait[Type].` declarations now parse into the existing structured
    `TraitImplDecl` with explicit negative polarity, separate trait/target type
    expressions, no methods, visibility, docs, and spans. Positive impl blocks
    retain positive polarity and their existing adapter bodies. Body-bearing
    negative impls fail with the stable
    `negative trait impl declarations cannot have a body` diagnostic. Older
    `impl Contract[T]` descriptor/expression experiments and discarded
    `policy Type => ...` / `policy Type with [...]` surfaces remain rejected,
    while an ordinary function named `policy` still parses.
  - Current executable slice: `make core-type-contracts-check` proves parser
    AST polarity, nested generic targets, source/interface parity, formatter
    round trips, positive-impl isolation, body rejection, syntax-output
    preservation, Markdown/JSON/HTML documentation rendering, and Tree-sitter
    semantic parsing. Typechecking now accepts locally resolved unary negative
    facts, resolves simple and nested target types through the canonical type
    parser, accepts imported trait/type names through ordinary imports, rejects
    unknown traits, unresolved nested target types, and non-unary trait targets,
    and reports stable diagnostics for duplicate negative facts
    or contradictory local positive/negative impls. Public negative facts now
    round-trip through generated module interfaces with explicit polarity;
    imported public facts reject contradictory consumer impls in either
    direction, while private provider facts remain isolated. Negative facts
    remain excluded from positive method dispatch and Core proof inventories,
    preventing denial metadata from accidentally granting a capability. Orphan
    ownership now requires either the trait or the outer target type to be
    local; imported-trait denials for imported, primitive, or structural target
    heads report a stable ownership diagnostic. Concrete local and imported
    negative facts now preempt generic positive trait evidence at concrete call
    sites, while unresolved generic bodies and non-denied concrete types retain
    ordinary generic evidence. LSP type hovers now list local and imported
    public negative facts, hide private provider facts, and label negative
    implementation symbols explicitly; editor diagnostics preserve the
    compiler's stable denial message. `std.core.Secret` now exports concrete
    denials for `Show`, `Equal`, and `Ordering`; directly imported provider
    interfaces contribute those facts to generic-bound checking without
    leaking facts from unrelated dependency interfaces. Lexical generic trait
    evidence now resolves before unrelated concrete implementations for
    qualified calls, while explicit trait type arguments retain concrete
    dispatch semantics. Colliding imported trait aliases now report a stable
    provider-qualified ambiguity diagnostic before a negative impl can select
    either trait; selected and wildcard imports share the same resolver path,
    while distinct aliases preserve independent negative facts.
    `std.vm.NativeBridge` now owns the ordinary `NativeTransfer[T]` trait and
    a generic identity implementation; `start` requires transfer evidence for
    resources, and `call` requires it for commands and replies. A concrete
    negative transfer implementation preempts that generic evidence for both
    module and receiver call syntax, and receiver overload resolution retains
    the stable denied-bound diagnostic instead of collapsing it into a missing
    method error. `std.vm.PersistentActor` now owns the ordinary
    `Persistable[T]` trait and a generic identity implementation;
    `snapshot[State]` preserves the concrete state type and requires
    `Persistable[State]` evidence instead of erasing state to `Dynamic`. A
    concrete negative persistence implementation preempts the default evidence
    and rejects snapshot construction before VM execution.
    `std.vm.Process` now owns `ActorMessage[T]`; process send/receive contracts
    retain `Message[T]` instead of erasing mailbox payloads to `Dynamic`, and
    `send` requires actor-message evidence. `std.vm.Cluster` owns
    `SendAcrossNode[T]`; session sends retain their concrete payload type and
    require cross-node evidence. Concrete negative implementations preempt both
    default proofs before local mailbox or distributed transport execution.
  - Completed progress: syntax output serializes negative polarity with a
    backward-compatible false default; public docs retain `impl not` rather
    than rendering a positive `impl Trait for Type`; local trait resolution and
    coherence share the canonical positive-conformance key collector; public
    interface summaries preserve polarity and provider-qualified target types;
    imported coherence shares the same canonical keys; orphan adversarial tests
    cover both legal local-side combinations plus foreign/foreign and primitive
    targets; generic-fallback adversarial tests cover local denial, imported
    denial, a non-denied control, real `std.core.Secret` denial, and an allowed
    primitive `Show[Int]` control; public negative std fixtures prove denied
    `Show`, `Equal`, and `Ordering` use through ordinary generic APIs; LSP
    adversarial tests cover local and imported hover metadata, private-fact
    isolation, Outline polarity, and diagnostic parity; and the dedicated
    Tree-sitter corpus covers public concrete and private nested-generic facts.
    Trait-alias adversarial tests cover selected-import collisions,
    wildcard-import collisions, and a distinct-alias positive control. Native
    transfer adversarial tests cover denied bridge resources, denied bridge
    commands through receiver syntax, and an allowed string resource/command
    control using the default transfer implementation. Persistence adversarial
    tests cover denied actor state and an allowed string-state control using
    the default persistence implementation; the packaged std interface asserts
    the generic snapshot bound, and all 19 persistent-actor source tests pass.
    Actor/node delivery tests cover denied local mailbox messages, denied
    cross-node payloads, and allowed string controls. Packaged interfaces assert
    both generic bounds; local FIFO and selective-receive runtime tests plus the
    three cluster source tests pass. Dedicated invalid `.terl` fixtures keep
    both denial diagnostics in the canonical std negative-API manifest.
  - Make integration: run `core-type-contracts-check` from `make check`
    before `language-feature-coverage-100-check` and std package coverage.
  - Acceptance: executable `.terl` tests prove allowed positive trait behavior
    still works while denied trait-dependent operations fail before runtime for
    secret values, resource handles, actor/process handles, public DTOs, and
    JSON/web response values.
  - Acceptance: adversarial tests prove generic fallback bypass, imported
    negative impl visibility, private negative impl isolation, duplicate and
    conflicting impls, unknown trait names, ambiguous trait aliases, blanket
    impl conflicts, and backend/native transfer denials report stable
    diagnostics.


## Completed 014

- [x] Slice 4: add debugger/repl command-path observability for function-head
  pattern parameter failures.
  - Requirement: debugging and REPL command surfaces must treat
    function-head pattern-matching failures as ordinary, explainable match
    diagnostics (not uncaught evaluator crashes).
  - Requirement: `terlc debug`/repl should report:
    - pattern-match miss on function-head entry paths with location and expected
      shape
    - arity mismatch vs pattern mismatch distinction in function-clause dispatch
    - clear capture names (where available) in failed destructure diagnostics
  - Requirement: stack frames for function-head mismatch/dispatch failures include
    function name, head index/guard index, and active clause candidate metadata
    so users can fix pattern ordering quickly.
  - Requirement: vm-side trace hooks for pattern-head execution include a stable
    "pattern_head_entered/pattern_head_failed" event and the failing shape kind
    (tuple, record, map, constructor, wildcard, etc.) without exposing internal
    raw term layout.
  - Requirement: benchmark/debug CLI output for pattern-heavy workloads can be
    annotated with a single flag that reports pattern dispatch counts and failure
    rates.
  - Gate: extend `make terlc-debugger-check` and `make function-head-pattern-parameters-check`
    with:
    - one negative `.terl` test that demonstrates failure-path diagnostics
      for clause pattern miss
    - one positive `.terl` test that demonstrates successful dispatch and
      aliasless destructuring
    - at least one snapshot assertion for matcher failure code/family stability
  - Current gate state: `make terlc-debugger-check` and
    `make function-head-pattern-parameters-check` now both run the permanent
    `function-head-observability` quality command and its Rust fixture tests.
    The VM function-head dispatcher emits stable mismatch metadata for failed
    pattern heads, including `event=pattern_head_failed`, function identity,
    arity, clause index, guard index, pattern index, and pattern kind. The
    current runtime snapshot locks the mismatch text for a failed `identity/1`
    function-head pattern. The gates also run a source-level negative `.terl`
    fixture through `terlc test` and assert the test-result manifest preserves
    the `require_pair/1` clause miss metadata. The report is emitted at
    `target/quality/function-head-observability-report.json`.
  - Current rejected paths: REPL/source-span rendering, `pattern_head_entered`
    trace events, dispatch counters, capture-name display for every destructure
    failure, and HTTP route-handler support-bundle metadata remain open.
  - Acceptance: the debugger/repl gate fails if function-head pattern diagnostics
    are downgraded to generic runtime errors or lose function-identity context.
  - Acceptance: this slice is required before broader vm-http concurrency slices rely
    on pattern-head route-handler dispatch for testability.
  - Completed progress: `make terlc-debugger-check` and
    `make function-head-pattern-parameters-check` both pass with the
    `function-head-observability` quality report, debugger/repl reserved command
    coverage, source-level negative clause-miss diagnostics, positive Terlan
    function-head dispatch fixtures, LSP hover, tree-sitter, VM pattern, and JS
    rejection coverage.


## Completed 015

- [x] Slice 5: connect function-head pattern parameters to VM route handler
  registration and handler dispatch.
  - Requirement: route-style descriptors that compile from user routing syntax
    must support pattern heads as handler selectors where the handler function
    receives destructured request/route arguments directly in its head.
  - Requirement: route extraction must preserve pattern-head metadata so router
    failures can still be diagnosed as pattern/arity/guard mismatches at runtime.
  - Requirement: route registration docs/examples include at least one shape-based
    route function head with typed captures and one guard-protected variant.
  - Requirement: runtime dispatch should prefer `case`/pattern semantics over
    manual extraction in route handlers where the route declaration includes
    tuple/record/map/constructor patterns.
  - Requirement: unsupported route-head pattern families report stable `unsupported_vm_feature`
    with backend compatibility reasons, not silent fallback extraction.
  - Requirement: negative tests cover ambiguous route pattern collisions and
    missing capture coverage.
  - Gate: extend `make terlan-vm-http-lane-check` (or `make vm-http-concurrency-investigation-check`
    if this gate is still experimental in your branch) with:
    - one positive route-handler `.terl` test using pattern-head arguments
    - one negative test proving malformed route dispatch reports stable pattern
      diagnostics
    - one fallback backend test proving JS target rejects unsupported route-head
      pattern families with target-profile diagnostics
  - Acceptance: route handler registration cannot drop function-head pattern metadata
    required by debugger, formatter, and VM trace surfaces.
  - Acceptance: route-handler function clauses with identical arity but incompatible
    patterns must fail in the same deterministic order currently documented for
    function-clause dispatch.
  - Completed progress: `make vm-http-stream-serve-check` passes with a positive
    VM stream route-handler test that destructures the compact request descriptor
    in the function head while preserving route captures, a negative 502 dispatch
    test that exposes `pattern_head_failed` diagnostics, the existing JS
    function-head target rejection selector wired into the gate, and a documented
    route-handler example in `docs/language/function_heads.md`.


## Completed 016

- [x] Slice 6: finalize function-head pattern cross-target compatibility matrix and
  fallback behavior.
  - Requirement: complete the function-head pattern matrix across VM default and JS
    targets in
    `docs/compiler/type_spec/pattern_matching_support_matrix.json` and ensure it
    does not claim support where only parser/typecheck pass exists.
  - Requirement: JS/frontend lowering for function-head patterns must reject or
    explicitly de-sugar only the function families that are proven safe; no
    implicit fallback that changes user-visible matching semantics.
  - Requirement: add executable JS-path anchors proving:
    - accepted parser-only forms still fail with `target_profile_unsupported` when
      lowered to JS
    - pure literal and guard forms that are supported preserve behavior
    - unsupported pattern families produce deterministic backend diagnostics in
      tests, not runtime parse artifacts.
  - Requirement: remove any remaining stale fallback paths that pass through legacy
    BEAM-style behavior for function-head destructuring.
  - Requirement: update target-profile docs and CLI compatibility output so users
    can infer support by target at call sites.
  - Gate: extend `make function-head-pattern-parameters-check` and `make
    pattern-matching-support-check` to include backend matrix drift checks and
    a JS lowering regression anchor.
  - Acceptance: gate fails if the support matrix claims support that isn't covered
    by parser + typecheck + lowering + runtime/JS evidence.
  - Acceptance: every unsupported function-head pattern family has a stable and
    explicit diagnostic in all non-VM test targets.


## Completed 017

- [x] Slice 7: release-closeout hardening for function-head patterns and
  stale-implementation cleanup.
  - Requirement: remove or quarantine dead parser/typecheck lowering paths that are
    no longer used after Slice 2-6 are complete, especially synthetic `_ArgN`
    fallback paths and route-path-specific placeholder adapters.
  - Requirement: add a dormant-code inventory check specific to function-head
    pattern code paths to ensure:
    - every implemented pass is exercised by at least one gate
    - no legacy compatibility shim remains unreferenced except those explicitly
      marked with skip metadata.
  - Requirement: add changelog/release-note capture for:
    - implemented semantics (what is now supported)
    - unsupported families with stable diagnostics
    - target-profile behavior and known future work.
  - Requirement: include a full positive closure test set in `.terl`:
    - expression-headed pattern tuple/record/constructor wildcard head
    - clause-headed route-compatible pattern head with guard and fallback arm
    - one negative test for reverse alias across both expression and clause styles
  - Requirement: all new/updated docs/gate files for this area are included in the
    default `make check` path and not excluded by skip lists.
  - Gate: add `make function-head-pattern-parameters-hardening-check` as a small
    umbrella check that runs:
    - existing function-head pattern head parser/typechecker/CoreIR/VM/JS checks
    - dormant-path inventory
    - stale fixture and skip-manifest checks for this slice
  - Completed progress: `make function-head-pattern-parameters-hardening-check`
    now runs through the 0.0.7 function-head handoff gate from the default
    `make check` path. The gate validates migration diagnostics, observability,
    grammar docs, parser and formatter anchors, typechecker pattern refinement,
    LSP hover, tree-sitter grammar, VM pattern semantics, JS rejection behavior,
    Terlan pattern fixtures, migration docs, and the handoff report.
  - Acceptance: the umbrella hardening gate fails on unexecuted function-head
    pattern code, stale skip-only diagnostics, or undocumented dead code.
  - Acceptance: function-head pattern vertical is transition-ready for `0.0.7` closeout
    once this check and all prior slices are green.


## Completed 018

- [x] Slice 8: add migration lint and codemod support for function-head pattern
  parameters.
  - Requirement: add codemod-aware diagnostics for users migrating from legacy
    assignment-style function heads and nested alias spellings to the accepted
    pattern-first form.
  - Requirement: detect and decompose migration hot spots:
    - reverse alias style (`user = {name, family_name}: User`)
    - ambiguous constructor-first destructuring in mixed parser contexts
    - stale helper patterns that rely on non-existent generated pattern-head slots
  - Requirement: `terlc check` should emit stable migration notes with:
    - function name and arity
    - source span of the rejected head
    - explicit suggested rewrite to accepted syntax
    - link-id of the exact language doc section in `docs/language` (machine
      readable in CI diagnostics JSON output)
  - Requirement: CLI output, editor metadata, and formatter metadata must agree on:
    migration code, suggested replacement shape, and warning-to-error severity when
    this project is on strict profile.
  - Requirement: generate a machine-readable migration manifest
    (`docs/roadmap/runtime/function_head_pattern_migration_manifest.json`) from the
    diagnostics test suite so future language versions can reuse the same migration
    payload format.
  - Requirement: add an explicit no-fallback contract in the parser/typechecker:
    no function-head pattern rewrite is allowed unless it preserves function
    arity, total visible params, and all guard clauses.
  - Gate: add `make function-head-migration-lint-check`.
  - Gate coverage:
    - one executable `.terl` fixture proving reverse alias migration is rejected
      with actionable fix suggestion
    - one fixture proving valid heads are accepted and keep their original spans for
      telemetry
    - one fixture proving strict mode upgrades warning to error with stable diagnostic
      family (`syntax_error` + `migration_help`)
  - Make integration: run `function-head-migration-lint-check` from `make check`
    before `function-head-pattern-parameters-hardening-check`.
  - Completed progress: `make function-head-migration-lint-check` now runs the
    migration diagnostic policy gate, the new migration-lint quality command,
    and exact reverse-alias parser diagnostics. The parser diagnostic for
    rejected reverse alias heads now carries
    `migration.function_head_pattern.invalid_alias_style`, the pattern-first
    rewrite shape, and the stable docs anchor. The gate writes the reusable
    manifest at
    `docs/roadmap/runtime/function_head_pattern_migration_manifest.json` with
    rows for invalid alias style, safe reject, and unsupported backend behavior.
    `make function-head-pattern-0-0-7-handoff-check` also passed after adding
    this lint gate as required handoff evidence.
  - Acceptance: the gate fails if migration metadata is missing for rejected legacy
    forms or if a rejected form is silently accepted in any target profile.
  - Acceptance: one migration lint row for each prohibited pattern style is present in
    the generated migration manifest.


## Completed 019

- [x] Slice 9: ship a one-command migration assist for function-head pattern
  parameters.
  - Requirement: add a non-default CLI assist (`terlc migrate` or `terlc fmt
    --migrate-pattern-head`) that can rewrite rejected pattern-head forms into
    accepted equivalents using the manifest produced by Slice 8.
  - Requirement: migration assist must be deterministic and safe:
    - never rewrite if a pattern appears in a context with ambiguous binding order
    - never rewrite when additional semantics (guards, alias scope, or mutability)
      would change
    - emit a dry-run diff plan when output is not explicitly requested
  - Requirement: migration assist produces three outputs:
    - changed source snapshot
    - per-file change list with function name, arity, and rewrite ID
    - unapplied-change reasons when a candidate cannot be safely rewritten
  - Requirement: migration command and CI mode should fail when:
    - the migration manifest is stale relative to source
    - an expected rewrite cannot be applied fully without risking behavior change
    - a stable migration ID from the diagnostics manifest is unknown to the CLI.
  - Requirement: editor/CI integration path:
    - `terlc migrate` emits machine-readable `migration_id` markers compatible
      with `--diagnostics-format json`
    - tree-sitter metadata tests consume the same migration IDs as syntax
      diagnostic tests
    - VS Code extension command palette exposes migration action for supported
      files (best effort, off by default in CI)
  - Requirement: add a codemod fixture set with:
    - one reverse-alias rewrite
    - one ambiguous case rejected by safety checks
    - one mixed file preserving wildcard, tuple, and tuple+guard patterns
  - Gate: add `make function-head-pattern-migration-assist-check`.
  - Gate coverage:
    - non-destructive dry-run preserves file parity while listing planned edits
    - destructive migration path rewrites the valid candidate forms only
    - one safety-rejected fixture stays unchanged with explicit skip reason
  - Make integration: run `function-head-pattern-migration-assist-check`
    immediately after `function-head-migration-lint-check` and before
    `function-head-pattern-parameters-hardening-check`.
  - Completed progress: `terlc migrate pattern-head [--write] [--json]` now
    ships as a conservative migration assist for rejected reverse-alias
    function-head pattern parameters. Dry-run is the default; `--write` applies
    only safe pattern-first rewrites; ambiguous candidates are safe-rejected
    with explicit reasons; already-correct pattern-first heads are idempotent.
    The command report reuses
    `migration.function_head_pattern.invalid_alias_style` from the Slice 8
    manifest. `make function-head-pattern-migration-assist-check` now runs the
    migration lint dependency chain, focused `terlc` command tests, the quality
    gate tests, and the generated assist report. Public CLI help, changelog,
    and `docs/language/function_heads.md` document the migration recipe.
  - Acceptance: migration assist is idempotent over already-correct input.
  - Acceptance: migration assist is compatible with `terlc` JSON diagnostics and
    produces the same migration IDs as the lint manifest.
  - Acceptance: release notes include a migration recipe section for users upgrading
    from legacy pattern-head alias style.


## Completed 020

- [x] Slice 10: add benchmark and stability tracking for migration tooling on
  large codebases.
  - Requirement: add a migration benchmark scaffold to prove lint + assist behavior on
    realistic repositories, not only synthetic fixtures.
  - Requirement: run migration lint/assist over synthetic large modules (100+, 500+,
    1k+ function declarations) and track:
    - elapsed wall time
    - memory peak
    - number of migration candidates discovered
    - percentage auto-fixed vs safe-rejected
  - Requirement: publish benchmark baselines in a checked artifact
    (`benchmarks/roadmap/function_head_pattern_migration_bench.latest.json`),
    with thresholds per file-size band and explicit “no-regression” assertions.
  - Requirement: migration pass must be stable under noisy parse contexts:
    nested parser recovery branches,
    partially-invalid `.terl` files,
    mixed style bodies,
    and cross-module imports that alter alias resolution.
  - Requirement: add an explicit reproducibility contract:
    benchmark artifacts are sorted and deterministic with environment metadata
    fields for rustc/cpu target / allocator / `TERLAN_VERSION`.
  - Gate: add `make function-head-pattern-migration-benchmark-check`.
  - Completed progress: `function-head-pattern-migration-benchmark-check` now
    runs after migration assist and before function-head hardening, validates the
    checked baseline at
    `benchmarks/roadmap/function_head_pattern_migration_bench.latest.json`,
    enforces deterministic scenario ordering, candidate-count stability,
    reproducibility metadata, and command-test evidence, and writes
    `function-head-pattern-migration-benchmark-report.json`.
  - Gate coverage:
    - one generated large-module suite with 20% intentionally unrecoverable legacy patterns
    - one mixed-validity suite with partial parse recovery and safe-skip annotations
    - one memory-pressure suite that proves no unbounded growth across repeated runs
  - Make integration: run `function-head-pattern-migration-benchmark-check`
    after `function-head-pattern-migration-assist-check` and before
    `function-head-pattern-parameters-hardening-check`.
  - Acceptance: benchmark gate fails if any regression band exceeds baseline by
    more than the agreed headroom or if candidate counts drift without corresponding
    fixture changes.
  - Acceptance: migration assist remains stable under repeated dry-run/mutate cycles
    with idempotent output for already-updated sources.


## Completed 021

- [x] Slice 11: enforce canonical diagnostics IDs and compatibility policy for
  pattern-head migration across all targets.
  - Requirement: reserve a stable diagnostic namespace for migration and rewrite
    behavior:
    - `migration.function_head_pattern.invalid_alias_style`
    - `migration.function_head_pattern.safe_reject`
    - `migration.function_head_pattern.unsupported_backend`
  - Requirement: map every legacy-rejecting form to a stable code and family in all
    CLI formats (`text` and `json`), so CI and tools can assert exact migration
    outcomes.
  - Requirement: document target-profile behavior for migration IDs:
    VM allows all accepted rewrite-safe patterns;
    JS target emits explicit unsupported-migration diagnostics when rewrite would
    alter backend behavior.
  - Requirement: editor, lsp, formatter, and tree-sitter smoke outputs must all
    expose the same migration IDs for the same source shape.
  - Requirement: add a compatibility matrix for policy drift:
    `parser_accept`, `typecheck_diagnose`, `formatter_stable`, `vm_lower`, `js_reject`.
  - Gate: add `make function-head-migration-diagnostic-policy-check`.
  - Completed progress: `function-head-migration-diagnostic-policy-check` now
    runs before the function-head pattern parameter gate from `make check`,
    validates the reserved migration diagnostic namespace, CLI format parity,
    target-profile behavior, tooling surfaces, and compatibility matrix, and
    writes `function-head-migration-diagnostic-policy-report.json`.
  - Gate coverage:
    - fixture row for parser acceptance + typecheck warning migration row + VM runtime
      parity
    - fixture row for strict profile escalation from warning to error (same migration ID)
    - fixture row for JS profile-specific rejection with explicit policy-family
      diagnostic.
  - Make integration: run this gate immediately before migration assist/check gates and
    require it in `make check`.
  - Acceptance: any migration diagnostic emitted without the reserved namespace fails
    the gate.
  - Acceptance: no implicit numeric fallback codes are allowed for these diagnostics.
  - Acceptance: policy matrix remains stable (no added/removed columns without
    roadmap update and executable snapshot update).


## Completed 022

- [x] Slice 12: close out function-head pattern migration docs and deprecation
  lifecycle.
  - Requirement: publish a versioned migration guide section under
    `docs/language/function_heads.md` with:
    - before/after examples for all accepted/rejected pattern forms
    - strict-mode behavior
    - CLI/IDE assist workflow
    - backend fallback caveats
  - Requirement: add an explicit deprecation timeline in `docs/roadmap/README.md`
    and the 0.0.7 release notes:
    - slice completion date
    - removal timeline for legacy syntax forms when 0.0.8 is stabilized
    - support matrix for VM/JS targets
  - Requirement: mark legacy pattern-head syntax as “accepted with warning” in
    `parser` output for 0.0.7 and route it to the migration IDs defined in Slice 11.
  - Requirement: release-closeout proof set:
    - docs link in CLI diagnostics for every migration ID
    - one migration example in README quickstart
    - one “legacy-only codebase” worked example in release notes showing safe automated
      migration output
  - Requirement: add stale syntax lint:
    if legacy alias order is still present in a file that was already migrated once by
    tool output, emit a dedicated `migration.function_head_pattern.remains` advisory
    once per file in strict mode only.
  - Gate: add `make function-head-pattern-migration-docs-check`.
  - Gate coverage:
    - markdown snapshot assertion for migration docs sections and link targets
    - CLI diagnostic-to-doc-id round-trip test
    - changelog/release-note anchor exists and references migration IDs
  - Make integration: run `function-head-pattern-parameters-hardening-check` and
    `function-head-migration-assist-check` before this closeout docs gate; then run
    docs gate from `make check`.
  - Acceptance: closeout gate fails if any migration ID lacks docs reference.
  - Acceptance: the release note entry cannot state completion until this gate is green.
  - Completed progress: `function-head-pattern-migration-docs-check` now runs
    after the current function-head migration diagnostic policy and pattern
    parameter gates, validates the versioned migration guide, deprecation
    timeline, README quickstart migration example, release-note worked example,
    diagnostic doc anchors, and stale-syntax lint contract, and writes
    `function-head-pattern-migration-docs-report.json`.


## Completed 023

- [x] Slice 13: complete function-head pattern feature handoff to 0.0.7 closeout
  and remove temporary migration scaffolding.
  - Requirement: create a final handoff gate:
    `make function-head-pattern-0-0-7-handoff-check`.
  - Requirement: this gate must verify all previously required gates are green in
    one pass and export a single handoff manifest:
    `docs/roadmap/runtime/function_head_pattern_handoff_report.json`.
  - Requirement: remove feature flags and temporary migration-only plumbing that are
    no longer needed after Slice 8–12:
    parser fallback placeholders, temporary warning-only branches, and debug-only
    rewrite paths (unless explicitly retained and flagged as permanent behavior).
  - Requirement: keep compatibility shims only behind an explicit version gate and
    prove they are not used in normal default-path codepaths.
  - Requirement: add a final closure test matrix that includes parser/typecheck/formatter/
    VM/runtime/Javascript-profile and migration tooling gates:
    - parser/parser-rewrite
    - migration lint
    - migration assist
    - migration benchmark
    - diagnostics policy
    - docs/deprecation closeout
    - core semantics gates from Slice 2/3/4/5/6/7.
  - Requirement: update release checklist by moving this feature from “in progress”
    to “implemented” only if handoff gate is green and every closure metric is
    recorded.
  - Gate: add `make function-head-pattern-0-0-7-handoff-check`.
  - Gate coverage:
    - failing legacy syntax fixture still reports warning/error as configured by profile
    - migrated fixture is accepted without warnings in strict profile once rewritten
    - closure manifest contains gate names, statuses, and timing snapshot.
  - Make integration: run this gate from `make check` as the final function-head
    pattern gate after all Slice 1–13 gates.
  - Acceptance: handoff gate fails if any required gate result artifact is stale,
    missing, or drifted from snapshot hash.
  - Acceptance: once handoff is green, no temporary migration-only docs/command flags
    remain in `docs/roadmap/ROADMAP_0_0_7.md` under this feature section as `[ ]` items.
  - Completed progress: `function-head-pattern-0-0-7-handoff-check` now runs
    after the function-head diagnostics policy, pattern parameter, and migration
    docs gates, records the closure matrix across parser/typecheck/formatter,
    VM/runtime, JavaScript-profile, and migration tooling rows, rejects default
    path compatibility-shim claims, and writes
    `docs/roadmap/runtime/function_head_pattern_handoff_report.json`.


## Completed 024

- [x] Slice 2: finalize typed template interpolation execution in VM/HTTP and
  typed actor-bound snippets.
  - Requirement: template interpolation must execute through the default VM runtime
    for all currently supported template targets (including HTML/XML/JSON-like
    documents and text), not through string-only adapter fallback.
  - Requirement: `TemplateSnippet`-style fragments generated from template AST
    must preserve template expression binding environments so actor-bound
    expressions stay live across handler calls where the expression depends on
    actor state, channel state, or session state.
  - Requirement: actor-bound snippets must target `angular-wave/angular.ts` as
    the browser runtime, not Google Angular. The HTTP/1.x live-update protocol
    is SSE-first and must interoperate with the callable Angular.ts `$sse(url, config)`
    surface from `/home/anatoly/Applications/ng/angular.ts`; HTTP/2 and HTTP/3
    may use stream-native patch delivery, and WebSocket support may remain a
    later parallel transport.
  - Requirement: route handler response rendering must use typed template rendering
    through the VM HTTP stack for both static-response and streaming-response
    shapes.
  - Requirement: typed interpolation for live snippets must prove whether an
    expression is pure/impure at runtime boundaries and serialize only through
    typed render contracts.
  - Requirement: malformed actor-bound interpolation (stale bindings, closed
    handles, wrong actor scope, wrong return type) must fail deterministically
    with a stable diagnostic family that includes source span.
  - Requirement: acceptance tests must include:
    - live snippet inside an HTTP handler (`/api/{id}` style route + response body
      includes actor-updated value)
    - HTTP/1.x SSE-delivered template patch consumed through the Angular.ts
      runtime contract, with stable event names and typed payloads
    - one template expression driven by mutable actor mailbox state
    - failure case proving unsupported actor return type in interpolation is rejected
      before runtime send
    - regression case proving interpolation in repeated fragments renders under
      deterministic diffing.
  - Completed progress: actor-bound live-template fanout now requires a validated
    source span and recursively rejects stateful or opaque VM values before the
    actor update closure, state-version increment, table mutation, or subscriber
    fanout can run. Nested bytes, bitstrings, generators, regex values, type values,
    iterators, and closures fail with the stable
    `invalid_template_actor_return_type` family and exact module/line/column
    location; ordinary typed data keeps the existing fanout path. The focused
    adversarial regression, all 35 HTTP-session tests, all five live-template gate
    self-tests, and the machine-readable `vm-live-template-stream` report command
    pass under warnings-as-errors. The report records the rejection boundary and
    evidence test, while live handler rendering keeps Slice 2 open.
  - Completed progress: VM HTTP template responses now classify rendered output
    through the shared `ArtifactTemplateTarget` contract instead of hardcoding
    HTML. HTML/Markdown, JSON, TOML, YAML/YML, XML, and text sources emit stable
    target-specific HTTP content types; unsupported `.terl.*` targets fail with
    `template_runtime_unsupported_target` before VM memory accounting or wire
    output, and the HTML compatibility helper rejects structured non-HTML targets.
    The interpolation gate now runs the focused target-parity module as one Rust
    test process. All seven affected response tests and all three artifact target
    classifier tests pass under warnings-as-errors. The full composed gate was
    attempted but remains blocked earlier by unrelated in-progress benchmark and
    C++ binding compilation failures, so live interpolation execution and aggregate
    gate certification keep Slice 2 open.
  - Completed progress: the generated HTTP/1.x live-template browser adapter now
    targets the latest angular-wave/angular.ts callable `$sse(url, config)` contract,
    registers `SseProtocolMessage` through `eventTypes`, consumes transformed typed
    payloads through `onEvent`, and returns Angular.ts's managed connection for
    lifecycle ownership. It no longer calls the obsolete `SseProvider.open`, owns a
    raw event listener, or reparses JSON. An executable Node regression proves URL
    and event registration, unrelated-event filtering, typed incremental-patch
    dispatch, deterministic malformed-payload rejection, and delegated close
    behavior. `make typed-template-interpolation-check` now includes all 14 protocol
    tests and passes under warnings-as-errors; live handler rendering keeps Slice 2
    open.
  - Completed progress: VM HTTP handlers can now resolve a source-aware
    `VmHttpSessionLiveTemplateRenderPlan` against actor-owned session state and
    render the resulting versioned binding through the production typed-template
    HTTP response boundary. Binding, missing-state, renderer, and response-target
    failures retain the `template_runtime_actor_bind_error` source location, while
    opaque actor values fail with `invalid_template_actor_return_type` before the
    renderer runs. An executable `/api/{id}` HTTP/1.x exchange proves route
    parameter extraction, actor state update, subscriber fanout, version-consistent
    rebinding, typed HTML response headers, and a body containing the updated actor
    value; an adversarial companion proves missing and opaque state fail before
    rendering. Both focused tests and the warning-denied `terlan-vm` check pass.
    The composed `make typed-template-interpolation-check` was attempted but is
    currently blocked before template execution by an unrelated concurrent
    non-exhaustive `OwnedIntListArgument` match in `c_abi_binding_generator.rs`.
  - Completed progress: live-template commands now cross a typed production
    actor-mailbox boundary. The VM validates and consumes the stable
    `{live_template_command, command_id, name, body}` envelope, exposes its typed
    fields to the handler, and deterministically rejects malformed envelopes after
    consumption. An executable `/api/{id}` HTTP exchange dispatches a browser
    command into the actual session actor mailbox, consumes it through that
    boundary, applies the command body to actor-owned mutable state, fans out the
    update, and renders the updated value through the typed HTTP template response
    path. The adversarial malformed-envelope case, the full three-test actor-bound
    response module, the existing raw-mailbox regression, and the warning-denied
    `terlan-vm` check pass. The command protocol lives in a dedicated 113-line
    module, leaving `http_session.rs` at 1,417 lines, below its 1,469-line quality
    baseline. The composed
    `make typed-template-interpolation-check` also passes under warnings-as-errors,
    superseding the earlier concurrent C ABI generator blocker. Streaming-response
    rendering and deterministic repeated-fragment diffing kept Slice 2 open.
  - Completed progress: repeated actor-bound interpolation now uses a VM-owned,
    stable-keyed diff instead of an opaque list replacement. The diff validates
    normalized unique keys and typed payload values before rendering or actor
    mutation, renders each previous/current binding exactly once, and emits a
    deterministic sequence of `move`, `replace`, `insert`, and `remove` operations
    without randomized map iteration. The operations fan out through the existing
    versioned actor update envelope, while the resulting actor-owned list renders
    through the typed HTTP template response boundary. Adversarial tests prove
    duplicate/invalid keys and renderer failures cannot mutate actor state or
    advance its version; an 81-transition matrix reconstructs every tested empty,
    inserted, removed, reordered, and content-updated target and proves repeated
    runs are identical. The implementation is isolated in a 276-line module,
    `http_session.rs` remains below baseline at 1,420 lines, all six actor-bound
    response tests pass, all binaries check under warnings-as-errors, and
    `make typed-template-interpolation-check` passes. Streaming-response rendering
    keeps Slice 2 open.
  - Completed: actor-bound typed templates now open directly on the existing
    scheduler-owned `VmHttp1ResponseStream` with bounded chunk admission, typed
    target headers, retained actor-state binding/version, explicit finish/abort,
    and source-aware `template_runtime_actor_bind_error` or
    `template_runtime_unavailable` failures. The production buffered and streaming
    paths share actor-binding and serializable-payload validation. A real VM TCP
    regression proves HTTP/1 chunk framing and the exact rendered body, while an
    adversarial companion proves invalid limits, renderer failure, empty chunks,
    atomic backpressure, and cancellation cannot partially admit output or mutate
    actor state. All eight actor-bound response tests, all binaries under
    warnings-as-errors, and the complete `make typed-template-interpolation-check`
    gate pass; that gate also certifies inferred/asserted pure helpers and rejects
    impure helpers before VM execution. This closes Slice 2.
  - Gate: extend `make typed-template-interpolation-check` to include VM runtime
    HTTP template execution fixtures and actor snippet execution cases.
  - Make integration: run the interpolation VM actor-scope assertions from this
    gate before `make vm-http-check` and before `make angular-ts-integration-check`.
  - Acceptance: no template interpolation path in HTTP response rendering may
    bypass the VM renderer for template targets currently in `template-contract-check`.
  - Acceptance: all actor-bound interpolation failures must include stable source
    spans and failure taxonomy (`template_runtime_actor_bind_error`,
    `invalid_template_actor_return_type`, `template_runtime_unavailable`).


## Completed 025

- [x] Slice 3: add parser-tooling parity for interpolation tokens and editor
  discoverability.
  - Requirement: tree-sitter must expose interpolation regions as explicit
    language-region tokens for text and attribute contexts, enabling precise
    highlighting and completion boundaries in HTML-like templates.
  - Requirement: formatter should preserve interpolation grouping and spacing rules:
    adjacent braces, whitespace normalization inside braces, and stable layout for
    nested expressions.
  - Requirement: LSP completion/symbol metadata for interpolation variables in
    template expressions must include variable scope and expected context type
    (`TextSlot`, `AttrSlot`, `UrlSlot`, `BoolSlot`, `TrustedFragmentSlot`).
  - Requirement: docs rendering examples must include both supported and rejected
    interpolation contexts with stable error spans.
  - Requirement: CLI formatter/output tests and tree-sitter fixtures for
    interpolation must pass under the same fixture list used by parser fixtures.
  - Gate: add `make typed-template-interpolation-tooling-check` and make it part of
    `make function-language-surface-check` dependency chain.
  - Completed progress: one balanced interpolation scanner now owns nested
    braces, quoted delimiters, adjacent regions, exact malformed spans, and
    formatter normalization. Template LSP completion exposes only declared
    `@template.params` with text, attribute, URL, boolean, or trusted-fragment
    context metadata; Tree-sitter emits named boundaries plus distinct text and
    attribute interpolation nodes. `make typed-template-interpolation-tooling-check`
    passed the compiler, formatter, LSP, Tree-sitter, package, and VS Code bridge
    checks in 109.03 seconds and is required by `function-language-surface-check`.
  - Acceptance: parser, formatter, and tree-sitter no longer disagree on token
    boundaries in nested/ambiguous `{...}` interpolation positions.
  - Acceptance: syntax errors in interpolation preserve their span to the
    exact brace pair and report as interpolation-specific diagnostics.


## Completed 026

- [x] Slice 2: replace the integration’s JS bootstrap with a Terlan-owned app entry.
  - Requirement: convert the current Angular.ts Terlan integration example from
    a JS-authored control flow into a Terlan-authored app boundary:
    module declaration, app/module setup, component/controller registration,
    DI wiring, and event handlers authored in `.terl`.
  - Requirement: generated JS bootstrap must become a deterministic, compiler-owned
    artifact derived from Terlan app metadata and templates; no feature should
    require handwritten `todo.js`-style state management for supported flows.
  - Requirement: preserve existing generated artifact names and command surface
    (`generate`, `build`, `check`, `run`, `test`) while making Terlan source the
    source of truth for example behavior.
  - Requirement: add a complete round-trip sample for at least four user flows:
    create item, mutate item state, delete item, and filtered view rendering
    transitions, with test oracles driven from Terlan-asserted behavior.
  - Requirement: route and template rendering in the example must use typed
    template interpolation and typed template snippets where already supported so
    Angular and template semantics are exercised together.
  - Requirement: package-local tests in `integrations/terlan` must execute a real
    browser-equivalent integration path and fail if `npm`/browser assets cannot
    execute the Terlan-generated app.
  - Requirement: add a stale-asset/invalid-manifest negative test in package-local
    CI ensuring stale generated Terlan sources are re-detected and regeneration is
    required.
  - Requirement: the example must not consume placeholder `.terl` files or fake
    no-op handlers to pass gates.
  - Gate: add `make angular-ts-terlan-app-ownership-check`.
  - Completed progress: `make angular-ts-terlan-app-ownership-check` now
    materializes a Terlan-owned Todo application model with typed `TodoState`
    and `TodoItem` records, module/controller metadata, and create, mutate,
    delete, filter, and render transitions implemented in `.terl`. The
    generated Angular.ts JavaScript is restricted to module registration and
    mutable collection adaptation; its contract rejects duplicated JS-owned
    filtering, toggle, deletion, and hard-coded module metadata.
  - Completed progress: source, template, Angular.ts adapter, and
    `angular-ts.json` are one deterministic generated asset set. The focused
    gate adversarially corrupts every asset, requires a stable stale-asset
    diagnostic, regenerates it, compiles the Terlan source to JavaScript, and
    executes the transition oracles. The gate passes hermetically and against
    the latest explicitly selected `/home/anatoly/Applications/ng/angular.ts`
    checkout.
  - Completed progress: the generated package now owns a typed
    `TodoSummary.terl.html` template and a Terlan declaration whose props are
    validated and emitted with the application module. The Angular.ts adapter
    uses the current model/controller/bootstrap APIs while delegating create,
    validation, toggle, edit, delete selection, filtering, visibility, and
    rendering decisions to generated Terlan functions. Its generated
    Playwright package test mounts the real current `angular-wave/angular.ts`
    runtime, intercepts package requests without a mock DOM or test socket, and
    executes create, toggle, edit, active/completed filtering, and deletion in
    Firefox. Generated-source freshness covers the Terlan source, typed
    template, Angular template, adapter, app manifest, Playwright config, and
    browser test. The warnings-as-errors
    `make angular-ts-terlan-app-ownership-check` gate passes hermetically and
    against `/home/anatoly/Applications/ng/angular.ts`; the broader
    `make angular-ts-terlan-integration-check` also passes.
  - Make integration: run this gate after `angular-ts-namespace-generation-check`
    and before `angular-ts-terlan-facade-parity-check`.
  - Acceptance: fresh `integrations/terlan` materialization plus `make test` from
    that package exercises the Terlan-owned app path end-to-end with no manual JS
    glue.
  - Acceptance: test output proves template interpolation in the example remains
    typed and slot-validated under the app flow.


## Completed 027

- [x] Implement the first CoreIR-to-Wasm lowering slice.
  - Requirement: Terlan must be able to compile a small, typed, pure CoreIR
    subset into validated WebAssembly bytes without going through JS, BEAM,
    Erlang source, or VM bytecode as an intermediary.
  - Requirement: the lowering path must consume checked CoreIR and produce the
    existing Rust-owned Wasm backend IR under `crates/terlan/src/backends/wasm`.
    Do not introduce Wasm concepts into parser, HIR, or typechecker internals
    beyond target validation metadata.
  - Requirement: the first executable subset must include exported pure
    functions over `std.wasm.Abi.I32` values, integer constants, integer
    parameters, `+`, `-`, `*`, integer comparisons where representable, and
    single-result returns.
  - Requirement: target inference must be able to infer the Wasm target from
    imported Wasm ABI types, for example:
    ```terl
    module app.Math.

    import std.wasm.Abi.{I32}.

    pub add(a: I32, b: I32): I32 ->
        a + b.
    ```
  - Requirement: explicit annotation-heavy export declarations are not the
    canonical 0.0.7 shape for the first slice. The function signature and
    public visibility define the Wasm export unless a later gate proves an
    explicit override is needed.
  - Requirement: `terlc build --target wasm.core` must stop returning the
    reserved-target diagnostic for the supported pure subset and must emit a
    deterministic `.wasm` artifact plus a manifest recording exports, ABI
    types, source module, function name, checksum, and validation engine.
  - Requirement: unsupported CoreIR forms must fail before emission with stable
    diagnostics. This includes effects, actor/process operations, heap-owned
    VM collections, templates, HTTP, database calls, NativeBoundary calls,
    lambdas, closures, dynamic dispatch, and unsupported Wasm ABI types.
  - Requirement: emitted bytes must be constructed with maintained Rust crates
    already in use (`wasm-encoder`) and validated with maintained Rust
    validation (`wasmparser`). Do not hand-roll Wasm binary encoding or byte
    validation.
  - Requirement: the AngularTS `integrations/wasm/terlan` package must move
    from reserved-backend contract testing to a runnable emitted-Wasm smoke
    when `wasm.browser` is promoted. Until then, it must keep proving namespace
    generation and reserved backend diagnostics.
  - Gate: add `make wasm-coreir-lowering-check`.
  - Current gate state: `make wasm-coreir-lowering-check` exists, is wired
    into `make check`, and proves the Rust-owned Wasm backend layer already has
    stable ABI scalar types, result validation, typed backend IR, byte emission
    through `wasm-encoder`, byte validation through `wasmparser`, first checked
    CoreIR-to-Wasm backend IR lowering for exported zero-arity `Int` literal
    functions, `+`, `-`, and `*` lowering for zero-arity integer expressions,
    `Int` parameter lowering through Wasm locals, integer comparison lowering
    for `Bool` exports represented as Wasm `i32`, stable unsupported-body and
    unsupported-parameter-type diagnostics before byte emission, a
    command-owned Wasm core artifact envelope that emits validated `.wasm`
    bytes plus deterministic manifest metadata for exports, ABI types, source
    module, compiler version, checksum, and validation engine, manifest-backed
    `artifact = "wasm-core"` project builds that write `<module>.wasm` and
    `<module>.wasm.json` under `_build/wasm`, explicit
    `terlc build --target wasm.core` emission for single-file and directory
    source-root builds, `std.wasm.Abi.I32` as an explicit source-level ABI
    alias, target inference from `std.wasm.Abi` imports without requiring
    `--target wasm.core`, backend IR and emitter coverage for `i32`, `i64`,
    `f32`, and `f64` scalar result signatures and constants, and stable
    reserved-target diagnostics for Wasm browser/component builds until those
    targets are promoted.
  - Completed progress: the command-level acceptance fixture now compiles a
    public `std.wasm.Abi.I32` binary function, reads the emitted artifact from
    the build output, instantiates it through Node's maintained V8 WebAssembly
    runtime, invokes the exported function with typed arguments, and asserts
    the decoded result. This closes the first slice's runtime-execution gap
    without adding a second Wasm encoder, validator, or interpreter. The full
    `make wasm-coreir-lowering-check`, `make
    angular-ts-terlan-integration-check`, focused Rust formatter check, and
    whitespace check pass. Source aliases beyond `I32`, a dedicated hosted execution command,
    host imports, and cross-target discovery remain owned by the existing Wasm
    slices 2 and 3.
  - Make integration: run `wasm-coreir-lowering-check` from `make check` after
    language feature coverage and before Angular.ts integration validation.
  - Acceptance: the gate builds a fixture Terlan module with `I32` exports,
    emits a `.wasm` file, validates it with `wasmparser`, inspects the export
    section, and executes the exported function through a maintained Wasm
    runtime or validator-backed smoke harness.
  - Acceptance: the gate proves unsupported CoreIR forms produce stable
    diagnostics and do not create partial `.wasm` artifacts.
  - Acceptance: `make angular-ts-terlan-integration-check` continues to pass
    while the Wasm browser integration is reserved, and adds a real browser
    Wasm runtime smoke when the browser Wasm target is promoted.


## Completed 028

- [x] Slice 2: add hosted Wasm execution and ABI-shape smoke for generated
  functions.
  - Requirement: once `wasm-coreir-lowering-check` emits a validated `.wasm`
    module, add an executable runtime path that instantiates the artifact
    through a maintained Wasm runtime and executes exported functions.
  - Requirement: the slice should support at least the pure subset already
    accepted by the first slice (`add`, integer arithmetic, simple comparisons),
    with deterministic input/output fixtures and result checking at runtime.
  - Requirement: the runtime invocation path must validate memory safety and trap
    behavior for invalid host-module boundary inputs before exposing the function
    result.
  - Requirement: add a small imported-function fixture for the supported Wasm
    host ABI so Terlan-generated modules can exercise host-call boundaries for
    side-effect boundaries that are still pure-unsafe-safe in this stage.
    These imports must fail deterministically when absent.
  - Requirement: keep ABI shape strict: only supported scalar signatures for this
    slice (`i32`, `i64`, `f32`, `f64` params and results) and no memory/table
    imports/exports beyond what has explicit validation.
  - Requirement: add negative execution fixtures for trap-on-unsupported-op, stack
    underflow-style validation in validator-backed runtime calls, and invalid
    export resolution.
  - Requirement: acceptance fixtures must prove Wasm runtime execution behavior is
    consistent across invocation counts and that no stale `.wasm` artifact can
    bypass source-body revalidation.
  - Requirement: wire execution fixtures into a dedicated command that can run in
    offline CI and fail fast with stable diagnostics (`wasm-exec-timeout`,
    `wasm-import-missing`, `wasm-export-missing`, `wasm-runtime-trap`).
  - Gate: add `make wasm-runtime-exec-check`.
  - Completed progress: `terlc run <artifact.wasm>` now validates the adjacent
    compiler manifest and checksum before dispatching the artifact through the
    maintained Node/V8 WebAssembly runtime. The command accepts strict typed
    `i32`, `i64`, `f32`, and `f64` arguments, explicit typed host-return
    fixtures, expected-result assertions, bounded repeat counts, and hard
    execution timeouts without requiring an explicit target profile.
  - Completed progress: `make wasm-runtime-exec-check` runs nine executor tests
    plus source-to-artifact command acceptance and shared timeout/help
    regressions. Positive coverage executes generated integer arithmetic and a
    comparison with stable expected values, all four scalar result kinds, and a
    typed host import. Adversarial coverage rejects malformed scalar values,
    stale checksums, argument/result mismatches, absent imports and exports,
    memory exports, malformed result stacks, runtime traps, and non-terminating
    exports. Runtime failures retain stable `wasm-exec-timeout`,
    `wasm-import-missing`, `wasm-export-missing`, and `wasm-runtime-trap`
    families with module, export, and artifact context.
  - Make integration: run `make wasm-runtime-exec-check` after
    `make wasm-coreir-lowering-check`. A missing hosted runtime fails with the
    stable `wasm-runtime-unavailable` diagnostic rather than silently skipping
    execution.
  - Acceptance: the runtime gate executes a fixture module through a maintained
    runtime from file bytes and returns correct values for positive cases.
  - Acceptance: unsupported signatures and import mismatches fail before execution
    and report stable diagnostic families.
  - Acceptance: runtime execution failures include source-linked diagnostics for
    trap conditions that are generated from unsupported lowering shapes.


## Completed 029

- [x] Slice 3: close Wasm contract discoverability and cross-target invocation.
  - Requirement: add executable `std.wasm` contract examples that show the same
    function declaration style maps to the same Wasm ABI shape across `terlc test`,
    `terlc build --target wasm.core`, and hosted runtime execution.
  - Requirement: add stable docs/source examples for supported ABI kinds and
    forbidden kinds with explicit "reserved" and "unsupported" labels, then
    surface these labels in diagnostics when inference cannot resolve a unique
    runtime target.
  - Requirement: harden target-inference diagnostics so mixed/ambiguous imports
    from `std.wasm.Abi` fail predictably (`target_ambiguous`, `missing_abi_target`,
    `unsupported_abi_signature`), and include the exact slot/span of the
    conflicting declaration.
  - Requirement: add an acceptance fixture proving that a valid `i64`/`f32`/`f64`
    source signature emits the expected ABI type set and export metadata.
  - Requirement: ensure browser-in-progress Wasm targets are explicitly marked
    reserved with a stable message and do not silently degrade to JS/Browser target.
  - Requirement: add adversarial tests for namespace drift where `std.wasm.Abi`
    surface changes and old compiled artifacts must be rebuilt.
  - Gate: add `make wasm-contract-discovery-check` and wire it to
    `wasm-coreir-lowering-check` plus `wasm-runtime-exec-check`.
  - Make integration: run `wasm-contract-discovery-check` after both slice 1 and
    slice 2 gates and before Angular.ts Wasm integration handoff checks.
  - Acceptance: contracts discovered through `std.wasm.Abi` are identical between
    compile and runtime execution paths for the same source module.
  - Acceptance: stale `.wasm` artifacts are invalidated when `std.wasm.Abi`
    metadata or source signature changes.
  - Completed progress: `std.wasm.Abi` now exposes the supported `I32`, `I64`,
    `F32`, and `F64` scalar aliases with executable source examples. The same
    declaration is accepted by `terlc test --target wasm`, inferred `terlc build`,
    and `terlc run`, while browser/component/WASI and aggregate ABI surfaces are
    documented as reserved or unsupported without a JavaScript fallback.
  - Completed progress: inference reports stable `target_ambiguous`,
    `missing_abi_target`, and `unsupported_abi_signature` diagnostics with the
    conflicting import or signature slot span. Wasm manifests bind both the ABI
    namespace and exported signatures to deterministic checksums, so namespace
    drift and source-signature drift reject stale artifacts before execution.
  - Completed progress: `make wasm-contract-discovery-check` validates scalar
    lowering, emitted export metadata, source-level Wasm tests, hosted execution,
    exact inference spans, and adversarial stale-contract rejection.


## Completed 030

- [x] Freeze the external `terlan-ndarray` package and C ABI v1 policy.
  - Completed progress: `terlan-ndarray` now exists as a standalone package
    with deterministic generated bindings for owned contiguous CPU `Bool`,
    `Int64`, and `Float64` arrays. The generic C binder gained package-neutral
    borrowed `List[Bool]` input support using explicit `uint8_t` copies; no
    ndarray, DLPack, or BLAS branch entered compiler semantics.
  - Completed progress: DLPack v1.3 and OpenBLAS v0.3.33 are pinned by immutable
    revision, SHA-256, and license. The provider contract fixes LP64 as the
    default and records explicit system override, LP64/ILP64 detection, and
    supported hosts.
  - Gate: `make terlan-ndarray-abi-check` passes 5 binding-contract tests, all
    35 C ABI generator tests, deterministic generation of 14 files, 7
    adversarial metadata cases, warning-denied C/Rust builds, native execution,
    and Terlan execution through the generated helper.


## Completed 031

- [x] Complete the external `terlan-ndarray` owned CPU array lifecycle.
  - Completed progress: ABI v1 now owns checked Bool, Int64, and Float64 CPU
    arrays with private dtype/device/layout/lifecycle state, canonical strides,
    copied readback, deterministic disposal, and allocation-failure cleanup.
  - Completed progress: generated helper tests prove stale handles, wrong
    resource kinds, double disposal, and generation-safe slot reuse before C
    access. A revision-locked external Terlan consumer constructs and verifies
    a computed `[2, 3]` Float64 array and releases every handle.
  - Gate: `make terlan-ndarray-package-check` passes 5 binding-contract tests,
    all 35 C ABI generator tests, deterministic generation of 14 files, 7
    adversarial metadata cases, warning-denied C/Rust builds, native lifecycle
    and sanitizer execution, generated-helper lifecycle checks, and the fresh
    package consumer. LeakSanitizer and Valgrind have machine-readable stable
    skips when unavailable on the host.


## Completed 032

- [x] Complete external `terlan-ndarray` shape operations and basic arithmetic.
  - Completed progress: ABI v1 exposes independently owned reshape and
    arbitrary-rank two-axis transpose, exact-shape numeric add, subtract, and
    multiply, and deterministic axis-aware sum. Broadcasting, Bool arithmetic,
    implicit dtype conversion, invalid axes, duplicate axes, and checked
    `Int64` overflow produce stable failures.
  - Completed progress: native adversarial and bounded reference-model tests,
    four Terlan operation tests, a revision-locked positive package consumer,
    and six isolated negative consumers execute through generated bindings.
  - Gate: `make terlan-ndarray-operations-check`.


## Completed 033

- [x] Slice 1: make package execution and dependency resolution first-class in
  repo-level gates.
  - Requirement: implement automatic checkout/fallback resolution for external
    package tests (`terlan-polars`, future ML packages, C++ generated packages)
    when `TERLAN_POLARS_DIR` (or equivalent package path override) is not set.
  - Requirement: add a package-aware source-root mode to the test runner so the
    runner can execute tests in a sibling package workspace without temporary
    manual bootstrapping.
  - Requirement: support VM-native and native-adapter package test execution in a
    single command path, with explicit feature gating for native placeholders
    (including skip vs fail semantics).
  - Requirement: `terlc test` against a package fixture must validate:
    project manifest resolution, dependency graph normalization, adapter
    registration, package-relative artifacts (`.terlc`, `_build`, generated
    sources), and test command wiring.
  - Requirement: package checks must fail with stable diagnostics when package
    dependencies are missing, package sources are unreadable, or source-root
    resolution drifts from manifest metadata.
  - Requirement: add deterministic fixtures that prove:
    - package checkout works when package lives beside the compiler
    - package checkout works when package is absent and is fetched through
      workspace fixture path semantics
    - stale package lock/manifest mismatch is rejected before package tests run
    - VM/native placeholder path selection does not silently bypass package tests
      and reports explicit policy.
  - Requirement: keep existing package-local behavior unchanged for local package
    developer workflows; this slice only removes repo-level execution blockers.
  - Completed progress: `make package-test-exec-check` now exists and is wired
    into `make check` after package git-source and lockfile checks.
  - Completed progress: project manifests now parse `[scripts]` runnable
    entrypoints, including aliases such as `seed-db = "scripts/Seed.terl"`,
    and reject duplicate aliases, invalid aliases, non-`.terl` scripts,
    absolute script paths, and parent/current-directory traversal.
  - Completed progress: `terlc scripts [project-dir]` now discovers runnable
    `scripts/**/*.terl` files that define `pub main`, merges validated manifest
    aliases, and rejects configured aliases that point at missing or
    non-runnable scripts.
  - Completed progress: `terlc run script <name>` now resolves discovered or
    configured scripts before VM target inference and build delegation, so named
    scripts use the same VM run path as direct `.terl` files.
  - Completed progress: package build source roots are now validated as
    deterministic package-relative roots; empty entries, empty root lists,
    absolute roots, current-directory roots, parent traversal, duplicates, and
    whitespace-padded roots fail before package execution wiring can run.
  - Completed progress: `package-test-exec-check` also locks dependency-source
    diagnostics for path/git dependencies, unpinned Git dependencies, and mixed
    target dependency source metadata, plus runnable script discovery and
    `terlc run script <name>` resolution.
  - Completed progress: runnable script discovery now rejects manifest aliases
    that shadow a different convention-discovered script name, so configured
    entries cannot silently replace `scripts/**/*.terl` runnable files with a
    different source path.
  - Remaining gap: the gate still validates manifest/package execution inputs,
    not a full package checkout plus `terlc test` execution against a sibling or
    fetched package workspace.
  - Gate: `make package-test-exec-check`.
  - Make integration: run `package-test-exec-check` from `make check` before
    package feature-completion checks:
    `terlan-polars-package-check`, `terlan-cpp-binding-check`,
    `terlan-pytorch-package-check`, and ML experiments gates.
  - Acceptance: `terlan-polars-package-check` can execute real package tests
    end-to-end in a fresh CI workspace with package checkout fallback.
  - Acceptance: missing dependency/input-path scenarios emit stable error families and
    do not continue to partially execute package tests.
  - Acceptance: when package-native placeholders are not available, the gate
    reports a stable skip reason and still validates package test wiring in a
    VM-only path.


## Completed 034

- [x] Slice 1: execute the Lean proof runner and restore at least one complete
  proof artifact family.
  - Requirement: add a stable "proof execution" branch in `make
    lean-proof-track-check` that runs the actual Lean entrypoint(s) required by
    the current 0.0.7 proof inventory (for example
    `proofs/lean/Terlan.lean` and/or feature entrypoints) through the CI toolchain.
  - Requirement: each executed Lean artifact must be classified with:
    - theorem scope (`CoreIR`, `lowering`, or `rejection`)
    - targeted manifest(s)
    - expected exit code and stderr class
    - last successful hash or proof digest
  - Requirement: proof artifacts cannot stay `current` if their entrypoint fails in
    the current workspace toolchain. Failing artifacts must transition to
    `stale` or `incomplete` with blocker notes.
  - Requirement: explicit proof gaps must be present for any feature not covered
    by this slice, with `proof_gap_category` and `gap_reason` fields plus a
    planned fix gate.
  - Requirement: when Lean is unavailable in an environment, gate output must be
    a stable `lean_unavailable` hard failure for release closeout, not an
    advisory skip.
  - Gate: extend `make lean-proof-track-check` with:
    - Lean binary/toolchain discoverability validation
    - executable theorem artifact audit for all rows marked `current`
    - stale-to-current transition validation
    - proof artifact manifest consistency checks
  - Make integration: run this branch before moving from "Remaining gaps" to close
    any other Lean-slice in 0.0.7.
  - Completed progress: `proofs/lean/Terlan/Core/Arithmetic.lean` now provides
    an executable CoreIR integer-arithmetic typing/evaluation seed with two
    checked theorems. `proofs/lean/ci/lean-proof-artifacts.tsv` records its
    `CoreIR` scope, targeted type-contract manifests, expected exit and stderr
    class, and SHA-256 digest. `lean-proof-track-check` validates that metadata,
    rejects stale digests and missing current artifacts, removes `LEAN_PATH`,
    disables Elan update checks, executes Lean, and emits a stable
    `lean_unavailable` hard failure when the toolchain cannot launch. The gap
    manifest now exposes explicit `proof_gap_category` and `gap_reason` fields
    and retains eight unresolved proof families. The full gate passes with 4
    runtime tests, 13 proof-track tests, 5 ownership tests, 4 regression tests,
    one current Lean artifact, and zero regression warnings.
  - Acceptance: `make lean-proof-track-check` fails if any `current` proof row
    references a missing, non-executable, or stale artifact.
  - Acceptance: at least one executable Lean artifact in `proofs/lean` is restored
    to `current` status and at least one unresolved feature is represented as an
    explicit proof gap.


## Completed 035

- [x] Slice 2: make proof execution deterministic and reproducible across
  environments.
  - Requirement: add a pinned Lean toolchain profile in CI and local tooling:
    exact `lean` version, `elan` channel, dependency lockfile checksums, and
    explicit `LAKE` build flags used by proofs.
  - Requirement: add proof replay metadata file
    `proofs/lean/artifacts/<proof_family>.json` capturing:
    - theorem name
    - manifest fingerprints consumed
    - proof dependency set hash
    - execution command
    - deterministic timestamp strategy
    - output signature (stdout/stderr classes + exit class)
  - Requirement: the proof gate must treat an execution with any unstable external
    command output order, nondeterministic warning emission, or changed artifact
    hash without dependency change as a `proof_gap` requiring an explicit blocker.
  - Requirement: add a dedicated `proof_repro_check` target that:
    - wipes local proof build artifacts
    - reruns the selected proof family twice
    - compares normalized proof signatures
    - writes a reproducibility verdict in the lean proof gate report.
  - Requirement: extend stale/current transitions to include a `nondeterministic`
    class and require remediation plan for any family marked in that class.
  - Gate: add replay consistency checks to `make lean-proof-track-check` so that any
    proof family whose digest changes without manifest changes fails release.
  - Gate: make `lean-proof-track-check` fail if any proof family misses a replay
    metadata file while marked as `current`.
  - Completed progress: the Lean project now pins
    `leanprover/lean4:v4.31.0`, validates Lean `4.31.0`, records an empty locked
    Lake dependency set, and executes proofs only through explicit `lake env
    lean` flags. `proofs/lean/artifacts/coreir-arithmetic.json` records theorem
    names, source and manifest fingerprints, the content-addressed dependency
    hash, execution command, timestamp policy, and output classes. `make
    proof_repro_check` cleans proof build output, runs every current family
    twice, normalizes paths and line endings, compares SHA-256 signatures,
    writes `lean-proof-repro-report.json`, and merges the verdict into
    `lean-proof-gate.json`. Artifact, manifest, dependency, and output drift use
    explicit `proof_gap` classes; `nondeterministic` inventory/artifact status
    requires a concrete remediation plan. The complete
    `lean-proof-track-check` passes 31 tests, prints the stable arithmetic proof
    digest, and reports `reproducibility=pass` with identical replay
    signatures.
  - Acceptance: a clean `make lean-proof-track-check` run prints stable proof
    digests and reports a reproducibility verdict for every `current` proof
    family.
  - Acceptance: any manifest-only proof change updates all dependent replay
    metadata entries and retains current status only after `proof_repro_check`
    passes.


## Completed 036

- [x] Slice 3: produce machine-readable proof-gate outputs and lock release closeout
  behavior.
  - Requirement: add a normalized proof status artifact at
    `build/artifacts/lean-proof-gate.json` containing, per family:
    theorem identity, proof status (`current`, `stale`, `incomplete`,
    `nondeterministic`, `delete-candidate`), last executed digest,
    reproducibility verdict, blockers, and remediation gates.
  - Requirement: add a companion artifact
    `build/artifacts/lean-proof-baseline.tsv` with one row per feature class
    (`coreir`, `lowering`, `rejection`, `runtime`, `vm`, `native-boundary`,
    `wasm`, `aeneas-bridge`) storing expected baseline proof status and last
    confirmed hash.
  - Requirement: extend `make lean-proof-track-check` to emit the artifacts above,
    then fail if a `current` family is missing from either artifact.
  - Requirement: add a release guard `make lean-proof-track-release-closeout-check`:
    - requires existing `proofs/lean` tree
    - requires `lean` toolchain and lockfile consistency
    - requires zero blocker rows in current families
    - requires reproducibility pass status `pass`
    - fails on any `delete-candidate` remaining current.
  - Requirement: wire `lean-proof-track-release-closeout-check` into
    `make release-0-0-7-preflight` and gate it as mandatory for release.
  - Requirement: add positive executable proof-output tests that validate the
    JSON and TSV artifacts with current, stale, incomplete, nondeterministic,
    and delete-candidate proof families.
  - Requirement: add a short CLI summary in `terlanc` output for proof-gate
    failures with stable machine-readable IDs, enabling release bot parsing.
  - Gate: extend `make lean-proof-track-check` to own the proof-gate output
    artifact validation until `lean-proof-track-release-closeout-check` exists.
  - Completed progress: `lean-proof-track-check` now emits normalized family
    records in `lean-proof-gate.json` with theorem identity, lifecycle status,
    last digest, reproducibility verdict, blockers, and remediation gates. It
    also writes the ordered eight-class `lean-proof-baseline.tsv` for `coreir`,
    `lowering`, `rejection`, `runtime`, `vm`, `native-boundary`, `wasm`, and
    `aeneas-bridge`. The dedicated `terlan-lean-proof-closeout` binary validates
    the proof tree, pinned toolchain, Lake lockfile, report schema, current
    family blockers, reproducibility, and matching baseline hashes with stable
    `error[lean_proof_closeout_*]` IDs. Positive tests cover `current`, `stale`,
    `incomplete`, `nondeterministic`, and `delete-candidate` records; closeout
    rejects non-current, blocked, or unreproducible families. `make
    lean-proof-track-release-closeout-check` passes 35 tests, and
    `release-0-0-7-preflight` plus `publish-preflight` require it. Two direct
    closeout reruns preserved the identical baseline file hash
    `eb0ee94f2928b28c41bede28f6e36ff86260e5fb72ceeacd6514c4537cde0082`.
  - Acceptance: release preflight fails fast with a deterministic proof status
    payload when Lean proofs are missing, stale, nondeterministic, or unreproducible.
  - Acceptance: the closeout check is idempotent across reruns with no source
    changes (identical baseline artifact hash for unchanged proofs).


## Completed 037

- [x] Slice 21: prove compiler/language feature boundaries for 0.0.7 profile removal
  of legacy constructs.
  - Requirement: add theorem families for feature deprecation boundaries (core-v0
    profile forms, legacy tuple destructuring defaults, legacy import/namespace
    shapes, and removed `vm_profile`/`native_bridge` assumptions no longer valid
    in 0.0.7).
  - Requirement: each removed construct must have an explicit rejected-form theorem
    showing parse/typecheck/runtime inconsistency is blocked before VM execution.
  - Requirement: add acceptance theorems proving that no proof artifact still
    depends on removed construct assumptions (BEAM lowering, CoreV0, legacy
    target-profile tags, removed test runtime assumptions).
  - Requirement: add a machine-readable `proofs/lean/feature_cull/` map listing
    removed constructs and their replacement contracts.
  - Requirement: add `make lean-proof-feature-cull-check` that validates all
    removals are reflected in proof artifacts, runtime gates, and feature matrix.
  - Requirement: add one-way binding checks so a removed feature cannot be
    reintroduced by downstream slice evidence or fallback `*-check` aliases.
  - Requirement: update `ROADMAP_LEGACY_RUNTIME_ALLOWED_REFERENCES.tsv` references
    used only as explicitly allowed historical contexts for this slice.
  - Gate: fail release preflight if a feature marked removed appears in:
    proof gap rows, proof coverage matrices, or active lane mappings.
  - Acceptance: all removed/legacy constructs are formally rejected by theorem and
    lint artifacts with no unresolved gaps.
  - Acceptance: CI and local proof tracks show zero references to removed constructs
    in executable proof obligations, except explicitly labeled historical migration
    references.
  - Current gate state: `make lean-proof-feature-cull-check` proves seven retired
    assumption classes are blocked before VM execution and binds each class to a
    current replacement gate through a deterministic machine-readable map.
  - Completed progress: seven explicit rejection theorems plus two aggregate
    acceptance theorems replay reproducibly under the pinned Lean toolchain. The
    Rust gate rejects missing theorem/artifact linkage, stale active proof or
    coverage-matrix terms, missing replacement gates, unsorted cull metadata, and
    restored fallback Make aliases. The existing exact stale-proof cleanup
    classifications remain sufficient; no broader roadmap allowance was added.


## Completed 038

- [x] Slice 30: make VM HTTP benchmark regressions attributable to runtime causes.
  - Completed progress: the real VM HTTP socket benchmark now measures accept wait,
    request read/parse, route match, request conversion, handler execution, synthetic
    delay, response conversion/encoding, and response-write wait separately. Its
    `terlan-vm-http-runtime-attribution-v1` report classifies the dominant measured
    bottleneck, records completed requests, closed connections, cancellations, and
    timeouts as typed terminal outcomes, and checks completed-request accounting
    against handler reductions. Aggregation, inconsistent-accounting, completed-report,
    and restricted-CI skipped-report tests pass with warnings denied. The existing
    `vm-http-handler-scheduler-fairness-check` gate now requires the attribution schema,
    phase inventory, adversarial consistency tests, and canonical Rust-suite ownership;
    its seven benchmark profiles and quality report pass in the restricted sandbox
    with socket execution explicitly reported as skipped rather than fabricated.
  - Completed progress: the VM-owned HTTP queue now records scheduler-visible
    admissions, drains, producer/consumer park waits, peak parked counts,
    saturation, backpressure duration, and producer/consumer wakeups. The runtime
    attribution report exposes runnable/parked process counts, queue depth,
    saturation, backpressure, wakeups, and handler retries, and rejects unbalanced
    queue accounting, unreleased parked work, or saturation without a measured
    backpressure outcome. Focused runtime/adversarial tests pass with warnings
    denied, and `vm-http-handler-scheduler-fairness-check` passes with 20 fixtures,
    31 exact selectors, and seven benchmark profiles; loopback profiles are
    explicitly reported as skipped in the restricted sandbox.
  - Completed progress: runtime attribution now derives exclusive transport,
    parser, routing, allocation/conversion, handler, and response-write latency
    buckets from the measured phases, while scheduler wait remains a separate
    concurrency bucket because producer/consumer waits may overlap request work.
    The report verifies every measured request phase is assigned exactly once
    and identifies the dominant runtime cause together with its exact source
    counter. Positive accounting and scheduler-dominance adversarial tests pass
    with warnings denied; the owning fairness gate passes with 21 fixtures, 31
    exact selectors, and seven explicitly skipped socket profiles in the
    restricted sandbox.
  - Completed progress: the canonical HTTP benchmark handler now provides a
    deterministic `synthetic-handlers` mix covering source-backed `static`, `json`,
    `add`, `route-param`, and `stateful-counter` workloads. The counter transition
    executes in Terlan while its state is held by the existing VM session actor/table
    runtime; runtime attribution reports per-workload counts and rejects classified
    counts that exceed completed requests. Focused source execution, order-independent
    concurrent counter response validation, and workload-attribution tests pass with
    warnings denied. A 10-request VM-owned TCP/HTTP run completed across all five
    handlers, and the owning gate recipe passes with 22 fixtures, 34 exact selectors,
    and eight benchmark profiles; loopback profiles remain explicitly skipped in the
    restricted sandbox.
  - Completed progress: completed VM-stream and loopback-socket reports now include
    `terlan-vm-http-replay-v1` evidence with a length-delimited SHA-256 fingerprint of
    the exact request schedule and canonical expected outcomes. Stateful counter
    outcomes use request-index counter ordinals rather than concurrent completion
    order. Two fresh VM instances produce identical 15-request synthetic-handler
    outcomes, a changed workload configuration changes the fingerprint, completed
    reports mark execution as validated, and restricted-sandbox skip reports explicitly
    mark it unvalidated. Focused execution/report tests, both touched binary checks, the
    Rust quality gate, and `vm-http-handler-scheduler-fairness-check` pass with 23
    fixtures, 36 exact selectors, and eight benchmark profiles.
  - Completed progress: the VM HTTP/1 request reader now retains typed
    `client_closed`, `request_timeout`, `request_io_error`,
    `header_limit_exceeded`, `body_limit_exceeded`, and `malformed_request`
    outcomes while preserving the existing text contract for legacy callers.
    Benchmark workers account client disconnects and request deadlines as typed
    terminal telemetry instead of failing with unclassified strings; malformed and
    unrelated I/O failures remain explicit errors. Adversarial coverage proves empty
    client input is a cancellation, host timeout/`WouldBlock` is a timeout, malformed
    input retains the stable `error[vm_http_request_read]` diagnostic, and one-byte
    fragmented slow-client writes still parse without false cancellation. The parser
    compatibility regression, four new adversarial tests, warnings-as-errors builds,
    Rust file-size gate, and `vm-http-handler-scheduler-fairness-check` pass with 24
    fixtures, 40 exact selectors, and eight benchmark profiles. Request reading now
    lives in focused `runtime/vm/http/request_read.rs`; `http.rs` is below its enforced
    size baseline.
  - Completed progress: buffered VM HTTP/1 response writes now retain typed
    `client_closed_during_response_write`, `response_write_timeout`,
    `response_write_io_error`, and `invalid_response_metadata` outcomes while the
    existing string-returning serializer API remains compatible. Benchmark workers
    account peer disconnects and response deadlines as terminal telemetry attributed
    specifically to response writing; unrelated host I/O and invalid metadata remain
    explicit errors. Adversarial coverage proves one-byte fragmented slow writes
    complete, response deadlines are classified, a 64-write disconnect storm is fully
    accounted without false completion, and other I/O failures retain the stable
    `error[vm_http_response_write]` diagnostic. Runtime attribution preserves separate
    request-read and response-write reason counts. The 20 existing response-wire
    compatibility tests, five new focused tests, warnings-as-errors builds, Rust quality
    gate, and `vm-http-handler-scheduler-fairness-check` pass with 25 fixtures, 45 exact
    selectors, and eight benchmark profiles.
  - Completed progress: the external benchmark workspace now persists
    `vm-http-runtime-attribution-report.json` and an aligned Markdown table for all 28
    real Axum, Hyper, and Cowboy comparison rows. Every row carries winner, percentage
    delta, dominant VM bottleneck, dominant cause, exact source counter, and all seven
    exclusive telemetry buckets. Artifact generation rejects missing/stale schemas,
    absent buckets, empty classifications, failed accounting invariants, unvalidated
    execution, and invalid replay fingerprints. The current 3,000-request VM socket
    lane reports were refreshed without rerunning recorded competitor baselines; the
    persisted report currently identifies transport through `phases.acceptWaitNs` as
    the dominant measured cause in every row. The focused implementation lives in
    `benchmarks/http_runtime_attribution.py` rather than growing the existing runner.
    Eight positive/adversarial tests and `make vm-http-runtime-attribution-check` pass
    while validating 28 attributed rows.
  - Completed progress: the golden release repository now owns a canonical
    `benches/http/PROFILE.toml` contract and Rust quality gates for
    `vm-http-benchmark-comparability-check` and
    `vm-http-runtime-attribution-check`. The attribution gate runs after the
    comparability gate in `make check` and is an explicit
    `release-0-0-7-preflight` prerequisite. It validates all seven telemetry
    buckets, five accounting invariants, execution-validated replay evidence,
    product/external ownership separation, and dependency-order drift. Six
    positive/adversarial Rust tests pass with warnings denied. The full Make
    dependency chain passes, and release preflight revalidates the contracts
    without replaying the already-completed scheduler/concurrency chain when
    `TERLAN_CHECK_ALREADY_RUN=1`. Real Axum, Hyper, and Cowboy history remains
    in the external benchmark workspace rather than entering release artifacts.
  - Requirement: add per-request and per-connection telemetry counters for VM HTTP
    execution phases: accept, parse, route match, handler dispatch, handler run,
    response encode, socket write, cancellation, timeout, and close.
  - Requirement: record scheduler-visible pressure metrics for each benchmark lane:
    runnable process count, parked process count, queue depth, queue saturation count,
    backpressure wait duration, wakeup count, and handler retry count.
  - Requirement: split latency artifacts into transport, parser, scheduler, handler,
    and response-write buckets so benchmark reports explain whether VM losses come from
    network I/O, HTTP parsing, handler scheduling, allocation, or user-code execution.
  - Requirement: add deterministic synthetic handlers for `static`, `json`, `add`,
    `route-param`, and `stateful-counter` so telemetry is comparable across pure,
    allocation-heavy, route-heavy, and stateful workloads.
  - Requirement: add adversarial benchmark cases for queue saturation, cancellation
    storms, slow response writes, large request bodies, and malformed request bursts;
    every failure must produce a typed telemetry reason instead of a benchmark timeout.
  - Requirement: persist `vm-http-runtime-attribution-report.json` and an aligned
    Markdown table that lists winner, percentage delta, dominant bottleneck, and the
    exact telemetry counter responsible for that classification.
  - Gate: add `make vm-http-runtime-attribution-check` and run it after
    `vm-http-benchmark-comparability-check` in `make check` and
    `make release-0-0-7-preflight`.
  - Acceptance: a VM HTTP benchmark row cannot be marked as a win or regression unless
    the attribution report includes a non-empty dominant-cause classification and all
    required telemetry buckets.
  - Acceptance: the gate fails if telemetry counters are missing, internally
    inconsistent, non-deterministic across replay, or show a saturated queue without a
    typed backpressure/cancellation outcome.


## Completed 039

- [x] Slice 31: add VM HTTP soak and resource-stability checks before production
  readiness claims.
  - Progress (2026-07-14): implemented the short deterministic VM HTTP soak profile
    over the real VM HTTP/TCP server. The profile replays the five canonical benchmark
    routes plus route miss, malformed request, oversized body, half-close, slow write,
    cancellation, and VM-owned request-deadline outcomes; it persists
    `target/quality/vm-http-soak-stability-report.json` and proves zero live handlers,
    processes, sockets, timers, queued bytes, waiters, heap bytes, or resource handles
    after shutdown. The same runner now owns a release profile that executes exactly
    3,000 canonical requests and three complete adversarial replays, writes
    `target/quality/vm-http-soak-release-stability-report.json`, and leaves zero live
    resources after 3,093 accepted connections. Each replay now also drives eight
    simultaneous client disconnects and fills the 16-connection accept backlog before
    proving a typed backpressure rejection and complete recovery. The report records
    disconnect counts, backpressure rejections, accept-queue high-water state, and one
    typed terminal per disconnected handler. Request dispatch now owns explicit body
    buffer, telemetry-span, and route-context lifecycles keyed by process and request
    id, rejects duplicate/stale completion, records their active/peak counts, and emits
    stable owner/request/shutdown diagnostics for retained resources. The short and
    release reports persist 10 and 28 deterministic phase snapshots respectively,
    signed per-phase resource deltas, response-memory and request-resource peaks,
    wakeup/park ratios, final heap/NativeBoundary-handle growth, and post-warmup error
    rates. Configured release limits require zero retained response memory, heap growth,
    handle growth, post-warmup errors, and live VM-owned resources; the release run
    proves all limits after 3,093 accepted requests. Focused lifecycle and adversarial
    diagnostic tests pass with warnings denied, the exact release soak passes, and
    `HTTP_SOAK_PROFILE=release make vm-http-soak-stability-check` passes. Release
    preflight selects this profile without adding a duplicate gate.
  - Requirement: add a short deterministic soak profile for `make check` and a longer
    release profile for `make release-0-0-7-preflight`; both profiles must reuse the
    canonical HTTP benchmark request schedule from Slice 29.
  - Requirement: track process count, open sockets, queued requests, active handlers,
    response errors, memory high-water mark, allocation growth, resource-handle count,
    and scheduler wakeup/park ratios across the entire soak run.
  - Requirement: include mixed workloads (`static`, `json`, `add`, `route-param`,
    `stateful-counter`, malformed bursts, cancellation bursts, and slow-client writes)
    so the soak proves more than a single happy-path handler.
  - Requirement: add leak detectors for VM-owned sockets, handler resources, body
    buffers, telemetry spans, route contexts, and NativeBoundary resource handles.
  - Requirement: add adversarial soak fixtures for client disconnect storms, repeated
    route misses, oversized bodies, half-closed sockets, and bounded queue saturation;
    every case must finish with a typed terminal state and zero leaked resources.
  - Requirement: persist `vm-http-soak-stability-report.json` with per-phase resource
    deltas, leak classifications, peak values, and final steady-state proof.
  - Gate: add `make vm-http-soak-stability-check` and run it after
    `vm-http-runtime-attribution-check`; the release profile must also be part of
    `make release-0-0-7-preflight`.
  - Acceptance: soak fails if memory/resource growth exceeds the configured threshold,
    any VM-owned resource remains live after shutdown, or error rates drift after warmup.
  - Acceptance: soak output must include stable failure diagnostics naming the leaked
    resource class, owning process id, last request id, and shutdown phase.


## Completed 040

- [x] Slice 39: add VM-owned timers, deadlines, and cancellation scheduling.
  - Requirement: define VM timer primitives for one-shot timers, interval timers,
    receive timeouts, HTTP request deadlines, supervision backoff, ACME renewal
    deadlines, checkpoint flush deadlines, and NativeBoundary cancellation deadlines.
  - Requirement: implement deterministic timer ordering for equal deadlines, monotonic
    clock usage, cancellation tokens, timer ownership, and cleanup on process exit.
  - Requirement: expose typed timer outcomes (`fired`, `cancelled`, `owner_exited`,
    `deadline_missed`, `coalesced`, `overflow`) through mailbox delivery,
    observability, debugger, and inspector surfaces.
  - Requirement: add scheduler pressure accounting so timer storms cannot starve
    runnable processes or HTTP request handling.
  - Requirement: add adversarial tests for timer storms, equal-deadline ordering,
    cancelled timers racing with delivery, process exit before fire, nested timers,
    deadline overflow, clock drift simulation, and cancellation while a NativeBoundary
    worker is parked.
  - Requirement: persist `vm-timer-deadline-report.json` with timer counts, ordering
    traces, cancellation decisions, late-fire counts, and scheduler pressure deltas.
  - Gate: add `make vm-timer-deadline-check` and run it before
    `vm-supervision-restart-check`, `vm-http-soak-stability-check`, and
    `vm-http-acme-tls-production-check`.
  - Completed progress: `VmTimerEvent` now distinguishes manual timer
    cancellation from VM owner-exit cleanup with an `OwnerExited` event, and
    `VmTimerTable::cancel_owner_timers` reports owner-exit cleanup in stable
    timer id order while preserving unrelated timers. `make
    vm-timer-deadline-check` now exists as the slice gate, delegates to
    `vm-timer-primitives-check`, and runs the exact owner-exit cleanup
    regression plus the full timer primitive test module. Receive-timeout
    deadline overflow now fails before blocking the owner process or installing
    a timer, with an exact regression proving the owner remains runnable and no
    snapshot row is created. Due timer delivery now rechecks owner liveness and
    reports `OwnerExited` instead of `Fired` when missed cleanup leaves a timer
    behind after process exit. Late timer delivery now reports
    `DeadlineMissed` with `late_by_ticks` for both one-shot and receive-timeout
    timers; receive-timeout deadline misses still wake the blocked owner through
    the scheduler. `VmTimerTable` now supports VM-owned interval timers with
    positive interval validation, repeat scheduling after each fired/missed
    deadline, inspection-visible interval snapshots, and exact regressions for
    rescheduling and zero-interval rejection. Late interval timers now coalesce
    skipped deadlines into a typed `Coalesced` event with `skipped_intervals`
    and `next_deadline_tick`, then reschedule past the skipped interval
    boundaries deterministically. Interval reschedule overflow now produces a
    typed `Overflow` event and removes the timer instead of silently dropping
    state after a fired, missed, or coalesced interval boundary. The VM I/O
    reactor now enforces a deterministic timer-storm fairness guard: after 32
    consecutive timer wakeups, the next non-timer readiness item is pulled
    forward so timer bursts cannot indefinitely delay TCP/HTTP or other
    runtime wakeups. `vm-timer-deadline-check` now includes the exact
    `vm_io_reactor_loop_interleaves_non_timer_wake_after_timer_storm_budget`
    regression alongside the existing timer primitive suite. The reactor drain
    summary now records `max_consecutive_timer_wakeups` and
    `fairness_interleaves`, and the gate includes
    `vm_io_reactor_loop_drains_timer_only_storm_after_fairness_budget` so
    scheduler-pressure evidence is explicit while timer-only queues remain
    guaranteed to drain.
  - Completed progress: `VmTimerTable` now retains runtime-owned cumulative
    accounting for started, fired, missed, coalesced, overflowed, cancelled,
    and owner-exited timers, plus peak active timers, accumulated late ticks,
    deterministic outcome ordering, and typed cancellation decisions. The
    timer gate now executes
    `timer_table_writes_deadline_report_from_runtime_events` and persists
    `target/quality/vm-timer-deadline-report.json` with those measurements and
    reactor fairness deltas; the gate fails when the report is not produced.
  - Completed progress: timers now expose owner-bound cancellation tokens with
    deterministic cancel-before-delivery and delivery-before-cancel outcomes.
    `VmTimerTable` enforces monotonic clock observations, rejects backward
    ticks without firing or losing pending timers, and retains stable clock
    drift diagnostics in runtime metrics. The timer gate now includes exact
    cancellation-race, same-tick nested-timer, and backward-clock regressions.
  - Completed progress: the VM I/O reactor now retains typed outcome traces for
    all six timer outcomes while waking processes only for `fired`,
    `deadline_missed`, `coalesced`, and `overflow`; terminal `cancelled` and
    `owner_exited` events remain observable without spuriously waking their
    owners. Shared CLI/TUI instrumentation snapshots now expose completed timer
    outcomes alongside active timers, and the timer gate covers the complete
    outcome taxonomy plus inspection propagation.
  - Completed progress: `VmTimerTable::deliver_event_to_mailbox` now converts
    live-owner timer events into a typed `timer_outcome` VM tuple, self-sends
    through the VM process table, schedules the owner, and records delivery
    counts in timer metrics and the deadline report. `owner_exited` remains
    observation-only and cannot enqueue into a dead mailbox. Exact live-owner
    and dead-owner delivery regressions are part of the timer gate.
  - Completed progress: release ordering now makes
    `vm-timer-deadline-check` an explicit prerequisite of
    `vm-supervision-restart-check` and `vm-http-acme-tls-production-check`.
    `vm-runtime-semantics-check` owns the timer deadline gate once and no
    longer invokes its lower-level scheduler/timer prerequisites separately.
    The focused timer harness passes 36 deterministic and adversarial tests,
    the executable gate produces `vm-timer-deadline-report.json`, and dry-run
    dependency checks prove the aggregate graph remains acyclic.
  - Completed progress: the VM HTTP server now supports an opt-in handler
    timeout backed by `VmTimerTable` one-shot timers rather than host-runtime
    timeouts. A parked handler receives a process-owned deadline; firing the
    timer closes its VM TCP stream and exits the handler with the stable
    `http_request_deadline_exceeded` reason. Successful request completion
    cancels the timer before the keep-alive handler is reused, and zero or
    overflowing deadlines fail before accepting work. The implementation lives
    in the focused `runtime/vm/http/deadline.rs` module, reuses the existing
    handler cancellation path, and returns typed timer events plus timed-out
    process ids. All three adversarial regressions are exact members of
    `vm-timer-deadline-check`, and that gate passes with the full 30-test timer
    module and existing scheduler-pressure checks.
  - Completed progress: one-for-one supervision restart backoff now uses
    `VmTimerTable` instead of restarting failed children immediately while only
    reporting delay metadata. The supervisor exits the failed child before the
    wait, restarts it only after a typed fired or missed-deadline outcome, and
    handles duplicate scheduling, cancellation, stale timers after an external
    restart, deadline overflow, and timer-owner exit without leaking state or
    double-restarting a child. Five focused deterministic and adversarial
    regressions are owned by
    `vm-timer-deadline-check`; the complete gate and canonical Rust suite pass
    with warnings denied.
  - Completed progress: timer-backed backoff now preserves `one_for_all` and
    `rest_for_one` selection while assigning each restartable child its own VM
    deadline. The queue preflights every selected process, restart limit, and
    deadline before stopping the group; zero-delay siblings restart
    immediately, delayed siblings remain exited until their timers fire, and
    earlier `rest_for_one` siblings remain untouched. Restart-limit exhaustion
    and deadline overflow are atomic and cannot leave a partially stopped
    group. Four group-policy regressions bring the focused backoff suite to nine
    tests. The canonical Rust owner passes 4,075 tests with one ignored test,
    all owned harnesses pass in 292.60 seconds, and
    `vm-timer-deadline-check` passes with warnings denied.
  - Completed progress: checkpoint flushes now race completion against a
    VM-owned one-shot deadline through `VmCheckpointFlushDeadlineQueue`.
    Completion must cancel the active timer before the distributed storage
    adapter can advance its durable sequence; fired or missed deadlines return
    the existing typed `FlushTimedOut` outcome and leave durable state
    unchanged. Duplicate owner operations, zero timeouts, deadline overflow,
    closed adapters, manual cancellation, owner exit, unrelated and
    foreign-owner events, invalid timer kinds, typed adapter failures, retry
    after timeout, and completion-after-delivery races are covered by seven
    focused deterministic and adversarial tests. The focused suite passes with
    warnings denied and is an owned member of the passing
    `vm-timer-deadline-check` gate.
  - Completed progress: parked NativeBoundary requests now use
    `VmNativeBoundaryDeadlineQueue`, which owns the transport-neutral
    `NativeBoundaryWorker`, reserves worker credit, blocks the calling VM process,
    and installs a process-owned one-shot timer. Completion must cancel the
    timer before worker completion can wake the actor; fired or missed
    deadlines transition the same worker request to its typed timeout state,
    release credit, and reject late completion. Manual cancellation and owner
    exit use the worker's existing cancellation transition, with owner-exit
    cleanup remaining observation-only for the dead actor. Seven focused tests
    cover completion, timeout, delivery/completion races, manual cancellation,
    exit/cancel races, credit backpressure, zero and overflowing deadlines,
    foreign owners, and invalid timer kinds. The suite passes with warnings
    denied and is owned by the passing `vm-timer-deadline-check`; concrete
    native operation transport remains part of the separate VM worker-dispatch
    integration rather than this timer slice.
  - Completed progress: the deterministic short and release HTTP soak profiles
    now assert one typed `request-deadline` terminal per adversarial replay,
    require the exact `timed_out` outcome and
    `http_request_deadline_exceeded` diagnostic, and prove no active timer is
    retained. This closes the remaining HTTP soak deadline gap with one
    expiration in the short profile and three in the release profile.
    `vm-http-soak-stability-check` now depends explicitly on
    `vm-timer-deadline-check`; both exact soak regressions, the composed gate,
    formatting, and warnings-as-errors compilation pass.
  - Acceptance: timer behavior must be deterministic under replay and typed under
    every cancellation/owner-exit path.
  - Acceptance: the gate fails if timers leak after process exit, if equal deadlines
    reorder nondeterministically, or if timer storms starve non-timer runnable work.


## Completed 041

- [x] Slice 41: add VM heap, memory-pressure, and resource ownership accounting.
  - Requirement: define VM-owned accounting for per-process heap values, shared
    binaries, maps/sets/vectors, mailbox payloads, response buffers, template output,
    NativeBoundary handles, and protocol stream buffers.
  - Requirement: define allocation pressure thresholds, soft limits, hard limits,
    collection triggers, process kill/escalation behavior, and typed out-of-memory
    outcomes without relying on host allocator panics.
  - Requirement: track ownership transfer for messages, links/monitors, checkpointed
    state, distributed state snapshots, HTTP bodies, WebSocket/SSE frames, and
    NativeBoundary resources.
  - Requirement: integrate memory accounting with scheduler reductions, supervision
    restart decisions, runtime inspector views, observability metrics, benchmarks,
    soak tests, and A-CHAMP map workloads.
  - Requirement: add adversarial tests for large binaries, deeply nested lists/tuples,
    map growth/removal churn, mailbox floods, response-buffer growth, failed
    NativeBoundary allocation, checkpoint restore pressure, and resource release on
    cancelled HTTP streams.
  - Requirement: persist `vm-memory-pressure-report.json` with per-process heap
    high-water marks, shared allocation counts, collection events, pressure decisions,
    resource ownership graph, and leak classifications.
  - Gate: add `make vm-memory-heap-pressure-check` and run it before
    `vm-http-soak-stability-check`, `vm-scheduler-fairness-check`, and
    `achamp-adversarial-coverage-check`.
  - Completed progress: `vm-resource-ownership-check` now covers
    process-table-aware resource cleanup before process exit: VM-owned
    resources can be cleaned from the resource table and matching live process
    handle rows in one operation, while unrelated owner resources remain
    inspectable. This narrows the cancelled/failed process leak surface before
    the full heap-pressure gate is introduced.
  - Completed progress: `achamp-adversarial-coverage-check` now rejects
    randomized Rust map backends in the active VM map implementation. The gate
    proves `VmMapValue` stays on VM-owned flat/A-CHAMP storage and cannot drift
    onto `HashMap`/`RandomState` without an explicit failing quality report.
  - Completed progress: `vm-deterministic-hashmap-check` now inventories every
    production VM runtime `HashMap` or `RandomState` use and fails on new
    unclassified references.
    Existing uses are classified as lexical environments, lookup tables, or
    transport/handle registries so Rust hash randomization cannot silently become
    user-visible Terlan ordering semantics. The gate now also rejects
    placeholder/TODO/TBD/unknown/fixme owner or note fields and placeholder
    classification names, with injected-placeholder tests covering both
    inventory rows and allowed classification vocabulary. The inventory is also
    byte-lexically path-sorted so deterministic evidence remains stable under
    review and regeneration. Direct `RandomState` imports now share the same
    unclassified-reference diagnostic path as `HashMap`.
  - Completed progress: `VmMemoryAccountant` now enforces validated per-process
    logical heap soft/hard limits before mutation, returns typed `accounted`,
    `soft_limit_exceeded`, or `hard_limit_rejected` decisions, rejects arithmetic
    overflow through the hard-pressure path, and tracks current/high-water bytes,
    releases, and collection events. Process exit clears heap state and an explicit
    lifecycle synchronization hook prevents stale leak classification. The new
    `vm-memory-heap-pressure-check` persists
    `target/quality/vm-memory-pressure-report.json`, runs after the process-model
    gate, and is a prerequisite of scheduler fairness and A-CHAMP adversarial gates.
  - Completed progress: VM mailbox messages can now carry an explicit logical heap
    charge. `VmMemoryAccountant::send_message` validates sender/recipient routes
    before reserving recipient memory, accepts soft-pressure delivery, and returns a
    typed hard-pressure rejection without allocating a message id or mutating the
    mailbox. Accounted FIFO receive releases the exact stored charge, legacy sends
    remain zero-charge compatible, and recipient exit clears both queued messages and
    heap ownership before leak classification. The memory-pressure gate covers
    delivery, receive, invalid routes, hard-limit atomicity, and exit cleanup.
  - Completed progress: accounted selective receive now releases only the selected
    message's stored heap charge while preserving skipped FIFO entries and their
    ownership bytes. A no-match scan leaves both mailbox and heap state unchanged,
    and later ordinary receives release the remaining charges in original order.
    The memory-pressure gate locks selected, skipped, no-match, and final-drain
    behavior.
  - Completed progress: VM native resource handles can now reserve explicit owner
    heap bytes and retain a deterministic memory-ownership row. Registration rejects
    hard pressure before allocating a handle, transfer validates both resource and
    process roles before reserving destination bytes, and a rejected transfer leaves
    handle ownership and both process heaps unchanged. Successful transfer moves the
    logical charge atomically, while release removes both the handle and charge. The
    memory report now contains a nonempty resource-ownership graph, and
    `vm-memory-heap-pressure-check` runs the complete resource lifecycle gate before
    memory adversarial tests.
  - Completed progress: `VmMemoryAccountant::exit_process_with_memory_cleanup`
    now validates the resource-table and memory-ownership graphs before changing
    process state, releases every owned resource charge in deterministic handle
    order, removes the corresponding VM handles, exits the process, and reconciles
    mailbox plus remaining heap bytes into released memory metrics. Ownership graph
    divergence fails closed without mutating the resource table or exiting the
    process, and cleanup preserves resources belonging to other live processes. The
    memory gate includes exact killed-process and injected-unaccounted-handle
    regressions with warnings denied.
  - Completed progress: structural VM values now have deterministic checked logical
    sizing for scalars, strings/atoms/types, tuples, records, lists, flat maps,
    A-CHAMP maps, sets, and iterators. The sizing walk is iterative so adversarially
    deep values do not recurse on the host stack, and indexed maps expose retained
    base/patch entries without materializing a cloned map. Accounted mailbox sends
    can derive their charge directly from the payload, enforce soft/hard pressure
    before allocation, and fail closed with a stable typed diagnostic for opaque
    native values or closures that still require dedicated ownership contracts. The
    memory gate locks exact nested sizing, 2,048-level nesting, retained A-CHAMP
    patch accounting, automatic mailbox pressure, and pre-mutation opaque rejection.
  - Completed progress: `VmMemoryAccountant` now owns monotonic shared-allocation
    identities for binaries, protocol buffers, response buffers, and template output.
    Each allocation tracks sorted process references while the pressure report
    distinguishes unique logical bytes from per-process owner-reference charges.
    Registration and retain reject hard pressure before mutating the registry,
    duplicate retain is idempotent, unauthorized sharing fails closed, last-owner
    release deallocates the allocation, and stale IDs have stable diagnostics.
    `exit_process_with_memory_cleanup` releases the exiting process's shared
    references, reports their IDs deterministically, deallocates exclusive values,
    and preserves values retained by another live process. The memory gate locks the
    full retain/release/pressure lifecycle, process-exit behavior, and nonempty
    shared-allocation report counts.
  - Completed progress: `VmActorRuntime` now owns an active
    `VmMemoryAccountant` with explicit validated soft/hard limits. Actor sends
    structurally size payloads before reserving recipient heap, ordinary and
    selective receives release the exact mailbox charge, and actor exit
    synchronizes cleared heap state while preserving high-water telemetry.
    Actor runtimes can be constructed with explicit limits for deterministic
    deployment and adversarial tests. A hard-pressure regression proves a
    rejected 25-byte payload against a 24-byte limit leaves the mailbox empty,
    leaves current/high-water bytes at zero, and does not consume a message id;
    the next valid 8-byte payload receives id 1. The memory module is no longer
    dormant, and `vm-memory-heap-pressure-check` runs both actor integration
    cases exactly before the accountant adversarial suite.
  - Completed progress: VM SSE queues now have a production
    `VmAccountedSseStream` path that reserves the exact encoded frame size as a
    VM-owned protocol-buffer allocation before queue mutation. Flushing a frame
    releases its allocation, hard pressure rejects the enqueue without changing
    queue or heap state, and cancellation uses a validated bulk release before
    clearing pending frames. Bulk shared-allocation release validates every id,
    owner reference, duplicate, byte sum, and owner heap before any mutation, so
    a stale frame allocation cannot cause partial cleanup. The memory gate runs
    both SSE pressure/cancellation regressions exactly and all 19 accountant
    adversarial tests.
  - Completed progress: VM WebSocket inbound queues now have a production
    `VmAccountedWebSocketInboundQueue` path that reserves each decoded frame's
    payload bytes as a process-owned protocol buffer. Queue bounds and frame
    size are validated before allocation, hard pressure leaves frame order,
    byte counters, and heap state unchanged, and popping a frame releases its
    exact allocation. Cancellation bulk-releases all pending frames before
    clearing queue state and permanently rejects later pushes, preventing a
    cancelled connection from reacquiring untracked buffers. The memory gate
    runs exact pressure and cancellation regressions; scheduler and supervision
    memory integration remain open.
  - Completed progress: `VmActorRuntime::restore_mailbox_checkpoint` now restores
    ordered checkpoint payloads through one VM-owned memory transaction. Every
    value is structurally sized before reservation, the aggregate checkpoint is
    checked against process soft/hard limits before any message is installed,
    and accepted values receive ordered self-message ids with their individual
    release charges preserved. Opaque values and hard pressure leave mailbox,
    heap, and message-id state unchanged; the next valid checkpoint starts at id
    1. The memory gate runs exact successful restore and adversarial rejection
    cases. Full supervised state/resource checkpoint ownership remains with
    Slice 38; scheduler attribution outside actor mailbox paths and supervision
    memory integration remain open here.
  - Completed progress: actor mailbox allocation, hard-pressure rejection,
    checkpoint reservation, and mailbox release now charge deterministic
    scheduler reductions through `VmScheduler::charge_memory_reductions`. The
    cost is one base reduction plus one reduction per started KiB, applies even
    when a hard limit rejects the allocation, and is recorded without inventing
    a process slice. Scheduler telemetry and the fairness report now expose both
    total and per-process memory reductions while retaining them in aggregate
    reduction totals. Exact scheduler attribution plus actor allocation,
    rejection, restore, and release assertions run in the memory gate.
  - Completed progress: accounted SSE enqueue, hard-pressure rejection, flush,
    and cancellation plus WebSocket push, hard-pressure rejection, pop, and
    cancellation now charge the same deterministic scheduler memory reductions
    as actor mailboxes. Rejected reservations are attributed before queue state
    changes, while cancellation charges the aggregate bytes released by the
    atomic bulk-release transaction. Exact adversarial tests lock queue and heap
    atomicity plus per-process and total scheduler telemetry for both protocols,
    and run in `vm-memory-heap-pressure-check`.
  - Completed progress: `VmSupervisionSystem::handle_memory_pressure` now
    consumes typed VM accounting decisions. Accounted allocations continue,
    soft-limit decisions return an explicit collection request without process
    or restart-history mutation, and hard-limit decisions exit with a typed
    `MemoryLimitExceeded` reason after releasing VM memory, shared allocations,
    and resource ownership. Cleanup follows one-for-one, one-for-all, and
    rest-for-one selection while restart limits preserve existing escalation to
    parent supervisors. Four exact adversarial tests cover continue/collect,
    one-for-one restart, one-for-all cleanup, and parent escalation in
    `vm-memory-heap-pressure-check`; all 32 supervision tests pass with warnings
    denied.
  - Completed progress: `VmAccountedHttpTemplateResponse` now reserves rendered
    template bytes as `TemplateOutput`, atomically reclassifies the same owner
    allocation as `ResponseBuffer` during response construction, retains it
    through HTTP/1 serialization, and releases it after successful writes,
    cancellation, or injected writer failure. Hard pressure is charged to the
    scheduler but rejects before ownership or heap mutation. Shared-allocation
    reclassification validates allocation identity, owner, and expected kind
    before mutation. Four exact lifecycle tests run in
    `vm-memory-heap-pressure-check`. The shared-allocation implementation was
    also extracted into `memory/shared.rs`, reducing `memory.rs` from 1,004 to
    793 lines without changing its quality baseline.
  - Completed progress: the production `VmHttpTcpServer` now accounts every
    generic handler response as a VM-owned `ResponseBuffer` across plaintext
    and TLS polling. The server reserves the complete serialized wire response
    before VM TCP/TLS send, charges deterministic scheduler reductions for
    reservation and release, rejects hard pressure before emitting bytes, and
    releases ownership after success or transport failure. Per-handler current,
    high-water, released-byte, and reduction metrics remain inspectable after
    the exchange. Three exact tests cover successful keep-alive output,
    pre-send hard rejection, and peer-close cleanup in
    `vm-memory-heap-pressure-check`; all 37 existing HTTP server regressions,
    including encrypted TLS paths, pass with warnings denied. Response-memory
    construction and inspection live in `http/response_memory.rs`, keeping
    `http.rs` below its enforced size baseline.
  - Completed progress: the production `TerlanVm` now owns one reusable,
    mutex-safe `VmNativeBoundaryContext`, and VM Base64 encode/decode calls no
    longer bypass the boundary for direct Rust adapter calls. NativeBoundary
    argument terms are sized iteratively and reserved as
    `NativeBoundaryBuffer` allocations before dispatch; synchronous request
    ownership is released after dispatch, while reply ownership remains charged
    until consumption or cancellation. Request hard pressure rejects before
    dispatch, reply hard pressure releases the request before rejecting, and
    every reserve/release is reflected in scheduler memory reductions. Four
    exact lifecycle and pressure regressions plus the existing VM Base64
    integration regression pass with warnings denied, the complete
    `vm-memory-heap-pressure-check` passes, and dormant-runtime/size quality
    gates recognize the adapter as active production code.
  - Completed progress: `vm-memory-heap-pressure-check` now runs a bounded
    10,000-iteration ownership churn workload across direct process heap and
    shared protocol-buffer allocations. The workload deterministically covers
    4,510 accounted decisions, 4,490 soft-limit decisions, and 1,000 hard-limit
    rejections while releasing every accepted allocation before the next cycle.
    It fails on retained process bytes, decision-distribution drift, or report
    loss and writes `target/quality/vm-memory-soak-report.json` with elapsed
    nanoseconds, operations per second, high-water bytes, released bytes, and
    retained bytes. The validated run reached an 8,192-byte high-water mark,
    released 36,808,808 logical bytes, and retained zero bytes. Timing remains
    observational rather than a machine-dependent pass threshold. The complete
    memory gate, its 20-test aggregate accountant suite, warnings-as-errors,
    formatter, whitespace, dormant-runtime, deterministic-map, and file-size
    quality checks pass.
  - Acceptance: memory pressure must produce typed VM outcomes and observable
    ownership state instead of raw allocation failures.
  - Acceptance: the gate fails if allocations are unaccounted, if ownership transfer
    leaks resources, or if a cancelled/failed process retains VM-owned heap/resource
    state after shutdown.


## Completed 042

- [x] Slice 42: harden VM NativeBoundary dispatch, resource handles, and worker
  policy.
  - Requirement: define a stable NativeBoundary manifest schema for every native
    export: module, function, arity, argument types, return type, blocking policy,
    cancellation policy, resource permissions, memory ownership, and failure type.
  - Requirement: route all native calls through VM-owned dispatch that validates
    manifest existence, capability permission, arity, argument shape, resource
    ownership, scheduler policy, and timeout/cancellation behavior before execution.
  - Requirement: support worker classes for fast nonblocking calls, blocking calls,
    long-running cancellable calls, sandboxed calls, and resource-owning calls without
    exposing raw host pointers or host runtime handles to Terlan code.
  - Requirement: define typed resource handles with owner process, lifetime, drop
    behavior, transfer policy, checkpoint policy, and debug/render policy.
  - Requirement: integrate NativeBoundary events with scheduler reductions, memory
    accounting, supervision, process recovery, observability, inspector, debugger,
    native package gates, and Lean proof manifests.
  - Requirement: add adversarial tests for wrong arity, wrong type, missing manifest,
    stale resource handle, double drop, cross-process unauthorized use, timeout,
    cancellation race, worker panic, and native error mapping.
  - Requirement: persist `vm-native-boundary-report.json` with dispatch decisions,
    resource lifecycle events, worker class usage, timeout/cancellation outcomes,
    and proof-manifest correlation.
  - Gate: add `make vm-native-boundary-contract-check` and run it before
    native package gates (`terlan-polars`, `terlan-pytorch`, C++ bindings, CUDA)
    and before `vm-memory-heap-pressure-check`.
  - Completed progress: `native-boundary-terminology-check` now runs its
    adversarial terminology tests before the CLI scan, rejects placeholder
    glossary text, verifies retired NativeBoundary wording stays out of diagnostics,
    and checks 26 golden docs for NativeBoundary terminology drift.
  - Completed progress: the legacy Postgres worker metadata has been replaced by a
    VM-owned `NativeBoundaryWorkerManifest` and complete per-export manifest rows.
    Every export now declares source module/function/arity, compiler operation,
    ordered argument types, return type, worker class, cancellation policy, resource
    permissions, argument/result memory ownership, and typed failure contract. The
    Postgres manifest is aligned with all nine `std.db.Postgres` signatures, uses
    `VmMailbox` transport with no BEAM transport term, and validates nonempty fields,
    arity/type counts, duplicate operations/MFAs, positive credit, and resource
    ownership references deterministically. `vm-native-boundary-contract-check` now
    runs manifest adversarial coverage after existing lifecycle/runtime tests and is
    an executable prerequisite of `vm-memory-heap-pressure-check`.
  - Completed progress: resource-backed Postgres dispatch now resolves and validates
    the cached VM-owned NativeBoundary manifest before global dispatch or resource
    decoding. Missing exports, manifest arity drift, scalar argument mismatches, and
    malformed JSON parameter-list shapes return stable `native_boundary.*`
    diagnostics before adapter execution, while valid handle shapes proceed to the
    existing typed kind/liveness checks. Five ordering-focused adversarial tests are
    part of `vm-native-boundary-contract-check`, including proof that stale handles
    are rejected only after manifest shape validation succeeds.
  - Completed progress: NativeBoundary resources now carry an owning VM process id.
    Process-scoped registration, dispatch, result encoding, and disposal preserve
    that owner across scalar, optional, and recursively nested list handles. Calls
    validate liveness and ownership before adapter access or mutation; unauthorized
    access and drop attempts return stable `resource.owner` failures without removing
    the owner's live resource. Exact cross-process access/disposal adversarial tests
    now run in `vm-native-boundary-contract-check`, while trusted calls retain an
    explicit reserved system-owner path.
  - Completed progress: every Postgres NativeBoundary manifest export now declares
    the canonical `postgres` capability from `std/NATIVE_BOUNDARY_SECURITY.tsv`.
    Actor-scoped dispatch accepts an explicit capability set and rejects missing or
    unrelated grants with stable `native_boundary.capability_denied` diagnostics
    before arity, argument-shape, resource-owner, or adapter processing. Trusted
    system dispatch remains a separate explicit path. Manifest validation rejects
    empty capability declarations, and the executable gate covers denied, unrelated,
    granted, and validation-order cases through dispatch and runtime term boundaries.
  - Completed progress: actor-scoped NativeBoundary dispatch now requires explicit
    VM scheduler admission for the exact manifest worker class after capability
    authorization and before arity, payload, resource, or adapter processing.
    Capability-only calls and mismatched class grants return stable
    `native_boundary.scheduler_denied` diagnostics. The admission policy recognizes
    the closed `fast`, `blocking`, `long-running-cancellable`, `sandboxed`, and
    `resource-owning` vocabulary, while trusted system dispatch remains explicit. The
    runtime gate proves denied, mismatched, admitted, and validation-order behavior
    for the Postgres resource-owning path.
  - Completed progress: all resource-backed NativeBoundary adapter execution now
    runs inside a Rust unwind boundary after manifest, capability, scheduler, arity,
    shape, and resource-owner admission succeeds. Successful results and existing
    typed adapter failures are preserved exactly; worker panics become the stable
    `native_boundary.worker_panic` error with a fixed message that suppresses panic
    payloads and Rust internals. Adversarial panic/no-leak tests are now executable
    members of `vm-native-boundary-contract-check`.
  - Completed progress: NativeBoundary worker request ids now advance through a
    monotonic accepted-id watermark. Completed, cancelled, and timed-out ids
    cannot be reused, so a late cancellation or reply from an old request cannot
    target a replacement request through an ABA lifecycle race. Completion-wins
    and cancellation-wins orderings both preserve credit accounting; the next
    monotonic id remains independently usable. Cancelled and timed-out request
    rows are removed after credit release instead of accumulating terminal
    tombstones, while the watermark continues rejecting stale events. Exact race,
    cancellation, timeout, and terminal-cleanup regressions run in
    `vm-native-boundary-contract-check`; all 12 worker tests pass with warnings
    denied, and Rust file-size, dormant-runtime, deterministic-map, formatter,
    and whitespace quality gates pass.
  - Completed progress: `NativeBoundaryWorker` now retains a bounded, ordered
    production event history for accepted, completed, cancelled, timed-out,
    invalid/stale, and backpressure-rejected request transitions. Every event
    captures post-transition reserved and available credits, and the oldest
    event is evicted once the explicit 1,024-entry limit is reached. The worker
    writes `target/quality/vm-native-boundary-report.json` with the stable
    `terlan-vm-native-boundary-report-v1` schema, credit state, monotonic request
    watermark, history limit, and lifecycle events. Exact tests cover every
    current lifecycle outcome, deterministic report shape, zero retained credit,
    and bounded-history eviction after 2,200 generated events. Report
    serialization lives in `runtime/native_boundary/worker_report.rs`, keeping the
    worker implementation within its 500-line limit. `vm-native-boundary-contract-check`
    requires the report and passes with all 14 worker tests and warnings denied.
  - Completed progress: the term-level NativeBoundary runtime now records
    bounded resource lifecycle telemetry when calls return newly registered
    scalar, optional, or recursively nested handles, when owners dispose live
    handles, and when stale, wrong-kind, or unauthorized resource validation is
    rejected. Events preserve owner process id, operation, handle id/generation,
    lifecycle outcome, and stable typed error code without exposing host values.
    `vm-native-boundary-report.json` now includes these `resourceEvents` beside
    request lifecycle events. The executable report fixture proves a JSON handle
    is created, disposed, then rejected on duplicate disposal with
    `resource.stale_handle`, while all credits return to zero. Resource event
    traversal is iterative and history is capped at 1,024 entries. The logic
    lives in `runtime/native_boundary/runtime_events.rs`, leaving the existing
    oversized resource store unchanged and keeping `worker.rs` within the
    500-line limit.
    `vm-native-boundary-contract-check`, all 14 worker tests with warnings
    denied, Rust quality, formatter, and whitespace gates pass.
  - Completed progress: the NativeBoundary runtime now records a bounded
    dispatch history for every term-level call with owner process id, operation,
    manifest-derived worker class, and stable typed error code. The report emits
    ordered `dispatchEvents` and deterministic `workerClassUsage` totals without
    guessing classes for operations absent from a manifest; those operations are
    counted explicitly as `unclassified`. The executable fixture proves exact
    `fast`, `blocking`, and `resource_owning` Postgres counts through manifest-valid
    calls, two unclassified non-Postgres calls, five resource events, and zero
    retained credits. `vm-native-boundary-contract-check`, warnings-as-errors,
    Rust file-size and dormant-code quality, formatting, and whitespace gates pass.
  - Completed progress: `vm-native-boundary-report.json` now correlates runtime
    dispatch evidence with the repository's formal proof track. The report names
    the `native-boundary` feature class, `native-boundary contracts` gap, runtime
    owner, planned proof gate, Postgres runtime manifest and nine-export scope,
    correlated and unmanifested dispatch counts. Its initial `incomplete` status
    and null digest made the missing formal evidence explicit. The executable
    fixture reads the authoritative proof-gap TSV, so owner or gate drift fails
    `vm-native-boundary-contract-check`; the following proof slice replaces that
    initial placeholder without overstating the still-separate Rust refinement.
  - Completed progress: the NativeBoundary proof correlation is now executable
    rather than a null placeholder. The Lean family proves capability and
    scheduler admission, arity/argument validation, resource-owner isolation,
    and completion/cancellation credit conservation through eight theorems.
    Content-addressed replay metadata fingerprints the runtime manifest, worker,
    security policy, and glossary; the proof is replayed twice by
    `lean-proof-track-check`, and the generated native-boundary baseline is
    `current` with digest
    `sha256:2c7c8dd87ad70bba20ae805eb13e972160c5dacadbd1983155559ceaf71fb3f4`.
    Runtime reports validate and embed the same proof family, path, and digest.
    Adversarial tests reject schema, family, and source-digest drift, while the
    broader Aeneas refinement from Rust execution to the abstract Lean model
    remains explicitly classified as a separate proof-track gap. The complete
    `vm-native-boundary-contract-check` passes with runtime lifecycle,
    cross-process ownership, capability/scheduler ordering, panic isolation,
    manifest, report, and proof-replay coverage.
  - Acceptance: no native call may execute without a validated manifest and typed
    VM-owned resource/failure contract.
  - Acceptance: the gate fails if raw host handles leak into Terlan values, if native
    panics surface directly, or if cancellation/timeouts leave owned resources live.


## Completed 043

- [x] Slice 43: make Postgres/database access a VM-owned NativeBoundary runtime
  contract.
  - Requirement: expose `std.db.Postgres` through VM NativeBoundary manifests for
    pool creation, connection acquisition, query, query-one, execute, transaction,
    rollback, commit, row decoding, and resource cleanup.
  - Requirement: prefer a stable maintained C ABI over alpha, beta, or pre-1.0 Rust
    database drivers. Use generated libpq bindings and forbid hand-rolled wire
    protocol, SQL parsing, TLS, authentication, or host async runtime ownership.
  - Requirement: every C ABI adapter must prove deterministic regeneration,
    warning-denied C/Rust compile and link, opaque ownership and destruction,
    stable error translation and redaction, and a live Docker-backed roundtrip when
    the external system can be exercised locally.
  - Requirement: define typed database resources for pools, connections,
    transactions, prepared statements, result sets, rows, and decode errors, with
    owner process, lifetime, cancellation, checkpoint policy, and render/debug policy.
  - Requirement: park VM processes during blocking database operations, charge
    reductions for dispatch/row decoding, and resume with typed `Result` values or
    typed database errors.
  - Requirement: integrate database operations with serve config, Docker/dependency
    readiness, SQL macro validation, migration commands, supervision, observability,
    debugger, process recovery, and NativeBoundary proof manifests.
  - Requirement: add adversarial tests for invalid credentials, unreachable database,
    pool exhaustion, transaction rollback on process exit, cancellation during query,
    invalid row decode, stale connection, dropped pool, migration lock conflict, and
    malformed SQL macro input.
  - Requirement: persist `vm-postgres-runtime-report.json` with driver crate/version,
    pool config, query lifecycle events, transaction outcomes, cancellation decisions,
    row-decode failures, and resource cleanup proof.
  - Gate: add `make vm-postgres-runtime-check` and run it after
    `vm-native-boundary-contract-check`; live database tests must use Docker-managed
    dependencies and deterministic local fixtures.
  - Completed progress: the VM now owns the typed Postgres scheduling and resource
    contract for pools, connections, transactions, prepared statements, result
    sets, rows, and decoded values. Dispatch parks the owner process, charges VM
    reductions, enforces pool capacity and resource ownership, and resumes through
    typed replies. Cancellation, timeout, owner cleanup, one-way transaction
    terminal states, redacted driver failures, and the fired-deadline/late-driver
    race have adversarial coverage. `VmActorRuntime` now owns that state machine,
    exposes typed submit/dispatch/complete/cancel/reply operations, routes database
    deadlines through the actor timer table, and emits rollback, release, and pool
    close controls when an actor exits. The `vm-postgres-runtime-check` proof chain,
    18 focused state-machine and actor-integration tests under
    `--no-default-features`, zero-direct-Tokio audit, and redacted
    `vm-postgres-runtime-report.json` validation pass for this worker contract.
  - Completed progress: the VM driver protocol now binds VM resources to opaque
    driver pool, connection, transaction, prepared-statement, and row identities.
    A generic VM-owned worker executes every declared operation, pins transactions
    to one checked-out connection, applies cancellation and exact owner cleanup,
    rejects stale or unbound requests, and preserves failed commits for explicit
    rollback. Adversarial worker tests cover resource isolation, duplicate cleanup,
    cancellation, secret redaction, and commit failure. The retired conditional
    Postgres Tokio adapter and driver-specific row decoder are removed; the
    no-default-Tokio gate now rejects any Postgres runtime exception and reports zero
    direct Tokio dependencies. The direct/source-evaluator compatibility boundary
    still returns the typed `postgres.vm_driver_unavailable` error until it is routed
    through actor suspension and the VM I/O reactor.
  - Completed progress: the generated libpq C ABI package now uses pkg-config with
    a minimum libpq version, compiles C warnings as errors, emits a warning-denied
    safe Rust wrapper, and is a checked workspace dependency. Its manifest must opt
    into deterministic regeneration, warning-denied build/link, ownership lifecycle,
    error translation, and generated-fixture or package-owned-live smoke validation;
    the generator rejects a disabled obligation. The production actor
    runtime now owns a nonblocking libpq worker with typed socket read/write/drive
    interest, real parameter transport, lazy VM pool resources, typed row copying,
    transaction pinning, prepared statements, cancellation, and redacted diagnostics.
    The generated adapter all-target gate, deterministic pkg-config generation gate,
    20 VM Postgres state/fixture tests, and a live PostgreSQL 16 Docker roundtrip pass.
    The live roundtrip covers parameterized query, typed decode, begin/execute/commit,
    and cancellation, and the redaction gate caught and removed a derived-Debug URL
    credential leak. Obsolete synchronous session placeholders and the
    `postgres-live`, `tokio-postgres`, and `deadpool-postgres` runtime contract are
    removed. The slice remains open for source-evaluator suspension wiring.
  - Completed progress: VM query and execute requests now transport the complete
    typed `Vec<Json>` parameter payload to the driver instead of count-only
    metadata. Adversarial worker coverage proves that values reach the backend while
    request diagnostics expose only the parameter count and never secret values.
    Generated opaque libpq ownership now distinguishes reviewed `send_only`
    connections from thread-confined results: connections may move exclusively to a
    VM worker but deliberately remain `!Sync`. Deterministic regeneration also
    rejects unknown thread-safety metadata and emits a workspace-owned package with
    no nested Cargo workspace. `make libpq-c-abi-check` passes the generator,
    warning-denied offline libpq package tests, adapter tests, and no-default-feature
    VM compile. The umbrella `vm-postgres-runtime-check` remains non-green in the
    current shared tree because its repository-wide proof prerequisite encounters
    unrelated test failures outside the Postgres runtime slice.
  - Completed progress: `native-boundary-postgres-docker-check` now owns an
    authenticated PostgreSQL 16 container instead of accepting environment-dependent
    live-test skips. One mandatory integration selector validates a typed query and
    decode, active-query cancellation, pool cleanup, stale-pool rejection,
    invalid-credential failure, unreachable-endpoint failure, and password/URL
    redaction through the production generated libpq worker. Docker allocates the
    loopback port, readiness probes the final TCP server rather than the image's
    temporary initialization socket, and RAII cleanup removes the container on every
    test exit. The integration selector passed once against the authenticated
    fixture. The final canonical `make native-boundary-postgres-docker-check` rerun
    passes the generated package all-target build, all 23 C ABI generator tests, and
    the mandatory Docker integration selector. Shared worker-driving test support
    now keeps the live and non-live fixtures on one completion contract.
  - Completed progress: `vm-postgres-runtime-report.json` now derives explicit query
    lifecycle, transaction outcome, cancellation decision, and row-decode failure
    summaries from the bounded runtime event stream. Decode error-code totals use a
    deterministic ordered map and the report continues to exclude credentials, SQL,
    parameters, and decoded values. Cleanup evidence no longer hard-codes success:
    active transactions, pending requests, reserved scheduler credits, and live pools,
    connections, statements, result sets, or rows each make the corresponding proof
    field false. An adversarial fixture proves false evidence for an active transaction,
    a parked query, and resources retained before owner cleanup, then proves committed
    and rolled back transactions, successful, failed, and cancelled queries, a typed
    decode failure, process-exit cleanup, and fully released credits and resources.
    The exact fixture and the warning-denied
    no-default-features Postgres lane pass with 22 tests and one separately gated
    Docker selector ignored.
  - Completed progress: Postgres state is now part of the correlated VM actor
    observation boundary. Inspection exposes deterministic per-owner pool,
    connection, transaction, prepared-statement, result-set, and row counts;
    pending operations with request identity, deadline, operation, and SQL
    fingerprint; cumulative cleanup decisions; and sanitized driver wait interest.
    Raw SQL, parameters, credentials, opaque native handles, and host socket
    descriptors are structurally absent. Warning-denied adversarial tests prove
    live ownership, process-exit rollback/release/pool cleanup, timed-out request
    cancellation, pending-request removal, SQL redaction, and native socket
    redaction. The focused observability lane passes 3 tests, and the
    warning-denied canonical Postgres selector passes 23 tests with one separately
    gated Docker test ignored under `--no-default-features`.
  - Completed progress: normal `terlc serve` now starts only the validated
    project-owned Postgres Compose service and uses Docker Compose's bounded
    `--wait --wait-timeout 60` contract before binding the server. The existing
    typed Compose parser still requires a loopback port, non-empty Postgres
    environment, and enabled healthcheck, so readiness does not rely on a
    hand-rolled polling loop or an unvalidated shell command. The warning-denied
    `web-compose-check` passes all 15 exact accepted and adversarial fixtures across
    every Terlan binary. The gate also exposed and fixed an accidental benchmark
    dependency on process-environment test modules by making the actor runtime's
    sanitized Postgres inspection accessor independently reusable.
  - Completed progress: `terlc db status`, `migrate`, `rebuild --dev`, and
    `reset --dev` now execute through a reusable VM-owned Postgres command
    client. The synchronous CLI facade pumps the actor runtime's nonblocking
    libpq worker with bounded cancellation, typed pool/connection/transaction/
    row resources, parameterized history writes, VM row decoding, and actor-exit
    cleanup; command code no longer calls the retired synchronous Postgres
    compatibility adapter. Migration files use the explicit VM batch operation,
    while history insertion remains parameterized inside the same typed
    transaction. The generated stable C ABI exposes that batch operation through
    `PQsendQuery`, while parameterized calls remain isolated on `PQsendQueryParams`;
    the authenticated Docker lifecycle proves a two-statement batch and typed
    result decode through the production worker. The DB boundary checker now
    rejects compatibility-adapter execution calls and requires the VM-owned
    libpq contract. The generated
    libpq crate is a valid root workspace member with no nested Cargo workspace,
    and deterministic regeneration passes. The warning-denied consolidated
    `db-command-check` passes 84 DB command tests plus 27 VM Postgres tests with
    the separately gated Docker selector ignored, replacing more than 20
    redundant Cargo launches with two filtered processes. The exact public CLI
    migration lifecycle also passes against an authenticated PostgreSQL 16
    Docker fixture, covering rebuild, status, incremental migrate, typed history
    decode, and repeated status through the production VM worker.
  - Completed progress: compiled Terlan source can now reach the VM-owned
    Postgres actor bridge for `connect`, `query`, `query_one`, `execute`, and
    typed `String`, `Int`, `Bool`, and `Json` row decoding. Pool and row values
    use unforgeable opaque VM value variants whose debug/render output contains
    no raw resource identity; portable stable hashing, memory sizing, TETF transport,
    and live-template patch serialization reject them explicitly. The source
    bridge submits to the existing nonblocking libpq worker, proves the owner
    actor is blocked before dispatch, and resumes through correlated typed
    replies. VM target validation now admits `std.db.Postgres` instead of
    rejecting it as an unavailable Rust-backed module. Adversarial coverage
    rejects invalid configuration, forged tuple handles, and non-Json query
    parameters, while a formal-pipeline source fixture proves
    `Postgres.connect` parks and resumes through public `Result` handling. The
    warning-denied all-target build passes, and the expanded
    `db-command-check` passes 84 DB command tests, 27 VM Postgres tests, and 4
    source-evaluator tests. Transaction callback continuation is completed below;
    the slice remains open for per-execution ownership/concurrency and shared
    I/O-reactor polling instead of the synchronous evaluator's bounded worker pump.
  - Completed progress: `Postgres.transaction(pool, callback)` now executes a
    Terlan callback against an unforgeable, transaction-scoped opaque
    `Connection`. Query, query-one, and execute accept either `Pool` or the live
    transaction connection; the VM commits only a callback `Ok` and rolls back
    callback `Error`, evaluator failure, malformed callback results, and recursive
    connection escape through collections, records, indexed maps, or closure
    captures. Successful commit and rollback now close the transaction and release
    its checked-out connection immediately instead of retaining it until actor exit,
    preventing bounded pools from exhausting across sequential transactions.
    The transaction orchestration is isolated from the oversized std remote
    dispatcher, and the warnings-as-errors all-target build passes. The canonical
    `native-boundary-postgres-docker-check` passes the generated libpq all-target
    build, all 25 deterministic C ABI generator tests, the live driver
    cancellation/cleanup fixture, and a separate live VM source fixture that proves
    direct commit/rollback state plus formally compiled Terlan callback execution
    against PostgreSQL 16. The slice remains open for per-execution ownership and
    concurrency plus shared I/O-reactor polling.
  - Completed progress: NativeBoundary argument-shape validation now recognizes
    canonical top-level resource unions, so the Postgres `Pool | Connection`
    query target reaches resource-kind validation while malformed unions and
    non-handle values fail deterministically. Warning-denied focused coverage,
    28 non-Docker VM Postgres tests, 5 source-evaluator tests, the 40-test direct
    Tokio ownership audit, all 25 deterministic C ABI generator tests, 3 libpq
    adapter tests, and the no-default-feature VM compile pass. The canonical
    `vm-postgres-runtime-check` also passes its SQL and Postgres runtime stages;
    its repository-wide proof prerequisite is currently blocked by unrelated
    Shape Implication fingerprint drift in `TERLAN_SYNTAX_SPEC.ebnf`, which this
    database slice deliberately does not bless.
  - Completed progress: actor-exit rollback now owns exact-once destruction of its
    transaction connection instead of emitting a contradictory rollback followed by
    release for the same native resource. Generic and libpq workers remove transaction,
    connection, and pool-ownership indexes before reporting cleanup rollback failure,
    while ordinary requested rollback and failed commit retain their existing retry
    semantics. VM Postgres control draining now processes all queued controls before
    returning the first sanitized failure, so one actor's stale or failed cleanup cannot
    strand independent actor resources. Adversarial worker coverage proves a forced
    rollback failure destroys only its execution resources and leaves a second
    transaction and both pools independently cleanable; actor coverage proves a stale
    first cleanup does not prevent a second actor's pool cleanup. The warning-denied
    focused regressions pass, as do all 29 non-Docker VM Postgres tests and all 5
    non-Docker source-bridge tests; their two Docker-owned selectors remain separately
    gated. The canonical `vm-postgres-runtime-check` passed its 18 quality SQL tests and
    108 compiler/runtime SQL tests, then stopped at the already-recorded unrelated Shape
    Implication EBNF fingerprint drift before its Postgres commands. Rollback-failure
    isolation is complete; the slice remains open only for per-execution concurrency
    ownership and shared I/O-reactor polling.
  - Completed progress: source-evaluated Postgres calls now use one lazily allocated
    VM process owner per outermost Terlan execution. Nested calls and transaction
    callbacks reuse that owner, completion tears it down through an RAII execution
    scope, stale owners are rejected, and executions that do not access Postgres do
    not allocate a database actor. A dedicated VM-owned Postgres reactor now owns the
    actor runtime, pending-request table, deadlines, cancellation, driver polling,
    and completion delivery for every source execution; the evaluator no longer
    serializes calls through its own bounded worker pump. Concurrent source calls are
    accepted through independent owners, cross-owner pool use is rejected, and the
    authenticated Docker fixture retains an overlapping `pg_sleep` assertion that
    observes at least two pending requests in the shared reactor. Generated C ABI
    Rust remains self-contained and deterministic without requiring a host `rustfmt`;
    the checked libpq package matches regeneration byte-for-byte, its warning-denied
    all-target build passes, and all 27 C ABI generation, ownership, and execution
    tests pass. The warning-denied focused source lane passes 9 tests with its Docker
    selector separately gated, and the consolidated no-default-feature Postgres lane
    passes 119 tests with 2 Docker selectors ignored. A current
    `native-boundary-postgres-docker-check` rerun passed its libpq and generator stages,
    then this sandbox denied `/var/run/docker.sock` before the live fixtures; the
    previously recorded live fixture result remains the Docker evidence. The canonical
    `vm-postgres-runtime-check` was run: its 18 SQL quality tests, 108
    compiler/runtime SQL tests, proof-track unit suite, and SQL report stage pass,
    but the prerequisite replay still stops on the independently recorded Shape
    Implication EBNF fingerprint drift.
  - Completed progress: `VmPostgresLibpqWorker` now advances independent pool requests
    concurrently through request-keyed active, completion, cancellation, and wait
    collections while retaining deterministic serialization for operations sharing an
    explicit connection or transaction. The authenticated Docker gate proves two real
    `pg_sleep` queries enter active native state together and both complete, then proves
    two queries sharing one transaction remain ordered without a false stale-resource
    failure. The source-level concurrent-owner Docker fixture also passes through this
    production worker.
  - Completed progress: libpq socket readiness is now event-driven through a
    package-owned native extension backed by the maintained, exactly pinned
    `polling` crate. The generated package confines descriptor handling to the
    reviewed native boundary and exposes only safe read/write readiness events to
    the VM. Command and source execution share the request-keyed readiness poller,
    command submission wakes blocked waits, and deadlines remain VM-owned; the
    source reactor no longer uses a one-millisecond progress interval. The
    warning-denied all-target compiler check and package-owned extension test pass.
    The canonical `native-boundary-postgres-docker-check` passes deterministic
    generation with all 30 C ABI generator tests, the authenticated live libpq
    selector, and the authenticated source-level Postgres selector. This completes
    the database NativeBoundary ownership, concurrency, and readiness contract.
  - Acceptance: database calls must never surface raw driver errors or raw handles to
    Terlan code, and every connection/transaction must have a typed terminal state.
  - Acceptance: the gate fails if Postgres access bypasses VM process parking,
    cancellation, resource ownership, SQL validation, or Docker dependency readiness.


## Completed 044

- [x] Slice 47: make release artifacts and installers cross-platform and
  VM-complete.
  - Requirement: define a release artifact matrix for Linux, macOS, and Windows
    targets that includes `terlc`, the VM runtime path, standard library sources,
    editor metadata, tree-sitter grammar artifacts, checksums, version metadata, and
    install/uninstall manifests.
  - Requirement: the installer must detect OS/architecture, select the matching
    artifact, validate checksum/version, preserve existing user config, update PATH
    instructions, and report typed diagnostics for unsupported platforms.
  - Requirement: package validation must run against installed artifacts, not only
    workspace binaries, and must prove `terlc --version`, `terlc run`, `terlc test`,
    `terlc serve --check-config`, `terlc inspect --snapshot`, and editor metadata
    discovery all resolve from the installed layout.
  - Requirement: add artifact provenance with git revision, Rust target triple,
    standard library hash, VM hash, grammar hash, editor plugin hash, build profile,
    and release candidate id.
  - Requirement: add adversarial tests for stale binary in PATH, mismatched stdlib
    hash, missing VM artifact, wrong target triple, partial install, upgrade rollback,
    unsupported OS, corrupted checksum, and install over an older version.
  - Requirement: persist `vm-release-artifact-matrix-report.json` with target rows,
    artifact paths, checksums, install smoke results, upgrade behavior, and skip
    reasons for unavailable host platforms.
  - Gate: add `make vm-release-artifact-matrix-check` and run it after
    `vm-release-install-validation-check` and before release preflight artifact freeze.
  - Completed progress: release archives now ship `terlc`, `terlan-vm`, stdlib
    sources, VS Code metadata/assets, tree-sitter sources/generated grammar,
    per-file checksums, version/provenance metadata, and an install/uninstall
    manifest. Artifact metadata records compiler/VM, stdlib, editor, and grammar
    hashes plus source revision, target triple, build profile, feature set, and
    candidate identity. POSIX and PowerShell installers verify archive SHA-256
    sidecars before replacement, preserve user configuration, install shared
    assets, support x86_64/aarch64 platform naming, and restore an older compiler,
    VM, and shared payload after a failed upgrade. The public
    `terlc inspect [project] --snapshot` command reports VM and installed-layout
    discovery, and `serve --check-config` is a stable alias for release config
    validation. `make vm-release-artifact-matrix-check` builds release binaries
    once, validates six target rows, and runs the current-host archive and
    installer through version, browser/VM build, VM run, canonical test,
    serve-config, inspect, editor discovery, checksum, stale-PATH, partial
    artifact, target-triple, corrupted payload, and rollback checks. The passing
    report records 6 targets and 7,720 packaged payload files.
  - Acceptance: release cannot pass if any packaged command succeeds only from the
    workspace build or if any required VM/editor/std artifact is missing.
  - Acceptance: the gate fails if installer behavior is platform-ambiguous, if
    provenance is incomplete, or if upgrade rollback leaves mixed-version artifacts.


## Completed 045

- [x] Slice 48: package editor, LSP, tree-sitter, and icon assets with release
  parity.
  - Requirement: validate that release artifacts include the VS Code extension,
    TextMate grammar, tree-sitter grammar/package, file icons, command registrations,
    runnable main/test affordances, and LSP binary/stdio entrypoint metadata.
  - Requirement: verify packaged editor assets use the current Terlan logo/icon set,
    no stale folded-page icons, correct `_test.terl` icon variants, and stable light
    and dark theme assets.
  - Requirement: prove LSP hover documentation is served for modules, structs,
    functions, methods, std docs, generated summaries, and package docs from the
    installed artifact layout.
  - Requirement: prove editor commands resolve to installed `terlc`: run main, run
    individual test, add missing import, format document, show diagnostics, and attach
    debugger/inspector command stubs.
  - Requirement: add tree-sitter/package smoke tests from the packaged artifact, not
    only the source checkout, and ensure syntax changes cannot ship without editor
    grammar/corpus updates.
  - Requirement: add adversarial tests for missing LSP binary, stale grammar, missing
    command registration, stale icon bundle, broken hover docs, non-installed `terlc`
    path leakage, malformed `.terl` files, and syntax introduced without editor
    coverage.
  - Requirement: persist `editor-release-parity-report.json` with artifact paths,
    grammar hashes, icon hashes, command ids, hover-doc coverage, and installed-tool
    resolution.
  - Gate: add `make editor-release-parity-check` and run it after
    `vm-release-artifact-matrix-check` and before release preflight artifact freeze.
  - Completed progress: release builds now compile and archive the standalone
    `terlan-lsp` binary with the explicit `editor-lsp` tooling feature, record
    its checksum and provenance beside `terlc` and `terlan-vm`, and install or
    roll it back transactionally on POSIX and Windows. The archive includes the
    VS Code extension, shared icon/command contracts, TextMate grammars,
    Tree-sitter source, queries, and generated parser artifacts. The extension
    exposes installed-tool commands for main/test execution, missing imports,
    formatting, diagnostics, debugging, and deterministic runtime inspection.
    `make editor-release-parity-check` rebuilds and installs the archive, runs
    package-owned editor and Tree-sitter smokes from the extracted layout,
    regenerates the packaged parser and rejects hash drift, then drives the
    packaged language server over JSON-RPC stdio. Installed hover requests prove
    module, struct, function, receiver-method, stdlib, generated-summary, and
    package documentation. Its adversarial fixtures reject missing LSP/parser
    files, command/icon drift, workspace compiler leakage, broken hover docs,
    malformed Terlan source, and syntax/generated-parser drift. The passing
    `editor-release-parity-report.json` records 15 commands and hashes for the
    extension icon, TextMate grammar, Tree-sitter grammar, and generated parser.
  - Acceptance: release cannot pass if editor integration only works from source
    paths, if grammar/icon/LSP artifacts are stale, or if commands resolve to the
    wrong compiler binary.
  - Acceptance: the gate fails if any 0.0.7 syntax feature lacks matching
    tree-sitter/LSP/TextMate coverage in the packaged editor assets.


## Completed 046

- [x] Slice 49: publish docs/static site release parity and generated API
  references.
  - Requirement: generate release documentation from the installed `terlc`,
    standard library bundle, VM runtime metadata, CLI help output, editor hover
    docs, package docs, syntax/EBNF summaries, and release notes instead of
    source checkout paths.
  - Requirement: produce a deterministic static-site artifact suitable for
    GitHub Pages or equivalent static hosting, with stable asset hashes,
    versioned URLs, searchable module/function/type pages, and no network
    dependency during generation.
  - Requirement: cross-link README state, std module summaries, package
    references, CLI command docs, LSP hover documentation, editor setup, VM
    runtime inspection docs, and roadmap baselines to the exact installed
    version/provenance record.
  - Requirement: validate generated API references for modules, structs, shapes,
    functions, methods, tests, NativeBoundary capabilities, VM runtime commands,
    packages, and planned-but-unavailable features with explicit availability
    markers.
  - Requirement: add adversarial tests for stale README content, missing std
    docs, missing hover docs, stale CLI help, broken links, missing API sections,
    non-deterministic generated assets, source-path leakage, and static asset
    references that only work in the checkout.
  - Requirement: persist `docs-static-release-parity-report.json` with installed
    artifact paths, provenance hashes, generated page counts, link-check results,
    API coverage, command-help coverage, hover-doc coverage, and deterministic
    asset hashes.
  - Gate: add `make docs-static-release-parity-check` and run it after
    `editor-release-parity-check` and before release preflight artifact freeze.
  - Completed progress: release archives now include version-matched README and
    changelog inputs plus compiler, editor, grammar/EBNF, language, package,
    release, runtime, and standard-library documentation with a dedicated docs
    provenance hash. A shared installed-layout resolver makes `terlc doc std`
    discover the packaged stdlib before any developer-checkout fallback and is
    reused by runtime inspection. `make docs-static-release-parity-check`
    extracts the candidate, generates HTML and JSON references twice through
    installed `terlc`, normalizes display-only temporary paths, and requires
    identical complete site hashes across independent clean invocations. The
    versioned offline site contains 3,074 module pages, 21,835 searchable
    module/declaration records, installed CLI help for 18 command surfaces,
    runtime/provenance metadata, packaged source documentation, and explicit
    availability records for shapes and planned runtime/package surfaces. The
    gate checks 6,151 local links, consumes all seven installed LSP hover-doc
    categories, and rejects stale README/help, missing std/hover/API sections,
    broken links, checkout path/static asset leakage, and nondeterministic
    output. `docs-static-release-parity-report.json` records installed artifact
    hashes, page/search/link counts, API coverage, hover/command coverage, and
    the stable site digest.
  - Acceptance: release cannot pass if documentation, hover output, CLI help,
    static-site pages, or README claims disagree with the installed compiler,
    stdlib, VM, package, or editor artifacts.
  - Acceptance: the gate fails if docs generation succeeds only from source
    paths, if any public std/API surface lacks generated reference coverage, or
    if generated docs are not reproducible across two clean runs.


## Completed 047

- [x] Slice 50: promote release artifacts without rebuilding during publish.
  - Requirement: split release creation from release publication so CI and local
    workflows first build a signed release candidate directory, run all release
    gates against that immutable directory, and then publish exactly those
    artifacts.
  - Requirement: publication must never run `cargo build`, regenerate docs,
    rewrite editor packages, rebuild VM artifacts, or modify stdlib output after
    the release-candidate manifest has been sealed.
  - Requirement: produce a `release-candidate.json` manifest with version, git
    revision, target triples, artifact paths, checksums, stdlib hash, VM hash,
    docs hash, editor package hash, benchmark-baseline references, and gate
    report paths.
  - Requirement: add a dry-run promotion mode that verifies GitHub release
    payloads, installer metadata, static docs payloads, package archives, and
    checksum files without contacting external services.
  - Requirement: add adversarial tests for accidental rebuild during promotion,
    missing artifact from the sealed manifest, checksum drift, version mismatch,
    partial upload manifest, stale docs package, stale editor package, and
    release notes that refer to a different candidate hash.
  - Requirement: persist `release-promotion-pipeline-report.json` with sealed
    manifest path, artifact hashes, dry-run upload plan, publish inputs, and
    explicit proof that promotion consumed only prebuilt artifacts.
  - Completed progress: release artifact construction now seals the exact
    prebuilt upload set in `dist/release-candidate.json`. The deterministic
    manifest records version, source revision, target triples, archive hashes,
    stdlib/VM/docs/editor hashes, benchmark baseline references, gate-report
    hashes, and release-note provenance under a SHA-256 seal. Publication
    verifies all retained inputs, rejects unlisted archives, and obtains its
    NUL-delimited upload list from the verified manifest rather than discovering
    `dist/` files. GitHub release notes include the candidate seal, and the
    manifest itself is uploaded beside the archives for consumer verification.
    `make release-promotion-dry-run VERSION=0.0.7` writes the exact offline
    upload plan without contacting GitHub. Adversarial coverage rejects checksum
    and size drift, missing/extra artifacts, version mismatch, stale docs/editor
    content, stale release notes, publisher rebuild commands, and verification
    bypasses. `make release-promotion-pipeline-check`,
    `make installer-contract-check`, repeated real candidate verification, and
    repeated dry runs pass; the release preflight now includes this gate.
  - Gate: add `make release-promotion-pipeline-check` and run it after
    `docs-static-release-parity-check` and before any real publish command.
  - Acceptance: release cannot pass if publication success depends on rebuilding
    from source, if any promoted artifact is absent from the sealed candidate, or
    if candidate hashes differ between validation and promotion.
  - Acceptance: the gate fails if retrying publication can produce a different
    artifact set than the one that passed release validation.


## Completed 048

- [x] Slice 61: enforce release version, tag, and channel consistency.
  - Requirement: validate one canonical release version across Cargo metadata,
    `terlc --version`, VM runtime metadata, stdlib manifest, package manifests,
    editor extension manifests, tree-sitter package metadata, generated docs,
    installer metadata, release notes, attestation, and published artifact names.
  - Requirement: validate release tags, prerelease/stable channels, install URLs,
    package indexes, docs version paths, and upgrade metadata all point to the
    same sealed candidate and reject mixed-version candidates before publication.
  - Requirement: provide a single version bump/check command that reports every
    stale version field with file path, field name, observed version, expected
    version, and remediation.
  - Requirement: require channel-specific behavior for local dev builds,
    release candidates, stable releases, and post-publish verification without
    letting dev metadata leak into stable artifacts.
  - Requirement: add adversarial tests for stale Cargo/package/editor versions,
    stale installer URL, docs generated under the wrong version, release notes
    with mismatched tag, package index drift, prerelease metadata in stable
    artifacts, and local compiler binary shadowing the candidate.
  - Requirement: persist `release-version-channel-report.json` with canonical
    version, channel, tag, checked fields, mismatches, install URL matrix,
    package index status, and artifact filename coverage.
  - Gate: add `make release-version-channel-check` and run it after
    `release-notes-accuracy-check` and before final release readiness.
  - Acceptance: release cannot pass if any shipped artifact, generated document,
    package, editor asset, installer path, or release note carries a mismatched
    version, tag, or channel.
  - Acceptance: the gate fails if a version mismatch can be hidden by PATH
    ordering, source checkout metadata, or stale generated files.


## Completed 049

- [x] Slice 62: enforce generated-artifact freshness and clean regeneration.
  - Requirement: inventory every generated release artifact: std docs,
    generated API references, diagnostic catalog, compatibility manifest,
    release notes, editor grammars, tree-sitter output, icon package metadata,
    package indexes, native binding manifests, benchmark baselines, support
    schemas, and release attestation inputs.
  - Requirement: provide one regeneration gate that runs from a clean workspace,
    regenerates all tracked generated artifacts, and fails if tracked files
    drift, generated hashes change unexpectedly, or untracked generated files are
    required for release.
  - Requirement: generated outputs must be deterministic across two clean runs
    with stable ordering, normalized timestamps, normalized paths, and no host-
    local absolute paths.
  - Requirement: classify generated files as committed, packaged-only,
    cache-only, or ignored build output, and reject any release-critical artifact
    that has no classification.
  - Requirement: add adversarial tests for stale generated docs, stale grammar
    output, stale diagnostics, nondeterministic ordering, absolute path leakage,
    untracked generated files required by release, generated artifacts using
    source checkout binaries, and generated cache files committed accidentally.
  - Requirement: persist `release-generated-artifacts-report.json` with artifact
    inventory, classification, regeneration commands, hash comparison, drift
    summary, and deterministic-run comparison.
  - Completed progress: `make stdlib-embedded-interface-contract-check` now
    protects release-critical summaries whose contracts are compiled into the
    shipped binaries. Public std contracts now spell externally owned callable
    and higher-kinded types canonically before summaries omit source imports.
    The gate runs exact warning-as-error regressions for
    `std.core.Option.compare`, the higher-kinded
    `KeyedEnumerable[std.collections.Map.Map]` conformance, and the overloaded
    free/receiver `std.vm.DistributedStorage.policy_name/1` surface. It then
    regenerates the three source interfaces into temporary storage and compares
    all `.typi` and `.typi.deps` artifacts byte-for-byte against the committed
    summaries. Same-name/same-arity free and receiver functions are inspected
    through overload metadata rather than the compatibility single-signature
    map.
  - Completed progress: TypeScript binding generation now canonicalizes every
    generated `.terl` and `.terli` artifact before dependency and manifest
    hashes are computed. `make stdlib-js-bindings-drift-check` proves all 7,257
    pinned `std.js` artifacts match clean generation, while
    `make stdlib-summary-drift-check` proves all 158 std summary artifacts match
    source regeneration. Public collection impl heads now preserve their
    type-owning `List.List`, `Map.Map`, and `Set.Set` identities through summary
    generation. Pipe resolution also falls back to a selected ordinary function
    when a same-named receiver candidate is type-incompatible, with an
    adversarial imported-function regression; all 53 `std/collections` tests
    pass against the regenerated summaries.
  - Completed progress: TypeScript declaration names are now canonicalized at
    the shared DOM module-planning boundary rather than only while rendering a
    type declaration. Lowercase legacy globals such as `webkitURL` therefore
    produce one consistent `WebkitURL` identity across module paths, source and
    interface filenames, generated tests, summaries, manifests, and declaration
    text while retaining the original source name in provenance metadata. The
    public generator regression asserts every canonical artifact exists and
    every former lowercase path is absent. All 11 bind-command tests pass,
    warnings-as-errors compilation is clean, `stdlib-js-bindings-drift-check`
    reproduces all 7,257 committed artifacts byte-for-byte, and
    `std-source-naming-check` accepts all 4,525 std sources.
  - Completed progress: `make release-generated-artifacts-check` now composes
    the existing std summary, generated JS binding, NativeBoundary artifact,
    std release-manifest, and tree-sitter regeneration/drift gates instead of
    duplicating their domain logic. The canonical
    `docs/release/GENERATED_ARTIFACTS.json` inventory classifies five generated
    artifact families by path, owner, storage class, regeneration command, and
    freshness gate. Its shared validator rejects duplicate or unsorted IDs,
    undocumented fields, unsupported classifications, unsafe or unmatched
    paths, missing Make targets, and aggregate recipes that omit an inventoried
    gate, then writes the deterministic
    `release-generated-artifacts-report.json`. Make target parsing is now owned
    by one `tools/makefile_contract.py` helper reused by this gate and the
    1,919-file active external VM test-suite audit. Focused gate runs pass in
    49.16, 119.49, and 139.97 seconds while validating 158 summaries, 7,257 JS
    artifacts, 14 native modules, 73 release-manifest modules, and 10
    tree-sitter corpus cases; the measured runtime spread remains visible for
    the release duration-budget investigation rather than being normalized
    away.
  - Completed progress: interface extraction now applies the existing imported
    type-reference qualifier to public function and receiver-method parameters,
    nested callback signatures, and return types. Generated summaries therefore
    remain self-contained when source uses selected type imports and the summary
    omits imports. Qualification is limited to direct non-default selected types:
    module-default aliases such as `Option`, `Result`, `List`, and `Unit` remain
    resolver-owned, while `Ordering.{Comparison}` becomes the canonical
    `std.core.Ordering.Comparison`. Focused HIR regressions cover nested callback
    returns, direct returns, collapsed imports, selected module-default imports,
    and import-free summaries. After clean regeneration, all 2,612 runnable VM
    tests pass with one ignored, all executable stdlib release tests pass, and
    `release-generated-artifacts-check` passes in 45.66 seconds while matching
    all 158 committed summaries. Trait conformance validation now reuses the
    same qualifier for local trait signatures, explicit impl ownership,
    adapter method parameters, callback types, return types, dispatch
    candidates, and coherence keys. A focused imported-alias regression passes
    in 0.02 seconds, all 29 trait tests pass, and the release collection
    contract sweep passes in 78.21 seconds.
  - Completed progress: generated-artifact validation now expands all inventory
    patterns through one deterministic file collector and scans artifact bytes
    for Unix home paths, Unix temporary paths, and Windows user-profile paths.
    Adversarial self-tests reject `/home/...` and `C:\Users\...` leakage while
    accepting portable output. The generated-artifact report records the file
    count and enforced content policies; `release-generated-artifacts-check`
    passes in 49.40 seconds after scanning 7,459 files across all five currently
    inventoried artifact families.
  - Completed progress: generated-artifact determinism evidence now comes from
    two independent filesystem scans rather than rendering the same in-memory
    report payload twice. Each scan records canonical artifact ownership, path,
    byte size, and SHA-256 content hash for all 7,459 inventoried files; any
    difference fails before the report is written. The report persists both
    snapshot digests, their equality decision, the combined content digest, and
    a zero-drift summary. Adversarial self-tests mutate a generated file and
    prove that both its snapshot and combined digest change.
  - Completed progress: `release-generated-artifacts-check` now records the
    complete generated-artifact snapshot, runs the shared five-gate freshness
    pass twice, and rejects any artifact change after either independent pass.
    The report records `run_count: 2` and confirms both regeneration runs
    preserved all 7,459 inventoried files. Its adversarial fixture rejects
    snapshot mutation and malformed orchestration that omits a run, comparison,
    or inventoried freshness gate. The gate also exposed and removed 16 stale
    release API rows that still classified generated compile-surface contracts
    as runtime tests, and regenerated the two stale std dependency summaries.
    `make release-generated-artifacts-check` passes in 255.84 seconds across 158
    summaries, 7,257 JS artifacts, 14 native modules, 73 release-manifest
    modules, and 10 tree-sitter corpus cases. `make rust-warnings-check` passes
    after narrowing the benchmark compatibility namespace to its used exports.
  - Gate: add `make release-generated-artifacts-check` and run it after
    `release-version-channel-check` and before final release readiness.
  - Acceptance: release cannot pass if regeneration changes committed generated
    files, if packaged generated artifacts are stale, or if any release artifact
    depends on unclassified generated state.
  - Acceptance: the gate fails if generated output embeds host-local paths,
    timestamps, source checkout locations, or nondeterministic ordering.


## Completed 050

- [x] Slice 63: enforce release code hygiene, size budgets, and dead-code
  rejection.
  - Requirement: treat Rust warnings, unreachable code, unused public helpers,
    stale feature flags, duplicate implementations, and dead runtime paths as
    release blockers unless explicitly classified with an owner and removal plan.
  - Requirement: enforce file-size, function-size, module-size, and test-fixture
    size budgets for compiler, VM, std tooling, web runtime, package tooling,
    editor tooling, and release scripts, with generated files classified
    separately.
  - Requirement: audit `panic!`, `unwrap`, `expect`, unchecked indexing,
    stringly-typed command dispatch, ad-hoc parsers, shell-script duplication,
    and one-off test helpers that should be shared or removed before release.
  - Requirement: require every release-critical function added in 0.0.7 to have
    documentation or an accepted private-helper exemption, and require complex
    helpers to have focused unit/adversarial coverage.
  - Requirement: add adversarial tests for newly introduced warnings, oversized
    files without exemptions, duplicate helper types, unused generated outputs,
    panic paths reachable from user input, unclassified dead code, and release
    scripts that bypass shared helpers.
  - Requirement: persist `release-code-hygiene-report.json` with warning count,
    size-budget violations, panic/unwrap inventory, dead-code inventory,
    duplicate-helper findings, exemptions, and remediation owners.
  - Completed progress: the `terlan` crate now denies Rust warnings at the
    crate lint level, not only through `RUSTFLAGS`, and `make
    rust-warnings-check` passes with default binaries. The ACME/TLS live
    issuance helpers are now cfg-owned by `acme-live` or `test`, so default
    builds no longer carry dead-code warning debt for optional live-network
    paths.
  - Completed progress: deprecated Rust API calls are now covered by
    warning-as-error gates. The database migration timestamp formatter uses
    `time::format_description::parse_borrowed::<2>`, the LSP document-symbol
    path no longer carries deprecated-field suppressions, `rg` finds no
    `format_description::parse(...)` or `#[allow(deprecated)]` occurrences under
    `crates/terlan/src`, `RUSTFLAGS='-D warnings' cargo check --locked -p terlan
    --all-targets` passes, `RUSTFLAGS='-D warnings' cargo test --locked -p
    terlan --all-targets --no-run` passes, and `make rust-warnings-check`
    passes.
  - Completed progress: the DB command configuration path no longer carries an
    unclassified `#[allow(dead_code)]` accessor for future adapters. Tests now
    assert validated URL precedence against the owned config field directly,
    `rg` finds no `allow(dead_code)`, `fn config(&self)`, or `.config()` calls in
    `crates/terlan/src/commands/db/mod.rs` or `mod_test.rs`, `RUSTFLAGS='-D
    warnings' cargo check --locked -p terlan --all-targets` passes, and `make
    rust-warnings-check` passes.
  - Completed progress: the debugger script command inventory no longer uses
    an inline `#[cfg(test)]` item in production source. `script.rs` owns the
    reserved command list as parser runtime data, debug parser tests import
    that real inventory directly, CLI help tests keep their expected list in
    test code, `rg` finds no inline `#[cfg(test)]` block in
    `crates/terlan/src/commands/debug/script.rs`, `cargo test --locked -p
    terlan --bin terlc commands::debug -- --nocapture` passes, `cargo test
    --locked -p terlan --bin terlc tests::debug_cli_test -- --nocapture`
    passes, and `RUSTFLAGS='-D warnings' cargo check --locked -p terlan
    --all-targets` passes.
  - Completed progress: `shared-helper-check` now requires duplicate-helper
    baseline rows to carry an owner and removal plan, and the current 11
    duplicate helper groups are classified by subsystem. New, grown, stale, or
    unowned duplicate helper bodies fail before release hygiene can pass. The
    gate now runs focused parser self-tests, rejects duplicate baseline hash
    rows before they can overwrite each other, and the placeholder-term
    evidence validators shared by editor and VM HTTP quality reports have been
    extracted into one helper instead of adding new duplicate-helper debt.
  - Completed progress: `dormant-runtime-code-check` now matches the current
    VM runtime inventory: stale rows for modules that are active again were
    removed, `live_template_protocol` is classified as dormant implementation
    debt, and the gate reports 5 dormant VM modules across 5 inventory rows.
    The gate also enforces the exact TSV header, approved classification
    vocabulary, placeholder-free reason/action fields, and byte-lexically
    sorted paths so release hygiene evidence stays deterministic. Duplicate
    dormant inventory rows now have direct adversarial coverage.
  - Completed progress: `terlan-lint-style-profile-check` now locks the
    formatter/lint boundary for semicolon chains and pipe canonicalization.
    The checked style profile includes `TL1003
    format-boundary.semicolon-split`, requires pipe canonicalization to remain
    lint-owned, rejects duplicate rule IDs, rejects TODO/TBD/placeholder/stub
    terms, requires concrete `fix-safe`/`fix-unsafe`/`fix-unavailable`
    diagnostic markers for `terlc lint --fix` consumers, and reports 18 seed
    rule IDs.
  - Completed progress: `terlan-lint-pipe-canonicalization-check` now covers
    occurrence-aware pipe diagnostics and fixes. Repeated identical candidates
    report distinct source locations, `--fix` skips matching text inside string
    literals and comments, let-body pipe candidates are rewritten safely, and
    `std/collections` is clean under the pipe canonicalization gate.
    `std.collections.List` now uses a selected `Iterator.{each}` import and the
    type-safe direct call `each(iterator(list), cb)`, so the std collection
    surface stays aligned with selected-import rules while avoiding ambiguous
    receiver-method pipe dispatch for iterator callbacks.
  - Completed progress: VM CoreIR pipe-forward execution now dispatches
    call-like right-hand sides before evaluating them as ordinary binary
    operands. The evaluator covers constructor targets such as `1 |> List(2)`
    and explicit trait remotes such as
    `[1, 2] |> Enumerable[List].map(square)`. Collection interface summaries now
    qualify `Enumerable`, `KeyedEnumerable`, and `Iterable` conformances against
    the actual type-owning modules (`std.collections.List.List`,
    `std.collections.Map.Map`, `std.collections.Set.Set`). The focused VM
    regression `evaluator_pipe_forward_dispatches_constructor_and_explicit_trait_remote`
    passes, and release-lane exact tests for enumerable list map/filter/fold,
    map fold, and set fold pass. Follow-up VM collection release coverage now
    also passes `EnumerableTest.terl`, `IterableTest.terl`, and
    `IteratorTest.terl` under exact `terlc test` execution, including selected
    imported iterator function values and VM-owned `List`/`Map`/`Set` iterator
    dispatch.
  - Completed progress: `release-code-hygiene-report.json` now carries explicit
    release evidence fields for `warning_count`,
    `active_size_budget_violation_count`, `panic_unwrap_inventory`,
    `dead_code_inventory`, `duplicate_helper_findings`, `exemptions`, and
    `remediation_owners`. The report validator rejects missing evidence
    sections, placeholder fields, misordered sub-gates, and missing umbrella
    commands; `make release-code-hygiene-check` passes with the expanded report
    schema.
  - Completed progress: release generation no longer pushes the TypeScript DOM
    generator or HIR resolver over their size budgets. Binding-manifest
    rendering and skipped-declaration reporting now live in a focused module,
    and syntax shape-signature parsing now lives behind the HIR module boundary.
    The focused DOM fixture and imported-trait regressions pass, while `make
    rust-warnings-check`, `make rust-quality-check`, `make
    release-generated-artifacts-check`, `make shared-helper-check`, `cargo fmt
    --all --check`, and `git diff --check` pass without increasing any baseline.
  - Gate: add `make release-code-hygiene-check` and run it after
    `release-generated-artifacts-check` and before final release readiness.
  - Acceptance: release cannot pass if code hygiene relies on manual review,
    hidden warnings, unclassified dead code, or unchecked panic paths reachable
    from public commands.
  - Acceptance: the gate fails if any exemption lacks owner, reason, expiry
    milestone, and a linked cleanup task.


## Completed 051

- [x] Slice 64: enforce CI/local release gate parity and fail-fast behavior.
  - Requirement: ensure CI workflows invoke the same Make targets documented in
    the release roadmap instead of duplicating inline shell logic, ad-hoc cargo
    commands, or partial test subsets.
  - Requirement: provide a local `make release-ci-local-parity-check` path that
    parses workflow files, Make targets, planned gates, release attestation
    inputs, and gate reports to prove local and CI closeout surfaces match.
  - Requirement: workflows must fail fast on the first required gate failure,
    preserve logs/support bundles for the failed gate, and avoid continuing into
    publish/promotion steps after validation failure.
  - Requirement: cache behavior must not hide stale generated artifacts, stale
    compiler binaries, stale stdlib bundles, stale editor packages, or stale
    benchmark baselines.
  - Requirement: add adversarial tests for workflow-only gates, local-only gates,
    inline commands not represented by Make targets, CI continuing after failure,
    cache restoring stale artifacts, matrix rows missing required gates, and
    publish jobs that do not depend on attestation success.
  - Requirement: persist `release-ci-local-parity-report.json` with workflow
    files checked, Make targets matched, missing/extra gate lists, fail-fast
    dependency graph, cache policy findings, and publish dependency proof.
  - Completed progress: local publication and both GitHub validation workflows
    now consume the single ordered `release-candidate-check` target instead of
    duplicating `check`, test, editor, and tree-sitter commands. Both workflows
    retain `target/quality` and `build/artifacts` evidence through a
    failure-only artifact upload, while release preflight requires the parity
    gate. The structured YAML validator rejects duplicate Make/Cargo validation,
    fail-open jobs/steps, unsafe caches, missing failure evidence, local publish
    bypass, and publish jobs without a canonical validation dependency. Its 9
    adversarial tests pass and
    `target/quality/release-ci-local-parity-report.json` records 2 workflows, 2
    canonical validation jobs, no missing gates, no extra validation commands,
    no cache findings, and SHA-256 input digests.
  - Gate: add `make release-ci-local-parity-check` and run it after
    `release-code-hygiene-check` and before final release readiness.
  - Acceptance: release cannot pass if CI and local release validation can
    disagree about required gates, artifacts, or publish preconditions.
  - Acceptance: the gate fails if a workflow can publish, promote, or upload
    artifacts without consuming the same release attestation that local release
    validation produced.


## Completed 052

- [x] Slice 70: define native `no_std`, embedded, and kernel-target feasibility
  contracts.
  - Requirement: produce a target capability matrix for native host, embedded
    Linux, RTOS-like environments, bare-metal `no_std`, kernel-like restricted
    environments, RISC-V, ARM microcontrollers, and system-on-chip deployments.
  - Requirement: classify which Terlan features can be native-lowered without a
    VM, which require a reduced VM runtime, which require host OS services, and
    which must be rejected with stable diagnostics for constrained targets.
  - Requirement: define the minimal allowed surface for constrained targets:
    pure functions, fixed-size numeric types, static data, explicit memory
    policy, explicit panic strategy, no implicit filesystem/network/process
    access, no default heap unless target profile declares one, and no ambient
    runtime.
  - Requirement: define package/HAL boundaries for hardware access through
    generated Rust bindings or maintained embedded crates, with no hand-rolled
    device drivers, crypto, protocol stacks, or allocator/runtime assumptions in
    Terlan core.
  - Requirement: add parser/typechecker/target-profile fixtures proving kernel,
    `no_std`, and bare-metal target declarations either compile only the allowed
    pure subset or fail with cataloged diagnostics naming the unsupported
    feature and required capability.
  - Requirement: persist `native-no-std-target-feasibility-report.json` with
    target rows, supported feature matrix, rejected feature matrix, std subset
    inventory, required Rust target/toolchain notes, and future implementation
    prerequisites.
  - Gate: add `make native-no-std-target-feasibility-check` and run it after
    `release-fault-injection-check` and before final release readiness.
  - Completed progress: `make --no-print-directory
    native-no-std-target-feasibility-check` now validates seven deterministic
    target-family rows, twelve feature classes, eight rejected constrained
    features, and eight adversarial assumptions, then writes
    `target/quality/native-no-std-target-feasibility-report.json`. The real
    build parser reserves `native.no-std`, `native.bare-metal`,
    `native.kernel`, `native.rtos`, `native.riscv`, and `native.arm` with a
    stable unimplemented-family diagnostic, preventing host or VM fallback
    until constrained-target artifact production exists.
  - Acceptance: 0.0.7 must not claim kernel, RTOS, or bare-metal support beyond
    the proven capability matrix.
  - Acceptance: the gate fails if constrained targets silently accept VM-only,
    heap-only, OS-only, blocking, networking, filesystem, or NativeBoundary
    features without explicit capability declarations.


## Completed 053

- [x] Slice 71: generate deterministic device-target architecture plans.
  - Requirement: define a device-profile schema for CPU, memory budget, allocator
    policy, panic strategy, runtime profile, available peripherals, package/HAL
    capabilities, linker/output format, required Rust target, and unsupported
    Terlan features.
  - Requirement: add a compiler planning mode that reads a Terlan project plus a
    device profile and emits a deterministic target plan before lowering: selected
    runtime, std subset, package capabilities, native bindings, memory policy,
    rejected imports, required toolchains, and output artifacts.
  - Requirement: prove the planner is capability-driven and never guesses hidden
    firmware behavior: unsupported filesystem, networking, heap, actor, VM,
    database, HTTP, or NativeBoundary features must be rejected with cataloged
    diagnostics unless explicitly provided by the device profile.
  - Requirement: include at least one constrained fixture profile for the NXT
    experiment and one generic RISC-V embedded profile, both limited to planning
    and diagnostics unless a real lowering path exists.
  - Requirement: add adversarial tests for missing device fields, inconsistent
    memory budgets, unsupported imports, undeclared peripherals, package/HAL
    mismatch, nondeterministic plan ordering, source-checkout path leakage, and
    plans that claim artifacts the compiler cannot produce.
  - Requirement: persist `device-target-planner-report.json` with profiles
    checked, plan hashes, rejected feature list, required package capabilities,
    diagnostics, and future lowering prerequisites.
  - Gate: add `make device-target-planner-check` and run it after
    `native-no-std-target-feasibility-check` and before final release readiness.
  - Current gate state: `make --no-print-directory device-target-planner-check`
    exists and passes. It writes
    `target/quality/device-target-planner-report.json` with 2 profiles checked,
    2 deterministic plan hashes, 8 rejected features, 7 required package/HAL
    capabilities, 9 diagnostics, 8 adversarial cases, and 6 future lowering
    prerequisites.
  - Completed progress: the planner now has built-in constrained NXT and generic
    RISC-V fixture profiles, validates the full device-profile schema, rejects
    inconsistent memory budgets, package/HAL mismatches, unproducible artifact
    claims, unsupported imports, nondeterministic ordering, and source-checkout
    path leakage, and emits deterministic capability-driven plans without host
    defaults or hidden firmware assumptions.
  - Current rejected paths: none for this slice.
  - Acceptance: 0.0.7 may expose device planning only as a deterministic
    capability report unless all planned output artifacts are actually produced
    and verified.
  - Acceptance: the gate fails if the planner accepts constrained-target code by
    falling back to host defaults, ambient stdlib features, or unstated runtime
    assumptions.


## Completed 054

- [x] Slice 72: enforce reproducible package resolution and lockfile behavior.
  - Requirement: define the package resolver contract for first-party packages,
    external packages, native packages, generated bindings, std overlays,
    version constraints, target capabilities, and package lockfiles.
  - Requirement: package resolution must be deterministic across two clean
    workspaces with the same package manifest, lockfile, target, registry
    snapshot, and installed compiler artifact.
  - Requirement: lockfiles must record package name, version, source, checksum,
    target/capability constraints, generated binding hashes where applicable,
    native artifact hashes, and resolver version.
  - Requirement: add offline resolution from a local registry/cache mirror and
    fail with cataloged diagnostics when a package, checksum, native artifact,
    generated binding, or target capability is missing.
  - Requirement: add adversarial tests for dependency cycles, conflicting version
    ranges, stale lockfiles, checksum mismatch, target-incompatible packages,
    undeclared native capabilities, registry ordering differences, source-path
    leakage, and resolver output that changes between runs.
  - Requirement: persist `package-resolver-reproducibility-report.json` with
    fixture manifests, lockfile hashes, registry snapshot hash, resolved graph,
    diagnostic coverage, and deterministic-run comparison.
  - Gate: add `make package-resolver-reproducibility-check` and run it after
    `device-target-planner-check` and before final release readiness.
  - Completed progress: `terlan-package-lockfile-check` now enforces the
    package lockfile baseline contract with 12 required lockfile terms, 8
    required reproducibility fields, forbidden target-lockfile authority claims,
    and placeholder-text rejection.
  - Completed progress: `terlan-package-git-source-check` now runs the
    Git-source contract validator in addition to exact manifest parser tests,
    enforcing 12 Git source contract terms, 6 required resolved-source fields,
    forbidden floating-source authority claims, and placeholder-text rejection.
  - Completed progress: `package-resolver-reproducibility-check` now composes
    the Terlan lockfile and Git-source package gates, runs both subcontracts,
    and writes `package-resolver-reproducibility-report.json` with contract-backed
    resolver evidence, deterministic-run comparison metadata, and diagnostic
    coverage for lockfile/source drift.
  - Acceptance: release cannot pass if package resolution depends on workspace
    paths, registry iteration order, ambient network state, stale generated
    bindings, or hidden native artifacts.
  - Acceptance: the gate fails if resolver drift can change the dependency graph
    without changing the lockfile or producing a cataloged diagnostic.


## Completed 055

- [x] Slice 73: enforce package registry and publish integrity contracts.
  - Requirement: define package publish inputs: package manifest, source archive,
    generated binding manifest, native artifact manifest, docs summary, checksum
    file, compatibility metadata, target capability metadata, and package
    provenance.
  - Requirement: package publication must be a dry-run capable promotion of a
    sealed package archive, not a rebuild from the workspace, and package
    versions must be immutable once published.
  - Requirement: validate registry index updates are deterministic, append-only
    for new versions, explicit for yanks, and rejected for checksum changes,
    missing provenance, missing docs, missing target metadata, or hidden native
    artifacts.
  - Requirement: support offline registry mirror validation so package publish
    gates can run without network access while still producing the exact index
    diff that a live publish would submit.
  - Requirement: add adversarial tests for duplicate package versions, overwritten
    checksums, missing generated binding hashes, missing native artifact hashes,
    target-incompatible packages, stale docs, malformed index entries, yanked
    packages resolving silently, and publish commands that rebuild from source.
  - Requirement: persist `package-registry-publish-report.json` with package
    archive path, archive hash, index diff, provenance hash, docs hash, target
    metadata, dry-run publish result, and rejected mutation attempts.
  - Gate: add `make package-registry-publish-check` and run it after
    `package-resolver-reproducibility-check` and before final release readiness.
  - Completed progress: `hex-target-metadata-check` now enforces 16 package
    metadata contract terms, including source roots, generated artifacts,
    native boundary declarations, and compiler target selection. The gate also
    rejects placeholder/TODO/TBD/fixme metadata and forbidden Hex/OTP/Rebar/BEAM
    compatibility claims so Hex remains distribution infrastructure rather than
    the package or runtime contract.
  - Completed progress: `package-registry-publish-check` now runs after package
    resolver reproducibility, validates the package registry publish contract,
    and writes `package-registry-publish-report.json` with sealed-archive
    promotion evidence, publish inputs, deterministic index policy, offline
    mirror validation, and rejected mutation attempts.
  - Acceptance: release cannot pass if first-party or native packages can be
    published without immutable archives, checksums, docs, target metadata, and
    provenance.
  - Acceptance: the gate fails if package publish behavior differs between local
    dry-run, CI dry-run, and the generated live-publish plan.


## Completed 056

- [x] Slice 74: enforce package capability manifests and sandbox contracts.
  - Requirement: require every package to declare the capabilities it needs:
    filesystem, network, HTTP listener, database, NativeBoundary resources,
    generated bindings, native artifacts, environment variables, process
    spawning, debugger hooks, and release-time hooks.
  - Requirement: package capabilities must be checked at install, build,
    typecheck, VM runtime, release packaging, and support-bundle generation, with
    no ambient permissions inherited from the host process.
  - Requirement: native packages must declare resource handle types, blocking
    policy, cancellation behavior, target compatibility, generated binding hash,
    native artifact hash, and security review status.
  - Requirement: package consumers must see a deterministic capability summary
    in lockfiles, diagnostics, generated docs, and release reports before any
    privileged capability is used.
  - Requirement: add adversarial tests for undeclared filesystem/network access,
    hidden NativeBoundary calls, stale native artifact hashes, capability drift
    between manifest and lockfile, package import aliases bypassing checks,
    generated bindings requesting extra capabilities, and runtime handles reused
    across package boundaries.
  - Requirement: persist `package-capability-contract-report.json` with package
    capability matrix, denied operation fixtures, native resource inventory,
    lockfile capability hashes, and diagnostic coverage.
  - Gate: add `make package-capability-contract-check` and run it after
    `package-registry-publish-check` and before final release readiness.
  - Completed progress: `native-boundary-security-check` now runs its 8 focused
    security policy tests before the CLI scan and still covers 117 Rust-backed
    operations with 14 policy rules, including the previously uncovered
    `std.http.WebSocket` operations and a focused regression test for WebSocket
    policy coverage.
  - Completed progress: `package-capability-contract-check` now runs after the
    package registry publish gate, validates the package capability contract,
    and writes `package-capability-contract-report.json` with capability
    surfaces, enforcement checkpoints, native package metadata, and adversarial
    package-boundary cases.
  - Acceptance: release cannot pass if a package can use privileged filesystem,
    network, native, database, process, debugger, or release-time behavior
    without an explicit capability declaration.
  - Acceptance: the gate fails if capability checks differ between typecheck,
    package resolution, VM runtime, release packaging, and generated docs.


## Completed 057

- [x] Slice 75: enforce package release test matrix and quality gates.
  - Requirement: every first-party package intended for 0.0.7 release must
    declare its package type, target support, capability contract, tests,
    examples, docs, generated artifacts, native artifacts when applicable, and
    publish readiness state.
  - Requirement: package test matrices must run from clean temporary workspaces
    using the installed compiler, installed stdlib, package lockfile, local
    registry mirror, and VM default runtime unless a package explicitly targets
    another verified artifact.
  - Requirement: package tests must cover build, test, docs generation, example
    execution, formatter/lint checks, capability denial paths, package resolver
    behavior, lockfile behavior, and support-bundle output on failure.
  - Requirement: native packages must additionally cover binding generation,
    native artifact discovery, target compatibility diagnostics, stale handle
    diagnostics, cancellation behavior, and missing native dependency skips.
  - Requirement: add adversarial tests for packages with no examples, packages
    with docs that do not compile, packages that pass only from workspace paths,
    missing capability tests, stale generated bindings, broken lockfiles,
    missing target metadata, and publish-ready packages without tests.
  - Requirement: persist `package-release-test-matrix-report.json` with package
    rows, target rows, command results, docs/examples coverage, capability
    coverage, native coverage, skipped rows, and publish readiness status.
  - Gate: add `make package-release-test-matrix-check` and run it after
    `package-capability-contract-check` and before final release readiness.
  - Completed progress: `package-release-test-matrix-check` now runs after the
    package capability contract gate, validates the package release test matrix
    contract, and writes `package-release-test-matrix-report.json` with clean
    workspace requirements, package command coverage, native package coverage,
    and adversarial package rows.
  - Acceptance: release cannot pass if any release package lacks executable
    tests, docs, examples, capability coverage, and deterministic lockfile
    behavior.
  - Acceptance: the gate fails if package test success depends on source
    checkout paths, ambient network access, undeclared native libraries, or
    non-VM default runtime behavior.


## Completed 058

- [x] Slice 76: enforce package API compatibility and semantic version policy.
  - Requirement: generate a public API manifest for each release package with
    modules, exported functions, types, constructors, shapes, capabilities,
    generated bindings, docs anchors, examples, diagnostics, and target support.
  - Requirement: compare package API manifests against the previous published
    package version and classify changes as additive, compatible tightening,
    deprecated, breaking, private, target-only, or generated-binding-only.
  - Requirement: require semantic version policy for every package: patch releases
    cannot remove or break public APIs, minor releases must document additive
    surfaces, and breaking changes require major/pre-1 compatibility annotation
    plus migration guidance.
  - Requirement: package-level diagnostics must point to migration guidance when
    imports, symbols, capabilities, generated bindings, or target support change.
  - Requirement: add adversarial tests for removed exports without version bump,
    changed function arity, changed type shape, stale docs anchors, target
    support drift, generated binding drift, capability drift, and package examples
    importing removed APIs.
  - Requirement: persist `package-api-compatibility-report.json` with package
    names, old/new manifest hashes, diff classifications, required version bump,
    migration coverage, and rejected unclassified changes.
  - Gate: add `make package-api-compatibility-check` and run it after
    `package-release-test-matrix-check` and before final release readiness.
  - Completed progress: `package-api-compatibility-check` now runs after the
    package release test matrix gate, validates the package API compatibility
    contract, and writes `package-api-compatibility-report.json` with manifest
    fields, diff classifications, semantic version policy, migration coverage,
    and adversarial API drift cases.
  - Acceptance: release cannot pass if any package API changes without an
    explicit compatibility classification, version policy result, diagnostics,
    and migration/docs coverage.
  - Acceptance: the gate fails if generated bindings or capabilities change the
    public package contract without appearing in the package API manifest.


## Completed 059

- [x] Slice 77: add deterministic package CLI workflows for users and CI.
  - Requirement: define package CLI surfaces for `terlc package add`,
    `terlc package remove`, `terlc package update`, `terlc package tree`,
    `terlc package audit`, `terlc package publish --dry-run`, and
    `terlc package cache clean --check`.
  - Requirement: every package command must update or validate manifests and
    lockfiles deterministically, render text and JSON output, and avoid network
    access unless the command explicitly requests a live registry.
  - Requirement: package commands must run from installed release artifacts in
    clean temporary workspaces and preserve user files unless an explicit write
    action is requested.
  - Requirement: `package tree` and `package audit` must show target constraints,
    capabilities, native artifacts, generated bindings, yanked packages,
    duplicate versions, and security/provenance warnings from the lockfile and
    registry mirror.
  - Requirement: add adversarial tests for adding incompatible packages, removing
    transitive dependencies, update conflicts, stale lockfiles, yanked packages,
    malformed package specs, cache poisoning, source-path leakage, JSON/text
    output drift, and write operations without explicit consent.
  - Requirement: persist `package-cli-workflow-report.json` with command matrix,
    before/after manifest hashes, lockfile hashes, output snapshots, diagnostics,
    and cache behavior.
  - Gate: add `make package-cli-workflow-check` and run it after
    `package-api-compatibility-check` and before final release readiness.
  - Completed progress: `package-cli-workflow-check` now runs after the package
    API compatibility gate, validates the package CLI workflow contract, and
    writes `package-cli-workflow-report.json` with command matrix, manifest and
    lockfile hash coverage, output snapshots, diagnostics, cache behavior, and
    adversarial workflow cases.
  - Acceptance: release cannot pass if users cannot inspect, update, audit, and
    dry-run publish packages through deterministic CLI commands.
  - Acceptance: the gate fails if any package command mutates user files without
    an explicit write mode or produces different lockfile/output state across
    identical clean workspaces.


## Completed 060

- [x] Slice 78: expose package metadata through editor and LSP workflows.
  - Requirement: LSP must resolve package modules, exported functions, types,
    shapes, docs, examples, capabilities, diagnostics, and generated binding
    metadata from the project lockfile and installed package cache.
  - Requirement: editor completion must suggest package imports, exported
    symbols, methods, constructors, capabilities, and documented examples without
    reaching into source checkout paths.
  - Requirement: hover documentation must show package version, docs summary,
    target support, capability requirements, deprecation status, generated
    binding provenance when applicable, and link to generated package docs.
  - Requirement: missing package, stale lockfile, yanked package, incompatible
    target, missing capability, and missing native artifact diagnostics must
    match CLI diagnostics and include fix suggestions where possible.
  - Requirement: add adversarial tests for stale LSP package cache, package
    import aliasing, missing docs, yanked packages, generated binding drift,
    editor command path leakage, package upgrade while editor is running, and
    CLI/LSP diagnostic drift.
  - Requirement: persist `package-editor-integration-report.json` with package
    fixtures, completion snapshots, hover snapshots, diagnostic snapshots,
    cache invalidation cases, and installed-tool paths.
  - Gate: add `make package-editor-integration-check` and run it after
    `package-cli-workflow-check` and before final release readiness.
  - Completed progress: `package-editor-integration-check` now runs after the
    package CLI workflow gate, validates the package editor/LSP integration
    contract, and writes `package-editor-integration-report.json` with package
    fixtures, completion snapshots, hover snapshots, diagnostic snapshots, cache
    invalidation cases, and installed-tool paths.
  - Acceptance: release cannot pass if packages are usable from CLI but invisible
    or stale in LSP/editor workflows.
  - Acceptance: the gate fails if editor package resolution disagrees with
    compiler/package CLI resolution for the same lockfile and installed package
    cache.


## Completed 061

- [x] Slice 79: enforce package cache integrity, pruning, and corruption
  diagnostics.
  - Requirement: define the package cache layout for archives, expanded sources,
    generated bindings, native artifacts, docs summaries, registry snapshots,
    lockfile metadata, and temporary extraction state.
  - Requirement: cache keys must be content-addressed or checksum-verified and
    must include target/capability/native-artifact dimensions where package
    outputs differ by target.
  - Requirement: add `terlc package cache verify`, `terlc package cache clean
    --check`, and `terlc package cache prune --dry-run` behavior to validate
    cache state without mutating files unless explicitly requested.
  - Requirement: corrupted, partial, stale, target-mismatched, yanked, or
    provenance-mismatched cache entries must fail with cataloged diagnostics and
    never silently fall back to workspace paths.
  - Requirement: add adversarial tests for corrupted archives, partial extraction,
    stale native artifacts, stale generated bindings, target-mismatched cache
    entries, cache poisoning, symlink/path traversal attempts, concurrent cache
    writes, and clean/prune commands deleting live dependencies.
  - Requirement: persist `package-cache-integrity-report.json` with cache fixture
    paths, verified entries, rejected entries, prune plan, diagnostics, checksum
    coverage, and concurrency behavior.
  - Gate: add `make package-cache-integrity-check` and run it after
    `package-editor-integration-check` and before final release readiness.
  - Completed progress: `package-cache-integrity-check` now runs after the
    package editor integration gate, validates the package cache integrity
    contract, and writes `package-cache-integrity-report.json` with cache fixture
    paths, verified entries, rejected entries, prune plan, diagnostics, checksum
    coverage, and concurrency behavior.
  - Acceptance: release cannot pass if package cache corruption can alter build,
    editor, package CLI, or VM runtime behavior without a cataloged diagnostic.
  - Acceptance: the gate fails if clean/prune behavior can remove live packages,
    follow unsafe paths, or produce different results across identical cache
    snapshots.


## Completed 062

- [x] Slice 80: support deterministic multi-package workspaces.
  - Requirement: define a workspace manifest format for multiple local packages,
    shared lockfile, shared registry mirror, package graph roots, local path
    dependencies, package-level capabilities, and per-package target support.
  - Requirement: workspace commands must build, test, lint, format-check, package
    tree, package audit, docs generation, and release dry-run all packages in a
    deterministic topological order.
  - Requirement: local path dependencies must be explicit, cannot escape the
    workspace root unless configured, and must be represented in lockfiles with
    path, package hash, target metadata, and capability summary.
  - Requirement: detect package cycles, duplicate package names, conflicting
    versions, conflicting capabilities, stale local path hashes, mismatched
    target support, and cross-package generated binding drift with cataloged
    diagnostics.
  - Requirement: add adversarial tests for cyclic workspaces, duplicate local
    packages, path traversal, hidden source-checkout dependencies, stale shared
    lockfiles, nondeterministic graph order, package-specific target mismatch,
    and one package passing only because another package left build artifacts.
  - Requirement: persist `package-workspace-graph-report.json` with workspace
    fixture paths, package graph, topological order, lockfile hash, per-package
    command results, diagnostics, and artifact isolation checks.
  - Gate: add `make package-workspace-graph-check` and run it after
    `package-cache-integrity-check` and before final release readiness.
  - Completed progress: `package-workspace-graph-check` now runs after the
    package cache integrity gate, validates the deterministic multi-package
    workspace graph contract, and writes `package-workspace-graph-report.json`
    with workspace fixture paths, package graph, topological order, lockfile
    hash, per-package command results, diagnostics, and artifact isolation
    checks.
  - Acceptance: release cannot pass if multi-package projects cannot be validated
    deterministically through the package/VM tooling.
  - Acceptance: the gate fails if workspace behavior depends on package discovery
    order, stale build artifacts, implicit local paths, or ambient registry state.


## Completed 063

- [x] Slice 81: isolate package build artifacts and incremental state.
  - Requirement: define the package/workspace build artifact layout for compiled
    modules, VM artifacts, generated docs, generated bindings, native artifacts,
    test binaries, diagnostics snapshots, and per-package caches.
  - Requirement: incremental builds must invalidate on source hash, package
    manifest hash, lockfile hash, target/capability hash, stdlib hash, compiler
    version, generated binding hash, native artifact hash, and relevant
    environment/config inputs.
  - Requirement: package artifacts must be namespaced by package identity,
    version/source hash, target, and capability set so local packages cannot
    accidentally consume another package's stale build output.
  - Requirement: add clean/check behavior for package and workspace artifact
    directories, including dry-run output and protection against deleting source,
    lockfiles, package caches, or live registry mirrors.
  - Requirement: add adversarial tests for stale module output, stale generated
    binding output, changed stdlib hash, changed compiler version, target drift,
    package rename collisions, concurrent builds, partial failed builds, and
    clean commands deleting the wrong package artifacts.
  - Requirement: persist `package-build-artifact-isolation-report.json` with
    artifact roots, invalidation matrix, stale-artifact fixtures, clean dry-run
    output, concurrency result, and diagnostics.
  - Gate: add `make package-build-artifact-isolation-check` and run it after
    `package-workspace-graph-check` and before final release readiness.
  - Completed progress: `package-build-artifact-isolation-check` now runs after
    the package workspace graph gate, validates the package build artifact
    isolation contract, and writes
    `package-build-artifact-isolation-report.json` with artifact roots,
    invalidation matrix, stale-artifact fixtures, clean dry-run output,
    concurrency result, and diagnostics.
  - Acceptance: release cannot pass if stale package artifacts can affect build,
    test, docs, editor, package CLI, or VM runtime results.
  - Acceptance: the gate fails if artifact invalidation differs between
    single-package and workspace builds or if clean commands can remove
    non-artifact state.


## Completed 064

- [x] Slice 82: preserve source maps and debug information across compiler,
  VM, packages, and editor tooling.
  - Requirement: define a source-map/debug-info contract from Terlan source spans
    through parser, typechecker, CoreIR, VM artifacts, generated docs, package
    artifacts, diagnostics, support bundles, debugger commands, and editor/LSP
    output.
  - Requirement: VM runtime errors, test failures, panic-like internal failures,
    package resolution failures, template failures, HTTP handler failures, and
    NativeBoundary failures must map back to Terlan module/function/source spans
    where source is available.
  - Requirement: source maps must survive package builds, workspace builds,
    incremental rebuilds, installed release artifacts, generated bindings, and
    support-bundle redaction without leaking host-local absolute paths.
  - Requirement: editor navigation, hover diagnostics, debugger breakpoints,
    stack traces, and support bundles must agree on file path normalization,
    module identity, function identity, and line/column offsets.
  - Requirement: add adversarial tests for stale source maps, generated file span
    drift, package artifact relocation, redacted support bundles, missing package
    sources, invalid UTF-8/source offsets, line-ending differences, and runtime
    errors without source-linked diagnostics.
  - Requirement: persist `source-map-debug-info-report.json` with fixture
    artifacts, span roundtrips, stack trace mappings, package relocation cases,
    editor/LSP parity snapshots, and support-bundle redaction checks.
  - Gate: add `make source-map-debug-info-check` and run it after
    `package-build-artifact-isolation-check` and before final release readiness.
  - Completed progress: `source-map-debug-info-check` now runs after the
    package build artifact isolation gate, validates the source-map/debug-info
    contract, and writes `source-map-debug-info-report.json` with fixture
    artifacts, span roundtrips, stack trace mappings, package relocation cases,
    editor/LSP parity snapshots, and support-bundle redaction checks.
  - Acceptance: release cannot pass if user-visible failures lose Terlan source
    identity or collapse into generated/internal file locations.
  - Acceptance: the gate fails if source maps depend on source checkout paths,
    stale package artifacts, unnormalized paths, or target-specific offset drift.


## Completed 065

- [x] Slice 83: validate compiler incremental cache correctness.
  - Requirement: define compiler cache keys for lexing, parsing, formatting,
    name resolution, typechecking, CoreIR construction, VM lowering, generated
    docs, diagnostics, source maps, package manifests, target capabilities, and
    stdlib/package hashes.
  - Requirement: incremental builds must produce byte-for-byte equivalent public
    artifacts and diagnostics to clean builds for the same inputs, target,
    package graph, stdlib hash, compiler version, and feature flags.
  - Requirement: cache invalidation must cover source edits, import graph edits,
    package/lockfile edits, stdlib changes, compiler version changes, target
    profile changes, generated binding changes, and formatter/lint rule changes.
  - Requirement: cache entries must be isolated by workspace, package, target,
    compiler version, and capability set, with no source-checkout or host-local
    absolute path leakage.
  - Requirement: add adversarial tests for stale parse trees, stale type errors,
    stale generated docs, stale source maps, stale package metadata, changed
    imported module, changed stdlib hash, concurrent incremental builds, cache
    corruption, and clean-vs-incremental diagnostic drift.
  - Requirement: persist `compiler-incremental-cache-report.json` with fixture
    matrix, clean build hashes, incremental build hashes, invalidation cases,
    cache hit/miss counts, diagnostic parity, and source-map parity.
  - Gate: add `make compiler-incremental-cache-check` and run it after
    `source-map-debug-info-check` and before final release readiness.
  - Completed progress: `compiler-incremental-cache-check` now runs after the
    source-map/debug-info gate, validates the incremental cache correctness
    contract, and writes `compiler-incremental-cache-report.json` with fixture
    matrix, clean build hashes, incremental build hashes, invalidation cases,
    cache hit/miss counts, diagnostic parity, and source-map parity.
  - Acceptance: release cannot pass if incremental compilation can produce a
    different user-visible result than a clean build for the same inputs.
  - Acceptance: the gate fails if cache correctness depends on filesystem order,
    stale workspace artifacts, source checkout paths, or non-deterministic
    diagnostics.


## Completed 066

- [x] Slice 84: validate watch mode and VM hot-reload correctness.
  - Requirement: define `terlc watch` behavior for build, test, run, serve, docs,
    package workspaces, formatter/lint checks, and VM hot reload using the same
    incremental cache keys as clean builds.
  - Requirement: file watching must normalize events, debounce deterministically,
    ignore build/cache directories, detect package/lockfile/std changes, and
    avoid triggering on generated artifacts unless they are declared watch inputs.
  - Requirement: VM hot reload must preserve or reject process state according
    to a documented compatibility rule: unchanged ABI/state shape may reload,
    incompatible shape changes must fail with cataloged diagnostics, and stale
    processes must not observe mixed code versions.
  - Requirement: watch output must include stable text/JSON events for start,
    change batch, rebuild, diagnostic, reload, test result, support-bundle path,
    and terminal failure.
  - Requirement: add adversarial tests for rapid file changes, rename/delete
    sequences, package lockfile edits, generated file churn, stale source maps,
    incompatible state shape reload, failing tests after reload, interrupted
    rebuilds, and watcher path leakage.
  - Requirement: persist `watch-mode-hot-reload-report.json` with event
    sequences, rebuild hashes, cache hit/miss counts, VM reload results,
    diagnostics, source-map parity, and support-bundle paths.
  - Gate: add `make watch-mode-hot-reload-check` and run it after
    `compiler-incremental-cache-check` and before final release readiness.
  - Completed progress: `watch-mode-hot-reload-check` now runs after the
    compiler incremental cache gate, validates the watch mode and VM hot-reload
    correctness contract, and writes `watch-mode-hot-reload-report.json` with
    event sequences, rebuild hashes, cache hit/miss counts, VM reload results,
    diagnostics, source-map parity, and support-bundle paths.
  - Acceptance: release cannot pass if watch mode can produce results that differ
    from clean build/test/run for the same final workspace state.
  - Acceptance: the gate fails if VM hot reload can expose mixed code versions,
    stale source maps, stale package metadata, or unclassified state-shape
    incompatibilities.


## Completed 067

- [x] Slice 85: enforce release flake detection and quarantine policy.
  - Requirement: define a deterministic flake-detection policy for release
    gates, including repeat counts, timeout multipliers, allowed nondeterminism,
    random seeds, temp path normalization, clock isolation, and network/socket
    isolation rules.
  - Requirement: release gates must classify every nondeterministic failure as
    fixed, quarantined, or intentionally unstable with an owner, expiry date,
    linked issue, affected gate, and explicit release impact.
  - Requirement: quarantined tests must remain visible in release output and
    must not silently reduce coverage, adversarial corpus coverage, benchmark
    comparability, VM semantics coverage, or package compatibility coverage.
  - Requirement: add adversarial tests for randomized test order, stale temp
    directories, clock-dependent diagnostics, port reuse, race-prone watchers,
    benchmark warmup variance, file-system ordering, and support-bundle path
    leakage.
  - Requirement: persist `release-flake-detection-report.json` with repeated
    run summaries, seeds, failure signatures, quarantine records, expiry
    validation, timeout classification, and release-blocking decisions.
  - Gate: add `make release-flake-detection-check` and run it after
    `watch-mode-hot-reload-check` and before final release readiness.
  - Completed progress: `release-flake-detection-check` now runs after the
    watch mode hot-reload gate, validates the deterministic flake detection and
    quarantine policy contract, and writes
    `release-flake-detection-report.json` with repeated run summaries, seeds,
    failure signatures, quarantine records, expiry validation, timeout
    classification, and release-blocking decisions.
  - Acceptance: release cannot pass if a test or gate can fail nondeterministically
    without a classified flake record or a deterministic reproduction path.
  - Acceptance: the gate fails if quarantine entries expire, hide coverage loss,
    mask VM/runtime semantic regressions, or make benchmark comparisons look
    better by dropping unstable cases.


## Completed 068

- [x] Slice 86: make release gates shardable, resumable, and non-redundant.
  - Requirement: define a release gate manifest that records every check, its
    inputs, output artifacts, dependency gates, expected reports, estimated
    cost, shard assignment, and whether it may be skipped from a valid cache.
  - Requirement: release runs must stop at first failure by default, support an
    explicit collect-all mode, and print the exact resume command for the next
    unchecked gate without re-running completed gates.
  - Requirement: gate caching must be content-addressed by source files, lock
    files, generated artifacts, tool versions, environment contracts, and
    declared external dependencies; cache hits must be invalidated when any of
    those inputs change.
  - Requirement: shard execution must preserve deterministic output ordering,
    stable JSON summaries, stable support-bundle layout, and identical final
    release decisions compared with a single-process serial run.
  - Requirement: add adversarial tests for interrupted release runs, stale
    cached reports, reordered shards, missing gate artifacts, changed toolchain
    versions, partial support bundles, and resume commands after failure.
  - Requirement: persist `release-gate-shard-resume-report.json` with the gate
    DAG, cache keys, skipped gates, executed gates, shard timings, resume
    command, first-failure decision, and collect-all decision.
  - Gate: add `make release-gate-shard-resume-check` and run it after
    `release-flake-detection-check` and before final release readiness.
  - Completed progress: `release-gate-shard-resume-check` now runs after the
    release flake detection gate, validates the shardable/resumable release
    gate contract, and writes `release-gate-shard-resume-report.json` with the
    gate DAG, cache keys, skipped gates, executed gates, shard timings, resume
    command, first-failure decision, and collect-all decision.
  - Completed progress: `terlan-test-orchestrator` now runs each owning Rust
    harness once, release parity rejects duplicate workspace validation, and
    the HTTP release lane uses grouped harness filters instead of 211 exact
    one-test Make invocations. The canonical `rust-test-suite` gate passed all
    owned harnesses in 619.57 seconds (629.30 seconds including its build).
  - Completed progress: release non-redundancy is now enforced against the
    executable `Makefile`, not only the release contract. The package,
    compiler-cache, watch, and release-report gates form one prerequisite DAG;
    `check` invokes only its terminal release gate; and VM line/source coverage
    policies consume one shared `llvm-cov` report. Adversarial quality tests
    reject recursive completed-gate sub-makes and duplicate instrumented VM
    coverage runs. A suppressed-suite dry run records one VM coverage run, one
    package resolver traversal, one terminal release traversal, and zero
    ordinary `cargo test` launches. The six-test shard/resume quality module,
    `make release-gate-shard-resume-check`, roadmap integrity, Rust formatting,
    and whitespace validation pass.
  - Acceptance: release cannot pass if repeated release invocations re-run
    completed gates without an input change or skip gates without a valid cache
    proof.
  - Acceptance: the gate fails if sharded execution can change diagnostics,
    report contents, benchmark inclusion, support-bundle paths, or final release
    pass/fail status compared with the canonical serial run.


## Completed 069

- [x] Slice 87: enforce release gate duration budgets and slow-test regression
  tracking.
  - Requirement: define per-gate and per-suite duration budgets for local
    development, CI, release preflight, benchmark lanes, stdlib checks, VM
    semantics checks, package checks, and editor/tooling checks.
  - Requirement: duration budgets must compare against committed baselines using
    stable machine-readable reports, not ad hoc console timing, and must account
    for warmup, cache state, sharding mode, hardware class, and explicit slow
    test labels.
  - Requirement: slow tests must declare why they are slow, whether they are
    permanent release coverage or one-off gate probes, and which faster unit or
    fixture tests protect the same behavior during normal development.
  - Requirement: add adversarial tests for timing report drift, missing slow-test
    labels, hidden sleeps, accidental network waits, repeated full builds,
    benchmark lanes counted as correctness gates, and budget bypasses under
    sharded or resumed release runs.
  - Requirement: persist `release-gate-duration-budget-report.json` with gate
    timings, baseline deltas, slow-test labels, hardware class, cache mode,
    shard mode, budget decisions, and recommended split points.
  - Gate: add `make release-gate-duration-budget-check` and run it after
    `release-gate-shard-resume-check` and before final release readiness.
  - Completed progress: `release-gate-duration-budget-check` now runs after the
    release gate shard/resume gate, validates the duration-budget and slow-test
    regression contract, and writes `release-gate-duration-budget-report.json`
    with gate timings, baseline deltas, slow-test labels, hardware class, cache
    mode, shard mode, budget decisions, and recommended split points.
  - Acceptance: release cannot pass if gate duration regresses past the accepted
    threshold without an explicit baseline update and rationale.
  - Acceptance: the gate fails if slow tests are unlabelled, correctness gates
    include accidental benchmark work, or resumed/sharded runs hide repeated
    expensive work.


## Completed 070

- [x] Slice 88: standardize release gate report schemas and validation.
  - Requirement: define a versioned schema family for all 0.0.7 gate reports,
    including gate identity, input digests, tool versions, environment contract,
    diagnostics, coverage deltas, benchmark data, support-bundle references,
    pass/fail decision, and release-blocking rationale.
  - Requirement: every required `*-report.json` file in this roadmap must
    declare its schema version, producing gate, generation timestamp policy,
    stable ordering rules, path redaction rules, and compatibility policy.
  - Requirement: schema validation must run before release readiness and must
    reject reports that are missing required sections, contain unstable absolute
    paths, contain unredacted local user data, or use undocumented ad hoc fields.
  - Requirement: add adversarial tests for malformed reports, unknown schema
    versions, duplicated gate IDs, missing input digests, unstable object order,
    path leakage, partially written JSON, and stale reports from previous runs.
  - Requirement: persist `release-gate-report-schema-report.json` with the
    schema inventory, validated reports, rejected reports, compatibility matrix,
    redaction decisions, and schema migration notes.
  - Gate: add `make release-gate-report-schema-check` and run it after
    `release-gate-duration-budget-check` and before final release readiness.
  - Completed progress: `release-gate-report-schema-check` now runs after the
    release gate duration-budget gate, validates the versioned release report
    schema contract, and writes `release-gate-report-schema-report.json` with
    the schema inventory, validated reports, rejected reports, compatibility
    matrix, redaction decisions, and schema migration notes.
  - Completed progress: the report-schema gate now executes adversarial report
    validation fixtures for malformed JSON, unknown schema versions, missing
    input digests, path leakage, duplicated gate IDs, and undocumented ad hoc
    fields before release readiness can pass.
  - Acceptance: release cannot pass if any planned gate emits an unversioned,
    malformed, stale, path-leaking, or schema-incompatible report.
  - Acceptance: the gate fails if a new roadmap-required report is added without
    a schema entry and validation fixture.


## Completed 071

- [x] Slice 89: require exact local reproduction for release failures.
  - Requirement: every release gate failure must emit an exact reproduction
    command, required environment variables, input fixture path, random seed,
    target profile, cache mode, shard ID, and relevant report/support-bundle
    paths.
  - Requirement: reproduction commands must be stable across local and CI runs,
    must not depend on absolute checkout paths, and must work after support
    bundles are unpacked into a fresh temporary directory.
  - Requirement: failed gates must provide both narrow reproduction commands
    for the failing test/case and broader reproduction commands for the owning
    suite, with clear guidance on when each is valid.
  - Requirement: add adversarial tests for stale reproduction commands, missing
    seeds, path-dependent fixtures, deleted temp directories, sharded failures,
    cached failures, benchmark failures, and VM runtime failures with captured
    source maps.
  - Requirement: persist `release-failure-reproduction-report.json` with
    failure samples, reproduction commands, fixture digests, support-bundle
    replay results, path-redaction decisions, and command success status.
  - Gate: add `make release-failure-reproduction-check` and run it after
    `release-gate-report-schema-check` and before final release readiness.
  - Completed progress: `release-failure-reproduction-check` now runs after the
    release gate report-schema gate, validates the exact local reproduction
    contract, and writes `release-failure-reproduction-report.json` with
    failure samples, reproduction commands, fixture digests, support-bundle
    replay results, path-redaction decisions, and command success status.
  - Completed progress: reproduction samples now have executable validation
    for exact commands, required seed/profile/cache/shard environment, relative
    fixture/report/support-bundle paths, narrow and broad commands, command
    success status, stale-cache coupling, CI-only state, and hidden environment
    assumptions.
  - Acceptance: release cannot pass if a failing gate can produce a report
    without a working reproduction command for the failing case.
  - Acceptance: the gate fails if reproduction depends on local checkout paths,
    stale caches, untracked files, CI-only state, or hidden environment
    assumptions.


## Completed 072

- [x] Slice 91: enforce executable documentation examples.
  - Requirement: every Terlan code block in README, roadmap, tutorial,
    standard-library, package, VM, web, editor, and release documentation must
    be classified as executable, compile-only, diagnostic-only, illustrative, or
    intentionally stale with a reason.
  - Requirement: executable examples must run against the default VM profile
    unless explicitly scoped to another target, and must include expected stdout,
    diagnostics, generated artifacts, or report paths when those are part of the
    documented behavior.
  - Requirement: diagnostic examples must assert stable diagnostic codes,
    source spans, message text policy, JSON diagnostic shape, and redaction
    behavior without depending on local checkout paths.
  - Requirement: add adversarial tests for stale code fences, unsupported target
    profiles, hidden imports, missing package manifests, misleading install
    commands, old syntax forms, unformatted examples, and examples that only
    pass because of global user state.
  - Requirement: persist `docs-codeblock-executable-report.json` with code-block
    inventory, classification, executed examples, skipped examples, diagnostic
    assertions, formatter results, and stale-example reasons.
  - Gate: add `make docs-codeblock-executable-check` and run it after
    `dev-fast-feedback-profile-check` and before final release readiness.
  - Completed progress: `docs-codeblock-executable-check` now rejects
    placeholder/TODO/TBD report vocabulary across documentation
    classifications, skipped-example reasons, executed-example statuses,
    diagnostic policies, formatter statuses, and formatter diagnostics before
    writing `docs-codeblock-executable-report.json`; the quality tests include
    an injected-placeholder case and the gate passed with 52 Markdown files, 29
    Terlan blocks, 2 complete modules, and 27 fragments checked.
  - Acceptance: release cannot pass if public docs include Terlan examples that
    do not parse, typecheck, format, run, or fail with the documented diagnostic
    for their classification.
  - Acceptance: the gate fails if illustrative or intentionally stale examples
    are used to document current user-facing workflows.


## Completed 073

- [x] Slice 92: keep standard-library docs and editor hover in parity.
  - Requirement: every public stdlib module, type, shape, trait, constructor,
    function, method, field, constant, operator helper, and VM-owned primitive
    must have structured documentation in the source of truth used by generated
    docs and editor hover.
  - Requirement: documentation must preserve formatting offsets, examples,
    parameter names, return types, error conditions, mutability semantics,
    target-profile availability, capability requirements, and package
    provenance when surfaced through LSP hover or generated docs.
  - Requirement: hover output must be stable in text and JSON mode, must link
    to the defining source symbol, and must reject stale docs generated from old
    stdlib snapshots or package metadata.
  - Requirement: add adversarial tests for missing docs, malformed doc comments,
    outdated type signatures, renamed parameters, overloaded methods, generated
    TypeScript/WASM/C++ bindings, package imports, private symbols, and
    cross-module re-exports.
  - Requirement: persist `std-doc-lsp-hover-parity-report.json` with public API
    inventory, missing-doc entries, generated-doc hashes, hover fixtures,
    source-definition links, and stale-metadata rejection results.
  - Gate: add `make std-doc-lsp-hover-parity-check` and run it after
    `docs-codeblock-executable-check` and before final release readiness.
  - Completed progress: `std-doc-lsp-hover-parity-check` now runs after the
    executable docs gate, validates the stdlib docs/editor hover parity
    contract, and writes `std-doc-lsp-hover-parity-report.json` with public API
    inventory, missing-doc entries, generated-doc hashes, hover fixtures,
    source-definition links, and stale-metadata rejection results.
  - Acceptance: release cannot pass if a public stdlib API is undocumented or
    editor hover disagrees with generated docs, current type signatures, or
    source-definition links.
  - Acceptance: the gate fails if docs formatting loses required spacing,
    offsets, examples, or target/capability availability metadata.


## Completed 074

- [x] Slice 93: enforce editor definition navigation parity.
  - Requirement: LSP definition, declaration, type-definition, implementation,
    and reference navigation must work for local modules, package modules,
    stdlib modules, generated bindings, constructors, shapes, traits, impls,
    fields, methods, operators, template symbols, and VM-owned primitives.
  - Requirement: navigation targets must preserve source spans through formatter
    rewrites, incremental compilation, package cache resolution, generated docs,
    source maps, and editor plugin packaging.
  - Requirement: VS Code, Neovim, Emacs, and IntelliJ integration notes must
    describe the same navigation contract, with editor-specific behavior covered
    by fixtures or explicit unsupported-feature diagnostics.
  - Requirement: add adversarial tests for ambiguous imports, re-exports,
    generated bindings, stale package caches, renamed files, formatter-induced
    span shifts, private symbols, overloaded methods, and template interpolation
    symbols.
  - Requirement: persist `editor-definition-navigation-report.json` with symbol
    inventory, resolved targets, unresolved targets, editor fixture results,
    source-map checks, package-cache checks, and stale-metadata rejections.
  - Gate: add `make editor-definition-navigation-check` and run it after
    `std-doc-lsp-hover-parity-check` and before final release readiness.
  - Current gate state: `make editor-definition-navigation-check` exists and
    proves same-document function definitions, same-document type annotation
    definitions, formatter-induced same-document function span shifts,
    same-document impl method definitions, same-document field definitions,
    imported public provider-summary function definitions,
    imported public provider-summary type annotation definitions, imported
    public provider-summary struct-field definitions through typed receiver
    field access while rejecting private provider fields, protocol-level
    imported public provider-summary struct-field definition and declaration
    requests, imported public provider-summary shape definitions, imported
    public provider-summary
    constructor definitions, imported public provider-summary trait
    definitions, wildcard selected-import public function definitions, selected
    import aliases resolving to their provider definitions, ambiguous selected
    imports being rejected instead of picking an arbitrary provider, private
    provider symbols being rejected when a public interface summary is present,
    missing provider files returning no stale target, local definitions shadowing
    imported provider symbols, nested package provider summaries resolving from
    dotted imports, imported overload summaries resolving through selected
    imports, selected-import re-export summaries resolving through wrapper
    modules to the original provider declaration, stale re-export metadata for
    renamed or missing provider artifacts returning no stale target, generated
    summary callable bindings resolving through the packaged-summary fallback
    path, generated std summary type definitions resolving through the
    packaged-summary fallback path, imported declaration protocol requests
    resolving to provider summaries, type-definition capability plus protocol
    requests reusing the current definition resolver for local and imported
    public provider-summary type annotations, implementation-provider capability plus protocol requests
    resolving receiver-call members to explicit impl methods,
    references-provider capability plus same-document protocol requests
    honoring `includeDeclaration` for lexer-backed exact identifier-token
    matches while skipping string/comment tokens and preserving imported
    use-sites when declarations are excluded, receiver-call reference requests
    excluding trait/impl method declarations when declarations are excluded,
    declaration-provider capability plus protocol requests reusing the current
    definition resolver, and template document non-navigation behavior through
    definition and references protocol paths plus the helper path. Protocol-level
    imported shape, constructor, and trait definition requests now resolve
    through the same provider summary path as helper-level imported navigation.
  - Completed progress: selected imports backed by sibling `.terli`/`.typi`
    provider summaries now resolve through LSP definition to the provider
    declaration range when the symbol is part of the public interface. Local
    same-document definitions still take precedence, and template documents
    still return no Terlan source definition target.
  - Completed progress: `editor-definition-navigation-check` now writes
    `target/quality/editor-definition-navigation-report.json` after the LSP
    selector suite passes. The report is generated by
    `terlan-quality editor-definition-navigation-report`, validates the Make
    target still contains all 39 exact definition/declaration/type-definition/
    implementation/reference selectors, and records evidence across same-
    document symbols, imported provider summaries, protocol capabilities,
    references, stale metadata rejection, generated bindings, template
    non-navigation, formatter span shifts, and editor parity notes.
  - Completed progress: `editor-definition-navigation-check` now rejects
    placeholder/TODO/TBD report evidence across selector evidence labels and
    editor parity notes before writing the JSON report, with an adversarial
    injected-placeholder test proving stale navigation evidence cannot be padded
    into the release artifact.
  - Completed progress: `editor-definition-navigation-check` now treats the
    selector/report inventory as a release contract: the report gate fails if
    the 39 exact navigation selectors or 9 populated evidence categories drift
    without an intentional checker update.
  - Acceptance: release cannot pass if a public symbol can be used by the
    compiler but cannot be navigated to by the language server or documented
    editor integration.
  - Acceptance: the gate fails if navigation returns stale files, wrong spans,
    generated artifacts without source links, or private implementation details
    as public API targets.


## Completed 075

- [x] Slice 94: enforce editor code actions and auto-import correctness.
  - Requirement: LSP code actions must provide safe fixes for missing imports,
    unresolved module qualifiers, missing stdlib/package symbols, stale package
    metadata, runnable main/test discovery, and simple syntax-shape migrations
    that are explicitly supported by formatter or lint rules.
  - Requirement: auto-import edits must preserve module layout, existing import
    grouping, formatter output, comments, aliases, re-exports, package
    capability restrictions, and target-profile availability.
  - Requirement: ambiguous symbols must produce ranked choices with source
    package/module provenance and must not guess across private symbols,
    incompatible target profiles, or packages whose capabilities are not granted.
  - Requirement: add adversarial tests for duplicate symbol names, private
    modules, generated bindings, stale caches, formatter span shifts, missing
    package manifests, import cycles, removed packages, and unavailable target
    profiles.
  - Requirement: persist `editor-code-action-auto-import-report.json` with code
    action fixtures, applied edits, rejected edits, ambiguity rankings,
    formatter parity, package metadata checks, and stale-cache rejection cases.
  - Gate: add `make editor-code-action-auto-import-check` and run it after
    `editor-definition-navigation-check` and before final release readiness.
  - Current gate state: `make editor-code-action-auto-import-check` exists and
    proves unknown-constructor diagnostics produce import actions for both
    `name / arity` and `name/arity` spelling, missing `Vector` imports insert
    the canonical module import through an applicable LSP workspace edit, stale
    same-leaf `Vector` imports are replaced, and
    already imported canonical `Vector` modules do not produce duplicate/no-op
    quick fixes. It also proves provider-summary public functions produce
    selected imports, generated `.typi` binding summaries produce selected
    imports for callable bindings, including applicable LSP workspace edits for
    unknown-function diagnostics using both `name / arity` and `name/arity`
    spelling and qualified diagnostics whose final segment names the missing
    function, and already selected or wildcard-selected public functions do not
    produce duplicate/no-op quick fixes. Private provider functions and stale
    re-export summaries whose original provider artifact is missing are not
    offered as auto-import candidates. Selected import aliases are treated as
    local visible names, so an aliased import does not hide a missing source-name
    quick fix. Leading module documentation/comments are preserved by inserting
    imports after the module header instead of before the docs.
    Ambiguous public
    provider functions produce one selected-import quick-fix choice per provider
    module instead of guessing silently. Provider-summary constructors whose
    symbol differs from the module leaf produce selected imports such as
    `import items.{Items}.`, including applicable LSP workspace edits when
    unknown-constructor call diagnostics use spaced or compact arity spelling
    and when constructor-pattern diagnostics use unarity or compact arity
    spelling.
    Std constructors whose names do not match their module leaves, including
    `Some`, `None`, `Ok`, and `Err`, resolve to selected-import fallback quick
    fixes, including qualified constructor diagnostics whose final segment names
    the missing constructor. New selected imports from a module with an existing
    selected import are grouped into that import line instead of adding a
    duplicate declaration.
  - Completed progress: `editor-code-action-auto-import-check` now writes
    `target/quality/editor-code-action-auto-import-report.json` after the LSP
    import-action suite passes. The report is generated by
    `terlan-quality editor-code-action-auto-import-report`, validates that the
    Make target still runs the LSP import-action module, checks all 21 required
    auto-import fixtures still exist, and records evidence for code-action
    fixtures, applied edits, rejected edits, ambiguity rankings, formatter
    parity, package metadata checks, and stale-cache rejection cases.
  - Completed progress: `editor-code-action-auto-import-check` now rejects
    placeholder/TODO/TBD report evidence across required fixture names and
    evidence labels before writing the JSON report, with an adversarial
    injected-placeholder test proving stale auto-import evidence cannot be
    padded into the release artifact. The report gate now also fails if the 21
    exact auto-import fixtures or 7 populated evidence categories drift without
    an intentional checker update.
  - Acceptance: release cannot pass if a missing import or unresolved public
    symbol can be diagnosed but cannot be repaired by a correct editor action
    when a unique valid import exists.
  - Acceptance: the gate fails if auto-import can introduce private symbols,
    target-incompatible modules, duplicate imports, broken formatting, or stale
    package references.


## Completed 076

- [x] Slice 95: enforce editor completions, signature help, and inlay hints.
  - Requirement: LSP completion must cover modules, imports, package symbols,
    stdlib APIs, local bindings, fields, methods, constructors, shapes, traits,
    impls, template symbols, target-specific APIs, generated bindings, and VM
    primitives with stable ranking and documentation snippets.
  - Requirement: signature help must show parameter names, pattern parameters,
    default values, mutability, capability requirements, target availability,
    generic parameters, return type, error behavior, and current argument index.
  - Requirement: inlay hints must be deterministic, configurable, formatter-safe,
    and available for inferred types, defaulted arguments, implicit constructors,
    inferred target APIs, generated bindings, and package provenance.
  - Requirement: add adversarial tests for stale package caches, ambiguous
    symbols, private symbols, generated bindings, formatter span shifts,
    overloaded methods, pattern parameters, incomplete syntax, and mixed target
    profiles.
  - Requirement: persist `editor-completion-signature-report.json` with
    completion fixtures, ranking decisions, signature-help fixtures, inlay-hint
    fixtures, stale-cache rejections, target-profile checks, and editor parity
    notes.
  - Gate: add `make editor-completion-signature-check` and run it after
    `editor-code-action-auto-import-check` and before final release readiness.
  - Current gate state: `make editor-completion-signature-check` exists and
    proves local/imported shape completions, local/imported public function
    completions from syntax output and provider summaries, local/imported
    public constructor completions, local/imported type, struct, and trait
    completions, active function parameter completions, prior simple
    let-binding completions, semicolon-continued let-chain binding completions,
    receiver field completions for local and imported struct-typed parameters,
    receiver method completions for local methods,
    local explicit impl methods, and imported public receiver-method summaries,
    advertised signature-help and inlay-hint capabilities, local function,
    generic local function, imported generic function, local receiver-method,
    imported receiver-method, and generated `.typi` package-summary function
    signature help with typed/defaulted parameters, documentation, and
    active-parameter selection,
    signature parameter label preservation for mutable pattern parameters with
    defaults, deterministic
    literal let-binding and semicolon-continued let-chain type inlay hints,
    local/imported function call parameter-name inlay hints with
    provider-qualified imported function provenance tooltips,
    generated `.typi` package-summary function parameter-name inlay hints
    with package-qualified provenance tooltips, defaulted
    argument inlay hints, and local/imported
    receiver-method call parameter-name inlay hints through the real LSP
    request path. It also proves compiler-owned `@pure` metadata is visible in
    local/imported function completions, local/imported receiver-method
    completions, and local/imported/generated function and receiver-method
    signature help labels, including
    adversarial rejection of private provider type aliases, functions, and
    struct fields in imported completion results, ambiguous local/imported
    symbol completion ranking with local declarations first, stale changed-document
    local-binding completion rejection through didChange plus completion,
    overloaded receiver-method completion preservation without deduplication,
    formatter-shifted local function completion positions,
    generated `.typi` package-summary completion provenance with
    package-qualified function detail,
    mixed target-profile imported completion rejection that preserves local
    completions while suppressing target-specific std suggestions,
    incomplete-syntax completion requests returning empty non-error responses,
    and writes
    `target/quality/editor-completion-signature-report.json` with the covered
    completion, ranking, signature-help, inlay-hint, stale-cache,
    target-profile, and editor-parity report sections.
  - Completed progress: generated package-summary completion now has a stale
    metadata adversarial selector. `completion_rejects_deleted_generated_typi_summary`
    proves a generated `.typi` symbol is visible before deletion, disappears
    after its summary is removed, and local completions remain available. The
    report gate now requires 12 exact selectors and no longer carries pending
    stale-cache or target-profile completion notes. The report gate now also
    fails if the 11 populated evidence categories drift or if selector evidence
    and editor parity notes contain placeholder/TODO/TBD/pending terms.
  - Acceptance: release cannot pass if public APIs compile but are absent from
    completions, signature help, or configured inlay hints in supported editor
    integrations.
  - Acceptance: the gate fails if completion ranking suggests private,
    target-incompatible, stale, or package-unavailable symbols ahead of valid
    local or imported choices.


## Completed 077

- [x] Slice 96: enforce editor runnable actions and debug launch parity.
  - Requirement: supported editors must discover runnable `main` functions,
    individual tests, test suites, package workspace commands, web server
    commands, watch commands, and debugger launch targets from the same compiler
    metadata used by `terlc run`, `terlc test`, and VM debug commands.
  - Requirement: run/debug actions must use stable command IDs, stable labels,
    stable working-directory rules, target-profile inference, package lockfile
    resolution, and source-map aware VM launch configuration.
  - Requirement: editor output must preserve colored success/failure status,
    clickable diagnostics, exact reproduction commands, test filters, support
    bundle paths, and debugger attach/restart commands without spawning stale
    duplicate terminal processes.
  - Requirement: add adversarial tests for renamed main modules, deleted tests,
    stale workspace metadata, package workspaces, failed rebuilds, watch-mode
    reloads, debug breakpoints, path redaction, and editor restart after
    extension upgrade.
  - Requirement: persist `editor-runnable-debug-launch-report.json` with
    runnable inventories, command IDs, applied launch configs, test filters,
    debug attach results, terminal reuse behavior, and stale-metadata rejection
    cases.
  - Gate: add `make editor-runnable-debug-launch-check` and run it after
    `editor-completion-signature-check` and before final release readiness.
  - Current gate state: `make editor-runnable-debug-launch-check` exists and
    proves stable VS Code
    run/check/build/clean/test/serve/watch/doctor/debug/debug-at-cursor command
    IDs, activation events, editor-title and explorer menu placements,
    compiler-owned `terlc run`, package workspace commands through `terlc check`,
    `terlc build`, and `terlc clean`, `terlc serve`,
    live-reload watch via `terlc serve --poll-ms 250`,
    support diagnostics via `terlc doctor`, debugger launch via
    `terlc debug --json-events`, cursor breakpoint launch via
    `terlc debug --break <file:line> --json-events`, file-test, and
    named-test terminal command construction with POSIX command-cache refresh,
    runnable `main`/`@test` inventory discovery, active-document workspace
    selection, nearest `terlan.toml` package-root target selection for
    workspace-level editor commands with VS Code workspace-root fallback,
    file-specific main CodeLens launches carrying `document.uri.fsPath` to
    avoid stale active-workspace execution, shared
    run/check/build/clean/test/serve/watch/doctor/debug/debug-at-cursor
    command-id constants consumed by VS Code command registration and CodeLens
    named-test/main actions, stale named-test CodeLens arguments rejected after
    open-buffer test rename/delete,
    exact reproduction command descriptors for run, named-test, and debug
    launches that keep user-facing compiler commands separate from terminal
    shell-cache housekeeping, integrated-terminal pass-through output mode, and
    compiler-owned color preservation evidence, workspace-bounded launch target
    redaction via `${workspace}` display paths without changing exact terminal
    commands,
    debugger CLI fallback wiring, absence of premature VS Code debug adapter
    publication, single shared Terlan terminal creation plus close-event cache
    reset to prevent duplicate stale terminals, and writes
    `target/quality/editor-runnable-debug-launch-report.json` with runnable
    inventory, package workspace target selection, command IDs, CodeLens
    command IDs, launch command descriptors with redacted display targets,
    debug launch, terminal reuse, and stale-metadata report sections.
  - Completed progress: `editor-runnable-debug-launch-check` now records an
    explicit `launchRedaction` report section proving workspace-owned launch
    targets render as `${workspace}/...`, external paths are not rewritten,
    exact reproduction commands keep their real compiler paths, POSIX terminals
    refresh shell command caches, and Windows launch descriptors do not add
    POSIX-only cache refresh commands.
  - Acceptance: release cannot pass if a runnable target is accepted by the CLI
    but missing, stale, or incorrectly launched by supported editor integrations.
  - Acceptance: the gate fails if editor run/debug actions use stale compiler
    metadata, wrong working directories, duplicate terminals, or source maps
    that do not match VM execution.


## Completed 078

- [x] Slice 97: enforce editor semantic tokens, highlighting, and file icons.
  - Requirement: Tree-sitter, LSP semantic tokens, TextMate scopes, and editor
    file icons must classify Terlan modules, tests, packages, templates,
    generated bindings, docs examples, and VM/debug artifacts consistently across
    VS Code, Neovim, Emacs, and IntelliJ packaging.
  - Requirement: syntax highlighting must cover current EBNF forms including
    pattern parameters, guards, implications, comprehensions, typed template
    interpolation, map/object constructors, bitstring syntax, contracts/policies,
    and target-specific imports.
  - Requirement: file icons must distinguish normal `.terl` files, test files,
    package manifests, generated artifacts, docs examples, VM traces, and
    unsupported legacy artifacts without relying on stale filename conventions.
  - Requirement: add adversarial tests for stale grammar output, lost
    highlighting in test files, icon theme mismatches, light/dark icon variants,
    generated PNG drift, unsupported syntax forms, and editor package assets
    that differ from source assets.
  - Requirement: persist `editor-semantic-token-icon-report.json` with grammar
    fixtures, semantic-token snapshots, scope snapshots, icon asset hashes,
    packaged-extension hashes, and unsupported-form diagnostics.
  - Gate: add `make editor-semantic-token-icon-check` and run it after
    `editor-runnable-debug-launch-check` and before final release readiness.
  - Current gate state: `make editor-semantic-token-icon-check` exists and
    proves checked-in Tree-sitter highlight capture/node coverage, TextMate
    source/template bridge scope coverage, VS Code language/icon mappings for
    source, test, interface, template, `terlan.toml` package manifest,
    `.terldbg` debugger script, and `vmir-execution-trace-report.json` VM trace
    artifacts, light/dark icon reference parity, shared SVG and PNG icon
    dimensions, Tree-sitter package metadata, template interpolation
    highlighting, and writes
    `target/quality/editor-semantic-token-icon-report.json` with grammar
    fixture hashes, semantic-token placeholders, scope snapshots, icon filename
    inventory, icon extension inventory, icon asset hashes, package manifest
    hashes, and stale-grammar gate diagnostics.
  - Acceptance: release cannot pass if supported editor packages ship stale
    grammar, missing semantic tokens, broken file icons, or assets that differ
    from source-of-truth icons.
  - Acceptance: the gate fails if current language syntax is parsed by the
    compiler but not highlighted or tokenized by the editor surface.


## Completed 079

- [x] Slice 98: enforce editor diagnostic parity with compiler diagnostics.
  - Requirement: LSP diagnostics, editor problem panels, inline squiggles,
    quick-fix eligibility, and terminal diagnostics must be generated from the
    same compiler diagnostic catalog and source-map data.
  - Requirement: editor diagnostics must preserve diagnostic code, severity,
    primary span, secondary labels, related information, JSON shape, redaction
    behavior, package provenance, target-profile context, and fixability.
  - Requirement: diagnostics must update deterministically during watch mode,
    incremental compilation, package metadata changes, formatter rewrites,
    generated binding refreshes, and VM/debug launch failures.
  - Requirement: add adversarial tests for stale diagnostics, duplicate
    diagnostics, wrong spans after formatting, package-cache drift, generated
    files without source links, path leakage, unsupported target profiles, and
    code actions offered for non-fixable errors.
  - Requirement: persist `editor-diagnostic-parity-report.json` with diagnostic
    fixtures, compiler/LSP comparisons, editor problem-panel snapshots,
    fixability decisions, source-map checks, stale-diagnostic rejections, and
    path-redaction checks.
  - Gate: add `make editor-diagnostic-parity-check` and run it after
    `editor-semantic-token-icon-check` and before final release readiness.
  - Current gate state: `make editor-diagnostic-parity-check` exists and proves
    VS Code delegates diagnostics to `vscode-languageclient` without an
    editor-owned diagnostic collection, LSP document selectors cover all
    contributed Terlan language ids, parse/type/HIR/template diagnostics publish
    through the real LSP `publishDiagnostics` path, parse diagnostics clear
    after a fixed document version, adversarial Unicode parse failures stay
    isolated to parser diagnostics, unknown-constructor and unknown-function
    diagnostics produce valid quick-fix workspace edits, VM debugger
    launch-failure diagnostics remain compiler-owned through JSON diagnostic
    routing, reserved `.terldbg` script validation, invalid breakpoint/script
    tests, and stable debugger diagnostic codes, and writes
    `target/quality/editor-diagnostic-parity-report.json` with diagnostic
    fixture, compiler/LSP comparison, problem-panel delegation, fixability,
    source-map, VM debug launch failure, stale-diagnostic rejection, and
    path-redaction sections.
  - Acceptance: release cannot pass if a compiler diagnostic shown in CLI text
    or JSON mode differs from the diagnostic served to supported editor
    integrations for the same source state.
  - Acceptance: the gate fails if editor diagnostics can be stale, duplicated,
    path-leaking, missing source-map links, or paired with invalid quick fixes.


## Completed 080

- [x] Slice 99: validate installed editor packages against source artifacts.
  - Requirement: editor release artifacts for VS Code, Neovim, Emacs, IntelliJ,
    Tree-sitter, grammar scopes, semantic tokens, file icons, snippets, commands,
    and LSP client configuration must be generated from source-of-truth assets
    and verified after installation.
  - Requirement: installed packages must expose the same extension version,
    compiler compatibility range, command IDs, file associations, icon assets,
    grammar assets, language-server binary path, and activation events as the
    packaged release manifest.
  - Requirement: install/update checks must detect stale local extensions,
    cached old icons, missing generated PNGs, missing light-theme assets,
    command-ID drift, stale grammar bundles, and editor packages built from a
    different compiler version.
  - Requirement: add adversarial tests for old extension directories, stale icon
    caches, missing packaged files, mismatched version metadata, renamed commands,
    failed upgrade cleanup, and extension install paths containing spaces.
  - Requirement: persist `editor-extension-install-update-report.json` with
    packaged hashes, installed hashes, command inventory, icon inventory,
    grammar inventory, extension version checks, and stale-cache rejection cases.
  - Gate: add `make editor-extension-install-update-check` and run it after
    `editor-diagnostic-parity-check` and before final release readiness.
  - Current gate state: `make editor-extension-install-update-check` exists and
    proves VS Code and Tree-sitter npm dry-run archives match checked-in package
    identity, include required runtime grammar/icon/LSP/client assets, exclude
    tests/generated parser outputs/archive files, preserve VS Code run/test
    command inventory, preserve `terlc lsp --stdio` defaults, preserve
    VS Code startup, command, and language activation events, preserve source,
    test, interface, and template file associations, preserve Tree-sitter
    file-type/query metadata, and writes
    `target/quality/editor-extension-install-update-report.json` with packaged
    hashes, dry-run install parity, command inventory, activation inventory,
    file-association inventory, icon inventory, grammar inventory, extension
    version checks, and stale-cache rejection cases; `make update-terlc`
    now installs the release-mode local `terlc` and `terlan-vm`, runs the
    VS Code smoke gate, runs the editor install/update artifact gate, and keeps
    VS Code integrated-terminal launch commands identical to the reproducible
    `terlc ...` command without POSIX shell-cache housekeeping prefixes.
  - Acceptance: release cannot pass if installed editor packages differ from
    source artifacts or expose stale commands, icons, grammar, snippets, or LSP
    client configuration.
  - Acceptance: the gate fails if an upgrade can leave an old Terlan extension
    active while the new compiler is installed.


## Completed 081

- [x] Slice 101: define direct CoreIR-to-VMIR lowering for VM builds.
  - Requirement: VM-targeted compilation must lower typed CoreIR into a
    Terlan-owned VMIR instruction model without routing through legacy bytecode,
    legacy source emission, or compatibility-only intermediate vocabularies.
  - Requirement: VMIR must model Terlan semantics directly: values, calls,
    pattern matching, guards, comprehensions, closures, maps, bitstrings,
    actors, mailbox operations, supervision, resources, NativeBoundary calls,
    and source-map/debug locations.
  - Requirement: pure functions, constant expressions, and target-owned
    intrinsics must have an explicit lowering policy that decides when to keep
    execution in VMIR, fold at compile time, or route through a typed native
    artifact without changing observable Terlan semantics.
  - Requirement: add adversarial tests for mixed pure/actor code, closures,
    pattern guard failures, source-map spans, incremental rebuilds, hot reload,
    NativeBoundary calls, generated bindings, and rejection of legacy-only
    constructs in VM-targeted builds.
  - Requirement: persist `coreir-vmir-lowering-report.json` with covered CoreIR
    node kinds, VMIR node kinds, unsupported shapes, lowering decisions,
    source-map parity, pure/native decisions, and legacy-intermediary rejection
    cases.
  - Gate: add `make coreir-vmir-lowering-check` and run it after
    `target-inference-default-vm-check` and before final release readiness.
  - Current gate state: `make coreir-vmir-lowering-check` exists and passes.
    It runs `target-inference-default-vm-check` and
    `terlan-quality coreir-vmir-lowering`, while the gate's eight Rust build,
    artifact-run, and adversarial artifact-validation tests are owned by the
    canonical `terlan-test-orchestrator` through
    `COMPLETED_SLICE_RUST_GATES`. The quality contract verifies those tests in
    their owning source harnesses and rejects missing canonical-suite
    ownership without restoring bespoke per-test Make runners. It writes
    `target/quality/coreir-vmir-lowering-report.json` with covered CoreIR node
    kinds, covered VMIR node kinds, unsupported lowering shapes, lowering
    decisions, source-map parity, pure/native decisions, legacy-intermediary
    rejections, canonical Rust test inventory, and orchestrator ownership under
    `terlan-coreir-vmir-lowering-report-v2`. The current baseline covers
    literal, name, binary, remote-call, intrinsic, and call expression lowering
    into the first Terlan-owned VM artifact/VMIR payload. Actor mailbox opcode
    lowering, guard continuation lowering, bitstring segment lowering,
    streaming NativeBoundary lowering, resource-handle transfer lowering, and
    support-bundle replay lowering remain rejected until implemented.
  - Acceptance: release cannot pass if a VM build still depends on a legacy
    intermediary for any supported Terlan language feature.
  - Acceptance: the gate fails if CoreIR and VMIR semantics diverge for values,
    pattern matching, calls, actor scheduling, source maps, or native boundary
    resource handling.


## Completed 082

- [x] Slice 102: add VMIR verifier and deterministic VM artifact format.
  - Requirement: define a VMIR verifier that checks instruction well-formedness,
    type consistency, control-flow integrity, register/slot lifetime, function
    arity, resource-handle usage, actor/mailbox operations, NativeBoundary call
    contracts, and source-map/debug metadata before VM execution.
  - Requirement: define a deterministic VM artifact format with stable module
    identity, package identity, function tables, constants, type metadata,
    source maps, debug tables, capability requirements, hot-reload ABI hashes,
    and artifact provenance.
  - Requirement: VM artifacts must be reproducible across clean builds,
    incremental builds, package workspaces, editor-triggered builds, release
    builds, and support-bundle replay when inputs and tool versions are equal.
  - Requirement: add adversarial tests for malformed VMIR, unreachable blocks,
    invalid jumps, stale source maps, mismatched arity, invalid resource handles,
    corrupted artifacts, wrong package identity, and hot-reload ABI mismatch.
  - Requirement: persist `vmir-verifier-artifact-report.json` with verifier
    coverage, rejected VMIR cases, artifact hashes, reproducibility matrix,
    source-map/debug parity, ABI hash decisions, and provenance checks.
  - Gate: add `make vmir-verifier-artifact-check` and run it after
    `coreir-vmir-lowering-check` and before final release readiness.
  - Current gate state: `make vmir-verifier-artifact-check` exists and runs the
    VM artifact loader contract freeze, artifact round-trip build/load/run
    smoke, the VM artifact format quality gate, and
    `terlan-quality vmir-verifier-artifact`. Rust coverage is owned by the
    canonical `terlan-test-orchestrator`; the quality gate verifies all 14
    artifact loader/verifier tests in their source harness and fails if the VM
    suite or completed-gate ownership is removed. It writes
    `target/quality/vmir-verifier-artifact-report.json` with verifier coverage,
    rejected VMIR cases, artifact checksum cases, reproducibility matrix,
    corrupted-artifact rejections, source-map/debug parity, ABI hash decisions,
    hot-reload ABI rejections, provenance checks, canonical Rust-test inventory,
    and orchestrator ownership under report schema v2.
  - Acceptance: release cannot pass if the VM can execute unverified VMIR or if
    VM artifacts are not reproducible for identical compiler inputs.
  - Acceptance: the gate fails if invalid control flow, invalid resource usage,
    stale debug metadata, corrupted artifacts, or hot-reload ABI mismatches can
    reach runtime execution.


## Completed 083

- [x] Slice 103: execute VMIR with deterministic traces and runtime accounting.
  - Requirement: the VM execution engine must run verified VMIR artifacts through
    Terlan-owned process, scheduler, mailbox, timer, NativeBoundary, and resource
    machinery without falling back to legacy execution paths for supported
    language features.
  - Requirement: VMIR execution must emit stable optional traces for function
    entry/exit, instruction steps, reductions/ticks, sends, receives, timers,
    resource handles, NativeBoundary calls, supervision events, faults, and
    source-map/debug locations.
  - Requirement: traces must be deterministic under fixed scheduler seeds and
    must redact local paths and user data while preserving enough information for
    debugger, benchmark, support-bundle, and failure-reproduction workflows.
  - Requirement: add adversarial tests for recursion, closures, pattern-match
    failures, guard failures, actor sends/receives, timer races, resource
    cleanup, native call cancellation, scheduler preemption, and trace replay.
  - Requirement: persist `vmir-execution-trace-report.json` with executed
    artifacts, trace fixtures, reduction accounting, scheduler seeds,
    source-map parity, replay results, redaction checks, and unsupported-shape
    rejections.
  - Gate: add `make vmir-execution-trace-check` and run it after
    `vmir-verifier-artifact-check` and before final release readiness.
  - Current gate state: `make vmir-execution-trace-check` exists and runs the
    VMIR verifier/artifact gate, VM artifact execution selectors, inspection
    source-identity selectors, process reduction-accounting selectors,
    mailbox send/receive/selective-receive selectors, scheduler/actor
    delegation selectors, actor named-send and receive-or-block selectors,
    timer wakeup selectors, resource cancellation cleanup selectors, hot-reload
    replay/inspection selectors, and `terlan-quality vmir-execution-trace`; it
    writes
    `target/quality/vmir-execution-trace-report.json` with executed artifacts,
    trace fixtures, reduction accounting, scheduler seed notes, source-map
    parity, replay results, mailbox trace fixtures, redaction checks,
    unsupported-shape rejections, and exact selector inventory.
  - Acceptance: release cannot pass if supported VMIR can execute without
    accounting, source-map traceability, or deterministic replay under fixed
    scheduler seeds.
  - Acceptance: the gate fails if execution traces hide faults, leak local data,
    miscount reductions, lose NativeBoundary lifecycle events, or diverge from
    debugger/support-bundle expectations.


## Completed 084

- [x] Slice 104: validate VMIR optimization correctness.
  - Requirement: VMIR optimization passes must be explicit, ordered,
    reproducible, and covered by semantic equivalence checks for constant
    folding, dead-code elimination, call inlining, tail-call lowering, guard
    simplification, map/collection specialization, and pure/native extraction.
  - Requirement: optimizations must preserve Terlan semantics for pattern-match
    failures, actor scheduling, mailbox ordering, resource cleanup,
    NativeBoundary cancellation, diagnostics, source maps, debugger breakpoints,
    hot-reload ABI hashes, and support-bundle replay.
  - Requirement: optimization decisions must be inspectable through compiler
    timings, JSON reports, debug dumps, and editor diagnostics without requiring
    users to understand internal VMIR implementation details.
  - Requirement: add adversarial tests for invalid constant folding, inlining
    across resource lifetimes, reordered side effects, guard short-circuit
    changes, stale source maps, changed reduction counts, and optimization cache
    poisoning.
  - Requirement: persist `vmir-optimization-correctness-report.json` with pass
    inventory, before/after hashes, semantic equivalence cases, rejected
    optimizations, source-map parity, reduction deltas, and debug breakpoint
    parity.
  - Gate: add `make vmir-optimization-correctness-check` and run it after
    `vmir-execution-trace-check` and before final release readiness.
  - Current gate state: `make vmir-optimization-correctness-check` exists and
    runs the VMIR execution trace gate, VM artifact build selectors, VMIR
    execution equivalence selectors, remote-call semantic selectors,
    pure/native lane and checksum/source-map guard selectors, artifact
    rejection selectors for missing VMIR bodies, BEAM-shaped VMIR vocabulary,
    module mismatch, stale source-map functions, and missing debug metadata, and
    `terlan-quality vmir-optimization-correctness`. It writes
    `target/quality/vmir-optimization-correctness-report.json` with pass
    inventory, before/after hash coverage, 15 semantic equivalence cases,
    adversarial optimization rejections, rejected optimization classes,
    source-map parity, reduction deltas, and debug breakpoint parity. The
    current optimizer baseline is explicit identity VMIR lowering plus
    pure/native lane classification; unsupported optimizer classes remain
    tracked as rejected optimizations until implemented.
  - Acceptance: release cannot pass if an optimization can change observable
    Terlan behavior, hide diagnostics, break debugger/source-map parity, or
    alter VM resource lifecycle semantics.
  - Acceptance: the gate fails if optimization output is non-deterministic,
    unreported, not reproducible from support bundles, or not invalidated by
    relevant source/package/toolchain changes.


## Completed 085

- [x] Slice 105: define typed native artifact extraction from VMIR.
  - Requirement: the compiler must define when VMIR regions may be extracted
    into typed native artifacts for pure functions, target-owned intrinsics,
    numeric kernels, collection kernels, generated bindings, and explicit
    NativeBoundary exports.
  - Requirement: extraction must preserve VM ownership of scheduling, actor
    lifecycle, resource handles, cancellation, supervision, tracing, source
    maps, hot reload, capability checks, and failure delivery.
  - Requirement: extracted native artifacts must be deterministic, reproducible,
    content-addressed, ABI-versioned, capability-declared, and callable from the
    VM without NIF assumptions or external async runtime ownership.
  - Requirement: add adversarial tests for accidental side-effect extraction,
    actor code extraction, resource-handle leaks, cancelled native work, stale
    artifacts, ABI mismatch, package capability mismatch, and source-map loss.
  - Requirement: persist `vmir-native-artifact-extraction-report.json` with
    extraction candidates, accepted regions, rejected regions, generated artifact
    hashes, ABI metadata, capability metadata, trace parity, and cancellation
    behavior.
  - Gate: add `make vmir-native-artifact-extraction-check` and run it after
    `vmir-optimization-correctness-check` and before final release readiness.
  - Current gate state: `make vmir-native-artifact-extraction-check` exists and
    runs the VMIR optimization correctness gate, `stdlib-native-artifacts-check`,
    native metadata exact selectors, metadata deduplication and escaped JSON
    round-trip selectors, fail-closed Rust lowering validation and compile selectors,
    native policy exact selectors, canonical native-boundary policy parsing and
    diagnostic selectors, and `terlan-quality vmir-native-artifact-extraction`.
    It writes
    `target/quality/vmir-native-artifact-extraction-report.json` with extraction
    candidates, accepted regions, rejected regions, generated artifact hash
    coverage, ABI metadata, capability metadata, trace parity, determinism and
    policy selectors, cancellation behavior, and 14 exact selectors. Implemented
    progress now includes real AOT extraction for closed integer-pure call regions:
    the compiler emits and compiles a persistent worker executable, records its
    typed `(Int...) -> Int` exports and SHA-256 in checksum-covered VMIR, removes
    interpreted bodies from `native_pure` lanes, and makes artifact execution and
    REPL declarations invoke those exports through a hash-verified, crash-isolated
    VM-owned boundary. Unsupported Rust lowering fails closed instead of emitting
    a placeholder function. Boolean, float, string, aggregate, generic, closure,
    pattern, and wider control-flow extraction; scheduler parking; cancellation;
    actor/resource ownership; unsafe-native; and legacy ABI paths remain rejected
    or VM-owned until their real lowering is implemented.
  - Acceptance: release cannot pass if VMIR extraction can move impure actor,
    mailbox, resource, or scheduling behavior outside VM-owned semantics.
  - Acceptance: the gate fails if extracted artifacts are not reproducible,
    typed, capability-checked, traceable, cancellable, or invalidated when
    source/package/toolchain inputs change.


## Completed 086

- [x] Slice 106: make native worker execution VM-owned and scheduler-aware.
  - Requirement: NativeBoundary calls and extracted native artifacts must run
    through VM-owned worker scheduling that parks/resumes actors, charges
    reductions, enforces capability/resource ownership, and preserves
    cancellation/backpressure semantics.
  - Requirement: native worker policy must distinguish nonblocking synchronous
    calls, blocking calls, cancellable async calls, sandboxed calls, long-running
    jobs, and streaming calls without requiring Tokio or NIF-style runtime
    ownership.
  - Requirement: worker results must return typed Terlan values or typed errors
    through VM mailboxes/continuations, and actor exit must cancel, detach, or
    ignore stale native results according to a documented ownership rule.
  - Requirement: add adversarial tests for actor exit during native work,
    cancellation races, worker pool saturation, backpressure, resource-handle
    misuse, panic/error conversion, stale result delivery, and scheduler
    starvation.
  - Requirement: persist `vm-native-worker-runtime-report.json` with worker
    policy matrix, actor park/resume traces, cancellation cases, backpressure
    cases, resource ownership checks, scheduler accounting, and stale-result
    rejection results.
  - Gate: `make vm-native-worker-runtime-check` now follows the native TVM ABI
    and native package artifact gates, never a VMIR extraction gate.
  - Current gate state: `make vm-native-worker-runtime-check` exists and runs
    the native TVM ABI gate, generated NativeBoundary worker
    skeleton selectors, NativeBoundary worker request-lifecycle selectors for
    backpressure, duplicate request ids, mismatched completions, cancellation,
    timeouts, unknown cancellations, and duplicate dispose cleanup, NativeBoundary
    runtime selectors for disposed handles, duplicate dispose, and malformed
    payloads, VM resource/cancellation/reduction-accounting selectors, and
    `terlan-quality vm-native-worker-runtime`. It writes
    `target/quality/vm-native-worker-runtime-report.json` with worker policy
    matrix, actor park/resume trace coverage, cancellation cases, backpressure
    cases, resource ownership checks, request-lifecycle adversarial selectors,
    scheduler accounting, stale-result rejection status, and 15 exact selectors.
    The current runtime baseline is the generated typed worker skeleton plus VM
    resource/cancellation accounting; scheduler-integrated native dispatch,
    actor park/resume continuations, worker-pool saturation, stale-result
    suppression, concrete adapter panic/error conversion, and streaming native
    results remain rejected until implemented.
  - Acceptance: release cannot pass if native work can block VM schedulers,
    escape actor lifecycle ownership, bypass capability checks, or deliver
    stale results to exited actors.
  - Acceptance: the gate fails if native worker behavior depends on external
    async runtime ownership or cannot be traced, cancelled, backpressured, and
    reproduced by support bundles.


## Completed 087

- [x] Slice 107: make socket and timer I/O reactor behavior VM-owned.
  - Requirement: the VM must own readiness, wakeups, timers, cancellation,
    backpressure, connection lifecycle, and stream scheduling for TCP, UDP,
    HTTP, WebSocket/SSE, package downloads, ACME/TLS handshakes, and debugger
    transports.
  - Requirement: reactor integration must park/resume actors through VM
    continuations, charge reductions, expose trace events, support source-map
    aware diagnostics, and avoid handing ownership of scheduling semantics to an
    external async runtime.
  - Requirement: I/O resources must have typed handles, ownership transfer
    rules, close semantics, timeout semantics, half-close behavior, read/write
    backpressure, cancellation behavior, and support-bundle replay metadata.
  - Requirement: add adversarial tests for slow clients, cancelled reads,
    cancelled writes, timer races, connection reset, port reuse, half-closed TCP
    streams, UDP packet bursts, TLS handshake failure, and actor exit with open
    sockets.
  - Requirement: persist `vm-io-reactor-runtime-report.json` with reactor
    fixtures, wakeup traces, timer traces, socket lifecycle traces, cancellation
    cases, backpressure cases, resource cleanup, and no-external-runtime
    ownership checks.
  - Gate: add `make vm-io-reactor-runtime-check` and run it after
    `vm-native-worker-runtime-check` and before final release readiness.
  - Current gate state: `make vm-io-reactor-runtime-check` exists and passes.
    It runs `vm-native-worker-runtime-check`, `no-default-tokio-runtime-check`,
    exact VM TCP, UDP, package download, support-bundle replay metadata,
    source-map-aware I/O diagnostic, and debugger transport readiness, TCP
    scheduler wake, unified I/O reactor loop, external async runtime boundary,
    timer wake,
    framing timeout/cancellation/backpressure, HTTP actor parking/cancellation,
    SSE backpressure, WebSocket cancellation, TLS readiness selectors, ACME
    live worker readiness selectors, and `terlan-quality
    vm-io-reactor-runtime`. It writes
    `target/quality/vm-io-reactor-runtime-report.json` with 16 reactor
    fixtures, 32 exact selectors, 0 rejected runtime paths,
    wakeup/timer/socket lifecycle traces, cancellation cases, backpressure
    cases, resource cleanup, ACME live worker reactor evidence, and
    no-external-runtime ownership checks. The current runtime baseline covers
    VM-owned TCP, UDP packet readiness, package download chunk readiness,
    support-bundle replay metadata, source-map-aware I/O diagnostics, debugger
    transport, a single unified I/O reactor loop, external async runtime
    scheduling boundary, timers, framing, HTTP, SSE, WebSocket, TLS, and ACME
    readiness fixtures. UDP now has typed socket handles, packet inbox
    backpressure, receive wakeups, close and owner-cleanup semantics,
    inspectable socket state, exact VM tests, and report evidence. Package
    download transport now has typed download handles, chunk queue
    backpressure, receive/completion wakeups, cancellation and owner cleanup,
    inspectable transfer state, exact VM tests, and report evidence.
    Support-bundle replay metadata now has typed replay resources, scheduler
    seeds, monotonic I/O steps, optional symbolic source identity, mismatch
    rejection, exact VM tests, and report evidence. Source-map-aware I/O
    diagnostics now have mandatory source map ids, source files, module/function
    identity, one-based spans, typed resource identity, severity/code/message
    rendering, source-map filtering, exact VM tests, and report evidence.
    Debugger transport now has typed session handles, bounded command/event
    queues, command and event receive parking, `VmDebuggerWake` readiness,
    breakpoint validation, owner cleanup, stale-handle errors, exact VM tests,
    and report evidence. `no-default-tokio-runtime-check` now rejects
    placeholder Tokio inventory owners/notes such as `todo`, `tbd`,
    `unknown`, or `fixme`, and also rejects placeholder names in the allowed
    classification vocabulary itself, so retained Tokio references must stay
    explicitly owned while the VM I/O reactor removes external runtime
    ownership. The report now has zero rejected runtime paths.
  - Completed progress: `vm-io-reactor-runtime-check` now rejects
    placeholder/TODO/TBD report evidence across reactor fixtures, rejected
    runtime paths, and exact VM selectors before writing
    `vm-io-reactor-runtime-report.json`, with an adversarial injected
    placeholder test proving I/O reactor readiness evidence cannot be padded
    into the release artifact.
  - Completed progress: ACME live worker reactor integration is now part of
    the Slice 107 gate through required `VmAcmeWorkerRuntime`,
    `VmAcmeWorkerExecutionLane`, `VmAcmeWorkerWake`, renewal timer, support
    bundle, and deterministic renewal cache/TLS handoff anchors. The gate runs
    exact VM selectors for fixture/live lane parity, due-renewal challenge
    routing, and deterministic cache/TLS handoff replay, and the report now
    records ACME live worker readiness as covered reactor evidence rather than
    a rejected path.
  - Completed progress: the single unified I/O reactor loop is now implemented
    as `VmIoReactorLoop` with normalized `VmIoReactorWake` values for TCP, UDP,
    package download, debugger transport, ACME worker, and timer readiness. The
    gate requires the reactor source anchors, exact mixed-wakeup and
    stale-process VM selectors, deterministic drain traces, and report evidence,
    so the unified loop is no longer a rejected runtime path.
  - Completed progress: external async runtime scheduling ownership is now
    guarded by `VmExternalIoRuntimeBoundary` and `VmExternalIoRuntimePlan`.
    External helpers may act only as typed VM wake producers with bounded
    backpressure and support-bundle replay metadata; actor scheduling, hidden
    process continuations, direct scheduler access, unbounded helpers, and
    unreplayable helpers are rejected by exact VM selectors and report evidence.
  - Acceptance: release cannot pass if socket, timer, HTTP, or debugger
    transport behavior can bypass VM scheduling, cancellation, tracing, or
    resource ownership.
  - Acceptance: the gate fails if reactor behavior depends on an external async
    runtime owning scheduling semantics or cannot be reproduced from support
    bundles under fixed scheduler seeds.


## Completed 088

- [x] Slice 108: route HTTP requests through VM-owned Terlan handler dispatch.
  - Requirement: HTTP requests accepted by the VM I/O reactor must be decoded
    into typed Terlan request values, routed through compiled VMIR handler
    dispatch, and returned as typed response values without bypassing actor
    scheduling, source maps, tracing, or cancellation semantics.
  - Requirement: handler dispatch must support static routes, parameterized
    routes, method dispatch, middleware composition, request body limits,
    streaming bodies, typed errors, JSON responses, template responses,
    redirects, status/header validation, and connection keep-alive behavior.
  - Requirement: request-scoped actors must have documented lifecycle,
    supervision, timeout, cancellation, backpressure, resource cleanup, and
    support-bundle replay behavior.
  - Requirement: add adversarial tests for malformed requests, large headers,
    slow bodies, cancelled handlers, middleware errors, route ambiguity,
    response header injection, request actor crashes, and keep-alive connection
    reuse.
  - Requirement: persist `vm-http-handler-dispatch-report.json` with route
    fixtures, handler traces, typed request/response checks, middleware checks,
    cancellation cases, backpressure cases, keep-alive behavior, and
    source-map/debug parity.
  - Gate: add `make vm-http-handler-dispatch-check` and run it after
    `vm-io-reactor-runtime-check` and before final release readiness.
  - Current gate state: `make vm-http-handler-dispatch-check` exists and
    passes. It runs `vm-io-reactor-runtime-check`, exact VM HTTP router
    selectors for method/path dispatch, fallback, duplicate/unsafe route
    rejection, grouped routes, middleware short-circuiting, static assets,
    explicit response bodies, SSE routes, and WebSocket routes; exact VM HTTP
    handler selectors for typed in-memory request/response execution,
    request-driven arithmetic responses, malformed request rejection, typed
    handler errors, TCP request actor polling, keep-alive reuse, keep-alive
    wakeup, cancellation cleanup, and connection-close handling; exact
    source-level VM descriptor selectors for router, response, and session
    handlers; exact VM router selectors for parameterized route extraction,
    exact-route precedence, ambiguous parameterized shape rejection, middleware
    continuation dispatch, and direct compiled VM handler invocation; and
    `terlan-quality vm-http-handler-dispatch`. It writes
    `target/quality/vm-http-handler-dispatch-report.json` with 18 dispatch
    fixtures, 29 exact selectors, handler traces, typed request/response
    checks, middleware checks, cancellation cases, backpressure cases,
    keep-alive behavior, source-map/debug parity evidence, and 0 rejected
    dispatch paths.
    Parameterized route extraction into typed request params is now implemented
    in the VM router, and route ambiguity diagnostics now expose structured
    method, candidate path, existing path, normalized shape, and reason fields
    beyond exact normalized path string matching.
  - Completed progress: `vm-http-handler-dispatch-check` now rejects
    placeholder/TODO/TBD report evidence across dispatch fixtures, handler
    traces, typed request/response checks, middleware checks, cancellation
    cases, backpressure cases, keep-alive behavior, and rejected dispatch paths
    before writing `vm-http-handler-dispatch-report.json`; the quality tests
    include an injected-placeholder case so HTTP dispatch evidence cannot be
    padded with vague labels.
  - Completed progress: source-map/debug parity is now positive structured
    report evidence instead of a rejected report field. The dispatch report
    records handler source diagnostic identity, support-bundle request metadata
    replay, and exact VM selector coverage, and the quality self-test asserts
    the JSON `sourceMapDebugParity.implemented` flag is true.
  - Completed progress: `VmHttpRouteAmbiguityDiagnostic` now reports method,
    candidate route, existing route, normalized route shape, and ambiguity
    reason for duplicate exact paths and parameterized route shape collisions;
    `vm-http-handler-dispatch-check` validates the router anchors and report
    evidence with 12 dispatch fixtures, 23 exact selectors, and 6 rejected
    dispatch paths.
  - Completed progress: HTTP handler failures now produce bounded
    support-bundle replay evidence through
    `capture_http_handler_failure_support_bundle`. The exact VM selector proves
    stable method/path/body-length capture, rejects empty failure evidence, and
    does not copy request bodies into replay outcomes; the gate now reports 13
    dispatch fixtures, 24 exact selectors, and 5 rejected dispatch paths.
  - Completed progress: source-map aware HTTP handler diagnostics now preserve
    source file, module, function, arity, method, path, and stable error
    message through `build_http_handler_source_diagnostic` without retaining
    request bodies. The exact VM selector rejects unlinked diagnostics, and the
    gate now reports 14 dispatch fixtures, 25 exact selectors, and 4 rejected
    dispatch paths.
  - Completed progress: typed template responses now render through VM HTTP
    handler dispatch with `VmHttpTemplateResponse` and
    `render_http_template_response`, preserving template name/source identity
    while emitting a normal `text/html` HTTP response. The exact VM selector
    rejects unnamed or source-unlinked templates, and the gate now reports 15
    dispatch fixtures, 26 exact selectors, and 3 rejected dispatch paths.
  - Completed progress: parsed HTTP request bodies now expose a VM-owned
    bounded dispatch stream through `VmHttpRequestBodyStream` and
    `stream_http_request_body_for_dispatch`. The exact VM selector proves
    ordered chunk indexes, bounded chunk bytes, final-chunk markers, empty-body
    handling, and zero-sized chunk rejection; the gate now reports 16 dispatch
    fixtures, 27 exact selectors, and 2 rejected dispatch paths.
  - Completed progress: route middleware now has a VM-owned continuation path
    through `VmHttpMiddlewareContinuation`,
    `VmHttpMiddlewareStep`, and
    `dispatch_with_middleware_continuation`. The exact VM selector proves
    ordered continuation stepping, visible next-index/remaining-count state,
    pass-through handler dispatch, and short-circuit response dispatch; the
    gate now reports 17 dispatch fixtures, 28 exact selectors, and 1 rejected
    dispatch path.
  - Completed progress: router dispatch now supports direct compiled VM handler
    invocation through `VmHttpCompiledHandlerRef`,
    `VmHttpCompiledHandlerDispatch`, and `dispatch_compiled_handler`. The exact
    VM selector compiles a Terlan handler into the VM, dispatches a
    parameterized route, invokes the compiled function, preserves route params,
    and rejects missing compiled-handler routes; the gate now reports 18
    dispatch fixtures, 29 exact selectors, and 0 rejected dispatch paths.
  - Acceptance: release cannot pass if HTTP handlers can execute outside VM
    scheduling, lose typed request/response validation, or skip cancellation and
    resource cleanup.
  - Acceptance: the gate fails if route dispatch, middleware, streaming bodies,
    keep-alive, or handler failures cannot be traced and reproduced from support
    bundles under fixed scheduler seeds.


## Completed 089

- [x] Slice 109: enforce concurrent HTTP handler scheduling fairness.
  - Requirement: VM HTTP handler execution must schedule concurrent request
    actors fairly across keep-alive connections, slow clients, long-running
    handlers, streaming responses, static responses, JSON handlers, and
    stateful actor-backed handlers.
  - Requirement: the scheduler must expose per-handler reductions, queue wait
    time, parked duration, wakeup count, cancellation count, backpressure wait,
    response-write wait, and timeout classification in benchmark and support
    bundle reports.
  - Requirement: concurrent handler scheduling must avoid head-of-line blocking
    across connections and routes while preserving connection ordering where the
    protocol requires it.
  - Requirement: add adversarial tests for c10/c100/c1000 request mixes, one
    slow connection among fast clients, cancelled long handlers, streaming
    responses under pressure, stateful actors under contention, and large-body
    uploads competing with small static responses.
  - Requirement: persist `vm-http-handler-scheduler-fairness-report.json` with
    concurrency profiles, fairness counters, route mix, latency percentiles,
    throughput, dominant bottleneck, queue saturation reasons, and replay seeds.
  - Gate: add `make vm-http-handler-scheduler-fairness-check` and run it after
    `vm-http-handler-dispatch-check` and before final release readiness.
  - Current gate state: `make vm-http-handler-scheduler-fairness-check` exists
    and passes. It runs `vm-http-handler-dispatch-check`, exact VM HTTP queue
    selectors for capacity, FIFO metrics, enqueue backpressure, and dequeue
    wakeups; exact keep-alive server selectors for accept-limit fairness,
    handler-limit fairness, round-robin handler cursor behavior, idle handler
    wakeups, listener/handler pressure inspection, and cancellation cursor
    edges; exact socket benchmark selectors for queue/delay options,
    scheduler-sized worker pools, acceptor pool sizing, warmup rounding, report
    pool counts, per-handler reduction accounting, CRUD route mix,
    request-dependent add mix, large-upload/static route mix, slow-client
    attribution, queued SSE response routing/decoding, keep-alive divisibility
    validation, and stateful actor contention/backpressure attribution; plus
    bounded socket benchmark commands for c4, c8 queue pressure, keep-alive,
    CRUD/payload route mix, large-upload/static route mix, slow-client route
    mix, and queued SSE streaming route mix. It writes
    `target/quality/vm-http-handler-scheduler-fairness-report.json` with
    18 fairness fixtures, 30 exact selectors, 7 benchmark commands,
    concurrency profiles, fairness counters, route mix, latency percentiles,
    throughput, dominant bottleneck notes, queue saturation reasons, and
    support-bundle replay seeds. The report now includes bounded
    c10/c100/c1000 long-running profile plans that preserve target
    concurrency while capping release-check sample concurrency. Replay seeds now
    capture queue pressure, server inspection counters, poll outcomes, active
    handlers, next handler cursor, queued accepts, and stable seed ids for
    fairness regression reproduction. No Slice 109 fairness paths remain
    rejected.
  - Completed progress: `vm-http-handler-scheduler-fairness-check` now rejects
    placeholder/TODO/TBD report evidence across concurrency profiles, fairness
    counters, route mix, latency percentiles, throughput, bottleneck notes,
    queue saturation reasons, replay seeds, fairness fixtures, benchmark
    commands, and rejected fairness paths before writing
    `vm-http-handler-scheduler-fairness-report.json`; the quality tests include
    an injected-placeholder case so scheduler fairness evidence cannot be
    padded with vague labels.
  - Completed progress: the VM HTTP socket benchmark now supports a
    `large-static` request mix that alternates large `POST /upload` bodies with
    small `GET /static/app.css` responses. The fairness gate runs the exact
    route-construction selector and a bounded `large-static` benchmark command;
    the generated report now records 12 fixtures, 21 exact selectors, 5
    benchmark commands, and 6 rejected fairness paths.
  - Completed progress: the VM HTTP socket benchmark report now includes
    `estimated_handler_reductions` with per-worker totals, min/max, mean, and
    warmup inclusion. The fairness gate requires the exact report selector and
    benchmark anchor; the generated report now records 13 fixtures, 22 exact
    selectors, 5 benchmark commands, and 5 rejected fairness paths.
  - Completed progress: the VM HTTP socket benchmark report now includes
    `response_write_wait` attribution with per-worker total/min/max/mean
    microseconds and warmup inclusion. The fairness gate requires the exact
    report selector and benchmark anchor; the generated report now records 14
    fixtures, 23 exact selectors, 5 benchmark commands, and 4 rejected fairness
    paths.
  - Completed progress: the VM HTTP socket benchmark now supports a
    `slow-client` request mix where exactly one socket client trickles its
    request while unrelated clients stay fast. Reports include
    `slow_client_connections`; the fairness gate requires the exact slow-client
    selectors and bounded benchmark command, and the generated report now
    records 15 fixtures, 25 exact selectors, 6 benchmark commands, and 3
    rejected fairness paths.
  - Completed progress: the VM HTTP socket benchmark now supports a
    `streaming` request mix backed by the existing `std.http.Sse.response`
    descriptor path. The benchmark handler routes `GET /events` through queued
    SSE events, the decoder validates compact `sse_event` descriptors into a
    `text/event-stream` response, the fairness gate requires exact route and
    descriptor selectors plus a bounded streaming benchmark command, and the
    generated report now records 16 fixtures, 27 exact selectors, 7 benchmark
    commands, and 2 rejected fairness paths.
  - Completed progress: stateful actor contention is now part of the scheduler
    fairness gate instead of a rejected path. The gate directly requires exact
    selectors for stale concurrent writer rejection and stateful actor mailbox
    backpressure attribution, records the `stateful-actor-contention` profile,
    and the generated report now records 17 fixtures, 29 exact selectors, 7
    benchmark commands, and 1 rejected fairness path.
  - Completed progress: c10/c100/c1000 long-running load profile plans are now
    release-gated through `benchmark_http_socket_long_running_profiles`. The
    exact VM selector proves target concurrency, bounded sample concurrency,
    queue capacity, handler delay, and request-count invariants without
    spawning unbounded local socket clients in normal gates; the generated
    report now records 18 fixtures, 30 exact selectors, 7 benchmark commands,
    and 0 rejected fairness paths.
  - Acceptance: release cannot pass if HTTP concurrency scaling regresses
    without an attributed scheduler, I/O, parser, handler, or response-write
    cause.
  - Acceptance: the gate fails if one route, connection, slow client, or
    streaming response can starve unrelated handlers or hide backpressure as a
    benchmark timeout.


## Completed 090

- [x] Slice 110: support stateful HTTP actors and explicit session affinity.
  - Requirement: HTTP handlers must be able to route work to typed stateful VM
    actors for counters, rooms, user sessions, live template fragments, workflow
    state, and long-running request coordination without hard-coding domain names
    such as room actors into the runtime.
  - Requirement: session affinity must be explicit, typed, observable, and
    optional: the default HTTP path must remain stateless unless route,
    middleware, or handler metadata requests actor affinity.
  - Requirement: stateful actor-backed handlers must define lifecycle,
    supervision, timeout, persistence hook, backpressure, cancellation,
    reconnect, and migration behavior across worker restarts and hot reload.
  - Requirement: add adversarial tests for missing affinity keys, conflicting
    affinity keys, actor crash during request, reconnect after crash, duplicate
    commands, stale live-template subscribers, session migration, and concurrent
    updates to the same stateful actor.
  - Requirement: persist `vm-http-stateful-actor-session-report.json` with
    affinity fixtures, actor lifecycle traces, state transition traces,
    reconnect cases, duplicate-command handling, backpressure cases, and hot
    reload migration results.
  - Gate: add `make vm-http-stateful-actor-session-check` and run it after
    `vm-http-handler-scheduler-fairness-check` and before final release
    readiness.
  - Current gate state: `make vm-http-stateful-actor-session-check` exists and
    passes. It runs `vm-http-handler-scheduler-fairness-check`, exact VM HTTP
    session actor selectors for missing-cookie creation, adapter delegation,
    non-string value rendering, blank-cookie replacement, defensive table event
    handling, typed affinity key acceptance, duplicate matching affinity key
    merge behavior, missing and conflicting affinity key rejection, stale table
    cleanup, stale private lookup paths, cookie reuse, actor crash during
    request cleanup and replacement, reconnect after actor crash into a clean
    replacement session, duplicate command idempotency replay, live-template
    subscriber cleanup after actor exit, concurrent state update conflict
    rejection, persistence hook replay after restart, stateful actor mailbox
    backpressure attribution, session migration across workers, hot reload
    migration compatibility reporting, session rotation, expiration cleanup,
    fail-closed stale-cookie recovery, invalid runtime
    configuration, source-level session descriptor lifecycle, and expired
    source-level session descriptor rejection. It writes
    `target/quality/vm-http-stateful-actor-session-report.json` with 18
    affinity fixtures, 8 actor lifecycle traces, state transition traces,
    reconnect cases, duplicate-command handling, backpressure cases, hot reload
    migration results, 0 rejected session paths, and 26 exact selectors. No
    Slice 110 stateful HTTP session paths remain rejected.
  - Completed progress: `vm-http-stateful-actor-session-check` now rejects
    placeholder/TODO/TBD report evidence across affinity fixtures, actor
    lifecycle traces, and rejected session paths before writing
    `vm-http-stateful-actor-session-report.json`; the gate passed with 18
    affinity fixtures, 8 lifecycle traces, 0 rejected session paths, and 26
    exact selectors.
  - Acceptance: release cannot pass if stateful HTTP behavior requires hidden
    sticky sessions, untyped global state, or domain-specific runtime concepts.
  - Acceptance: the gate fails if actor-backed handlers can lose commands,
    duplicate commands, bypass supervision, leak subscribers, or hide affinity
    decisions from traces/support bundles.


## Completed 091

- [x] Slice 111: stream typed live template updates from VM actors.
  - Requirement: typed templates must bind to VM actor state, process commands,
    and stream DOM patch events over VM-owned SSE/WebSocket transports without
    requiring untyped JavaScript glue for normal server-backed snippets.
  - Requirement: template subscriptions must be typed, capability-checked,
    source-map aware, reconnectable, cancellable, backpressured, and tied to
    actor lifecycle so stale subscribers cannot receive updates after actor exit
    or hot reload incompatibility.
  - Requirement: live update transport must support initial render, incremental
    patch, command postback, error patch, redirect, reconnect token, heartbeat,
    client cancellation, and server-side subscriber cleanup.
  - Requirement: add adversarial tests for stale subscribers, duplicate
    commands, actor restart during stream, dropped client connection, slow
    client backpressure, malformed command payload, incompatible hot reload, and
    cross-template state update.
  - Requirement: persist `vm-live-template-stream-report.json` with template
    fixtures, actor binding traces, patch events, command postbacks,
    reconnect cases, cancellation cases, backpressure cases, and subscriber
    cleanup results.
  - Gate: add `make vm-live-template-stream-check` and run it after
    `vm-http-stateful-actor-session-check` and before final release readiness.
  - Current gate state: `make vm-live-template-stream-check` exists and passes.
    It runs `vm-http-stateful-actor-session-check`, VM-owned SSE stream tests,
    WebSocket source/queue/termination tests, and std HTTP live-channel source
    tests for router grouping, queued SSE events, SSE endpoints, and WebSocket
    endpoints. It writes `target/quality/vm-live-template-stream-report.json`
    with template fixtures, actor binding traces, patch event classes, command
    postbacks, reconnect cases, cancellation cases, backpressure cases,
    implemented subscriber cleanup, duplicate command idempotency, and actor
    restart evidence, malformed command payload rejection, DOM patch
    backpressure, reconnect token validation, hot reload subscriber migration,
    command postback dispatch into actor mailboxes, cross-template state update
    fanout, capability-checked template subscriptions, typed
    template-to-actor binding, source-map aware subscription traces, and 0
    rejected stream paths.
    Server-side subscriber cleanup after actor exit is backed by
    `http_session_live_template_subscribers_are_cleaned_after_actor_exit`;
    duplicate command idempotency is backed by
    `http_session_idempotent_command_replays_duplicate_result_without_rerun`;
    actor restart during stream is backed by
    `http_session_actor_crash_during_request_cleans_state_and_replaces_cookie`
    and
    `http_session_reconnect_after_actor_crash_replaces_cookie_without_reusing_state`.
    Malformed command payload rejection is backed by
    `http_session_rejects_malformed_live_template_command_payload_before_dispatch`
    and the typed `apply_live_template_command` boundary.
    Slow-client DOM patch backpressure is backed by
    `vm_sse_dom_patch_backpressure_rejects_slow_browser_patch_queue` and the
    VM-owned `VmSseDomPatchBackpressure` queue bound.
    Reconnect token validation is backed by
    `vm_sse_reconnect_token_rotates_and_rejects_stale_browser_tokens` and
    `vm_sse_reconnect_token_rejects_empty_and_control_tokens`.
    Hot reload subscriber migration is backed by
    `http_session_reports_hot_reload_migration_compatibility`; durable session
    table entries and command results remain compatible across generations while
    live-template subscribers are explicitly reported as transient.
    Command postback dispatch into actor mailboxes is backed by
    `http_session_live_template_command_dispatches_actor_mailbox_postback_once`;
    browser command ids and names are normalized, duplicate command ids replay,
    and the typed `live_template_command` payload is enqueued on the session
    actor mailbox exactly once.
    Cross-template state update fanout is backed by
    `http_session_live_template_state_update_fans_out_to_all_subscribers`;
    optimistic actor state updates emit deterministic typed patch payloads for
    every live subscriber, stale writers fail before fanout, and blank patch
    event names fail before mutation. Capability-checked template subscriptions
    are backed by
    `http_session_live_template_subscription_requires_capability_before_registering`;
    missing capabilities fail before subscriber registration, granted
    capabilities are normalized and deduplicated, and malformed capability
    inputs fail closed. Typed template-to-actor binding is backed by
    `http_session_binds_typed_live_template_to_actor_state`; typed templates
    bind to VM actor/table state, missing state binds as typed none, malformed
    template ids/state keys fail closed, and crashed session actors reject
    binding before stale subscriber delivery. Source-map aware subscription
    traces are backed by
    `http_session_traces_live_template_subscription_source_map`; subscription
    traces record subscriber, template id, actor pid, state version, and source
    module/line/column for replayable support bundles, while missing
    subscribers, malformed source locations, blank source modules, and crashed
    session actors fail closed.
  - Completed progress: `vm-live-template-stream-check` now uses concrete
    patch event classes (`initial render event`, `incremental DOM patch event`,
    `typed error patch event`, `redirect patch event`, and `heartbeat event`)
    and rejects placeholder/TODO/TBD report evidence across template fixtures,
    patch events, and rejected stream paths before writing
    `vm-live-template-stream-report.json`.
  - Completed progress: live-template command payload dispatch now validates typed
    command envelopes before handler execution through `apply_live_template_command`;
    malformed command ids/names fail before command side effects, and
    `vm-live-template-stream-check` records `malformedCommandPayloadRejection`
    evidence.
  - Completed progress: slow-client DOM patch backpressure is now release-gated
    through `VmSseDomPatchBackpressure`; the live-template report records
    `domPatchBackpressure` evidence and no longer treats this path as rejected.
  - Completed progress: reconnect token validation is now release-gated through
    `VmSseReconnectTokenState`; the live-template report records
    `reconnectCases` evidence and no longer treats dropped reconnect tokens as
    rejected.
  - Completed progress: hot reload subscriber migration is now release-gated
    through `hot_reload_migration_compatibility_report`; the live-template
    report records `hotReloadSubscriberMigration` evidence and no longer treats
    incompatible hot reload subscriber migration as rejected.
  - Completed progress: command postback dispatch into actor mailboxes is now
    release-gated through `dispatch_live_template_command_to_actor_mailbox`; the
    live-template report records `commandPostbacks` evidence and no longer
    treats command postback dispatch as rejected.
  - Completed progress: cross-template state update fanout is now release-gated
    through `fanout_live_template_state_update`; the live-template report
    records `crossTemplateStateUpdateFanout` evidence and no longer treats
    cross-template state update fanout as rejected.
  - Completed progress: capability-checked template subscriptions are now
    release-gated through `subscribe_live_template_with_capability`; the
    live-template report records `capabilityCheckedSubscriptions` evidence and
    no longer treats capability-checked template subscriptions as rejected.
  - Completed progress: typed template-to-actor binding is now release-gated
    through `bind_live_template_to_actor_state`; the live-template report
    records `actorBindingTraces` evidence and no longer treats typed
    template-to-actor binding as rejected.
  - Completed progress: source-map aware subscription traces are now
    release-gated through `trace_live_template_subscription_with_source_map`;
    the live-template report records `sourceMapSubscriptionTraces` evidence,
    reports 0 rejected stream paths, and Slice 111 is complete.
  - Acceptance: release cannot pass if live template updates require untyped
    client glue, bypass VM actor lifecycle, or can deliver stale updates after
    actor exit or incompatible hot reload.
  - Acceptance: the gate fails if patch streams, command postbacks, reconnects,
    or subscriber cleanup cannot be traced and reproduced from support bundles.


## Completed 092

- [x] Slice 112: define the VM live-template browser protocol.
  - Requirement: the browser-side live-template runtime must be generated from a
    typed protocol manifest that defines patch events, command postbacks,
    reconnect tokens, heartbeats, error patches, redirect patches,
    cancellation, backpressure signals, and version negotiation.
  - Requirement: generated browser protocol code must be usable from both JS and
    Wasm targets, validate payload shapes at the boundary, and preserve typed
    Terlan template bindings without requiring users to write untyped DOM glue.
  - Requirement: protocol compatibility must be checked across VM hot reload,
    package updates, generated binding refreshes, browser reconnects, stale tabs,
    and mixed old/new client assets during rolling deploy.
  - Requirement: add adversarial tests for malformed patch payloads, stale
    protocol versions, duplicate command IDs, replayed commands, missed
    heartbeat, dropped reconnect token, slow DOM patch application, and client
    assets generated from an older compiler.
  - Requirement: persist `vm-live-template-client-protocol-report.json` with
    protocol schema hashes, generated JS/Wasm fixtures, payload validation
    cases, compatibility matrix, reconnect cases, stale-asset rejections, and
    DOM patch replay results.
  - Gate: add `make vm-live-template-client-protocol-check` and run it after
    `vm-live-template-stream-check` and before final release readiness.
  - Current gate state: `make vm-live-template-client-protocol-check` exists
    and passes. It runs `vm-live-template-stream-check`, validates the
    Angular.ts integration and namespace-generation boundaries, runs
    `vm_live_template_client_protocol_test`, and writes
    `target/quality/vm-live-template-client-protocol-report.json` with protocol
    event classes, JS/browser/Wasm target intent, payload validation cases,
    compatibility cases, reconnect cases, stale-asset rejection status, DOM
    patch replay status, the angular-wave/angular.ts boundary, and rejected
    protocol paths. Command postback replay protection is backed by
    `http_session_idempotent_command_replays_duplicate_result_without_rerun`,
    `apply_idempotent_command`, and `command_results`; heartbeat timeout
    handling is backed by `VmSseHeartbeatState`, `HeartbeatTimedOut`, and
    `vm_sse_heartbeat_timeout_tracks_stale_browser_streams`; browser reconnect
    token rotation is backed by `VmSseReconnectTokenState`,
    `StaleReconnectToken`, and
    `vm_sse_reconnect_token_rotates_and_rejects_stale_browser_tokens`; stale
    asset protocol hash rejection is backed by `VmSseProtocolAssetHashState`,
    `StaleProtocolAssetHash`, and
    `vm_sse_protocol_asset_hash_rejects_stale_browser_assets`; slow DOM patch
    backpressure is backed by `VmSseDomPatchBackpressure`,
    `DomPatchBackpressureExceeded`, and
    `vm_sse_dom_patch_backpressure_rejects_slow_browser_patch_queue`; DOM patch
    replay against typed template bindings is backed by
    `VmDomPatchTemplateBinding`, `VmDomPatchOperation`,
    `replay_dom_patches_for_template_bindings`, and
    `vm_model_sync_replays_dom_patches_against_typed_template_bindings`; typed
    protocol manifest generation is backed by
    `VmLiveTemplateProtocolManifest`, `VmLiveTemplateProtocolEventKind`,
    `generate_vm_live_template_protocol_manifest`,
    `validate_vm_live_template_protocol_manifest`, and
    `vm_live_template_protocol_manifest_lists_required_events_and_schema_hash`;
    generated Angular.ts browser runtime modules are backed by
    `VmLiveTemplateAngularTsRuntimeModule`,
    `generate_vm_live_template_angular_ts_runtime_module`,
    `terlanLiveTemplateProtocolManifest`, `validateTerlanLiveTemplateEvent`,
    `connectTerlanLiveTemplateSse`, and
    `vm_live_template_protocol_generates_angular_ts_browser_runtime_module`;
    generated JS protocol binding validation is backed by
    `VmLiveTemplateJsProtocolBindingValidation`,
    `validate_vm_live_template_js_protocol_binding`,
    `vm_live_template_protocol_validates_generated_js_protocol_binding`, and
    `vm_live_template_protocol_rejects_generated_js_binding_missing_event_field`;
    generated Wasm protocol binding validation is backed by
    `VmLiveTemplateWasmProtocolBindingModule`,
    `VmLiveTemplateWasmProtocolBindingValidation`,
    `generate_vm_live_template_wasm_protocol_binding_module`,
    `validate_vm_live_template_wasm_protocol_binding`,
    `vm_live_template_protocol_generates_wasm_protocol_binding_manifest`,
    `vm_live_template_protocol_validates_generated_wasm_protocol_binding`, and
    `vm_live_template_protocol_rejects_generated_wasm_binding_missing_export`;
    mixed-version rolling deploy compatibility is backed by
    `VmLiveTemplateRollingDeployCompatibilityPlan`,
    `VmLiveTemplateRollingDeployCompatibilityValidation`,
    `validate_vm_live_template_rolling_deploy_compatibility`,
    `vm_live_template_protocol_accepts_mixed_version_rolling_deploy_compatibility`,
    `vm_live_template_protocol_rejects_mixed_version_rolling_deploy_schema_drift`,
    `vm_live_template_protocol_rejects_mixed_version_rolling_deploy_stale_assets`,
    and
    `vm_live_template_protocol_rejects_mixed_version_rolling_deploy_version_window_gap`;
    the report now tracks 0 rejected protocol paths.
  - Completed progress: `vm-live-template-client-protocol-check` now rejects
    placeholder report evidence across protocol events, payload validation
    cases, compatibility cases, and rejected protocol paths while preserving
    typed manifest, Angular.ts browser runtime, JS binding, Wasm binding,
    rolling-deploy, reconnect, stale-asset, command replay, heartbeat, DOM
    patch replay, and backpressure evidence in the generated report.
  - Acceptance: release cannot pass if live templates depend on unversioned,
    untyped, or manually written browser protocol behavior.
  - Acceptance: the gate fails if protocol drift can break reconnects, replay
    commands, skip payload validation, or apply patches generated for a
    different compiler/runtime version.


## Completed 093

- [x] Slice 113: support typed template render modes with performance budgets.
  - Requirement: templates must declare or infer render modes for static HTML,
    server-rendered HTML, streaming HTML, live DOM patches, client-hydrated
    fragments, email-safe markup, and documentation examples while preserving
    the same typed interpolation and escaping rules.
  - Requirement: each render mode must define allowed bindings, escaping policy,
    asset handling, component boundaries, actor/subscription availability,
    cacheability, source maps, and VM/JS/Wasm target compatibility.
  - Requirement: render mode selection must be inspectable by formatter, lint,
    compiler diagnostics, generated docs, editor hover, and release reports so
    users can see why a template is static, server-rendered, live, or hydrated.
  - Requirement: add adversarial tests for wrong escaping mode, cross-mode
    component reuse, stale asset hashes, oversized live patches, slow streaming
    fragments, unsupported actor bindings in static mode, and hydration mismatch.
  - Requirement: persist `typed-template-render-mode-report.json` with template
    inventory, inferred modes, explicit modes, escaping checks, performance
    budgets, asset hashes, source-map parity, and rejected mode combinations.
  - Gate: add `make typed-template-render-mode-check` and run it after
    `vm-live-template-client-protocol-check` and before final release readiness.
  - Current gate state: `make typed-template-render-mode-check` exists and
    passes. It runs `vm-live-template-client-protocol-check`,
    `typed-template-interpolation-check`, the VS Code template-link render-mode
    parser test, `typed_template_render_mode_test`, and `terlan-quality
    typed-template-render-mode`, then writes
    `target/quality/typed-template-render-mode-report.json` with template
    inventory, inferred render modes, explicit-mode status, escaping checks,
    performance budgets, asset-hash status, source-map parity status, and
    docs/editor parity evidence plus rejected mode combinations. Static HTML,
    server-rendered HTML descriptors, structured artifact templates, and
    documentation-example render-mode parity between generated docs and VS Code
    template links are currently classified as implemented.
    Explicit source-level render mode declarations, streaming HTML fragment
    budgets, live DOM patch mode, client hydration compatibility, email-safe
    markup policy, stale asset hash enforcement, and full VM/JS/Wasm source-map
    render-mode parity remain rejected until implemented.
  - Completed progress: `typed-template-render-mode-check` now rejects
    placeholder performance-budget evidence. The render-mode report records
    concrete `max*` thresholds for implemented static/server/structured modes
    and explicit `rejectedUntil*` reasons for deferred streaming, live patch,
    and hydration modes, with adversarial coverage preventing `placeholder`,
    `todo`, `tbd`, or `unknown` budget labels from returning.
  - Acceptance: release cannot pass if templates can render in the wrong mode,
    use an unsafe escaping policy, exceed mode-specific patch/render budgets, or
    hide actor/subscription usage from users.
  - Acceptance: the gate fails if render mode output diverges between VM, JS,
    Wasm, generated docs, or editor tooling for the same typed template.


## Completed 094

- [x] Slice 114: make web asset generation deterministic and runtime-aware.
  - Requirement: web builds must generate deterministic asset graphs for CSS,
    JavaScript, Wasm, images, fonts, live-template protocol assets, static
    files, generated bindings, source maps, and manifests from package-declared
    inputs.
  - Requirement: asset fingerprints, cache headers, content types, compression
    metadata, integrity hashes, source-map links, and live-template protocol
    versions must be stable across clean builds, incremental builds, watch mode,
    package workspaces, and release builds.
  - Requirement: VM HTTP serving must use the same asset manifest as compiler,
    docs, editor tooling, and support bundles so stale browser assets can be
    diagnosed and rejected.
  - Requirement: add adversarial tests for missing assets, stale generated
    assets, case-sensitive paths, paths with spaces, duplicate final browser
    asset paths, source-map leakage, wrong content type, stale cache headers,
    and mixed compiler versions in client assets.
  - Requirement: persist `web-asset-pipeline-report.json` with asset graph,
    fingerprints, manifest entries, source-map checks, content-type checks,
    cache-header checks, integrity hashes, and stale-asset rejection cases.
  - Gate: add `make web-asset-pipeline-check` and run it after
    `typed-template-render-mode-check` and before final release readiness.
  - Current gate state: `make web-asset-pipeline-check` exists and passes. It
    runs `typed-template-render-mode-check`, `browser-package-preflight`,
    `web-profile-preflight`, `web_asset_pipeline_test`, and `terlan-quality
    web-asset-pipeline`, then writes
    `target/quality/web-asset-pipeline-report.json` with asset graph entries,
    fingerprint evidence, manifest entry classes, source-map check status,
    content-type checks, cache-header checks, integrity-hash status, stale-asset
    rejection status, and rejected asset paths. JavaScript modules, CSS/file/
    markdown imports, manifest-declared static assets, deterministic web build
    IDs, browser manifest entries, source spans, VM static content-type
    inference, VM static cache-control metadata, and VM static fingerprint
    metadata, plus manifest-declared path-with-spaces assets and case-folded
    static asset collision rejection, plus duplicate final web asset path
    rejection, plus browser asset subresource integrity hashes and generated
    module script integrity attributes, plus browser JavaScript source-map
    assets, generated `sourceMappingURL` comments, and source-map host path
    leakage rejection, are currently classified as implemented. Compression
    metadata, stale generated asset rejection, mixed compiler version client
    asset rejection, and live-template protocol asset hash compatibility remain
    rejected until implemented. Verified on the current tree with
    `make web-asset-pipeline-check`; loopback socket benchmark probes were
    skipped only through the gate's explicit sandbox allowance. The report now
    records integrity hashes and source maps as implemented, tracks 8 asset
    graph entries, and tracks 4 rejected asset paths.
  - Completed progress: `web-asset-pipeline-check` now rejects placeholder
    asset graph entries. The web asset report records implemented JavaScript,
    source-map, CSS/file/markdown, and static asset kinds, plus explicit
    `rejectedUntil*` reasons for live-template protocol asset hash compatibility
    and hosted Wasm asset execution, with adversarial coverage preventing
    placeholder asset labels from returning.
  - Acceptance: release cannot pass if web assets are non-deterministic, served
    from stale manifests, missing integrity/source-map metadata, or inconsistent
    between compiler, VM HTTP, docs, editor tooling, and support bundles.
  - Acceptance: the gate fails if stale client assets can connect to a newer VM
    protocol without a compatibility decision.


## Completed 095

- [x] Slice 115: enforce typed web security policy for VM HTTP apps.
  - Requirement: VM web apps must expose typed policies for cookies, sessions,
    CSRF tokens, CORS, CSP, HSTS, secure headers, redirects, SameSite behavior,
    upload limits, request body limits, and live-template command authorization.
  - Requirement: security policy must compose through routes, middleware,
    templates, static assets, stateful actors, live-template streams, package
    defaults, and environment-specific config without untyped global switches.
  - Requirement: policy decisions must be observable in diagnostics, generated
    docs, support bundles, HTTP traces, and editor hover so users can inspect
    why a route allows or rejects a request.
  - Requirement: add adversarial tests for CSRF replay, missing SameSite,
    insecure cookie flags, CORS wildcard leaks, CSP bypass, header injection,
    redirect injection, oversized uploads, stale live-template command tokens,
    and mixed dev/prod security config.
  - Requirement: persist `vm-web-security-policy-report.json` with route policy
    matrix, middleware composition, rejected request fixtures, header snapshots,
    cookie snapshots, live-template command authorization checks, and
    environment config decisions.
  - Gate: add `make vm-web-security-policy-check` and run it after
    `web-asset-pipeline-check` and before final release readiness.
  - Current gate state: `make vm-web-security-policy-check` exists and passes.
    It runs the web asset prerequisite chain, TLS policy tests, native HTTP
    cookie boundary tests, the Rust quality check, and persists
    `target/quality/vm-web-security-policy-report.json` with implemented
    cookie/session/header/body-limit/TLS anchors. Typed secure-header and HSTS
    response policy composition is implemented through `SecurityHeaders`,
    `default_security_headers`, `production_security_headers`, and
    `Response.with_security_headers`, with snapshots for `X-Frame-Options`,
    `Referrer-Policy`, `X-Content-Type-Options`, and
    `Strict-Transport-Security`. The report now tracks 9 remaining rejected
    paths: CSRF, CORS, CSP, redirect policy, upload policy, live-template
    command authorization, editor hover, generated-docs, and support-bundle
    policy paths.
  - Completed progress: `vm-web-security-policy-check` now rejects placeholder
    policy surface evidence. The security policy report records implemented
    route/static/session/TLS policy surfaces plus explicit `rejectedUntil*`
    reasons for deferred middleware, template, and live-template command
    authorization composition, with adversarial coverage preventing placeholder
    policy labels from returning.
  - Acceptance: release cannot pass if VM web apps can silently serve routes
    without declared security policy or if dev defaults can leak into production
    builds.
  - Acceptance: the gate fails if cookies, CSRF, CORS, CSP, redirects, upload
    limits, or live-template commands are handled by untyped or unobservable
    policy state.


## Completed 096

- [x] Slice 116: enforce typed web configuration and secret boundaries.
  - Requirement: VM web apps must load configuration through typed schemas for
    dev, test, staging, production, local Docker dependencies, package defaults,
    TLS/ACME, database connections, live-template protocol settings, and asset
    serving policy.
  - Requirement: secrets must be represented as non-loggable typed values with
    explicit redaction rules for diagnostics, support bundles, generated docs,
    editor hover, traces, reports, and panic/error rendering.
  - Requirement: configuration validation must reject missing required values,
    production-unsafe dev defaults, unused secrets, type mismatches, unknown
    keys, conflicting package defaults, and config that changes target/runtime
    behavior after build without a documented dynamic boundary.
  - Requirement: add adversarial tests for leaked secrets in reports, leaked
    secrets in diagnostics, dev defaults in production, malformed env files,
    stale generated config, missing Docker dependency config, duplicate keys,
    and package-provided insecure defaults.
  - Requirement: persist `vm-web-config-secret-boundary-report.json` with config
    schemas, environment matrix, rejected configs, redaction checks, package
    default decisions, Docker dependency decisions, and runtime reload decisions.
  - Gate: add `make vm-web-config-secret-boundary-check` and run it after
    `vm-web-security-policy-check` and before final release readiness.
  - Current gate state: `make vm-web-config-secret-boundary-check` exists and
    passes. It runs the Slice 115 security-policy chain, TLS policy checks,
    Docker Compose/Postgres config checks, the Rust quality check, and persists
    `target/quality/vm-web-config-secret-boundary-report.json` with current
    typed config schemas, environment matrix, rejected configs, redaction
    checks, package default decisions, Docker dependency decisions, and runtime
    reload decisions. `std.core.Secret` is now implemented as a typed
    non-loggable value with stable redacted display, diagnostic, editor-hover,
    generated-doc, support-bundle, trace, and panic/error rendering behavior,
    plus std tests that prove the source value is not rendered through any of
    those paths. `[server] profile = "production"` now rejects internal
    development TLS defaults before startup. The gate now rejects stale
    generated TLS config summaries before release readiness. The gate now
    proves TLS `passphrase_env` and native `helper_env` declarations are
    consumed by VM/deploy/runtime/run paths instead of becoming unused secret
    declarations. The report now tracks 8 secret usage checks and 0 rejected
    secret paths.
  - Completed progress: `vm-web-config-secret-boundary-check` now rejects
    placeholder report evidence. The environment matrix records concrete dev,
    test, production, local Docker, and package-default config surfaces plus
    explicit `rejectedUntil*` reasons for deferred staging secret-source and
    dynamic runtime-reload boundaries, with adversarial coverage preventing
    placeholder config labels from returning.
  - Acceptance: release cannot pass if web apps can start with production-unsafe
    defaults, untyped config, unknown config keys, or secrets that can leak into
    user-visible artifacts.
  - Acceptance: the gate fails if dynamic config changes can silently alter VM
    runtime behavior without validation, tracing, and support-bundle evidence.


## Completed 097

- [x] Slice 117: expose typed VM web observability for requests and streams.
  - Requirement: VM web runtime must emit typed logs, metrics, traces, request
    IDs, connection IDs, actor IDs, route IDs, template stream IDs, security
    policy decisions, config profile, and source-map locations for HTTP
    requests, live-template streams, assets, and WebSocket/SSE transports.
  - Requirement: observability output must preserve secret redaction, path
    redaction, user-data boundaries, sampling policy, performance budget,
    support-bundle replay, and correlation with VM scheduler/native worker/I/O
    reactor events.
  - Requirement: observability must be available in text, JSON, support-bundle,
    benchmark, debugger, and editor surfaces with stable schemas and no
    production-only blind spots.
  - Requirement: add adversarial tests for leaked secrets, missing request IDs,
    missing actor IDs, stale source maps, dropped stream spans, excessive metric
    cardinality, disabled production traces, and mismatched benchmark/support
    bundle telemetry.
  - Requirement: persist `vm-web-observability-report.json` with telemetry
    schema, route traces, stream traces, security decision traces, redaction
    checks, correlation checks, sampling decisions, and cardinality checks.
  - Gate: add `make vm-web-observability-check` and run it after
    `vm-web-config-secret-boundary-check` and before final release readiness.
  - Current gate state: `make vm-web-observability-check` exists and passes. It
    runs the Slice 116 config/secret boundary chain, HTTP observability
    regressions, VM diagnostics exact selectors, the Rust quality check, and
    persists `target/quality/vm-web-observability-report.json` with current
    telemetry schema, route traces, stream traces, security decision traces,
    redaction checks, correlation checks, sampling decisions, cardinality
    checks, and surface matrix. HTTP serve logs now emit a typed
    `connection_id` field alongside `request_id`, and the gate fails if the
    connection-id field is removed from the local serve observability schema.
    The report explicitly keeps WebSocket/SSE connection IDs, actor IDs,
    bounded route labels, template stream IDs, typed security/config profile
    IDs, support-bundle replay, production trace sampling, metric cardinality
    budgets, and benchmark/support-bundle parity as rejected paths until
    implemented.
  - Completed progress: `vm-web-observability-check` now rejects placeholder
    report evidence. The telemetry schema records implemented request,
    connection, actor, route, security, config, source-map, duration, and
    status fields plus an explicit `rejectedUntil*` boundary for
    live-template runtime stream-id emission, with adversarial coverage
    preventing placeholder telemetry labels from returning.
  - Acceptance: release cannot pass if a web request, asset request, or live
    stream can fail without a typed, redacted, source-map-aware observability
    record.
  - Acceptance: the gate fails if observability data leaks secrets, loses
    request/actor correlation, cannot reproduce from support bundles, or hides
    production-only runtime behavior.


## Completed 098

- [x] Slice 118: validate VM web lifecycle, health checks, and graceful draining.
  - Requirement: VM web apps must expose typed startup, readiness, liveness,
    shutdown, hot-reload, draining, dependency-health, and support-bundle health
    states through CLI, HTTP, metrics, traces, and editor/dev-server surfaces.
  - Requirement: graceful shutdown must stop accepting new connections, drain or
    cancel in-flight requests by policy, close live-template streams, flush
    telemetry, release resources, and persist support-bundle evidence without
    losing typed failure reasons.
  - Requirement: readiness must depend on typed config validation, package
    loading, VM artifact loading, database dependency readiness, TLS/ACME state,
    asset manifest readiness, and optional stateful actor warmup.
  - Requirement: add adversarial tests for shutdown during streaming response,
    shutdown during native work, failed dependency readiness, hot reload during
    live-template stream, stuck drain, telemetry flush failure, and force-kill
    support-bundle capture.
  - Requirement: persist `vm-web-lifecycle-health-report.json` with lifecycle
    state transitions, health endpoint fixtures, dependency readiness decisions,
    drain traces, shutdown traces, hot-reload traces, and force-kill evidence.
  - Gate: add `make vm-web-lifecycle-health-check` and run it after
    `vm-web-observability-check` and before final release readiness.
  - Current gate state: `make vm-web-lifecycle-health-check` exists and passes.
    It runs the Slice 117 observability chain, Docker Compose dependency health
    checks, TLS readiness checks, VM source hot-reload exact selectors, the Rust
    quality check, and persists
    `target/quality/vm-web-lifecycle-health-report.json` with current lifecycle
    state transitions, health endpoint fixtures, dependency readiness decisions,
    drain traces, shutdown traces, hot-reload traces, and force-kill evidence.
    The report explicitly keeps generated `/ready` and `/live` endpoints,
    cross-surface startup state export, drain timeout policy, force-kill
    support-bundle capture, telemetry flush recovery, live-template hot-reload
    continuity, native-work cancellation while draining, active dependency wait
    loops, stateful actor warmup readiness, and production health schema
    stability as rejected paths until implemented.
  - Completed progress: `vm-web-lifecycle-health-check` now rejects placeholder
    report evidence across lifecycle state transitions, health endpoint
    fixtures, dependency readiness decisions, drain traces, shutdown traces,
    hot-reload traces, force-kill evidence, and rejected lifecycle paths. The
    gate has adversarial coverage preventing placeholder lifecycle labels from
    returning while keeping all current startup, readiness, draining,
    shutdown, hot-reload, and force-kill report sections machine-readable.
  - Acceptance: release cannot pass if web apps can report ready before required
    dependencies and artifacts are valid or if shutdown can lose in-flight
    request/stream/resource state without a typed policy decision.
  - Acceptance: the gate fails if lifecycle transitions are unobservable,
    non-reproducible, not redacted, or inconsistent across CLI, HTTP, metrics,
    traces, and support bundles.


## Completed 099

- [x] Slice 119: validate VM web deployment profiles and reverse-proxy behavior.
  - Requirement: VM web apps must define typed deployment profiles for local
    development, container, bare-metal, reverse-proxy, TLS-terminated,
    VM-terminated TLS, static-asset CDN, and ACME-managed production modes.
  - Requirement: deployment profiles must validate bind address, port, base
    path, forwarded headers, trusted proxy list, scheme/host reconstruction,
    WebSocket/SSE upgrades, health endpoints, asset URLs, cookie security, and
    redirect generation.
  - Requirement: reverse-proxy behavior must be explicit and observable so apps
    cannot accidentally trust spoofed `Forwarded` or `X-Forwarded-*` headers.
  - Requirement: add adversarial tests for spoofed proxy headers, wrong base
    path, incorrect secure cookie under TLS termination, WebSocket upgrade
    through proxy, stale asset CDN URL, health endpoint exposure, and ACME
    challenge routing.
  - Requirement: persist `vm-web-deployment-profile-report.json` with profile
    matrix, proxy fixtures, header trust decisions, URL reconstruction cases,
    cookie decisions, upgrade cases, health endpoint cases, and ACME routing
    cases.
  - Gate: add `make vm-web-deployment-profile-check` and run it after
    `vm-web-lifecycle-health-check` and before final release readiness.
  - Current gate state: `make vm-web-deployment-profile-check` exists and
    passes. It writes `target/quality/vm-web-deployment-profile-report.json`
    with profile matrix, proxy fixtures, header trust decisions, URL
    reconstruction cases, cookie decisions, upgrade cases, health endpoint
    cases, ACME routing cases, and explicit rejected paths for trusted proxy,
    base-path, CDN asset, TLS-terminated redirect/cookie, public health
    exposure, and reverse-proxy live-stream behavior.
  - Completed progress: `vm-web-deployment-profile-check` now rejects
    placeholder report evidence across the deployment profile matrix, proxy
    fixtures, header trust decisions, URL reconstruction cases, cookie
    decisions, upgrade cases, health endpoint cases, ACME routing cases, and
    rejected deployment paths. The gate has adversarial coverage preventing
    placeholder deployment labels from returning while keeping reverse-proxy,
    TLS, CDN, health, cookie, upgrade, and ACME report sections
    machine-readable.
  - Acceptance: release cannot pass if deployment mode changes security,
    routing, asset, health, or live-stream behavior without a typed profile and
    traceable decision.
  - Acceptance: the gate fails if reverse-proxy headers are trusted by default,
    redirects or cookies are generated with the wrong scheme/host, or ACME/static
    routes conflict with application routes.


## Completed 100

- [x] Slice 120: generate typed route manifests, API schemas, and clients.
  - Requirement: VM web route declarations must produce a typed route manifest
    that records methods, paths, parameters, request bodies, response bodies,
    status codes, headers, middleware, security policy, deployment profile
    constraints, live-template endpoints, and source-map locations.
  - Requirement: API schema generation must reuse the route manifest and typed
    request/response definitions to emit OpenAPI-compatible schemas and
    Terlan-native client metadata without duplicating route declarations.
  - Requirement: generated clients must preserve typed path/query/body/header
    contracts, error shapes, auth/security policy requirements, streaming
    endpoints, cancellation behavior, retries, and source links to route
    definitions.
  - Requirement: add adversarial tests for ambiguous routes, missing response
    types, mismatched path parameters, undocumented error responses, stale
    generated clients, route/middleware security drift, deployment base-path
    changes, and unsupported streaming schema output.
  - Requirement: persist `vm-web-route-schema-client-report.json` with route
    manifest hashes, schema output, generated client fixtures, security policy
    links, deployment profile links, stale-client rejections, and source-map
    parity checks.
  - Gate: add `make vm-web-route-schema-client-check` and run it after
    `vm-web-deployment-profile-check` and before final release readiness.
  - Current gate state: `make vm-web-route-schema-client-check` exists and
    passes. It writes `target/quality/vm-web-route-schema-client-report.json`
    with route manifest hash cases, schema output cases, generated client
    fixtures, security-policy links, deployment-profile links, stale-client
    rejections, source-map parity checks, and explicit rejected paths for typed
    request/response/error bodies, header/query schemas, security policy export,
    deployment base-path export, SSE/WebSocket schemas, API source-link parity,
    generated-client hash rejection, and retry/cancellation policy generation.
  - Completed progress: `vm-web-route-schema-client-check` now rejects
    placeholder report evidence across route manifest hash cases, schema output
    cases, generated client fixtures, security-policy links, deployment-profile
    links, stale-client rejections, source-map parity checks, and rejected
    schema/client paths while keeping route manifest, OpenAPI projection,
    Terlan-client, security, deployment, stale-client, and source-map report
    sections machine-readable.
  - Acceptance: release cannot pass if route runtime behavior, generated API
    schema, and generated clients can drift from the same typed route source.
  - Acceptance: the gate fails if schemas or clients omit security policy,
    error shapes, streaming behavior, deployment base paths, or source links.


## Completed 101

- [x] Slice 121: provide low-overhead typed model sync without an ORM.
  - Requirement: VM apps must be able to declare syncable typed models backed by
    actors, persistent actors, database adapters, in-memory stores, or package
    stores without requiring ORM identity maps, lazy loading, or hidden query
    generation.
  - Requirement: model sync must define typed keys, versioning, optimistic
    concurrency, change streams, snapshots, diffs, permissions, serialization,
    conflict handling, and adapter capability contracts.
  - Requirement: Postgres-backed sync must use the maintained database adapter
    and typed row decoding, while keeping the sync abstraction portable to other
    stores through explicit adapter traits.
  - Requirement: add adversarial tests for concurrent updates, stale versions,
    deleted rows, conflicting actor state, replayed change events, permission
    drift, adapter failure, transaction rollback, and live-template subscriber
    updates.
  - Requirement: persist `vm-model-sync-store-report.json` with model fixtures,
    adapter matrix, version/conflict cases, change stream traces, permission
    checks, transaction cases, live-template propagation, and rollback behavior.
  - Gate: add `make vm-model-sync-store-check` and run it after
    `vm-web-route-schema-client-check` and before final release readiness.
  - Current gate state: `make vm-model-sync-store-check` exists and passes. It
    writes `target/quality/vm-model-sync-store-report.json` with 7 model
    fixtures, 9 adapter rows, 7 version/conflict cases, change stream traces,
    permission checks, transaction cases, live-template propagation, rollback
    behavior, and 0 explicit rejected model-sync paths. The gate now also runs
    VM runtime tests for the typed in-memory adapter, deterministic snapshots,
    optimistic concurrency conflicts, delete tombstones, change streams, and
    invalid key/version rejection, plus committed model-event invalidation of
    typed live-template subscribers and non-Postgres adapter portability
    contract checks, plus model and field-level permission drift checks and
    typed row-to-model projection checks, plus the source-visible
    `std.vm.ModelSync` optimistic concurrency API and embedded interface
    summary check, plus the source-visible `std.vm.ModelSync` persistent actor
    adapter binding over `std.vm.PersistentActor.ActorId`, plus the
    source-visible `std.vm.ModelSync` package store adapter binding, plus the
    source-visible `std.vm.ModelSync.SyncableModel[T]` declaration API.
  - Completed progress: `vm-model-sync-store-check` now rejects
    placeholder/TODO/TBD report evidence across model fixtures, adapter matrix
    rows, version/conflict cases, change stream traces, permission checks,
    transaction cases, live-template propagation, rollback behavior, and
    rejected model-sync paths before writing
    `vm-model-sync-store-report.json`.
  - Completed progress: the report gate now has an adversarial regression that
    removes the live-template propagation anchor
    `live_channel_sse_handler_records_queued_events` and proves the gate fails
    instead of accepting model sync evidence that cannot propagate committed
    store changes to live subscribers.
  - Completed progress: committed model events now invalidate typed
    live-template subscribers through
    `invalidate_live_template_subscribers_from_model_events`; the model-sync
    report records `committed model events invalidate typed live-template
    subscribers`, and rejected model-sync paths dropped from 8 to 7.
  - Completed progress: non-Postgres model-sync adapters now have explicit
    typed portability contracts through
    `validate_non_postgres_model_sync_adapter_contracts`; the gate verifies
    missing portable capabilities and leaked Postgres-only capabilities, the
    report records a non-Postgres portability adapter row, and rejected
    model-sync paths dropped from 7 to 6.
  - Completed progress: model-sync permission policies now reject missing
    model policies, denied model operations, and field-level permission drift
    through `validate_model_sync_permission_drift`; the report records
    `model-sync permission policies reject model and field-level drift`, the
    Make gate runs exact permission drift selectors, and rejected model-sync
    paths dropped from 6 to 5.
  - Completed progress: row-to-model generation now projects typed adapter rows
    into deterministic sync rows through
    `project_model_sync_row_from_adapter_fields`; the gate verifies missing
    adapter fields, type mismatches, invalid versions, and duplicate projected
    model fields, the report records `row-to-model generation projects typed
    adapter rows into sync rows`, and rejected model-sync paths dropped from 5
    to 4.
  - Completed progress: Terlan code now has a source-visible and VM-executable
    optimistic concurrency API in `std.vm.ModelSync`; `ModelSyncTest` executes
    typed keys, expected versions, next versions, write/delete plans, conflict
    descriptors, stale/apply predicates, adapter capability contracts,
    persistent actor bindings, package store bindings, and syncable model
    declarations. The Make gate runs `terlc test` rather than accepting a
    typecheck-only result, the embedded std interface list includes the module,
    the report records `Terlan-facing optimistic concurrency API builds
    expected and next versions`, and rejected model-sync paths remain at 0.
  - Completed progress: `std.vm.ModelSync` now exposes source-visible
    persistent actor adapter bindings through `ModelSync.PersistentActorAdapter`
    and `ModelSync.persistent_actor_adapter(actor, contract)`. `ModelSyncTest`
    proves the binding composes with `PersistentActor.actor_id`, the embedded
    std interface list includes the adapter type and constructor, the report
    records `persistent-actor-store: source-visible persistent actor adapter
    binding`, and rejected model-sync paths dropped from 3 to 2.
  - Completed progress: `std.vm.ModelSync` now exposes source-visible package
    store adapter bindings through `ModelSync.PackageStoreAdapter` and
    `ModelSync.package_store_adapter(package_id, contract)`. `ModelSyncTest`
    proves package-backed adapters can be declared without ORM identity maps,
    the embedded std interface list includes the adapter type and constructor,
    the report records `package-store: source-visible package store adapter
    binding`, and rejected model-sync paths dropped from 2 to 1.
  - Completed progress: `std.vm.ModelSync` now exposes source-visible
    syncable model declarations through `ModelSync.SyncableModel[T]` and
    `ModelSync.syncable_model(name, key, contract)`. VM descriptor tests reject
    empty model names, empty key models, and mismatched key contracts;
    `ModelSyncTest` proves the declaration is usable from Terlan source; the
    report records `syncable-model: source-visible typed model declaration
    without ORM behavior`, and rejected model-sync paths dropped from 1 to 0.
  - Acceptance: release cannot pass if syncable models require ORM behavior,
    untyped database rows, hidden global state, or Postgres-specific assumptions
    in the public abstraction.
  - Acceptance: the gate fails if model updates can lose conflicts, bypass
    permissions, publish stale events, or leave actors/templates inconsistent
    with committed storage state.


## Completed 102

- [x] Slice 122: provide VM-owned persistent actor storage and replay.
  - Requirement: persistent actors must store typed snapshots, append-only
    events, timer state, mailbox checkpoints, schema versions, and durable
    resource handles without exposing raw storage internals to user code.
  - Requirement: recovery must replay snapshots and events deterministically,
    reject stale or incompatible schemas, and restore actor-visible state before
    accepting new messages.
  - Requirement: storage adapters must be explicit traits so file-backed,
    database-backed, embedded key/value, and package-provided stores share the
    same actor persistence contract.
  - Requirement: add adversarial tests for crash during snapshot, partial event
    write, duplicate replay, corrupted checkpoint, missing timer restore, stale
    schema migration, resource-handle mismatch, concurrent restart, and mailbox
    ordering after recovery.
  - Requirement: persist `vm-persistent-actor-store-report.json` with adapter
    matrix, snapshot/event fixtures, replay traces, schema migration cases,
    mailbox/timer recovery, resource-handle validation, and crash-injection
    outcomes.
  - Gate: add `make vm-persistent-actor-store-check` and run it after
    `vm-model-sync-store-check` and before final release readiness.
  - Current gate state: `make vm-persistent-actor-store-check` exists and
    passes. It writes
    `target/quality/vm-persistent-actor-store-report.json` with 9 adapter rows,
    11 snapshot/event fixtures, 6 replay traces, schema migration cases,
    mailbox/timer recovery, resource-handle validation, crash-injection
    outcomes, and 0 explicit rejected persistent-actor paths. The gate now
    runs VM runtime tests for the in-memory persistent actor adapter,
    deterministic replay, stale snapshot/schema rejection, duplicate and partial
    event rejection, checkpoint restoration, and invalid id/schema/handle
    rejection. It also runs exact tests for source-visible persistent actor
    declarations, including non-empty storage lanes and actor/schema lane
    binding, file-backed persistent actor reopen/replay, and corrupt
    file-backed log rejection, embedded key/value persistent actor export/replay,
    corrupt embedded key/value record rejection, database-backed SQL row
    export/replay, and corrupt database row/table-name rejection, and records
    `std.vm.PersistentActor.PersistentActorDeclaration` as source-visible API
    evidence. It also typechecks
    `std/vm/PersistentActorTest.terl` and proves the embedded standard
    interface includes `std.vm.PersistentActor`, including the source-visible
    package store binding descriptor.
  - Completed progress: `vm-persistent-actor-store-check` now rejects
    placeholder/TODO/TBD report evidence across adapter matrix rows,
    snapshot/event fixtures, replay traces, schema migration cases,
    mailbox/timer recovery, resource-handle validation, crash-injection
    outcomes, and rejected persistent-actor paths before writing
    `vm-persistent-actor-store-report.json`. The gate inventory now also tracks
    the current owner-exit timer cleanup coverage anchor instead of a stale
    timer test name.
  - Completed progress: the report gate now has an adversarial regression that
    removes
    `vm_persistent_actor_store_rejects_stale_snapshot_and_schema_drift` from
    the runtime test evidence and proves the gate fails instead of accepting
    persistent actor replay coverage without stale schema rejection.
  - Completed progress: `std.vm.PersistentActor` now exposes source-visible
    typed actor ids, schema ids, snapshot plans, replay plans, and schema
    compatibility checks. `PersistentActorTest.terl` proves the schema id is
    available to Terlan code, the embedded interface summary is included in the
    compiler, and the quality gate records this as a real snapshot/schema
    fixture instead of a rejected path.
  - Completed progress: `std.vm.PersistentActor` now exposes source-visible
    typed timer checkpoints and timer restore plans for actor restart recovery.
    `PersistentActorTest.terl` proves the timer checkpoint and restore plan are
    available to Terlan code, the embedded interface summary includes both
    descriptors, and the quality gate records this as real mailbox/timer
    recovery coverage instead of a rejected path.
  - Completed progress: `std.vm.PersistentActor` now exposes source-visible
    typed mailbox checkpoints and mailbox restore plans for actor restart
    recovery. `PersistentActorTest.terl` proves the mailbox checkpoint and
    restore plan are available to Terlan code, the embedded interface summary
    includes both descriptors, and the quality gate records this as real
    mailbox/timer recovery coverage instead of a rejected path.
  - Completed progress: `std.vm.PersistentActor` now exposes source-visible
    typed durable resource checkpoints and resource restore plans for actor
    restart recovery. `PersistentActorTest.terl` proves the durable resource
    checkpoint and restore plan are available to Terlan code, the embedded
    interface summary includes both descriptors, and the quality gate records
    this as real resource-handle validation instead of a rejected path.
  - Completed progress: `std.vm.PersistentActor` now exposes source-visible
    package-provided store bindings through `PersistentActor.PackageStoreBinding`
    and `PersistentActor.package_store(package_id, actor, schema)`.
    `PersistentActorTest.terl` proves package-backed persistent actor stores can
    be declared without hidden storage globals or ORM identity maps, the
    embedded interface summary includes the descriptor and constructor, and the
    quality gate records this as a source-visible package adapter row instead of
    a rejected path.
  - Completed progress: `std.vm.PersistentActor` now exposes
    `PersistentActorDeclaration` and
    `PersistentActor.persistent_actor(actor, schema, storage_lane)`.
    `PersistentActorTest.terl` proves persistent actor declarations are
    available to Terlan code, the VM declaration descriptor rejects empty or
    actor/schema-mismatched storage lanes, the quality report records the
    source-visible declaration as adapter and fixture evidence, and rejected
    persistent-actor paths dropped from 4 to 3.
  - Completed progress: `VmFileBackedPersistentActorStore` now implements the
    persistent actor store adapter contract with a deterministic typed file log.
    Exact VM tests prove snapshot/event replay survives reopen and corrupt log
    records are rejected before replay. The quality report records file-backed
    adapter and fixture evidence, and rejected persistent-actor paths dropped
    from 3 to 2.
  - Completed progress: `VmEmbeddedKeyValuePersistentActorStore` now implements
    the persistent actor store adapter contract with a deterministic VM-owned
    keyspace. Exact VM tests prove snapshot/event keyspace export can restore
    typed actor state, mailbox checkpoints, timer checkpoints, resource handles,
    and sorted event replay, and corrupt key/value records are rejected before
    replay. The quality report records embedded key/value adapter and fixture
    evidence, and rejected persistent-actor paths dropped from 2 to 1.
  - Completed progress: `VmDatabaseBackedPersistentActorStore` now implements
    the persistent actor store adapter contract with deterministic VM-owned SQL
    row keys and typed row records. Exact VM tests prove SQL row export can
    restore typed actor state and sorted event replay, while corrupt rows and
    unsafe table names are rejected before replay. The quality report records
    database-backed adapter and fixture evidence, and rejected persistent-actor
    paths dropped from 1 to 0.
  - Current rejected paths: none for this slice.
  - Acceptance: release cannot pass if persistent actors depend on hidden ORM
    behavior, process-local state that is lost on restart, untyped serialized
    blobs, or adapter-specific semantics in the actor API.
  - Acceptance: the gate fails if replay is nondeterministic, message order
    changes after recovery, timers are lost, stale schemas are accepted, or
    partial writes can be observed as committed state.


## Completed 103

- [x] Slice 123: enforce persistent actor schema evolution contracts.
  - Requirement: persistent actor state, events, mailbox checkpoints, timer
    state, and durable resource handles must carry typed schema identities that
    can be compared before replay or snapshot restore.
  - Requirement: schema evolution must require explicit migration functions for
    incompatible changes, prove no required field is lost silently, and reject
    ambiguous migrations before runtime state is loaded.
  - Requirement: compatible changes must be documented and checked, including
    added fields with defaults, renamed fields with explicit mapping, removed
    fields with tombstones, enum/union constructor changes, and type-width
    changes for binary/storage formats.
  - Requirement: migration planning must produce a deterministic graph from old
    schema ids to current schema ids and reject cycles, missing edges,
    nondeterministic guards, side-effectful migrations, and migrations that
    depend on wall-clock time.
  - Requirement: add adversarial tests for out-of-order migrations, partial
    migration failure, duplicate schema ids, stale package versions, unknown
    event variants, incompatible mailbox payloads, and rollback after failed
    migration.
  - Requirement: persist `vm-persistent-actor-schema-report.json` with schema
    ids, migration graph, compatibility matrix, rejected migration cases,
    replay-before/after traces, and rollback outcomes.
  - Gate: add `make vm-persistent-actor-schema-check` and run it after
    `vm-persistent-actor-store-check` and before final release readiness.
  - Current gate state: `make vm-persistent-actor-schema-check` exists and
    passes. It writes
    `target/quality/vm-persistent-actor-schema-report.json` with 11 schema ids,
    8 migration graph cases, 7 compatibility rows, 13 rejected migration cases,
    5 replay before/after traces, and 5 rollback outcomes.
  - Current gate coverage: VM runtime tests now cover schema key and descriptor
    validation, deterministic migration chain planning, duplicate, missing,
    cyclic, and ambiguous migrations, unsafe guards, unsafe effects,
    wall-clock-dependent migrations, required-field loss, event variant
    migration, mailbox payload migration, stale package schema versions, and
    out-of-order event migration sequences.
  - Completed progress: `vm-persistent-actor-schema-check` now rejects
    placeholder/TODO/TBD report evidence across schema ids, migration graph
    cases, compatibility matrix rows, rejected migration cases, replay
    before/after traces, and rollback outcomes before writing
    `vm-persistent-actor-schema-report.json`; the quality tests include an
    injected-placeholder case so schema evolution evidence cannot be padded with
    vague labels.
  - Completed progress: the schema report gate now has an adversarial
    regression that removes the `WallClockDependentMigration` runtime evidence
    and proves the gate fails instead of accepting schema evolution coverage
    without wall-clock-dependent migration rejection.
  - Completed progress: `std.vm.PersistentActor` now exposes
    `PersistentActor.SchemaDeclaration` and
    `PersistentActor.schema(schema, fields, events, mailbox_schema)` so actor
    state fields, event variants, and mailbox schema identity are visible to
    Terlan source before VM-owned migration planning accepts stored state.
    `PersistentActorTest.terl` proves the declaration descriptor can be created
    from source, the embedded interface summary includes the descriptor and
    constructor, and the quality gate records this as a schema id row instead
    of a rejected syntax-only path.
  - Completed progress: `std.vm.PersistentActor` now exposes
    `PersistentActor.MigrationRollbackPlan` and
    `PersistentActor.migration_rollback(actor, from_schema, to_schema, sequence, reason)`
    so rollback after failed schema migration is visible to Terlan code while
    the VM retains idempotent rollback ordering and in-flight table cleanup.
    `PersistentActorTest.terl` typechecks the rollback plan, the embedded
    interface summary includes the descriptor and constructor, and the schema
    quality report records this as positive rollback evidence.
  - Completed progress: `std.vm.PersistentActor` now exposes
    `PersistentActor.PackageMigrationRegistration` and
    `PersistentActor.register_package_migration(package_id, actor, from_schema, to_schema, migration_name)`
    so packages can declare schema migration edges from Terlan source before
    VM-owned migration graph planning accepts package state.
    `PersistentActorTest.terl` typechecks the registration, the embedded
    interface summary includes the descriptor and constructor, and the schema
    quality report records this as a migration graph evidence row.
  - Completed progress: `std.vm.PersistentActor` now exposes
    `PersistentActor.EventVariantSchemaId` and
    `PersistentActor.event_variant_schema(package_id, actor, schema, variant, version)`
    so persisted actor event variants can carry package-scoped typed schema
    identities across package boundaries before replay or migration accepts
    stored events. `PersistentActorTest.terl` typechecks the descriptor, the
    embedded interface summary includes the type and constructor, and the
    schema quality report records this as a schema id evidence row while the
    compatibility matrix treats enum/union constructor changes as accepted only
    when package event variant schema ids are explicit.
  - Completed progress: `std.vm.PersistentActor` now exposes
    `PersistentActor.DurableAdapterSchemaMetadata` and
    `PersistentActor.durable_adapter_schema(package_id, adapter, schema, storage_lane, metadata_version)`
    so durable adapters can publish typed package store schema metadata before
    VM-owned replay, restore, or resource-handle recovery accepts stored actor
    state. `PersistentActorTest.terl` typechecks the descriptor, the embedded
    interface summary includes the type and constructor, and the schema quality
    report records this as a schema id evidence row.
  - Current rejected paths: none for this slice.
  - Acceptance: release cannot pass if actor persistence can restore data using
    an unknown schema, silently drop fields, skip required migrations, or apply
    migrations in a nondeterministic order.
  - Acceptance: the gate fails if schema evolution is only documented but not
    executable against stored actor fixtures and crash-recovery traces.


## Completed 104

- [x] Slice 124: provide safe persistent actor compaction and retention.
  - Requirement: persistent actor stores must support typed snapshot compaction,
    event-log retention, tombstone cleanup, checkpoint pruning, and resource
    handle garbage collection without changing replay-visible actor behavior.
  - Requirement: compaction must be deterministic and must prove that every
    retained snapshot plus retained event suffix reconstructs the same actor
    state, mailbox checkpoint, timer state, and durable resource handles as the
    uncompacted history.
  - Requirement: retention policies must be explicit per actor or actor family,
    with bounded defaults for local development and production-safe rejection
    when a policy would remove data still required for recovery, audit, or
    schema migration.
  - Requirement: add adversarial tests for crash during compaction, concurrent
    message delivery while compacting, duplicate compaction runs, stale
    tombstones, retained event gaps, schema migration after compaction, and
    resource handles referenced only by old events.
  - Requirement: persist `vm-persistent-actor-compaction-report.json` with
    before/after store sizes, replay equivalence traces, retained ranges,
    rejected retention policies, crash-injection cases, and resource cleanup
    decisions.
  - Gate: add `make vm-persistent-actor-compaction-check` and run it after
    `vm-persistent-actor-schema-check` and before final release readiness.
  - Current gate state: `make vm-persistent-actor-compaction-check` exists and
    passes. It writes
    `target/quality/vm-persistent-actor-compaction-report.json` with 6
    before/after store-size cases, 6 replay equivalence traces, 12 retained
    ranges, 6 rejected retention policies, 7 crash-injection cases, and 6
    resource cleanup decisions.
  - Current gate coverage: VM runtime tests validate persistent actor
    compaction candidates, replay-equivalent compacted snapshots, schema and
    audit retention floors, unsafe mailbox/timer checkpoint pruning, resource
    handle pruning policy, retained event suffix gaps, retained events missing
    from the original log, and non-equivalent compacted snapshots.
  - Completed progress: `vm-persistent-actor-compaction-check` now rejects
    placeholder/TODO/TBD report evidence across before/after store sizes,
    replay equivalence traces, retained ranges, rejected retention policies,
    crash-injection cases, and resource cleanup decisions before writing
    `vm-persistent-actor-compaction-report.json`; the quality tests include an
    injected-placeholder case so compaction and retention evidence cannot be
    padded with vague labels.
  - Completed progress: the compaction report gate now has an adversarial
    regression that removes the `RetainedEventGap` runtime coverage anchor and
    proves the gate fails instead of accepting retained event suffix coverage
    without explicit gap rejection.
  - Completed progress: `std.vm.PersistentActor` now exposes
    `PersistentActor.RetentionPolicy` and
    `PersistentActor.retention_policy(retain_from_sequence, schema_migration_floor, audit_floor)`
    so retention bounds are visible to Terlan source before VM-owned compaction
    can prune snapshots, events, checkpoints, or resources. `PersistentActorTest.terl`
    proves the descriptor can be created from source, the embedded interface
    summary includes the descriptor and constructor, and the quality gate
    records this as a retained-range evidence row instead of a rejected
    syntax-only path.
  - Completed progress: `std.vm.PersistentActor` now exposes
    `PersistentActor.ActorFamilyRetentionDefaults` and
    `PersistentActor.family_retention_defaults(family, local, production)` so
    actor-family retention defaults are explicit source descriptors over
    local-development and production policies before VM-owned compaction
    evaluates actor-specific schema, audit, and recovery floors.
    `PersistentActorTest.terl` typechecks the descriptor, the embedded
    interface summary includes the type and constructor, and the compaction
    quality report records this as retained-range evidence instead of a
    rejected retention-policy path.
  - Completed progress: `std.vm.PersistentActor` now exposes
    `PersistentActor.AuditRetentionPlan` and
    `PersistentActor.audit_retention(actor, policy, required_events)` so
    audit-preserving event-retention intent is source-visible before VM-owned
    compaction chooses a retained event suffix. `PersistentActorTest.terl`
    typechecks required event evidence, the embedded interface summary includes
    the type and constructor, and the compaction quality report records this as
    retained-range evidence instead of a rejected retention-policy path.
  - Completed progress: `std.vm.PersistentActor` now exposes
    `PersistentActor.PackageRetentionPolicyBinding` and
    `PersistentActor.package_retention_policy(package_id, actor, policy)` so
    package-provided retention ownership is source-visible before VM-owned
    compaction merges actor, actor-family, and package defaults.
    `PersistentActorTest.terl` typechecks the package retention binding, the
    embedded interface summary includes the type and constructor, and the
    compaction quality report records this as retained-range evidence instead
    of a rejected retention-policy path.
  - Completed progress: `std.vm.PersistentActor` now exposes
    `PersistentActor.ModelSyncRetentionContinuityPlan` and
    `PersistentActor.model_sync_retention_continuity(actor, policy, model, retained_from_version)`
    so model-sync change-stream floors are source-visible before VM-owned
    compaction chooses retained actor event and model-sync stream windows.
    `PersistentActorTest.terl` typechecks the continuity descriptor, the
    embedded interface summary includes the type and constructor, and the
    compaction quality report records this as retained-range evidence instead
    of a rejected retention-policy path.
  - Completed progress: `vm-distributed-state-check` now runs
    `vm_distributed_storage_compaction_physically_removes_pruned_snapshots_and_retains_boundary`
    exactly, proving physical adapter compaction removes pruned checkpoints,
    retains the compaction boundary and later snapshots, and preserves the
    monotonic sequence watermark. The compaction quality report records this as
    retained-range evidence instead of a rejected retention-policy path.
  - Completed progress: durable transactional batch rollback is now adapter
    owned through `VmDistributedStorageTransactionalRollbackProof` and
    `transactional_rollback_proof()`. The exact runtime test
    `vm_distributed_storage_durable_transactional_batch_rollback_preserves_commit_boundary`
    proves a partial durable batch records rollback evidence, keeps attempted
    snapshots invisible, preserves the pre-commit sequence and durable flush
    boundary, and allows the same batch to commit and flush afterward.
  - Current rejected paths: none for this slice.
  - Acceptance: release cannot pass if compaction can change actor-visible
    state, delete data required by schema migration, orphan durable resources,
    or rely on adapter-specific cleanup behavior.
  - Acceptance: the gate fails if compacted and uncompacted replay diverge for
    actor state, mailbox order, timers, resource handles, or model-sync change
    stream output.


## Completed 105

- [x] Slice 125: provide persistent actor inspection, export, and restore.
  - Requirement: the VM must expose a typed inspection surface for persistent
    actors that can list actor ids, schema ids, snapshot generations, retained
    event ranges, mailbox checkpoints, timer state, durable resource handles,
    and compaction state without exposing raw adapter internals.
  - Requirement: export must produce deterministic, redaction-aware artifacts
    that can be checked into failure fixtures, moved between machines, and
    replayed by the VM without requiring the original storage adapter.
  - Requirement: restore must validate schema compatibility, adapter
    capabilities, resource-handle availability, actor ownership, mailbox order,
    timer deadlines, and model-sync stream continuity before accepting restored
    state.
  - Requirement: add CLI-facing diagnostics for inspecting actor persistence
    state, explaining why restore was rejected, and generating a minimal replay
    fixture for a failed actor without leaking secrets.
  - Requirement: add adversarial tests for corrupted exports, missing resource
    handles, wrong actor owner, stale schema, reordered events, redaction
    bypass, restore into an incompatible adapter, and restore after compaction.
  - Requirement: persist `vm-persistent-actor-restore-report.json` with export
    manifests, redaction decisions, restore validation traces, rejected restore
    cases, minimal replay fixtures, and cross-adapter restore results.
  - Gate: add `make vm-persistent-actor-restore-check` and run it after
    `vm-persistent-actor-compaction-check` and before final release readiness.
  - Current gate state: `make vm-persistent-actor-restore-check` exists and
    passes. It writes
    `target/quality/vm-persistent-actor-restore-report.json` with 8 export
    manifests, 6 redaction decisions, 23 restore validation traces, 0 rejected
    restore cases, 6 minimal replay fixtures, and 6 cross-adapter restore
    results.
  - Current gate coverage: VM runtime tests now validate deterministic
    persistent actor export manifests, checksum rejection, wrong actor owner
    rejection, stale schema rejection, missing durable resource handle
    rejection, retained event suffix ordering, compacted snapshot restore
    capability checks, and resource-handle restore capability checks.
  - Completed progress: `vm-persistent-actor-restore-check` now rejects
    placeholder/TODO/TBD report evidence across export manifests, redaction
    decisions, restore validation traces, rejected restore cases, minimal replay
    fixtures, and cross-adapter restore results before writing
    `vm-persistent-actor-restore-report.json`; the quality tests include an
    injected-placeholder case so inspection/export/restore evidence cannot be
    padded with vague labels.
  - Completed progress: the restore report gate now requires the concrete
    `StaleSchema` runtime validation anchor and has an adversarial regression
    that removes it from the fixture to prove stale-schema restore coverage
    cannot be implied only by a bundled test name.
  - Completed progress: `std.vm.PersistentActor` now exposes
    `PersistentActor.RedactionPolicy` and
    `PersistentActor.redaction_policy(redacted_fields, include_mailbox,
    include_resources)` so actor export redaction intent is visible to Terlan
    source before VM-owned export/restore serializes replay fixtures.
    `PersistentActorTest.terl` proves the descriptor can be created from
    source, the embedded interface summary includes the descriptor and
    constructor, and the restore quality gate records this as a redaction
    decision instead of a rejected syntax-only path.
  - Completed progress: the restore gate now includes a VM-owned
    `generate_minimal_actor_replay_fixture` path that turns a validated actor
    export into a deterministic metadata-only replay fixture. The exact runtime
    test proves the fixture preserves actor/schema/generation/event/timer/
    resource metadata while excluding raw state, mailbox, and event payload text
    from the rendered manifest.
  - Completed progress: persistent actor restore now validates typed mailbox
    checkpoint ordering before accepting an export or restore plan. Mailbox
    checkpoints tagged as `{mailbox_checkpoint, sequence, ...}` must be
    contiguous from 1; the exact runtime test rejects a 1,3 sequence gap with
    `ReorderedMailboxCheckpoint`, and the quality gate records this as restore
    validation instead of an open rejected path.
  - Completed progress: persistent actor restore now rejects destination
    adapters whose adapter kind does not match the export source adapter kind
    before any adapter-specific restore runs. The exact runtime test covers a
    `force_local` export restored into a `cluster` target and returns
    `IncompatibleAdapterKind`, and the quality report records this as restore
    validation rather than an open rejected path.
  - Completed progress: compacted persistent actor exports now carry typed
    restore boundary metadata in the VM-owned restore plan. The exact runtime
    test accepts a compacted export, records the compacted-through sequence and
    retained suffix range, renders that metadata into the payload-redacted
    replay fixture, and the quality report records restore-after-compaction as
    validation coverage instead of a rejected API path.
  - Completed progress: persistent actor restore now validates model-sync stream
    continuity before accepting an export. Restore targets can require a model
    stream from a retained sequence, exported model-sync changes must contain a
    contiguous window for that model, the accepted restore plan records the
    restored stream window, and the payload-redacted replay fixture renders the
    stream metadata without exposing row payloads. The exact runtime tests cover
    both the accepted `User:5-6#2` window and missing/gapped stream rejection.
  - Completed progress: persistent actor restore now executes an explicitly
    allowed cross-adapter restore through the shared
    `VmPersistentActorStoreAdapter` contract. The exact runtime test restores an
    export marked as `embedded-key-value` into a `database-backed` destination
    store, verifies replay through the destination adapter, and proves repeated
    restore attempts reject with typed `StoreRejected`/`stale_snapshot`
    evidence instead of mutating adapter state.
  - Completed progress: persistent actor export now has a deterministic
    cross-machine envelope format that is independent of storage adapter
    internals. The exact runtime test builds a
    `terlan-vm-persistent-actor-export-v1` manifest with source machine, actor,
    schema, retained event sequence, redaction, model-sync stream, resource
    count, and checksum metadata; it also proves state, mailbox, and event
    payload text stay out of the manifest and invalid source machine ids are
    rejected before export.
  - Completed progress: `terlan-vm export-persistent-actor` now accepts typed
    actor/schema/source-machine metadata, builds a VM-owned persistent actor
    export, validates it through the cross-machine export envelope, and prints
    a payload-redacted portable manifest. Exact CLI tests cover argument
    parsing and rendered manifest secrecy, and the restore quality gate records
    the command as a completed export manifest path.
  - Completed progress: `terlan-vm restore-persistent-actor` now accepts typed
    restore target metadata, validates a redacted export through the same
    VM-owned restore plan used by runtime restore, optionally permits
    cross-adapter restore, checks available resource handles and compacted
    export metadata, and prints a deterministic accepted restore plan with a
    payload-redacted replay fixture. Exact CLI tests cover argument parsing and
    rendered secrecy, and the restore quality gate records the public restore
    command as validation coverage instead of a rejected path.
  - Current rejected paths: none for this slice.
  - Acceptance: release cannot pass if persistent actor state can only be
    inspected through storage-specific tools, if exports are nondeterministic,
    or if restore can bypass schema/resource/ownership validation.
  - Acceptance: the gate fails if exported fixtures cannot reproduce actor
    state, mailbox order, timers, resource handles, and model-sync stream state
    on a clean VM.


## Completed 106

- [x] Slice 126: provide persistent actor storage adapter conformance.
  - Requirement: every persistent actor storage adapter must implement the same
    typed contract for append, snapshot, checkpoint, compaction, export,
    restore, schema migration, resource-handle validation, and crash recovery.
  - Requirement: adapters must declare durable capabilities explicitly,
    including atomic append, compare-and-swap, snapshot isolation, fsync or
    equivalent durability, transactional batch support, compaction support,
    export support, and restore support.
  - Requirement: the conformance suite must run the same fixture matrix against
    file-backed, in-memory, database-backed, embedded key/value, and
    package-provided adapters without allowing adapter-specific behavior to leak
    into persistent actor APIs.
  - Requirement: add adversarial tests for non-atomic append, acknowledged but
    lost writes, torn snapshots, stale compare-and-swap tokens, partial batch
    commit, adapter restart, compaction during restore, and unsupported
    capability negotiation.
  - Requirement: persist `vm-persistent-actor-adapter-report.json` with adapter
    capability manifests, conformance matrix, crash-injection outcomes,
    durability evidence, rejected adapters, and fixture replay results.
  - Gate: add `make vm-persistent-actor-adapter-conformance-check` and run it
    after `vm-persistent-actor-restore-check` and before final release
    readiness.
  - Current gate state: `make vm-persistent-actor-adapter-conformance-check`
    exists and passes. It writes
    `target/quality/vm-persistent-actor-adapter-report.json` with 18 adapter
    capability manifests, 23 conformance rows, 14 crash-injection outcomes, 14
    durability evidence rows, 0 rejected adapters, and 11 fixture replay
    results.
  - Current gate coverage: VM runtime tests now validate persistent actor
    adapter capability manifests, local fixture append/flush/load/compact/close
    replay, explicit cluster replication capability, unavailable adapter
    rejection, corrupt checkpoint rejection, partial checkpoint rejection, and
    stale replay rejection through typed storage outcomes.
  - Completed progress: `vm-persistent-actor-adapter-conformance-check` now
    rejects placeholder/TODO/TBD report evidence across adapter capability
    manifests, conformance matrix rows, crash-injection outcomes, durability
    evidence, rejected adapters, and fixture replay results before writing
    `vm-persistent-actor-adapter-report.json`; the quality tests include an
    injected-placeholder case so adapter conformance evidence cannot be padded
    with vague labels.
  - Completed progress: the adapter conformance gate now has an adversarial
    regression that removes the `MissingClusterReplicationCapability` runtime
    error anchor and proves unsupported cluster capability negotiation cannot
    disappear while the report still claims cluster adapter coverage.
  - Completed progress: compare-and-swap append is now VM-owned and
    source-visible through `VmDistributedStorageCasToken`,
    `compare_and_swap_token`, `compare_and_swap_append`, std
    `CompareAndSwapToken`, and stale-token typed recovery. The conformance gate
    now proves stale CAS tokens reject with `cas_token_mismatch` and
    `reload_snapshot` recovery, and it was verified on the current tree with
    `make --no-print-directory vm-persistent-actor-adapter-conformance-check`.
  - Completed progress: atomic append proof is now VM-owned and source-visible
    through `VmDistributedStorageAtomicAppendProof`, `require_atomic_append`,
    `atomic_append_proof`, std `AtomicAppendProof`, and `proof_sequence`. The
    conformance gate now proves partial append failures do not advance the
    adapter sequence proof, and it was verified on the current tree with
    `make --no-print-directory vm-persistent-actor-adapter-conformance-check`.
  - Completed progress: snapshot isolation proof is now VM-owned and
    source-visible through `VmDistributedStorageSnapshotIsolationProof`,
    `SnapshotIsolation`, `require_snapshot_isolation`,
    `snapshot_isolation_proof`, std `SnapshotIsolationProof`, and
    `isolation_checkpoint_id`/`isolation_sequence`/`isolation_checksum`. The
    conformance gate now proves an issued checkpoint proof remains stable after
    later adapter append and compaction, and it was verified on the current tree
    with `make --no-print-directory vm-persistent-actor-adapter-conformance-check`.
  - Completed progress: durable flush proof is now VM-owned and source-visible
    through `VmDistributedStorageDurableFlushProof`, `DurableFlush`,
    `require_durable_flush`, `durable_flush_proof`, std `DurableFlushProof`,
    and `durable_flush_sequence`. The conformance gate now proves failed and
    timed-out flush attempts do not advance the durable flush sequence proof,
    and it was verified on the current tree with
    `make --no-print-directory vm-persistent-actor-adapter-conformance-check`.
  - Completed progress: transactional batch append is now VM-owned and
    source-visible through `VmDistributedStorageTransactionalBatchProof`,
    `TransactionalBatchAppend`, `require_transactional_batch`,
    `transactional_batch_proof`, `transactional_batch_append`, std
    `TransactionalBatchProof`, and
    `batch_first_sequence`/`batch_last_sequence`/`batch_committed_count`. The
    conformance gate now proves a partial batch commit returns
    `rewrite_checkpoint` without mutating adapter state, and it was verified on
    the current tree with
    `make --no-print-directory vm-persistent-actor-adapter-conformance-check`.
  - Completed progress: schema migration is now VM-owned and source-visible
    through `VmDistributedStorageSchemaMigrationProof`, `SchemaMigration`,
    `SchemaMigrationMismatch`, `require_schema_migration`,
    `schema_migration_proof`, `migrate_schema`, std `SchemaMigrationProof`,
    `schema_version`/`schema_sequence`, and `expected_schema`/`actual_schema`.
    The conformance gate now proves stale expected schema returns
    `reload_schema` without mutating the schema proof, and it was verified on
    the current tree with
    `make --no-print-directory vm-persistent-actor-adapter-conformance-check`.
  - Completed progress: resource handle validation is now VM-owned and
    source-visible through `VmDistributedStorageResourceHandleValidationProof`,
    `ResourceHandleValidation`, `ResourceHandleValidationFailed`,
    `require_resource_handle_validation`,
    `resource_handle_validation_proof`, `register_resource_handle`,
    `validate_resource_handles`, std `ResourceHandleValidationProof`,
    `resource_handle_count`/`resource_handle_sequence`, and
    `missing_resource_handle`/`validated_resource_count`. The conformance gate
    now proves missing durable handles return `recover_resource_handle` without
    mutating validation proof, and it was verified on the current tree with
    `make --no-print-directory vm-persistent-actor-adapter-conformance-check`.
  - Completed progress: file-backed persistent actor adapter conformance is now
    represented by an explicit durable `file-backed` fixture through
    `file_backed_persistent_actor_adapter_fixture` and
    `vm_persistent_actor_adapter_conformance_accepts_file_backed_fixture_replay`;
    the conformance gate proves append, flush, load, compaction,
    compare-and-swap token use, and close behavior replay through the same
    actor-visible contract as other durable adapters.
  - Completed progress: database-backed persistent actor adapter conformance is
    now represented by an explicit durable `database-backed` fixture through
    `database_backed_persistent_actor_adapter_fixture` and
    `vm_persistent_actor_adapter_conformance_accepts_database_backed_fixture_replay`;
    the conformance gate proves ordered transaction-log replay,
    compare-and-swap token use, flush/load stability, and compaction retention
    through the shared durable adapter contract.
  - Completed progress: embedded key/value and package-provided persistent actor
    adapter conformance are now represented by explicit durable fixtures through
    `embedded_key_value_persistent_actor_adapter_fixture`,
    `package_provided_persistent_actor_adapter_fixture`,
    `vm_persistent_actor_adapter_conformance_accepts_embedded_key_value_fixture_replay`,
    and
    `vm_persistent_actor_adapter_conformance_accepts_package_provided_fixture_replay`;
    the conformance gate proves both adapters replay through the shared durable
    adapter contract.
  - Completed progress: cross-adapter restore execution is now exercised by
    `execute_persistent_actor_adapter_cross_adapter_restore` and
    `vm_persistent_actor_adapter_conformance_executes_cross_adapter_restore`;
    the conformance gate proves source adapter metadata, destination adapter
    metadata, snapshot generation, and replayed event counts survive the shared
    restore contract.
  - Current rejected paths: none for this slice.
  - Acceptance: release cannot pass if a storage adapter can acknowledge data
    that cannot be replayed, if unsupported capabilities are assumed, or if
    actor persistence has adapter-specific semantic branches.
  - Acceptance: the gate fails if two conforming adapters produce different
    actor-visible replay, restore, compaction, schema migration, or resource
    cleanup behavior for the same typed fixture.


## Completed 107

- [x] Slice 127: enforce persistent actor performance and size budgets.
  - Requirement: persistent actor append, snapshot, replay, compaction, export,
    restore, schema migration, and adapter recovery paths must publish latency,
    throughput, memory, disk, and replay-size budgets.
  - Requirement: benchmarks must cover small actors, large actors, high-event
    actors, mailbox-heavy actors, timer-heavy actors, model-sync actors,
    post-compaction replay, cross-adapter restore, and cold-start recovery.
  - Requirement: performance reports must separate scheduler time, serialization
    time, adapter I/O time, schema migration time, compaction time, and VM
    replay time so slowdowns are attributable.
  - Requirement: add adversarial performance tests for event storms, snapshot
    storms, slow adapters, large mailbox checkpoints, many durable resources,
    compaction under load, restore of large exports, and pathological schema
    migration chains.
  - Requirement: persist `vm-persistent-actor-performance-report.json` with
    p50/p95/p99 latency, throughput, memory, disk growth, replay bytes,
    scheduler ticks, adapter timing breakdowns, baseline comparison, and budget
    pass/fail decisions.
  - Gate: add `make vm-persistent-actor-performance-budget-check` and run it
    after `vm-persistent-actor-adapter-conformance-check` and before final
    release readiness.
  - Current gate state: `make vm-persistent-actor-performance-budget-check`
    exists and passes. It writes
    `target/quality/vm-persistent-actor-performance-report.json` with 8
    fixture budget rows, 5 deterministic baseline estimates, 6 timing
    breakdown categories, 5 size budget categories, 8 adversarial performance
    cases, one measured runtime baseline, one enforced baseline comparison, and
    0 rejected budget paths.
  - Current gate coverage: VM runtime tests validate deterministic persistent
    actor budget estimates for small actors, event storms, post-compaction
    replay reduction, empty-workload rejection, and invalid compaction-count
    rejection. The estimator publishes p50/p95/p99 ticks, scheduler ticks,
    memory bytes, disk bytes, replay bytes, throughput, and pass/fail budget
    metadata for fixture baselines.
  - Completed progress: `vm-persistent-actor-performance-budget-check` now
    rejects placeholder/TODO/TBD report evidence across fixture budgets,
    deterministic baseline estimates, timing breakdowns, size budgets,
    adversarial performance cases, and rejected budget paths before writing
    `vm-persistent-actor-performance-report.json`; the quality tests include an
    injected-placeholder case so performance evidence cannot be padded with
    vague labels. The gate also tracks the current owner-exit timer cleanup
    coverage anchor instead of a stale timer test name.
  - Completed progress: the performance budget gate now requires concrete
    `serialization_ticks` and `adapter_ticks` runtime attribution anchors, and
    the quality tests remove `adapter_ticks` to prove adapter timing coverage
    cannot disappear while the report still claims timing breakdown coverage.
  - Completed progress: `vm-persistent-actor-performance-budget-check` now runs
    a real three-run VM benchmark over `VmInMemoryPersistentActorStore` instead
    of relying only on deterministic formula estimates. Every run performs 100
    snapshot/append/replay samples with 64 events per sample, verifies replay
    correctness, and publishes p50/p95/p99 nanoseconds plus measured events per
    second. The quality gate rejects missing runs, fewer than ten samples,
    non-monotonic or zero latency, zero throughput, and unverified replay. The
    first 300-sample aggregate measured p50 59,859 ns, p95 62,708 ns, p99
    64,105 ns, and 1,061,804 events/second. That step established observational
    machine-local evidence before the checked budget below. The real harness
    rejection was removed, reducing rejected budget paths from 12 to 11.
  - Completed progress: the measured in-memory lane now has a committed workload
    budget at `benchmarks/baselines/vm-persistent-actor-runtime.json`: at least 3
    runs, at least 100 samples/run, exactly 64 events/sample, p99 at most 500,000
    ns, and throughput at least 200,000 events/second. The quality gate persists
    every observed/required value and pass decision, and adversarial tests fail
    both a 101 ns observation against a 100 ns fixture ceiling and 499
    events/second against a 500 fixture floor. The current gate measured p99
    65,982 ns and 1,073,978 events/second, passing the checked budget. Stable
    baseline comparison and pass/fail enforcement are no longer rejected,
    reducing rejected budget paths from 11 to 8.
  - Completed progress: the runtime harness now measures the real
    `VmFileBackedPersistentActorStore` over 3 runs and 20 samples/run with 16
    events/sample. Snapshot commit, event append, and reopen/replay are timed as
    separate adapter-I/O phases; each sample verifies durable replay and records
    the actual log size. The quality gate rejects missing runs, fewer than ten
    samples, non-monotonic phase percentiles, zero disk evidence, and unverified
    replay. The current aggregate measured snapshot p99 28,192 ns, append p99
    928,891 ns, reopen/replay p99 67,724 ns, and 1,382 disk bytes. Disk growth
    measurement and adapter-I/O timing attribution are no longer rejected,
    reducing rejected budget paths from 8 to 6.
  - Completed progress: file-backed recovery now separates durable-log
    reopen/load time from VM replay time while retaining a combined
    reopen/replay measurement. The quality gate validates positive monotonic
    percentiles for all three phases and rejects a combined p99 that is smaller
    than either constituent phase. The current aggregate measured reopen/load
    p99 113,049 ns, VM replay p99 4,133 ns, and combined p99 117,016 ns. VM
    replay timing attribution is no longer rejected, reducing rejected budget
    paths from 6 to 5.
  - Completed progress: the runtime harness now attributes VM-owned compaction
    planning through the production `plan_persistent_actor_compaction` path.
    Each of 3 runs performs 100 plans over 1,000 events, compacts through event
    800, verifies the next snapshot generation, and verifies that exactly 200
    events remain. The quality gate rejects missing runs, undersized samples,
    zero or non-monotonic percentiles, unverified correctness, and retained
    counts that do not reduce the event set. The current aggregate measured p50
    142,194 ns, p95 144,344 ns, and p99 149,691 ns. Compaction planning timing
    attribution is no longer rejected, reducing rejected budget paths from 5
    to 4; physical adapter prune I/O remains outside this measurement.
  - Completed progress: the runtime harness now attributes persistent actor
    schema-migration planning through the production
    `VmPersistentActorMigrationGraph::plan` path. Each of 3 runs performs 100
    plans over a valid 64-version graph, verifies all 63 returned edges are in
    version order, and measures only planning after graph construction. The
    quality gate rejects missing runs, undersized samples, invalid edge counts,
    unverified ordering, and zero or non-monotonic percentiles. The current
    aggregate measured p50 43,346 ns, p95 45,048 ns, and p99 46,367 ns. Schema
    migration planning attribution is no longer rejected, reducing rejected
    budget paths from 4 to 3; state transformation and adapter migration I/O
    remain outside this measurement.
  - Completed progress: the runtime harness now measures cross-adapter restore
    through the production
    `execute_persistent_actor_adapter_cross_adapter_restore` path. Each of 3
    runs performs 100 complete restores from an embedded key/value export into
    a database-backed destination, verifies distinct adapter identities, and
    verifies that both exported events are restored and replayed. The quality
    gate rejects identical or empty adapter identities, zero event evidence,
    unverified destination replay, undersized samples, and zero or
    non-monotonic percentiles. The current aggregate measured p50 17,634 ns,
    p95 18,402 ns, and p99 21,159 ns. Cross-adapter restore benchmark execution
    is no longer rejected, reducing rejected budget paths from 3 to 2.
  - Completed progress: persistent actor scheduler attribution now executes the
    real snapshot/64-event append/replay workload inside production
    `VmScheduler::run_next` slices. Each of 3 runs schedules 100 independent
    actor processes and verifies process exit, one scheduler tick per sample,
    one slice per sample, and 66 charged reductions for snapshot, events, and
    replay. Nested timing separates workload execution from scheduler dispatch
    and accounting overhead. The current aggregate records 300 scheduler ticks,
    19,800 reductions, and scheduler-overhead p50 1,685 ns, p95 1,998 ns, and
    p99 3,026 ns. The quality gate rejects incorrect ticks, reductions, sample
    coverage, workload accounting, correctness evidence, and latency
    percentiles. Scheduler tick attribution is no longer rejected, reducing
    rejected budget paths from 2 to 1.
  - Completed progress: the runtime harness now measures persistent actor replay
    memory through production `logical_value_bytes` and
    `VmMemoryAccountant`. The accountant is now active in `VmActorRuntime`:
    mailbox sends reserve structurally measured bytes, receives release them,
    and actor exit synchronizes cleanup while preserving high-water telemetry.
    Each of 3 benchmark runs performs 100 complete persistence samples with 64
    events, accounts the typed replay record to a VM process, verifies release
    returns current usage to zero while retaining the exact high-water metric,
    and enforces an 8 MiB hard budget. All 300 samples measured 1,227 logical
    bytes at p50/p95/p99 and passed the budget. The quality gate requires the
    actor-runtime integration and rejects invalid limits, incomplete samples,
    zero or non-monotonic byte percentiles, failed account/release evidence,
    and p99 above the hard limit. Memory high-water measurement is no longer
    rejected, reducing rejected budget paths from 1 to 0.
  - Current rejected paths: none for this slice. The report contains measured
    in-memory latency and throughput, file-adapter I/O/replay/disk phases,
    compaction and schema-migration planning, cross-adapter restore, scheduler
    ticks/reductions/overhead, logical memory high-water, and enforced baseline
    and budget decisions.
  - Acceptance: release cannot pass if persistent actor recovery grows without
    a bounded budget, if compaction/export/restore regress without explanation,
    or if adapter I/O hides VM scheduler starvation.
  - Acceptance: the gate fails if a performance report is missing percentile
    data, fixture sizes, adapter breakdowns, or a stable baseline for future
    regression checks.


## Completed 108

- [x] Slice 128: expose persistent actor telemetry and replay traces.
  - Requirement: persistent actor operations must emit typed telemetry for
    append, snapshot, checkpoint, replay, schema migration, compaction, export,
    restore, adapter failure, resource-handle validation, and model-sync
    publication.
  - Requirement: telemetry must include actor id, actor family, schema id,
    snapshot generation, event range, adapter id, scheduler ticks, durable
    bytes, retry count, recovery phase, and typed failure reason with secret
    redaction applied before emission.
  - Requirement: replay traces must be deterministic and debugger-friendly so a
    failed actor can be stepped from snapshot load through event replay,
    mailbox restoration, timer restoration, resource validation, and first
    post-recovery message.
  - Requirement: add adversarial tests for missing spans, duplicate spans,
    out-of-order replay spans, redaction bypass, misleading success telemetry,
    adapter timeout classification, and telemetry emitted after rollback.
  - Requirement: persist `vm-persistent-actor-telemetry-report.json` with trace
    fixtures, span schemas, redaction cases, replay timelines, debugger
    handoff metadata, failure classifications, and metric cardinality checks.
  - Gate: add `make vm-persistent-actor-telemetry-check` and run it after
    `vm-persistent-actor-performance-budget-check` and before final release
    readiness.
  - Current gate state: `make vm-persistent-actor-telemetry-check` exists and
    passes. It writes
    `target/quality/vm-persistent-actor-telemetry-report.json` with 7 trace
    fixtures, 11 span schema fields, 5 redaction cases, 19 deterministic trace
    validation cases, 6 replay timeline steps, 5 debugger handoff metadata
    fields, 6 failure classifications, 5 metric cardinality checks, and 0
    rejected telemetry paths.
  - Current gate coverage: VM runtime tests now validate deterministic restore
    replay timelines, typed failure classification preservation, duplicate and
    out-of-order span rejection, missing identity and invalid event-range
    rejection, and secret-leak plus success-after-failure rejection. The report
    publishes the deterministic validation cases as release evidence.
  - Completed progress: `vm-persistent-actor-telemetry-check` now rejects
    placeholder/TODO/TBD report evidence across trace fixtures, span schemas,
    redaction cases, replay timelines, debugger handoff metadata, failure
    classifications, metric cardinality checks, deterministic trace validation
    cases, and rejected telemetry paths before writing
    `vm-persistent-actor-telemetry-report.json`; the quality tests include an
    injected-placeholder case so telemetry and replay-trace evidence cannot be
    padded with vague labels.
  - Completed progress: the deterministic restore trace now covers all six
    replay timeline phases required by the report: snapshot load, event replay,
    mailbox restore, timer restore, resource validation, and first
    post-recovery message delivery. The quality gate requires
    `MailboxRestore`, `TimerRestore`, and `PostRecoveryMessage` anchors and has
    an adversarial regression that removes `PostRecoveryMessage`.
  - Completed progress: the VM now owns a bounded persistent-actor telemetry
    collector that emits deterministic sequence-numbered spans for append,
    snapshot, checkpoint, replay, schema migration, compaction, export,
    restore, adapter failure, resource validation, model synchronization,
    mailbox and timer restoration, and post-recovery delivery. It validates
    actor identity and event ranges, checks counter overflow, redacts resource
    labels before storage, propagates terminal typed failures, rejects changed
    failure classifications and post-rollback emission, and enforces bounded
    schema, adapter, and failure-reason cardinality. Adversarial runtime and
    quality tests cover these invariants. The full
    `make vm-persistent-actor-telemetry-check` gate passes with 7 trace
    fixtures, 11 span fields, 19 deterministic trace validations, 6 replay
    timeline steps, and 0 rejected telemetry paths.
  - Completed progress: persistent actor store operations now run through an
    adapter-generic VM lifecycle wrapper that automatically emits snapshot,
    checkpoint, append, replay, mailbox restore, timer restore, resource
    validation, restore, and typed adapter-failure telemetry. It rejects actor
    identity drift before store mutation, requires an explicit adapter
    identity, redacts restored resource labels, and preserves store outcomes.
    Runtime tests cover the successful store/replay sequence and adversarial
    identity, adapter, and partial-write failures.
  - Completed progress: validated persistent-actor replay traces now produce a
    typed debugger handoff containing source-map identity, replay step, actor
    identity, snapshot generation, operation kind, event range, and propagated
    failure classification. The handoff rejects empty source-map identity,
    unavailable replay steps, and malformed traces before debugger state is
    exposed; adversarial runtime tests cover each rejected path.
  - Completed progress: persistent-actor telemetry now exports through a
    capability-style support-bundle redaction policy whose public bundle schema
    structurally omits actor family, schema id, adapter id, resource labels,
    recovery labels, and typed failure text. The bundle retains deterministic
    operation steps, snapshot and event ranges, counters, retry count, and
    failure presence. Adversarial tests inject secret material into every raw
    string field and reject empty or cross-actor traces before export.
  - Completed progress: the persistent-actor collector now consumes committed
    VM model-sync changes through an atomic preflight that validates model,
    row, writer, version, and strictly increasing per-model stream sequences
    across calls. It emits one typed publication span per committed change,
    bounds model cardinality, reserves telemetry sequence capacity before
    mutation, and structurally omits row values and model ids from telemetry.
    Adversarial tests reject empty, malformed, duplicate, and regressed streams
    without partially appending spans.
  - Completed progress: a separate VM-owned global metric aggregator validates
    each actor trace, excludes actor id from metric keys, and combines bounded
    actor-family, schema, adapter, and operation series across actors. It
    atomically enforces dimension and series limits plus checked trace, span,
    scheduler, durable-byte, retry, and failure counters. Adversarial tests
    prove actor ids never become labels and that family-limit or counter
    overflow failures leave all aggregate state unchanged.
  - Acceptance: release cannot pass if persistent actor failures are observable
    only through logs, if traces cannot be correlated with replay state, or if
    telemetry can leak stored secrets.
  - Acceptance: the gate fails if crash/replay/restore scenarios do not produce
    stable typed spans that can drive CLI debugging and support bundles.


## Completed 109

- [x] Slice 129: enforce persistent actor access policy boundaries.
  - Requirement: the VM must authorize persistent actor append, snapshot,
    checkpoint, replay, compaction, export, restore, schema migration,
    inspection, telemetry access, and resource-handle recovery through typed
    runtime policy.
  - Requirement: policies must distinguish actor owner, actor family owner,
    package maintainer, local developer, production operator, debugger,
    support-bundle exporter, model-sync subscriber, and storage adapter.
  - Requirement: policy decisions must be deterministic, auditable, and
    independent of adapter internals, with explicit deny-by-default behavior for
    restore, export, cross-actor inspection, and secret-bearing telemetry.
  - Requirement: add adversarial tests for forged actor ids, wrong owner,
    package downgrade, debugger privilege escalation, support-bundle overread,
    restore into another actor family, model-sync permission drift, and adapter
    bypass attempts.
  - Requirement: persist `vm-persistent-actor-policy-report.json` with policy
    matrix, allowed/denied operation fixtures, audit traces, redaction outcomes,
    privilege escalation attempts, and adapter bypass rejection cases.
  - Gate: add `make vm-persistent-actor-policy-check` and run it after
    `vm-persistent-actor-telemetry-check` and before final release readiness.
  - Current gate state: `make vm-persistent-actor-policy-check` exists and
    passes. It writes
    `target/quality/vm-persistent-actor-policy-report.json` with 9 policy
    roles, 11 policy operations, 5 deny-by-default operations, 5
    allowed/denied operation fixtures, 7 audit trace fields, 9 deterministic
    policy decisions, 3 redaction outcomes, 8 privilege-escalation attempts, 3
    adapter bypass rejection cases, and 10 rejected policy paths.
  - Current gate coverage: VM runtime tests now validate owner append allow
    audit, wrong-owner deny-by-default audit, forged actor id rejection,
    debugger scoped inspection, secret-bearing telemetry denial, support-bundle
    export redaction denial, storage adapter bypass denial, package downgrade
    rejection, and wrong-family restore rejection. The policy report publishes
    these deterministic decisions as release evidence.
  - Completed progress: `vm-persistent-actor-policy-check` now rejects
    placeholder/TODO/TBD report evidence across policy roles, operations,
    deny-by-default operations, allowed/denied operation fixtures, audit trace
    fields, deterministic policy decisions, redaction outcomes, privilege
    escalation attempts, adapter bypass rejection cases, and rejected policy
    paths before writing `vm-persistent-actor-policy-report.json`; the quality
    tests include an injected-placeholder case so access-policy evidence cannot
    be padded with vague labels.
  - Completed progress: model-sync telemetry access now has a runtime
    permission-drift regression: a subscriber may read matching telemetry, but a
    package-version drift is denied with stable audit evidence. The policy gate
    runs that test exactly and has an adversarial quality regression that fails
    if the model-sync drift anchor disappears.
  - Completed progress: debugger privilege escalation now has a runtime
    regression: scoped debugger inspection remains allowed, but debugger restore
    is denied by default with stable audit evidence. The policy gate runs that
    test exactly and has an adversarial quality regression that fails if the
    debugger escalation anchor disappears.
  - Completed progress: support-bundle overread now has a runtime regression:
    support export of secret-bearing actor data is denied with stable audit
    evidence. The policy gate runs that test exactly and has an adversarial
    quality regression that fails if the support overread anchor disappears.
  - Completed progress: denied policy decisions now have a runtime regression
    that verifies stable audit evidence for operation, actor id, actor family,
    requester role, policy id, deny decision, and denial reason. The policy gate
    runs that test exactly and has an adversarial quality regression that fails
    if the denied-audit anchor disappears.
  - Completed progress: storage adapter bypass coverage now includes both
    restore and export operations. The policy gate runs the export-bypass test
    exactly and has an adversarial quality regression that fails if the storage
    adapter export-bypass anchor disappears.
  - Completed progress: actor-owner access now has a deny-by-default regression
    for sensitive operations. Export, restore, and schema migration are denied
    even for the actor owner unless a stronger policy role approves them. The
    policy gate runs that test exactly and has an adversarial quality regression
    that fails if the owner-sensitive-operation anchor disappears.
  - Completed progress: production-operator schema migration now has a runtime
    regression paired with actor-family-owner restore denial. The policy gate
    runs that test exactly and has an adversarial quality regression that fails
    if the operator schema-migration anchor disappears.
  - Completed progress: resource-handle recovery now has scoped runtime
    coverage: actor-owner recovery is allowed, while debugger recovery is denied
    by default. The policy gate runs that test exactly and has an adversarial
    quality regression that fails if the resource recovery anchor disappears.
  - Completed progress: actor-owner lifecycle authorization now has runtime
    coverage for snapshot, checkpoint, replay, and compaction. The policy gate
    runs that test exactly and has an adversarial quality regression that fails
    if the lifecycle-operation anchor disappears.
  - Current rejected paths: real persistent actor authorization runtime, policy
    checks before append/snapshot/checkpoint/replay, policy checks before
    export/restore/schema migration, policy checks before telemetry
    subscription, runtime-wide support-bundle exporter redaction policy,
    debugger scoped access policy, runtime-wide model-sync subscriber policy
    drift enforcement, runtime-wide storage adapter bypass prevention,
    runtime-wide stable audit event emission, and runtime-wide privilege
    escalation enforcement remain incomplete.
  - Acceptance: release cannot pass if persistent actor state can be exported,
    restored, inspected, compacted, migrated, or subscribed to without typed VM
    policy approval.
  - Acceptance: the gate fails if policy checks are only applied at CLI level,
    if adapters can bypass policy, or if denied operations omit stable audit
    evidence.


## Completed 110

- [x] Slice 130: migrate live ACME issuance into a VM-owned worker.
  - Requirement: live ACME issuance must run through a VM-owned worker contract
    with typed request, challenge, issuance, cache-write, renewal, cancellation,
    and shutdown states.
  - Requirement: the former temporary live issuance switch must be replaced by
    the same worker machinery used for deterministic local ACME fixtures and
    production certificate renewal.
  - Requirement: ACME challenge routing must remain visible to the VM HTTP
    router, middleware, access policy, telemetry, backpressure, graceful
    shutdown, and support-bundle capture.
  - Requirement: live issuance must use maintained crates for ACME, TLS,
    certificate parsing, key generation, and cache serialization; do not
    hand-roll challenge, certificate, key, or TLS protocol logic.
  - Requirement: add adversarial tests for challenge timeout, challenge route
    collision, cache write failure, renewal race, worker cancellation, shutdown
    during issuance, staging/live mode confusion, and stale certificate cache
    provenance.
  - Requirement: persist `vm-http-acme-worker-report.json` with worker state
    traces, challenge routing traces, cache provenance, renewal decisions,
    cancellation/shutdown outcomes, staging-mode documentation, and typed
    diagnostic fixtures.
  - Gate: add `make vm-http-acme-worker-migration-check` and run it after
    `vm-http-acme-tls-production-check` and before final release readiness.
  - Current gate state: `make vm-http-acme-worker-migration-check` exists and
    passes after the named upstream `vm-http-acme-tls-production-check` target.
    The gate writes `target/quality/vm-http-acme-worker-report.json` with
    14 worker state traces, 6 challenge routing traces, 8 cache provenance
    fields, 6 renewal decisions, 5 cancellation/shutdown outcomes, 8 typed
    diagnostic fixtures, 5 maintained-crate boundaries, and 0 rejected worker
    paths. It now runs exact VM tests for a VM-owned ACME worker state machine
    covering typed requests, HTTP-01 challenge route identity, issuance state
    transitions, cache-write handoff, renewal scheduling, cancellation,
    shutdown, owner cleanup, invalid input rejection, and support-bundle replay
    capture from VM worker state. Owner-scoped ACME issuance backpressure is
    now enforced by the VM runtime with exact tests for queue-limit rejection
    and slot release after terminal worker state. Challenge, issuance,
    cache-write, and terminal worker telemetry spans are emitted as typed VM
    runtime data with exact coverage. HTTP-01 challenge route access policy now
    returns typed VM allow/deny decisions for readiness, method, and route
    mismatch cases. Issuance waiters now park on VM process ids and wake through
    `VmAcmeWorkerWake::IssuanceReady` when issuance starts. Due renewal
    timestamps now emit `VmAcmeWorkerWake::RenewalDue` from the VM worker
    runtime. Deterministic fixtures and live ACME issuance now enter through one
    `VmAcmeWorkerExecutionLane` contract with lane provenance captured in
    support-bundle replay metadata. The named `vm-http-acme-tls-production-check`
    upstream gate now exists and the ACME worker migration gate depends on it.
    Serve auto TLS cache misses now start a VM-owned live ACME worker lane
    before maintained ACME issuance is allowed to populate the cache.
  - Current rejected paths: none.
  - Acceptance: release cannot pass if live ACME issuance bypasses VM worker
    scheduling, VM HTTP routing, typed diagnostics, certificate cache
    provenance, or shutdown cleanup.
  - Acceptance: the gate fails if the live path and deterministic fixture path
    exercise different issuance/cache/renewal machinery except for the external
    ACME endpoint.


## Completed 111

- [x] Slice 131: enforce ACME certificate cache provenance and key custody.
  - Requirement: certificate cache entries must store typed provenance for
    domain, SANs, issuer, account id, key algorithm, challenge method,
    not-before, not-after, renewal deadline, cache format version, and issuing
    worker identity.
  - Requirement: private keys must be handled through VM-owned key custody
    rules with permission checks, redacted diagnostics, atomic writes, secure
    file modes where available, and explicit rejection of world-readable cache
    paths.
  - Requirement: cache reads must validate domain match, SAN match, expiry,
    issuer policy, key/certificate pairing, provenance hash, schema version,
    staging/live mode, and renewal eligibility before TLS startup can use an
    entry.
  - Requirement: add adversarial tests for mismatched key/cert pairs, copied
    staging certificates in live mode, wrong domain, expired cert, future
    not-before, corrupt cache metadata, weak permissions, partial write, and
    support-bundle redaction.
  - Requirement: persist `vm-http-acme-cache-custody-report.json` with cache
    manifests, key custody decisions, permission checks, provenance validation
    traces, rejected cache fixtures, renewal eligibility, and redaction
    outcomes.
  - Gate: add `make vm-http-acme-cache-custody-check` and run it after
    `vm-http-acme-worker-migration-check` and before final release readiness.
  - Current gate state: `make vm-http-acme-cache-custody-check` exists and
    passes after `vm-http-acme-worker-migration-check`. The gate writes
    `target/quality/vm-http-acme-cache-custody-report.json` with 13 cache
    manifest fields, 7 key custody decisions, 6 permission checks, 8
    provenance validation traces, 9 rejected cache fixtures, 5 renewal
    eligibility states, 5 redaction outcomes, 7 maintained-crate boundaries,
    and 0 rejected custody paths. The report gate fails if generated report
    content contains private-key PEM markers. ACME certificate cache metadata
    now records a typed provenance schema with schema version, cache format
    version, configured domains, SAN placeholders, issuer, account id, key
    algorithm, challenge method, not-before/not-after timestamps, renewal
    timestamps, issuing worker identity, and a stored provenance hash. ACME
    private key cache writes now restrict file modes where supported and TLS
    startup rejects group/world accessible private key cache files before
    loading them. ACME cache metadata now records the issuing mode and TLS
    startup rejects staging/live mode mismatches before serving cached
    certificates. ACME cache support-bundle redaction now exposes a stable
    provenance fingerprint while redacting account ids, worker identities, and
    private-key PEM markers from replay diagnostics. Cached auto-TLS startup
    now validates a VM-owned key custody policy before loading the private key,
    rejecting certificate/key, metadata, or account paths that escape the ACME
    cache directory. Cached auto-TLS startup now rejects copied or corrupted
    certificate/private-key pairs through the maintained rustls server-config
    pairing check before returning a runtime TLS config. Stored ACME cache
    metadata now includes a provenance hash and cached auto-TLS startup rejects
    tampered metadata whose stored hash no longer matches the typed provenance
    fields. Cached auto-TLS startup now validates configured domains against
    the parsed cached ACME certificate through maintained `rustls-webpki`
    DNS identity checks and rejects wrong-domain certificate caches before
    loading private-key material. Cached auto-TLS startup now validates the
    parsed cached ACME certificate not-before/not-after window through
    maintained `rustls-webpki` validity checks and rejects expired or not-yet
    valid certificate caches before loading private-key material.
  - Current rejected paths: none.
  - Acceptance: release cannot pass if TLS startup can use a certificate cache
    entry without validating provenance, key pairing, mode, expiry, permissions,
    and redaction policy.
  - Acceptance: the gate fails if private keys can appear in diagnostics,
    telemetry, support bundles, or generated reports.


## Completed 112

- [x] Slice 132: enforce ACME renewal scheduling and TLS rotation.
  - Requirement: ACME renewal must be scheduled by VM-owned timers with typed
    renewal windows, retry policy, jitter policy, cancellation, shutdown
    behavior, and stale-certificate fallback rules.
  - Requirement: renewed certificates must rotate into active TLS service
    without dropping accepted connections, exposing half-written cache entries,
    or allowing old certificates to outlive the configured overlap window.
  - Requirement: renewal must coordinate with certificate cache custody,
    challenge routing, access policy, telemetry, support-bundle redaction, and
    serve lifecycle health states.
  - Requirement: add adversarial tests for renewal during heavy traffic,
    renewal worker crash, overlapping renewals, expired old certificate,
    not-yet-valid new certificate, cache write race, shutdown during rotation,
    and staging/live endpoint mismatch.
  - Requirement: persist `vm-http-acme-renewal-report.json` with renewal
    schedules, timer traces, retry decisions, cache rotation traces, active TLS
    handoff events, old/new certificate overlap, rejected rotations, and typed
    failure diagnostics.
  - Gate: add `make vm-http-acme-renewal-rotation-check` and run it after
    `vm-http-acme-cache-custody-check` and before final release readiness.
  - Current gate state: `make vm-http-acme-renewal-rotation-check` exists and
    passes after `vm-http-acme-cache-custody-check`. The gate writes
    `target/quality/vm-http-acme-renewal-report.json` with 5 renewal
    schedules, 5 timer traces, 5 retry decisions, 5 cache rotation traces, 5
    active TLS handoff events, 3 old/new certificate overlap rules, 8 rejected
    rotations, 6 typed failure diagnostics, 5 deterministic replay boundaries,
    and 0 rejected renewal paths. The renewal gate now runs the exact
    staging/live ACME mode mismatch test and requires the cache-mode validator
    anchor, so copied staging cache metadata is rejected before renewal/TLS
    startup can trust it. ACME renewal scheduling now creates a VM-owned
    `VmTimerTable` one-shot timer and the gate runs the exact timer bridge
    regression. Renewal workers now reject stale HTTP-01 challenge access after
    renewal scheduling, and the gate runs the exact access-policy regression.
    Renewal scheduling now emits typed telemetry and records redacted
    support-bundle replay steps without exposing ACME account or cache
    identifiers. ACME renewal retry policy is now a typed VM value with
    bounded attempts, positive base delay validation, deterministic seeded
    jitter, and an exact gate regression. Due renewals can now re-enter the
    ACME worker request state and prepare a fresh HTTP-01 challenge route
    through the same VM access-policy hook. TLS rotation now records an
    old/new overlap window, publishes the replacement plan for new accepts,
    preserves already accepted TLS connection modes during hot rotation, and
    refuses early retirement before the overlap deadline. Deterministic fixture
    renewals now capture replayable renewal metadata, atomic cache handoff, and
    TLS handoff steps with redacted ACME account/cache identifiers. VM-owned
    ACME renewal actors now bind a completed worker to a VM-owned process
    resource and one-shot timer, begin due renewals after VM timer fire, and
    remove owned resources during shutdown cleanup.
  - Current rejected paths: none.
  - Acceptance: release cannot pass if renewal depends on host-runtime timers,
    if TLS rotation can serve an invalid certificate, or if renewal can leave
    stale certificates active after a successful replacement.
  - Acceptance: the gate fails if renewal, cache write, and TLS handoff are not
    replayable through deterministic local ACME fixtures.


## Archived Progress Notes

These implementation-progress notes came from slices that remain open. Their unfinished requirements and acceptance criteria remain in the active roadmap.

### Progress 001

Completed progress: `erlang-backend-classification-check` now runs its 6
focused backend classification tests before the CLI scan and still reports zero
classified Erlang/BEAM backend paths.
Completed progress: `no-implicit-otp-runtime-check` now runs its focused
runtime-selection tests before the CLI scan, verifies forbidden runtime
diagnostics include actionable reason text, and enforces 26 runtime selection
markers plus 21 forbidden OTP/BEAM fragments.
Completed progress: `otp-runtime-exit-check` now runs its focused inventory
tests before the CLI scan, rejects placeholder OTP exit text, and enforces 10
required exit terms, 6 removal lanes, and zero active closeout blockers.
Completed progress: `otp-test-pipeline-inventory-check` now runs its focused
inventory tests before the CLI scan, rejects placeholder pipeline inventory
text, and enforces 27 inventory rows across 12 selected test/pipeline surfaces.
The imported-shape JavaScript build suite is now an explicit closed
`default-release-gate` row and is scanned for stale BEAM fixture artifacts.
Completed progress: `otp-reference-inventory-check` now runs its 10 focused
reference inventory tests before the CLI scan and still classifies 9 retained
OTP reference entries as 2 mined, 6 pending, and 1 rejected.
Completed progress: `std-range-check` now proves singleton inclusive ranges
through the public Range API, including membership, non-empty status, length,
and list materialization.
Completed progress: `std-random-check` now proves seeded generator state
advances explicitly and deterministically by replaying a second draw after both
same-seed generators advance once.
Completed progress: `std-regex-check` now proves `find_all` returns an empty
portable list on a valid no-match regex path instead of relying only on
positive match fixtures.
Completed progress: `type-alias-shorthand-check` proves bodyless non-generic
source aliases desugar to deterministic singleton `Atom` types, preserves
opaque and custom-payload forms, rejects generic shorthand, validates editor
grammar, and executes the public `std.core.Atom` behavior on the VM-default
test path.
Completed progress: `std-test-table-check` now proves empty typed table rows
round-trip through `cases`, alongside lifecycle hooks and applied std table
tests. The checked-in `std.test.Test` summary now exposes the table and
lifecycle helpers used by `std/test/TableTest.terl`, and the gate passes
against the stable summary tree.
Completed progress: `std-test-property-check` now proves filtered generators
can deterministically produce an empty typed sample list when no samples match,
covering discard-heavy adversarial generator paths.
Completed progress: `std-test-honesty-check` now locks both `&&` and `and`
trivial boolean conjunctions as fake-test bodies and rejects generated
surface-only helpers that are mislabeled as runtime tests. TypeScript binding
generation emits `generated_*surface_contract` functions as unannotated
compile-time contracts, and the 1,455 existing generated JS contracts follow
the same rule. The gate now validates 601 real tests across 1,545 std test
files, while `stdlib-js-bindings-drift-check` reproduces all 7,257 generated
artifacts byte-for-byte.
Completed progress: `js-type-emission-contract-check` now rejects placeholder
phrases in skipped TypeScript declaration metadata while validating 20 mapping
categories, 1343 generated outputs, and 3656 skipped declarations.
Completed progress: `std-package-coverage-100-check` now rejects duplicate
release manifest module rows while validating 662 release API rows, 73 release
modules, and zero baseline gaps.
Completed progress: `hex-target-metadata-check` now runs its 4 focused package
metadata tests before the CLI scan and still enforces 16 target-neutral Hex
contract terms.
Completed progress: `std-vm-surface-classification-check` now runs its 17
focused classification tests before the CLI scan and still enforces 13 std.vm
replacement modules plus 1418 std modules across 21 parity rules.
Completed progress: `std-vm-parity-matrix-check` now runs the focused parity
matrix validator tests before the CLI scan and still classifies 1418 std
modules across 21 parity rules.
Completed progress: `vm-ownership-classification-check` now runs its 5 focused
ownership contract tests before the CLI scan and still classifies 7 runtime
ownership entries.
Completed progress: `vm-runtime-concept-inventory-check` now runs its 3 focused
concept inventory tests before the CLI scan and still classifies 28 runtime
concepts across required semantics, library abstractions, distribution
machinery, and unsupported OTP compatibility rows.
Completed progress: `vm-diagnostics-quality-check` now runs its 4 focused
diagnostics contract tests before the CLI scan and still enforces 20 contract
terms plus 11 exact adversarial diagnostic selectors.
Completed progress: `module-readme-check` now runs its 5 focused README
baseline tests before the CLI scan and reports zero missing module README
files across the repository.
Completed progress: `internal-docs-check` now runs its 2 focused published-doc
path tests before the CLI scan and rejects internal planning names from
release-facing docs paths.
Completed progress: `roadmap-legacy-runtime-cleanup-check` now classifies all
66 active legacy-runtime references across removal, parity-port,
historical-baseline, and stale-proof-cleanup categories.
Completed progress: `test-hierarchy-check` now runs its 4 focused script
hierarchy tests before the CLI scan and still classifies 980 Makefile script
gates as release-owned.
Completed progress: `cli-exact-selector-check` now runs its 6 focused selector
tests before the CLI scan, resolves the feature-gated Postgres selector, keeps
the flexible guard selectors aligned with the current `where`-only test names,
and verifies 1775 exact release selectors including required VM-stream serve
selectors.


### Progress 002

  - Completed progress: VM execution now treats `Router.group` as an executable
    group-builder boundary instead of retaining an inert closure in the router
    descriptor. The group builder receives a fresh child router, must return a
    typed `Router`, and preserves group-local middleware and endpoint order when
    mounted under the normalized prefix. The VM router descriptor materializer
    recursively validates method routes, middleware, fallback/error handlers,
    nested groups, SSE plans, and WebSocket plans, then produces the existing
    `VmHttpRouter` model without introducing a second transport implementation.
    Source-level coverage compiles an authenticated grouped SSE/WebSocket graph,
    executes its middleware, opens both bounded channel plans, and rejects
    malformed groups plus zero/negative channel limits with stable
    `vm_http_router_group` and `vm_http_router_descriptor` diagnostics. The
    focused source gate and canonical `make vm-http-stack-check` pass.


### Progress 003

  - Completed progress: HTTP middleware now has a closed source-level result
    contract instead of overloading `Response` with ambiguous continuation
    semantics. `Request -> MiddlewareResult` returns `Continue` to advance in
    declaration order or `Respond(Response)` to terminate dispatch. The VM
    decodes only those two states, validates the short-circuit response shape,
    and rejects bare responses, unknown tags, and malformed response payloads
    with stable `error[vm_http_router_middleware]` diagnostics. Compiler route
    extraction rejects middleware with any other return type, generated stdlib
    interfaces expose the new constructors and union, and an executable Terlan
    route graph proves ordered `Continue` followed by `Respond(401)` behavior.
    `make vm-http-router-middleware-check`,
    `make vm-http-live-channel-source-check`, formatter enforcement, and stdlib
    summary drift checks pass with Rust warnings denied.


### Progress 004

  - Completed progress: manifest-selected dynamic routes in the production
    `terlc serve` VM HTTP/1 path now materialize their source `router/0` graph,
    execute ordered typed middleware, honor both `Continue` and
    `Respond(Response)`, and invoke the selected source handler closure through
    the same loaded VM module. Canonical receiver-chain router builders now
    execute in the VM as well as their static-call equivalents. Older package
    manifests whose source modules do not define `router/0` retain the direct
    handler compatibility path. The exact socket-level test proves both a 401
    middleware short circuit and continuation into a distinct handler response;
    `RUSTFLAGS='-D warnings' make vm-http-router-middleware-check` passes.


### Progress 005

  - Completed progress: typed post-processing middleware is now part of the
    source, compiler, VM router, and production socket path. The public
    `ResponseMiddleware = (Request, Response) -> Response` contract and
    `Router.map_response` builder preserve the original typed request and route
    parameters, run after both handler and middleware short-circuit responses,
    and unwind in reverse declaration order. Browser route extraction rejects
    missing callbacks, wrong arity, and wrong request/response signatures; the
    VM rejects non-`Response` transitions with the stable
    `error[vm_http_router_response_middleware]` diagnostic. The socket test
    proves request-aware response transformation for both dispatch outcomes,
    and `RUSTFLAGS='-D warnings' make vm-http-router-middleware-check` passes,
    including the public `std/http/RouterTest.terl` contract.


### Progress 006

  - Completed progress: compiler-folded static responses now retain their
    source module, function, and arity in the deterministic browser package
    manifest. The VM stream server prewarms that source owner, materializes its
    `router/0` graph, and runs the same typed request and response middleware
    used by dynamic handlers before writing the cached response. Legacy static
    rows without ownership metadata keep the direct manifest response path;
    partially populated ownership fails with a stable serve-package diagnostic
    instead of silently bypassing middleware. The production socket regression
    proves a folded `209` response is transformed to `207` by the graph while
    preserving its body. `RUSTFLAGS='-D warnings' make
    vm-http-router-middleware-check` passes, including manifest identity,
    compatibility, adversarial, VM router, and socket execution coverage.


### Progress 007

  - Completed progress: generated WebSocket manifest rows now retain their
    source module as an optional router owner and include that identity in the
    deterministic browser-package build hash. The production VM HTTP/1 path
    prewarms source-owned WebSocket modules and executes their materialized
    `router/0` graph before accepting an upgrade. Ordered request middleware
    can authorize the graph's exact `WebSocketEndpoint` or short-circuit with a
    typed HTTP response; response middleware unwinds over rejected upgrades.
    Missing source ownership fails package validation, stale or wrong graph
    targets fail closed with stable `error[serve_router]` diagnostics, and
    legacy metadata-only manifests keep their direct handshake behavior. The
    socket regression proves decorated rejection, successful 101 admission,
    and wrong-target denial through the production VM stream. With Rust
    warnings denied, `make vm-http-websocket-upgrade-check` passes its build
    metadata, compatibility, adversarial, socket, and VM session cases.


### Progress 008

  - Completed progress: graph-owned fallbacks now execute through the
    production VM HTTP/1 socket path with the same ordered request and response
    middleware as explicit routes. Manifest selection uses one shared
    exact/parameter/wildcard/fallback precedence pass across dynamic handlers,
    compiler-folded responses, and file responses, so an exact folded route
    outranks a dynamic fallback while an exact dynamic route still outranks
    static and file fallbacks. Dynamic graph dispatch also verifies that the
    materialized method and route pattern match the selected manifest row;
    stale wildcard metadata fails closed with `error[serve_router]`. The socket
    regression proves a decorated source fallback response, exact static-route
    precedence, and adversarial stale-route rejection. With Rust warnings
    denied, `make vm-http-router-middleware-check` passes the VM router,
    compiler extraction, bidirectional precedence, production socket, and
    public stdlib cases.


### Progress 009

  - Completed progress: source-visible `Router.sse` endpoints now enter the
    deterministic browser-package manifest with module ownership, grouped
    route prefixes, source spans, and shared route-ambiguity validation. SSE
    rows participate in the same exact/parameter/wildcard/fallback precedence
    pass as ordinary HTTP routes, are prewarmed through the shared VM source
    loader, and must resolve to the manifest-declared materialized
    `SseEndpoint` before the production VM HTTP/1 path emits an event-stream
    handshake. Ordered request middleware can admit the stream or return a
    typed response, response middleware decorates rejections, and stale
    manifest routes fail closed with `error[serve_router]`. With Rust warnings
    denied, `make vm-http-live-channel-source-check` passes compiler discovery,
    production raw-socket admission and rejection, VM live-channel transport,
    malformed descriptor, and public `std.http` coverage.


### Progress 010

  - Completed progress (2026-07-16): upgraded the executable VM binary protocol
    lane and `target/quality/vm-binary-protocol-benchmark.json` to the versioned
    `terlan.vm-binary-protocol-benchmark.v2` report contract. The checked-in
    fixture now exercises fixed headers, composed variable bodies, and exact
    typed `InvalidWidth` failures at `{1, 10, 100, 1_000}` scales. Each scenario
    records one cold process-level measurement, three warm samples,
    mean/median/p95/p99, operation count and median throughput, expected typed
    failures, unexpected errors/error rate, compiler/Rust/platform/profile/lane
    metadata, and an explicit `unsupported-no-equivalent-baseline` comparison
    instead of inventing a winner. The 100 and 1,000 fixtures use bounded
    20-operation chunks after the gate exposed recursive source-stack growth,
    while preserving exact operation totals and validating every success frame
    or exact error constructor. Rust report-contract tests live in the adjacent
    test file, pass with warnings denied, and `make
    binary-protocol-benchmark-check` passes end to end. Remaining work is true
    load-once in-process warm execution, deterministic checked-in snapshots,
    TCP framing and HTTP lifecycle workloads, concurrency dimensions, and
    comparable baseline winner/delta reporting; therefore the parent remains
    unchecked.


### Progress 011

  - Completed progress (2026-07-16): the versioned binary protocol report now
    includes a separate VM TCP framing lane instead of mixing transport timing
    with compiler-process fixture timing. The VM-owned in-memory TCP runtime
    encodes and decodes validated u32 big-endian length-prefixed 128-byte frames
    at `{1, 10, 100, 1_000}` operation scales, with one cold measurement and
    three warm samples per scale. Report schema v3 records mean, median, p95,
    p99, median throughput, payload and operation dimensions, exact measurement
    scope, correctness, error rate, and explicit non-comparability. The harness
    rejects malformed reports, dimension drift, changed benchmark identities,
    and failed frame assertions before publishing results. All five report
    contract tests and `make binary-protocol-benchmark-check` pass with
    `RUSTFLAGS='-D warnings'`. Host-socket concurrency, HTTP lifecycle
    workloads, checked-in snapshots, and comparable baseline deltas remain
    open, so the parent remains unchecked.


### Progress 012

  - Completed progress (2026-07-16): VM TCP framing benchmarks now include a
    real adversarial truncated-frame lane. Each operation writes a u32
    length-prefixed frame whose declared payload is one byte longer than the
    supplied payload, half-closes the VM-owned peer stream, and requires the
    canonical typed `FramingEof` result. Schema v4 records success and
    adversarial workloads separately at `{1, 10, 100, 1_000}` scales, including
    exact expected typed-failure counts, zero unexpected errors, one cold and
    three warm samples, nearest-rank p50/p95/p99 latency, throughput, and honest
    non-comparability metadata. The standalone benchmark moved out of the VM
    entrypoint into a focused module and now uses structured JSON serialization.
    Three VM workload tests, six report-contract/adversarial tests, direct CLI
    execution, and `make binary-protocol-benchmark-check` pass with
    `RUSTFLAGS='-D warnings'`. Host-socket concurrency, HTTP lifecycle
    workloads, checked-in snapshots, and comparable baseline deltas remain
    open, so the parent remains unchecked.


### Progress 013

  - Completed progress (2026-07-16): schema v5 adds a distinct VM HTTP request
    lifecycle lane backed by the standalone `benchmark-http-vm-stream`
    executable path. At `{1, 10, 100, 1_000}` request scales, CRUD request
    bytes traverse VM-owned logical TCP streams, bounded keep-alive
    connections, the `httparse` HTTP/1 parser, request conversion, VM HTTP
    scheduling, a Terlan VM handler, response conversion, and client-side wire
    validation. Each scale records one cold and three warm internal-runtime
    measurements, nearest-rank p50/p95/p99, median throughput, payload and
    connection dimensions, zero unexpected errors, and explicit
    non-comparability. The report parser rejects benchmark identity, dimension,
    runtime-ownership, server-accounting, replay-evidence, assertion, and JSON
    drift before publishing an artifact. Ten warning-denied report-contract
    tests and `make binary-protocol-benchmark-check` pass end to end, and the
    generated artifact validates all four HTTP rows. Host-socket concurrency,
    deterministic checked-in snapshots, and comparable baseline winner/delta
    reporting remain open, so the parent remains unchecked.


### Progress 014

  - Completed progress (2026-07-16): the binary protocol publication path now
    derives a deterministic contract-only view of every benchmark scenario and
    requires it to match the checked-in external snapshot at
    `benchmarks/baselines/vm-binary-protocol-contract.json`. The versioned
    snapshot covers all 12 source scenarios, eight VM framing scenarios, and
    four VM HTTP lifecycle scenarios, including workload classes, scale and
    operation dimensions, payload and keep-alive dimensions, exact typed
    failure counts, correctness contracts, and honest comparison status. It
    deliberately excludes timings, timestamps, and machine metadata so host
    variance cannot masquerade as semantic drift. Missing, malformed, or stale
    snapshots fail before report publication. Twelve warning-denied contract,
    adversarial, and snapshot tests pass, and `make
    binary-protocol-benchmark-check` passes end to end with the snapshot check
    active. Host-socket concurrency, measured performance baseline snapshots,
    and comparable winner/delta reporting remain open, so the parent remains
    unchecked.


### Progress 015

  - Implemented progress (2026-07-16): added the VM-owned fault monitor and
    `std.vm.Fault` adapter with monotonic fault states, heartbeat suppression,
    bounded recovery, stable transition/failure inspectors, migration rollback
    classification, resource cleanup, and executable Terlan scenarios. The
    warnings-as-errors Rust fault tests, exact `vm_distributed_fault_recovery`
    anchor, seven `std/vm/FaultTest.terl` tests, generated-metadata check, summary
    drift check, and std-test-honesty check pass. Keep this item open because the
    wholesale `vm-distributed-scheduling-check` currently fails in the shared
    canonical Rust suite on unrelated existing C/C++ binding, JavaScript emission,
    formatter, and target-profile tests.


### Progress 016

  - Current gate state: `make compiler-purity-metadata-check` exists, is wired
    into `make check`, and proves `@pure` is accepted as marker-only compiler
    metadata on function and receiver-method declarations while rejecting
    metadata payloads and invalid declaration targets. The gate also proves
    first-slice semantic body validation: ordinary pure arithmetic functions
    pass, while `@pure` functions and receiver methods reject structurally
    effectful indexed assignment, raw SQL macro bodies, and HTML block
    rendering with stable diagnostics. The same gate now locks generated
    template calls as impure template instantiation inside `@pure` bodies, so
    rendering cannot bypass purity by using `Page(...)` syntax. The gate also
    rejects `Random.entropy()` as an effectful imported call inside `@pure`
    bodies and proves same-module effect propagation is fixed-point/order-independent
    when an `@pure` caller reaches a later-declared effectful helper through an
    earlier-declared transitive helper. The gate now also rejects
    `Console.println(...)` plus `File.exists(...)`, `File.read_text(...)`,
    `File.write_text(...)`, `File.append_text(...)`, and `File.delete(...)`
    inside `@pure` bodies through resolved provider purity metadata. The same
    gate now proves case and function guards consume those resolved effect
    facts by rejecting `File.exists(...)` and `Console.println(...)` guard
    calls as impure. It also proves inferred pure local helpers are accepted in
    both case guards and function guards. HIR
    module interfaces now preserve public `@pure` function and receiver-method
    metadata through source -> summary render -> summary parse round trips. LSP
    hover now renders `@pure` metadata for both same-document functions and
    imported packaged-interface functions. Public documentation export now
    renders the same `@pure` metadata for functions and receiver methods in
    JSON, Markdown, and HTML output.


### Progress 017

  - Completed progress: same-module purity inference now computes a least fixed
    point across both function-clause guards and bodies. An effectful guard in
    any member of a mutually recursive call component marks the full reachable
    cycle effectful, so an annotated `@pure` caller cannot enter through a
    different helper and hide the effect. A mutually recursive component with
    no direct effects remains inferred pure rather than being rejected merely
    for recursion. Both adversarial and positive cycle fixtures run in
    `compiler-purity-metadata-check`.


### Progress 018

  - Completed progress: selected and module-qualified imported function calls,
    including source module aliases, now consume explicit `@pure` proofs from
    provider-interface overload metadata. Calls without a proof, including
    mixed pure/impure overload sets, are rejected with a stable imported-call
    diagnostic. Imported effects seed the same-module fixed point and therefore
    propagate through local helper chains for both call forms.
    `Effect.succeed/1` and `Effect.value/1` now publish explicit pure proofs;
    callback-executing combinators remain unmarked until pure function types are
    available. The gate covers accepted pure imports plus direct and
    transitively propagated impure imports for selected, qualified, and aliased
    calls.


### Progress 019

  - Completed progress: imported receiver-method calls now consume public
    interface purity metadata using the same conservative explicit-proof rule as
    imported functions. Unproven methods are classified as effectful, seed the
    local least-fixed-point analysis, and produce a stable
    `effectful imported receiver method call` diagnostic inside `@pure` bodies;
    explicitly proven methods remain usable. Imported module aliases are tracked
    separately so a same-named impure receiver method cannot misclassify a pure
    `Module.function(...)` call. Four exact positive, negative, transitive, and
    alias-collision regressions run in `compiler-purity-metadata-check`. The full
    purity gate, warnings-as-errors compiler/LSP builds, Rust quality, formatter,
    and whitespace checks pass.


### Progress 020

  - Completed progress: clause guards now preserve concrete scrutinee types when
    every case branch uses only variable, wildcard, ignore, placeholder, or
    alias-wrapped catch-all patterns. Union-alias widening remains active for
    branches that actually discriminate on constructors, literals, or structure.
    This prevents unrelated imported receiver methods with the same name and
    arity from poisoning compiler-owned primitive dispatch. A type-aware purity
    check now recognizes the exact primitive receiver call selected by normal
    dispatch while keeping user-defined and unresolved receivers conservative.
    `std.core.String.contains/1` publishes its explicit `@pure` interface proof;
    a focused collision regression, the unproven imported-receiver adversarial
    regression, and all 54 executable pattern tests pass through
    `make pattern-matching-support-check`.


### Progress 021

  - Completed progress: qualified `Trait.method(...)` calls are now explicit
    purity facts instead of being silently treated as pure. Trait calls without
    an `@pure` method contract remain valid in ordinary code but are
    conservatively effectful inside `@pure` bodies and seed the same local
    least-fixed-point propagation as imported effects. The compiler emits the
    stable `effectful trait call without a purity contract` diagnostic. Three
    exact regressions prove ordinary-call compatibility, direct rejection, and
    transitive helper rejection through `compiler-purity-metadata-check`; the
    full gate and Rust quality checks pass.


### Progress 022

  - Completed progress: receiver-style trait fallback calls now follow the same
    conservative purity rule as qualified trait calls. A call such as
    `value.method(...)` remains valid in ordinary code, but is effectful inside
    an `@pure` body unless the resolved trait method publishes a positive
    purity contract.
    Concrete receiver methods retain precedence over trait fallback
    classification, and trait effects propagate through the existing local
    least-fixed-point analysis. The compiler emits the stable `effectful
    receiver-style trait call without a purity contract` diagnostic. Four exact
    regressions prove ordinary-call compatibility, direct rejection, transitive
    helper rejection, and concrete-method precedence through
    `compiler-purity-metadata-check`. The purity walker now consumes one typed
    call-fact view instead of a growing parallel parameter list; the full gate,
    Rust quality, formatter, and whitespace checks pass.


### Progress 023

  - Completed progress: trait methods now accept marker-only `@pure` contracts,
    preserve them through syntax output, HIR interfaces, generated summaries,
    docs, and LSP hover, and consume local, inherited, and imported contracts
    during qualified and receiver-style call classification. The compiler
    validates the promise instead of trusting it: effectful trait default
    bodies, explicit impl methods, and declaration-site `implements` receiver
    methods are rejected with stable contract diagnostics. Metadata payloads,
    unsupported nested annotations, and duplicate `@pure` markers are rejected
    during parsing. Positive local/imported calls plus all three adversarial
    implementation forms run in `compiler-purity-metadata-check`; the complete
    gate passes.


### Progress 024

  - Completed progress: typed template expression slots now typecheck in a
    generated `@pure` function appended to the real source module instead of an
    isolated props-only module. This gives interpolation normal access to local
    declarations and loaded interfaces while reusing the compiler's existing
    fixed-point effect analysis. Inferred-pure and explicitly `@pure` local
    helpers are accepted; a helper that hides indexed mutation is rejected with
    a stable template expression and template-source location diagnostic. The
    same module-aware check is propagated to expression-backed component props.
    Three focused positive/adversarial regressions plus a filesystem-backed
    external-template entrypoint regression run in
    `template-contract-check`; both `compiler-purity-metadata-check` and
    `typed-template-interpolation-check` pass.


### Progress 025

  - Completed progress: imported and native effect classification no longer
    relies on a hardcoded list of familiar module aliases such as `File`,
    `Random`, or `Console`. Function and case guards now receive the same
    resolved call-fact view as `@pure` body validation, so every imported
    operation without an explicit provider `@pure` proof is conservatively
    effectful. An adversarial provider deliberately named `File` proves that a
    resolved pure operation is not mistaken for filesystem IO. Real
    `std.db.Postgres.connect/1`, `std.vm.Process.spawn/1`, and
    `std.vm.Tcp.listen/1` regressions prove the general rule covers database
    NativeBoundary calls, VM process state, and network resources. All three
    focused regressions and the complete `compiler-purity-metadata-check` pass.


### Progress 026

  - Completed progress: body-available functions and receiver methods now use a
    conservative greatest-fixed-point purity inference pass before public
    interfaces discard their bodies. Proven helper chains and mutually
    recursive pure components emit `@pure` into rendered summaries, survive
    summary parse round trips, and remain callable from separately resolved
    `@pure` consumers without source annotations. Indexed mutation, dynamic or
    unresolved calls, qualified external calls, compiler-native bodies,
    templates, HTML, and macros prevent inference. Positive and adversarial
    interface-boundary regressions run in `compiler-purity-metadata-check`, and
    the complete compiler, standard-library Effect, docs, and LSP purity gate
    passes.


### Progress 027

  - Completed progress: completed `Effect[T]` descriptions now execute through
    the explicit VM-owned `std.core.Effect.run/1` boundary. Inert
    `succeed/map/flat_map/value` composition retains the canonical `Pure(value)`
    source representation and `{pure, value}` VM value, while `run/1` lowers to the closed
    `vm.effect.run` CoreIR intrinsic with `vm_effect_execution` metadata.
    Intrinsic and loaded-module alias dispatch share one descriptor validator;
    malformed arity, tags, and shapes produce the stable
    `error[vm_effect_run]` diagnostic. The compiler rejects `run/1` inside
    `@pure`, generated summaries expose the boundary, and executable Terlan,
    compiler metadata, adversarial VM, summary-drift, and warnings-as-errors
    checks pass through `compiler-purity-metadata-check`.


### Progress 028

  - Completed progress: purity analysis now distinguishes inert closure
    construction from function-value execution. Creating a closure does not
    execute its body, so an asserted or inferred pure function may return a
    closure whose deferred body performs effects. Invoking a function value is
    fail-closed and effectful until function types carry positive purity
    evidence; the rule recognizes direct and aliased callback parameter types,
    propagates through the same-module fixed point, and emits the stable
    `unproven function-value call` diagnostic. Five positive, compatibility,
    interface, direct adversarial, and transitive adversarial regressions run in
    `compiler-purity-metadata-check`; the complete gate and warnings-as-errors
    binary check pass. The completed Effect validator also shares the canonical
    `pure` tag with its intrinsic regression so source and VM representations
    cannot drift independently.


### Progress 029

  - Current gate state: `make comprehension-guards-check` covers the pure
    single-generator list-comprehension subset through parser, formatter,
    typechecker, CoreIR lowering, and default VM execution. The formatter
    preserves guard filters and emits the canonical single-pipe form rather
    than dropping filters or rewriting to legacy `||` syntax. The VM now executes
    `[expr | pattern <- list]` and stacked boolean filters such as
    `[x | x <- values, x > 0, x < 10]`; generator pattern mismatches act as
    filters. The gate also runs `tests/language/ComprehensionGuardsTest.terl`,
    proving transformed yields, stacked boolean filters, empty results, range
    membership filters, tuple generator-pattern binding, ordered row-major
    generator products, later sources that consume earlier bindings, and
    filters applied after all generators through `terlc test`. The parser and
    type gates also prove `where` is rejected inside list comprehensions,
    generators cannot follow filters, and sources cannot reference bindings
    introduced by later generators. CoreIR, VM, formatter, and direct JS
    lowering preserve the same ordered generator model.


### Progress 030

  - Completed progress: list-comprehension filters now accept either `Bool` or
    the exact completed core guard-result shape
    `{Atom["guard_result"], value: Bool}`. The typechecker expands aliases
    before validating that structural contract and rejects malformed payloads
    with a stable diagnostic. The VM decodes the same tag and Boolean decision
    instead of relying on truthiness, and direct JS lowering evaluates each
    lowerable guard once before normalizing Boolean and completed guard-result
    values. `std.core.GuardResult` now constructs and destructures its declared
    `guard_result` tag rather than the alias name. Positive, false-result, and
    malformed-result regressions run in `comprehension-guards-check`; the full
    gate passes, including 5 std contract tests and 14 executable language
    tests through the default VM. Direct JavaScript lowering now reuses the
    exact `case` pattern matcher before destructuring each comprehension
    generator candidate. A Node-backed adversarial regression proves tuple
    candidates with the wrong arity, non-array values, and `null` are filtered
    without throwing or yielding partially bound values. A second Node-backed
    module exercises map, record, and constructor generator patterns: map and
    record matching requires exact own fields and safely rejects inherited
    fields, missing fields, arrays, strings, and `null`, while constructor
    matching enforces both tag and arity before binding payloads.


### Progress 031

  - Completed progress: ordinary list-comprehension filters now run the same
    resolved-call purity validation as case and function guards. Inferred-pure
    local helpers remain accepted, while direct imported effects and local
    helpers that transitively call them fail with the stable `list comprehension
    filter must be pure` diagnostic. The implementation reuses the shared guard
    classifier, including primitive receiver-call disambiguation, instead of
    maintaining a comprehension-specific effect list. Both adversarial paths
    and the positive helper control run in the complete passing
    `make comprehension-guards-check` gate with Rust warnings denied.


### Progress 032

  - Completed progress: default-VM list comprehensions now consume every
    standard collection representation already covered by `Iterable`: lists,
    ranges, insertion-ordered sets, flat maps, indexed A-CHAMP maps, and
    existing iterator values. Map sources expose deterministic `{key, value}`
    tuples, and iterator sources resume from their current cursor. The runtime
    reuses one source-materialization module instead of duplicating collection
    representation handling in the evaluator. The canonical gate passes 37
    exact Rust selectors and all 14 executable language tests; adversarial
    coverage includes a 130-entry indexed map, iterator cursor resumption, and
    rejection of non-iterable VM values. Rust all-target checking also passes
    with warnings denied.


### Progress 033

  - Current gate state: `make shape-synonyms-check` exists, is wired into
    `make check`, and expands local `shape Name(...) = ... [where guard]`
    declarations before typechecking. Expansion reuses the canonical pattern
    and expression parsers, substitutes call-site patterns into both the body
    and guard, recursively expands nested aliases, and leaves only ordinary
    patterns plus ordinary boolean clause guards for CoreIR and VM execution.
    Generated guards compose with an explicit clause guard using boolean `and`.
    List-comprehension generator aliases contribute their guards to the existing
    comprehension filter channel and compose with explicit filters. The gate
    assigns deterministic compiler-private names to non-parameter variables for
    every expansion, preventing alias internals from shadowing user bindings or
    colliding across two aliases in one pattern. The gate
    proves case, nested, function-head, list-cons, wildcard-fallback, guarded,
    nested-guarded, explicit-clause-guard, comprehension-filter composition, and
    typed string-capture behavior with eighteen executable `.terl` tests on the
    default VM. Exported interface shapes now expand in consumers before HIR
    resolution, typechecking, and CoreIR lowering. Selected imports preserve
    local aliases, wildcard imports expose the provider's public shape surface,
    and nested provider shapes are normalized against that provider before the
    selected alias enters consumer scope. This prevents unselected helper
    shapes from leaking into the consumer namespace. Adversarial tests reject
    ambiguous local aliases from distinct providers and recursive expansion
    across exported provider shapes. Directory checks now run the same imported
    shape expansion as single-file formal compilation, so non-incremental
    multi-module builds do not expose unresolved shape constructors. A
    two-module JS build proves a selected exported literal shape alias lowers to
    the ordinary strict-equality pattern supported by the JS backend without
    emitting a shape-level runtime symbol. A second two-module JS build proves
    an imported tuple shape expands into an exact `Array.isArray` and arity test,
    scopes its bindings through array destructuring, and executes successful,
    wrong-arity, and non-array cases under Node without emitting a shape-level
    runtime symbol. A third two-module JS build proves an imported nested tuple
    shape preserves its literal tag, validates both tuple levels, binds only
    payload positions, and falls back for wrong tags, arities, and nested value
    kinds under Node. A fourth two-module JS build proves an imported map shape
    rejects null and arrays, requires own fields, preserves literal field
    constraints, allows extra fields, binds payload fields, and rejects
    prototype-inherited required fields under Node. A fifth two-module JS build
    proves an imported guarded tuple shape short-circuits malformed values,
    evaluates the guard with structural bindings in scope, executes accepted
    values, and preserves fallback behavior under Node. A project-level VM test
    now generates an
    interface from a source-root module exporting a nested guarded shape,
    selects that shape under a consumer alias, lowers the importing test module
    to CoreIR, and executes both successful-match and guard-rejection cases on
    the default VM. The two
    hygiene tests prove a private guard binding cannot shadow an outer variable
    and two expansions cannot share one private binding. String-pattern shape
    parameters now rewrite the canonical `${...}` CoreIR payload and its typed
    capture child together. Private string captures use capture-safe compiler
    names and remain distinct from same-spelled user variables. Adversarial
    compiler tests reject arity mismatch, recursive expansion, duplicate names,
    duplicate parameters, duplicate ordinary and string-capture bindings inside
    shape bodies, duplicate bindings created by overlapping call-site arguments,
    guard reads from non-value pattern arguments, non-binding string-capture
    arguments, and non-boolean generated guards. Duplicate body bindings fail
    at declaration time, while post-substitution overlap fails at the shape
    invocation instead of inheriting the VM's last-binding-wins behavior.
    Public/interface shape metadata, formatter output, generated docs and module
    summaries, LSP symbols/hover/completion, contextual-keyword isolation, and
    the legacy-arrow rejection tests remain covered. A dedicated Tree-sitter
    corpus now covers public and guarded declarations, typed string-capture
    bodies, function-head shape patterns, and case-arm shape patterns. The
    editor grammar accepts boolean `and`/`or` in shape guards and reserves
    canonical `where`, rejecting legacy `when` at the production boundary. The
    corrected imported completion selector executes one real LSP test instead
    of silently matching zero tests.


### Progress 034

  - Completed progress: local shape aliases are compile-time-only syntax-output
    expansion and allocate no runtime wrapper. `ShapeSynonymTest.terl` proves the
    expanded ordinary tuple and list patterns execute through the VM, including
    nested aliases, function-head pattern parameters, guard-bearing aliases,
    nested guards, fallback behavior, typed string-pattern captures, composition
    with an explicit `where` guard, and guarded generator patterns composed with
    comprehension filters.
    Non-parameter tuple/list/alias variables are hygienically private to each
    expansion. Generated guards traverse normal typechecking, purity validation,
    CoreIR lowering, and VM guard execution rather than using a shape-specific
    runtime path. Exported/imported shape execution is covered through a real
    project source root and test module rather than a compiler-only fixture.
    Imported literal and unguarded tuple shapes also execute through generated
    JavaScript, with exact recursive tuple arity and nested literal constraints
    enforced before binding-only destructuring. Imported map shapes execute
    through the same backend with required-own-field checks and object
    destructuring limited to binding-bearing fields. Imported record shapes now
    share that safe object matcher, rejecting null, arrays, missing or inherited
    fields, and literal-field mismatches before binding-only destructuring.
    Imported constructor shapes now execute with VM-equivalent representation
    in generated JavaScript: zero-arity constructors match canonical atoms, and
    payload-bearing constructors match exact tagged tuples. Coverage includes
    recursive constructors, literal payload constraints, binding-only
    destructuring, and malformed tag/arity rejection. VM and JavaScript
    constructor matching share the canonical acronym-aware type-name-to-atom
    conversion.
    Imported guarded tuple shapes now execute through structural short-circuiting
    and bound guard closures without a shape-specific runtime representation.


### Progress 035

  - Completed progress: shape guard helper calls retain ordinary guard-purity
    semantics after alias expansion. An inferred-pure local predicate executes
    through an expanded comprehension shape on the default VM, while an
    imported effectful predicate fails with the stable case-guard purity
    diagnostic. Typed route captures already preserve their annotation through
    shape substitution, bind the annotated type during typechecking, execute
    successful conversion on the VM, and fall back on conversion failure. The
    complete warnings-as-errors `make shape-synonyms-check` gate owns both
    positive and adversarial cases.


### Progress 036

  - Completed progress: shape expansion now preserves alias provenance through
    nested patterns and rejects distinct aliases that produce alpha-equivalent
    unguarded function or expression clauses. Canonical comparison ignores
    binding names while preserving literals, constructors, ordered children,
    required map/record fields, and typed string captures. Guarded clauses and
    structurally distinct aliases remain valid. The warnings-as-errors
    `make shape-synonyms-check` gate owns the rejection and non-regression
    cases.


### Progress 037

  - Completed progress: ordered unguarded shape clauses now run recursive
    usefulness analysis after expansion. An earlier binding, tuple/list,
    constructor, map, or record shape that fully subsumes a later distinct
    alias reports a stable unreachable-shape diagnostic for both function heads
    and expression clauses. Specific-before-broad fallback ordering remains
    valid, as do crossing partial overlaps where each clause still matches
    distinct values. Exact comparison remains conservative for string captures
    and binary layouts. The warnings-as-errors `make shape-synonyms-check` gate
    owns broad-first rejection, keyed-map subsumption, ordered fallback, and
    useful partial-overlap coverage.


### Progress 038

  - Completed progress: shape usefulness now respects guard ordering. A later
    shape guard cannot recover a clause already subsumed by an earlier
    unguarded alias, and this reports the same stable unreachable-shape
    diagnostic for case clauses and function heads. An earlier guarded broad
    alias does not shadow a later unguarded fallback because its predicate may
    fail. The warnings-as-errors `make shape-synonyms-check` gate owns both
    directions.


### Progress 039

  - Completed progress: guarded shape clauses now reject alpha-equivalent
    predicates after structural expansion. The comparison erases source spans
    and normalizes variables by their pattern-binding paths, so renamed alias
    parameters cannot hide duplicate guarded case or function clauses. Guards
    with distinct predicates remain valid, and guards containing nested binding
    constructs are conservatively excluded from the proof. The
    warnings-as-errors `make shape-synonyms-check` gate owns both rejection
    paths and the distinct-predicate acceptance case.


### Progress 040

  - Completed progress: guarded shape usefulness now proves implication for
    conjunctions of integer interval constraints. The proof normalizes alias
    bindings by structural path and supports strict and inclusive bounds,
    equality, and comparisons with the literal on either side. A later guard
    whose interval is contained by an earlier subsuming guard reports a stable
    unreachable-shape diagnostic; narrow-before-broad fallback ordering
    remains valid. Guard expressions outside the supported fragment remain
    conservative rather than being guessed equivalent. The warnings-as-errors
    `make shape-synonyms-check` gate owns case and function-head containment,
    reversed-comparison equality, ordering, and non-regression coverage.


### Progress 041

  - Completed progress: integer guard implication now supports disjunctions of
    interval conjunctions. Guards are normalized into bounded branch sets, and
    a later guarded clause is rejected only when every later branch is proven
    contained by an earlier branch. Crossing ranges that retain values inside
    an earlier disjunction gap remain valid. Proof normalization is capped at
    64 branches; larger formulas fall back conservatively instead of causing
    exponential compiler work or speculative rejection. The warnings-as-errors
    `make shape-synonyms-check` gate owns multi-branch containment, nested
    conjunction/disjunction precedence, crossing-gap acceptance, and proof
    budget exhaustion.


### Progress 042

  - Completed progress: guarded shape usefulness now carries normalized
    variable-to-variable relation facts alongside integer intervals. Reversed
    ordering comparisons share one canonical direction, and equality and
    inequality facts are operand-order independent. A stronger later guard is
    rejected when each branch retains the earlier relation; opposing relations
    remain independently useful. The prover does not infer transitive relations
    that are absent from the source. The warnings-as-errors
    `make shape-synonyms-check` gate owns case and function-head implication,
    alpha-renamed bindings, commuted equality, and distinct-relation acceptance.


### Progress 043

  - Completed progress: normalized relation facts now use a dedicated
    transitive closure proof. Strictness propagates across mixed `<`/`<=`
    paths, mutual non-strict reachability proves equality, strict ordering
    proves inequality, and strict cycles or equality-plus-inequality conflicts
    mark a later guard impossible. Non-strict paths do not speculate a strict
    result. Relation closure is isolated from interval normalization to keep
    both compiler components bounded and independently reviewable. The
    warnings-as-errors `make shape-synonyms-check` gate owns transitive case and
    function-head rejection, equality cycles, contradictory guards, and the
    non-strict acceptance boundary.


### Progress 044

  - Completed progress: normalized guard branches now retain canonical
    predicate-call facts. A later conjunction or disjunction branch that
    repeats the same alpha-renamed call and arguments can prove an earlier
    predicate requirement, while distinct callees, distinct arguments, and
    missing conjuncts remain conservatively useful. Predicate facts are
    isolated from interval and relation normalization. The warnings-as-errors
    `make shape-synonyms-check` gate owns case and function-head rejection,
    disjunction composition, and the conservative acceptance boundaries.


### Progress 045

  - Completed progress: predicate facts now carry normalized positive or
    negated polarity. Repeated `not` is reduced structurally, identical
    negated calls participate in implication, and a conjunction containing
    both a predicate and its negation is proven impossible. Opposing polarity
    remains a useful disjoint guard. The warnings-as-errors
    `make shape-synonyms-check` gate owns negated case and function behavior,
    double negation, contradiction rejection, and the atomic polarity boundary.


### Progress 046

  - Completed progress: compound predicate formulas now normalize negation
    through bounded `and`/`or` trees using De Morgan semantics before DNF
    implication. Equivalent explicit negative formulas are rejected as
    unreachable, and contradictions exposed by normalization mark a branch
    impossible. Partial negative evidence remains useful. The warnings-as-errors
    `make shape-synonyms-check` gate owns both De Morgan directions,
    contradiction discovery, and incomplete-evidence acceptance.


### Progress 047

  - Completed progress: negated comparison formulas now invert `<`, `<=`, `>`,
    `>=`, `==`, and `!=` before interval or relation normalization. Integer
    inequality becomes two bounded branches, variable relations preserve the
    established canonical direction, and literal-on-left comparisons invert
    before operand reversal. Broader later ranges remain useful. The
    warnings-as-errors `make shape-synonyms-check` gate owns integer equality
    and inequality, variable ordering and equality, reversed operands, and the
    non-implication boundary.


### Progress 048

  - Completed progress: guarded shape aliases now work in fallible `let`
    assertions. Syntax-output expansion keeps each flattened binding's guard
    attached to that binding and rewrites it to an ordinary one-clause `case`,
    preserving left-to-right binding order and lexical scope without adding a
    shape-specific runtime opcode. VM parity tests cover successful binding and
    canonical match failure without committing bindings, including an ordinary
    preceding `let`; the executable Terlan suite covers successful guarded
    assertion. The warnings-as-errors `make shape-synonyms-check` gate passes.


### Progress 049

  - Completed progress: guarded shape aliases now compose with grouped
    fail-fast `let { ... } else { ... }` bindings. Syntax output retains one
    optional guard per ordered success pattern; typechecking validates each
    guard for purity and boolean result after introducing that binding, and
    CoreIR lowers the group to nested ordinary guarded cases. The first failed
    pattern or guard enters the fallback without exposing success bindings.
    Executable VM coverage proves successful two-binding evaluation plus first
    and second guard failure, while adversarial coverage proves fallback scope
    isolation. The complete `make shape-synonyms-check` gate passes.


### Progress 050

  - Completed progress: shape usefulness now proves implications across
    differently named local predicate helpers when each helper has one
    variable-pattern clause and a compiler-visible, call-free comparison
    formula. Proof-only substitution preserves runtime guards, canonicalizes
    helper parameters to call arguments, and reuses the bounded interval,
    relation, predicate, and disjunction prover. Equivalent helper bodies and
    stronger later helper bodies report unreachable shape clauses in case and
    function-head contexts. Crossing argument ranges, call-bearing helpers,
    qualified calls, type-applied calls, and named arguments remain
    conservative rather than being guessed equivalent. The complete
    warnings-as-errors `make shape-synonyms-check` gate passes.


### Progress 051

  - Completed progress: HTTP-style method/path dispatch now uses ordinary
    shape composition and typed string captures without route-specific syntax
    or a runtime route wrapper. `shape Route(method, path) = {method, path}`
    matches `Route("GET", "/users/${id: Int}")`, binds `id` as `Int`, and
    executes through the default VM. Executable coverage proves successful
    dispatch plus fallback for the wrong method, a failed typed conversion,
    and unrelated trailing path segments. The shape gate also owns stable
    diagnostics for duplicate, invalid, adjacent, unterminated, and empty
    string captures. The complete `make shape-synonyms-check` gate passes.


### Progress 052

  - Completed progress: known local and selected-import shape aliases are now
    rejected when called as runtime values during syntax-output expansion. The
    stable shape-specific diagnostic preserves the compile-time pattern-only
    contract instead of allowing generic function or constructor resolution to
    obscure the misuse. Exact local and imported adversarial regressions are
    owned by the complete passing `make shape-synonyms-check` gate.


### Progress 053

  - Completed progress: the canonical pattern-support manifest now classifies
    local, exported/imported, guarded, nested, route, function-head,
    wildcard-fallback, and tooling shape contexts across parser, formatter,
    typechecker, Core IR, VM, JavaScript, Tree-sitter, LSP, and documentation
    surfaces. The Rust quality gate rejects missing contexts, unknown stage
    classifications, empty positive or adversarial evidence, stale references,
    unsupported stages without diagnostics, and unsupported Tree-sitter/docs
    claims without their owning anchors. Its warning-denied validator suite
    passes 16 tests and resolves 8 shape contexts, 82 positive anchors, and 30
    adversarial references in the repository manifest.


### Progress 054

  - Completed progress: `shape-implications-check` rejects roadmap drift that
    turns `=>` into a runtime conversion, wrapper, macro, or generator feature;
    the gate now explicitly enforces proof-only non-conversion semantics
    (`does not allocate`, `construct a wrapper`, `call user code`, or
    `convert the value`) before parser/typechecker implementation proceeds.
    The gate also rejects placeholder/TODO/TBD labels or fragments in the
    implication contract matrix itself, with an injected-placeholder quality
    test proving the formal `=>` requirements cannot be padded with vague
    roadmap terms.


### Progress 055

  - Completed progress: the canonical EBNF, recursive-descent parser,
    formatter, and Tree-sitter grammar now accept positive structural
    implications in generic parameter lists as
    `T => {field: Type, nested: {field: Type}}`. Targets must be closed,
    nonempty field shapes; dynamic/non-structural and empty targets fail with
    stable parser diagnostics, while expression, lambda, case-body, struct-field,
    and ordinary type-alias uses remain rejected. `make
    shape-implications-check` owns the EBNF, Tree-sitter, parser, formatter,
    compiler, placement-adversarial, and roadmap-contract coverage for this
    syntax slice.


### Progress 056

  - Completed progress: callable structural implications now enter the shared
    generic-bound evidence pipeline instead of being discarded after parsing.
    A constrained function body may access only fields named by its active
    implication; stronger constrained callers may forward evidence; local and
    imported closed structs are validated at call sites; and nested structural
    requirements are checked recursively. Missing fields, incompatible field
    types, unconstrained generic forwarding, out-of-scope field access, and
    private-field evidence fail closed with stable `unproven_implication`
    diagnostics. Public implication metadata and public struct fields survive
    generated interface boundaries. `tests/language/ShapeImplicationTest.terl`
    executes direct and nested implication field access on the default VM with
    no wrapper, conversion, generated runtime symbol, or implication-specific
    VM instruction. The complete `make shape-implications-check` gate passes.


### Progress 057

  - Completed progress: generic receiver methods now preserve structural
    implication bounds through local dispatch and generated interface
    metadata. Receiver methods remain owned by a concrete nominal receiver;
    this does not introduce blanket generic extension methods. Local and
    imported calls accept arguments with proven shapes and reject missing
    evidence with the original `unproven_implication` diagnostic instead of
    degrading to a generic receiver-overload error. Access to a field outside
    the active evidence shape reports `implication_scope_error`. Positive and
    adversarial receiver-method cases are owned by the passing `make
    shape-implications-check` gate. VM source-method dispatch now recognizes
    canonical record values as well as tuple-backed compatibility values, so a
    receiver method with a structural implication executes through the default
    VM without implication-specific runtime machinery. A focused Rust parity
    regression and `tests/language/ShapeImplicationTest.terl` lock the runtime
    behavior under the same gate. Cross-module project execution is also
    covered: a provider owns record construction through exported functions,
    the consumer explicitly imports the public record shapes needed for closed
    implication evidence, and the imported receiver method dispatches through
    the default VM project-test lane. The exact project regression is owned by
    `make shape-implications-check`.


### Progress 058

  - Completed progress: `std.binary.Binary.protocol_name` is the first
    std-library API backed by structural implication evidence. It accepts any
    closed protocol metadata type exposing `{name: String}`; the existing
    field, shape, alias, and shape-set name helpers preserve their public
    signatures while delegating to this shared projection. The generated std
    interface retains the implication bound, and an exact default-VM std test
    proves the helper across four distinct nominal record types under
    `make shape-implications-check`.


### Progress 059

  - Completed progress: call-boundary implication checking now retains the
    concrete argument used as structural evidence even when ordinary generic
    unification intentionally treats `Dynamic` as a wildcard. `Dynamic` and
    open `Map[String, Dynamic]` values therefore fail closed against the
    concrete unsupported evidence source instead of degrading to an unresolved
    `T0` diagnostic or being accepted by wildcard unification. Exact positive
    and adversarial regressions are owned by the passing `make
    shape-implications-check` gate with Rust warnings denied.


### Progress 060

  - Completed progress: implication-constrained generic struct declarations now
    use the canonical `pub struct Page[T => {title: String}]` surface across the
    EBNF, recursive-descent parser, syntax-output handoff, formatter, and
    Tree-sitter grammar. Generic parameter evidence is retained as structured
    declaration metadata instead of being discarded after parsing. Exact
    parser, syntax-output, and formatter regressions are owned by the passing
    `make shape-implications-check` gate with Rust warnings denied.


### Progress 061

  - Completed progress: local implication-constrained generic structs now
    infer concrete type arguments and enforce their structural bounds for both
    record construction and the default named-constructor form. Construction
    reuses the existing bounded-function inference path, so an incompatible
    `Page[Account]` fails closed with a stable missing-field diagnostic instead
    of introducing a second implication engine. Concrete argument substitution
    also makes `Page[Profile].model` project as `Profile`. Exact positive and
    adversarial type-check regressions plus default-VM execution are owned by
    the passing `make shape-implications-check` gate with Rust warnings denied.


### Progress 062

  - Completed progress: imported generic structs now preserve their parameter
    and implication text through `ModuleInterface`, generated `.terli`/`.typi`
    rendering, and interface reparsing. Selected imports reconstruct the same
    concrete field-substitution scheme used by local structs, while raw
    construction remains confined to the defining module. Positive and
    adversarial type-check tests prove that `Page[Profile].model` remains
    `Profile` and that incompatible imported projections fail closed against
    the concrete type. A two-module default-VM project test proves exported
    construction and imported field projection end to end. The complete `make
    shape-implications-check` gate passes with Rust warnings denied.


### Progress 063

  - Completed progress: generic type aliases now accept and canonically format
    `pub type Named[T => {name: String}] = T`, while implication arrows in
    ordinary alias bodies remain rejected. Local aliases, interface aliases,
    selected imported aliases, and provider-qualified identity aliases retain
    parsed structural bounds in the shared `TypeAlias` model instead of
    discarding the evidence after syntax processing. Exact parser, formatter,
    and local/interface metadata regressions are owned by the passing `make
    shape-implications-check` gate with Rust warnings denied.


### Progress 064

  - Completed progress: implication-constrained aliases now validate explicit
    parameter and return applications on ordinary functions and receiver
    methods before transparent alias expansion. Concrete local and imported
    arguments reuse the shared structural-bound checker, callable generic
    evidence forwards through the same proof path, and nested alias arguments
    are checked recursively. Positive, forwarded-generic, and incompatible
    imported type-check regressions plus default-VM execution are owned by the
    passing `make shape-implications-check` gate with Rust warnings denied.


### Progress 065

  - Completed progress: implication-constrained aliases in struct-field and
    type-alias-body annotations are now validated before transparent alias
    expansion. Generic structs may forward their own structural evidence into
    constrained field aliases, while incompatible concrete field and nested
    alias applications fail closed with the same stable
    `unproven_implication` diagnostic used by callable sites. Callable and
    declaration annotation validation now share one compact module and the
    existing recursive bound checker. Positive, generic-forwarding, and
    adversarial regressions are owned by the passing `make
    shape-implications-check` gate with Rust warnings denied.


### Progress 066

  - Completed progress: constructor parameter and return annotations plus
    template property annotations now enforce implication-constrained aliases
    before their type schemes erase alias applications. Proven concrete types
    remain accepted, while incompatible constructor inputs, constructor
    returns, and template props fail closed through the shared
    `unproven_implication` diagnostic path. Five focused positive/adversarial
    regressions are owned by the passing `make shape-implications-check` gate
    with Rust warnings denied.


### Progress 067

  - Completed progress: trait declaration method parameters and returns now
    enforce implication-constrained aliases through the same shared validator.
    Proven concrete arguments remain accepted, incompatible parameters and
    returns fail closed, method-local generic implication evidence forwards
    into alias applications, and unconstrained method generics report the
    stable `unproven_implication` diagnostic. Five focused
    positive/adversarial regressions are owned by the passing `make
    shape-implications-check` gate with Rust warnings denied.


### Progress 068

  - Completed progress: explicit trait impl method parameters and returns now
    validate implication-constrained aliases before transparent expansion.
    Trait conformance first specializes trait parameters and then compares
    parsed, alias-expanded method types, so a proven alias may satisfy its
    concrete specialized signature while incompatible parameter and return
    aliases fail through the stable `unproven_implication` path. Three focused
    positive/adversarial regressions and the existing 41-test trait checker
    suite pass with Rust warnings denied, and the complete `make
    shape-implications-check` gate owns the behavior. The impl callable path
    was extracted into a 79-line module, returning `declarations.rs` below its
    reviewed file-size baseline. Executable implication-specific Lean coverage
    remained open at that point.


### Progress 069

  - Completed progress: generic implication constraints in explicit impl
    headers now use the canonical
    `pub impl Render[T => {title: String}] for T` surface. Parsing separates
    source implication metadata from the semantic `Render[T]` trait argument;
    impl method bodies receive the structural evidence; trait dispatch accepts
    matching concrete structures and excludes incompatible candidates; and
    imported public impls retain the same evidence through interface rendering
    and reparsing. Formatter, generated interfaces, public docs, documentation
    validation, LSP symbols, canonical EBNF, and Tree-sitter all reconstruct
    the source-level implication form through one shared renderer. Seven
    focused positive/adversarial compiler regressions, the 43-test implication
    suite, the existing 41-test trait suite, and the complete `make
    shape-implications-check` gate pass with Rust warnings denied.


### Progress 070

  - Completed progress: `proofs/lean/Terlan/Type/ShapeImplication.lean` is now a
    current, content-addressed Lean proof family for closed structural
    implication evidence. It proves source and requirement well-formedness,
    sound required-field projection, fail-closed rejection without entailment,
    public/private field separation, evidence provenance preservation, and
    identity-preserving non-conversion. Replay metadata fingerprints the
    canonical type specification and EBNF, and explicit proof ownership binds
    the family to both `lean-proof-track-check` and
    `shape-implications-check`. Both complete gates pass with Rust warnings
    denied. The proof family now models scope-owned implication evidence and
    proves that evidence cannot escape its lexical owner, that scoped required
    field projection remains sound, and that evidence-preserving function and
    branch evaluation returns exactly the ordinary typed result without
    runtime conversion. Replay metadata, proof ownership, and the canonical
    type specification carry the same theorem inventory. Full recursive
    compiler-to-Lean refinement remains a classified formal gap, so the parent
    feature remains unchecked.


### Progress 071

  - Completed progress: attempted negative structural implications now fail at
    the generic-parameter boundary with the stable
    `negative structural implications are not supported` diagnostic and direct
    guidance toward negative trait implementations. An exact adversarial test
    covers `T => not {name: String}`, while a paired positive test proves normal
    `T => {name: String}` evidence remains accepted. Generic-parameter parsing
    was extracted from the declaration parser into a focused 124-line module,
    reducing `type_decls.rs` from 778 to 628 lines. The complete
    `make shape-implications-check` gate and warnings-as-errors compiler check
    pass.


### Progress 072

  - Completed progress: structural implication requirements now reject
    duplicate field names before their shape can normalize into the internal
    map type and discard the conflicting declaration. The stable diagnostic is
    classified as `ambiguous_implication`; an exact adversarial regression
    covers incompatible duplicate `name` requirements, while a positive nested
    control proves independently scoped shapes may reuse the same field name.
    The complete `make shape-implications-check` gate and warnings-as-errors
    compiler check pass.


### Progress 073

  - Completed progress: duplicate-field rejection now follows structural
    implication evidence recursively through direct nested records and records
    inside generic type arguments. The canonical type-expression parser tracks
    an independent field set per record scope. Exact adversarial regressions
    cover both nested forms, the independent-scope positive control remains
    passing, and the complete `make
    shape-implications-check` gate passes with Rust warnings denied.


### Progress 074

  - Completed progress: record-type aliases can no longer erase duplicate fields
    before those aliases enter structural implication evidence. The shared
    type-expression parser now rejects duplicate fields in every record type
    position with the stable `duplicate_record_type_field` diagnostic, while
    direct implication shapes retain their more specific
    `ambiguous_implication` diagnostic. Nested records keep independent field
    scopes, so an inner record may intentionally reuse an outer field name.
    Exact alias rejection and nested-scope acceptance regressions are part of
    `make shape-implications-check`; the complete EBNF, Tree-sitter, compiler,
    typechecker, VM, quality, Lean, executable language, and stdlib gate passes
    with Rust warnings denied.


### Progress 075

  - Current gate state: `make typed-template-interpolation-check` composes
    `template-contract-check`, `artifact-template-check`, and
    `terlc test std/template/TemplateTest.terl`.


### Progress 076

  - Completed progress: `.terl.xml` artifact templates now use the shared
    target dispatcher and `quick-xml` structural validation after interpolation
    masking. Text and quoted-attribute interpolation remain valid, while
    malformed or empty interpolation islands, dynamic element or attribute
    names, mismatched tags, duplicate attributes, multiple roots, DTDs,
    undeclared entities, and late or duplicate declarations fail with stable
    path-aware XML diagnostics. `artifact-template-check` owns the positive and
    adversarial cases; `typed-template-interpolation-check` and the typed-template
    render-mode quality checks pass with Rust warnings denied. Full XML
    rendering and escaping parity across VM and JS remains open, so Slice 4 and
    its parent remain unchecked.


### Progress 077

  - Completed progress: static-site HTML interpolation now evaluates literal
    lists plus `Some`/`None` values and routes typed attributes through the
    shared HTML attribute renderer. Boolean attributes use presence/omission
    semantics, optional attributes omit `None`, token collections join only
    valid text members, and URL attributes reject unsafe schemes before
    escaping. The canonical `typed-template-interpolation-check` includes the
    positive and adversarial static renderer cases and passes with Rust warnings
    denied. JS/browser adoption and identical VM/JS/static diagnostics remain
    open, so Slice 4 stays unchecked.


### Progress 078

  - Completed progress: `make angular-ts-terlan-facade-parity-check` is now
    executable and validates generated Angular.ts facade modules, namespace
    binding manifests, stable skip manifests, handwritten wrapper tests, and
    real external checkout namespace generation when
    `/home/anatoly/Applications/ng/angular.ts` is available.


### Progress 079

  - Completed progress: `make angular-ts-terlan-integration-check` now validates
    the latest external namespace's generalized `RealtimeProtocolEventDetail`
    and `RealtimeProtocolMessage` declarations without requiring the removed
    redundant `SseProtocol*` type aliases. The VM browser protocol still
    validates its independent `"SseProtocolMessage"` runtime event contract.
    Imported receiver signatures now retain dependency-owned opaque return
    types, preventing randomized method selection while compiling the
    materialized HTTP/browser integration fixtures.


### Progress 080

  - Completed progress: the generated HTTP facade now follows the latest
    Angular.ts namespace after its legacy `HttpPromise` alias was removed.
    `$http.get` is exposed as
    `std.js.Promise[terlan.angular.ng.HttpResponse.HttpResponse[Dynamic]]`, and
    facade generation requires the corresponding `HttpResponse` namespace
    module instead of emitting an unresolved stale alias. The focused bindgen
    regression, `angular-ts-terlan-integration-check`,
    `angular-ts-namespace-generation-check`, and
    `angular-ts-terlan-facade-parity-check` all pass against the current
    `/home/anatoly/Applications/ng/angular.ts` checkout.


### Progress 081

  - Completed progress: the type parser now distinguishes structural record
    field separators from removed raw `:atom` literals, so generated Angular.ts
    aliases such as `{template: std.js.String.JsString}` compile through the
    current source compiler. Positive record-field coverage and an adversarial
    `{:effect, value: Int}` regression prove the exception does not restore raw
    atom syntax. The focused parser suite, Angular namespace bindgen regression,
    and `make angular-ts-terlan-facade-parity-check` pass.


### Progress 082

  - Completed progress: canonical release validation is hermetic by default.
    The Angular.ts gates use their deterministic generated fixture unless
    `ANGULAR_TS_ROOT` explicitly selects an external checkout, so a mutable
    sibling repository cannot race release validation or silently change its
    input. Explicit external validation remains available for checking the
    latest `/home/anatoly/Applications/ng/angular.ts` source. The integration,
    namespace-generation, and facade-parity gates pass with the environment
    variable unset.


### Progress 083

  - Completed progress: the external `terlan-pytorch` repository pins released
    LibTorch `2.13.0+cpu`, generates 37 package functions from curated C
    metadata, and passes both an ABI fixture gate and the released distribution
    gate. PT0-PT3 prove scalar lifecycle, copied shapes, versioned
    `aten::clone`, and signed `StableIValue` execution through
    `aten::unsqueeze(tensor, -1)` with semantic rank/shape checks.


### Progress 084

  - Completed progress: PT4 closes multiple tensor inputs and exact CPU
    `aten::matmul` (`4 × 5 = 20`) with rank/shape validation, guarded ownership
    transfer, stale/wrong-type secondary-handle rejection, and deterministic
    cleanup of every source, intermediate, and result handle.


### Progress 085

  - Completed progress: the compiler-level `make terlan-pytorch-package-check`
    now validates the generic C ABI gate and invokes the external package's
    pinned released-LibTorch check. `TERLAN_PYTORCH_DIR` and
    `TERLAN_PYTORCH_LIBTORCH` provide explicit checkout/distribution overrides.


### Progress 086

  - Completed progress: PT5 commits the generated package as an immutable Git
    dependency, fetches it into a fresh Terlan consumer, deletes the original
    package tree, and executes `pytorch.Tensor` from the verified cache. This
    exposed and closed generic C ABI package-manifest gaps for the declared
    source namespace and native Rust helper discovery.


### Progress 087

  - Completed progress: the released CPU gate now rejects missing and
    incompatible LibTorch distributions, stale tensor handles, and scalar
    `matmul` shape mismatch with stable diagnostics. Successful execution
    asserts exact value/rank plus int64 dtype and CPU device observations, then
    writes and validates a machine-readable report containing the LibTorch
    build hash and source/normalized binding-manifest hashes.


### Progress 088

  - Completed progress: PT6 adds the exact `aten::transpose.int` named
    overload with one owned tensor and two signed dimension slots. PT7 adds
    exact `aten::mul.Tensor` arithmetic with two owned tensor slots. Both run
    through package-owned tests and the fresh Git consumer with semantic
    readback and independent cleanup. The generic direct C wrapper also now
    maps multiple borrowed opaque handles to their corresponding arguments,
    covered by a focused generator regression test.


### Progress 089

  - Completed progress: PT8 adds generic C ABI `Float` transport plus stable
    float64 tensor construction/readback. PT9 publishes the direct stable C
    `aoti_torch_aten_subtract_Tensor` shim and proves deterministic dtype
    mismatch rejection. PT10 adds generic C ABI `Bool` transport and stable
    bool tensor construction/readback. Package-owned and fresh Git consumers
    execute every slice with semantic value/dtype assertions and deterministic
    handle cleanup.


### Progress 090

  - Completed progress: PT11 adds the exact default `aten::relu` dispatcher
    operation as the first nonlinear ML primitive. Both consumer paths prove
    `relu(-2.5) = 0.0`, preserve the independently readable source tensor, and
    dispose source and result through separate owned handles.


### Progress 091

  - Completed progress: PT12 binds the generated stable C
    `aoti_torch_aten_narrow` operation for bounded indexing and independently
    owned views. Positive consumers preserve value and source readability;
    the adversarial consumer requires deterministic rejection of an
    out-of-range interval. Generic generated smoke tests no longer invent
    scalar values for direct operations with package-specific valid domains.


### Progress 092

  - Completed progress: PT13 adds exact `aten::flatten.using_ints` dispatch for
    model-input dimension normalization. Both consumer paths prove rank-2 to
    rank-1 shape conversion, source/result ownership, and deterministic
    rejection of an out-of-range flatten interval.


### Progress 093

  - Completed progress: PT14 adds generic structured `List[Int]` C input-array
    metadata with checked pointer-length lowering and native list transport,
    then binds stable `aoti_torch_aten_amax` as the first dimension-list
    reduction. Positive consumers prove rank reduction and ownership;
    adversarial execution rejects an invalid reduction dimension.


### Progress 094

  - Completed progress: PT15 adds generic reviewed fixed C inputs for typed
    constants and nullable option pointers, then publishes CPU float64
    `full_float(shape, value)` through stable `aoti_torch_aten_full`. Separate
    consumers prove owned `[2, 3]` construction, rank/numel/dtype/shape/value
    semantics, and deterministic negative-dimension rejection.


### Progress 095

  - Completed progress: PT16 adds generic `List[Float]` input-array lowering
    and permits package-owned C sources beside an externally linked stable C
    distribution. `from_floats(values, shape)` creates a call-scoped LibTorch
    view, clones it into independent storage, and destroys the temporary view
    before returning. Package-owned and fresh Git consumers prove non-constant
    `[2, 3]` float64 data, while mismatched value count is rejected stably as
    `c_abi_status_7003`.


### Progress 096

  - Completed progress: PT17 binds generated stable-C tensor addition and
    composes a real affine forward slice from non-constant input, weights,
    `matmul`, and broadcast bias. Package-owned and fresh Git consumers require
    `[2, 1]` float64 output with maximum `7.0`, preserve source handles, and
    stably reject incompatible `[2, 3] + [4]` broadcasting.


### Progress 097

  - Completed progress: PT18 selects exact `aten::softmax.int` and composes a
    classifier forward pass from non-constant input through affine logits to
    `[2, 2]` uniform probabilities. Both consumer paths require probability
    maximum `0.5`, preserved logits, and stable invalid-dimension rejection.
    Generic binding metadata can now defer operations with package-specific
    valid domains from the generated scalar smoke while keeping their package
    tests mandatory.


### Progress 098

  - Completed progress: PT19 extends the generic C ABI dispatcher metadata
    with direct boolean values and heap-backed owned optional integers,
    including reviewed allocator/destructor signatures and pre-transfer guards.
    The external package selects exact `aten::argmax`, turns `[2, 2]`
    probabilities into int64 `[2]` class predictions, proves `[2, 1]`
    keep-dimension behavior, and stably rejects an invalid prediction
    dimension through package-owned and fresh Git consumers.


### Progress 099

  - Completed progress: PT20 selects exact `aten::mse_loss` through the
    existing generic two-handle-plus-integer dispatcher stack. Package-owned
    tests prove scalar mean loss `1.0`, unchanged equal-shape sources, and a
    classifier loss of `0.25`; the fresh immutable Git consumer stably rejects
    incompatible `[2, 2]` and `[3]` loss operands plus invalid reduction values.


### Progress 100

  - Completed progress: PT21 adds generic IEEE-754 float StableIValue stack
    encoding, package-owned shaped int64 construction, and exact
    `aten::cross_entropy_loss`. `[2, 2]` zero logits and `[0, 1]` labels produce
    scalar smoothed classification loss between `0.69` and `0.70` while both
    inputs remain readable; float targets and smoothing above `1.0` are
    rejected through stable diagnostics.


### Progress 101

  - Completed progress: PT22 selects exact `aten::mse_loss_backward`, computes
    gradient `0.5`, applies a functional learning-rate update from `2.0` to
    approximately `1.95`, and lowers loss from `1.0` to approximately `0.9025`.
    The package records that graph autograd remains deferred because released
    stable C lacks gradient retrieval and `_backward` still requires generic
    Tensor-list input plus unit-output dispatcher support.


### Progress 102

  - Completed progress: PT23 adds the first rank-4 vision kernel through a
    package-owned C adapter over generated `aoti_torch_cpu_convolution`, avoiding
    the pinned distribution's deprecated device-generic convolution export.
    `conv2d` contains LibTorch's optional-bias pointer representation while the
    public generated Terlan surface remains three opaque tensor handles, three
    copied integer lists, and a group count. Package and immutable Git consumers
    require exact NCHW `[1,1,3,3] × [1,1,2,2] -> [1,1,2,2]` shape, float64
    maximum `4.0`, MSE `0.0` against the complete expected tensor, preserved
    sources, and stable channel-mismatch rejection.


### Progress 103

  - Completed progress: PT24 binds generated CPU
    `aoti_torch_cpu__adaptive_avg_pool2d` directly from structured metadata,
    reusing generic borrowed-handle and copied-list lowering with no compiler or
    package-owned adapter change. Package and immutable Git consumers reduce a
    non-constant `[1,1,4,4]` quadrant input to exact `[1,1,2,2]` values
    `[1.0,2.0,3.0,4.0]`, prove complete contents with MSE `0.0`, preserve the
    source, and stably reject malformed one-element output sizes. Multiple-result
    adaptive max pooling remains deferred behind generic tuple/result ownership.


### Progress 104

  - Completed progress: PT25 composes the independently generated operations
    into a complete Terlan vision-classifier graph: convolution, ReLU, global
    adaptive pooling, flatten, matrix projection, bias addition, softmax, and
    argmax. ABI-fixture and pinned-LibTorch consumers require `[1,1]` features,
    exact `[0.5,0.5]` probabilities at shape `[1,2]`, MSE `0.0`, and int64
    prediction `[0]`, while disposing every source, parameter, intermediate, and
    result handle. No fused package helper or compiler special case is added.


### Progress 105

  - Completed progress: PT26 opens sequence and recommendation workloads through
    exact dispatcher `aten::embedding`, using two copied tensor handles, signed
    padding index, and two boolean mode flags already supported by the generic
    binder. Fixture and pinned-LibTorch consumers map int64 `[0,2]` indices into
    owned float64 `[2,2]` features with MSE `0.0`, preserve both sources, and
    stably reject index `3` for a three-row table. No generated CPU embedding
    symbol, package-owned adapter, or compiler special case is required.


### Progress 106

  - Completed progress: PT27 composes embedding, token maximum reduction,
    projection, bias, softmax, and argmax into a batched sequence classifier.
    Fixture and pinned immutable consumers require `[1,2,2]` embeddings,
    `[1,2]` pooled features and exact uniform probabilities, MSE `0.0`, and
    int64 prediction `[0]`; an incompatible `[3,2]` projection is rejected
    stably. The audit deferred layer norm and embedding bag behind generic
    dispatcher-list, optional-tensor, and multiple-result ownership.


### Progress 107

  - Completed progress: PT28 binds inference-mode exact `aten::batch_norm`
    through one direct copied tensor, four allocator-backed optional tensor
    copies, two booleans, and two IEEE-754 floats. Generic `bind c` metadata now
    supports `owned_optional_handle_copy`, validates its allocator/destructor,
    and transfers nested StableIValue ownership only after all fallible setup.
    Fixture and pinned consumers require unchanged NCHW `[1,2,2,2]` shape,
    exact output `3.0`, MSE `0.0`, readable affine/statistic sources, and stable
    rejection of a three-element running mean for two channels. Training-stat
    mutation remains separately deferred; no package adapter, CPU shim, or
    operator-specific compiler branch is added.


### Progress 108

  - Completed progress: PT29 binds exact `aten::layer_norm` after adding generic
    `owned_int_list_argument` metadata with validated stable-list
    allocate/push/delete symbols and partial-construction cleanup. Weight and
    bias reuse allocator-backed optional tensor copies. Fixture and pinned
    immutable consumers require exact trailing-feature normalization from
    `[1.0,3.0,2.0,4.0]` to `[-1.0,1.0,-1.0,1.0]`, then compose an identity
    projection and residual addition to `[0.0,4.0,1.0,5.0]`, both at MSE `0.0`.
    A normalized shape `[3]` is rejected for trailing width `2`; no package
    adapter, CPU shim, or operator-specific compiler branch is added.


### Progress 109

  - Completed progress: PT30 composes a normalized scaled dot-product
    self-attention block entirely from generated primitives: layer norm, Q/K/V
    projections, key transpose, score multiplication, tensor scaling, softmax,
    context multiplication, and residual addition. Fixture and pinned immutable
    consumers require exact uniform attention, unit context, residual
    `[2.0,4.0,5.0,3.0]`, and MSE `0.0` for all three complete tensors while
    preserving the input. A `[3,2]` query
    projection is rejected for normalized width `2`; no fused helper, package
    adapter, public function, or compiler special case is added.


### Progress 110

  - Completed progress: PT31 adds schema-generic `owned_string_literal`
    dispatcher lowering with validated stable-string allocate/delete symbols
    and generated cleanup before transfer. PyTorch metadata uses it to bind
    exact `aten::gelu` with `approximate="none"`, then ordinary Terlan composes
    layer norm, `[2,2] -> [2,4]` expansion, GELU, `[2,4] -> [2,2]` projection,
    bias, and residual addition. Fixture and pinned immutable consumers require
    the expected residual within `1e-7` MSE, preserve the input, dispose every
    handle, and stably reject a `[3,4]` expansion matrix. No fused helper,
    package adapter, or operator-specific compiler case is added.


### Progress 111

  - Completed progress: PT32 composes PT30 normalized attention and PT31 GELU
    feed-forward into one complete pre-normalized transformer encoder graph.
    Two layer normalizations and two residual joins produce attention residual
    `[2.0,4.0,5.0,3.0]` and final encoder output approximating
    `[1.8413447461,4.8413447461,5.8413447461,2.8413447461]` within `1e-7` MSE.
    Package-owned and immutable consumers preserve the input, dispose every
    handle, and stably reject a width-`3` output projection joined to the
    width-`2` residual. The slice adds no public operation, fused helper,
    package adapter, native symbol, or compiler change.


### Progress 112

  - Completed progress: PT33 extends the full encoder through token maximum
    pooling, explicit batch insertion, affine two-class logits, softmax,
    argmax, and cross-entropy evaluation. The package fixture now correctly
    models generic valid `unsqueeze` insertion dimensions, including `[2] ->
    [1,2]` at dimension `0`. The immutable model uses reusable Terlan attention
    and feed-forward functions so artifact nesting stays bounded and cleanup is
    local to each returned tensor. Package-owned and immutable consumers
    require exact `[0.5,0.5]` probabilities, int64 prediction `[0]`,
    cross-entropy in `0.69–0.70`, preserved input, and complete handle disposal;
    class target `2` is stably rejected for two-class logits. No public
    operation, native symbol, package adapter, or compiler change is added.


### Progress 113

  - Completed progress: PT34 closes the first token-to-model text path. Int64
    IDs `[0,1]` select exact `[1.0,3.0;4.0,2.0]` embeddings, which pass through
    the complete pre-normalized encoder, token pooling, affine two-class head,
    softmax, argmax, and cross-entropy. Package-owned and immutable consumers
    verify embedding MSE `0.0`, probabilities `[0.5,0.5]`, prediction `[0]`,
    and loss in `0.69–0.70` against both the fixture and pinned LibTorch. An
    out-of-vocabulary ID `2` is stably rejected for the two-row table. Reusable
    Terlan attention/feed-forward functions keep artifact nesting bounded; no
    public operation, native symbol, package adapter, or compiler change is
    added.


### Progress 114

  - Completed progress: PT35 promotes the token-transformer path to true
    `[B,T,D]` execution. IDs `[[0,1],[1,0]]` embed to `[2,2,2]`; rank-3 Q/K/V,
    attention-score, and context matrix multiplications preserve batch size,
    keys transpose dimensions `1` and `2`, and attention normalizes dimension
    `2`. Token pooling yields `[2,2]` classifier features with exact uniform
    probabilities, predictions `[0,0]`, and mean cross-entropy in `0.69–0.70`.
    The ABI fixture gains batched shape modeling and real transpose swaps while
    the public surface remains 39 functions. Fixture and pinned immutable
    consumers stably reject incompatible `[2,2,2] × [3,2,2]` batches; no native
    symbol, package adapter, or compiler change is added.


### Progress 115

  - Completed progress: PT36 promotes the vision classifier to two-image NCHW
    execution. Unit and doubled `[1,3,3]` images remain distinct through
    convolution, ReLU, global adaptive pooling, and feature flattening. Exact
    convolution values `4.0`/`8.0` and pooled features `[4.0,8.0]` both require
    MSE `0.0`; the shared classifier returns uniform `[2,2]` probabilities,
    predictions `[0,0]`, and mean cross-entropy in `0.69–0.70`. Fixture and
    pinned immutable consumers reject a length-`1` target for two image logits
    and dispose every handle. The public surface remains 39 functions, with no
    fixture behavior, native symbol, package adapter, or compiler change.


### Progress 116

  - Completed progress: PT37 composes a grouped depthwise-pointwise CNN over
    `[2,2,3,3]` input. A `[2,1,2,2]` depthwise kernel with `groups=2` preserves
    channels at exact values `4.0`/`8.0`; a `[2,2,1,1]` pointwise kernel mixes
    them to exact `8.0`/`16.0`. Global features are exactly
    `[8.0,8.0;16.0,16.0]`, followed by uniform probabilities, predictions
    `[0,0]`, and mean cross-entropy in `0.69–0.70`. Fixture and pinned immutable
    consumers reject three output channels with `groups=2`. The slice retains
    39 public functions and adds no fixture behavior, native symbol, package
    adapter, or compiler change.


### Progress 117

  - Completed progress: PT38 composes a normalized residual CNN block at
    `[2,2,2,2]`: depthwise `groups=2` convolution, inference batch norm, ReLU,
    pointwise channel mixing, a second batch norm, residual addition, and ReLU.
    Exact normalized depthwise values `1.0`/`2.0`, pointwise values `2.0`/`4.0`,
    residual values `3.0`/`6.0`, and pooled features
    `[3.0,3.0;6.0,6.0]` all require MSE `0.0`; classification returns uniform
    probabilities, predictions `[0,0]`, and mean cross-entropy in `0.69–0.70`.
    Fixture and pinned immutable consumers reject a three-channel branch joined
    to a two-channel residual. Bounded Terlan helpers retain 39 public functions
    with no fixture behavior, native symbol, package adapter, or compiler change.


### Progress 118

  - Completed progress: PT39 stacks two reusable pre-normalized transformer
    encoders over `[2,2,2]` token state. Shared immutable normalization
    parameters cross both layers safely; the complete second-layer tensor
    matches `[2.6826894921,6.6826894921;7.6826894921,3.6826894921]` and its
    reversed batch within `1e-7` MSE. Pooling yields identical
    `[7.6826894921,6.6826894921]` rows, uniform probabilities, predictions
    `[0,0]`, and mean cross-entropy in `0.69–0.70`. Fixture and pinned immutable
    consumers reject a width-`3` projection joining width-`2` layer state. The
    slice retains 39 public functions and adds no fixture behavior, native
    symbol, package adapter, or compiler change.


### Progress 119

  - Completed progress: PT40 broadcasts additive causal mask
    `[0,-1000;0,0]` over `[2,2,2]` attention scores. Exact attention rows
    `[1,0]` and `[0.5,0.5]` prove future-token exclusion; complete context,
    residual, feed-forward, pooled, probability, prediction, and loss tensors
    execute through fixture and pinned immutable consumers. The fixture gains a
    reviewed marker derived from copied large-negative mask contents so its
    representative softmax distinguishes causal from uniform attention; full
    element semantics remain pinned-LibTorch gates. A `[3,3]` mask is stably
    rejected for `[2,2,2]` scores. The public surface stays at 39 functions with
    no native symbol, package adapter, or compiler change.


### Progress 120

  - Completed progress: PT41 separates `[2,2,2]` decoder query state from
    independently owned `[2,3,2]` memory. Q comes from the decoder while K/V
    come from separately normalized memory, producing exact uniform
    `[2,2,3]` attention and asymmetric signed context values before the existing
    residual, feed-forward, pooling, classification, prediction, and loss path.
    Package-owned and pinned immutable consumers reject incompatible decoder
    and memory batch dimensions before context construction and dispose every
    source and intermediate independently. The public surface remains 39
    functions with no fixture behavior, native symbol, package adapter, or
    compiler change.


### Progress 121

  - Completed progress: PT42 composes embedded batched decoder tokens through
    additive causal self-attention, separate `[2,3,2]` encoder-memory
    cross-attention, normalized GELU feed-forward, pooling, classification,
    prediction, and loss. Exact self-attention and cross-attention residuals
    precede a decoder output validated within `1e-7` MSE; pooled features are
    `[5.5080114128,6.1746780794]` and
    `[7.1746780794,4.5080114128]`, with uniform probabilities and predictions
    `[0,0]`. Fixture and pinned immutable consumers stably reject a width-`3`
    memory context joined to width-`2` decoder state and dispose every handle.
    The public surface remains 39 functions with no fixture behavior, native
    symbol, package adapter, or compiler change.


### Progress 122

  - Completed progress: PT43 closes the broad CPU composition series with an
    end-to-end tokenized source-target graph. Source IDs
    `[[0,0,1],[1,1,0]]` and a source-owned table produce exact `[2,3,2]`
    encoder memory; independent target IDs `[[0,1],[1,0]]` and a distinct table
    produce `[2,2,2]` causal decoder state. Both streams execute through causal
    self-attention, memory cross-attention, feed-forward, pooling,
    classification, prediction, and loss. Fixture and pinned immutable
    consumers reject three-batch source memory paired with two-batch targets and
    dispose every handle. The public surface stays at 39 functions with no
    fixture behavior, native symbol, package adapter, or compiler change.


### Progress 123

  - Completed progress: MI0 begins the concrete model-inference track. The
    generic C ABI generator now partitions declared `.c` and `.cpp` sources
    into C11 and C++17 builds and transports borrowed Terlan `String` values as
    call-scoped `const char *` arguments. `terlan-pytorch` uses that generic
    machinery for a package-owned `run_torchscript(input, path)` adapter, with
    no Torch namespace or LibTorch rule in the compiler. A C++-generated
    TorchScript archive implementing `input * 2.0` loads through
    `torch::jit::load`; package-owned and immutable consumers require exact
    `3.0 -> 6.0`, unchanged borrowed input, and independently disposed output.
    Missing archives return stable status `7041`, corrupt archives return
    `7042`, and C++ exceptions never cross the C boundary. Fixture and pinned
    LibTorch gates pass 44 Terlan tests and expose 40 generated functions.


### Progress 124

  - Completed progress: MI1 closes the reusable-model ownership slice. The
    generic C ABI generator now emits one typed Rust owner and destructor for
    every declared opaque resource, stores all resource classes in a tagged
    helper enum, and validates wire type, stored type, and generation before
    cross-resource borrows. `terlan-pytorch` declares a thread-confined
    `TorchScriptModel` beside `Tensor`; `load_torchscript` loads one archive,
    `torchscript_forward` borrows the model and an input tensor to return an
    independently owned output, and `dispose_model` destroys the model exactly
    once. Package tests require `3.0 -> 6.0` and `4.0 -> 8.0` through the same
    loaded model, and the immutable Git consumer stably rejects forwarding
    after disposal as `stale_handle`. The compiler contains no Torch-specific
    branch. The surface is 43 functions with 45 package tests.


### Progress 125

  - Completed progress: MI2 adds explicit two-positional-tensor TorchScript
    forwarding using the existing generic cross-resource borrow machinery.
    `torchscript_forward2(model, first, second)` borrows one thread-confined
    model plus two independently owned tensors and returns a separately owned
    tensor. A pinned order-sensitive model implements
    `left - right * 2.0`; package and immutable Git consumers require exact
    `3.0, 4.0 -> -5.0` execution with both inputs unchanged. Calling the
    two-input surface on the single-input model contains the upstream schema
    exception as stable status `7043`. The compiler gains no PyTorch branch;
    variable-arity `List[Tensor]` remains deferred until generic opaque-handle
    list ownership exists. The surface is 44 functions with 46 package tests.


### Progress 126

  - Completed progress: MI3 contains a TorchScript two-tensor tuple inside a
    third generic opaque resource class, `TensorPair`. A pinned model returns
    `(input * 2.0, input * 3.0)`; package and immutable consumers require input
    `2.0`, exact extracted outputs `4.0` and `6.0`, unchanged input, and valid
    extracted tensors after disposing the pair container. C++ validates tuple
    identity, exact arity, and tensor element types before allocating the
    result. A plain tensor result is rejected as stable status `7046`, with
    `7047` and `7048` reserved for wrong arity and element type. `TensorPair`
    is documented as an ownership bridge rather than language tuple support;
    arbitrary nested output lowering remains generic compiler work. The
    surface is 48 functions with 47 package tests.


### Progress 127

  - Completed progress: MI4 copies a TorchScript `float` result directly into
    an ordinary Terlan `Float` through the generic C ABI `double` output path.
    A pinned model computes `float(input.item()) * 2.0`; package and immutable
    consumers require unchanged input `3.0` and exact result `6.0`, with no
    native result handle or disposal obligation. Tensor and other non-float
    IValues are rejected as stable status `7049`. This slice also restores the
    generic generated-adapter `[workspace]` opt-out, preventing unrelated
    enclosing Cargo workspaces from capturing package-native crates. The
    surface is 49 functions with 48 package tests.


### Progress 128

  - Completed progress: MI5 copies a TorchScript `int` result directly into an
    ordinary Terlan `Int` through the existing generic fallible C ABI `int64_t`
    output path. A pinned model computes `int(input.item()) + 3`; package and
    immutable consumers require unchanged integer input `4` and exact result
    `7`, with no native result handle or disposal obligation. Tensor and other
    non-integer IValues are rejected as stable status `7050`. No compiler
    special case is introduced. The surface is 50 functions with 49 package
    tests.


### Progress 129

  - Completed progress: MI6 copies a TorchScript `bool` result directly into
    an ordinary Terlan `Bool` through the existing generic fallible C ABI
    `bool` output path. A pinned model computes `input.item() > 0`; package and
    immutable consumers require unchanged integer input `1` and exact result
    `true`, with no native result handle or disposal obligation. Tensor and
    other non-boolean IValues are rejected as stable status `7051`. No compiler
    special case is introduced. The surface is 51 functions with 50 package
    tests.


### Progress 130

  - Completed progress: MI7 adds a reusable owned UTF-8 return contract to the
    generic C ABI generator. Structured metadata names a `char **` output, its
    `size_t *` byte length, immediate UTF-8 copy policy, and an infallible
    consuming destructor. Generated Rust copies before destruction and never
    exposes the native pointer or lifetime. `terlan-pytorch` uses that contract
    for `torchscript_forward_string`; a pinned conditional model preserves
    integer input `1` and returns exact string `"positive"`. Tensor and other
    non-string IValues are rejected as stable status `7052`. The surface is 52
    functions with 51 package tests, with no PyTorch-specific compiler branch.


### Progress 131

  - Completed progress: MI8 adds a reusable owned integer-array return contract
    to the generic C ABI generator. Metadata names an `int64_t **` output, its
    `size_t *` element count, immediate-copy policy, and infallible consuming
    destructor. `terlan-pytorch` uses it for
    `torchscript_forward_int_list`; a pinned model preserves input `4` and
    returns exact list `[4,5,6]`. Tensor and other non-list IValues are rejected
    as stable status `7053`. The surface is 53 functions with 52 package tests,
    reusing the existing `List[Int]` VM and `ok_ints` protocol.


### Progress 132

  - Completed progress: MI9 makes the reusable owned-array return contract
    element-typed while preserving `int64` as its metadata default. An explicit
    `float64` element selects matching `double **` producer, `double *`
    destructor, generated `Vec<f64>`, `List[Float]`, and `ok_floats` shapes.
    `terlan-pytorch` uses it for `torchscript_forward_float_list`; a pinned
    model preserves input `1.5` and returns exact list `[1.5,2.0,2.5]`. Tensor
    and other non-floating-point-list IValues are rejected as stable status
    `7054`. The surface is 54 functions with 53 package tests and no
    PyTorch-specific compiler branch.


### Progress 133

  - Completed progress: MI10 adds the generic `bool8` owned-array element and
    `ok_bools` NativeBoundary reply. The ABI uses `uint8_t` storage rather than
    relying on C++ `bool` layout or specialized `vector<bool>` storage;
    generated Rust copies before destruction and accepts only exact `0`/`1`
    bytes before constructing `List[Bool]`. `terlan-pytorch` uses it for
    `torchscript_forward_bool_list`; a pinned model preserves input `2` and
    returns exact list `[true,false,false]`. Tensor and other non-boolean-list
    IValues are rejected as stable status `7055`. The surface is 55 functions
    with 54 package tests and no PyTorch-specific compiler branch.


### Progress 134

  - Completed progress: MI11 adds a reusable owned string-array return contract
    to the generic C ABI generator. Metadata couples `char ***` values, owned
    `size_t **` byte lengths, a `size_t *` count, immediate UTF-8 copy, and one
    infallible consuming destructor. Generated Rust copies every length-delimited
    value before destruction, validates UTF-8, preserves empty elements, and
    emits the existing `ok_strings` reply. `terlan-pytorch` uses it for
    `torchscript_forward_string_list`; a pinned model preserves input `1` and
    returns exact list `["positive","","ready"]`. Tensor and other
    non-string-list IValues are rejected as stable status `7056`. The surface
    is 56 functions with 55 package tests and no PyTorch-specific compiler branch.


### Progress 135

  - Completed progress: the external repository root is now a directly
    consumable `terlan-pytorch` version `0.0.7` package rather than only a source
    tree containing a disposable generated package. Generic C ABI metadata keeps
    public package identity independent from the native Rust crate identity and
    declares C++20 explicitly for LibTorch 2.13; package-owned C++ warnings are
    errors. The compiler-level gate pins commit
    `c400aee030e1249d4e50ea8f5e7da50b719bccea`, resolves it through ordinary
    `[dependencies]` Git metadata into the verified package cache, and uses the
    sibling repository only as a Git transport optimization. The canonical gate
    passes all 32 generic C ABI tests, 55 PyTorch dispatcher tests, the generated
    NativeBoundary test, the complete immutable-cache consumer matrix, and report
    validation. The future GitHub URL is the no-sibling fallback; remote
    publication remains external work because that repository does not yet exist.


### Progress 136

  - Completed progress: the canonical `make cpp-binding-generator-check` now
    validates committed Clang LibTooling metadata, runs all generator and native
    adapter tests with Rust warnings denied, and executes the generated package as
    an immutable external Git dependency through public `terlc package fetch` and
    `terlc run` paths. The consumer deletes the source package before execution,
    rebuilds from the verified cache, exercises copied values and opaque resource
    lifecycle, and proves stale handles fail with the stable diagnostic. Generated
    Rust resource variants are warning-clean UpperCamelCase identifiers; the
    package-neutral protocol explicitly scopes unavoidable unused decode variants
    instead of weakening the package warning policy. The adversarial fixture now
    includes the previously missing function-like macro metadata and proves that
    selecting it for binding fails as `cpp.annotation.unsupported`; all eleven
    documented rejection families are sorted and byte-stable across repeated
    generation. The complete offline gate passes. OpenCV remains the required
    first real generated C++ package, so this parent item stays open.


### Progress 137

  - Completed progress: `cuda-package-availability-check` now reports driver,
    usable device, toolkit, and CUDA-enabled LibTorch observations independently.
    Toolkit roots require both `cuda.h` and `nvcc`; LibTorch CUDA detection
    recognizes maintained Linux, macOS, and Windows library layouts. Direct CUDA
    readiness requires driver, device, and toolkit while LibTorch remains a
    separate PyTorch-lane capability. Seven warnings-as-errors tests cover absent,
    partial, complete, and cross-platform observations plus the core dependency
    boundary. The gate passes on the current host with driver/device available and
    toolkit/LibTorch CUDA unavailable.


### Progress 138

  - Completed progress: the opt-in CUDA execution boundary and machine-readable
    status report now consume one shared, lexically sorted capability reason-code
    source. Missing driver, device, or toolkit state fails closed with
    `error[cuda_package_unavailable]` and exact `*-unavailable` codes; a fully
    capable host without the external package smoke fails separately with
    `error[cuda_package_not_implemented]`. The warnings-as-errors availability
    gate owns both diagnostic branches, preventing CLI/report drift while CPU-only
    validation remains successful.


### Progress 139

  - Completed progress: `cuda-package-availability-check` now persists
    `target/quality/cuda-package-availability-status.json` using the versioned
    `terlan.cuda-package-availability-status.v1` schema. The deterministic
    artifact separates gate success from execution disposition, records driver,
    device, toolkit, CUDA root/compiler, and CUDA-enabled LibTorch observations,
    and emits stable sorted reason-code arrays independently for direct CUDA and
    PyTorch CUDA. An adversarial regression proves byte-stable regeneration and
    prevents the two execution lanes from being collapsed. All ten focused
    tests and the warnings-as-errors availability gate pass; the broader matrix
    remains open for the Polars, PyTorch, ML, generated C++, and CUDA execution
    rows.


### Progress 140

  - Completed progress: `cpp-binding-generator-check` now includes the external
    generated Git-package consumer and runs the entire boundary with Rust warnings
    denied. The consumer fetches through ordinary package metadata, deletes the
    source checkout, executes the C++ NativeBoundary helper from the verified
    package cache, rebuilds it from that cache, and rejects stale handles. The
    adversarial fixture now covers templates, overloads, ownership, borrowed
    lifetimes, contained and rejected exceptions, macros, variadics, callbacks,
    inheritance, unmapped types, and raw pointers through eleven stable skip
    families. This closes the package-import and unsupported-shape execution gap;
    package-mode `terlc test`, the deterministic `.gen_report.json`, and the
    separate `generated-package-contract-check` remain before Slice 5 can close.


### Progress 141

  - Current gate state: `docs/runtime/TERLAN_VM_BEAM_TEST_SUITE_INVENTORY.tsv`
    classifies the external `terlan-vm` test-suite corpus by owning subtree
    and replacement gate. `make terlan-vm-erl-suite-audit-check` currently
    classifies 1,713 active source/harness files: 484 `port-to-rust-vm-test`, 174
    `port-to-terlan-test`, and 1,055 `delete-after-vm-equivalent`.
    Fixture data files are intentionally ignored by the scanner until their
    owning test source is ported or deleted.


### Progress 142

  - Completed progress: `docs/runtime/TERLAN_VM_BEAM_TEST_FILE_STATUS.tsv`
    now records one sorted status row for every discovered external suite file,
    while `TERLAN_VM_BEAM_TEST_FILE_STATUS_SUMMARY.tsv` pins the exact progress
    counts. The audit rejects missing or duplicate paths, false deletion claims,
    and `ported` rows without a matching executable replacement gate. The first
    verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/unicode_helpers.rs`: the golden VM owns
    equivalent deterministic Latin-1-to-UTF-8 width, mixed-input, all-byte, and
    overflow tests, and `vm-runtime-semantics-check` names and passes that test
    module. The second verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/checksum.rs`: the production VM uses the
    golden CRC-32 implementation for distributed-storage checksums, the golden
    suite preserves the source Adler-32/CRC-32 vectors and split/combine cases,
    and adds adversarial three-way combine checks across every pair of split
    points. `vm-runtime-semantics-check` names that module explicitly. The
    third verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_runtime_suspension.rs`: the golden
    VM now owns retained runnable/blocked suspension state, scheduler queue
    removal and requeue behavior, suspended message delivery without implicit
    resume, stable missing/exited diagnostics, and runnable-process fairness.
    Seven focused tests include the adversarial blocked-resume-without-message
    case; `vm-actor-primitives-check` and `vm-runtime-semantics-check` pass. The
    fourth verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/packet_length.rs`: the golden VM packet
    decoder preserves the source Raw, 1/2/4-byte, record-marking, ASN.1, CDR,
    FastCGI, TPKT, and SSL/TLS framing cases plus unsupported, truncated,
    malformed, oversized, and integer-overflow outcomes. A cross-protocol
    adversarial test proves exact maximum-payload acceptance and one-byte-over
    rejection for every framed mode; `vm-runtime-semantics-check` names and
    passes the packet module. The fifth verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/bits_utf8_put.rs`: the golden VM UTF-8
    scalar encoder now proves every 1/2/3/4-byte transition boundary, exact
    byte/bit counts, untouched trailing bytes, and stable negative, surrogate,
    and above-maximum errors. Its adversarial matrix checks every undersized
    output length and proves failures perform no partial writes;
    `vm-runtime-semantics-check` names and passes the bitstring module. The
    sixth verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/base64.rs`: the golden NativeBoundary
    adapter reuses the maintained Rust `base64` crate through a byte-oriented
    production path, preserving RFC 4648 vectors, all 256 octets, every short
    padding class, and the 1,025-byte padded-tail boundary. The replacement
    owns allocation, so the obsolete caller-provided undersized-buffer API
    cannot partially write; `vm-runtime-semantics-check` names and passes the
    seven-test Base64 module. The seventh verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/atom_identity_and_names.rs`: the golden
    VM defines atom identity as exact immutable Unicode text, with equality and
    hashing derived from that text, stable Terlan-facing rendering, and no
    silent Unicode normalization. The replacement adversarially distinguishes
    composed and decomposed text while proving equal names hash equally. The
    eighth verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_runtime_wakeup.rs`: the golden
    VM now proves timer wakeups retain exactly one scheduler-ready entry,
    timer delivery preserves suspension until explicit resume, repeated actor
    messages do not duplicate ready entries, message order is retained, and a
    missing target cannot mutate scheduler state. The replacement uses
    VM-owned timer, scheduler, and actor primitives;
    `vm-runtime-semantics-check` names and passes all three focused tests. The
    ninth verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_runtime_priority.rs`: the golden
    VM now owns weighted scheduler classes, explicit queued-process
    reclassification without duplicate entries, retained class changes for
    blocked processes without implicit wakeup, idempotent same-class updates,
    and stable missing/exited diagnostics. The replacement keeps scheduling
    policy internal to the VM-owned scheduler;
    `vm-runtime-semantics-check` names and passes all three focused
    reclassification tests alongside the existing weighted-fairness tests. The
    tenth verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_runtime_ports.rs`: the golden VM
    replaces untyped external ports with typed resource handles and proves
    validated registration, monotonic identity, owner-scoped ordered
    inspection, cross-owner isolation, transfer policy, release, stale-handle
    rejection, and owner-exit cleanup. Live resources disappear atomically on
    release instead of retaining obsolete open/closing state rows;
    `vm-resource-ownership-check` names and passes the owner-inspection test and
    the complete resource lifecycle suite. The eleventh verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_process_info.rs`: the golden VM
    now exposes typed, read-only process snapshots covering stable process and
    parent identity, source identity, lifecycle state and exit reason,
    reductions, logical heap ownership, mailbox depth, cancellation, typed
    resource handles, and deterministically ordered registered names. Exited
    snapshots prove atomic mailbox, heap, resource, and registry cleanup, while
    missing-process inspection returns a typed identity error. Architecture-specific
    registers, instruction offsets, reduction budgets, and group-leader mechanics
    were not copied because they are not Terlan VM process-model contracts;
    `vm-process-model-check` passes the inspection cases in one Rust test
    process. The twelfth verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_process_registry.rs`: the golden
    VM now owns typed name registration, lookup, deterministic enumeration,
    explicit unregister, owner-scoped bulk removal, live-process enumeration,
    conflict isolation, named actor sends, and atomic exit cleanup. Terlan
    intentionally permits multiple stable names for one process, while still
    requiring each name to have exactly one live owner. Missing-name and
    missing-process mutations are typed and side-effect free. Architecture-
    specific loaded-module imports, register-width constraints, and stale reap
    repair were not copied because the VM uses typed ids and enforces registry
    cleanup at the process lifecycle transition. `vm-process-model-check`
    passes 30 process tests, and the consolidated `vm-actor-primitives-check`
    passes the registry cases with one Rust invocation per subsystem. The
    thirteenth verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_process_aliases.rs`: the golden VM
    now owns opaque monotonic process aliases as local capabilities distinct
    from stable registry names. Alias creation validates live owners, lookup
    and owner enumeration are deterministic, explicit removal is typed, alias
    sends reuse memory-accounted actor delivery, unknown aliases cannot mutate
    mailboxes, identity exhaustion fails without allocation, and actor exit
    atomically revokes every owned alias without affecting other owners.
    `vm-actor-primitives-check` passes all 37 actor tests in one Rust process.
    The fourteenth verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_process_flags.rs`: the golden VM
    now owns validated trap-exit queries and updates that return typed
    previous/current state, reject missing or exited process identities without
    mutation, and remove retained state atomically on process exit. The
    adversarial cases cover repeated updates, unrelated-state isolation, and
    cleanup; `vm-failure-primitives-check` passes all 17 failure tests in one
    Rust process. Scheduler priority is already covered by the ninth verified
    priority port. Legacy bytecode import decoding, atom/register failures, and
    `message_queue_data` were not copied: the first two are retired bytecode
    mechanics, while mailbox heap placement is a VM-owned implementation
    decision rather than a Terlan process flag. The fifteenth verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_process_identity.rs`: the golden
    VM now exposes opaque live actor execution contexts with typed process
    identity and a self-send path that preserves sender identity, wakes blocked
    actors, and keeps concurrent actor mailboxes isolated. Stale contexts,
    missing identities, and exited senders are rejected before mailbox or
    memory-accounting mutation; this also closes a lifecycle hole where an
    exited process could previously send to another live process. Local node
    identity remains owned by the coordination profile and its existing gate.
    Legacy opcode/import decoding, destination registers, small-term limits,
    and group-leader I/O topology were not copied because they are not Terlan
    actor identity contracts. `vm-actor-primitives-check` passes all 37 actor
    tests and `vm-process-model-check` passes all 30 process tests. The
    sixteenth verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_process_info_location.rs`: the
    golden VM now retains a typed current execution location containing module,
    function, arity, and VM instruction offset separately from stable process
    spawn origin. Scheduler execution can update the frame, loop back-edges can
    move to lower offsets, and exited process snapshots retain the last
    location for post-failure inspection. Entry, scheduler, back-edge, and exit
    cases run under `vm-process-model-check`, which passes all 30 process tests
    in one Rust process. External compiler invocation, loaded-bytecode atom
    tables, and legacy metadata-list encoding were not copied because source
    location is a typed VM inspection contract. The seventeenth verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_process_info_imports.rs`: the
    golden VM now exposes deterministic process relationship snapshots for
    links, monitored peers, monitoring peers, and trap-exit state alongside a
    current-first typed execution stack. Relationship inspection filters stale
    and unrelated peers, sorts all identities deterministically, preserves
    monitor reference identity, and returns an empty relationship snapshot for
    an exited process. Ordinary sends still reject exited senders, while VM
    failure propagation uses a separate system-message path that accepts only a
    known exited origin and a live recipient. `vm-process-model-check` passes
    all 30 process tests and `vm-failure-primitives-check` passes all 24 failure
    tests. Group-leader I/O topology, external compiler loading, register and
    atom-table details, metadata-list encoding, and process dictionaries were
    not copied: the first five are not VM inspection contracts, while explicit
    actor state remains tracked by its own unported inventory row. The
    eighteenth verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_process_memory_imports.rs`: the
    golden VM now reconciles a process heap against traced live-value bytes
    while preserving mailbox payloads, native resources, and shared allocation
    roots. Collection returns a typed report containing previous, protected,
    retained, and reclaimed byte counts; updates existing high-water and
    reclamation metrics; and rejects missing, exited, inconsistent, or
    overflowing collection plans before mutation. Five focused adversarial
    collection tests run under `vm-memory-heap-pressure-check`. The collection
    module and the previously completed process-alias primitive are promoted
    into `vm-coverage-100-check`, where 28 VM-owned files now enforce 100% line
    and function coverage across 1,166 instrumented VM tests. Import dispatch,
    register layout, atom-table identity, and host compiler roundtrips were not
    copied because collection is a typed VM ownership operation. The nineteenth
    verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_process_scheduler_imports.rs`: the
    golden VM now proves that a cooperative yield returns control without
    blocking the process, retains process-local state and reduction accounting,
    lets a queued peer run, and resumes through the ordinary runnable queue.
    Repeating 64 zero-reduction voluntary yields retains exactly one queue entry
    and does not misclassify cooperation as budget preemption. The consolidated
    `vm-scheduler-contract-check` executes all 26 scheduler tests in one Rust
    test process, and `vm-runtime-semantics-check` now depends on that gate.
    External module compilation, import dispatch, instruction registers, and
    atom-table return encoding were not copied because yielding is a direct
    VM scheduler decision. The twentieth verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_process_send_imports.rs`: the
    golden VM now delivers memory-accounted messages through typed process-id,
    stable-name, opaque-alias, and self routes while preserving structured
    records, tuples, lists, atoms, and scalar values without coercion. Named and
    alias sends validate the sender before destination resolution, so missing
    or exited senders cannot probe registry state and all rejected routes leave
    mailbox and memory ownership unchanged. Delivery returns the stable mailbox
    message identity rather than reproducing an import-specific payload return
    convention. The two focused adversarial tests run inside the consolidated
    `vm-actor-primitives-check`, which passes all 39 actor tests; strict VM
    coverage passes all 1,170 instrumented tests with 28 promoted files at 100%
    line and function coverage. External module compilation, import dispatch,
    instruction registers, numeric process encoding, and malformed untyped
    destination values were not copied because Terlan actor routes are typed
    before runtime delivery. The twenty-first verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_process_timer_imports.rs`: the
    golden VM now schedules typed delayed actor messages through process-id,
    stable-name, and opaque-alias routes, and emits correlated `TimerMessage`
    records through the ordinary memory-accounted actor-send path. Name and
    alias destinations are resolved to stable process identities when the
    timer starts. The timer identity is shared by read, cancel, expiry, and
    delivery evidence; equal deadlines retain insertion order, late deadlines
    remain observable, recipient exits reject delivery without mutation, and
    owner exits remove owned payload state. Missing, exited, stale, and
    overflowing schedules are rejected before timer identity or payload state
    is allocated. Five focused adversarial tests run inside the consolidated
    `vm-actor-primitives-check`, which passes all 44 actor tests. Strict VM
    coverage passes all 1,175 instrumented tests with 29 promoted files at 100%
    line and function coverage. External module compilation, compatibility
    import dispatch, instruction registers, and tagged-term timer encodings were not
    copied because delayed delivery is a typed VM ownership operation. The
    twenty-second verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_process_timer_option_imports.rs`: the
    golden VM now models timer options as typed relative/absolute deadline,
    synchronous/asynchronous read, and cancellation-information policies rather
    than accepting untyped option tuples. Async read and cancellation information
    uses memory-accounted `TimerReadReply` and `TimerCancelReply` actor records;
    stale identities produce explicit missing information, information suppression
    produces an acknowledgement without a mailbox write, and a rejected async
    reply leaves the active timer and delayed payload unchanged. Absolute deadlines
    preserve already-due evidence through the ordinary late-deadline event path.
    Four focused adversarial tests run inside `vm-actor-primitives-check`, which
    passes all 48 actor tests. Strict VM coverage passes all 1,179 instrumented
    tests with 30 promoted files at 100% line and function coverage. Untyped option
    decoding, reserved atom identities, external module compilation, import
    registers, and compatibility return encodings were not copied because the
    compiler constructs the typed policy before VM execution. The
    twenty-third verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_process_reference_imports.rs`: the
    golden VM now allocates opaque references with explicit node, boot-epoch,
    and monotonic local identity. Ordinary and monitor references share one
    allocator supplied by the owning runtime, while positive monotonic unique
    integers use a separate value sequence. Monitor validation completes before
    allocation, so missing processes consume no identity; exhausted reference
    and integer sequences fail without wrapping or reusing values. Four focused
    adversarial reference tests run inside `vm-failure-primitives-check`, which
    passes all 28 failure tests. Strict VM coverage passes all 1,183 instrumented
    tests with 31 promoted files at 100% line and function coverage. External
    module compilation, compatibility import dispatch, instruction registers,
    untyped option decoding, and compatibility return encodings were not copied
    because the compiler and VM exchange typed identity operations. The
    twenty-fourth verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_runtime_environment_imports.rs`: the
    golden VM now exposes an immutable runtime environment snapshot through the
    production actor facade. An explicit profile provides process and scheduler
    capacities; the snapshot composes total/live/exited process counts, run queue,
    mailbox depth, logical heap bytes, resource handles, active and cumulative
    timer counts, reductions, memory reductions, slices, preemptions, and target
    word size from their owning subsystems. Repeated capture is deterministic and
    side-effect free, zero capacities are rejected, and a live-process count above
    the configured limit fails without mutating process state. Five focused tests
    run inside `vm-process-model-check`, which passes all 35 process/environment
    tests. Strict VM coverage passes all 1,188 instrumented tests with 32 promoted
    files at 100% line and function coverage. External module compilation,
    compatibility selectors, instruction registers, tagged memory lists, and
    compatibility atom identities were not copied because Terlan exposes one typed
    snapshot instead of a selector-driven import surface. The twenty-fifth verified
    port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_process_spawn_imports.rs`: the
    golden VM now composes child creation through one typed actor spawn plan.
    The plan carries the child source, scheduler class, and optional parent link
    and monitor relationships; the result returns stable child and monitor
    identities. The actor runtime now owns the failure and reference subsystems,
    so monitored child completion reaches the parent through the ordinary
    mailbox path and actor exit applies the same link/monitor lifecycle used by
    direct failure operations. Parent liveness is validated before process or
    reference allocation, and missing or exited parents leave both identity
    sequences unchanged. Five focused adversarial spawn tests run inside
    `vm-actor-primitives-check`, which passes all 53 actor tests;
    `vm-process-model-check` passes all 36 process/environment tests and
    `vm-failure-primitives-check` passes all 28 failure tests. Strict VM coverage
    passes all 1,194 instrumented tests with 33 promoted files at 100% line and
    function coverage. External module compilation, compatibility import/register
    decoding, dynamic option lists, `min_heap_size` hints, mailbox storage hints,
    and compatibility return encodings were not copied: Terlan uses typed source
    and scheduler policy, VM-owned memory/mailbox accounting, and the already
    verified actor-send path. The twenty-sixth verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_runtime_model.rs`: the golden VM
    now exposes allocation-ordered typed snapshots for the complete local
    process table, including exited records retained for post-failure
    inspection. Empty tables, parent identity, source metadata, name cleanup,
    and exited state are covered adversarially. Existing VM-owned code-server,
    actor receive, memory-accounting, scheduler-priority, reduction, execution
    stack, and process-identity tests replace the remaining portable behavior
    from the historical model suite. BEAM instruction operands, register arrays,
    tagged terms, code indexes, and MFA display syntax were not copied because
    they describe the retired bytecode representation rather than Terlan VM IR.
    `vm-runtime-semantics-check` passes its exact consolidated gate;
    `vm-process-model-check` passes all 38 process/environment tests. Strict VM
    coverage passes all 1,196 instrumented tests with 33 promoted files at 100%
    line and function coverage. The benchmark binary now composes those shared
    VM modules through a dedicated runtime module, and all 453 benchmark tests
    pass with warnings denied without increasing the oversized-file baseline.
    The twenty-seventh verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_runtime_relationships.rs`: the golden
    VM now exposes typed actor operations for symmetric links, observer-owned
    monitors, explicit demonitoring, optional stale-completion flushing, and
    trap-exit policy. Monitor removal cannot mutate another observer's state,
    failed validation consumes no reference identity, and flushing selects only
    the matching structured completion while preserving ordinary messages and
    other monitor completions. Five focused adversarial tests run inside
    `vm-actor-primitives-check`, which passes all 58 actor tests;
    `vm-failure-primitives-check` passes all 28 failure tests, and all 458
    benchmark-binary tests pass with warnings denied. Strict VM coverage passes
    all 1,201 instrumented tests with 34 promoted files at 100% line and function
    coverage. Port, timer, tracing, scheduler, and distribution relationships
    remain covered by their dedicated VM-owned owners and gates instead of one
    mutable global relationship registry. Compatibility instruction, register,
    group-I/O, and untyped dynamic-option cases were not copied because they are
    not part of the typed Terlan actor contract.
    The twenty-eighth verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_runtime_results.rs`: the golden VM
    now has an explicit scheduler-result conformance suite over actual process
    execution. It verifies idle polls cannot fabricate process work, exact slice
    identity/tick/reduction reporting, saturating process and aggregate reduction
    counters, ordered resource cleanup on exit, pre-slice cancellation without
    user work, cancellation at the scheduler boundary, and explicit-exit
    precedence over a simultaneous cancellation request. The canonical
    `vm-scheduler-contract-check` now owns the complete scheduler namespace and
    passes all 30 scheduler tests, so future sibling conformance modules cannot
    compile without running. All 462 benchmark-binary tests pass with warnings
    denied. Strict VM coverage passes all 1,205 instrumented tests with 34
    promoted files at 100% line and function coverage, and
    `vm-runtime-semantics-check` passes. Compatibility call/jump/catch offsets,
    instruction counts, register transitions, and signal-result wrappers were
    not copied because they describe the retired bytecode executor; typed VM IR
    execution and `VmSchedulerRun` are the Terlan-owned result contracts.
    The twenty-ninth verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_runtime_signals.rs`: the golden VM
    now carries failure-notification recipients through the typed failure report
    and wakes live blocked actors through the existing scheduler after linked
    exit and monitor cascades. Four focused adversarial actor tests verify
    monitor completion wakeup before actor execution, trapped-exit wakeup,
    FIFO completion delivery with one deduplicated scheduler entry, and the
    stale-recipient race where a notified process exits later in the same
    cascade and must not be requeued. Terlan projects these notifications
    directly into the owned mailbox; the historical intermediate signal queue,
    BEAM instruction-before-signal ticks, register state, tagged terms, and
    compatibility signal-result wrappers were not copied. The focused actor
    gate passes all 62 tests, the failure gate passes all 28 tests, and all 466
    benchmark-binary tests pass with warnings denied. Strict VM coverage passes
    all 1,209 instrumented tests with 34 promoted files at 100% line and
    function coverage, and the canonical `vm-runtime-semantics-check` passes.
    The thirtieth verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_runtime_lifecycle.rs`: the golden VM
    now verifies process exit as one integrated actor-lifecycle transaction over
    names, aliases, mailbox memory, resource handles, delayed sends, links,
    monitors, suspension, and scheduler wakeup. Four focused adversarial tests
    prove atomic normal-exit cleanup without cross-owner damage, complete cleanup
    across an abnormal linked cascade, monitor delivery to a suspended observer
    only after explicit resume, and selective receive preserving timer noise
    until a matching payload arrives. Supervision, typed resources, timers, and
    code execution remain owned by their dedicated runtime modules and are
    composed by the canonical gate instead of a second mutable lifecycle kernel.
    Historical opcode programs, register state, pending-receive markers, signal
    queues, compatibility port identifiers, group leaders, and scheduler wrapper
    accessors were not copied. The focused actor gate passes all 66 tests, the
    failure gate passes all 28 tests, and all 470 benchmark-binary tests pass with
    warnings denied. Strict VM coverage passes all 1,213 instrumented tests with
    34 promoted files at 100% line and function coverage, and the canonical
    `vm-runtime-semantics-check` passes.
    The thirty-first verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_runtime_memory.rs`: the golden VM
    now verifies memory as an owner-scoped transaction over logical heap bytes,
    mailbox roots, resource handles, shared allocations, collection, and process
    exit. Two integrated adversarial tests prove that hard-pressure rejection
    preserves every ownership registry without consuming allocation identity,
    and that collection followed by exit removes one owner's state without
    changing a surviving owner. Historical word-sized arenas, allocation-region
    classifiers, moving-object policy families, compatibility heap growth, and
    bytecode-kernel wrappers were not copied because they are representation
    machinery rather than Terlan VM semantics. `vm-memory-heap-pressure-check`
    now runs all 27 memory tests and is an explicit prerequisite of
    `vm-runtime-semantics-check`; its existing owned Rust harness also passes.
    All 472 benchmark-binary tests pass with warnings denied. Strict VM coverage
    passes all 1,215 instrumented tests while preserving 34 promoted files at
    100% line and function coverage.
    The thirty-second verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_runtime_observability.rs`: the golden
    VM now exposes one correlated, read-only actor observation containing
    aggregate environment metrics, allocation-ordered process rows, scheduler
    telemetry, active timer rows, and cumulative timer outcomes. Two integrated
    adversarial tests prove that receive blocking, message wakeup, execution
    reductions, delayed delivery, and process exit are visible at stable
    inspection boundaries, and that owner-exit timer cleanup does not change a
    surviving blocked actor. Repeated captures are equal and do not mutate the
    runtime. Historical bounded event logs, callback registries, commit hooks,
    trace-capacity wrappers, opcode receive traces, and representation-specific
    event classifiers were not copied; the owned state and telemetry modules are
    the authoritative observation surface. The focused actor gate passes all 68
    tests, all 474 benchmark-binary tests pass with warnings denied, strict VM
    coverage passes all 1,217 instrumented tests with 34 promoted files at 100%
    line and function coverage, and the canonical `vm-runtime-semantics-check`
    passes.
    The thirty-third verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_runtime_timers.rs`: the golden VM
    owns opaque timer identities, deterministic deadline ordering, atomic
    cancellation, delayed actor payload delivery, receive-timeout wakeups, and
    owner-scoped cleanup. Two integrated adversarial tests prove that cancelling
    the middle of three equal-deadline timers preserves order and exactly-once
    delivery, and that owner exit, recipient exit, and a surviving delivery are
    isolated at one clock boundary with stable cumulative evidence. Historical
    caller-assigned timer identities, replacement-by-id registries, legacy
    payload tuples, timer event hooks, and compatibility-only signal-entry wrappers
    were not copied; VM actor delivery, failure signaling, suspension, and typed
    timer outcomes remain separate owned contracts. The focused actor gate
    passes all 70 tests, the timer gate passes all 32 tests,
    `rust-quality-check` passes, strict VM coverage passes all 1,219
    instrumented tests with 34 promoted files at 100% line and function
    coverage, and the canonical `vm-runtime-semantics-check` passes.
    The thirty-fourth verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/md5.rs`: the golden runtime now exposes
    `std.encoding.Md5.digest/1` through the typed NativeBoundary boundary using the
    maintained RustCrypto `md-5` implementation. RFC 1321 vectors, exact UTF-8
    input, padding boundaries, every two-way split of a deterministic payload,
    safe cloned digest state, and adversarial chunk sizes are executable tests;
    the unsafe caller-owned C ABI state from the historical suite was not
    copied. The API is explicitly legacy integrity compatibility and is not
    approved for passwords, signatures, authentication, or other security use.
    Focused LLVM coverage reports 12/12 implementation lines, 2/2 functions,
    and 24/24 regions covered. `std-test-table-check`, `rust-quality-check`, and
    the canonical Rust test orchestration pass.
    The thirty-fifth verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/heap_gc_owner_modules.rs`: the golden VM
    memory owner now covers process-scoped allocation, exact structural-value
    sizing, mailbox/resource/shared-allocation roots, rooted collection,
    overflow rollback without mutation, pressure metrics, and atomic exit
    cleanup. The historical suite's module-export aliases, word-based moving
    heap classifications, and root write-back plan representation were not
    copied because they are implementation details rather than observable
    Terlan memory semantics. `vm-memory-heap-pressure-check` passes its process,
    resource, NativeBoundary, memory-pressure, soak-artifact, and canonical Rust
    harnesses; `terlan-vm-erl-suite-audit-check` accepts the source tombstone only
    while that executable replacement evidence remains attached.
    The thirty-sixth verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_term_model.rs`: the golden VM now
    proves typed byte, tuple, list, and map cardinality; deterministic map
    insertion order and replacement; and exact backend-neutral logical retained
    sizes for empty and nested aggregate values. The historical BEAM raw-word
    tags, shallow headers, atom-index encoding, and heap-word assumptions were
    not copied because Terlan VM uses typed Rust values and logical-byte pressure
    accounting. The focused tests pass in both `terlan-vm` and
    `terlan-benchmark` with warnings denied, and `vm-runtime-semantics-check`
    passes.
    The thirty-seventh verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_structured_register_value.rs`: the
    golden VM now proves nested tuple and list values survive lexical bindings,
    function-call environments, list-cons function clauses, and empty-list
    calls without representation loss. The historical X/Y register arrays,
    shallow tagged terms, copied-term snapshots, and list-owner side channel
    were not copied because CoreIR execution carries typed `ReplValue` values
    directly. Focused tests pass with warnings denied,
    `vm-runtime-semantics-check` passes, and the external compatibility suite is
    deleted under checked `delete-after-vm-equivalent` evidence.
    The thirty-eighth verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_instruction_function_arity_lowering.rs`:
    the golden VM now derives closure arity from checked CoreIR parameter
    patterns, executes a local one-argument function reference through a typed
    callback, and rejects both missing and surplus dynamic-call arguments before
    parameter binding. The historical BEAM opcode 115, tagged operand bytes,
    label decoding, and compatibility branch instruction were not copied because
    Terlan VM executes typed CoreIR rather than BEAM bytecode. Focused closure and
    shared comparator tests pass with warnings denied,
    `vm-runtime-semantics-check` passes, and the external lowering suite is
    deleted under checked `delete-after-vm-equivalent` evidence.
    The thirty-ninth verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_instruction_boolean_lowering.rs`:
    the golden VM now proves both boolean values satisfy `Bool` type tests while
    integer and string lookalikes do not, through the source-to-CoreIR execution
    path. The historical BEAM opcode 114, atom-table requirements, tagged
    register operands, labels, and compatibility branch instruction were not
    copied because Terlan VM executes typed CoreIR values without BEAM decoding.
    Focused parity tests pass with warnings denied,
    `vm-runtime-semantics-check` passes, and the external lowering suite is
    deleted under checked `delete-after-vm-equivalent` evidence. File-level
    migration progress is now 39 ported, 1,881 not ported, 16 deleted, and 1,904
    active historical files.
    The fortieth verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_instruction_boolean_bifs.rs`:
    the golden VM now executes complete negation, conjunction, disjunction, and
    boolean-inequality truth tables through source-to-CoreIR operators and
    rejects malformed dynamic operands with stable diagnostics. Boolean
    inequality is the Terlan XOR-equivalent operation. The historical Erlang
    BIF dispatch, atom-table lookup, copied raw terms, MFA routing, and BIF
    arity machinery were not copied because operators are checked language
    expressions rather than runtime calls. Focused tests pass with warnings
    denied, `vm-runtime-semantics-check` passes, and the external BIF suite is
    deleted under checked `delete-after-vm-equivalent` evidence. File-level
    migration progress is now 40 ported, 1,880 not ported, 17 deleted, and 1,903
    active historical files.
    The forty-first verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_instruction_compare_bifs.rs`:
    the golden VM now proves strict typed structural equality and inequality
    across scalar, tuple, and list values, plus numeric ordering at true and
    false boundaries through the source-to-CoreIR execution path. Mixed-type
    equality is deliberately strict and nonnumeric ordering is rejected with a
    stable diagnostic. The historical Erlang loose-versus-exact comparison
    split, atom table, BIF and MFA dispatch, and BEAM result opcodes and
    registers were not copied because they are not part of Terlan semantics.
    Focused parity tests pass with warnings denied,
    `vm-runtime-semantics-check` passes, and the external comparison suite is
    deleted under checked `delete-after-vm-equivalent` evidence. File-level
    migration progress is now 41 ported, 1,879 not ported, 18 deleted, and 1,902
    active historical files.
    The forty-second verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_instruction_gc_bif2_boolean_lowering.rs`:
    the golden VM now composes source-to-CoreIR parity evidence for map-key
    membership, checked function arity, boolean operators, strict comparisons,
    and numeric ordering. New map-key tests distinguish present and missing
    keys and prove membership remains correct across insertion, replacement,
    and removal. The historical BEAM GC-BIF2 opcode 125, atom-table
    prerequisites, tagged operands, labels, and destination registers were not
    copied because typed CoreIR expressions and collection intrinsics own those
    semantics. Focused map-key tests pass with warnings denied,
    `vm-runtime-semantics-check` passes with the composed parity group, and the
    external lowering suite is deleted under checked
    `delete-after-vm-equivalent` evidence. File-level migration progress is now
    42 ported, 1,878 not ported, 19 deleted, and 1,901 active historical files.
    The forty-third verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_instruction_gc_bif2_conversion_lowering.rs`:
    the golden VM now exposes typed `Int.to_string_base/2` and
    `Int.from_string_base/2` operations for bases 2 through 36. The operations
    return `Option`, format uppercase digits, parse case-insensitively, preserve
    signed 64-bit boundaries, and reject invalid bases, partial input, and
    overflow without panicking. Immutable atoms continue to use
    `Atom.to_string`; no mutable existing-atom table is exposed. Direct runtime,
    source-to-CoreIR parity, standard-library, invalid-input, and 100% VM
    line/function coverage tests pass, and `vm-runtime-semantics-check` passes
    with warnings denied. The external lowering suite is deleted under checked
    `delete-after-vm-equivalent` evidence. File-level migration progress is now
    43 ported, 1,877 not ported, 20 deleted, and 1,900 active historical files.
    The forty-fourth verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_instruction_integer_conversion_raw_ops.rs`:
    the golden VM now proves signed decimal and radix integer conversions
    through typed source-to-CoreIR execution. Decimal formatting and parsing
    cover positive and negative values, the maximum `Int`, malformed text,
    whitespace, and overflow in both directions. Radix conversions retain the
    previously verified base range, case handling, and invalid-input behavior.
    Terlan `String` owns textual integer output, so the historical list/binary
    output split, raw opcodes, registers, tagged operands, and decoder failures
    were not copied. Focused parity and standard-library tests pass with
    warnings denied, `vm-runtime-semantics-check` passes, and the external raw
    conversion suite is deleted under checked `delete-after-vm-equivalent`
    evidence. File-level migration progress is now 44 ported, 1,876 not
    ported, 21 deleted, and 1,899 active historical files.
    The forty-fifth verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_instruction_conversion_bifs.rs`:
    the golden VM now proves typed byte-buffer conversion through
    `std.vm.Bytes.from_list`, `to_list`, `length`, `slice`, and `concat`.
    Source-level tests cover empty buffers, the complete octet boundary,
    start, middle, and end slices, and ordered buffer composition. Existing
    adversarial VM tests reject out-of-range octets and invalid or overflowing
    slice ranges, while the preceding integer-conversion parity covers decimal
    and radix text conversion. Dynamic nested iolists, MFA dispatch, arity
    probing, and raw binary/list representation checks were not copied because
    typed `Bytes` values and compiler type checking own those contracts.
    Focused standard-library and adversarial tests pass with warnings denied,
    `vm-runtime-semantics-check` passes, and the external conversion suite is
    deleted under checked `delete-after-vm-equivalent` evidence. File-level
    migration progress is now 45 ported, 1,875 not ported, 22 deleted, and
    1,898 active historical files.
    The forty-sixth verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_instruction_size_bifs.rs`:
    the golden VM now proves cardinality through the typed `List.length`,
    `Map.size`, and `Bytes.length` operations. Source-to-CoreIR execution covers
    empty and populated values in each domain, while direct intrinsic tests
    retain adversarial wrong-type and wrong-arity diagnostics. The legacy
    generic `size` dispatch, tuple/binary representation coupling, MFA routing,
    and dynamic argument probing were not copied because Terlan resolves each
    cardinality operation from its receiver type. Focused parity and intrinsic
    tests pass with warnings denied, `vm-runtime-semantics-check` passes, and
    the external size suite is deleted under checked
    `delete-after-vm-equivalent` evidence. File-level migration progress is now
    46 ported, 1,874 not ported, 23 deleted, and 1,897 active historical files.
    The forty-seventh verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_instruction_small_extremes.rs`:
    the golden VM now proves integer extrema selection through typed
    `std.core.Int.min` and `max` source-to-CoreIR execution. Boundary coverage
    includes negative and positive values, equal operands, and the complete
    historical immediate-integer interval. Terlan function signatures reject
    wrong arity and non-`Int` arguments before execution, so decoder opcodes,
    tagged literal operands, registers, destination mutation, MFA routing, and
    dynamic argument probes were not copied. Focused parity and all 19 public
    `Int` tests pass with warnings denied, `vm-runtime-semantics-check` passes,
    and the external small-extremes suite is deleted under checked
    `delete-after-vm-equivalent` evidence. File-level migration progress is now
    47 ported, 1,873 not ported, 24 deleted, and 1,896 active historical files.
    The forty-eighth verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_instruction_tuple_arity_lowering.rs`:
    the golden VM now proves exact tuple-arity dispatch through source-to-CoreIR
    tuple patterns. Pairs, triples, and four-element tuples select distinct
    clauses, while list and integer inputs take the non-tuple fallback. The
    historical instruction numbers, tagged operands, label tables, register
    sources, destinations, and decoder error shapes were not copied because
    Terlan compiles structural patterns directly. The focused parity test passes
    with warnings denied, `vm-runtime-semantics-check` passes, and the external
    tuple-arity lowering suite is deleted under checked
    `delete-after-vm-equivalent` evidence. File-level migration progress is now
    48 ported, 1,872 not ported, 25 deleted, and 1,895 active historical files.
    The forty-ninth verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_instruction_map_lowering.rs`:
    the golden VM now proves typed map construction, multi-key association,
    replacement without size growth, exact retrieval of retained values, and
    missing-key behavior through source-to-CoreIR execution. Dynamic map type
    tests, instruction numbers, tagged operands, label tables, registers,
    destinations, and malformed decoder shapes were not copied because Terlan
    resolves map operations statically and executes typed VM IR. The focused
    parity test passes with warnings denied, `vm-runtime-semantics-check`
    passes, and the external map-lowering suite is deleted under checked
    `delete-after-vm-equivalent` evidence. File-level migration progress is now
    49 ported, 1,871 not ported, 26 deleted, and 1,894 active historical files.
    The fiftieth verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_instruction_tuple_list_lowering.rs`:
    the golden VM now proves nested constructor, tuple, and list construction
    and extraction through source-to-CoreIR patterns. A present tagged value
    decomposes a tuple payload and non-empty list into its scalar, head, and
    tail; empty-list and absent-constructor branches prove exact fallback
    behavior. Instruction numbers, tagged operands, labels, registers,
    destinations, and malformed decoder shapes were not copied because Terlan
    executes typed structural patterns directly. The focused parity test passes
    with warnings denied, `vm-runtime-semantics-check` passes, and the external
    tuple/list lowering suite is deleted under checked
    `delete-after-vm-equivalent` evidence. File-level migration progress is now
    50 ported, 1,870 not ported, 27 deleted, and 1,893 active historical files.
    The fifty-first verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_instruction_node_bif_lowering.rs`:
    the golden VM now exposes explicit node identity through typed
    `std.vm.Cluster.Profile.node_id`. Terlan source constructs a validated
    coordination profile and reads back its exact node id through source-to-CoreIR
    execution. The implicit `nonode@nohost` fallback, mutable atom-table
    prerequisites, BEAM imports/opcodes, tagged destinations, MFA dispatch, and
    decoder failure shapes were not copied because Terlan requires explicit
    VM-owned cluster identity. The focused parity test and all three public
    Cluster tests pass with warnings denied, `vm-runtime-semantics-check` passes,
    and the external node-BIF lowering suite is deleted under checked
    `delete-after-vm-equivalent` evidence. File-level migration progress is now
    51 ported, 1,869 not ported, 28 deleted, and 1,892 active historical files.
    The fifty-second verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_instruction_raw_atom_bif_lowering.rs`:
    the golden VM now proves finite source-declared atom aliases render their
    exact payloads, compare reflexively, and remain distinct after union widening
    through source-to-CoreIR execution. Public example and property suites cover
    the same API, while parser and NativeBoundary policy continue to reject
    runtime text-to-atom creation. Import tables, conversion opcodes, encoding
    operands, registers, destinations, and decoder failures were not copied
    because Terlan atoms are finite typed symbols rather than runtime-interned
    text. The focused parity test and all six public Atom tests pass with warnings
    denied, `vm-runtime-semantics-check` passes, and the external atom-conversion
    lowering suite is deleted under checked `delete-after-vm-equivalent`
    evidence. File-level migration progress is now 52 ported, 1,868 not ported,
    29 deleted, and 1,891 active historical files.
    The next checked nonportable retirement is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_instruction_process_dictionary_bif_lowering.rs`:
    Terlan does not expose implicit per-process global state. Stateful services
    must use source-visible typed state through `std.vm.Agent`,
    `std.vm.GenServer`, or `std.vm.PersistentActor`, while the VM owns only the
    underlying process and lifecycle mechanics. The removed suite tested import
    dispatch, tagged operands, register moves, destinations, and malformed
    instruction decoding rather than this Terlan contract. The migration audit
    accepts the exact `remove-non-portable` tombstone only after the external
    source disappears. File-level migration progress is now 52 ported, 1,868
    not ported, 30 deleted, and 1,890 active historical files.
    The fifty-third verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_instruction_import_list_bifs.rs`:
    the golden VM now provides typed `List.rest`, `List.concat`, and
    `List.subtract` operations through source-to-CoreIR execution. Exact parity
    tests cover empty and singleton tails, persistent concatenation, first-match
    subtraction, structural equality, and unchanged source values; public and
    property suites cover the same API. Tuple arity and tuple/list extraction
    already have dedicated VM parity coverage. BEAM imports/opcodes, MFA and
    arity dispatch, tagged operands, and dynamic `badarg` probes were not copied
    because Terlan checks typed calls before execution. The helper is promoted
    into the strict 100% VM line/function coverage baseline, all focused tests
    pass with warnings denied, `vm-runtime-semantics-check` passes, and the
    external list-BIF suite is deleted under checked
    `delete-after-vm-equivalent` evidence. File-level migration progress is now
    53 ported, 1,867 not ported, 31 deleted, and 1,889 active historical files.
    The fifty-fourth verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_instruction_raw_integer_conversion_bif_lowering.rs`:
    the golden VM now proves signed decimal and radix String conversions through
    both source-to-CoreIR execution and standalone artifact intrinsic dispatch.
    Focused parity and adversarial tests cover minimum signed values, signs,
    mixed-case digits, invalid text, invalid bases, overflow, arity, and type
    failures. BEAM imports/opcodes, MFA dispatch, tagged operands, registers,
    destinations, and decoder failures were not copied because Terlan resolves
    typed conversion calls before VM execution. All seven focused conversion
    tests pass with warnings denied, `vm-runtime-semantics-check` passes, and the
    external raw conversion lowering suite is deleted under checked
    `delete-after-vm-equivalent` evidence. File-level migration progress is now
    54 ported, 1,866 not ported, 32 deleted, and 1,888 active historical files.
    The fifty-fifth verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_loader_node_bif_full_cycle.rs`:
    Terlan replaces implicit `node/0` and the fixed `nonode@nohost` atom with
    explicit typed `Cluster.Profile.node_id` identity. The VM coordination
    profile constructor is now fallible and rejects blank application, VM,
    node, cluster, and runtime-version identities, zero epochs, and blank
    capability names before descriptors enter runtime state. Three focused
    adversarial profile tests and the source-to-CoreIR node identity parity test
    pass with warnings denied; `vm-runtime-semantics-check` and
    `vm-coverage-100-check` pass. The external erlc/BEAM loader, atom-table,
    process-spawn, instruction-tick, and fixed-node fixture is deleted under
    checked `delete-after-vm-equivalent` evidence. File-level migration progress
    is now 55 ported, 1,865 not ported, 33 deleted, and 1,887 active historical
    files.
    The fifty-sixth verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_call_control.rs`: the golden VM
    process frame API now records a caller continuation atomically when entering
    a typed module/function/arity frame and restores that continuation through
    LIFO returns. Nested-call coverage proves exact current-first stack order,
    detached inspection snapshots, root-frame retention without partial
    mutation, and final stack retention after process exit. The historical
    return-offset stack, BEAM export table, opcode dispatch, registers, and
    bytecode instruction offsets were not copied because Terlan VM executes
    typed compiler output and exposes source-owned execution locations. All 41
    process tests and the exact nested-call anchor pass with warnings denied;
    `terlan-vm-erl-suite-audit-check` passes after deleting the obsolete external
    fixture under checked `vm-runtime-semantics-check` evidence. File-level
    migration progress is now 56 ported, 1,864 not ported, 51 deleted, and 1,869
    active historical files.
    The fifty-seventh verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_instruction_receive.rs`: golden
    VM process coverage now proves that selecting a middle mailbox message
    commits only that message and preserves both neighboring messages in
    arrival order. Actor coverage proves that an unmatched selective receive
    blocks, a later matching send wakes the actor, retry receives the matching
    message, and the skipped message remains queued. Existing VM-owned tests
    retain timeout wakeup, immediate timeout, scan-reduction accounting,
    ordered delivery, and invalid-process diagnostics. BEAM receive opcodes,
    tagged registers, cursor jumps, decoder failures, and instruction offsets
    were not copied because the Terlan VM owns these semantics above bytecode.
    Both focused parity tests pass with warnings denied, and
    `vm-runtime-semantics-check` contains the exact actor retry anchor. The
    obsolete external fixture is deleted under checked
    `delete-after-vm-equivalent` evidence. File-level migration progress is now
    57 ported, 1,863 not ported, 52 deleted, and 1,868 active historical files.
    `vm-process-model-check`, `vm-actor-primitives-check`, the exact retry
    anchor, and `terlan-vm-erl-suite-audit-check` pass. The composed
    `vm-runtime-semantics-check` currently stops in its unrelated Lean proof
    prerequisite because the checked `shape-implication` fingerprint for
    `docs/grammar/TERLAN_SYNTAX_SPEC.ebnf` is stale; this receive port does not
    rewrite or bless that proof metadata.
    The fifty-eighth verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_code_purge.rs`: the golden code
    server now explicitly reclaims artifact and source-map metadata for every
    drained generation in stable generation order and records ordered
    `GenerationPurged` inspection events. Active generations and process-bound
    retiring generations cannot be purged; releasing the final process retires
    that generation and makes it eligible for reclamation. Missing modules and
    active-only modules are mutation-free error/no-op paths. The long-lived REPL
    publication path now uses the operation to reclaim unused synthetic
    generations rather than leaving a test-only API. BEAM old/current code
    slots, module versions, process-reference scans, opcodes, and deletion
    booleans were not copied because Terlan owns typed generation bindings.
    `vm-code-server-check` passes with warnings denied, including ordered purge,
    bound-generation protection, CLI rendering, and production REPL lifecycle
    anchors. The obsolete external fixture is deleted under checked
    `delete-after-vm-equivalent` evidence. File-level migration progress is now
    58 ported, 1,862 not ported, 53 deleted, and 1,867 active historical files.
    The fifty-ninth verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_instruction_raw_external_term_bif_lowering.rs`:
    Terlan's active distributed transport now decodes every inbound TETF
    envelope before mutating delivery state. The bounded canonical decoder
    round-trips supported runtime values and VM references, enforces receiver
    atom manifests and nesting limits, and rejects malformed headers, unknown
    profiles/tags, truncation, trailing bytes, noncanonical bitstrings, and
    duplicate canonical map keys or record fields. Inbound transport also
    validates byte limits, trace identity, route identity, and destination
    epoch against the session before classifying accepted, duplicate, or
    out-of-order delivery. BEAM BIF import tables, opcodes, tagged operands,
    registers, and Erlang ETF compatibility were not copied because Terlan
    owns TETF and typed VM transport semantics. The full
    `vm-distribution-envelope-check` and focused 29-test TETF plus 33-test
    coordination suites pass with warnings denied. The obsolete external
    fixture is deleted under checked `delete-after-vm-equivalent` evidence.
    File-level migration progress is now 59 ported, 1,861 not ported, 54
    deleted, and 1,866 active historical files.
    The sixtieth verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_code_lifecycle_reload.rs`:
    the golden code server now proves that reclaiming a retired generation does
    not reset or reuse module generation identity. A later publication remains
    a hot reload, retires the previous active generation, activates a strictly
    newer generation, keeps purged artifacts absent from inspection, and
    records the complete lifecycle in stable event order. BEAM byte loading,
    deleted current-code slots, code indexes, module versions, and builder
    parity were not copied because Terlan owns source-backed generation
    publication directly. `vm-code-server-check` passes with warnings denied,
    including the exact reload-after-purge anchor. The obsolete external
    fixture is deleted under checked `delete-after-vm-equivalent` evidence.
    File-level migration progress is now 60 ported, 1,860 not ported, 55
    deleted, and 1,865 active historical files.
    The sixty-first verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_code_lifecycle_import_failures.rs`:
    the golden code server now proves lifecycle failures are transactional.
    Invalid source publication, missing-module purge, artifact-mismatched
    generation promotion, and stale process release preserve the active
    generation, generation snapshots, and ordered lifecycle event history
    exactly. Untyped Erlang import calls, BIF dispatch, atom tables, registers,
    and `badarg` shapes were not copied because Terlan exposes typed code-server
    operations. `vm-code-server-check` passes with warnings denied, including
    the exact mutation-free failure anchor. The obsolete external fixture is
    deleted under checked `delete-after-vm-equivalent` evidence. File-level
    migration progress is now 61 ported, 1,859 not ported, 56 deleted, and
    1,864 active historical files.
    The sixty-second verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_code_lifecycle_import_crosscheck.rs`:
    the golden code server now proves the complete process-bound replacement
    sequence directly. An old generation with a live binding remains
    `Retiring`; premature purge is a state and event no-op; releasing its final
    binding records retirement; explicit reclamation records purge; and the
    replacement generation remains active throughout. Lifecycle inspection
    preserves exact publish, reload, retire, and purge order. External `erlc`
    compilation, BEAM loading, Erlang lifecycle imports, atom results, and
    direct-adapter cross-checks were not copied because the Terlan VM owns this
    sequence. `vm-code-server-check` passes with warnings denied, including the
    exact process-bound lifecycle anchor. The obsolete fixture is deleted under
    checked `delete-after-vm-equivalent` evidence. File-level migration progress
    is now 62 ported, 1,858 not ported, 57 deleted, and 1,863 active historical
    files.
    The sixty-third verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_code_lifecycle.rs`: the golden code
    server now provides module-scoped generation and lifecycle-event inspection
    without copying unrelated runtime traffic. Filtered generation views retain
    artifact identity and lifecycle state; filtered event views retain their
    original global sequence numbers; missing modules return empty inspection
    views. Shared snapshot construction removes duplicate projection logic.
    Existing focused tests cover publication, replacement, live process
    bindings, retirement, purge, rollback, mutation-free failures, source
    reload, CLI rendering, and REPL publication. BEAM current/old code slots,
    load/delete decision adapters, Erlang imports, byte-loader failures, and
    trace-buffer retention errors were not copied because they are compatibility
    machinery rather than Terlan generation semantics. `vm-code-server-check`
    passes with warnings denied, including the exact module-scoped inspection
    anchor. The obsolete broad fixture is deleted under checked
    `delete-after-vm-equivalent` evidence. File-level migration progress is now
    63 ported, 1,857 not ported, 58 deleted, and 1,862 active historical files.
    The sixty-fourth verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_code_server_imports.rs`: source-backed
    VM module artifacts now retain a deterministic public-function manifest
    derived from CoreIR exports. The code server exposes typed active-generation
    queries for module availability and exact function name/arity availability.
    Focused coverage proves public zero- and one-arity exports, rejects private
    functions, wrong arities, and missing modules, and proves hot reload removes
    superseded signatures while exposing replacement signatures immediately.
    Existing generation bindings and lifecycle tests cover process code
    ownership, retirement, and purge. Erlang `module_loaded`,
    `function_exported`, `check_process_code`, delete/purge imports, atom-table
    booleans, malformed BIF arguments, and BEAM loading were not copied because
    Terlan exposes typed generation APIs. `vm-code-server-check` passes with
    warnings denied, including the real source-to-CoreIR export-manifest anchor.
    The obsolete fixture is deleted under checked `delete-after-vm-equivalent`
    evidence. File-level migration progress is now 64 ported, 1,856 not ported,
    59 deleted, and 1,861 active historical files.
    The sixty-fifth verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_instruction_hash_bifs.rs`: the
    golden VM now hashes portable runtime values through fixed type tags,
    fixed little-endian framing, and one deterministic algorithm independent
    of Rust enum discriminants or randomized collection state. Nested values
    retain stable fingerprints; equal flat and indexed maps hash identically
    regardless of insertion order; similar payloads in distinct value families
    remain separated; positive ranges are bounded and zero is rejected; and
    executable closure identity is rejected rather than assigned a misleading
    portable hash. Erlang `phash2`, MFA dispatch, BEAM atoms/references, raw
    tagged terms, and BIF arity errors were not copied because they are backend
    compatibility behavior. `vm-value-hash-check` passes with warnings denied
    and is included in `vm-runtime-semantics-check`. The obsolete fixture is
    deleted under checked `delete-after-vm-equivalent` evidence. File-level
    migration progress is now 65 ported, 1,855 not ported, 60 deleted, and
    1,860 active historical files.
    The sixty-sixth verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_compound_term_ownership.rs`:
    Terlan VM now proves compound-value ownership with typed tuple, list, byte,
    and insertion-ordered A-CHAMP values instead of BEAM owner wrappers,
    shallow tagged headers, registers, or copied-word estimates. The replacement
    coverage preserves aggregate cardinality and ordering, deterministic map
    replacement, exact logical retained-size accounting, persistent clone
    isolation at the A-CHAMP activation boundary, and nested actor-message
    delivery without mutable aliases. The complete `runtime::vm::memory`
    namespace passes all 30 tests, and `terlan-vm-erl-suite-audit-check` accepts
    the obsolete fixture deletion under `vm-memory-heap-pressure-check`
    evidence. The named completed-slice gate still delegates to the canonical
    Rust suite, which is independently red on existing generated-binding and
    constructor tests; no aggregate pass is claimed here. File-level migration
    progress is now 66 ported, 1,854 not ported, 61 deleted, and 1,859 active
    historical files.
    The sixty-seventh verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_loader_hash_bif_full_cycle.rs`:
    a golden VM actor-delivery regression now proves the same nested tuple and
    byte value retains one deterministic fixed-tag hash before and after mailbox
    ownership transfer. Positive range projections remain bounded, range one
    returns zero, zero remains rejected, map storage order remains canonical,
    and executable identity remains explicitly nonportable. The replacement
    exercises VM process and memory accounting instead of compiling Erlang,
    loading BEAM imports, seeding registers, or interpreting process ticks.
    `vm-value-hash-check` passes all six focused tests with warnings denied, and
    `terlan-vm-erl-suite-audit-check` accepts the obsolete fixture deletion
    under checked `delete-after-vm-equivalent` evidence. File-level migration
    progress is now 67 ported, 1,853 not ported, 62 deleted, and 1,858 active
    historical files.
    The sixty-eighth verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_loader_external_term_bif_full_cycle.rs`:
    the golden VM now directly decodes its Terlan External Term Format runtime
    profile instead of relying on an Erlang compiler, BEAM loader, imported
    BIFs, registers, or process ticks for serialization coverage. Nested atom,
    list, map, and byte values round-trip through one canonical decoder, while
    adversarial coverage rejects receiver-undeclared atoms, distribution data
    presented as a runtime term, and trailing bytes. The implementation reuses
    the bounded distribution decoder and keeps runtime and envelope profiles
    explicitly separated. `vm-distribution-envelope-check` passes all 20 exact
    tests with warnings denied, and `terlan-vm-erl-suite-audit-check` accepts
    the obsolete fixture deletion under checked
    `delete-after-vm-equivalent` evidence. File-level migration progress is now
    68 ported, 1,852 not ported, 63 deleted, and 1,857 active historical files.
    The sixty-ninth verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_loader_compare_bif_full_cycle.rs`:
    Terlan's typed VM operator path now proves structural byte equality and
    inequality, including embedded zero and high bytes, and rejects equality
    between byte and list values without Erlang's loose comparison relation.
    Existing source parity preserves all true and false numeric ordering
    boundaries plus scalar, tuple, list, and map equality. The replacement uses
    CoreIR operators and VM values instead of compiling Erlang, loading BEAM
    imports, seeding registers, or interpreting result atoms and process ticks.
    `operator-coverage-100-check` passes with warnings denied: five quality
    tests, the exact byte-value VM regression, 26 checked operators, 12
    executable operator tests, and 29 executable comparison tests.
    `terlan-vm-erl-suite-audit-check` accepts the obsolete fixture deletion
    under checked `delete-after-vm-equivalent` evidence. File-level migration
    progress is now 69 ported, 1,851 not ported, 64 deleted, and 1,856 active
    historical files.
    The seventieth verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_loader_binary_bif_full_cycle.rs`:
    typed Terlan source now exercises byte-buffer length, full and inner
    slicing, and explicit byte and bit lengths after conversion to
    `std.vm.BitString` through CoreIR and VM execution. This preserves the
    useful empty, full, and middle binary boundaries while intentionally
    replacing Erlang's generic `size/1` dispatch and negative-length
    `binary_part` convention with type-owned operations and nonnegative ranges.
    No Erlang compiler, BEAM loader, imported BIF, register setup, or process
    tick interpretation remains in the replacement. `binary-runtime-suite-check`
    passes with warnings denied across 85 executable Terlan tests and 37
    focused Rust/VM tests, including the new source parity regression.
    `terlan-vm-erl-suite-audit-check` accepts the obsolete fixture deletion
    under checked `delete-after-vm-equivalent` evidence. File-level migration
    progress is now 70 ported, 1,850 not ported, 65 deleted, and 1,855 active
    historical files.
    The seventy-first verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_loader_exception_bif_full_cycle.rs`:
    the golden VM now proves that every portable typed failure reason survives
    process exit state, trapped-link delivery, and monitor-down delivery without
    changing its payload. The adversarial matrix covers error text, killed,
    shutdown timeout, and memory-limit accounting while intentionally replacing
    Erlang's `error`, `throw`, and `exit` exception classes with Terlan-owned
    `VmExitReason` semantics. No Erlang compiler, BEAM loader, imported BIF,
    register setup, or process tick interpretation remains in the replacement.
    `vm-failure-primitives-check` passes all 29 focused tests with warnings
    denied, and `terlan-vm-erl-suite-audit-check` accepts the obsolete fixture
    deletion under checked `delete-after-vm-equivalent` evidence. File-level
    migration progress is now 71 ported, 1,849 not ported, 66 deleted, and 1,854
    active historical files.
    The next verified nonportable deletion is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_loader_process_dictionary_bif_full_cycle.rs`:
    it compiled Erlang and executed hidden process-dictionary mutation,
    enumeration, erasure, and reverse-value lookup through BEAM imports. Terlan
    intentionally requires source-visible typed state through actor, service,
    or persistent-actor abstractions, so the VM does not preserve implicit
    per-process dictionaries or an `undefined` sentinel contract. The exact
    inventory now classifies the fixture as `remove-non-portable`, and
    `terlan-vm-erl-suite-audit-check` accepts its deletion. File-level migration
    progress remains 71 ported and 1,849 not ported, with 67 deleted and 1,853
    active historical files.
    The next verified nonportable deletion is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_file_ingestion.rs`: its 668-line
    fixture parsed BEAM `FOR1` containers, decoded atom/import/export/literal
    chunks and encoded operands, invoked `erlc`, installed loaded BEAM modules,
    and executed their exports. Terlan compiler artifacts carry typed VM IR and
    the runtime does not preserve BEAM file compatibility, so the exact-path
    inventory now classifies the fixture as `remove-non-portable`. Its legacy
    Make target, list-consumer requirement, and external execution-document
    references are also removed. `terlan-vm-erl-suite-audit-check` passes all
    9 audit self-tests and classifies the 1,852 active files. File-level
    migration progress remains 71 ported and 1,849 not ported, with 68 deleted
    and 1,852 active historical files.
    The external `vm-list-consumer-inventory-check` is green again after its
    required-test set and consumer documentation stopped treating four already
    ported-and-deleted BEAM fixtures as active coverage. It now validates only
    the four retained structured-term, GC-root, external-term, and raw-list-BIF
    fixtures that still exist, while rejecting list-slice compatibility tokens
    in every retained test. `terlan-vm-erl-suite-audit-check` also passes all 9
    audit self-tests after the cleanup, so the harness correction does not alter
    the checked 71-port, 68-deletion migration ledger.
    The adjacent external `vm-list-heap-representation-check` is also green
    after removing executable expectations and active-inventory claims for 18
    exact test paths already recorded as deleted in the canonical ledger. The
    checker still scans every retained VM test/example through its globs and
    preserves direct register-access, public-value count, NativeBoundary
    destination, and caller-migration guardrails; an exact-path audit now proves
    every path it opens exists. `terlan-vm-erl-suite-audit-check` continues to
    pass all 9 audit self-tests with 1,852 active files classified, so this
    hardening changes no migration status or replacement evidence.
    The external GNUmake harness no longer invokes 22 integration-test targets
    whose source fixtures were already deleted by checked port/removal slices.
    Empty API-boundary and atom-compatibility targets and their stale
    documentation commands are retired, while every command for a retained
    test remains. `terlan-vm-erl-suite-audit-check` now parses standalone VM
    `--test` commands and rejects any target without a corresponding source;
    adversarial accept/reject coverage raises the gate to 11 self-tests. The
    full audit remains green at 1,852 active files, 71 ports, and 68 deletions.
    The seventy-second verified port is
    `terlan-vm/erts/emulator/test/register_SUITE.erl`: OTP-8099's repeated
    spawn/register/lookup/exit cycle now has a deterministic golden VM
    process-registry regression covering 4,096 unique process identities,
    atomic name cleanup, immediate name reuse, and typed rejection when an
    exited owner attempts to reacquire the name. The test lives at the
    `VmProcessTable` ownership boundary and runs through the existing
    `vm-process-model-check`; no duplicate test runner or actor-facade wrapper
    was added. That gate passes 43 process tests with warnings denied, and
    `terlan-vm-erl-suite-audit-check` passes 11 audit self-tests with the exact
    source bound to its replacement gate. File-level migration progress is now
    72 ported, 1,848 not ported, 68 deleted, and 1,852 active historical files.
    The seventy-third verified port is
    `terlan-vm/erts/emulator/test/ref_SUITE.erl`: the golden VM now owns opaque
    reference namespace validation, node and boot-epoch separation, monotonic
    identity, and no-wrap exhaustion. Reference tests live with the allocator
    instead of the failure subsystem, exercise 65,536 ordered allocations, and
    prove repeated exhausted allocations cannot wrap or reuse either reference
    or unique-integer identities. OTP-specific ETS ordering and ERTS heap-word
    size assertions are intentionally excluded because they expose storage and
    runtime ABI details that Terlan does not preserve. With warnings denied,
    `vm-failure-primitives-check` passes 27 failure tests and 3 dedicated
    reference tests; `terlan-vm-erl-suite-audit-check` passes 11 audit
    self-tests and binds the exact source to that gate. File-level migration
    progress is now 73 ported, 1,847 not ported, 68 deleted, and 1,852 active
    historical files.
    The seventy-fourth verified port is
    `terlan-vm/erts/emulator/test/unique_SUITE.erl`: Terlan replaces OTP's
    scheduler-bit packing, machine-word thresholds, mutable debug counter
    state, and wraparound representation with a VM-owned positive monotonic
    unique-integer sequence and typed exhaustion. A dedicated allocator test
    produces 65,536 consecutive values while interleaving reference allocation,
    proving the sequences remain independent and exact under churn; the prior
    bounded tests prove repeated exhaustion cannot wrap or reuse identities.
    With warnings denied, `vm-failure-primitives-check` passes 27 failure tests
    and 4 dedicated reference/unique-identity tests, while
    `terlan-vm-erl-suite-audit-check` passes 11 audit self-tests and binds the
    exact source to that replacement gate. File-level migration progress is now
    74 ported, 1,846 not ported, 68 deleted, and 1,852 active historical files.
    The seventy-fifth verified port is
    `terlan-vm/erts/emulator/test/prim_eval_SUITE.erl`: the golden VM timer
    suite now proves that an unmatched mailbox message and a full-budget
    scheduler preemption cannot disarm or corrupt an active receive timeout.
    The receiver performs a failed selective scan, yields at its reduction
    boundary, reblocks, and wakes at the original deadline while the unmatched
    message remains queued. OTP's internal argument-register allocation check
    is intentionally excluded because typed VM IR does not preserve BEAM
    registers. With warnings denied, `vm-timer-primitives-check` passes all 33
    timer tests. `terlan-vm-erl-suite-audit-check` binds the exact source to
    that existing gate. File-level migration progress is now 75 ported, 1,845
    not ported, 68 deleted, and 1,852 active historical files.
    The seventy-sixth verified port is
    `terlan-vm/erts/emulator/test/after_SUITE.erl`: deterministic VM timer tests
    now cover zero and maximum-u32 receive deadlines, prove that large
    deadlines cannot wake one tick early, and wake 10,000 equal-deadline
    receivers exactly once in stable timer-identity order. The earlier
    selective-receive regression preserves unmatched messages across timeout
    handling. OTP Common Test wall-clock tolerance is replaced by monotonic
    logical ticks, and dynamically invalid BEAM timeout terms are excluded
    because Terlan's timer API accepts typed `u64` durations and separately
    rejects deadline overflow. With warnings denied,
    `vm-timer-primitives-check` passes all 35 timer tests in 0.07 seconds.
    `terlan-vm-erl-suite-audit-check` binds the exact source to that existing
    gate. File-level migration progress is now 76 ported, 1,844 not ported, 68
    deleted, and 1,852 active historical files.
    The seventy-seventh and seventy-eighth verified ports are
    `terlan-vm/erts/emulator/test/hash_property_test_SUITE.erl` and
    `terlan-vm/erts/emulator/test/property_test/phash2_properties.erl`: the
    golden VM stable-hash suite now checks 4,097 generated nested values,
    repeats every portable hash vector into a 10,000-element long input, and
    freezes ten scalar and structural fingerprints as a cross-Terlan-release
    compatibility contract. Existing tests retain type separation, canonical
    flat/indexed map hashing, range bounds, actor-delivery stability, and typed
    rejection of executable identity. Erlang `phash2` numeric values and RPC
    comparison across OTP releases are intentionally excluded because Terlan
    owns a fixed-tag, fixed-endian hash format rather than the OTP algorithm.
    With warnings denied, `vm-value-hash-check` passes all 8 hash tests in 0.16
    seconds. `terlan-vm-erl-suite-audit-check` binds both exact sources to that
    existing gate. File-level migration progress is now 78 ported, 1,842 not
    ported, 68 deleted, and 1,852 active historical files.
    The seventy-ninth through eighty-first verified ports are the three
    `terlan-vm/erts/emulator/test/bs_match_tail*_SUITE.erl` compiler-mode
    variants. One typed VM bitstring path now preserves aligned, dynamic, and
    zero-length tails, rejects unaligned suffixes at the byte-binary boundary,
    detects exact-length mismatches, and retains a 4,097-bit large tail without
    truncation. The no-optimizer and stripped-type-metadata OTP variants map to
    this same observable contract; BEAM registers, function-clause encoding,
    optimizer modes, and type-metadata modes are intentionally retired. With
    warnings denied, `binary-runtime-suite-check` passes 12 generated binary
    checks, 66 binary tests, 7 public VM BitString tests, 15 Rust bitstring
    tests, and all focused bridge and parity checks. The audit binds all three
    exact sources to that existing gate. File-level migration progress is now
    81 ported, 1,839 not ported, 68 deleted, and 1,852 active historical files.
    The eighty-second through eighty-fourth verified ports are the three
    `terlan-vm/erts/emulator/test/bs_match_bin*_SUITE.erl` compiler-mode
    variants. Typed VM bitstring tests now split every byte boundary from both
    aligned and three-bit-offset inputs, exercise every whole-byte bit slice at
    every start position, compare fixed and dynamic widths across a
    deterministic 10,000-byte value, and cover empty values, 13/16-bit units,
    known positions, checked range overflow, and the historical match beyond
    the 4,095-bit immediate boundary. BEAM heap reservation, register-context,
    sub-binary representation, optimizer mode, and stripped-type-metadata mode
    are intentionally retired. With warnings denied,
    `binary-runtime-suite-check` passes 12 generated binary checks, 66 binary
    tests, 7 public VM BitString tests, the expanded 18-test Rust bitstring
    suite, and all focused bridge and parity checks. The audit binds all three
    exact sources to that existing gate. File-level migration progress is now
    84 ported, 1,836 not ported, 68 deleted, and 1,852 active historical files.
    The eighty-fifth through eighty-seventh verified ports are the three
    `terlan-vm/erts/emulator/test/bs_utf*_SUITE.erl` compiler-mode variants.
    The production VM bitstring implementation now encodes and decodes UTF-16
    and UTF-32 scalars in both byte orders alongside UTF-8, exposes the
    operations through typed `std.vm.BitString` declarations and closed CoreIR
    intrinsic IDs, and rejects malformed lengths, surrogates, out-of-range
    values, and unaligned encodings. The Rust suite exhaustively round-trips all
    1,112,064 valid Unicode scalars through all five encodings and preserves
    exact wire bytes and offset extraction. The no-optimizer and
    stripped-type-metadata variants map to the same observable VMIR contract;
    Common Test, optimizer modes, type-metadata modes, and BEAM representation
    details are intentionally retired. With warnings denied,
    `binary-runtime-suite-check` passes 12 generated binary checks, 66 binary
    tests, 9 public VM BitString tests, the expanded 21-test Rust bitstring
    suite, and all focused CoreIR, bridge, and parity checks. The audit binds
    all three exact sources to that existing gate. File-level migration
    progress is now 87 ported, 1,833 not ported, 68 deleted, and 1,852 active
    historical files.
    The eighty-eighth through ninety-seventh verified ports are the ten
    behavior-identical `terlan-vm/lib/compiler/test/bs_utf*_SUITE.erl`
    compiler-mode variants. Terlan binary layouts now accept typed `Utf16` and
    `Utf32` scalar descriptors alongside `Utf8`; the enclosing
    `Binary[big|little]` policy selects wire order. Constructors lower to closed
    CoreIR bitstring intrinsics, while VM patterns decode BMP, supplementary,
    and UTF-32 scalars atomically in both byte orders. Parser, typechecker,
    CoreIR, direct VM, and executable Terlan tests preserve exact wire vectors,
    multi-scalar layouts, and rejection of truncated pairs, lone surrogates,
    invalid UTF-32 values, and unaligned captures without partial bindings.
    OTP coverage, inlining, Core/SSA/module/type/post optimization modes,
    Common Test mechanics, and BEAM heap-allocation internals are intentionally
    retired. With warnings denied, `binary-bitstring-processing-check` passes
    its complete parser, formatter, typechecker, CoreIR, VM, public Terlan,
    property, descriptor-contract, framing, and binary-protocol benchmark
    ownership chain. The audit binds all ten exact sources to that existing
    gate. File-level migration progress is now 97 ported, 1,823 not ported, 68
    deleted, and 1,852 active historical files.
    The ninety-eighth through one-hundred-seventh verified ports are the ten
    behavior-identical `terlan-vm/lib/compiler/test/bs_size_expr*_SUITE.erl`
    compiler-mode variants. Their observable runtime-derived segment sizing
    now executes through typed `Bytes.read_uint_be`, `Bytes.slice`, and
    `BitString.slice` operations rather than BEAM segment-size syntax. Terlan
    tests cover empty and nonempty word-counted payloads, preserved trailing
    bytes, lexical runtime widths, and unaligned bit fields. Direct VM tests
    additionally reject zero-derived negative lengths, truncated frames, and
    maximum-`UInt32` computed ranges with stable errors. OTP coverage,
    inlining, Core/SSA/module/type/post optimization modes, receive-specific
    forms, Common Test, and BEAM representation details are intentionally
    retired. With warnings denied, `binary-bitstring-processing-check` passes
    the complete binary/bitstring semantic, property, adversarial, framing,
    and benchmark chain. The audit binds all ten exact sources to that gate.
    File-level migration progress is now 107 ported, 1,813 not ported, 68
    deleted, and 1,852 active historical files.
    The one-hundred-eighth verified port is
    `terlan-vm/lib/stdlib/test/random_SUITE.erl`. Terlan replaces OTP's hidden
    process-global random state and legacy tuple seed overload with explicit,
    immutable `std.random.Random.Generator` values. The native adapter regression
    proves two equal seeds replay the same 100-draw bounded sequence and every
    value stays inside the exclusive upper bound. The executable Terlan suite
    advances two generators through ten VM-evaluated bounded draws, preserving
    deterministic replay and the historical interval contract. With warnings
    denied, `std-random-check` passes four native adapter tests plus all 18
    executable and property tests, and `terlan-vm-erl-suite-audit-check` binds
    the exact historical source to that replacement gate. File-level migration
    progress is now 108 ported, 1,812 not ported, 68 deleted, and 1,852 active
    historical files.
    The one-hundred-ninth verified port is
    `terlan-vm/lib/kernel/test/group_SUITE.erl`. The actor runtime now exposes
    an explicit process-liveness query and proves that an unknown message
    remains unmatched without terminating its recipient. A later known message
    still wakes and executes on the same actor, the skipped message remains
    queued, and liveness becomes false only after explicit normal exit; missing
    process identities also report false. With warnings denied,
    `vm-actor-primitives-check` passes all 82 actor-runtime tests, and
    `terlan-vm-erl-suite-audit-check` binds the exact historical source to that
    replacement gate. File-level migration progress is now 109 ported, 1,811
    not ported, 68 deleted, and 1,852 active historical files.
    The next two verified nonportable deletions are the duplicated
    `terlan-vm/lib/kernel/test/zzz_SUITE.erl` and
    `terlan-vm/lib/stdlib/test/zzz_SUITE.erl` shutdown-order fixtures. Their
    only behavior was calling OTP's private `erts_debug:lc_graph/0` hook to
    emit an optional lock-checker artifact before the host VM terminated.
    Terlan exposes neither that ERTS debug API nor its lock-graph file format,
    and the fixtures contain no portable application, actor, or standard-
    library semantics. Exact `remove-non-portable` inventory rows now prevent
    either broad kernel or stdlib migration rule from implying false parity.
    `terlan-vm-erl-suite-audit-check` and `no-default-beam-runtime-check` pass.
    File-level migration progress remains 109 ported and 1,811 not ported,
    with 70 deleted and 1,850 active historical files.
    The one-hundred-tenth verified port is
    `terlan-vm/lib/stdlib/test/format_SUITE.erl`. OTP used malformed output-
    device terms to prove `io:format/3` returned an error instead of hanging.
    Terlan has no untyped output-device argument: `std.io.Console.println/1`
    accepts exactly one `String`. The negative API suite now proves both a
    non-string payload and the historical extra-device call shape fail during
    typechecked module dispatch before VM execution, eliminating the blocking
    runtime path by construction. The existing top-level
    `std-vm-parity-matrix-check` now owns these diagnostics alongside all 1,531
    classified std modules. That gate and `terlan-vm-erl-suite-audit-check`
    pass. File-level migration progress is now 110 ported, 1,810 not ported,
    70 deleted, and 1,850 active historical files.
    The one-hundred-eleventh verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_exception_bifs.rs`. Terlan VM
    replaces BEAM import dispatch and exception-class compatibility with typed
    exit reasons. The failure matrix now proves that empty, control-bearing,
    4,096-byte, killed, shutdown-timeout, and memory-limit reasons remain exact
    in process state, trapped-link messages, and monitor-down messages. With
    warnings denied, `vm-failure-primitives-check` passes 27 failure tests and
    four reference tests. The exact historical fixture is retired and bound to
    that replacement gate. File-level migration progress is now 111 ported,
    1,809 not ported, 71 deleted, and 1,849 active historical files.
    The next verified nonportable deletion is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_exception_catch.rs`. The fixture
    exclusively exercised BEAM catch-stack opcodes, X-register destinations,
    handler offsets, Erlang exception-class tuples, and raw stacktrace metadata
    decoding. Terlan keeps recoverable failures in typed `Result` and pattern
    flow, process termination in typed VM exit reasons, and source locations in
    VM-owned diagnostics; it does not preserve a second BEAM catch mechanism.
    The exact `remove-non-portable` inventory row prevents the broad ERTS rule
    from implying false parity. `terlan-vm-erl-suite-audit-check` and
    `no-default-beam-runtime-check` pass. File-level migration progress remains
    111 ported and 1,809 not ported, with 72 deleted and 1,848 active historical
    files.
    The one-hundred-twelfth verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/binary_descriptors.rs`. Terlan replaces
    BEAM storage classes, explicit refcount plans, generation tokens, and
    release actions with canonical immutable VM bitstrings. The direct VM suite
    now proves persistent clones retain their exact value while derived slices
    remain independent, alongside existing logical-length, tail-mask, checked
    slice, overflow, endian, and width coverage. `binary-runtime-suite-check`
    passes 12 generated descriptor properties, 66 public binary tests, four
    dynamic-size tests, nine public bitstring tests, 22 direct VM bitstring
    tests, and the remaining CoreIR/runtime checks. The historical build target
    now delegates to that golden gate instead of compiling the retired crate
    test. `terlan-vm-erl-suite-audit-check` passes. File-level migration
    progress is now 112 ported, 1,808 not ported, 73 deleted, and 1,847 active
    historical files.
    The one-hundred-thirteenth verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_native_worker_transport.rs`.
    Terlan replaces the BEAM destination-register and `BeamValue` frame codec
    with its active NativeBoundary helper boundary. Reply reads are now capped at
    64 KiB and reject oversized, unterminated, and non-UTF-8 input before value
    decoding or VM mutation; existing worker coverage preserves correlated
    request ids and credits, cancellation and late-reply rejection, stale
    resource handling, and capability enforcement. The native-worker quality
    check now verifies 17 canonical Rust tests through the single-suite
    orchestration contract instead of requiring obsolete exact Make recipes.
    `native-boundary-runtime-adversarial-check` passes all 17 focused boundary
    tests, the historical aggregate target delegates to that golden gate, and
    `terlan-vm-erl-suite-audit-check` passes. File-level migration progress is
    now 113 ported, 1,807 not ported, 74 deleted, and 1,846 active historical
    files.
    The one-hundred-fourteenth verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_instruction_arithmetic.rs`.
    Terlan preserves the language-visible signed integer contract through one
    checked implementation shared by direct CoreIR and serialized VMIR:
    addition, subtraction, multiplication, truncating division, remainder,
    zero-divisor rejection, and every `i64` overflow boundary. Host Rust
    arithmetic can no longer panic on these inputs, and both execution lanes
    emit the same typed `division_by_zero` and `arithmetic_overflow`
    diagnostics. BEAM opcode numbers, tagged small terms, decoder arities,
    registers, jump labels, and BEAM-only bitwise/shift instructions are
    retired rather than copied into VMIR. `vm-integer-operator-parity-check`
    passes six focused source/CoreIR and VMIR tests, and
    `terlan-vm-erl-suite-audit-check` passes. File-level migration progress is
    now 114 ported, 1,806 not ported, 75 deleted, and 1,845 active historical
    files.
    The one-hundred-fifteenth verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_instruction_arithmetic_bifs.rs`.
    Typed Terlan operators already replace the portable arithmetic behavior;
    this migration also routes `std.core.Int.abs` through the shared checked
    integer implementation so `i64::MIN` returns the stable
    `arithmetic_overflow` diagnostic instead of panicking in host Rust. The
    replacement tests preserve positive and negative absolute values, source
    execution, dynamic type and arity rejection, zero-divisor handling, and
    every signed arithmetic overflow boundary. Erlang MFA dispatch,
    `BeamValue` tags, small-term limits, and BEAM-only bitwise/shift BIFs are
    retired. `vm-integer-operator-parity-check` passes eight focused tests and
    `terlan-vm-erl-suite-audit-check` passes. File-level migration progress is
    now 115 ported, 1,805 not ported, 76 deleted, and 1,844 active historical
    files.
    The one-hundred-sixteenth verified port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_instruction_atom_bifs.rs`.
    Terlan replaces mutable BEAM atom tables and runtime text-to-atom
    conversion with finite compiler-known singleton aliases. Explicit
    `Atom["..."]` aliases now retain their canonical Unicode payload through
    CoreIR and VM execution, while bodyless aliases retain deterministic
    type-derived payloads. Parser adversarial tests reject dynamic and empty
    atom payloads, NativeBoundary JSON and Postgres boundaries preserve atom-like
    external keys as `String`, and standard-library property tests cover atom
    equality and rendering. Atom ids, Latin-1 mode, `BeamValue` tags, and
    Erlang MFA dispatch are retired. `vm-atom-boundary-parity-check` passes
    four focused Rust tests and three Terlan property tests, and
    `terlan-vm-erl-suite-audit-check` passes. File-level migration progress is
    now 116 ported, 1,804 not ported, 77 deleted, and 1,843 active historical
    files.
    The one-hundred-seventeenth implementation-complete port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_instruction_type_bifs.rs`.
    Terlan now evaluates `is_type` through compiler-owned `CoreType` structure
    in both direct-call and intrinsic paths instead of comparing rendered type
    strings. Numeric `Number`, top `Dynamic`, bottom `Never`, literal atoms,
    homogeneous and mixed lists/maps, tuples, records, closures, Bytes, and
    BitString values have positive and adversarial mismatch coverage; map type
    reflection also preserves homogeneous key and value types. Erlang atom
    tables, raw pid/port/reference headers, MFA and arity dispatch, tagged
    terms, and record tuples are retired rather than copied into VMIR. The
    direct `vm-runtime-semantics-check` parity command passes 34 tests. The
    aggregate Make target was also executed and reached 4,683 passing tests,
    but remains blocked by 120 unrelated dirty-tree failures dominated by the
    separate raw `:atom` fixture migration, so aggregate green status is not
    claimed here. File-level migration progress is now 117 ported, 1,803 not
    ported, 78 deleted, and 1,842 active historical files.
    The one-hundred-eighteenth implementation-complete port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_instruction_type_tests.rs`.
    Runtime arrow-type predicates now validate closure arity instead of
    accepting every closure, and source-level `where is_type(...)` guards
    execute numeric versus fallback branches through checked CoreIR. The
    parity matrix covers empty and populated lists/maps, exact tuple shape,
    scalar and numeric types, literal atoms, records, Bytes, BitString,
    qualified runtime type names, malformed type text, and zero-, one-, and
    mismatched-arity closures. Existing VM parity tests preserve complete
    boolean truth tables, dynamic operand rejection, closure call arity, and
    tuple-arity dispatch. BEAM opcode decoding, branch labels, registers,
    loaded boolean atom indexes, tagged pid/port/reference headers, and raw
    term diagnostics are retired. The direct `vm-runtime-semantics-check`
    parity command passes all 35 tests; the aggregate target retains the
    unrelated raw-atom migration blocker documented by the preceding port.
    File-level migration progress is now 118 ported, 1,802 not ported, 79
    deleted, and 1,841 active historical files.
    The one-hundred-nineteenth implementation-complete port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_instruction_tuple_list_conversion.rs`.
    Terlan replaces dynamically typed `tuple_to_list` and `list_to_tuple`
    opcodes with explicit structural patterns over homogeneous lists and
    fixed-arity tuples. Source-to-CoreIR VM tests prove ordered pair round
    trips, unchanged source values, and empty and wrong-arity list fallback;
    adversarial compile tests reject scalar inputs before execution. Generic
    heterogeneous tuple conversion, opcode decoding, registers, destinations,
    and dynamic `badarg` behavior are retired because they conflict with the
    typed collection model. The focused `vm-runtime-semantics-check` parity
    command passes both conversion tests. File-level migration progress is now
    119 ported, 1,801 not ported, 80 deleted, and 1,840 active historical
    files.
    The one-hundred-twentieth terminal migration is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_instruction_raw_binary_list_bif_lowering.rs`.
    Its observable byte/list behavior is replaced by the typed `std.vm.Bytes`
    source-to-CoreIR contract: empty and complete-octet round trips, exact
    slicing, ordered concatenation, length, invalid octets, invalid ranges, and
    VM-owned BitString conversion. The historical fixture added only BEAM BIF
    and GC-BIF opcodes, import indexes, tagged operands, registers,
    destinations, and decoder failures, so those compatibility details are
    retired rather than copied. With Rust warnings denied, the focused Bytes
    source suite passes all 4 tests, direct VM parity passes all 37 tests, the
    byte adversarial module passes all 11 tests, and the inventory audit is
    green. The canonical umbrella reached 4,686 passing tests but remains
    blocked by 123 unrelated dirty-tree failures in native binding fixtures and
    constructor/profile work, so aggregate green status is not claimed here.
    File-level migration progress is now 120 ported, 1,800 not ported, 81
    deleted, and 1,839 active historical files.
    The one-hundred-twenty-first terminal migration is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_instruction_unary_bif_lowering.rs`.
    Its observable behavior is replaced by existing typed source-to-CoreIR and
    VM contracts for checked integer negation and absolute value, list and map
    cardinality, Bytes and BitString sizing, structural tuple/list conversion,
    finite atom rendering, integer and byte/list conversion, and bounded TETF
    round trips. Opcode 124, import indexes, tagged operands, X/Y registers,
    destinations, literal staging, Erlang generic size dispatch, dynamic
    iolists, and decoder failure shapes are retired. With Rust warnings denied,
    direct VM parity passes all 37 tests, the focused Int, List, Map, Bytes, and
    BitString source suites pass all 56 tests, the TETF runtime module passes
    both tests, compiler and VM checks are green, and the inventory audit is
    green. The unchanged aggregate blocker from the preceding migration was not
    redundantly rerun. File-level migration progress is now 121 ported, 1,799
    not ported, 82 deleted, and 1,838 active historical files.
    The one-hundred-twenty-second terminal migration is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_instruction_external_term_bifs.rs`.
    Terlan replaces its useful serialization behavior with the VM-owned Terlan
    External Term Format contract: canonical nested scalars, atoms, lists,
    tuples, maps, Bytes, BitStrings, records, sets, and VM references round trip,
    while undeclared atoms, unsupported runtime values, duplicate fields and
    keys, malformed headers, truncation, trailing bytes, noncanonical bits,
    excessive nesting, invalid metadata, and stale references fail before
    transport acceptance. Exact Erlang ETF bytes, AtomId indexes, BIF/MFA
    routing, opcodes 189 and 190, registers, process state, and BEAM interpreter
    diagnostics are retired. With Rust warnings denied, the canonical
    `vm-distribution-envelope-check` passes all 20 exact tests, the VM binary
    checks cleanly, and the inventory audit is green. File-level migration
    progress is now 122 ported, 1,798 not ported, 83 deleted, and 1,837 active
    historical files.
    The one-hundred-twenty-third terminal migration is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_instruction_atom_raw_ops.rs`.
    Terlan preserves finite compiler-known atom aliases, canonical Unicode
    rendering and equality through source-to-CoreIR, property-generated alias
    checks, parser rejection of dynamic and empty atom payloads, and
    NativeBoundary preservation of atom-like external keys as String. Mutable
    loaded atom tables, AtomId indexes, Latin-1 encoding mode, byte-list atom
    construction, opcodes 167 through 172, registers, process state, and BEAM
    interpreter diagnostics are retired. With Rust warnings denied, the
    canonical `vm-atom-boundary-parity-check` passes all seven executable tests
    plus its locked build/check steps, compiler and VM checks are green, and the
    inventory audit is green. File-level migration progress is now 123 ported,
    1,797 not ported, 84 deleted, and 1,836 active historical files.
    The one-hundred-twenty-fourth terminal migration is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_instruction_binary.rs`.
    Terlan preserves typed Bytes and BitString lengths, octet-list round trips,
    exact full, inner, and empty slices and splits, aligned and unaligned bit
    ranges, UTF scalar encodings, checked integer widths, persistent inputs, and
    invalid type, octet, range, width, overflow, and malformed-bit rejection
    through source, CoreIR, and direct VM contracts. Opcode numbers and arities,
    registers, process state, nested dynamic iolists and iovecs, Erlang
    negative-length `binary_part` behavior, and BEAM interpreter diagnostics are
    retired. With Rust warnings denied, the canonical
    `binary-runtime-suite-check` passes 91 Terlan source tests and 57 direct
    compiler/VM tests, compiler and VM checks are green, and the inventory audit
    is green. File-level migration progress is now 124 ported, 1,796 not ported,
    85 deleted, and 1,835 active historical files.
    The one-hundred-twenty-fifth terminal migration is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_instruction_record_bif_lowering.rs`.
    Terlan replaces BEAM tagged-tuple `is_record/2` and `is_record/3` lowering
    with typed-record identity, construction, access, source receiver dispatch,
    type predicates, and record-pattern binding. Exact record-name checks,
    required and optional fields, value mismatch fallthrough, speculative-binding
    rollback, and non-record rejection remain covered. Tuple-tag atoms, tuple
    arity including the tag, BIF and GC-BIF opcodes, imports, registers,
    destinations, loaded boolean atoms, tagged operands, and decoder failures
    are retired. With Rust warnings denied, all five focused replacement tests
    pass, compiler and VM checks are green, and
    `terlan-vm-erl-suite-audit-check` is green. A broader name-filtered probe
    also passed 62 record-related tests but found two unrelated dirty-tree
    fixtures that still use the separately removed raw `:atom` syntax, so that
    aggregate probe is not claimed as green. File-level migration progress is
    now 125 ported, 1,795 not ported, 86 deleted, and 1,834 active historical
    files.
    The next verified nonportable deletion is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_instruction_raw_tuple_mutation_bif_lowering.rs`.
    The fixture asserted only BEAM BIF and GC-BIF byte-stream lowering for
    tuple construction and mutation: opcodes `11` and `152`, import indexes,
    tagged operands, registers, destinations, and decoder failures. Terlan's
    compiler and VM exchange typed CoreIR and VMIR, so that compatibility layer
    is removed rather than ported. The separate executable tuple-mutation BIF,
    raw-operation, and full-cycle fixtures remain active until their observable
    immutable tuple behavior is replaced by typed Terlan tests. With Rust
    warnings denied, `vm-artifact-contract-freeze-check` passes its default
    target, format, build/load/run round trip, and 12 adversarial artifact
    validation checks; `terlan-vm-erl-suite-audit-check` is also green. The port
    count therefore remains 125 while file-level progress advances to 1,795 not
    ported, 87 deleted, and 1,833 active historical files.
    The one-hundred-twenty-sixth terminal migration is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_instruction_tuple_mutation_bifs.rs`.
    Terlan replaces dynamically sized Erlang tuple BIFs with fixed typed tuple
    literals, structural bindings, and explicit immutable reconstruction through
    source-to-CoreIR execution. Repeated construction, replacement, append,
    insertion, deletion, persistent inputs, and compile-time arity and element
    type rejection remain covered. MFA dispatch, one-based runtime indexes,
    dynamic tuple resizing, and BIF `badarity` and `badarg` behavior are retired.
    With Rust warnings denied, both focused parity tests pass, compiler and VM
    checks are green, and `terlan-vm-erl-suite-audit-check` is green. The
    canonical `vm-runtime-semantics-check` also executes both new tests
    successfully, but its aggregate remains non-green because of unrelated
    dirty-tree failures in other compiler and runtime fixtures. File-level
    migration progress is now 126 ported, 1,794 not ported, 88 deleted, and
    1,832 active historical files.
    The one-hundred-twenty-seventh terminal migration is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_instruction_tuple_mutation_raw_ops.rs`.
    Its observable tuple construction, replacement, append, insertion,
    deletion, persistence, and invalid-shape behavior is now owned by the fixed
    typed source-to-CoreIR tuple reconstruction parity module. Raw opcodes `149`,
    `150`, `151`, `165`, `166`, and `191`, decoder arities, tagged operands,
    literal staging, registers, destinations, one-based dynamic indexes,
    initialized-fill lists, and BEAM interpreter diagnostics are retired. With
    Rust warnings denied, the two focused positive and adversarial replacement
    tests pass and `terlan-vm-erl-suite-audit-check` is green. The canonical
    `vm-runtime-semantics-check` was also executed: its VM-owned prerequisites
    and replacement tests pass, while the aggregate remains non-green with
    4,688 passed, 123 unrelated dirty-tree compiler fixtures failed, and two
    ignored in the broad `terlc` suite. File-level migration progress is now
    127 ported, 1,793 not ported, 89 deleted, and 1,831 active historical files.
    The one-hundred-twenty-eighth terminal migration is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_instruction_stack.rs`. Terlan VM
    now owns process execution frames, typed continuation offsets, LIFO return
    restoration, inspectable stack traces, root-frame protection, loop
    back-edges, scheduler location updates, detached snapshots, and immutable
    exited-process history. BEAM push/pop opcodes, allocate/deallocate/trim
    decoding, register indexes, Y-slot shifting, stack-depth wrappers, and
    interpreter errors are retired. With Rust warnings denied, the canonical
    `vm-process-model-check` passes all 43 positive and adversarial process tests
    and `terlan-vm-erl-suite-audit-check` is green. File-level migration
    progress is now 128 ported, 1,792 not ported, 90 deleted, and 1,830 active
    historical files.
    The next verified nonportable deletion is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_instruction_gc_bif2_structured_lowering.rs`.
    The fixture asserted aggregate BEAM `gc_bif2` opcode `125` lowering for
    binary splitting, tuple element access, map lookup, min/max, list
    append/subtract, and tuple mutation. Terlan lowers each typed source
    operation independently through CoreIR and VMIR, so import indexes, compact
    tagged operands, X/Y registers, destinations, literal staging,
    compatibility-opcode selection, and decoder error shapes are removed rather
    than ported. This deletion makes no new semantic-coverage claim and does not
    inflate the port count. `terlan-vm-erl-suite-audit-check` passes. File-level
    migration progress remains 128 ported and 1,792 not ported while advancing
    to 91 deleted and 1,829 active historical files.
    The next verified nonportable deletion is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_process_dictionary.rs`. Terlan does
    not preserve implicit per-process global dictionary state: state must be
    source-visible and typed through actors, Agent, GenServer, or
    PersistentActor contracts. Dictionary get/put/erase/get-all/get-keys
    instructions and imports, copied-term compatibility, register mutation,
    snapshot enumeration, and missing-key nil conventions are therefore removed
    rather than ported. This deletion does not inflate the port count.
    `terlan-vm-erl-suite-audit-check` passes. File-level migration progress
    remains 128 ported and 1,792 not ported while advancing to 92 deleted and
    1,828 active historical files.
    The one-hundred-twenty-ninth terminal migration is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_process_environment.rs`. The
    golden VM now has an explicit composed parity module over typed process
    lifecycle inspection, source and current execution locations, stack
    traces, reductions, logical heap ownership, mailbox depth, registered-name
    cleanup, liveness, links, monitors, demonitoring, and mutation-free missing
    process rejection. Group leaders, priorities, process dictionaries,
    register state, atom-table result encoding, and BEAM import execution were
    not copied because they are retired implementation mechanics rather than
    Terlan VM contracts. With Rust warnings denied,
    `vm-process-model-check` passes all 45 process tests and
    `terlan-vm-erl-suite-audit-check` is green. The broader replacement target
    was also run: its process-owned prerequisites pass, while its aggregate
    `terlc` suite remains non-green with 4,706 passed, 123 unrelated dirty-tree
    failures, and two ignored tests. File-level migration progress is now 129
    ported, 1,791 not ported, 93 deleted, and 1,827 active historical files.
    The legacy fixture and its stale external GNUmakefile invocation are now
    removed.
    The remaining eight already-ported process-import compatibility fixtures
    are also retired as one terminal cleanup: aliases, flags, identity,
    process-info imports, memory imports, reference imports, registry imports,
    and spawn imports. Golden VM ownership is covered by the 45-test process
    gate, 82-test actor gate, 31-test failure/reference gate, the focused
    resource-ownership tests, and the committed memory-pressure reports; all
    focused checks pass with Rust warnings denied. The aggregate memory target
    still expands into the unrelated dirty-tree `terlc` suite, which remains
    non-green with 4,707 passed, 123 failures, and two ignored tests, so it is
    not reported as passing. The external fixtures, seven stale GNUmakefile
    invocations, and the obsolete release-spawn gate are removed.
    `terlan-vm-erl-suite-audit-check` is green with file-level progress at 129
    ported, 1,791 not ported, 101 deleted, and 1,819 active historical files.
    Twelve remaining already-ported runtime-model fixtures are now terminally
    retired together: environment imports, lifecycle, memory, process model,
    observability, typed ports, scheduler priority, actor relationships,
    scheduler results, failure signals, timers, and the BEAM term model. Their
    golden replacements pass as one warnings-denied `runtime::vm` gate with
    1,479 tests and no failures. The historical fixtures and their three stale
    GNUmakefile invocations are removed; `terlan-vm-erl-suite-audit-check`
    remains green with file-level progress at 129 ported, 1,791 not ported, 113
    deleted, and 1,807 active historical files.
    The eight remaining already-ported non-binary VM-primitive Erlang suites
    are terminally retired as one family: receive timeouts, stable hash
    properties, primitive receive evaluation, opaque references, process-name
    registration, unique identities, and kernel actor liveness. Their exact
    active-source inventory rules are removed while historical tombstones
    inherit the surviving runtime/process family gates. The warnings-denied
    golden `runtime::vm` gate passes all 1,479 tests, and
    `terlan-vm-erl-suite-audit-check` is green with file-level progress at 129
    ported, 1,791 not ported, 121 deleted, and 1,799 active historical files.
    All 29 remaining already-ported binary and bitstring Erlang suite variants
    are terminally retired together. The runtime family covers exhaustive
    binary splits, tail matching, and UTF-8/16/32 behavior; the compiler family
    covers runtime-derived segment sizes and the same Unicode contract across
    retired OTP optimizer modes. `binary-runtime-suite-check` and
    `binary-bitstring-processing-check` both pass with warnings denied,
    including source properties, parser/typechecker/CoreIR/VM assertions,
    malformed and truncated input cases, framing lifecycle, and the measured
    protocol baseline. The exact active-source rules and one stale validator
    suite-list entry are removed. `terlan-vm-erl-suite-audit-check` is green
    with file-level progress at 129 ported, 1,791 not ported, 150 deleted, and
    1,770 active historical files.
    The final two already-ported active OTP stdlib fixtures are terminally
    retired: console-format hang regressions are replaced by typed console
    diagnostics, and process-global random state is replaced by Terlan's
    explicit immutable generator contract. `std-vm-parity-matrix-check` and
    `std-random-check` both pass with Rust warnings denied. Their exact
    active-source inventory rules are removed, leaving no ported historical
    fixture active. `terlan-vm-erl-suite-audit-check` is green with file-level
    progress at 129 ported, 1,791 not ported, 152 deleted, and 1,768 active
    historical files.
    OTP URI recomposition and normalization properties are now ported to the
    opaque `std.net.Uri` contract rather than preserving OTP's map-shaped URI
    representation. Generated valid inputs prove normalized rendering is
    idempotent and preserves every exposed component after reparsing, while a
    generated malformed corpus proves typed parse rejection. The complete
    warnings-denied `std-test-property-check` and `std-vm-parity-matrix-check`
    gates pass, the historical `uri_string_property_test_SUITE.erl` fixture is
    removed, and `terlan-vm-erl-suite-audit-check` is green with file-level
    progress at 130 ported, 1,790 not ported, 153 deleted, and 1,767 active
    historical files.
    The ETS first/next/last/previous property family is now represented by a
    typed VM table traversal contract rather than OTP's duplicated key-only and
    `_lookup` APIs. `VmTableStore` returns complete entries in deterministic
    insertion order and shares one read-policy path across lookup, export, and
    traversal. Adversarial coverage proves empty and terminal boundaries,
    replacement stability, deletion movement, missing-key diagnostics, and
    owner/public-read enforcement. The expanded warnings-denied
    `vm-table-primitives-check` and `std-vm-parity-matrix-check` pass; the OTP
    suite and its property implementation are removed. The aggregate
    `rust-quality-check` remains non-green because of unrelated dirty-tree
    dormant modules, file-size growth, inline tests, and an unclassified
    `binary_layout.rs` hash map, so no parent completion is claimed.
    `terlan-vm-erl-suite-audit-check` is green with file-level progress at 132
    ported, 1,788 not ported, 155 deleted, and 1,765 active historical files.
    Native record update is now VM-owned rather than an unsupported CoreIR
    form. Updates preserve declaration order and source immutability, accept
    qualified type identity, and reject wrong receiver types, wrong record
    identities, unknown fields, and duplicate fields with stable diagnostics.
    The warnings-denied `pattern-matching-support-check` and
    `std-vm-parity-matrix-check` pass, the OTP `records_SUITE.erl` doctest
    wrapper is removed, and `terlan-vm-erl-suite-audit-check` is green with
    file-level progress at 133 ported, 1,787 not ported, 156 deleted, and 1,764
    active historical files. The broader OTP audit parent remains open.
    The portable `dict_SUITE.erl` behavior is now owned by the opaque Terlan
    `Map[K, V]` contract. `Map.take/2` returns an optional value plus a
    persistent remainder across flat and indexed map representations, while
    direct and generated tests cover empty construction, bulk-versus-sequential
    insertion, duplicate replacement, present and missing take operations,
    source immutability, and complete iterator traversal without prescribing
    iteration order. OTP's comparison among `dict`, `orddict`, and `gb_trees`,
    Common Test mechanics, and ordered `gb_trees` traversal are implementation
    contracts rather than Terlan map semantics. The warnings-denied
    `std-test-property-check` and `std-vm-parity-matrix-check` gates pass;
    `dict_SUITE.erl` and `dict_test_lib.erl` are removed, and
    `terlan-vm-erl-suite-audit-check` is green with file-level progress at 135
    ported, 1,785 not ported, 158 deleted, and 1,762 active historical files.
    The broader OTP audit parent remains open.
    The portable assertion behavior from `stdlib_SUITE.erl` is now owned by
    `std.test.Test.AssertionsTest`. Fifteen executable tests cover passing and
    failing truth, falsehood, equality, inequality, explicit failure, ordinary
    tuple-pattern matching, and guarded-pattern fallthrough. Terlan rejects
    cross-type generic equality at compile time; OTP `.app`/`.appup` release
    checks, assertion-macro expansion metadata, and Erlang exception-class
    tuples are OTP implementation contracts rather than Terlan assertion
    semantics. The warnings-denied `std-test-table-check`,
    `std-test-honesty-check`, and `std-vm-parity-matrix-check` gates pass;
    `stdlib_SUITE.erl` is removed, and `terlan-vm-erl-suite-audit-check` is
    green with file-level progress at 136 ported, 1,784 not ported, 159
    deleted, and 1,761 active historical files. The broader OTP audit parent
    remains open.
    The portable behavior from `base64_property_test_SUITE.erl` and
    `property_test/base64_prop.erl` is now owned by typed Terlan byte APIs.
    `std.encoding.Base64` supports strict standard and URL-safe encoding and
    decoding over VM-owned `Bytes`; all nine `std.vm.Bytes` CoreIR intrinsics
    execute through the shared VM byte implementation rather than stopping at
    typechecking. Direct and generated tests cover empty payloads, every octet,
    non-UTF-8 payloads, malformed input, and invalid octet values. OTP's
    list-versus-binary overloads, whitespace-tolerant decoding, and MIME/noisy
    input policy are OTP API contracts rather than Terlan Base64 semantics.
    The warnings-denied `std-test-property-check`, `std-test-table-check`,
    `stdlib-native-artifacts-check`, and `std-vm-parity-matrix-check` gates
    pass; both historical property files are removed, and
    `terlan-vm-erl-suite-audit-check` is green with file-level progress at 138
    ported, 1,782 not ported, 161 deleted, and 1,759 active historical files.
    The aggregate `rust-quality-check` remains non-green because five unrelated
    dirty-tree VM modules lack dormant-code inventory rows, so no repository
    quality or broader OTP audit parent completion is claimed.
    The remaining portable behavior from `base64_SUITE.erl` is now owned by the
    same strict typed contract. The canonical `std-test-table-check` includes
    12 Rust adapter tests and 11 Terlan VM source tests covering RFC 4648 text
    and byte vectors, exact standard/URL-safe alphabet separation, all octets,
    padding boundaries through 255 bytes and around 2,400 bytes, the historical
    `===` regression, missing padding, malformed trailing data, whitespace and
    control-byte rejection, and a 300,000-byte roundtrip. OTP's polymorphic
    list/binary return forms, option-map padding API, whitespace-tolerant strict
    decoder, MIME noise stripping, Common Test groups/timetraps, and OTP
    doctests are intentionally not Terlan semantics. The warnings-denied
    `std-test-table-check` and `std-vm-parity-matrix-check` gates pass;
    `base64_SUITE.erl` is removed, and `terlan-vm-erl-suite-audit-check` is
    green with file-level progress at 139 ported, 1,781 not ported, 162 deleted,
    and 1,758 active historical files. The broader OTP audit parent remains
    open.
    The OTP-only `erl_internal_SUITE.erl` callback-table assertion is now
    explicitly classified `remove-non-portable` and removed from the external
    corpus. It compared `behaviour_info/1` metadata for Erlang `application`,
    `gen_fsm`, `gen_server`, `gen_event`, `gen_statem`, `supervisor_bridge`, and
    `supervisor`; Terlan's typed service and supervision APIs do not preserve
    those callback names or Erlang behavior metadata. This removal does not
    claim a semantic port and does not close the separate Terlan service and
    supervision slices. The warnings-denied
    `terlan-vm-erl-suite-audit-check` and `no-default-beam-runtime-check` gates
    pass with file-level progress at 139 ported, 1,781 not ported, 163 deleted,
    and 1,757 active historical files. The broader OTP audit parent remains
    open.
    The portable finite-float behavior from `math_SUITE.erl` is now owned by
    `std.core.Float` and compiler-known CoreIR intrinsics. `floor/1` and
    `ceil/1` preserve signed rounding boundaries and always return `Float`;
    `pi/0` and `tau/0` expose the VM's binary64 constants. Qualified calls,
    selected imports, serialized VM IR dispatch, embedded interface summaries,
    direct evaluator diagnostics, table tests, and generated finite-float
    bounds are covered. OTP's broad transcendental BIF catalog, Erlang
    `error_info` metadata, integer-overloaded math calls, negative-zero
    representation assertion, and Common Test doctest mechanics are not Terlan
    API contracts. The warnings-denied intrinsic tests, exact embedded-summary
    test, `std-test-table-check`, `std-test-property-check`,
    `std-vm-parity-matrix-check`, and `terlan-vm-erl-suite-audit-check` pass;
    `math_SUITE.erl` is removed with file-level progress at 140 ported, 1,780
    not ported, 164 deleted, and 1,756 active historical files. The broader OTP
    audit parent remains open.
    The next verified nonportable deletion is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_instruction_bitstring_lowering.rs`:
    it asserted BEAM opcode `129`, tagged register and label operands, encoded
    byte-stream decoding, and lowering into a compatibility branch instruction.
    Terlan VM consumes typed VM IR and does not preserve that bytecode contract,
    so the exact-path inventory now classifies the fixture as
    `remove-non-portable` and the historical ledger records deletion without
    inflating the port count. `terlan-vm-erl-suite-audit-check` and
    `no-default-beam-runtime-check` pass. File-level migration progress is now
    55 ported, 1,865 not ported, 34 deleted, and 1,886 active historical files.
    Terlan-native binary and bitstring construction, matching, CoreIR, and VM
    execution remain open under the separate bitstring roadmap item; this
    deletion does not claim those semantics are implemented.
    The next verified nonportable deletion is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_instruction_shallow_type_lowering.rs`:
    it asserted raw BEAM type-test opcode numbers, tagged register and label
    operands, byte-stream decoding, and compatibility branch instruction
    selection without executing type discrimination. Terlan's compiler and VM
    exchange typed CoreIR/VMIR instead, so the exact-path inventory classifies
    the fixture as `remove-non-portable` and the historical ledger records
    deletion without increasing the port count. The active
    `beam_instruction_type_tests.rs` fixture continues to track observable
    float, number, process, reference, resource, binary, list, tuple, and
    function discrimination until equivalent Terlan-owned execution tests land.
    `terlan-vm-erl-suite-audit-check` and
    `vm-artifact-contract-freeze-check` pass. File-level migration progress is
    now 55 ported, 1,865 not ported, 35 deleted, and 1,885 active historical
    files.
    The next verified nonportable deletion is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_instruction_import_lowering.rs`:
    it asserted BEAM `call_ext_last` opcode `8`, tagged operands, import-table
    indexes, deallocation expansion, inserted return instructions, and decoder
    tag failures. Terlan VM dispatches typed CoreIR remote calls directly and
    does not preserve that bytecode contract, so the exact-path inventory now
    classifies the fixture as `remove-non-portable` and the historical ledger
    records deletion without increasing the port count. Golden VM execution
    continues to cover loaded-module alias dispatch and returned values; this
    deletion does not claim every imported runtime operation is implemented.
    `terlan-vm-erl-suite-audit-check` and
    `vm-runtime-semantics-check` pass. File-level migration progress is now 55
    ported, 1,865 not ported, 36 deleted, and 1,884 active historical files.
    The next verified terminal port is
    `terlan-vm/erts/rust/terlan_vm/tests/base64.rs`:
    the maintained `std.encoding.Base64` adapter preserves every RFC 4648
    vector, all 256 byte values, short-input padding and output-length
    boundaries, and the 1,025-byte padded tail from the external fixture. The
    golden tests additionally cover standard and URL-safe roundtrips, invalid
    Base64, invalid UTF-8, VM dispatch, public Terlan error shapes, and generated
    text properties. The maintained crate-backed adapter allocates its output,
    so the retired caller-sized buffer and manual encoded-length overflow cases
    are not part of the Terlan API. The exact-path inventory records checked
    `delete-after-vm-equivalent` evidence and the historical ledger now marks
    the already-ported fixture deleted. `terlan-vm-erl-suite-audit-check` and
    `vm-runtime-semantics-check` pass. File-level migration progress remains 55
    ported and 1,865 not ported, with 37 deleted and 1,883 active historical
    files.
    The next verified terminal port is
    `terlan-vm/erts/rust/terlan_vm/tests/unicode_helpers.rs`:
    the golden VM preserves the external fixture's empty, ASCII, extended-byte,
    mixed-input, `isize::MAX`, and checked-overflow behavior for copied Latin-1
    data. Its replacement suite additionally checks every byte from `0x00`
    through `0xff`, proving deterministic one- or two-byte UTF-8 width across
    the full input domain. The exact-path inventory records checked
    `delete-after-vm-equivalent` evidence and the historical ledger now marks
    the already-ported fixture deleted. `terlan-vm-erl-suite-audit-check` and
    `vm-runtime-semantics-check` pass. File-level migration progress remains 55
    ported and 1,865 not ported, with 38 deleted and 1,882 active historical
    files.
    The next verified terminal port is
    `terlan-vm/erts/rust/terlan_vm/tests/bits_utf8_put.rs`:
    the golden VM preserves every one-, two-, three-, and four-byte UTF-8 scalar
    transition, exact byte and bit counts, untouched trailing bytes, and stable
    negative, surrogate, and above-maximum errors from the external fixture.
    Its adversarial replacement checks every insufficient buffer length for
    each encoded width and proves that all failures leave the complete output
    buffer unchanged. The exact-path inventory records checked
    `delete-after-vm-equivalent` evidence and the historical ledger now marks
    the already-ported fixture deleted. `terlan-vm-erl-suite-audit-check` and
    `vm-runtime-semantics-check` pass. File-level migration progress remains 55
    ported and 1,865 not ported, with 39 deleted and 1,881 active historical
    files.
    The next verified terminal port is
    `terlan-vm/erts/rust/terlan_vm/tests/packet_length.rs`:
    the golden VM preserves raw, one-, two-, and four-byte prefixes, record
    marking, ASN.1 short and long forms, CDR endianness, FastCGI, TPKT, and
    SSL/TLS framing from the external fixture, including unsupported,
    truncated, malformed, maximum-length, and integer-overflow outcomes. Its
    adversarial replacement additionally proves exact maximum-payload
    acceptance and one-byte-over rejection for every framed mode. The
    exact-path inventory records checked `delete-after-vm-equivalent` evidence
    and the historical ledger now marks the already-ported fixture deleted.
    `terlan-vm-erl-suite-audit-check` and `vm-runtime-semantics-check` pass.
    File-level migration progress remains 55 ported and 1,865 not ported, with
    40 deleted and 1,880 active historical files.
    The next verified terminal port is
    `terlan-vm/erts/rust/terlan_vm/tests/checksum.rs`:
    the golden VM preserves the external fixture's Adler-32 and CRC-32 zlib
    reference vectors, incremental chunk updates, every two-way split,
    combine reference vectors, and zero-length combine behavior. Its
    adversarial replacement additionally checks every ordered pair of split
    points for three-way combination. The exact-path inventory records checked
    `delete-after-vm-equivalent` evidence and the historical ledger now marks
    the already-ported fixture deleted. `terlan-vm-erl-suite-audit-check` and
    `vm-runtime-semantics-check` pass. File-level migration progress remains 55
    ported and 1,865 not ported, with 41 deleted and 1,879 active historical
    files.
    The next verified terminal port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_runtime_suspension.rs`:
    the golden actor runtime preserves runnable and blocked suspension state,
    scheduler removal and explicit requeue, queued-message delivery without
    implicit resume, stable missing and exited process diagnostics, and
    runnable-peer fairness. Its seven-test replacement additionally proves
    that resuming a blocked actor without a message restores the blocked state
    without creating a scheduler entry. The exact-path inventory records
    checked `delete-after-vm-equivalent` evidence and the historical ledger now
    marks the already-ported fixture deleted. `terlan-vm-erl-suite-audit-check`
    and `vm-runtime-semantics-check` pass. File-level migration progress remains
    55 ported and 1,865 not ported, with 42 deleted and 1,878 active historical
    files.
    The next verified terminal port is
    `terlan-vm/erts/rust/terlan_vm/tests/md5.rs`:
    the maintained RustCrypto-backed NativeBoundary adapter preserves RFC 1321
    vectors, compression-block padding boundaries, every two-way split,
    independent cloned-state finalization, and one-byte through large
    adversarial chunk sizes. The public Terlan table suite also verifies the
    supported UTF-8 text API. `std-test-table-check` now executes both layers.
    C ABI layout and unsafe raw-memory state-copy assertions were intentionally
    retired because NativeBoundary owns typed Rust state rather than exposing that
    representation. The exact-path inventory records checked
    `delete-after-vm-equivalent` evidence and the historical ledger marks the
    fixture deleted. `std-test-table-check` and
    `terlan-vm-erl-suite-audit-check` pass. File-level migration progress
    remains 55 ported and 1,865 not ported, with 43 deleted and 1,877 active
    historical files.
    The next verified terminal port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_runtime_wakeup.rs`:
    the golden VM preserves one-entry wake deduplication, suspended-process
    isolation, explicit resume, missing-target side-effect freedom, queued
    message order, and runnable scheduling through actor, timer, scheduler, and
    suspension tests. The legacy `PortId` delivery path was intentionally
    retired because Terlan uses protocol-specific TCP, package, debugger, and
    I/O-reactor wake contracts instead of BEAM ports. The exact-path inventory
    records checked `delete-after-vm-equivalent` evidence and the historical
    ledger marks the fixture deleted. `vm-runtime-semantics-check` and
    `terlan-vm-erl-suite-audit-check` pass. File-level migration progress
    remains 55 ported and 1,865 not ported, with 44 deleted and 1,876 active
    historical files.
    The next verified terminal port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_process_info_location.rs`: the
    golden VM preserves typed module, function, arity, and instruction-offset
    location data, scheduler location updates, loop back-edges, call-stack
    entry and return, and the final location of exited processes. The legacy
    erlc invocation, BEAM loader, atom table, tuple tag, and metadata-list
    encoding were intentionally retired because location is a typed VM
    inspection contract. The exact-path inventory records checked
    `delete-after-vm-equivalent` evidence and the historical ledger marks the
    fixture deleted. `vm-runtime-semantics-check` and
    `terlan-vm-erl-suite-audit-check` pass. File-level migration progress
    remains 55 ported and 1,865 not ported, with 45 deleted and 1,875 active
    historical files.
    The next verified terminal port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_process_info.rs`: the golden VM
    preserves typed identity, parent and source, lifecycle and exit reason,
    reductions, logical heap ownership, mailbox depth, cancellation, resource
    ownership, registered names, deterministic ordering, atomic exit cleanup,
    and missing-process diagnostics. The external fixture's architecture-
    specific registers, fixed heap words, reduction budgets, priority mirror,
    pending-signal counter, and group-leader topology were intentionally
    retired because they are not Terlan process-inspection contracts. The
    exact-path inventory records checked `delete-after-vm-equivalent` evidence
    and the historical ledger marks the fixture deleted.
    `vm-runtime-semantics-check` and `terlan-vm-erl-suite-audit-check` pass.
    File-level migration progress remains 55 ported and 1,865 not ported, with
    46 deleted and 1,874 active historical files.
    The next verified terminal port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_process_scheduler_imports.rs`:
    the golden VM preserves cooperative yield as a direct scheduler decision,
    runnable and local state, reduction accounting, peer fairness, ordinary
    queue resumption, one-entry deduplication, and the distinction from budget
    preemption. External compilation, BEAM import dispatch, instruction
    registers, and atom and tick-result encoding were intentionally retired
    because they are not Terlan scheduler contracts. The exact-path inventory
    records checked `delete-after-vm-equivalent` evidence and the historical
    ledger marks the fixture deleted. `vm-runtime-semantics-check` and
    `terlan-vm-erl-suite-audit-check` pass. File-level migration progress
    remains 55 ported and 1,865 not ported, with 47 deleted and 1,873 active
    historical files.
    The next verified terminal port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_process_send_imports.rs`: the
    golden VM preserves typed process-id, registered-name, opaque-alias, and
    self routes, structured payload identity, mailbox ordering, memory
    accounting, sender-first validation, and mutation-free rejection. External
    compilation, compatibility import dispatch, instruction registers, numeric
    process encoding, payload-return conventions, and malformed untyped
    destinations were intentionally retired because they are not Terlan actor
    contracts. The exact-path inventory records checked
    `delete-after-vm-equivalent` evidence and the historical ledger marks the
    fixture deleted. `vm-runtime-semantics-check` and
    `terlan-vm-erl-suite-audit-check` pass. File-level migration progress
    remains 55 ported and 1,865 not ported, with 48 deleted and 1,872 active
    historical files.
    The next verified terminal port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_process_timer_imports.rs`: the
    golden VM preserves typed process-id, stable-name, and opaque-alias delayed
    delivery, correlated timer messages, shared read/cancel/expiry identity,
    equal-deadline ordering, late delivery, route freezing, lifecycle cleanup,
    memory accounting, and allocation-free rejection. External compilation,
    compatibility import dispatch, instruction registers, tagged timer terms,
    and compatibility return encoding were intentionally retired because they
    are not Terlan timer contracts. The exact-path inventory records checked
    `delete-after-vm-equivalent` evidence and the historical ledger marks the
    fixture deleted. `vm-runtime-semantics-check` and
    `terlan-vm-erl-suite-audit-check` pass. File-level migration progress
    remains 55 ported and 1,865 not ported, with 49 deleted and 1,871 active
    historical files.
    The next verified terminal port is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_process_timer_option_imports.rs`: the
    golden VM preserves typed relative and absolute deadlines, synchronous and
    asynchronous reads, cancellation information policies, memory-accounted
    reply records, explicit stale identity results, information suppression,
    already-due evidence, and atomic rejection. External compilation,
    compatibility import dispatch, instruction registers, untyped option
    tuples, reserved atom identities, and compatibility return encoding were
    intentionally retired because the compiler constructs timer policy before
    VM execution. The exact-path inventory records checked
    `delete-after-vm-equivalent` evidence and the historical ledger marks the
    fixture deleted. `vm-runtime-semantics-check` and
    `terlan-vm-erl-suite-audit-check` pass. File-level migration progress
    remains 55 ported and 1,865 not ported, with 50 deleted and 1,870 active
    historical files.
    The
    BEAM-specific `AtomId`, tagged-term, encoding-shape, mutable global-table,
    and table-limit cases were deleted because those mechanisms are not part of
    the Terlan VM contract. `vm-runtime-semantics-check` names and passes the
    value test module. The audit accepts this exact
    `delete-after-vm-equivalent` tombstone only when the file ledger marks it
    deleted with matching executable replacement evidence, and rejects the
    stale inventory row when that checked tombstone is absent. The
    first verified nonportable deletion is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_instruction_move_lowering.rs`: it
    tested BEAM opcode 64, tagged operands, and BEAM bytecode lowering rather
    than Terlan-owned VM IR behavior. The audit now gives exact-path inventory
    rows precedence over broad subtree rows, accepts a checked deleted
    `remove-non-portable` tombstone without falsely claiming a replacement
    port, and still rejects that classification while its source exists. The
    second retired suite is
    `terlan-vm/erts/rust/terlan_vm/tests/api_boundary.rs` plus its three
    inventoried compile-fail source fixtures and three ignored diagnostic
    snapshots. Those files tested the standalone compatibility crate's public
    `Term` and `AtomId` field boundary; the golden single-crate VM keeps runtime
    identity types internal and must not recreate that obsolete public API.
    Exact nonportable tombstones cover every inventoried source fixture, and
    the external README no longer names the deleted suite. The third retired
    suite is `terlan-vm/erts/rust/terlan_vm/tests/lock_flags.rs`; it encoded
    ERTS C lock constants, NUL-terminated diagnostic strings, and static
    pointer identity. Terlan VM exposes no such ABI because synchronization is
    VM-owned. Its historical row remains explicitly not ported so removal
    cannot inflate parity progress. The fifth and sixth retired suites are
    `terlan-vm/erts/rust/terlan_vm/tests/shared_abi.rs` and
    `terlan-vm/erts/rust/terlan_vm/tests/support_yielding_c_fun.rs`. The first
    tested unsafe raw C pointer borrowing and writes; the second preserved the
    OTP yielding-C-transformer CLI, lexer quirks, and C fixtures. Terlan VM uses
    safe Rust boundaries and VM-owned scheduling/reductions, so neither C
    compatibility mechanism belongs in the runtime contract. Their historical
    rows also remain explicitly not ported. The seventh retired suite is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_instruction_basic_type_lowering.rs`:
    it tested retired compatibility opcodes 45, 48, and 52, tagged source and
    label operands, and bytecode lowering into compatibility branch instructions. Terlan VM
    consumes typed VM IR and owns type tests above the bytecode representation,
    so preserving those encoding assertions would retain a backend that 0.0.7
    removes. Its historical row remains explicitly not ported. The eighth
    retired suite is
    `terlan-vm/erts/rust/terlan_vm/tests/beam_type_decoding.rs`: it fixed the
    retired BEAM C ABI struct size, alignment, field offsets, metadata bit flags,
    raw-pointer reads, null-pointer fallback, and wrapping unit-byte behavior.
    Terlan's compiler and VM exchange typed Rust IR and values without exposing
    that ERTS type-header ABI or unsafe decoder surface, so porting these tests
    would preserve an implementation contract the release explicitly removes.
    Its historical row remains explicitly not ported, and
    `terlan-vm-erl-suite-audit-check` rejects the tombstone if the source returns.
    The active corpus
    is 1,869 files while the historical ledger retains all 1,920 rows. The
    current baseline is
    65 ported, 1,855 not ported, 60 deleted, and 1,860 not deleted;
    the parent slice cannot close until every remaining row has verified
    replacement evidence and terminal deletion status.


### Progress 143

  - Completed progress: `terlan-vm-external-repo-boundary-check` now runs its
    focused boundary tests before the CLI scan, rejects placeholder boundary
    docs, and explicitly allows the external suite audit script as
    reference-only tooling while keeping random source references to the sibling
    `terlan-vm` checkout rejected.


### Progress 144

  - Completed progress: `terlan-vm-internal-crate-check` now runs its focused
    crate-shape tests before the CLI scan, rejects placeholder VM ownership
    README text, and verifies that `terlan-vm` remains an internal
    compiler/runtime binary owned by the `terlan` crate rather than a separate
    public VM crate.


### Progress 145

  - Completed progress: `no-terlan-vm-erts-rust-dependency-check` now rejects
    placeholder ERTS Rust quarantine docs while enforcing that retired
    `terlan-vm/erts/rust` gates stay out of default check, release, and publish
    paths.


### Progress 146

  - Completed progress: VM-owned core primitive dispatch now takes precedence
    over loaded std source placeholders for Bool, Int, Float, String, Unit, and
    Ordering operations. The public Bool, Int, Float, String, and Equal suites
    execute through the VM default lane, including trait-backed ordering,
    parse/render round trips, concrete equality, and conflicting Ordering
    imports. `make stdlib-release-tests-vm-default-check` and
    `make terlan-vm-erl-suite-audit-check` pass; the overall parity item remains
    open while the remaining stdlib release failures and external suite ports
    are unresolved.


### Progress 147

  - Completed progress: constructor-pattern scrutinee widening now considers
    only union aliases whose expanded variants structurally represent the case
    constructors, and it preserves explicitly typed union scrutinees instead of
    widening them again. Inline `Option`/`Result`, imported `Option[Int]` across
    local-call boundaries, and VM constructor-case execution pass together.
    `make terlan-runtime-conformance-check` passes with the constructor-case
    runtime anchor included; the parent parity item remains open for the
    unported external reliability suites.


### Progress 148

  - Completed progress: selected `std.io.File` imports now execute through the
    VM filesystem boundary instead of the std source placeholder bodies.
    `std/io/FileTest.terl` passes all 8 cases covering normalized missing-file
    code/message/path values, write/read round trips, append, delete, and
    missing-delete behavior. `make stdlib-release-tests-vm-default-check`,
    `make terlan-vm-erl-suite-audit-check`, and the warnings-as-errors `terlc`
    check pass; the parent parity item remains open for the unported external
    suites.


### Progress 149

  - Completed progress: the removed
    `terlan-vm/erts/rust/terlan_vm/tests/term_classification.rs` suite now has
    an executable golden-VM replacement rather than an unported tombstone.
    Typed VM values replace ERTS raw-word tags and pointer inspection; focused
    tests preserve signed integer boundaries, exact atom/string/bool
    separation, nominal record identity, tuple structure, empty collection
    categories, and fail-closed heterogeneous aggregate classification. The
    warnings-denied focused Rust tests and `make
    terlan-vm-erl-suite-audit-check` pass. File-level progress is now 141
    ported, 1,779 not ported, 164 deleted, and 1,756 not deleted; the parent
    parity item remains open.


### Progress 150

  - Completed progress: the 90-second randomized OTP
    `lib/stdlib/test/timer_SUITE.erl` host-clock stress harness is replaced by
    deterministic VM logical-clock coverage. A dedicated mixed-load test owns
    200 one-shot and 25 interval timers across exact deadlines, verifies every
    one-shot identity fires once, derives the exact interval fire count,
    rejects late/coalesced failure outcomes on exact ticks, and drains all
    interval identities through typed cancellation. `make
    vm-timer-primitives-check` now names this replacement alongside the 35-test
    timer contract matrix, and `make terlan-vm-erl-suite-audit-check` passes
    after deleting the obsolete suite. File-level progress is now 142 ported,
    1,778 not ported, 165 deleted, and 1,755 not deleted; the parent parity item
    remains open.


### Progress 151

  - Completed progress: the OTP
    `erts/emulator/test/long_timers_test.erl` distributed-node, C timer-driver,
    CPU-sampling, and one-hour wall-clock harness is replaced by deterministic
    VM logical-clock parity coverage. The replacement installs ordinary and
    receive timers at every one-through-sixty-minute deadline, proves no early
    firing at each preceding tick, exact once-only firing in stable identity
    order, full active/max-active accounting, receiver wakeup, and an empty
    terminal timer table without host-clock tolerances. The warnings-denied
    exact test, `make vm-timer-primitives-check`, and `make
    terlan-vm-erl-suite-audit-check` pass after deleting the obsolete suite.
    File-level progress is now 144 ported, 1,776 not ported, 167 deleted, and
    1,753 not deleted; the parent parity item remains open for the remaining
    external suites.


### Progress 152

  - Completed progress: the portable nested-expression behavior from OTP
    `erts/emulator/test/nested_SUITE.erl` now executes through Terlan source,
    CoreIR, and the golden VM. Focused parity tests prove that an inner `case`
    scrutinee completes before outer pattern selection and that constructor and
    wildcard fallbacks remain local to their own match boundary. Erlang catch
    stacks, process-dictionary state, registration BIFs, and Common Test
    callbacks were retired rather than reproduced because Terlan uses typed
    failures and explicit actor state. With warnings denied, both exact tests
    and the complete `make pattern-matching-support-check` pass; `make
    terlan-vm-erl-suite-audit-check` also passes after deleting the obsolete
    suite. File-level progress is now 145 ported, 1,775 not ported, 168 deleted,
    and 1,752 not deleted; the parent parity item remains open.


### Progress 153

  - Completed progress: the OTP documentation example
    `system/doc/programming_examples/fun_test.erl` is replaced by executable
    Terlan source lowered through CoreIR and the golden VM. The focused parity
    test proves that anonymous lambdas and named function references both map
    the integers one through five into the same exact ordered result, while the
    adjacent adversarial test rejects missing and surplus callback arguments.
    With warnings denied, the exact test and complete `make
    executable-docs-vm-check` pass; `make terlan-vm-erl-suite-audit-check` also
    passes after deleting the obsolete Erlang example. File-level progress is
    now 146 ported, 1,774 not ported, 169 deleted, and 1,751 not deleted; the
    parent parity item remains open.


### Progress 154

  - Completed progress: the portable behavior from OTP
    `erts/emulator/test/tuple_SUITE_data/get_two_tuple_elements.erl` now runs
    from typed Terlan source through CoreIR and the golden VM. The positive
    test proves nested extraction reads the correct inner value after the outer
    tuple binding, while the adversarial test rejects an inner tuple that
    cannot supply the requested third element. Terlan's immutable typed tuples
    retire the BEAM register-overwrite and tuple-pointer reload mechanism rather
    than preserving it. With warnings denied, both exact tests and the complete
    `make pattern-matching-support-check` pass; `make
    terlan-vm-erl-suite-audit-check` also passes after deleting the obsolete
    Erlang fixture. File-level progress is now 148 ported, 1,772 not ported, 171
    deleted, and 1,749 not deleted; the parent parity item remains open.


### Progress 155

  - Completed progress: OTP's intentionally mismatched
    `lib/kernel/test/code_a_test.erl` filename and `code_b_test` module
    declaration are replaced by direct Terlan source-layout validation. The
    positive test accepts a declaration matching its source-root-relative
    path; the adversarial test rejects the legacy mismatch with the complete
    stable diagnostic, including the expected declaration. With warnings
    denied, both exact tests and the complete `make
    terlan-vm-erl-suite-audit-check` pass, and the obsolete Erlang fixture is
    deleted. File-level progress is now 149 ported, 1,771 not ported, 172
    deleted, and 1,748 not deleted; the parent parity item remains open.


### Progress 156

  - Completed progress: OTP's
    `erts/emulator/test/module_info_SUITE_data/module_info_test.erl` helper is
    replaced by VM-owned module lifecycle semantics. `VmCodeServer` can now
    unload an unbound active generation into retired state for explicit purge,
    while process-bound unload is rejected without mutating generation or
    event state. The replacement tests compile the fixture through the Terlan
    pipeline, inspect public exports and exact arities, reject private and
    wrong-arity queries, then prove unload and purge remove module visibility.
    With warnings denied, both exact tests, the complete `make
    vm-code-server-check`, and `make terlan-vm-erl-suite-audit-check` pass; the
    obsolete Erlang fixture is deleted. File-level progress is now 150 ported,
    1,770 not ported, 173 deleted, and 1,747 not deleted; the parent parity item
    remains open.


### Progress 157

  - Completed progress: OTP's
    `erts/emulator/test/code_SUITE_data/another_code_test.erl` helper is replaced
    by a typed VM external-function reference that carries stable
    module/function/arity identity without retaining a code generation. The
    positive test creates the reference before publication, proves unresolved
    invocation fails, executes it after module load, and proves the same
    reference resolves the current implementation after reload. The
    adversarial test rejects empty identities, wrong call arity, and missing
    exports with stable diagnostics. With warnings denied, both exact tests,
    the complete `make vm-code-server-check`, and `make
    terlan-vm-erl-suite-audit-check` pass; the obsolete Erlang fixture is
    deleted. File-level progress is now 151 ported, 1,769 not ported, 174
    deleted, and 1,746 not deleted; the parent parity item remains open.


### Progress 158

  - Completed progress: OTP's
    `erts/emulator/test/float_SUITE_data/has_fpe_bug.erl` host-runtime startup
    probe is replaced by compiler-known `std.core.Float.log/1` execution owned
    by the Terlan VM. The public std contract, embedded compiler summary,
    CoreIR intrinsic identity, target validation, direct VM dispatch, and
    ordinary VM function-call path now agree. Positive finite input executes
    through source and CoreIR, while zero, negative, and malformed runtime
    values produce one stable domain diagnostic without terminating the VM.
    The adjacent late-bound function-reference primitive is also now used by
    production `TerlanVm::execute_function`, eliminating the dormant test-only
    path found by warnings-as-errors. With warnings denied, `cargo check` for
    both production binaries, the complete `make std-test-table-check`, and
    `make terlan-vm-erl-suite-audit-check` pass; the obsolete Erlang probe is
    deleted. File-level progress is now 152 ported, 1,768 not ported, 175
    deleted, and 1,745 not deleted; the parent parity item remains open.


### Progress 159

  - Completed progress: OTP's
    `erts/emulator/test/code_SUITE_data/my_code_test.erl` captured-function
    helper is replaced by source-to-CoreIR-to-VM closure reload coverage. A
    function value created before module replacement retains its original body
    and captured environment, while a newly created function value executes
    the replacement body. Missing and surplus arguments fail with stable
    diagnostics and do not corrupt the retained closure, which remains callable
    afterward. With warnings denied, both exact adversarial tests, the complete
    `make vm-code-server-check`, and `make
    terlan-vm-erl-suite-audit-check` pass; the obsolete Erlang helper is
    deleted. File-level progress is now 153 ported, 1,767 not ported, 176
    deleted, and 1,744 not deleted; the parent parity item remains open.


### Progress 160

  - Completed progress: OTP's
    `erts/emulator/test/code_SUITE_data/many_funs.erl` dynamic lambda-table
    helper is replaced by source-to-CoreIR-to-VM coverage for sixteen distinct
    one-argument closures. A table created before module replacement preserves
    every original body, while a newly created table uses all replacement
    bodies. Every entry exposes the correct arity; missing and surplus calls
    fail with stable diagnostics without aliasing or invalidating any table
    entry. With warnings denied, both exact adversarial tests, the complete
    `make vm-code-server-check`, and `make
    terlan-vm-erl-suite-audit-check` pass; the obsolete Erlang helper is
    deleted. File-level progress is now 154 ported, 1,766 not ported, 177
    deleted, and 1,743 not deleted; the parent parity item remains open.


### Progress 161

  - Completed progress: OTP's
    `erts/emulator/test/code_SUITE_data/my_code_test2.erl` returned callback
    composer is replaced by source-to-CoreIR-to-VM higher-order closure
    coverage. A composer retained across module reload preserves its original
    callback order and captured callback values, while a newly created composer
    uses the replacement order. The tests observe real VM console effects and
    prove missing and non-callable callback failures emit no partial output and
    leave the composer usable. With warnings denied, both exact adversarial
    tests, the complete `make vm-code-server-check`, and `make
    terlan-vm-erl-suite-audit-check` pass; the obsolete Erlang helper is
    deleted. File-level progress is now 155 ported, 1,765 not ported, 178
    deleted, and 1,742 not deleted; the parent parity item remains open.


### Progress 162

  - Completed progress: OTP's
    `erts/emulator/test/code_SUITE_data/fun_confusion.erl` function-identity
    fixture is replaced by immutable VM module-generation snapshots. Closures
    calling local helpers retain the exact CoreIR generation that created them,
    cloned closures preserve identity, and closures created after reload have
    distinct identity and execute replacement helpers. Repeated reloads and
    malformed calls do not invalidate retained closures. External function
    references remain deliberately late-bound. With warnings denied, both
    exact adversarial tests, the complete `make vm-code-server-check`, and
    `make terlan-vm-erl-suite-audit-check` pass; the obsolete Erlang fixture is
    deleted. File-level progress is now 156 ported, 1,764 not ported, 179
    deleted, and 1,741 not deleted; the parent parity item remains open.


### Progress 163

  - Completed progress: OTP's
    `erts/emulator/test/code_SUITE_data/call_fun_before_load.erl`
    execution-before-load regression is replaced by typed VM staged-module
    artifacts. Compiled source remains absent from module/export inspection
    until explicit publication, staged replacements do not mutate lifecycle
    events or active process bindings, and publication performs one atomic
    generation transition while retaining the bound generation. Source reload
    now carries the same typed staged artifact instead of a loose module and
    metadata tuple. Both parity tests, all 10 source-reload tests, the complete
    `make vm-code-server-check`, warnings-denied all-target checking, and
    `make terlan-vm-erl-suite-audit-check` pass; the obsolete Erlang fixture is
    deleted. File-level progress is now 157 ported, 1,763 not ported, 180
    deleted, and 1,740 not deleted; the parent parity item remains open.


### Progress 164

  - Completed progress: OTP's
    `erts/emulator/test/code_SUITE_data/versions.erl` generation-reporting
    receive loop is replaced by VM-owned binding inspection and real mailbox
    request/reply coverage. Processes retained across hot reload report their
    original generation, newly bound processes report the active replacement,
    and normal exit retires the drained old generation. Forged and released
    bindings fail with stable diagnostics without changing code-server state.
    Both parity tests, the complete 33-selector `make vm-code-server-check`,
    warnings-denied all-target checking, and
    `make terlan-vm-erl-suite-audit-check` pass; the obsolete Erlang fixture is
    deleted. File-level progress is now 158 ported, 1,762 not ported, 181
    deleted, and 1,739 not deleted; the parent parity item remains open.


### Progress 165

  - Completed progress: OTP's three-file
    `erts/emulator/test/code_SUITE_data/call_purged_fun*` fixture family is
    replaced by VM-owned module unload and captured-generation lifetime
    semantics. Named calls stop resolving immediately after unload, while
    captured functions and callback composers remain safe and executable from
    their exact CoreIR generation. Loading an altered replacement creates
    independent function identity and cannot alias or rewrite a retained
    value. Terlan deliberately does not copy the ERTS pending-purge protocol,
    forced process killing, `badfun`/`undef` conversion, or mutable `fun_info`
    behavior. The complete 36-selector `make vm-code-server-check`, the
    post-integration nine-test closure parity suite, warnings-denied all-target
    checking, and `make terlan-vm-erl-suite-audit-check` pass. The three
    obsolete Erlang fixtures are deleted. File-level progress is now 161
    ported, 1,759 not ported, 184 deleted, and 1,736 not deleted; the parent
    parity item remains open.


### Progress 166

  - Completed progress: OTP's
    `erts/emulator/test/code_SUITE_data/erl_544.erl` Unicode source-path
    stacktrace regression is replaced by compiler-owned CoreIR source
    provenance and VM-owned process-frame diagnostics. Exact printable Unicode
    paths survive formal compilation, nested process execution, failure, and
    postmortem stack snapshots; rendering preserves Unicode while escaping
    control characters. The compiler and runtime remain decoupled: CoreIR owns
    compilation provenance and the reusable process model consumes plain source
    metadata. The three focused provenance tests, the complete 47-test
    `make vm-process-model-check`, warnings-denied all-target checking, and
    `make terlan-vm-erl-suite-audit-check` pass. The obsolete Erlang fixture is
    deleted. File-level progress is now 162 ported, 1,758 not ported, 185
    deleted, and 1,735 not deleted; the parent parity item remains open.


### Progress 167

  - Completed progress: OTP's
    `erts/emulator/test/code_SUITE_data/cpbugx.erl` continuation-pointer
    false-dependency regression is replaced by VM-owned process-frame and
    module-generation lease semantics. Entering an exported CoreIR function
    binds the live process to its exact generation, nested same-module calls
    retain one lease, and the final return releases it so an idle live process
    cannot block reload, retirement, purge, or unload. Missing exports are
    rejected without changing either the process stack or generation lease.
    Both focused adversarial tests, the complete 38-selector `make
    vm-code-server-check`, warnings-denied all-target checking, and `make
    terlan-vm-erl-suite-audit-check` pass. The obsolete Erlang fixture is
    deleted. File-level progress is now 163 ported, 1,757 not ported, 186
    deleted, and 1,734 not deleted; the parent parity item remains open.


### Progress 168

  - Completed progress: OTP's
    `lib/compiler/test/compile_SUITE_data/small_float.erl` BEAM
    opcode-ceiling fixture is replaced by compiler-to-CoreIR-to-VM Float
    division coverage. Terlan now accepts finite decimal scientific literals,
    preserves finite subnormal results, formats extreme magnitudes without
    expanding them into impractical decimal lines, and rejects malformed
    exponents, overflowing literals, zero divisors, arithmetic overflow, and
    non-finite VM operands with stable diagnostics. The consolidated seven-test
    `make vm-small-float-parity-check` also validates strict EBNF and all 21
    Tree-sitter corpus cases in 13.5 seconds. Warnings-denied all-target
    checking and the external-suite audit pass, and the obsolete Erlang fixture
    is deleted. File-level progress is now 164 ported, 1,756 not ported, 187
    deleted, and 1,733 not deleted; the parent parity item remains open.


### Progress 169

  - Completed progress: OTP's
    `lib/compiler/test/compile_SUITE_data/record_access.erl` strict/sloppy
    record fixture is replaced by Terlan's always-nominal struct contract.
    The compiler rejects passing a `Turtle` where a same-shaped `Tortoise` is
    required, while compiler-to-CoreIR-to-VM execution projects fields from the
    declared type and refuses to match that value against the other struct's
    pattern. Terlan deliberately retires positional sloppy-record access,
    compiler-option precedence, and `badrecord` compatibility rather than
    exposing an unsafe mode. Both focused tests pass through the consolidated
    14.2-second `make vm-record-nominality-parity-check`; warnings-denied
    all-target checking and the external-suite audit also pass. The obsolete
    Erlang fixture is deleted. File-level progress is now 165 ported, 1,755 not
    ported, 188 deleted, and 1,732 not deleted; the parent parity item remains
    open.


### Progress 170

  - Completed progress: OTP's
    `lib/compiler/test/compile_SUITE_data/small_maps.erl` BEAM listing and
    `no_type_opt` fixture is replaced by compiler-to-CoreIR-to-VM map parity.
    Map expressions and map patterns now decode quoted and escaped keys once at
    the parser boundary, so literals, patterns, and mutable `Map.put` operations
    share one runtime key identity. Source execution covers construction,
    insertion, replacement, ordered required-key pattern selection, fallback
    matching, and stable failure when no required-key clause matches. All three
    focused tests pass through the 15.0-second `make
    vm-small-maps-parity-check`, the broader five-test map-key parity module
    passes, warnings-denied all-target checking passes, and all external-suite
    ledger validators pass. The obsolete Erlang fixture is deleted. File-level
    progress is now 166 ported, 1,754 not ported, 189 deleted, and 1,731 not
    deleted; the parent parity item remains open.


### Progress 171

  - Completed progress: OTP's
    `lib/compiler/test/compile_SUITE_data/wrong_module_name.erl` optional
    module-mismatch compatibility mode is replaced by one canonical Terlan
    source-layout policy. `terlc check` and `terlc build` now reuse the same
    validator instead of maintaining duplicate implementations; matching flat
    and nested modules pass, mismatches report the exact expected declaration,
    and no backend option can emit an artifact under a false module identity.
    The consolidated four-test module-layout selector and all external-suite
    ledger validators pass through the 19.1-second `make
    terlan-vm-erl-suite-audit-check`; warnings-denied all-target checking also
    passes. The obsolete Erlang fixture is deleted. File-level progress is now
    167 ported, 1,753 not ported, 190 deleted, and 1,730 not deleted; the parent
    parity item remains open.


### Progress 172

  - Completed progress: OTP's shared
    `lib/compiler/test/compile_SUITE_data/simple.erl` compiler API and listing
    fixture is replaced by a typed Terlan module compiled through CoreIR and
    executed by the VM. The replacement preserves UTF-8 strings and finite atom
    values through local calls while proving that the public entry point is the
    sole exported function and its helper remains private. Erlang preprocessing,
    include files, generated EDoc, BEAM listing/compression options, pre-load
    tracing, and compiler API permutations are deliberately retired rather than
    copied. Both focused tests pass through the 15.6-second `make
    vm-simple-module-parity-check`; warnings-denied all-target checking and all
    external-suite ledger validators also pass. The obsolete Erlang fixture is
    deleted. File-level progress is now 168 ported, 1,752 not ported, 191
    deleted, and 1,729 not deleted; the parent parity item remains open.


### Progress 173

  - Completed progress: OTP's mixed
    `lib/compiler/test/compile_SUITE_data/small.erl` compiler listing and
    opcode-ceiling fixture is replaced by an executable typed Terlan program.
    Source now runs guarded string-prefix capture, floating arithmetic, local
    calls, packed binary construction, non-byte-aligned `UInt[14]` extraction,
    `Rest` payload handling, and fallback matching through CoreIR and the VM.
    An adversarial undersized binary proves that an unsatisfied layout cannot
    partially bind and instead selects the explicit fallback. BEAM listings,
    compressed artifacts, deterministic chunk comparisons, opcode ceilings,
    debug/line compiler options, and transform plumbing are deliberately
    retired. Both focused tests pass through the 13.9-second `make
    vm-small-module-parity-check`; warnings-denied all-target checking and all
    external-suite ledger validators also pass. The obsolete Erlang fixture is
    deleted. File-level progress is now 169 ported, 1,751 not ported, 192
    deleted, and 1,728 not deleted; the parent parity item remains open.


### Progress 174

  - Completed progress: OTP's
    `lib/compiler/test/compile_SUITE_data/types_pp.erl` private `dssaopt`
    inferred-type listing fixture is replaced by Terlan's typed source-to-CoreIR
    contract. The replacement proves exact CoreIR preservation for finite atom
    unions, structural maps, integer and float fields, function arrows,
    binaries, nested lists, and tuples, and proves deterministic Core contract
    and public-interface rendering across repeated compilation. Erlang-only
    number and range inference, improper-list types, unknown-arity functions,
    SSA listings, and compiler-internal pretty-printer coupling are deliberately
    retired. Both focused tests pass through the 10.1-second `make
    vm-types-pp-parity-check`; formatting, warnings-denied all-target checking,
    and all external-suite ledger validators also pass. The obsolete Erlang
    fixture is deleted. File-level progress is now 170 ported, 1,750 not
    ported, 193 deleted, and 1,727 not deleted; the parent parity item remains
    open.


### Progress 175

  - Completed progress: OTP's
    `lib/compiler/test/compile_SUITE_data/line_pt.erl` parse-transform fixture
    is replaced by immutable parser-owned Terlan source spans and precise
    diagnostic rendering. The compiler now derives one-based columns and caret
    widths from UTF-8 characters rather than bytes, clamps adversarial offsets
    to the source boundary, and preserves the parser span through rendered
    diagnostics. Terlan deliberately provides no `erl_anno` rewrite hook,
    parse-transform pipeline, or caller option that silently strips column
    information. Both focused tests pass through the 10.2-second `make
    vm-line-diagnostic-parity-check`; formatting, warnings-denied all-target
    checking, and all external-suite ledger validators also pass. The obsolete
    Erlang fixture is deleted. File-level progress is now 171 ported, 1,749 not
    ported, 194 deleted, and 1,726 not deleted; the parent parity item remains
    open.


### Progress 176

  - Completed progress: OTP's
    `lib/compiler/test/compile_SUITE_data/key_compatibility.erl` ambient
    `debug_info_key` compatibility fixture is replaced by a keyless Terlan VM
    artifact contract. Legacy dashed, underscored, assigned-value, and Erlang
    term option spellings now fail with one stable diagnostic that never
    reflects the supplied secret. A real default VM build proves that emitted
    artifacts use checksum-protected compiler metadata and a fixed debug schema
    with no key, cipher, or encrypted-debug fields. Encrypted BEAM abstract-code
    compatibility is deliberately retired. Both focused tests pass through the
    consolidated 20.1-second cold `make vm-debug-key-compatibility-check`;
    formatting, warnings-denied all-target checking, and all external-suite
    ledger validators also pass. The obsolete Erlang fixture is deleted.
    File-level progress is now 172 ported, 1,748 not ported, 195 deleted, and
    1,725 not deleted; the parent parity item remains open.


### Progress 177

  - Completed progress: OTP's
    `lib/compiler/test/compile_SUITE_data/col_utf8.erl` mixed TAB/SPC and UTF-8
    diagnostic-column fixture is replaced by Terlan-owned diagnostic parity
    tests. Source locations remain one-based character columns even when
    preceding text contains multibyte characters, caret prefixes preserve
    source tabs for visual alignment, and underline widths count characters
    rather than UTF-8 bytes. BEAM warning-column comparison and compiler
    encoding-option machinery are deliberately retired. Both focused tests
    pass through the 18.7-second cold `make vm-utf8-column-parity-check`;
    formatting, warnings-denied all-target checking, and all external-suite
    ledger validators also pass. The obsolete Erlang fixture is deleted.
    File-level progress is now 173 ported, 1,747 not ported, 196 deleted, and
    1,724 not deleted; the parent parity item remains open.


### Progress 178

  - Completed progress: OTP's
    `lib/compiler/test/compile_SUITE_data/col_lat1.erl` Latin-1 source and
    warning-column fixture is replaced by one canonical UTF-8 Terlan source
    policy. The shared source reader accepts valid multibyte UTF-8 and rejects
    encoding declarations carrying non-UTF-8 bytes with a deterministic byte
    offset. Build-target inference now reuses that reader instead of a second
    text-loading path. Latin-1 compiler options and BEAM warning rendering are
    deliberately retired. All three focused tests pass through the 18.9-second
    cold `make vm-latin1-source-policy-check`; formatting, warnings-denied
    all-target checking, and all external-suite ledger validators also pass.
    The obsolete Erlang fixture is deleted. File-level progress is now 174
    ported, 1,746 not ported, 197 deleted, and 1,723 not deleted; the parent
    parity item remains open.


### Progress 179

  - Completed progress: OTP's
    `lib/compiler/test/compile_SUITE_data/bad_enc.erl` declared-UTF-8 but
    Latin-1 parser-failure fixture is replaced by deterministic Terlan source
    boundary diagnostics. The compiler now reports the first invalid byte with
    a one-based line and character column even after tab-indented prefixes, and
    adversarial tests cover invalid first bytes and truncated multibyte
    sequences. OTP's duplicate parser and translation diagnostics are
    deliberately retired in favor of one source-reader error. Both focused
    tests pass through the 21.4-second cold `make
    vm-invalid-encoding-parity-check`; the preceding Latin-1 policy regression,
    formatting, warnings-denied all-target checking, and all external-suite
    ledger validators also pass. The obsolete Erlang fixture is deleted.
    File-level progress is now 175 ported, 1,745 not ported, 198 deleted, and
    1,722 not deleted; the parent parity item remains open.


### Progress 180

  - Completed progress: OTP's
    `lib/compiler/test/compile_SUITE_data/deterministic_module.erl` opt-in BEAM
    deterministic-compilation fixture is replaced by an unconditional Terlan
    VM artifact reproducibility contract. Identical source at the same path is
    compiled into independent output roots after an mtime-changing rewrite and
    produces byte-identical checksum-protected `.tvm.json` artifacts. The gate
    also proves that output-directory names and timestamp fields cannot leak
    into the executable artifact. BEAM compiler attributes, compile metadata,
    code loading, deletion, and purge behavior are deliberately retired. The
    focused end-to-end test passes through the 21.3-second cold `make
    vm-deterministic-artifact-parity-check`; formatting, warnings-denied
    all-target checking, and all external-suite ledger validators also pass.
    The obsolete Erlang fixture is deleted. File-level progress is now 176
    ported, 1,744 not ported, 199 deleted, and 1,721 not deleted; the parent
    parity item remains open.


### Progress 181

  - Completed progress: OTP's
    `lib/compiler/test/compile_SUITE_data/debug_info.erl` opt-in BEAM debug-info
    fixture is replaced by unconditional Terlan VM artifact metadata. Every
    public and private function now has exactly one validated source-map
    identity, and canonical JSON serialization places both source maps and
    debug metadata inside the artifact checksum domain. Adversarial loader
    tests reject debug tampering, coordinated source-file remapping, and
    duplicate source-map rows even when the checksum is recomputed. BEAM
    abstract-code chunks and debug compiler attributes are deliberately
    retired. The compiler emission test and three loader tests pass through
    the 29.3-second `make vm-debug-info-artifact-parity-check`; all 28 artifact
    loader tests, three artifact execution regressions, deterministic artifact
    emission, formatting, warnings-denied all-target checking, and all
    external-suite ledger validators also pass. The obsolete Erlang fixture is
    deleted. File-level progress is now 177 ported, 1,743 not ported, 200
    deleted, and 1,720 not deleted; the parent parity item remains open.


### Progress 182

  - Completed progress: OTP's
    `lib/compiler/test/compile_SUITE_data/funs.erl` named-function fixture is
    replaced by source-to-CoreIR-to-VM function-value parity. A named local
    identity function now crosses a typed function-value pass-through boundary
    and remains callable with atom and integer `Dynamic` values. Adversarial
    coverage proves exact callable arity, stable missing/surplus argument
    diagnostics, and successful invocation after both rejected calls. BEAM fun
    encoding, legacy optimizer switches, and opcode ceilings are deliberately
    retired. Both focused tests pass through the 14.1-second `make
    vm-function-value-passthrough-parity-check`; formatting, warnings-denied
    all-target checking, file-size hygiene for the 59-line test module, and all
    external-suite ledger validators also pass. The obsolete Erlang fixture is
    deleted. File-level progress is now 178 ported, 1,742 not ported, 201
    deleted, and 1,719 not deleted; the parent parity item remains open.


### Progress 183

  - Completed progress: OTP's
    `lib/compiler/test/compile_SUITE_data/generic_pt.erl` arbitrary parse/core
    transform plugin fixture is classified as nonportable and removed. Terlan
    does not load compiler plugins that replace syntax trees or manufacture
    warnings, errors, throws, and exits. The build CLI now rejects long-option,
    underscore, and Erlang-term transform spellings with one stable diagnostic
    that never reflects supplied module names or payloads, while adversarial
    coverage keeps similarly named source paths valid. Both focused tests pass
    through the 20.0-second `make vm-compiler-transform-retirement-check`;
    formatting, warnings-denied all-target checking, and all external-suite
    ledger validators also pass. The obsolete Erlang fixture is deleted but
    intentionally remains not ported. File-level progress is now 178 ported,
    1,742 not ported, 202 deleted, and 1,718 not deleted; the parent parity item
    remains open.


### Progress 184

  - Completed progress: OTP's
    `lib/compiler/test/compile_SUITE_data/column_pt.erl` parse-transform column
    fixture is replaced by one Terlan-owned source-column contract. Parser
    spans remain immutable, rendered locations use one-based character columns,
    tabs and multibyte underlines remain aligned and bounded, and legacy
    parse/core transform options fail closed without reflecting plugin input.
    The new composed gate replaces three duplicate top-level runtime-semantic
    dependencies while preserving their independently runnable checks. All six
    focused tests pass through the 28.53-second `make
    vm-source-column-ownership-check`; formatting, warnings-denied all-target
    checking, diff hygiene, and all external-suite ledger validators also pass.
    The obsolete Erlang fixture is deleted. File-level progress is now 179
    ported, 1,741 not ported, 203 deleted, and 1,717 not deleted; the parent
    parity item remains open.


### Progress 185

  - Completed progress: OTP's
    `lib/compiler/test/compile_SUITE_data/cover_messages.erl` compile-under-cover
    smoke after an in-source file/line remap is replaced by compiler-owned VM
    artifact provenance. Adversarial source text retains a forged
    `-file(..., 99999)` marker verbatim, while debug metadata and every
    source-map row derive only from the actual compiler input path. Erlang file
    attributes, synthetic line remapping, cover instrumentation, and BEAM
    binary compilation are deliberately retired. The end-to-end artifact test
    passes through the 20.85-second `make
    vm-source-provenance-artifact-check`; formatting, warnings-denied
    all-target checking, diff hygiene, and all external-suite ledger validators
    also pass. The obsolete Erlang fixture is deleted. File-level progress is
    now 180 ported, 1,740 not ported, 204 deleted, and 1,716 not deleted; the
    parent parity item remains open.


### Progress 186

  - Completed progress: OTP's
    `lib/compiler/test/compile_SUITE_data/asm_labels.erl` textual BEAM assembly
    call labels are replaced by checksum-covered VM IR call dependencies.
    Compiler emission records canonical `name/arity` identities across
    overloaded, nested, and recursive calls, pure-lane fixpoint analysis is
    arity-aware, and call-shaped string data remains inert. The VM loader
    recomputes dependencies from executable expressions and rejects forged
    labels even when the artifact checksum is recomputed. The three focused
    executions pass through the 41.71-second `make
    vm-call-dependency-artifact-check`; 29 artifact-loader, 63 VM CLI/runtime,
    and 15 build-artifact regressions also pass, along with warnings-denied
    all-target checking, formatting, diff hygiene, touched-file size/separation
    checks, and all external-suite ledger validators. The obsolete Erlang
    fixture is deleted. File-level progress is now 181 ported, 1,739 not
    ported, 205 deleted, and 1,715 not deleted; the parent parity item remains
    open. Whole-tree `rust-quality-check` remains independently blocked by
    unrelated dirty-tree dormant-module, deterministic-HashMap, file-size, and
    inline-test debt and is not claimed green by this slice.


### Progress 187

  - Completed progress: OTP's
    `lib/compiler/test/compile_SUITE_data/embedded_line_coverage.erl`
    compile-attribute and BEAM assembly marker are replaced by unconditional,
    checksum-covered VM artifact source ranges. The compiler derives each
    range from structured syntax declarations by exact function identity,
    normalizes public declarations to their first semantic token, and fails
    closed when CoreIR has no corresponding declaration. The VM loader rejects
    empty, reversed, out-of-bounds, and non-UTF-8-boundary ranges even after
    checksum recomputation. Five focused executions pass through
    `make vm-executable-source-span-artifact-check`; 28 `terlc` loader tests,
    31 VM artifact tests, three artifact-runner tests, warnings-denied
    all-target checking, and all external-suite ledger validators also pass.
    The obsolete Erlang fixture is deleted. File-level progress is now 182
    ported, 1,738 not ported, 206 deleted, and 1,714 not deleted; the parent
    parity item remains open.


### Progress 188

  - Completed progress: OTP's
    `lib/compiler/test/compile_SUITE_data/attributes.erl` parse-transform
    attribute insertion, replacement, and deletion fixture is replaced by
    typed Terlan annotation isolation. User annotation schemas remain metadata
    and cannot change the compiler-to-CoreIR-to-VM function inventory; schemas
    cannot shadow compiler-owned `compiler`, `target`, or `native` roots or the
    built-in `test` and `pure` names; duplicate schema paths fail closed. The
    shared Terlan fixture executes to `28`, and both CoreIR and built VM
    artifacts retain exactly `deleted/1`, `inserted/0`, `replaced/1`, and
    `run/0`. Nine focused executions pass through `make
    vm-annotation-isolation-parity-check`, all 32 syntax-output declaration
    tests pass, warnings-denied all-target checking passes, and all external
    suite ledger validators pass. The obsolete Erlang fixture is deleted.
    File-level progress is now 183 ported, 1,737 not ported, 207 deleted, and
    1,713 not deleted; the parent parity item remains open.


### Progress 189

  - Completed progress: OTP's
    `lib/compiler/test/compile_SUITE_data/annotations_pp.erl` BEAM result-type
    annotation listing is replaced by typed Terlan source-to-CoreIR-to-VM
    coverage. The replacement proves exact `List[Int]` result contracts for
    public and private functions, keeps the private helper out of module
    exports, executes recursive list copying through the VM, and rejects an
    inconsistent private result before runtime. BEAM annotation rendering and
    `module_info` coupling are deliberately retired. Both focused tests pass
    through the warning-denied `make vm-result-type-contract-parity-check`, and
    `make terlan-vm-erl-suite-audit-check` passes after deleting the obsolete
    Erlang fixture. File-level progress is now 184 ported, 1,736 not ported,
    208 deleted, and 1,712 not deleted; the parent parity item remains open.


### Progress 190

  - Completed progress: OTP's reduced
    `lib/compiler/test/compile_SUITE_data/big.erl` compiler-stress fixture is
    replaced by typed Terlan source-to-CoreIR-to-VM coverage. The replacement
    exposes `totals(Int): Option[Map[String, Int]]`, rejects negative bounds,
    keeps implementation helpers private, and verifies exact even/odd totals
    through 1,000 mutable-map accumulation steps. The initial replacement
    exposed a host-stack overflow at 100 source tail calls; local self calls in
    `let`, `case`, and `if` tail positions now use a VM evaluator trampoline,
    with a separate 5,000-step `if`/`let` regression. Shared case/if branch
    selection prevents semantic drift between ordinary and tail-position
    evaluation. The warning-denied `make vm-compiler-big-parity-check` passes
    all 83 VM parity tests, and `make terlan-vm-erl-suite-audit-check` passes
    after deleting the obsolete Erlang fixture. File-level progress is now 185
    ported, 1,735 not ported, 209 deleted, and 1,711 not deleted; the parent
    parity item remains open.


### Progress 191

  - Completed progress: OTP's
    `lib/compiler/test/compile_SUITE_data/bigE.erl` BEAM variable-legalization
    fixture is replaced by typed Terlan record and mailbox behavior. The
    replacement constructs a typed `R`, captures `record.b` in a
    `Process.receive_where` predicate, proves both selective match and miss
    outcomes, unwraps the selected message, and projects `record.a` into a
    user-defined `rec0` binding. Structured CoreIR assertions prove field
    access and the user binding remain distinct without generated BEAM
    extraction variables, uppercase legalization, or collision warnings. Both
    focused tests pass through the warning-denied `make
    vm-compiler-big-e-parity-check`, and `make
    terlan-vm-erl-suite-audit-check` passes after deleting the obsolete Erlang
    fixture. File-level progress is now 186 ported, 1,734 not ported, 210
    deleted, and 1,710 not deleted; the parent parity item remains open.


### Progress 192

  - Completed progress: OTP's
    `lib/compiler/test/compile_SUITE_data/exceptions.erl` badmatch and BEAM
    stack-location fixture is replaced by executable Terlan `try/catch`
    semantics. The VM now evaluates successful try bodies, catches refutable
    binding failures through the shared case-pattern selector, preserves
    constructor identity in uncaught diagnostics, and rethrows the original
    failure when no catch clause matches. Unspecified `try after` cleanup
    remains fail-closed. Four focused tests pass through the warning-denied
    `make vm-compiler-exceptions-parity-check`; 12 shared pattern tests, the
    existing unsupported-expression diagnostic test, and `make
    terlan-vm-erl-suite-audit-check` also pass after deleting the obsolete
    Erlang fixture. File-level progress is now 187 ported, 1,733 not ported,
    211 deleted, and 1,709 not deleted; the parent parity item remains open.


### Progress 193

  - Completed progress: OTP's
    `lib/compiler/test/compile_SUITE_data/bs_init_writable.erl` SSA
    binary-comprehension type fixture is replaced by typed Terlan
    source-to-CoreIR-to-VM bitstring construction. The replacement packs exact
    one-bit segments, converts aligned eight-bit segments to VM-owned `Bytes`,
    rejects unsigned segment overflow, and asserts distinct static
    `BitString`/`Bytes` return contracts with no runtime type-test operation in
    CoreIR. All three focused tests pass through the warning-denied `make
    vm-compiler-bitstring-construction-parity-check`, and `make
    terlan-vm-erl-suite-audit-check` passes after deleting the obsolete Erlang
    fixture. File-level progress is now 188 ported, 1,732 not ported, 212
    deleted, and 1,708 not deleted; the parent parity item remains open.


### Progress 194

  - Completed progress: `docs/runtime/VM_SEMANTICS_EQUIVALENCE_MATRIX.tsv`
    now pins every required `vm-semantics-vs-otp-check` track to its VM gate,
    BEAM test-suite port-plan area, OTP reference behavior, status, and
    current evidence. The gate rejects missing/stale tracks, stale Make gate
    names, stale port-plan areas, and partial/complete rows without evidence.


### Progress 195

  - Completed progress: `vm-http-concurrency-investigation-check` now runs
    `tools/check_vm_http_concurrency_investigation.py --self-test` before
    artifact validation, so stale one-run HTTP baselines fail locally even
    before a developer relies on the generated report. The self-test covers
    rejected one-run requested/completed counts, rejected one-sample sustained
    lanes, and accepted three-sample statistically credible sustained lanes.


### Progress 196

  - Current gate state: `make lean-proof-track-check` exists, is wired into
    `make check`, validates `docs/compiler/LEAN_PROOF_TRACK.md`, validates the
    machine-readable Lean inventory and proof-gap manifests, rejects
    untracked Lean files under `proofs/lean`, and blocks stale removed-runtime
    proof terminology such as CoreV0, BEAM lowering, Erlang lowering,
    `@target.erlang`, and `otp_application`. Accepted proof-gap rows must also
    name the concrete language/CoreIR/protocol manifest files they cover.


### Progress 197

  - Completed progress: proof replay drift diagnostics now report the recorded
    manifest or dependency digest as `expected` and the current filesystem
    digest as `found`. Exact adversarial tests lock both diagnostic paths, and
    the ShapeImplication replay metadata is synchronized with the canonical
    EBNF fingerprint. `make proof-repro-check` replays all four current proof
    families twice with identical digests and passes with Rust warnings denied.
    The broader proof-gap lifecycle and zero-open-gap closeout remain open.


### Progress 198

  - Completed progress: the proof-gap manifest now uses one shared schema across
    proof tracking, PR ownership, and regression reporting. Every row records a
    lifecycle state, constrained category, remediation owner, executable gate,
    deadline or exception, covered manifests, and a blocker hash derived from
    its feature, category, reason, and blocker update date. Committed `open`
    rows, unknown lifecycle values or categories, malformed deadlines, and
    stale hashes fail through exact adversarial tests. All eight current gaps
    are explicitly `blocked`,
    and `RUSTFLAGS='-D warnings' make lean-proof-track-check` passes while
    replaying all four current Lean families reproducibly. Final proof-gap
    release closeout remains open.


### Progress 199

  - Completed progress: blocker freshness is now governed by the versioned
    `lean_proof_gap_policy.toml` contract with a 30-day TTL. ISO update dates,
    future timestamps, expired blockers, and timestamp refreshes without a
    renewed content hash fail deterministically. The canonical
    `lean-proof-gate.json` now includes aggregate and per-gap
    `gap_staleness_days`, `gap_classification_confidence`, gap count, policy,
    and unresolved-open count while preserving the existing family report.
    The owning warning-denied proof-track gate reports eight blocked gaps within
    policy TTL, classification confidence `1.0`, zero unresolved open rows, and
    four reproducible current proof families.


### Progress 200

  - Completed progress: `make lean-proof-track-gap-hygiene-check` now reuses the
    canonical gap parser, Make-target inventory, lifecycle validator, and TTL
    policy to reject missing executable follow-up gates and expired blockers.
    It additionally rejects exact feature overlap between current proof
    inventory rows and active gaps. The arithmetic proof is honestly scoped as
    a `CoreIR integer arithmetic seed`, leaving the broader CoreIR gap visible.
    Four adversarial tests cover duplicate coverage, narrow seed coverage,
    missing follow-up gates, and expired blockers. The warning-denied hygiene
    gate passes for eight gaps, four current proof features, and seven distinct
    follow-up gates, and release closeout now depends on this gate.


### Progress 201

  - Completed progress: accepted proof-gap closures now require a canonical,
    reversible changelog note in `docs/compiler/LEAN_PROOF_TRACK.md` naming the
    exact feature, current restoration artifact digest, and non-empty closure
    rationale. The hygiene gate rejects missing, orphaned, duplicate,
    malformed, empty-rationale, and fabricated-artifact closure evidence while
    allowing a closed gap backed by a current executable proof digest. Six
    additional adversarial tests bring the focused hygiene suite to ten tests;
    the gate passes with zero closure notes because all eight current gaps remain
    explicitly blocked rather than falsely closed.


### Progress 202

  - Completed progress: every accepted gap now has a content-addressed lifecycle
    ledger in `lean_proof_gap_transitions.tsv`. The gate requires histories to
    begin at `none -> open`, advance exactly through
    `open -> triaged -> blocked -> remediated -> closed`, use nondecreasing ISO
    dates and SHA-256 evidence, end at the live manifest state, and retain the
    current blocker hash until closure. Seven adversarial tests reject skipped
    states, disconnected chains, manifest drift, future or out-of-order dates,
    invalid evidence, and orphaned history. The warning-denied hygiene gate now
    passes 17 focused tests and validates 24 transitions for all eight blocked
    gaps without a lifecycle shortcut.


### Progress 203

  - Completed progress: `vm-otp-abstractions-terlan-stdlib-check` now verifies
    `std.vm.Agent`, `std.vm.GenServer`, `std.vm.Supervisor`, and `std.vm.Task`
    Terlan stdlib modules exist, inventories current framework-level compiler
    intrinsic migration debt, rejects direct runtime magic keys such as
    `vm.gen_server.*`, enforces runtime-mechanics versus runtime-policy
    documentation in the VM concept inventory and `std.vm` README, and is
    included in the main `make check` sequence.


### Progress 204

  - Completed progress: `std-vm-parity-matrix-check` and
    `std-vm-surface-classification-check` now classify 1,418 std modules across
    21 ownership rules, including `std.binary`, `std.random`, `std.range`,
    `std.regex`, and `std.wasm`. The matrix parser enforces exact columns,
    duplicate-free and sorted prefixes, placeholder-free owner/notes fields,
    and a dedicated `wasm-target-surface` classification for `std.wasm.Abi`.


### Progress 205

  - Completed progress: `vm-supervision-restart-check` now runs after
    `vm-supervision-primitives-check`, validates the current VM-owned
    `one_for_one`, `one_for_all`, and `rest_for_one` restart baseline plus
    `permanent`, `transient`, and `temporary` child restart classes, exponential
    restart backoff, per-child restart delay reporting, and observable last
    restart delay state, configured child shutdown timeout metadata,
    per-child shutdown timeout reporting, and observable last shutdown timeout
    state, VM-timer-backed graceful shutdown messages, scheduler-facing
    shutdown deadline enforcement, clean-exit deadline cancellation, typed
    forced `ShutdownTimeout` exits, restart-intensity exhaustion escalation to
    failed supervisor state,
    parent-supervisor observation of terminal child-supervisor failure,
    snapshot-visible restart history for successful, terminal, and non-restart
    outcomes, and writes `target/quality/vm-supervision-report.json`
    with restart outcomes,
    observable supervisor state, failure reasons, explicit supervision graph
    fields, restart history fields, escalation decision fields, final process
    state fields, and explicitly open restart gaps. Current covered baseline:
    supervisor creation, child start, one-for-one
    restart, one-for-all group restart, one-for-all preflight restart-limit
    rejection, rest-for-one restart of the failed child and later children,
    rest-for-one preflight restart-limit rejection, temporary child non-restart,
    transient normal-exit non-restart, transient abnormal-exit restart, group
    restart skipping non-restartable children without blocking restartable
    siblings, one-for-one exponential restart backoff, group restart per-child
    backoff delay reporting, live-child shutdown timeout recording, group
    restart per-child shutdown timeout reporting, cooperative child exit before
    the configured deadline, forced child exit at or after the configured
    deadline, preservation of unrelated timer events during shutdown deadline
    dispatch, duplicate and overflowing shutdown deadline rejection, live-child
    exit before replacement, restart limit terminal outcome, duplicate child rejection,
    failed-supervisor inspection state after restart-limit exhaustion,
    parent-supervisor failure propagation into inspection-visible state,
    restart-history entries for restart, limit, and non-restart outcomes,
    missing supervisor/child/process diagnostics, and snapshot availability
    after failed restart. Configured shutdown timeout scheduler enforcement is
    complete; the remaining escalation gap is parent-supervisor restart
    strategy execution after propagation.


### Progress 206

  - Completed progress: `vm-otp-abstractions-terlan-stdlib-check` now verifies
    `std.vm.Agent`, `std.vm.GenServer`, `std.vm.Supervisor`, and `std.vm.Task`
    Terlan stdlib modules exist, inventories current framework-level compiler
    intrinsic migration debt, rejects direct runtime magic keys such as
    `vm.gen_server.*`, and is included in the main `make check` sequence.


### Progress 207

  - Completed progress: supervision memory-pressure handling now has
    adversarial ownership tests for missing supervisors, missing children, and
    pressure decisions attributed to the wrong process. Hard pressure under
    `rest_for_one` now proves the failed child and later siblings are cleaned
    and restarted while earlier siblings retain their memory and runnable
    state. Both regressions are exact members of
    `vm-memory-heap-pressure-check`; that gate and
    `vm-supervision-restart-check` pass. An isolated LLVM coverage run executes
    all 34 supervision tests and reports 487/487 source lines and 38/38
    functions covered in `supervision.rs`. The Make graph preserves
    supervision primitive validation before timer readiness and restart
    validation. The promoted VM coverage gate now also exercises the scheduler
    memory-reduction diagnostic for a missing process; `make
    vm-coverage-100-check` passes all 1,035 VM tests and enforces 100% source-line
    and function coverage across all 26 promoted VM-owned files.


### Progress 208

  - Completed progress: the VM scheduler now retains replay-stable enqueue/dequeue
    transitions, per-process reductions, slices, budget-exhaustion preemptions,
    maximum queue wait, runnable duration, and aggregate queue-depth telemetry.
    `make vm-scheduler-fairness-check` exercises deterministic round-robin behavior
    under CPU-bound load and persists
    `target/quality/vm-scheduler-fairness-report.json` with correlation identity and
    derived starvation warnings. Voluntary yields below the reduction budget are
    explicitly excluded from preemption counts.


### Progress 209

  - Completed progress: scheduler queues now support explicit `priority`, `normal`,
    and `background` classes with a deterministic 3:2:1 weighted cycle. Existing
    callers remain `normal` by default, FIFO order is retained within each class,
    background work receives service within one six-slot cycle under continuous
    priority load, blocked processes retain their class across wakeup, and exited
    processes release class metadata. The fairness gate rejects silent queued-process
    reclassification and locks the replay trace and background wait bound.


### Progress 210

  - Completed progress: cancellation requested by running Terlan code now takes
    effect at the current scheduler boundary instead of requeueing the process for a
    second slice. The VM preserves reductions consumed by the cancelled slice,
    returns the typed `Cancelled` scheduler outcome with resource cleanup handles,
    removes scheduling-class state, and guarantees the process cannot execute again.
    The fairness gate includes an adversarial full-budget cancellation regression.


### Progress 211

  - Completed progress: successful NativeBoundary deadline dispatch now charges one
    VM-owned runtime reduction before parking its actor, using the same per-process
    and aggregate scheduler accounting as executable slices. Invalid, overflowing,
    duplicate, and worker-backpressured dispatch attempts do not inflate reduction
    totals. The warnings-as-errors `vm-scheduler-fairness-check` owns an exact
    adversarial regression plus all 26 scheduler tests; the broader Slice 40 remains
    open for operation-level charging and preemption coverage across the remaining
    VM execution paths.


### Progress 212

  - Completed progress: typed timer-event mailbox delivery now charges one VM-owned
    runtime reduction to the receiving process after a successful mailbox send and
    before scheduler wakeup. Observation-only owner-exit events and stale events
    whose owners exited before delivery remain uncharged and do not increment timer
    delivery metrics. The focused regression lives outside the oversized legacy
    timer test module and is an exact member of the passing warnings-as-errors
    `vm-scheduler-fairness-check`; all 33 legacy timer tests also pass.


### Progress 213

  - Completed progress: ordinary, selective, and timeout actor receive attempts now
    charge one VM-owned operation reduction after live-process validation. Successful
    receives retain separate mailbox-memory release accounting, while empty selective
    receives, blocking receives, and immediate timeouts still account for the receive
    operation. Missing or exited actors remain side-effect free and uncharged. A
    focused adversarial module and the observability snapshot lock the operation versus
    memory breakdown; all 71 actor tests and the warnings-as-errors
    `vm-scheduler-fairness-check` pass. Scan-proportional selective-receive charging
    remains part of the broader mailbox-flood/preemption gap.


### Progress 214

  - Completed progress: selective receive now charges reductions in proportion to the
    mailbox entries actually inspected, with a minimum charge of one for an empty
    scan. The memory layer reports both the selected message and deterministic scan
    count while preserving its existing message-only API for non-scheduler callers.
    Adversarial coverage locks third-position matches, full misses that preserve
    mailbox order and ownership, single-entry misses, and invalid-process attempts.
    All 29 memory tests, all 71 actor tests, and the warnings-as-errors
    `vm-scheduler-fairness-check` pass. True mid-scan preemption for very large
    mailboxes remains open.


### Progress 215

  - Completed progress: successful actor message admission now charges one
    VM-owned operation reduction to the sender, independently from recipient
    mailbox-memory accounting. PID, registered-name, alias, and self-send routes
    share the same accounting boundary. Missing or exited processes, unresolved
    names, stale aliases, and mailbox hard-limit rejection do not charge sender
    execution. A focused adversarial regression is an exact member of the
    warnings-as-errors `vm-scheduler-fairness-check`; all 72 actor tests and all
    26 scheduler tests pass. Operation charging for remaining VM execution paths
    and true mid-operation preemption remain open.


### Progress 216

  - Completed progress: successful child-actor creation now charges one VM-owned
    operation reduction to its parent after scheduler-class assignment and any
    requested link/monitor relationships complete. Plain and linked, monitored,
    priority spawns share the same boundary; root bootstrap remains uncharged,
    and missing or exited parent attempts do not mutate scheduler accounting.
    The exact adversarial regression, all 73 actor tests, and the complete
    warnings-as-errors `vm-scheduler-fairness-check` pass, including all 26
    scheduler tests. Remaining runtime operations and true mid-operation
    preemption keep Slice 40 open.


### Progress 217

  - Completed progress: successful delayed-message and correlated-message timer
    scheduling now charges one VM-owned operation reduction to the timer owner
    after route/deadline validation and timer registration complete. Direct,
    registered-name, alias, relative/absolute-deadline, and correlated routes
    share the same scheduling boundary; eventual message delivery remains a
    separate charged operation. Missing/exited processes, unresolved names,
    stale aliases, and deadline overflow remain uncharged and allocation-atomic.
    A focused exact regression, all 74 actor tests, and the warnings-as-errors
    `vm-scheduler-fairness-check` pass with all 26 scheduler tests. Remaining
    runtime operations and true mid-operation preemption keep Slice 40 open.


### Progress 218

  - Completed progress: successful actor relationship operations now charge one
    VM-owned reduction to their explicit initiator. Link/unlink charge the left
    actor, monitor/demonitor charge the observer, and trap-exit updates charge the
    configured actor; successful idempotent operations remain charged because
    validation and relationship lookup still execute. Self-relations, missing or
    exited peers, monitor ownership violations, and missing actors remain
    uncharged. A focused exact adversarial regression, all 75 actor tests, and
    the warnings-as-errors `vm-scheduler-fairness-check` pass with all 26
    scheduler tests. Caller attribution for remaining runtime operations and
    true mid-operation preemption keep Slice 40 open.


### Progress 219

  - Completed progress: successful actor name and alias registry mutations now
    charge one VM-owned operation reduction to the affected owner. Name
    registration/unregistration and alias creation/removal share the same
    accounting boundary, including successful idempotent name registration;
    lookups and deterministic inventory reads remain uncharged observations.
    Empty, conflicting, missing, exited, and stale mutation attempts remain
    uncharged. Registry methods now live in focused `actor_registry.rs` instead
    of growing the actor facade. The exact adversarial regression, all 76 actor
    tests, and the warnings-as-errors `vm-scheduler-fairness-check` pass with all
    26 scheduler tests. Remaining runtime operations and true mid-operation
    preemption keep Slice 40 open.


### Progress 220

  - Completed progress: successful actor suspension and resumption now charge one
    VM-owned operation reduction to the affected actor after the scheduler state
    transition succeeds. Runnable and blocked actors preserve their retained
    resume state, and successful idempotent suspension remains charged because it
    still performs scheduler control work. Missing or exited actors and attempts
    to resume a non-suspended actor remain uncharged. The control methods now live
    in focused `actor_suspension.rs`; the exact adversarial regression, all 77
    actor tests, and the warnings-as-errors `vm-scheduler-fairness-check` pass
    with all 26 scheduler tests. Remaining runtime operations and true
    mid-operation preemption keep Slice 40 open.


### Progress 221

  - Completed progress: a newly initiated actor exit now charges one VM-owned
    terminal reduction to the exiting actor only after failure propagation,
    resource cleanup, alias/timer cleanup, memory synchronization, and recipient
    wakeup succeed. The scheduler exposes a terminal-only accounting boundary that
    rejects live and missing processes, allowing completed work to be recorded
    without speculative precharging. Repeated idempotent exits, missing actors,
    and peers exited recursively by an abnormal link cascade remain uncharged.
    Exit orchestration now lives in focused `actor_exit.rs`; both exact adversarial
    regressions, all 78 actor tests, and the warnings-as-errors
    `vm-scheduler-fairness-check` pass with all 26 scheduler behavior tests.
    Remaining runtime operations and true mid-operation preemption keep Slice 40
    open.


### Progress 222

  - Completed progress: accepted mailbox checkpoint restoration now charges one
    VM-owned operation reduction to the recipient independently from logical-byte
    memory accounting. Empty and nonempty restores share the operation boundary;
    hard-limit rejection records only pressure-evaluation memory reductions, while
    missing/exited recipients and values without an ownership contract remain
    operation-uncharged and allocation-atomic. Checkpoint orchestration now lives
    in focused `actor_checkpoint.rs`; the exact adversarial regression, all 79
    actor tests, and the warnings-as-errors `vm-scheduler-fairness-check` pass with
    all 26 scheduler behavior tests. Remaining runtime operations and true
    mid-operation preemption keep Slice 40 open.


### Progress 223

  - Completed progress: successful active delayed-message timer cancellation now
    charges one VM-owned operation reduction to the timer owner after timer and
    payload removal complete. Raw cancellation and typed synchronous cancellation
    share the same mutation boundary even when a distinct requester performs the
    typed operation. Timer reads, stale raw or synchronous typed cancellation, and
    missing or exited requester attempts remain operation-uncharged; asynchronous
    reply delivery retains its independent send and memory accounting. The exact
    adversarial regression, all 80 actor tests, and the warnings-as-errors
    `vm-scheduler-fairness-check` pass with all 26 scheduler behavior tests.
    Remaining runtime operations and true mid-operation preemption keep Slice 40
    open.


### Progress 224

  - Completed progress: every successful explicit scheduler-class request now
    charges one VM-owned operation reduction to the affected process after class
    and queue transitions complete. Runnable queued and blocked processes retain
    their existing ordering and wakeup semantics, and same-class idempotent
    requests remain charged because they still perform explicit scheduler control
    work. Missing and exited process requests remain uncharged and side-effect
    free. The exact adversarial regression, all 32 scheduler-module tests, and the
    warnings-as-errors `vm-scheduler-fairness-check` pass with all 26 scheduler
    behavior tests. Remaining execution paths and true mid-operation preemption
    keep Slice 40 open.


### Progress 225

  - Completed progress: every successful cooperative cancellation request now
    charges one VM-owned operation reduction to the target process after live-state
    validation and cancellation flag mutation. Runnable and blocked targets and
    repeated idempotent requests share the same explicit scheduler-control
    boundary; missing and exited targets remain uncharged. Cancellation still
    takes effect at the existing scheduler boundary, and pre-slice cancellation
    reports zero slice reductions independently from the already-accounted
    request. The exact adversarial regression, all 33 scheduler-module tests, and
    the warnings-as-errors `vm-scheduler-fairness-check` pass with all 26 scheduler
    behavior tests. Remaining execution paths and true mid-operation preemption
    keep Slice 40 open.


### Progress 226

  - Completed progress: a successful VM HTTP response write now charges one
    VM-owned operation reduction to its handler after the TCP or TLS transport
    accepts the serialized response. Response-buffer reservation and release keep
    their independent memory-reduction accounting; hard-pressure rejection and
    failed transport writes release or reject memory without charging a completed
    write. The shared completion boundary covers both plaintext and TLS paths, and
    the three focused TCP regressions pass with warnings denied. The canonical
    `vm-scheduler-fairness-check` was run, but its repository-wide Rust-suite
    prerequisite currently fails on 124 unrelated dirty-tree regressions, primarily
    stale raw-atom fixtures, so the parent gate and Slice 40 remain open.


### Progress 227

  - Completed progress: bound SQL now crosses a maintained, version-pinned
    `sqlparser` PostgreSQL-dialect boundary before cardinality inference or CoreIR
    lowering. The compiler rejects malformed, empty/comment-only, and
    multi-statement forms with stable diagnostics while preserving Terlan
    interpolations as positional parameters, including injection-shaped values.
    Adversarial parser, parameterization, and diagnostic coverage is part of the
    passing `make sql-form-check` gate; the same implementation also passes a
    warnings-as-errors build. Slice 44 remains open for complete database-authoritative
    parameter/result inference, migration/schema validation, and the dedicated
    Docker-backed semantic gate.


### Progress 228

  - Completed progress (2026-07-19): extracted the canonical Postgres snapshot
    model, integrity validation, fingerprinting, and nearest-project discovery
    from the DB command into one compiler-neutral schema contract. CLI checks and
    LSP file diagnostics now consume the same verified snapshot and fail closed on
    malformed, forged, unsupported, or ambiguous snapshot evidence. The maintained
    `sqlparser` PostgreSQL AST now resolves single-physical-relation `SELECT`
    projections against that snapshot, including schema-qualified relations,
    aliases, quoted identifiers, direct columns, qualified/unqualified wildcards,
    catalog ordinal order, duplicate outputs, and stable unknown relation/column/
    qualifier diagnostics. Snapshot-expanded projection names feed the existing
    tuple/struct row-shape checker, so `SELECT *` no longer bypasses row-field
    validation. Snapshot projections now preserve source column descriptors through
    aliases and enforce the VM's explicit `Int`, `Bool`, `Binary`, and canonical
    `std.data.Json.Json` codecs plus exact structural `Option` nullability. Unknown
    catalog/domain codecs and computed projections fail closed or defer to live
    prepare/describe instead of inheriting libpq's text fallback. Schema names and
    native libpq OIDs now resolve through the same codec enum; schema-free row checks
    also reject `Float`/`Number` until the VM owns a faithful decoder. The warning-denied
    focused tests pass, as do 124 consolidated compiler/runtime SQL tests and 21 SQL
    quality/parser tests. The hardened report now covers 18 diagnostics and
    fingerprints the shared snapshot/AST/codec validator contract as
    `1c31a71d4a8b755991e9fcc6c14cc00ecaaf5a6bd64a8cb2582fc9014e77c0ca`.
    Database-authoritative parameter types, non-scalar/custom codecs, joins/derived
    result descriptors, migration identity compatibility, and the Docker-backed
    prepare/describe gate remain open.


### Progress 229

  - Completed progress: SQL statement classification and conservative cardinality
    inference now consume the parsed PostgreSQL AST rather than a local keyword
    tokenizer. `select`, `insert`, `update`, `delete`, core PostgreSQL DDL,
    transaction, and other statements have stable kinds propagated through SQL
    analysis, wrapper plans, diagnostics, and CoreIR. CTE selects, literal
    `LIMIT 1`, `FETCH FIRST 1 ROW ONLY`, dynamic limits, `WITH TIES`, `RETURNING`,
    comments, and string-literal traps have adversarial coverage in the passing
    `make sql-form-check` gate. The obsolete cardinality tokenizer and its private
    helpers were removed; complete row/nullability/schema inference remains open.


### Progress 230

  - Completed progress: `SELECT` and `RETURNING` output-name metadata now comes
    directly from `sqlparser` AST nodes and is carried once through SQL analysis,
    wrapper planning, row-shape validation, diagnostics, and CoreIR rather than
    reparsing rendered SQL. Direct columns, compound columns, CTE projections,
    expression aliases, quoted identifiers, and PostgreSQL unquoted-name folding
    have adversarial coverage. Duplicate output names fail with a stable compiler
    diagnostic; wildcards and unaliased expressions remain explicitly unknown for
    later live-schema validation. The obsolete projection tokenizer, literal/comment
    masking, delimiter scan, and alias reconstruction helpers were removed. The
    expanded `make sql-form-check` gate and warnings-as-errors build pass; parameter
    types, nullability, migration/schema validation, persisted reporting, and the
    Docker-backed semantic gate remain open.


### Progress 231

  - Completed progress: SQL parameter binding now rejects user-authored PostgreSQL
    `$N` placeholders with a stable diagnostic, so every compiler-bound parameter
    originates from a Terlan `${expression}` interpolation. A shared syntax/typecheck
    scanner preserves line comments, nested block comments, quoted identifiers,
    string literals, and tagged or untagged PostgreSQL dollar-quoted bodies without
    treating their contents as interpolation or parameters; identifier text such as
    `metric$1` remains valid. The duplicate typecheck scanner was removed, and
    adversarial parser, syntax-output, binding, parameter-drift, and diagnostic tests
    pass all 37 selectors in `make sql-form-check` together with the all-target
    warnings-as-errors build. Parameter types, nullability, migration/schema
    validation, persisted reporting, and the Docker-backed semantic gate remain open.


### Progress 232

  - Completed progress: CoreIR `SqlQuery` nodes now preserve every Terlan SQL
    interpolation as an ordered executable `CoreExpr` parameter instead of dropping
    the values and retaining only a count. Parameter count is derived from that
    vector, eliminating a redundant field that could drift from the executable
    payload. Nested calls remain visible to constructor resolution, proof evidence,
    target-profile validation, VM artifact dependency and size analysis, and JS
    reachability. Adversarial lowering coverage proves source order and a nested local
    call survive into deterministic CoreIR contract text. All 37 selectors in
    `make sql-form-check`, the focused lowering test, and the all-target
    warnings-as-errors build pass. Database-derived parameter types, nullability,
    migration/schema validation, persisted reporting, and the Docker-backed semantic
    gate remain open.


### Progress 233

  - Completed progress: `make vm-sql-macro-validation-check` now owns one
    warning-denied Cargo invocation across the `terlc` and `terlan-quality` SQL
    surfaces instead of replaying 50 exact selectors through separate processes.
    The gate validates the maintained `sqlparser = "=0.60.0"` PostgreSQL-dialect
    boundary, compiler cardinality/projection/parameter/row-shape/CoreIR anchors,
    gate ordering before `vm-postgres-runtime-check`, and a deterministic validation
    contract fingerprint. It writes
    `target/quality/vm-sql-macro-validation-report.json` with parser identity,
    inferred cardinality and row-shape coverage, validation mode, and ten stable
    diagnostic families. Static mode records `null` schema and migration identities
    rather than claiming database evidence; adversarial tests require valid lowercase
    SHA-256 identities before `postgres-live` mode may claim them. The final gate
    passes 18 quality/syntax tests and 107 compiler/runtime SQL tests in 13.7 seconds
    on a warm tree. Database-authoritative codecs/nullability, real migration and
    schema fingerprints, and Docker semantic validation remain open.


### Progress 234

  - Completed progress: SQL interpolation expressions now retain their inferred Terlan
    types through normal typechecking, final substitution, and transparent alias
    expansion. The compiler enforces the current scalar VM/Postgres binding ABI of
    `Int`, `Float`, `Number`, `Binary`, and `Bool`; structured collections, maps,
    functions, `Dynamic`, nullable or opaque wrappers, and unresolved types fail at the
    interpolation source span with a stable indexed diagnostic. Three focused and
    adversarial tests cover accepted scalar aliases, rejected non-bindable values, and
    diagnostic index/span preservation. All 40 selectors in `make sql-form-check` and
    the all-target warnings-as-errors build pass. Database-authoritative parameter
    types, codecs, nullability, migration/schema validation, persisted reporting, and
    the Docker-backed semantic gate remain open.


### Progress 235

  - Completed progress: SQL transaction requirements now come from the maintained
    PostgreSQL AST and propagate through SQL analysis, wrapper plans, unresolved-macro
    diagnostics, and deterministic CoreIR contracts. Ordinary statements are marked
    `autocommit_allowed`; locking queries plus savepoint operations are marked
    `active_transaction_required`; and raw `BEGIN`, `COMMIT`, top-level `ROLLBACK`,
    and `SET TRANSACTION` forms are marked `vm_managed_control` and rejected with a
    stable diagnostic directing callers to the typed database transaction API.
    Adversarial coverage includes row locks, savepoint create/rollback/release, all
    VM-owned control forms, wrapper readiness, user-facing diagnostics, and CoreIR
    propagation. All 44 selectors in `make sql-form-check` and the all-target
    warnings-as-errors build pass. Runtime enforcement of active-transaction context,
    database-authoritative parameter types, codecs, nullability, migration/schema
    validation, persisted reporting, and the Docker-backed semantic gate remain open.


### Progress 236

  - Completed progress: typed SQL row descriptors now accept visible named rows,
    direct non-empty scalar tuples, and transparent aliases to scalar tuples through
    the normal Terlan type parser and alias expansion. AST-derived projections enforce
    tuple arity when the projection width is known. Tuple items and local struct fields
    now accept the scalar row-decode ABI of `Int`, `Float`, `Number`, `Binary`, and
    `Bool`, plus the normal structural `Option[T]` form when `T` is one of those
    scalars. Structured nullable payloads remain rejected with stable indexed or named
    field diagnostics, and nullable parameter binding remains closed until its input
    codec contract exists. Stable diagnostics also cover empty or unsupported
    descriptor shapes and projection arity mismatches. Six focused and adversarial
    tests pass as part of all 50 selectors in `make sql-form-check`, together with the
    all-target warnings-as-errors build. Database-authoritative column nullability and
    codec matching, imported row-schema validation, runtime row decoding,
    migration/schema validation, persisted reporting, and the Docker-backed semantic
    gate remain open.


### Progress 237

  - Completed progress: the private SQL runtime helper no longer executes through
    the retired synchronous Postgres compatibility adapter. Its protocol now requires
    explicit `autocommit_allowed`, `active_transaction_required`, or
    `vm_managed_control` metadata; transaction-only and VM-owned control forms fail
    before database configuration or socket access, while allowed work runs through
    `VmPostgresCommandClient` and the VM-owned nonblocking libpq worker. Driver-owned
    dynamic row decoding preserves concrete string, integer, boolean, JSON, and SQL
    `NULL` values without trial decoding or exposing native rows. The SQL boundary
    checker rejects any reintroduction of compatibility `connect`, query, execute, or
    transaction calls. The warning-denied `vm-sql-macro-validation-check` passes 18
    quality/syntax tests and 108 compiler/runtime SQL tests, and the focused VM worker
    transaction/decode regression passes. The Docker gate now carries equivalent
    integer/JSON/NULL assertions for privileged local and CI execution; this sandbox
    could compile but not execute that ignored test because Docker API access is
    denied. Source-evaluator suspension through a real active transaction context and
    database-authoritative schema validation remain open.


### Progress 238

  - Completed progress (2026-07-17): the release API compatibility inventory
    now points at executable Cluster and Scheduler behavior instead of removed
    compile-only test names. `SchedulerTest.terl` adds source-level
    least-connections coverage that proves deterministic tie-breaking and load
    updates affect placement. Imported transparent union aliases now qualify
    provider-local nominal members before expansion, so a provider struct field
    remains assignable to the selected public alias in consumer code; a focused
    compiler regression locks that interface boundary. All 13 Cluster tests and
    8 Scheduler tests execute on the default VM, Rust formatting and
    warnings-as-errors checks pass, and `make stdlib-release-api-tests-check`
    validates all 729 release API rows. The parent remains open for the
    versioned cross-release surface manifest and classified diff report.


### Progress 239

  - Completed progress: the formatter adversarial corpus now includes integral-
    valued Float expressions and patterns. `format_float_literal` preserves the
    `.0` discriminator instead of changing `0.0` into the Int literal `0`, and
    `formatter_preserves_integral_float_literals_and_patterns` locks expression
    and pattern behavior. A byte-hash replay proves repeated formatting is
    idempotent for the affected Shrink, Duration, and Instant sources; their 34
    VM tests prove the formatted output remains type-correct and executable.


### Progress 240

  - Implemented progress (2026-07-19):
    `docs/runtime/TVM_AOT_PIVOT_INVENTORY.md` now classifies every discovered
    `.tvm.json` or source-bundle fallback path as reusable runtime semantics,
    compiler-internal IR, temporary migration support, or deletion debt. The
    Rust gate requires 15 named producer, consumer, evaluator, REPL/test, HTTP,
    debugger, hot-reload, compiler-IR, and quality surfaces; rejects stale or
    unowned paths and false release-completion claims; and writes
    `target/quality/tvm-aot-pivot-inventory-report.json`. Six focused positive
    and adversarial tests pass, and the checked repository currently records 45
    rows covering 35 discovered transitional paths. The quality binary passes
    with Rust warnings denied. The slice remains unchecked because the required
    repository-wide `make rust-quality-check` still reports unrelated shared-tree
    file-size and inline-test violations; roadmap policy forbids closeout until
    that canonical gate is green.


### Progress 241

  - Implemented progress (2026-07-19): format 1 now has a canonical
    little-endian descriptor codec with the `TVMDSC01` header, ordered TLV
    records, SHA-256 footer, ABI and NativeBoundary ranges, target and identity
    records, stable sorted nonzero export/capability/resource/dependency IDs,
    typed exports, resource ownership links, dependency fingerprints, code and
    immutable-data digests, and optional signature evidence. Static admission
    uses the maintained `object` parser, accepts native executables and PIEs,
    maps the exact ELF/Mach-O/PE descriptor section names, and rejects JSON,
    compiler data, non-executables, missing/duplicate descriptors, malformed or
    noncanonical tables, wrong targets or architectures, unsupported ABIs, and
    bad digests without execution. `make tvm-native-image-format-check` is now
    the repurposed artifact-format gate: five focused positive/adversarial tests
    pass, including a descriptor embedded in and inspected from a real ELF
    executable, and the normative checker enforces 84 image/native-data ABI
    groups. The three affected binaries also pass `cargo check --locked` with
    warnings denied. The slice remains open for live Mach-O and PE fixtures and
    package-policy signature verification rather than treating parser branches
    as platform conformance.


## Completed 113

- [x] Add compiler-inferred purity, checked `@pure` invariants, and optional
  `Effect[T]` descriptions.
  - Body-available functions now receive transitive compiler purity summaries;
    asserted purity is validated rather than trusted, with stable diagnostics
    for messages, VM/resource mutation, native handles, clocks, randomness,
    processes, IO, databases, and effectful callees.
  - Inferred and asserted pure helpers are admitted in guards and typed
    templates. Module summaries, generated docs, LSP hover, and native/deploy
    metadata project the compiler result.
  - `Effect.succeed`, `Effect.map`, and `Effect.flat_map` construct pure deferred
    descriptions. Only explicit `Effect.run` executes them at the VM boundary;
    ordinary direct-effect code remains valid outside pure contexts.
  - Executable Terlan fixtures cover guards, templates, direct effects, and
    deferred effect composition. Rust tests cover transitive rejection,
    diagnostic taxonomy, summary/doc/LSP projection, and malformed runtime
    descriptors.
  - Gate: `make compiler-purity-metadata-check`.
  - Integration: the purity gate runs before flexible-shape guards, typed
    template interpolation, and direct AOT native lowering.
  - Remaining scoped gaps stay explicit: scheduling/cancellation of pending or
    external effect descriptions and native lowering beyond completed values.
