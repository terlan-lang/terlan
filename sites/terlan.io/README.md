# `terlan.io`

This is the first consumer of `terl-docs` and the repository-owned source for
the future `terlan.io` GitHub Pages artifact.

Build and validate it from the repository root:

```sh
npm ci --prefix tools/web-toolchain
sites/terlan.io/scripts/check.sh
```

Run the browser suite after installing its pinned development dependency and
Chromium:

```sh
npm ci --prefix sites/terlan.io
npx --prefix sites/terlan.io playwright install chromium
npm --prefix sites/terlan.io test
```

Run the local development server:

```sh
target/debug/terlc static serve \
  sites/terlan.io/src/terlan_io/Site.terl \
  --out-dir sites/terlan.io/public \
  --source-dir sites/terlan.io \
  --docs \
  --as-of 2026-08-16 \
  --base-path /
```

Replace `--as-of 2026-08-16` with `--preview` when reviewing drafts or scheduled
posts. The normal command deliberately uses an explicit publication cutoff and
excludes unpublished content from HTML, aliases, navigation, search, and the
blog catalog.

The typed blog layout also renders generated archive, tag, and author pages.
Their route manifest is `blog-collections.json`; production rebuilds use it to
remove collection pages that existed only because preview content was visible.

The checked-in source owns content, layout, and presentation. Generated files
under `public/` are build output and must not be committed.

Terlan compiler core owns the reusable static-site and future API-documentation
primitives. The sibling `crates/terl-docs` module owns navigation, search, and
blog behavior; this site only supplies their project-specific content and
presentation.

The visual component contract comes from the dirty local `angular.css`
snapshot under `vendor/angular.css`. Tailwind is used only while authoring and
compiling that upstream component library. This site consumes ordinary CSS and
customizes semantic tokens in `assets/site.css`; its production build has no
Tailwind dependency.

Playwright remains a TypeScript test harness for now. Terlan has no executable
Node target yet, and its JS test path is validation-only. Browser application
logic belongs in Terlan scripts compiled for `js.browser`. A future narrow
`terl-playwright` wrapper becomes executable only after a `js.node` host can
run generated Terlan modules; it should bind stable Playwright test operations
rather than mirror the entire TypeScript API.

Search ranking policy lives in
`../../crates/terl-docs/terlan/src/terl_docs/Search.terl` and is executable
through the Terlan VM as well as emittable for `js.browser`. The browser adapter
is registered as an AngularTS directive and bundled by `terlc static --docs`
through the compiler-managed web toolchain selected by the exact
`[target.js.dependencies]` entry. It owns normalization, index loading, and DOM
updates until those browser calls are supported by the release JS emitter.
This is an explicit adapter boundary, not a second ranking implementation.
