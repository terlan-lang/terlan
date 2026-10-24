# Typed CSS

CSS properties are generated from the pinned `@webref/css` specification dataset.
Each `std.css.properties.Name` module exposes a nominal `Value`, a `Property`
struct for inclusion, and constructors for supported values. Width values cannot
be assigned to height properties; raw strings cannot become property values.

Use `OptionalProperty` to compose partial style blocks. Its field has type
`Option[Value]`: initialize absent declarations with `None` and present ones
with `Some(value)`. A shared empty style value can be specialized with ordinary
record updates, as demonstrated by Battleship.

Compose application styles with ordinary struct inclusion:

```terlan
import type std.css.properties.Width.{Property as WidthProperty}.
import type std.css.properties.Height.{Property as HeightProperty}.

pub struct Dimensions includes WidthProperty, HeightProperty {}.
pub struct TileStyle includes Dimensions {}.
```

`std.css.Values` supplies typed lengths, percentages, colors, angles, and times.
Property constructors enforce numeric ranges from the grammar. For example,
`Width.length(px(30.0))` returns `Some(value)`, while a negative width returns
`None`. Negative margins remain valid. RGB construction checks channel bounds.

Bounded space-separated shorthands expose a separate nominal `Component` type.
Use `component_length`, `component_percentage`, or supported keyword constructors
such as `component_auto`, then combine them with `single`, `pair`, `triple`, or
`quad` as permitted by the specification's repetition bounds:

```terlan
import std.css.Values.{px, root_em}.
import std.css.properties.Margin.

pub centered(): Margin.Value ->
    Margin.pair(Margin.component_length(px(0.0)), Margin.component_auto()).

pub spacing(): Margin.Value ->
    Margin.quad(
        Margin.component_length(root_em(1.0)),
        Margin.component_length(px(0.0)),
        Margin.component_length(root_em(1.5)),
        Margin.component_length(root_em(1.0)),
    ).
```

Component constructors preserve the specification's numeric ranges: for
example, `Padding.component_length` returns `Option[Padding.Component]` and
rejects negative lengths. Components from another property, complete shorthand
values, and CSS-wide keywords cannot be passed to a composition function.
`MarginInline` permits two components, while `Margin` permits up to four.
The generator currently finds supported repetitions in 56 property definitions.

Named `&&` and `||` groups expose `compose`, which supplies every group in
specification order. Each group with keywords, alternatives, or range checks
has its own opaque part type and prefixed constructors. A group containing one
unconstrained primitive accepts that shared type directly, such as `Color`:

```terlan
import std.core.Option.{None, Some}.
import std.css.Values.{px, rgb}.
import std.css.properties.Border.
import type std.core.Option.

pub unit_border(): Option[Border.Value] ->
    case {Border.line_width_length(px(2.0)), rgb(127, 136, 26)} {
        {Some(width), Some(color)} ->
            Some(Border.compose(width, Border.line_style_solid(), color));
        _ -> None
    }.
```

Width and style parts cannot be interchanged, and parts from another property
or CSS-wide keywords cannot fill those slots. Numeric constraints are checked
before parts are constructed. This API currently covers 47 properties,
including borders, outlines, flex-flow, and text decoration. `compose` supplies
all groups; APIs for partial combinations of `||` groups are future work.

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

The generator covers 752 property definitions. Its first version exposes
CSS-wide keywords and complete atomic alternatives resolved from the grammar,
including referenced types and numeric ranges, plus bounded space-separated
repetitions of up to four components and complete compositions of named
`&&`/`||` groups with up to four terms. Other compound forms, CSS functions,
typed custom properties, and typed at-rules are future work; use handwritten CSS
for those forms. `bindings.json` records the exact dataset version and checksum,
source grammars, supported constructor forms, and specification links. Draft
definitions can be newer than browser implementations.

The compiler fixture at `tests/fixtures/typed-css` exercises a CSS-only build.
The sibling Battleship application demonstrates the mixed path: its typed
`battleship.Styles` module owns board dimensions, tile sizing, setup controls,
disabled-state styles, placeholder spacing, automatic page centering, and
unit-state borders alongside its existing handwritten stylesheets.
