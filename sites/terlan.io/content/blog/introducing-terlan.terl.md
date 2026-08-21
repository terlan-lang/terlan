@page {
  title = "Introducing the Terlan documentation stack"
  description = "The first terlan.io slice uses Terlan's own typed static-site primitives."
  section = "Blog"
  nav_title = "Introducing terl-docs"
  parent = "/blog"
  weight = 10
  kind = "blog"
  date = "2026-08-15"
  summary = "Why terlan.io is built on the compiler's typed static-site primitives."
  authors = ["Terlan team"]
  tags = ["documentation", "compiler"]
  layout = "DocsLayout"
}

The first version of `terlan.io` is being built with the same language and
compiler primitives it documents.

## Why build on `terlc static`?

Markdown parsing, typed layouts, validated routes, asset copying, live preview,
and GitHub Pages base paths already belong to the compiler. The `terl-docs`
layer adds documentation-specific navigation, local search, and ordered blog
collections without introducing a second rendering engine.

## What works now

Every documentation build produces a deterministic local search index and a
small browser client with no hosted search dependency. Dated posts also produce
a newest-first blog catalog. The documentation layer also adds validated,
ordered navigation, breadcrumbs, and previous/next links; archive layouts and
feeds follow after that.
