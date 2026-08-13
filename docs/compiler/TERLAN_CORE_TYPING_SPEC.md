# Terlan Core Typing Spec

This document is the human-readable typing contract for the Terlan core
language accepted into typed CoreIR. The canonical syntax remains
`docs/grammar/TERLAN_SYNTAX_SPEC.ebnf`; this document defines what selected
forms mean after name resolution and typechecking.

The machine-readable companion index is
`docs/compiler/type_spec/terlan_core_typing_spec.toml`.

## Typing Judgment

The core judgment is:

```text
Gamma; Delta; Kappa; Constraints |- expr : Type
```

- `Gamma`: value environment for local bindings, parameters, functions, and
  callable values.
- `Delta`: type environment for aliases, opaque aliases, structs,
  constructors, traits, and imported interfaces.
- `Kappa`: kind environment for type variables, variance markers, and
  higher-kinded arities.
- `Constraints`: trait obligations, target requirements, native-boundary
  capabilities, and effect/capability facts.

The result is a resolved Terlan type that can be attached to CoreIR or a stable
diagnostic that rejects the form before backend lowering.

## Core Rule Families

### Literals

Integer literals have type `Int`.

```text
----------------
Gamma |- n : Int
```

String literals currently lower into the Lean-covered binary/string runtime
payload subset and remain source-level `String` in production CoreIR where the
compiler preserves that distinction.

### Variables

Local variables are looked up in `Gamma` by compiler-owned binding identity,
not by spelling alone. Source names are resolved to `CoreBindingId` before the
string-keyed type environment is populated. Two same-spelled declarations in
one `CoreBindingRegionId` are rejected; a nested lexical region may introduce a
fresh identity without replacing the outer binding.

```text
Gamma(x) = T
------------
Gamma |- x : T
```

Checked CoreIR carries declarations, resolved reference targets, region IDs,
stable source paths, and a deterministic evidence fingerprint. Backends reject
missing or forged identity evidence rather than reconstructing scope from
names. The complete contract is in
`docs/compiler/TERLAN_BINDING_IDENTITIES.md`.

### Tuples And Lists

Tuple expressions preserve positional element types.

```text
Gamma |- e1 : T1 ... Gamma |- en : Tn
-------------------------------------
Gamma |- {e1, ..., en} : {T1, ..., Tn}
```

List expressions are homogeneous in the initial core model.

```text
Gamma |- e1 : T ... Gamma |- en : T
-----------------------------------
Gamma |- [e1, ..., en] : List[T]
```

### Calls

Named calls resolve to a function signature before CoreIR lowering.

```text
Gamma(f) = (T1, ..., Tn) -> R
Gamma |- a1 : T1 ... Gamma |- an : Tn
-------------------------------------
Gamma |- f(a1, ..., an) : R
```

Remote, scoped, receiver-method, and trait-target calls must preserve their
resolved identity and capability/effect metadata. Forms not yet in the Lean
covered subset remain `proof-model-required` in the machine-readable index.

### Case And Patterns

Case expressions typecheck the scrutinee, typecheck every pattern against the
scrutinee type, extend branch environments with pattern bindings, and require a
common result type.

```text
Gamma |- scrutinee : T
Pattern_i : T => Gamma_i
Gamma, Gamma_i |- branch_i : R
--------------------------------
Gamma |- case scrutinee { Pattern_i -> branch_i } : R
```

Constructor patterns require a resolved constructor identity and arity-compatible
field types.

### Traits And Constraints

Trait and constraint forms are checked as qualified typing obligations.

```text
Gamma |- receiver : T
Constraints prove T implements Trait
Trait.method : (T, A...) -> R
-----------------------------------
Gamma; Constraints |- receiver.method(A...) : R
```

The compiler may use dictionaries, method tables, direct calls, or native
lowering later; those are backend decisions and must not change the source
typing judgment.

### Implication Evidence

Implication constraints are compiler-proven facts in `Constraints`. They are
not runtime operators, conversions, generators, or macros.

```text
Constraints prove T => Shape
Gamma(x) = T
Shape contains field name : String
---------------------------------
Gamma; Constraints |- x.name : String
```

The compiler must fail closed. If no explicit evidence proves the implication,
the program is rejected with `unproven_implication`. If multiple incompatible
evidence sources match, the program is rejected with `ambiguous_implication`.
If evidence is used outside the generic function, receiver method, impl, or
type declaration scope that introduced it, the program is rejected with
`implication_scope_error`. Runtime `where` guards do not introduce implication
evidence.

Valid implication evidence sources are:

- built-in core rules;
- explicit user declarations;
- generated binding manifests;
- shape definitions with guards;
- concrete closed types;
- already-proven trait/type facts.

Ad hoc name matching is not evidence. Accepted evidence must preserve
provenance so diagnostics, CoreIR metadata, backend validation, and future Lean
proof obligations can explain why the implication is valid.

The executable Lean family
`proofs/lean/Terlan/Type/ShapeImplication.lean` models closed, visibility-aware
structural evidence. It proves evidence well-formedness, required-field
projection, fail-closed rejection, provenance preservation, private/public
field separation, lexical evidence confinement, and identity-preserving
non-conversion semantics. Its scoped evaluation model also proves that
introducing implication evidence preserves exact function and branch results;
evidence cannot change a value or the result type selected by ordinary control
flow.

## CoreIR Preservation

For every accepted core form:

```text
If source parses under EBNF,
and source typechecks under this spec,
and target/capability validation succeeds,
then emitted CoreIR is well typed.
```

Every CoreIR-facing form must be classified in the machine-readable index as
one of:

- `lean-covered`
- `proof-model-required`
- `runtime-boundary`
- `artifact-only`

`lean-covered` rows must name a Lean anchor that appears in the Lean
conformance documentation or Lean source tree. Other rows must name the gate
that keeps the boundary explicit.

## Initial Enforced Scope

The first enforced scope is intentionally small:

- integer literals;
- string/binary literals;
- local variables;
- tuple expressions;
- homogeneous list expressions;
- named calls;
- case expressions;
- constructor patterns;
- trait-target calls;
- type parameters.

New language or CoreIR forms may expand this list, but they must update the
machine-readable index and pass `make core-typing-spec-check`.
