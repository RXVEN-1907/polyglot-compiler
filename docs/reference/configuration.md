# Configuration Reference

`polyglot.toml` — the project configuration file, in TOML format.

## File Location

Polyglot reads `polyglot.toml` from the directory you build: `polyglot build` checks the current directory, `polyglot build path/to/project` checks that directory. There is no upward search and no `--project-dir` flag in v0.2.0.

## Recognized Schema

```toml
[project]
name = "demo"             # informational in v0.2.0
version = "0.1.0"         # informational

[compiler]
cc = "/opt/llvm-17/bin/clang"   # C compiler: absolute path, PATH name, or relative path
cxx = "/opt/llvm-17/bin/clang++" # C++ compiler, same rules

[language]
rust = true               # build src/main.rs with rustc

[target]
triple = "x86_64-unknown-linux-gnu"  # reserved (not yet honored)
opt_level = 2                        # reserved (not yet honored)

[plugins]
clang = "0.1"             # reserved (not yet honored)

[[bin]]
name = "demo"             # HONORED: output executable name
source = "src/main.c"     # HONORED: source to compile (first [[bin]] wins)
cflags = ["-Wall"]        # reserved (not yet honored)
ldflags = ["-lm"]         # reserved (not yet honored)
```

## Key Reference

### `[compiler]` — honored

| Key | Type | Description |
|-----|------|-------------|
| `cc` | string | Compiler executable for C sources. Absolute path, path looked up on `PATH`, or path relative to the current directory. |
| `cxx` | string | Compiler executable for C++ sources, same rules. |

An unresolvable value aborts the build:

```text
error: polyglot.toml [compiler] cc = '/nonexistent/cc' does not resolve to an executable
```

`POLYGLOT_CC` / `POLYGLOT_CXX` environment variables override both keys.

### `[language]` — honored

| Key | Type | Description |
|-----|------|-------------|
| `rust` | bool | When `true`, the project builds `src/main.rs` via `rustc` (see below). |

### `[[bin]]` — `name` and `source` honored

| Key | Type | Description |
|-----|------|-------------|
| `name` | string | Output name for the project executable |
| `source` | string | Source file, relative to the config file's directory |
| `cflags` | array | Reserved — not read in v0.2.0 |
| `ldflags` | array | Reserved — not read in v0.2.0 |

Only the first `[[bin]]` table is used. Without `source`, the build falls back to the first `src/*.c` / `src/*.cpp` file.

### `[project]`, `[target]`, `[plugins]` — reserved

Written by `polyglot init` and accepted by the parser, but not yet honored when building. Set target and optimization on the command line (`polyglot --target ... --opt-level ... build`).

## Rust Project Behavior

With `[language] rust = true`, `polyglot build` (or `polyglot build <project-dir>`) runs:

```text
rustc <rust-opt-flag> src/main.rs -o <[[bin]] name>
```

and skips the C/C++ pipeline entirely. `rustc` is discovered like any other tool (`doctor` reports its path); optimization levels map to `-Copt-level=` values.

## Generated Template

`polyglot init NAME` writes:

```toml
[project]
name = "NAME"
version = "0.1.0"

[target]
triple = "x86_64-unknown-linux-gnu"
opt_level = 2

[plugins]
rustc = "0.1"

[[bin]]
name = "NAME"
source = "src/main.c"
```

## Path Resolution

`[[bin]] source` is relative to the directory containing `polyglot.toml`:

```toml
[[bin]]
source = "src/main.c"   # <project>/src/main.c
```

## Precedence

1. `POLYGLOT_CC` / `POLYGLOT_CXX` environment variables (highest)
2. `polyglot.toml` `[compiler] cc` / `cxx`
3. CLI `--compiler` and toolchain auto-discovery (lowest)

## Validation

`polyglot build` validates on use: malformed TOML warns and skips config, missing `[[bin]] source` files raise `Source file not found`, and a project with no discoverable source raises `No source file found in project`.

## Next Steps

- [CLI Reference](cli.md)
- [Targets Reference](targets.md)
- [Project Workflow](../user-guide/project-workflow.md)
