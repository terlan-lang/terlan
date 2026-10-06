# Terlan Grammar Notes

`TERLAN_SYNTAX_SPEC.ebnf` is the canonical grammar artifact. Keep it free of
commentary so grammar validation and diffs stay focused on productions.

This README owns the explanatory material: source conventions, semantic notes,
examples, and design rationale.

## Contract And Contextual Rules

The EBNF and the contextual restrictions documented here define canonical
source syntax. The production recognizer is the generated LALRPOP parser;
validation and AST lowering follow recognition. Tree-sitter is an editor
grammar with recovery support, not a second compiler validator. A disagreement
with this contract is a specification or implementation bug to resolve with
tests, not an alternative language definition.

`SyntaxSpec` consumes the complete input. `EOF` is an end-of-input condition,
not a source token; trailing whitespace and comments are permitted.

- A comprehension starts with a generator: `[x | x <- xs, x > 0]`.
  `[head | tail]` is list construction. A filter cannot precede the first
  generator, and a comma-separated sequence of filters alone is invalid.
- Positional call arguments precede named arguments. `f(1, value = 2)` is valid;
  `f(value = 2, 1)` is rejected during lowering. The compact `CallArgList`
  production intentionally leaves this ordering to contextual validation.
- Module calls use dotted imported namespaces, such as `Console.println(value)`.
- `TypeName`, `TypeVar`, `ConstName`, and `ConstructorAtom` name semantic roles
  of the same lexical `UpperIdent` class. Resolution determines their meaning.
- `AppliedTypeRef` denotes nominal type application. `TraitImplRef` additionally
  admits arguments such as `T => { name: String }` that bind structural evidence.
  It does not introduce a general type-parameter declaration list: arguments
  such as `const N: Int` are not accepted in an impl trait reference.
- Comparisons and casts associate to the left. `a < b < c` parses as
  `(a < b) < c`, not a Python-style comparison chain; ordinary type checking
  still applies. `value as Foo as Bar` likewise nests left to right.
- Comma and semicolon policies are production-specific. Value and pattern
  lists and call arguments permit trailing commas; type arguments, type
  parameters, and constraint lists currently do not. Universal trailing
  commas would require a coordinated language change, not an EBNF-only edit.
- Ordinary expression assignment targets indexed locations. Plain `x = y`
  is not general expression assignment; script bindings and named arguments
  have separate rules. `mut` receiver behavior is described under Methods.

The EBNF contract tests check production structure and reachability. Parser
regressions check acceptance, rejection, and AST classification; the Tree-sitter
corpus checks editor trees. These checks complement each other but do not yet
constitute an independent executable recognizer for every EBNF production.

## Shared Syntax Conformance Corpus

[`syntax_conformance.json`](fixtures/contract/syntax_conformance.json) maps each
named case to an EBNF production, independent EBNF acceptance, compiler acceptance
or rejection, and a compact AST shape for accepted input. Contextual rejections
can also pin a diagnostic.
The source examples and exact editor trees live in
[`syntax_contract.txt`](../../tree-sitter-terlan/test/corpus/syntax_contract.txt).
The Rust conformance test reads that same file; there are no copied compiler
fixtures to synchronize. Manifest schema 2 requires an `ebnf` outcome for every
case and a `contextual` explanation whenever grammar and compiler acceptance
differ. For example, the EBNF accepts positional arguments after named ones;
the documented static syntactic constraint rejects them.

The test-only reference recognizer lowers the EBNF contract into productions
and uses an Earley chart to check complete `SyntaxSpec` membership. It shares
the EBNF reader, but does not call Terlan's lexer, parser, or AST lowering.
Identifier, binding, number, and string patterns come from the EBNF and match
whole lexemes. Its independent lexical boundary policy handles global reserved
words, longest punctuation, whitespace, line comments, and block comments.
Reserved words must be kept consistent with the language's lexical policy;
contextual keywords remain available as identifiers.

Tree-sitter checks every expected tree, including `ERROR` and `MISSING` nodes
for recovery cases. The manifest distinguishes editor recovery from ordinary
acceptance and requires a reason whenever editor and compiler acceptance differ.
It currently records the editor's permissive named-argument ordering and type
list trailing commas. These are visible conformance differences, not compiler
syntax extensions. All examples are syntax tests; names and declared return
types need not form a well-typed program.

Run `make syntax-contract-check` for the combined gate. The compiler cases also
run in the ordinary Rust suite and `lalrpop-parser-parity-check`; editor cases
run through `npm run check:cli` in `tree-sitter-terlan`.

Changes to accepted syntax or tree interpretation must update the relevant
production or contextual rule and its positive/negative examples in the same
change. Review editor tree updates before accepting them. Do not change an
expected rejection merely to make a parser regression pass. New editor gaps
require an explicit reason in the manifest. Renamed or missing rules, duplicate
case IDs, unlisted source cases, missing AST expectations, unexplained divergence,
and EBNF/compiler/editor outcome drift fail the gate.

This corpus covers the initial high-risk rules rather than the whole language.
The reference recognizer is a bounded test oracle, not the production parser.
It supports the EOF and string-character predicates used by these cases;
opaque raw-block predicates and character classes are not implemented. If an
otherwise unsuccessful recognition encounters an unsupported predicate, it
reports an error instead of treating the input as rejected. State-budget and
input-size exhaustion also fail the test. This corpus does not establish
whole-language equivalence, embedded-language coverage, or AST semantics for
the EBNF. Extend the corpus and lexical/predicate coverage together.

## Keyed Containers

Terlan uses one field language for anonymous keyed values, nominal keyed values,
and destructuring.

```terlan
{ name: "Ada", age: 42 }
User { name: "Ada", age: 42 }

case value {
  { name: name } -> name;
  User { name: name } -> name
}
```

The syntax model is:

- `{ key: value }` is an anonymous keyed container. It can represent a map,
  object, document, JSON object, or any other container with fields whose
  concrete meaning is supplied by type context and lowering.
- `Type { key: value }` is a nominal keyed container.
- `{ a, b }` is a positional tuple.
- `{}` is the empty anonymous keyed container.
- `:` is the field separator for construction and destructuring.
- `#{ ... }` is not canonical Terlan source syntax.

Formatter output should include a space before nominal keyed container braces:
`User { name: "Ada" }`, not `User{name: "Ada"}`.

## Documentation Comments

Public documentation uses declaration-leading block comments:

```terlan
/**
 * @param value input value
 * @returns transformed value
 */
pub transform(value: Int): Int ->
  value.
```

Documentation blocks describe public APIs for humans and documentation tooling.
They are separate from `@annotation` metadata, which is consumed by compiler
phases.

Line comments beginning with `//` are implementation comments and are not public
documentation. The old `//!` and `///` public documentation forms are migration
debt and are not canonical Terlan source style.

## Annotations

Annotation values are typed compiler metadata, not runtime expressions.
Compiler-known annotation paths define schemas that are checked after parse:
required keys, optional keys, key spelling, value type, repeatability, and valid
declaration target.

Unknown annotation paths are preserved for macro or target-profile phases, but a
profile may reject them before lowering. Schemas should not force declarations
to repeat identity information already implied by module path, declaration name,
arity, receiver shape, or type signature.

Annotation schema validation is a compile-time gate. A declaration with missing
required keys, unknown keys, wrong value types, duplicate non-repeatable keys, or
an annotation/key applied to an invalid declaration kind is rejected before
semantic lowering, CoreIR, or backend emission.

`@pure` is compiler metadata for function and receiver-method declarations. It
is marker-only: source cannot pass trusted options or effect claims through
metadata. The compiler owns purity validation from the function body, and later
phases may use validated purity for guards, templates, native lowering, or
constant folding without changing runtime semantics.

## Visibility

Terlan source uses declaration-site visibility. A declaration is private to its
module unless marked `pub`.

- `pub` on a function exports the function by name and arity.
- `pub` on a type exports the type name and type-parameter arity.
- `pub` on an opaque type exports the type name and arity while hiding its
  representation.

Backend-specific export attributes are generated by backend emitters, not
accepted as Terlan source grammar.

## Type Alias Shorthand

A non-opaque source type alias without an explicit body is a singleton atom
alias. The compiler derives the atom name from the type name using snake case:

```terlan
pub type Hit.         // Atom["hit"]
pub type InvalidMove. // Atom["invalid_move"]
```

Generated interface summaries may still use bodyless type headers to declare a
nominal public type without exposing a body.

## Opaque Types

An opaque type declaration defines a nominal type whose representation is known
inside the defining module and hidden outside it.

Inside the defining module, the opaque type may be constructed from and coerced
to its representation type. Outside the defining module, only the opaque type
name and arity are visible. External code may use values of the opaque type but
may not construct, deconstruct, or coerce them through the representation type
unless public functions expose that behavior.

## Structs

Struct inclusion is compile-time expansion, not class inheritance. Inclusion
copies fields and eligible receiver functions from included parent structs. It
does not create subtyping, runtime parent objects, implicit coercions, virtual
dispatch, or true inheritance.

Field conflicts are errors. Method conflicts are errors unless an override rule
is later introduced. Traits are not included with `includes`; trait conformance
is expressed with `implements` or explicit `impl` declarations.

Struct visibility and construction authority are separate. `pub struct User`
exports the type identity so other modules may name `User` in signatures, fields,
and type arguments.

## Methods

A method may only be declared in the module that defines its receiver type.
Method identity is keyed by receiver type, method name, and method arity.

Receiver mutability belongs to the receiver binding inside the receiver
parentheses:

```terlan
pub (values: Vector[T]) len(): Int ->
    ...

pub (mut values: Vector[T]) push(value: T): Unit ->
    ...
```

Call sites do not carry mutation markers. Mutability is part of the method or
trait contract, so `values.push(item)` can update/rebind the receiver when
`push` is declared with `mut`, while read-only methods such as `values.len()`
remain observational.

## Modules And Imports

Terlan module paths are package-rooted dotted names. A module path begins with a
lower-case package segment and may continue with lower-case package segments or
upper-case public module namespace segments. This supports source paths such as
`std.core.Bool` while keeping package roots lower-case.

A module may expose a primary type whose name matches the final module segment,
similar to a typed default export. This is semantic resolution, not parser
behavior: the grammar preserves the path, and the resolver decides whether a
primary type exists and is visible.

Selective imports preserve identifier class. A lower-case imported symbol must
use a lower-case alias. An upper-case imported symbol must use an upper-case
alias. The parser does not distinguish type names from constructor names during
import parsing; semantic analysis owns that distinction.

Wildcard imports use the braced selector form:

```terlan
import std.core.Option.{*}.
```

Path-style wildcard imports such as `import std.core.Option.*.` are not
canonical.

## Raw Macros

Raw macro names are user-defined except for reserved built-in raw forms. The
parser accepts any non-reserved lower-case identifier followed immediately by a
raw block as a raw macro expression.

Built-in forms such as `html { ... }` are reserved and parse as dedicated source
constructs, not user-defined raw macros. Macro existence, visibility, imports,
expansion behavior, and raw-language parsing are resolved after parsing.

## Native Capabilities

VM native capability metadata is expressed through ordinary annotations on
ordinary declarations.

Compiler-owned Rust-backed standard operations use stable compiler-owned
operation ids. Native implementation details are package metadata and VM adapter
concerns, not separate source grammar.

## Higher-Kinded Types

Higher-kinded type parameters are canonical syntax for advanced functional
abstractions over type constructors. `F[_]` declares a unary type constructor
parameter, while `M[_, _]` declares a binary type constructor parameter.

Variance markers are part of the source contract, but semantic enforcement is
staged behind kind checking and trait-resolution support.

## Structural Implications

The implication arrow is accepted only as generic-parameter shorthand for a
closed structural field requirement:

```terlan
pub display_name[T => {name: String}](value: T): String ->
    value.name.
```

Implication targets must contain at least one named field. `=>` is not a runtime
operator, conversion, field decorator, or ordinary type-expression relation.
The parser, formatter, EBNF, and editor grammar preserve this form. Callable
typechecking grants only the declared fields inside the generic body, validates
local and imported closed struct shapes at call sites, recursively validates
nested shapes, permits constrained generic callers to forward stronger
evidence, and applies the same rules to generic receiver methods. Missing,
incompatible, private, or dynamic evidence fails closed with
`unproven_implication`; field access outside the active shape reports
`implication_scope_error`. Implications do not create runtime wrappers or
symbols.

## Varargs

Varargs parameters bind all remaining positional values into one list value
inside the callable body. The repeated parameter must be last and cannot declare
a default value.

## Canonical Formatting

`terlc fmt` owns source layout. Canonical Terlan uses four-space indentation
and a 100-column target. Calls, collections, records, maps, and declarations
that do not fit are written one item per line; multiline comma-separated forms
carry a trailing comma wherever the grammar permits it. Fluent call chains put
each method on its own continuation line. `case` and `if` arms are indented one
level inside their structural block, and imports are normalized into a stable
order.

Run `terlc fmt <path>` to rewrite a directory, or `terlc fmt --check <path>...` to
verify it without mutation. Formatting is deterministic and idempotent.
Generated files marked with both `@generated true` and `@do-not-edit true`
remain owned by their generator and are skipped during recursive formatting.

## Let Expressions

Boolean conjunction and disjunction use the canonical keyword operators `and`
and `or`. Symbolic aliases such as `&&` and `||` are not Terlan source syntax.

A `let` expression is one explicit binding followed by a required result
expression. Consecutive bindings are recursive let expressions, so every local
binding repeats the `let` keyword.

```terlan
let x = expr; x.
let x = 1; let y = 2; x + y.
```

Bindings never double as return values. The final expression after the binding
sequence is the value of the `let` expression. The retired implicit form
`let x = 1; y = 2; ...` and comma-grouped bindings are rejected. Run
`terlc fmt --migrate-repeated-lets <path>` on 0.0.6 source to migrate implicit
continuation bindings.

One lexical region cannot introduce the same immutable name twice. Function
parameters and the function body's top-level sequential `let` chain share a
region, so `run(x) -> let x = ...; ...` is rejected rather than treated as an
assignment or equality assertion. Structural patterns also reject duplicate
names. Use a guard or explicit `==` for an identity condition.

Intentional shadowing remains valid in a genuinely nested branch, lambda,
comprehension, or handler scope. The nested name denotes a fresh identity and
does not alter the outer value. See
`docs/compiler/TERLAN_BINDING_IDENTITIES.md` for the complete region contract.

A refutable binding uses `<-` with an `else` fallback. Multiple bindings use
the canonical grouped `let { ... } else { ... }` form, not a separate `with`
construct, and share one fallback. Each right-hand side runs once from left to
right. The first mismatch dispatches its value to the fallback clauses.
Success-bound names are available to later bindings and the final expression,
but not to the shared fallback. The braces delimit only the refutable binding
group; they are not a general block-expression form.

A single refutable binding omits the grouping braces:

```terlan
let Ok(user) <- Users.find(id) else {
    Err(reason) -> Err(reason)
};
Ok(user).
```

Two or more refutable bindings use the grouped form:

```terlan
let {
    Ok(user) <- Users.find(id);
    Ok(account) <- Accounts.find(user.account_id)
} else {
    Err(reason) -> Err(reason)
};
Ok(account).
```

## If Expressions

If expressions are clause-based and do not use an `else` keyword.

```terlan
if {
  ready -> run();
  _ -> wait()
}
```

Target profiles must require exhaustiveness before backend emission. For
boolean-style conditions, use explicit `true`/`false` clauses or a final `_`
fallback clause.

## String Capture Patterns

String capture patterns combine literal segments with `${name}` or
`${name: Type}` captures. An inferred capture has type `String`:

```terlan
let "assets/${bucket}/${file}.txt" = path;
bucket + "/" + file.
```

A typed capture converts the delimited text to the declared type before the
clause is selected:

```terlan
case request_line {
    "GET /users/${id: Int}" where id > 0 -> id;
    _ -> 0
}.
```

Captures are ordinary patterns and are valid in `let`, `case`, function-head,
and lambda positions. A `where` guard may constrain captured values. Every pair
of adjacent captures must have a non-empty literal delimiter; otherwise the
capture boundary is ambiguous and parsing fails.

Matching covers the entire string. An intermediate delimiter ends a capture at
its first occurrence; a trailing literal is an anchored suffix, and a final
capture consumes the remainder. String captures may be empty. Native typed
captures support `String`, `Int`, finite `Float`, and `Bool`; failed conversion
makes that clause fail before its guard runs. Other capture annotations produce
a compile-time conversion diagnostic rather than an interpreted fallback.

## Shape Synonyms

Shape synonyms are the reserved source form for reusable compile-time match
aliases. They use `shape` and `=`, not `pattern`, `=>`, or `->`:

```terlan
shape OkResponse(body) =
  {status, body} where status in 200..299.

pub shape UserAsset(id, file) =
  "users/${id: Int}/assets/${file}".

shape Route(method, path) =
  {method, path}.

pub user_id(request: Dynamic): Int ->
  case request {
    Route("GET", "/users/${id: Int}") -> id;
    _ -> -1
  }.
```

The intended semantics are compile-time expansion into ordinary pattern and
guard logic. A shape synonym must not allocate a wrapper, construct a runtime
value, or introduce a new nominal type. Exported shapes will become part of a
module's public matching API.

Current 0.0.7 status: shapes expand into ordinary patterns before typechecking
and execute through CoreIR and the VM in case clauses, function heads,
comprehensions, ordinary fallible lets, and grouped fail-fast lets. Shape guards
compose with explicit guards and comprehension filters. Local and imported
public shapes support tuples, lists, maps, records, constructors, typed string
captures, nesting, and hygienic private bindings. Generated JavaScript supports
the same expanded structural forms covered by the shape gate. Duplicate
bindings, recursive or ambiguous expansion, impure or non-Boolean guards, and
malformed typed captures fail before runtime with stable diagnostics.

Route matching should use ordinary pattern matching and shape/extractor
composition, as in `Route("GET", "/users/${id: Int}")` above. The route shape
adds no runtime wrapper; the method literal and typed path capture are ordinary
patterns after expansion. Terlan does not have route-specific grammar for HTTP
dispatch.

## Identifiers

Reserved words are matched before identifiers. A lower-case identifier token must
not have the same source spelling as any reserved word. There are no reserved
upper-case words.

Source identifiers use explicit roles:

- `NameRef` for syntactic name positions.
- `ConstructorAtom` for constructor-pattern heads.

Terlan does not treat bare identifiers as atoms. `Atom["name"]` is the
language-neutral primitive for stable symbolic singleton values.
Colon-prefixed and single-quoted atom spellings are rejected, including in
patterns and type annotations. Values and types use the same quoted payload
syntax.

## Calls

Call expressions are written `Name(...)` and are resolved semantically as either
function calls or constructor calls depending on declaration context.

Function-value invocation uses ordinary postfix call syntax: `f(10)` invokes the
value of `f` as a callable value.

Function-head pattern parameters may destructure a typed argument directly in
the function head. The pattern annotation belongs to the whole pattern:

```terlan
pub add({left, right}: {Int, Int}): Int ->
    left + right.
```

Clause-style heads use the same spelling:

```terlan
pub describe({status, body}: Dynamic): String.
describe({200, body}) -> body;
describe(_) -> "unknown".
```

Pattern-first aliasing keeps the whole value and its destructured bindings:

```terlan
pub full_name({name, family_name} = user: User): String ->
    user.id.to_string() + ": " + name + " " + family_name.
```

Reverse alias syntax such as `user = {name}: User` is rejected permanently.
Pattern parameters also reject default values; use plain named parameters for
defaults.

Pipes prefer receiver-method resolution. `value |> method(extra)` resolves as
`value.method(extra)` when `method` is available for `value`. If no receiver
method exists, the pipe falls back to ordinary call insertion. If both are valid,
semantic analysis reports an ambiguity unless the call is explicitly qualified.

## Runtime Boundaries

Terlan source grammar does not include legacy runtime interop forms. Historical
compatibility lanes are validation artifacts, not source syntax obligations.

Process send/receive semantics are not core Terlan syntax. VM-owned messaging
behavior is modeled with ordinary typed libraries and compiler capabilities.

Raw pointer syntax is not part of ordinary Terlan source mode. Native memory work
is expressed through typed abstractions such as fixed arrays, buffers, resources,
opaque native handles, and explicit native boundaries.
