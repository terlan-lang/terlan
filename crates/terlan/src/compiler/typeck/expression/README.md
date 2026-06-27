# Expression Typecheck Internals

This directory owns focused expression typechecking helpers. The implementation
splits call resolution, construction, control flow, and indexing from the root
expression module so each language feature can be tested independently.

## Responsibilities

- Typecheck expression subfamilies with stable diagnostics.
- Keep call and constructor resolution separate from control-flow rules.
- Route index and mutation semantics through compiler-owned type contracts.
- Preserve backend-neutral typed expression results.

## Public Surface

- `calls`: function, method, and constructor-call checking.
- `casts`: explicit `as` conversion inference and conversion-trait checks.
- `construction`: record, struct, map, list, and constructor construction.
- `control_flow`: case, if, let, and related flow expressions.
- `function_inference`: generic function inference, explicit type arguments,
  trait-bound validation, and trait lookup caching.
- `indexing`: index get/set validation.
- `operators`: unary, binary, boolean, comparison, arithmetic, and pipe
  operator inference.
- `sql`: SQL raw-form row type validation and wrapper result inference.
- `values`: variable, singleton alias, implicit value, and function-value
  lookup.

## Core Model

Expression typechecking transforms parsed expressions into typed expression
models used by later lowering. Each helper owns one family of expression rules
and returns diagnostics through the shared typecheck diagnostic model.

The main flow is:

1. Receive parsed expression nodes and typing context.
2. Resolve names, receiver methods, constructors, or expected types.
3. Return typed expression data or stable diagnostics.

Important invariants:

- Expression helpers cannot emit backend artifacts.
- Constructor-like syntax must resolve semantically before lowering.
- Mutable receiver and index behavior must remain explicit in typed results.

## Integration Points

- `crate::terlan_typeck::expression`: root expression typechecking dispatch.
- `crate::terlan_typeck::type_system`: type unification and type construction.
- `crate::terlan_typeck::receiver_methods`: receiver method lookup and validation.

## Edge Cases

- Ambiguous overloads must produce stable diagnostics.
- Unsupported target-specific expressions must fail before backend emission.
- Pipe and receiver behavior must preserve source evaluation order.

## Types And Interfaces

`calls`
: Checks ordinary calls, receiver methods, constructor calls, and function
value invocation.

`casts`
: Checks explicit conversion expressions without lowering backend artifacts.

`construction`
: Checks construction expressions and constructor chaining.

`control_flow`
: Checks expression forms that introduce flow-sensitive branches.

`function_inference`
: Checks generic function calls, explicit type arguments, and trait bounds.
  Submodules split explicit type-argument binding from trait-bound lookup/cache
  behavior.

`indexing`
: Checks indexed reads and writes through index contracts.

`operators`
: Checks expression operators while keeping pipe behavior routed through call
resolution.

`sql`
: Checks compiler-known SQL raw macros against visible row types.

`values`
: Checks bare names and singleton alias values.

## Testing Notes

- Expression behavior is covered by adjacent `expression_test.rs` and focused
  feature test modules.
- New expression features should add parser, typecheck, and lowering coverage.
- Diagnostics should be asserted by stable code and highlighted span.
