@page {
  title = "Language guide"
  description = "A concise map of Terlan modules, functions, types, and patterns."
  section = "Documentation"
  nav_title = "Language"
  parent = "/docs"
  weight = 20
  kind = "docs"
  layout = "DocsLayout"
}

Terlan programs are organized into modules with explicit imports and public
declarations. Function signatures put the return type before the body, keeping
the contract visible at the declaration boundary.

## A small module

```text
module hello.Main.

pub greeting(name: String): String ->
    "Hello, " + name.
```

## Values and patterns

Algebraic data types and exhaustive patterns describe application states
without sentinel values. Effects remain visible in function types so callers
can distinguish pure transformations from runtime work.

This page is intentionally a seed. The next content pass will derive the full
reference from the language specification and compiler examples rather than
maintaining a second source of truth.
