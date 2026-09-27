# CLI Reference

Generated from `polyglot --help` and `polyglot <command> --help` (v0.2.0).

```
polyglot [GLOBAL OPTIONS] <COMMAND> [COMMAND OPTIONS]
```

## Command Summary

| Command | Description |
|---------|-------------|
| `build` | Build a source file or project |
| `run` | Build and run a source file |
| `check` | Check syntax without building |
| `emit-ir` | Emit LLVM IR (clang) or GIMPLE (gcc) |
| `emit-asm` | Emit assembly |
| `preproc` | Run the preprocessor (`-E`) |
| `deps` | Emit Make-style dependencies (`-MM`) |
| `doctor` | Report on the toolchain environment |
| `targets` | List known target triples and availability |
| `fmt` | Format code with clang-format |
| `tidy` | Lint with clang-tidy (extra args after `--`) |
| `repl` | Interactive clang-repl (args after `--`) |
| `jit` | Run a `.ll`/`.bc` with lli (args after `--`) |
| `mca` | Analyze machine code with llvm-mca |
| `analyze` | clang static analyzer (`--analyze`) |
| `watch` | Rebuild on file changes |
| `stats` | Compilation statistics |
| `init` | Initialize a new project |
| `clean` | Remove build artifacts and cache |
| `version` | Show version and backend |

## Global Options

| Option | Description |
|--------|-------------|
| `-h`, `--help` | show help and exit |
| `-v`, `--verbose` | verbose output (full command lines, cache HIT/MISS) |
| `--quiet` | suppress `Running:` lines |
| `--target TARGET` | target triple (clang cross or host on gcc) |
| `--opt-level {none,less,default,aggressive,size,size-aggressive}` | optimization level (default: `default`) |
| `-o OUTPUT`, `--output OUTPUT` | output file |
| `--plugin PLUGIN` | plugin to use (legacy) |
| `--plugin-dir PLUGIN_DIR` | plugin directory (legacy) |
| `--compiler {auto,clang,gcc}` | force a backend (default: auto-discover; env `POLYGLOT_CC`/`POLYGLOT_CXX` override) |

## Optimization Levels

| Level | Flag | GCC/clang flag |
|-------|------|----------------|
| `none` | `--opt-level none` | `-O0` |
| `less` | `--opt-level less` | `-O1` |
| `default` | `--opt-level default` | `-O2` |
| `aggressive` | `--opt-level aggressive` | `-O3` |
| `size` | `--opt-level size` | `-Os` |
| `size-aggressive` | `--opt-level size-aggressive` | `-Oz` (gcc >= 12 and clang) |

## Shared Compile Options

Accepted by `build`, `run`, `check`, `emit-ir`, `emit-asm`, `preproc`, `deps`, `mca`, `stats`, `watch`:

| Option | Description |
|--------|-------------|
| `--std STD` | language standard (`c89..c2x` / `gnu*`, `c++98..c++23`) |
| `--warn {none,default,all,extra,pedantic}` | warning level (default: `default`) |
| `--werror` | warnings as errors |
| `--debug` | emit debug info (`-g`) |
| `--sanitize LIST` | sanitizers: `address,undefined,thread,memory,leak` (`memory` requires the clang backend) |
| `-I DIR` | include search path (repeatable) |
| `-D NAME[=VAL]` | preprocessor define (repeatable) |
| `--cc-arg ARG` | raw flag passed to the compiler (repeatable) |
| `-L DIR` | library search path (repeatable) |
| `-l NAME` | link library (repeatable) |

---

## Per-Command Options

### `build`

```
polyglot build [-o OUTPUT] [--std STD] [--warn LEVEL] [--werror] [--debug]
               [--sanitize LIST] [-I DIR] [-D NAME[=VAL]] [--cc-arg ARG]
               [-L DIR] [-l NAME] [--shared] [--static-lib] [-j JOBS] [--no-cache]
               [paths ...]
```

| Option | Description |
|--------|-------------|
| `-o`, `--output OUTPUT` | output file |
| `--shared` | build a shared library |
| `--static-lib` | build a static library (`ar rcs`) |
| `-j`, `--jobs JOBS` | parallel per-TU compilation |
| `--no-cache` | bypass the incremental object cache |
| `paths` | source files or project directory |

### `run`

```
polyglot run [--std STD] ... [--no-cache] file
```

| Argument | Description |
|----------|-------------|
| `file` | source file to run |

Program arguments follow a `--` separator: `polyglot run app.c -- --input f.txt`. The child's exit code is propagated.

### `check`

```
polyglot check [--std STD] ... paths [paths ...]
```

`paths` — source files to check (`-fsyntax-only`).

### `emit-ir`

```
polyglot emit-ir [-o OUTPUT] [--std STD] ... paths [paths ...]
```

clang emits LLVM IR (`-S -emit-llvm`); gcc emits GIMPLE (`-fdump-tree-gimple`).

### `emit-asm`

```
polyglot emit-asm [-o OUTPUT] [--intel] [--std STD] ... paths [paths ...]
```

`--intel` — Intel syntax (`-masm=intel`).

### `preproc`

```
polyglot preproc [-o OUTPUT] [--no-lines] [--std STD] ... file
```

`--no-lines` strips line markers (`-P`); `-o` writes to a file instead of stdout.

### `deps`

```
polyglot deps [-o OUTPUT] [--target-name NAME] [--std STD] ... file
```

`--target-name NAME` renames the make target (`-MT`); `-o` writes the depfile instead of stdout.

### `doctor`

```
polyglot doctor [--strict]
```

`--strict` exits `1` when anything is missing.

### `targets`

```
polyglot targets
```

No options. Prints known triples, their status (`native`, `needs-cross-gcc`, `unavailable`) and notes. See [Targets](targets.md).

### `fmt`

```
polyglot fmt [--style {llvm,google,chromium,mozilla,webkit,microsoft,file}] [--check] [-i]
             [--dump-config] paths [paths ...]
```

| Option | Description |
|--------|-------------|
| `--style` | clang-format style |
| `--check` | exit `1` if any file would change |
| `-i` | format files in place |
| `--dump-config` | print the style config and exit |

### `tidy`

```
polyglot tidy [--checks CHECKS] [--fix] [paths ...]
```

Check strings start with `-`, so use `--checks="-*,modernize-*"`. Extra clang-tidy arguments go after `--`.

### `repl`

```
polyglot repl
```

Interactive clang-repl; extra arguments go after `--`.

### `jit`

```
polyglot jit file
```

`file` — LLVM IR (`.ll`) or bitcode (`.bc`); runs it with `lli`. Extra `lli` arguments go after `--`.

### `mca`

```
polyglot mca [--iterations ITERATIONS] [--mcpu MCPU] [--std STD] ... file
```

`file` — `.c` source or `.s` assembly. `--iterations` defaults to 3.

### `analyze`

```
polyglot analyze paths [paths ...]
```

clang static analyzer; requires the clang backend.

### `watch`

```
polyglot watch [--timeout TIMEOUT] [--std STD] ... file
```

`--timeout N` — exit after N seconds (`0` = watch forever).

### `stats`

```
polyglot stats [--target TARGET] [--opt-level LEVEL] [--json] [--std STD] ... paths [paths ...]
```

`--json` produces machine-readable output (see [Cache and Parallelism](../user-guide/cache-and-parallelism.md#json-output-for-ci)).

### `init`

```
polyglot init [--language {c,cpp,rust}] [name]
```

### `clean`

```
polyglot clean [path]
```

Removes object/executable/IR/assembly artifacts and clears `.polyglot-cache/`.

### `version`

```
polyglot version
```

---

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | Success |
| `1` | General error: compile/link/run failure, file not found, tool error |
| `2` | Usage error: bad flag value, missing required argument |
| `3` | Missing tool or backend (e.g. `analyze` without clang) |

`run` exits with the child program's exit code; Ctrl+C exits `130`.

## Environment Variables

| Variable | Description |
|----------|-------------|
| `POLYGLOT_CC` | absolute path or name of the C compiler (highest precedence) |
| `POLYGLOT_CXX` | absolute path or name of the C++ compiler (highest precedence) |
| `NO_COLOR` | disable colored output |

Toolchain discovery order: `POLYGLOT_CC`/`POLYGLOT_CXX` > `polyglot.toml` `[compiler] cc/cxx` > `--compiler {auto,clang,gcc}` > `PATH` (clang, clang-NN, gcc, cc) > scan dirs (`~/llvm-install/*/bin`, `/usr/lib/llvm-*/bin`, `/usr/local/bin`, `/opt/*/bin`).

## Configuration File

See [Configuration](../getting-started/configuration.md) for `polyglot.toml`.

## Target Triples

See [Targets](targets.md) for the full list and the `polyglot targets` command.
