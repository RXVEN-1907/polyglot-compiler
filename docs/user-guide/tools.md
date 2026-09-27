# Tool Integrations

Polyglot 0.2.0 wraps the auxiliary tools that ship with LLVM so you do not have to remember their flags or their install locations. `polyglot doctor` shows which of these tools it found (clang tools can be discovered off `PATH` in scan directories such as `~/llvm-install/*/bin`).

| Tool | Command | Backing binary |
|------|---------|----------------|
| Formatter | `polyglot fmt` | `clang-format` |
| Linter | `polyglot tidy` | `clang-tidy` |
| Static analyzer | `polyglot analyze` | `clang --analyze` |
| Machine-code analyzer | `polyglot mca` | `llvm-mca` |
| JIT runner | `polyglot jit` | `lli` |
| Interactive C++ REPL | `polyglot repl` | `clang-repl` |

## Formatting with `fmt`

`polyglot fmt` is a clang-format frontend:

```bash
polyglot fmt -i src/*.c                          # format in place
polyglot fmt --style google -i src/*.c           # pick a style
polyglot fmt --check src/*.c                     # CI check (exit 1 if dirty)
polyglot fmt --dump-config src/main.c            # print resolved .clang-format
```

Paths must be files (shell-expand globs yourself); a directory path fails.

Available styles: `llvm`, `google`, `chromium`, `mozilla`, `webkit`, `microsoft`, `file` (use a `.clang-format` file).

### The `--check` workflow for CI

`--check` never writes files; it prints what would change and exits `1` if any file needs formatting. That makes it a one-line CI gate:

```yaml
# .github/workflows/format.yml
- name: Check formatting
  run: |
    polyglot fmt --check $(git ls-files '*.c' '*.cpp' '*.h' '*.hpp')
```

Captured failure output:

```text
error: 1 file(s) need formatting
would reformat: ugly.c
```

`--dump-config` prints the fully resolved style as a `.clang-format` document — handy as a starting point for a project style file:

```console
$ polyglot fmt --dump-config src/main.c
---
Language:        Cpp
# BasedOnStyle:  LLVM
AccessModifierOffset: -2
AlignAfterOpenBracket: Align
...
```

## Linting with `tidy`

`polyglot tidy` runs clang-tidy. Pass check sets with `=` syntax (the value starts with `-`, so `--checks -*,...` would be misread as a flag):

```bash
polyglot tidy "--checks=-*,modernize-*,bugprone-*" src/*.cpp
polyglot tidy "--checks=-*,modernize-*" --fix src/   # apply suggested fixes
```

Example finding captured on a one-line C++ file:

```console
$ cat null.cpp
void f(char *p) { if (p == 0) {} }
$ polyglot tidy "--checks=-*,modernize-use-nullptr" null.cpp
null.cpp:1:28: warning: use nullptr [modernize-use-nullptr]
    1 | void f(char *p) { if (p == 0) {} }
      |                            ^
      |                            nullptr
```

Without a `compile_commands.json`, clang-tidy runs without per-file flags and suppresses warnings from non-user code; system headers are skipped by default.

## Static Analysis with `analyze`

`polyglot analyze` runs clang's static analyzer (`clang --analyze`) over the given sources. It requires the clang backend; if the discovered compiler is GCC it exits `3` with a hint:

```text
error: the static analyzer requires the clang backend (retry with: polyglot --compiler clang analyze ...)
```

```bash
polyglot --compiler clang analyze src/*.c
```

Typical findings are null-dereference, use-after-free and uninitialized-value paths through your code.

## Machine-Code Analysis with `mca`

`polyglot mca` feeds generated machine code to `llvm-mca`, which simulates the CPU's out-of-order core and reports throughput and latency. It answers questions like *how much instruction-level parallelism does this loop have?* and *which ports are the bottleneck?*

Input is a `.c` source (compiled with your normal `--std`/`--opt-level` options first) or a `.s` assembly file. `--mcpu` selects the CPU model and `--iterations` sets the analysis window (default 3).

Captured example — a sum-of-squares loop compiled with the gcc backend (the tool in this install models an Intel Sapphire Rapids core):

```console
$ polyglot mca loop.c
Iterations:        3
Instructions:      48
Total Cycles:      109
Total uOps:        54

Dispatch Width:    6
uOps Per Cycle:    0.50
IPC:               0.44
Block RThroughput: 3.0
```

Below the summary, llvm-mca prints a per-instruction table (uOps, latency, reciprocal throughput, load/store flags) and per-resource port pressure, so you can see exactly which execution units are saturated.

## JIT Execution with `jit`

`polyglot jit` runs LLVM IR or bitcode directly with `lli` — no linking step. Produce IR with the clang backend, then execute it:

```console
$ polyglot --compiler clang emit-ir hello.c -o hello.ll
$ polyglot jit hello.ll
jit says hi
```

The file needs a `main` function; `lli` exits with an error like `Symbols not found: [ main ]` otherwise. Extra `lli` arguments go after `--`.

## Interactive C++ with `repl`

`polyglot repl` drops you into an interactive `clang-repl` session — a C++ interpreter with full clang semantics, including `#include` and standard-library types:

```console
$ polyglot repl
clang-repl> #include <iostream>
clang-repl> std::cout << 6 * 7 << std::endl;
42
clang-repl> %quit
```

Being an interactive terminal session, run it directly in your shell rather than in scripts. Extra clang-repl arguments go after `--`.

## Next Steps

- [Commands Reference](commands.md) — flags for each tool command
- [Cache and Parallelism](cache-and-parallelism.md) — build cache and `-j`
- [Optimization Guide](optimization.md) — choosing `--opt-level`
