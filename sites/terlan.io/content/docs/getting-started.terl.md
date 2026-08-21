@page {
  title = "Getting started"
  description = "Install Terlan and compile a first program."
  section = "Documentation"
  nav_title = "Getting started"
  parent = "/docs"
  weight = 10
  kind = "docs"
  layout = "DocsLayout"
  aliases = ["/start"]
}

This guide will become the canonical short path from a clean machine to a
running Terlan program.

## Install

Release installers and their verification commands will live here before the
public website ships. Until then, build the compiler from the repository with
the pinned Rust toolchain.

## Create a project

```sh
terlc init hello-terlan
cd hello-terlan
terlc build
```

The generated project includes a package manifest, source module, and test
module. The compiler reports unsupported syntax before producing an artifact.

## Next step

Read the [language guide](docs/language/) for the type, function, and pattern
syntax used by generated projects.
