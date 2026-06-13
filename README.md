# Terlan

### Write once. Compile everywhere.

Terlan is an open source, statically typed, functional programming language for
building safe, predictable, industrial-strength software across server, native,
and web platforms. Terlan uses Erlang/BEAM as its reliability core, then
supplements it with access to the Rust and JavaScript ecosystems
through explicit compiler targets.

Terlan favors immutable data, explicit types, and predictable control flow,
while remaining practical for object-style APIs, platform interop, and rich
domain modeling. If you have worked in the industry across multiple server stacks, everything about Terlan should feel familiar and intuitive.

## Hello World

The value proposition of Terlan is best demonstrated in the following example: 

```terlan
module hello_terl.Main.

import std.io.Console.{println}.

pub main(): Unit ->
    println("Hello Terl").
```

This compiles to Erlang:

```erlang
-module(hello_terl_main).

-export([main/0]).

main() ->
    begin io:format("~ts~n", ["Hello Terl"]), unit end.
```

## Status

Current version: `0.0.2`

Terlan is in a very early experimental stage. The compiler, standard library,
syntax, and release tooling are still changing quickly.

Input, issues, experiments, and design feedback are especially welcome at this stage.
If you want to support the project, please star the repository.

## Install

Install the Linux x86_64 release artifact:

```sh
curl -fsSL https://raw.githubusercontent.com/terlan-lang/terlan/main/install.sh | sh
```

Or install from a release checkout with Rust:

```sh
cargo install --path crates/terlan_cli --bin terlc --force
terlc version
```

## Erlang/OTP

Terlan is validated against Erlang/OTP 29 and requires an OTP 29.x
installation for the Erlang target. `terlc build` and `terlc test` invoke
`erlc` and `erl`, so both commands must be available on `PATH`.

Check the installed OTP release:

```sh
erl -noshell -eval 'io:format("~s~n", [erlang:system_info(otp_release)]), halt().'
```

The command should print:

```text
29
```

Install OTP 29 from the official Erlang downloads page:

```text
https://www.erlang.org/downloads/29
```

The official source-build instructions are here:

```text
https://www.erlang.org/doc/system/install.html
```

For a quick container check, Erlang publishes an OTP 29 image:

```sh
docker run -it erlang:29
```

## Create And Run

Create a new project:

```sh
terlc init hello
cd hello
```

Build it:

```sh
terlc build
```

Run it:

```sh
./_build/bin/hello
```

Expected output:

```text
hello from Terlan
```

## Test

`terlc init` creates a sample test file:

```text
tests/hello/main_test.tl
```

Run it with:

```sh
terlc test tests/hello/main_test.tl
```

Expected output:

```text
running 1 tests
test hello_text_is_stable ... ok
test result: ok. 1 passed; 0 failed
```

## Current Scope

0.0.2 expands Terlan from the first usable-program milestone into a
language-feature and base-standard-library milestone. It supports simple
Terlan programs, manifest-backed BEAM builds, runnable `_build/bin/<package>`
launchers, Terlan test files executed through the Erlang target, receiver
methods, trait conformance, function-value calls, semicolon-separated
expression sequencing, generated stdlib interface summaries, and an initial
portable standard-library surface for core values, collections, console I/O,
and text file I/O.
