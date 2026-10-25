# Typed CSS

CSS property modules expose a `Property` struct whose field accepts the allowed
keyword enums and typed units directly. Include these structs to compose an
application style:

```terlan
import std.css.properties.BoxSizing.{BorderBox}.
import type std.css.properties.BoxSizing.{Property as SizingProperty}.

pub struct TileStyle includes SizingProperty {}.

pub tile(): TileStyle ->
    TileStyle {box_sizing: BorderBox}.
```

The field supplies the property name, and its union type supplies the permitted
inputs. Keyword values need no wrapping function or serialized property record.
For example, `animation_direction: Alternate` emits `animation-direction: alternate`.
CSS-wide `Inherit`, `Initial`, `Unset`, `Revert`, and `RevertLayer` are accepted by
every property.

Each style chooses its declarations by including the properties it needs.
For a conditional declaration, define an application field such as
`animation_direction: Option[Alternate]`; `None` omits it and `Some(Alternate)`
emits it. `Some(None)` emits the CSS keyword `none` when that keyword belongs to
the field's permitted enum. Its distinction from an omitted declaration follows
the checked optional field type.

`std.css.Values` supplies lengths, percentages, colors, angles, and times.
Unconstrained inputs are assigned directly, such as `margin_left: px(-10.0)`.
Color fields accept `Transparent` and `CurrentColor` directly; `rgb` validates
channel bounds. Units are constrained by their shared types.

Numeric ranges retain actual validators. `Width.length(px(30.0))` returns
`Some(checked)`, while a negative width returns `None`. Assign the checked result
to the width field. A checked width cannot be substituted for a checked height.
Properties that need these validators expose a `Checked` type; keyword-only
properties have no opaque result type or constructor.

Bounded shorthands accept keywords and unconstrained typed units directly:

```terlan
import std.css.Values.{px, root_em}.
import std.css.properties.Margin.
import std.css.properties.Margin.{Auto}.

pub centered(): Margin.Checked ->
    Margin.pair(px(0.0), Auto).

pub spacing(): Margin.Checked ->
    Margin.quad(root_em(1.0), px(0.0), root_em(1.5), root_em(1.0)).
```

`Component` is the permitted input union for that shorthand. Padding components
retain `component_length` and `component_percentage` validators because their
amounts must be nonnegative. Complete shorthand values and CSS-wide keywords
cannot be passed as components. `MarginInline` permits two components; `Margin`
permits four. There are 48 supported repetitions.

Named `&&` and `||` groups expose `compose` with distinct typed part unions:

```terlan
import std.core.Option.{None, Some}.
import std.css.Values.{px, rgb}.
import std.css.properties.Border.
import std.css.properties.Border.{Solid}.
import type std.core.Option.

pub unit_border(): Option[Border.Checked] ->
    case {Border.line_width_length(px(2.0)), rgb(127, 136, 26)} {
        {Some(width), Some(color)} -> Some(Border.compose(width, Solid, color));
        _ -> None
    }.
```

Width and style part unions constrain the arguments. Checked numeric parts
cannot be forged outside their module. This API covers 18 properties, including
borders, outlines, flex-flow, and text decoration. Partial combinations of `||`
groups are future work. Compounds retain typed sequences and tuples until the
stylesheet output boundary, where their parts are serialized in order.

Generation keeps standards-track CSS implemented without prefixes or flags in
at least one current released browser. Pinned MDN compatibility and grammar data
filter unimplemented proposals, including additions to implemented properties.
A specification's editor-draft URL alone does not exclude an implemented feature.
Obsolete generated modules and interfaces are removed during generation.

An application stylesheet exports a pure `stylesheet/0` function returning a
list of `std.css.Stylesheet.Rule[T]` values, optionally wrapped in `Result`.
A shared style struct with optional properties can represent different rule
shapes. An error result fails the build. Style fields use the generated
snake_case property names; optional fields may use `None` to omit a declaration.
The build checks property identity and preserves rule and declaration order.
Struct inclusion uses the language's normal field-conflict diagnostics.

Register CSS and Terlan stylesheets in their intended cascade order:

```toml
[web.assets]
directory = "assets"
styles = ["assets/base.css", "src/example/Styles.terl"]
```

`terlc build --target js.browser` evaluates the Terlan functions, bundles the
styles with Lightning CSS, and sends the result through the managed browser
asset pipeline. Configured Terlan stylesheet modules are build inputs and are
excluded from browser JavaScript emission. A JavaScript entrypoint is optional.
Remove equivalent CSS imports from handwritten JavaScript to avoid loading a
stylesheet twice. Custom Rsbuild configurations cannot be combined with this
manifest-owned stylesheet path.

The managed toolchain owns Lightning CSS and its version. Media URLs retain
their original source location before the browser bundler copies their assets.
CSS source maps are emitted; typed declarations currently map to their generated
intermediate CSS. Handwritten CSS keeps its original file mapping.

## Generation and verification

Generation uses the checkout's compiler for canonical Terlan formatting.
Build it first, or set `TERLC` to the compiler executable to use.

```sh
npm ci --prefix tools/css --ignore-scripts
npm run generate --prefix tools/css
terlc interface std/css/Values.terl std/css/Stylesheet.terl std/css/properties/*.terl --out-dir std/summaries
make typed-css-check
```

The generator covers 570 implemented property definitions. Its first version exposes
CSS-wide keywords and complete atomic alternatives resolved from the grammar,
including referenced types and numeric ranges, plus bounded space-separated
repetitions of up to four components and complete compositions of named
`&&`/`||` groups with up to four terms. Other compound forms, CSS functions,
typed custom properties, and typed at-rules are future work; use handwritten CSS
for those forms. `bindings.json` records the exact dataset version and checksum,
source grammars, supported direct values and checked forms, and specification links. It also
records the pinned adoption datasets, implemented browsers, and reasons for
excluding 182 property definitions. Presence in the package means at least one
released browser implements the property; support across every browser is not
required.

The compiler fixture at `tests/fixtures/typed-css` exercises a CSS-only build.
It supplies a self-contained stylesheet with typed dimensions, spacing,
and borders alongside a handwritten CSS entry.

## Generated author documentation

Each generated module describes the property's purpose, its initial value,
inheritance and percentage behavior when available, and links to the specification
and author guide. Keyword documentation explains the effect of each documented
choice. Numeric validators document accepted ranges and failure results;
shorthand functions document component order and four-side expansion.

Generation reads the checked-in `tools/css/documentation-data.json` snapshot.
It combines MDN contributor prose at the recorded content commit with Microsoft
CSS value descriptions and a small set of authored specification summaries in
`documentation-overrides.json`. To refresh MDN content explicitly, check out the
recorded `mdn/content` revision and run:

```sh
node tools/css/collect-documentation.mjs /path/to/mdn/content
npm run generate --prefix tools/css
```

MDN-derived purpose and value descriptions are adapted under
[CC-BY-SA 2.5](../../tools/css/LICENSE-MDN.md), with source links in each module.
Additional Microsoft value descriptions retain their
[MIT notice](../../tools/css/LICENSE-Microsoft.md). These notices apply to the
attributed documentation; generated Terlan code retains the repository license.
