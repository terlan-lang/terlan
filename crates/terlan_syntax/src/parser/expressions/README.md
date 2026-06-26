# Parser Expressions Internals

This directory owns expression parsing. The implementation is centered on the
formal precedence chain and expression forms defined by canonical Terlan syntax.
Its most important boundary is that parser output records structure without
performing type or target validation.

## Responsibilities

- Parse expression precedence and postfix forms.
- Parse control expressions, literals, calls, and collections.
- Preserve spans and argument names for later diagnostics.

## Public Surface

- `mod.rs`: expression parser entry points.

## Core Model

Expression parsing lowers token streams into `SyntaxExprOutput` trees.

The main flow is:

1. Parse the precedence chain from low to high binding power.
2. Parse primary and postfix expression forms.
3. Attach source spans and child nodes.

Important invariants:

- Precedence must match the EBNF contract.
- Named arguments must remain parallel to source arguments.
- Parser recovery must not fabricate semantic types.

## Integration Points

- `parser::callables`: consumes expression bodies.
- `terlan_typeck`: consumes syntax expressions for type inference.

## Testing Notes

- Add parser fixtures for precedence, postfix calls, and control expressions.
