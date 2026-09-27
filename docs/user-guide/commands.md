# Commands Reference

Every command below is documented from the actual `polyglot <command> --help` output of the v0.2.0 driver. The full command set:

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

Options go **before** the command name:

```console
usage: polyglot [-h] [-v] [--quiet] [--target TARGET]
                [--opt-level {none,less,default,aggressive,size,size-aggressive}]
                [-o OUTPUT] [--plugin PLUGIN] [--plugin-dir PLUGIN_DIR]
                [--compiler {auto,clang,gcc}]
                {build,run,check,emit-ir,emit-asm,preproc,deps,doctor,targets,fmt,tidy,
                 repl,jit,mca,analyze,watch,stats,init,clean,version} ...
```

| Option | Description |
|--------|-------------|
| `-v`, `--verbose` | verbose output (full command lines, cache HIT/MISS) |
| `--quiet` | suppress `Running:` lines |
| `--target TARGET` | target triple (clang cross or host on gcc) |
| `--opt-level {none,less,default,aggressive,size,size-aggressive}` | optimization level (default: `default`) |
| `-o OUTPUT`, `--output OUTPUT` | output file |
| `--plugin PLUGIN` | JSON-RPC plugin binary (name or path; used by `build`) |
| `--plugin-dir PLUGIN_DIR` | plugin search directory (default: `~/.polyglot/plugins`) |
| `--compiler {auto,clang,gcc}` | force a backend (default: auto-discover; env `POLYGLOT_CC`/`POLYGLOT_CXX` override) |

Environment variables: `POLYGLOT_CC` and `POLYGLOT_CXX` select the C/C++ compiler outright and beat everything else, including `polyglot.toml` `[compiler]`. `NO_COLOR` disables colored output.

## Shared Compile Flags

`build`, `run`, `check`, `emit-ir`, `emit-asm`, `preproc`, `deps`, `mca`, `stats`, and `watch` accept the same core compile options:

| Option | Description |
|--------|-------------|
| `--std STD` | language standard (`c89..c2x` / `gnu*`, `c++98..c++23`) |
| `--warn {none,default,all,extra,pedantic}` | warning level (default: `default`; `all` = `-Wall`, `extra` = `-Wall -Wextra`, `pedantic` = `-Wall -Wextra -Wpedantic`) |
| `--werror` | warnings as errors |
| `--debug` | emit debug info (`-g`) |
| `--sanitize LIST` | sanitizers: `address,undefined,thread,memory,leak` (`memory` requires the clang backend) |
| `-I DIR` | include search path (repeatable) |
| `-D NAME[=VAL]` | preprocessor define (repeatable) |
| `--cc-arg ARG` | raw flag passed to the compiler (repeatable) |
| `-L DIR` | library search path (repeatable) |
| `-l NAME` | link library (repeatable) |

---

## build

```
usage: polyglot build [-h] [-o OUTPUT] [--std STD] [--warn {none,default,all,extra,pedantic}]
                      [--werror] [--debug] [--sanitize LIST] [-I DIR] [-D NAME[=VAL]] [--cc-arg ARG]
                      [-L DIR] [-l NAME] [--shared] [--static-lib] [-j JOBS] [--no-cache]
                      [paths ...]
```

- `paths` — source files or project directory
- `--shared` — build a shared library
- `--static-lib` — build a static library (`ar rcs`)
- `-j JOBS`, `--jobs JOBS` — parallel per-TU compilation
- `--no-cache` — bypass the incremental object cache

```bash
polyglot build hello.c -o hello
polyglot build main.c utils.c -o myapp      # multiple TUs
polyglot build                              # project build (reads polyglot.toml)
polyglot build -j 8 .                       # parallel project build
polyglot build --shared math.c -o libmath.so
```

## run

```
usage: polyglot run [-h] [--std STD] [--warn ...] [--werror] [--debug] [--sanitize LIST]
                    [-I DIR] [-D NAME[=VAL]] [--cc-arg ARG] [-L DIR] [-l NAME] [-j JOBS]
                    [--no-cache]
                    file
```

`file` — source file to run. Program arguments go after a `--` separator:

```bash
polyglot run hello.c
polyglot run argtest.c -- hello world       # arguments reach the program
```

The child process's exit code becomes polyglot's exit code.

## check

```
usage: polyglot check [-h] [--std STD] [--warn ...] [--werror] [--debug] [--sanitize LIST]
                      [-I DIR] [-D NAME[=VAL]] [--cc-arg ARG]
                      paths [paths ...]
```

Syntax-only check (`-fsyntax-only`). Accepts multiple files.

```bash
polyglot check src/*.c
```

## emit-ir

```
usage: polyglot emit-ir [-h] [-o OUTPUT] [--std STD] [--warn ...] [--werror] [--debug]
                        [--sanitize LIST] [-I DIR] [-D NAME[=VAL]] [--cc-arg ARG]
                        paths [paths ...]
```

- clang backend: LLVM IR (`-S -emit-llvm`)
- gcc backend: GIMPLE (`-fdump-tree-gimple`), with a note pointing at the `.gimple` file

```bash
polyglot --compiler clang emit-ir hello.c -o hello.ll   # true LLVM IR
polyglot emit-ir hello.c                                # GIMPLE on gcc
```

## emit-asm

```
usage: polyglot emit-asm [-h] [-o OUTPUT] [--intel] [--std STD] [--warn ...] [--werror]
                         [--debug] [--sanitize LIST] [-I DIR] [-D NAME[=VAL]] [--cc-arg ARG]
                         paths [paths ...]
```

`--intel` — Intel syntax (`-masm=intel`).

```bash
polyglot emit-asm hello.c -o hello.s --intel
```

## preproc

```
usage: polyglot preproc [-h] [-o OUTPUT] [--no-lines] [--std STD] [--warn ...] [--werror]
                        [--debug] [--sanitize LIST] [-I DIR] [-D NAME[=VAL]] [--cc-arg ARG]
                        file
```

Runs the preprocessor (`-E`). `--no-lines` strips line markers (`-P`); `-o` writes to a file instead of stdout.

```bash
polyglot preproc hello.c --no-lines -o hello.i
```

## deps

```
usage: polyglot deps [-h] [-o OUTPUT] [--target-name NAME] [--std STD] [--warn ...] [--werror]
                     [--debug] [--sanitize LIST] [-I DIR] [-D NAME[=VAL]] [--cc-arg ARG]
                     file
```

Make-style dependencies (`-MM`). `--target-name NAME` renames the target (`-MT`); `-o` writes a depfile instead of printing.

```console
$ polyglot deps src/main.c --target-name build/main.o
build/main.o: src/main.c
```

## doctor

```
usage: polyglot doctor [-h] [--strict]
```

Reports compilers, auxiliary tools, and the build-cache location. `--strict` exits `1` when anything is missing.

## targets

```
usage: polyglot targets [-h]
```

Lists the known target triples with their status for the discovered backend:

```console
$ polyglot targets
TARGET TRIPLE                      STATUS           NOTES
x86_64-unknown-linux-gnu           native           this host's architecture
aarch64-unknown-linux-gnu          needs-cross-gcc  GCC requires 'aarch64-linux-gnu-gcc' (apt install gcc-aarch64-linux-gnu)
arm-unknown-linux-gnueabihf        needs-cross-gcc  GCC requires 'arm-linux-gnu-gcc' (apt install gcc-arm-linux-gnu)
riscv64-unknown-linux-gnu          needs-cross-gcc  GCC requires 'riscv64-linux-gnu-gcc' (apt install gcc-riscv64-linux-gnu)
wasm32-wasip1                      unavailable      wasm linking requires wasm-ld/lld
```

## fmt

```
usage: polyglot fmt [-h] [--style {llvm,google,chromium,mozilla,webkit,microsoft,file}]
                    [--check] [-i] [--dump-config] paths [paths ...]
```

- `--style` — clang-format style (default `llvm`)
- `--check` — exit `1` if any file would change
- `-i` — format files in place
- `--dump-config` — print the style config and exit (requires a `paths` argument)

See [Tool Integrations](tools.md#formatting-with-fmt).

## tidy

```
usage: polyglot tidy [-h] [--checks CHECKS] [--fix] [paths ...]
```

clang-tidy lint. Extra clang-tidy arguments go after `--`. Because check strings start with `-`, pass them with `=`: `--checks="-*,modernize-*"`.

## repl

```
usage: polyglot repl [-h]
```

Starts an interactive clang-repl session. Extra arguments go after `--`.

## jit

```
usage: polyglot jit [-h] file
```

Runs an LLVM IR `.ll` or bitcode `.bc` file with `lli`. Extra `lli` arguments go after `--`.

## mca

```
usage: polyglot mca [-h] [--iterations ITERATIONS] [--mcpu MCPU] [--std STD] [--warn ...]
                    [--werror] [--debug] [--sanitize LIST] [-I DIR] [-D NAME[=VAL]]
                    [--cc-arg ARG]
                    file
```

`file` is a `.c` source or `.s` assembly file. `--iterations` (default 3) and `--mcpu` feed llvm-mca. See [Tool Integrations](tools.md#machine-code-analysis-with-mca).

## analyze

```
usage: polyglot analyze [-h] paths [paths ...]
```

clang static analyzer (`--analyze`). Requires the clang backend — with GCC discovered, it exits with a hint:

```
error: the static analyzer requires the clang backend (retry with: polyglot --compiler clang analyze ...)
```

## watch

```
usage: polyglot watch [-h] [--timeout TIMEOUT] [--std STD] [--warn ...] [--werror] [--debug]
                      [--sanitize LIST] [-I DIR] [-D NAME[=VAL]] [--cc-arg ARG]
                      file
```

Poll-and-rebuild loop. `--timeout N` exits after N seconds (`0` = watch forever).

## stats

```
usage: polyglot stats [-h] [--target TARGET]
                      [--opt-level {none,less,default,aggressive,size,size-aggressive}]
                      [--json] [--std STD] [--warn ...] [--werror] [--debug] [--sanitize LIST]
                      [-I DIR] [-D NAME[=VAL]] [--cc-arg ARG]
                      paths [paths ...]
```

`--json` switches to machine-readable JSON (source list, sizes, backend, compiler version, `total_ms`, `binary_size_bytes`, per-phase `timings`). See [Cache and Parallelism](cache-and-parallelism.md#json-output-for-ci).

## init

```
usage: polyglot init [-h] [--language {c,cpp,rust}] [name]
```

Scaffolds `polyglot.toml`, `src/main.c` (or `main.cpp` / `main.rs`), `README.md`, and `.gitignore`. Rust projects set `[language] rust = true` and build through `rustc`.

## clean

```
usage: polyglot clean [-h] [path]
```

Removes `*.o`, `*.obj`, `*.exe`, `a.out`, `*.ll`, `*.s`, `*.bc` in the directory, then clears `.polyglot-cache/`. Project executables named in `polyglot.toml` are left alone by default.

## version

```
usage: polyglot version [-h]
```

```console
$ polyglot version
polyglot 0.2.0
backend: /bin/gcc (gcc, gcc (Ubuntu 13.3.0-6ubuntu2~24.04.1) 13.3.0)
run polyglot doctor for details
```

---

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | Success |
| `1` | Compile/link/run failure, missing file, tool returned an error |
| `2` | Usage error (bad flag value, missing required argument, invalid `--std`) |
| `3` | Required tool or backend not available (e.g. `analyze` without clang) |

`run` propagates the child program's exit code; Ctrl+C exits `130`.

---

## Plugin Architecture

`build` can hand compilation to a **plugin** — a separate executable that
speaks the JSON-RPC 2.0 protocol implemented by
[`crates/polyglot-protocol`](../../crates/polyglot-protocol) (protocol
version `0.1.0`, `PROTOCOL_VERSION` in `crates/polyglot-protocol/src/types.rs`).

The shipped plugin is `polyglot-clang` (C/C++ frontend). Build it with:

```console
cargo build --release -p polyglot-clang
```

### Resolution order

`--plugin NAME` (used by `build`; other commands ignore it) resolves to an
executable in this order:

1. an explicit path, when `NAME` contains a `/`
2. `--plugin-dir`/`NAME` (also `polyglot-NAME`, and `.exe` variants; default dir `~/.polyglot/plugins`)
3. the workspace's `target/release/NAME`
4. `PATH`

An explicit `--plugin` is mandatory: if it cannot be found, or the
`initialize` handshake fails, the build exits `3` with a hint to run the
`cargo build` command above. Plugins listed in `polyglot.toml`
`[plugins]` are best-effort: a missing or unusable one prints a warning and
the build falls back to the direct compiler. Without `--plugin` (and
without `[plugins]`), nothing changes — the direct compiler is used.

### Wire protocol

Newline-delimited JSON-RPC 2.0 on the plugin's stdin/stdout (one JSON
object per line); the plugin's logs go to stderr. Methods: `initialize`,
`compile`, `shutdown` (requests) and `exit` (notification). A `compile`
request carries `{source_path, args, target_triple, opt_level, ...}` and
returns `{mlir_module, diagnostics}` — for `polyglot-clang`, `mlir_module`
is LLVM IR text (emitted via `clang -S -emit-llvm`). Compilation errors
come back as JSON-RPC error `-32001` with clang's diagnostics, which the
driver prints verbatim and exits `1`.

### Verified example

```console
$ cargo build --release -p polyglot-clang
$ polyglot --plugin target/release/polyglot-clang build examples/01_hello.c -o /tmp/pgplug
Built examples/01_hello.c -> /tmp/pgplug (via plugin polyglot-clang)
$ /tmp/pgplug
Hello, Polyglot!
```

`polyglot doctor` reports detected plugins (it scans `--plugin-dir` and
`target/release`) on its `plugins` line.

---

## Configuration File

See [Configuration](../getting-started/configuration.md) for `polyglot.toml` details.
