# Parser Callables Internals

This directory owns callable parsing helpers. The implementation is centered on
functions, receiver methods, constructors, and impl methods. Its most important
boundary is that callable syntax stays distinct from semantic resolution.

## Responsibilities

- Parse callable declarations and signatures.
- Preserve parameter names, defaults, mutability, and return types.
- Return syntax output without deciding semantic validity.

## Public Surface

- `mod.rs`: callable parsing entry points used by the parser.

## Core Model

Callable parsing turns token streams into structured syntax declarations.

The main flow is:

1. Parse visibility and callable head.
2. Parse parameters, defaults, and return type.
3. Parse body or signature terminator.

Important invariants:

- Parser output must keep source spans for diagnostics.
- Receiver syntax must not be confused with free function syntax.
- Semantic checks belong to later compiler phases.

## Integration Points

- `parser::expressions`: parses callable bodies.
- `terlan_typeck`: validates callable types and trait contracts.

## Testing Notes

- Add parser fixtures for new callable forms and span-sensitive failures.
