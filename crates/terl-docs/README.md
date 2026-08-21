# `terl-docs`

`terl-docs` is the documentation and blog layer for Terlan static sites. It
uses the Markdown, typed-template, route, asset, validation, preview, and base
path behavior already provided by `terlc static`.

Enable the layer during a build:

```sh
terlc static emit src/site/Site.terl \
  --out-dir public \
  --validate-output \
  --docs \
  --as-of 2026-08-16 \
  --base-path /
```

This initial slice writes:

- `navigation.json`, containing the ordered documentation tree;
- `search-index.json`, containing every imported Markdown page;
- `assets/terl-docs/search.js`, the local browser adapter;
- `assets/terl-docs/search-policy.js`, emitted from the VM-tested Terlan
  ranking module;
- `blog-index.json`, when at least one page has `kind = "blog"`.
- `blog-collections.json` plus typed-layout archive, tag, and author pages when
  the site has a `/blog` page with `@page.layout`.

Markdown headings receive stable, duplicate-safe fragment anchors from the
compiler. The documentation layer uses level-two through level-four headings
to generate an accessible “On this page” navigation fragment for typed page
layouts. The generated anchors are removed from keyboard tab order while TOC
links remain ordinary focusable links.

`static check` also resolves every generated local anchor against the completed
output tree, including fragment ids and GitHub Pages project prefixes. When an
HTML base path is active, the emitter qualifies fragment-only Markdown and
template links with the current route so in-page navigation cannot leak to the
site root.

Documentation metadata is declared in the existing Terlan Markdown header:

```text
@page {
  title = "Installing Terlan"
  description = "Install the compiler and run the first program."
  section = "Getting started"
  aliases = ["/start", "/install"]
}
```

Aliases emit accessible `noindex` redirect pages to the canonical route.
Canonical routes, aliases, and explicit handler routes share one collision
boundary, so an output path can never be claimed twice.

A blog post also declares its kind and ISO publication date:

```text
@page {
  title = "Terlan 0.0.8"
  kind = "blog"
  date = "2026-09-01"
  summary = "What changed in the 0.0.8 release."
  authors = ["Terlan team"]
  tags = ["release", "compiler"]
}
```

Draft pages use `draft = true`. Production docs builds omit their HTML,
aliases, navigation entries, search documents, and blog records. Local review
uses the same pipeline with an explicit mode:

```sh
terlc static serve src/site/Site.terl --docs --preview --base-path /
```

Rebuilding a preview output directory without `--preview` removes previously
emitted draft HTML so stale pages cannot leak into a production artifact.

Production builds also require an explicit `--as-of YYYY-MM-DD` publication
cutoff. Blog posts dated after that cutoff are treated as scheduled content and
excluded through the same HTML, alias, navigation, search, and blog boundaries.
The explicit date keeps CI and local release builds reproducible instead of
depending on the machine clock. Preview mode includes scheduled posts and does
not accept `--as-of`.

The `/blog` page is the collection host. Its declared typed layout renders
generated pages at `/blog/archive/`, `/blog/tags/<tag>/`, and
`/blog/authors/<author>/`; additional pages use `/page/<number>/`. Collection
membership and URL slugs are deterministic, distinct labels may not collapse
to the same slug, and generated routes may not collide with declared routes or
aliases. Rebuilding a preview directory for production removes collection
pages that were supported only by drafts or scheduled posts.

The search client enhances semantic HTML supplied by the site layout. It does
not inject input controls, which keeps labels, placement, and styling owned by
the site. Sites that declare the pinned managed Angular.ts dependency add the
module name to the search root:

```html
<form role="search" ng-app="terlDocs" data-terl-docs-search>
  <label for="site-search">Search documentation</label>
  <input id="site-search" type="search" autocomplete="off"
         data-terl-docs-search-input>
  <ul aria-live="polite" data-terl-docs-search-results></ul>
</form>
<script type="module" src="assets/terl-docs/search.js"></script>
```

Search ranking has one source of truth in
`terlan/src/terl_docs/Search.terl`. Its checked JavaScript artifact is verified
against the release compiler with:

```sh
TERLC=target/debug/terlc crates/terl-docs/scripts/check-search-policy.sh
target/debug/terlc test crates/terl-docs/terlan --target terlan-vm
```

The browser adapter is intentionally thin: it loads the index, normalizes
query text, renders safe text nodes, and calls the Terlan policy for every
term. A project opts into the Angular.ts directive and compiler-owned Rsbuild
bridge with an exact target dependency:

```toml
[target.js.dependencies]
angular_ts = { npm = "@angular-wave/angular.ts", version = "0.32.0" }
```

`terlc static --docs` then emits one managed `search.js` bundle. Standalone
docs sources without that dependency retain the small framework-free ES
module and do not require Node. Moving the remaining browser DOM operations
into Terlan source awaits executable browser APIs in the `js.browser` target.

For GitHub Pages, always pass the production base path (`/` for `terlan.io` or
`/<repository>/` for a project site). Both generated artifact URLs and the
browser client resolve through that base.
