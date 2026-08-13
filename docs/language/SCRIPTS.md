# Terlan Scripts

Terlan uses `.terls` for executable scripts and `.terl` for declaration-only
modules. The distinction is semantic, not merely a discovery convention.

A `.terls` file may contain imports and helper declarations followed by
ordered top-level expressions:

```terlan
#!/usr/bin/env terlc
import std.io.Console.{println}.

message(name: String): String -> "hello " + name.

name = "Terlan";
println(message(name)).
```

The compiler derives a stable module identity from the path and synthesizes a
public typed `main/0` around the top-level expression sequence. Resolution,
type checking, Core lowering, Cranelift compilation, and VM execution are the
same maintained AOT pipeline used by applications. There is no script
interpreter.

At script top level, `name = value;` is shorthand for an immutable `let`
binding. Intermediate expression results may be ignored. The final expression
is the script result: `Unit` is silent and every other value is rendered to the
calling process. Inline `assert`, `assert_equal`, `assert_false`,
`assert_not_equal`, and `assert_true` calls use the standard typed test API;
a failed assertion terminates the script with a nonzero process status.

Scripts may not declare `module` or `main`; either is a source-mode error.
Only `.terls` accepts a first-line shebang. `terlc fmt`, `terlc lint`, direct
`terlc run`, `terlc run script <name>`, and `[scripts]` manifest aliases all
select script behavior from the extension.

Configured aliases use package-relative paths:

```toml
[scripts]
seed = "scripts/SeedDatabase.terls"
```

Run the alias with `terlc run script seed`. Project modules are compiled into
the application closure, but the selected script's synthetic `main/0` is the
only application root.
