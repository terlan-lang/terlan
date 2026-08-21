# `terl-docs` executable roadmap

Status date: 2026-08-16

## Outcome

`terl-docs` is the Docsy-equivalent layer for Terlan static sites. It must use
`terlc static` for Markdown, typed templates, routes, assets, local preview,
output validation, and GitHub Pages base paths. It owns documentation-specific
content models and generated artifacts; it must not grow a second Markdown or
template engine.

The first consumer is `terlan.io`, published as static files by GitHub Pages.
The module remains reusable by package documentation sites.

## Architecture boundary

```text
content/*.terl.md + templates/*.terl.html
                  |
                  v
       terlc static compiler primitives
                  |
       typed pages and validated routes
                  |
                  v
  terl-docs search, navigation, blog, feeds
                  |
                  v
       public/ -> GitHub Pages artifact
```

`terl-docs` accepts compiler-neutral page records. This keeps content ordering,
search schemas, and blog behavior independently testable while the compiler
continues to own syntax and rendering safety.

The repository boundary is intentionally wider than the compiler-core
boundary. Terlan core owns API-documentation extraction and general static-site
primitives. The first-party `terl-docs` workspace module owns documentation
product policy such as navigation, browser search, blog catalogs, feeds, and
sitemaps. `sites/terlan.io` owns project-specific content, presentation, and
deployment. All three live in this repository and share CI without making blog
or search behavior part of the language semantics.

## Milestones and gates

### M0 — Contracts and local search (complete)

- [x] Create `crates/terl-docs` with a versioned content artifact schema.
- [x] Define `@page.description`, `section`, `kind`, and `date` metadata.
- [x] Generate deterministic, base-path-relative `search-index.json`.
- [x] Ship a dependency-free search client using text-only DOM operations.
- [x] Add a Terlan docs layout containing the accessible search widget.

Gate: `cargo test -p terl-docs` and the focused `terlc static` integration test
must prove deterministic page ordering, safe relative URLs, searchable text,
and emitted browser assets. A site enables this layer with
`terlc static emit <site.terl> --docs --as-of YYYY-MM-DD --base-path /`.

### M1 — Documentation information architecture (complete)

- [x] Add ordered navigation metadata (`weight`, `parent`, and `nav_title`).
- [x] Add route aliases with collision validation.
- [x] Generate a nested navigation tree, breadcrumbs, and previous/next links.
- [x] Add active-link state and a responsive navigation layout.
- [x] Add table of contents and heading anchors.
- [x] Reject duplicate sibling weights, orphan pages, and navigation cycles.
- [x] Reject broken internal links and route aliases that collide.

Gate: one fixture with at least three nested sections passes link checking,
keyboard navigation, HTML validation, and GitHub Pages project-prefix tests.

### M2 — Small blogging engine

- [x] Define blog classification and strict `YYYY-MM-DD` publication dates.
- [x] Generate a newest-first, versioned `blog-index.json` catalog.
- [x] Support drafts, authors, tags, and explicit summaries.
- [x] Generate paginated archive, tag, and author pages through typed layouts.
- [ ] Generate Atom feed, sitemap entries, canonical URLs, and social metadata.
- [x] Keep draft posts out of production builds while allowing an explicit
  preview mode.
- [x] Keep future-dated posts out of production builds with a deterministic
  release-date policy.

Gate: fixtures prove stable ordering, pagination boundaries, draft/future
filtering, valid feed XML, and absence of unpublished posts from search.

### M3 — `terlan.io` product surface

- [x] Add a repository-owned site module with responsive starter styling,
  landing content, documentation seeds, and a real dated post.
- [ ] Create the visual system and responsive layouts for landing, docs, API,
  blog, and 404 pages.
- [ ] Import the getting-started guide, language guide, package guide, and
  generated API reference without duplicating source-of-truth documents.
- [ ] Add syntax highlighting, copy buttons, callouts, tabs, and edit-page links.
- [ ] Meet WCAG 2.2 AA color, focus, landmark, and reduced-motion requirements.
- [x] Add a pinned Playwright browser suite for component contracts, local
  search, and keyboard focus behavior.
- [ ] Move browser-owned search behavior from the bootstrap JavaScript client
  into a Terlan `js.browser` script using the standard AngularTS runtime.
- [x] Let `terlc static --docs` emit a managed browser bundle so the AngularTS
  search directive ships without vendoring a runtime or exposing
  Rsbuild configuration to the site.
- [x] Move search ranking policy into a VM-tested Terlan module and verify its
  checked browser artifact against `terlc build --target js.browser`.
- [ ] Add a narrow `terl-playwright` binding after Terlan has an executable
  `js.node` target; until then Playwright remains a TypeScript test harness.

Gate: a production-like site builds from a clean checkout with no external
search service or client framework and passes automated accessibility and link
checks.

### M4 — GitHub Pages release

- [x] Add a least-privilege Pages workflow using `upload-pages-artifact` and
  `deploy-pages`, pinned action revisions, and concurrency cancellation.
- [x] Build once with `/` and once with a project prefix to prevent path leaks.
- [ ] Configure `CNAME` for `terlan.io`, a custom 404, robots.txt, and sitemap.
- [ ] Cache the Rust toolchain/build without caching generated public output.
- [ ] Add a pull-request artifact preview and a production deployment smoke
  check for HTML, search index, search client, and feed.

Gate: the Pages artifact is reproducible from a clean checkout, contains no
absolute project-path assumptions, and the deployed smoke check succeeds.

## Definition of done

The roadmap is complete when a contributor can add one Markdown documentation
page or dated post, run one local command, and receive validated navigation,
search, blog archives, feeds, and a GitHub Pages-ready artifact without editing
generator code or JavaScript indexes by hand.
