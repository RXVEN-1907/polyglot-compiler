# Quick Start

Polyglot does **not** require clang to be on your `PATH`. The driver discovers whatever C/C++ toolchain you have — GCC or clang, on or off `PATH` — and reports exactly what it found.

## Install

If you have not installed the driver yet, the fastest route is pipx:

```bash
pipx install git+https://github.com/RXVEN-1907/polyglot-compiler
```

See [Installation](installation.md) for the pip route, `install.sh`, from-source setup, and prerequisites (Python 3.8+, `toml`, any one C compiler).

## Step 0: Run the Doctor

The first command to run is `polyglot doctor`. It inventories your compilers and auxiliary tools and tells you how to fix anything missing:

```console
$ polyglot doctor
==============================================================
POLYGLOT ENVIRONMENT REPORT (doctor)
==============================================================
  driver          polyglot 0.3.0 (python 3.12.6)
  ok   C compiler     /bin/gcc [gcc (Ubuntu 13.3.0-6ubuntu2~24.04.1) 13.3.0]
  ok   C++ compiler   /bin/g++
  backend         gcc
  hint            clang is not on PATH but exists at:
                    /home/you/llvm-install/clang+llvm-17.0.6-x86_64-linux-gnu-ubuntu-22.04/bin/clang
                  consider: export PATH=/home/you/llvm-install/...:$PATH
--------------------------------------------------------------
Auxiliary tools:
  ok   clang-format   /home/you/llvm-install/.../bin/clang-format
  ok   clang-tidy     /home/you/llvm-install/.../bin/clang-tidy
  ...
--------------------------------------------------------------
  build cache     /home/you/demo/.polyglot-cache (empty - will be created on first build)
==============================================================
All components present.
```

How toolchain discovery works (first match wins):

1. `POLYGLOT_CC` / `POLYGLOT_CXX` environment variables (absolute or command name)
2. `polyglot.toml` `[compiler] cc` / `cxx` keys (project builds)
3. `--compiler {auto,clang,gcc}` — force a backend
4. `PATH` — clang, clang-NN, gcc, cc
5. Well-known scan directories: `~/llvm-install/*/bin`, `/usr/lib/llvm-*/bin`, `/usr/local/bin`, `/opt/*/bin`

On the machine above, clang is **not** on `PATH`, but the driver still finds it under `~/llvm-install/` — `doctor` prints the hint so you know it is there. With GCC 13 as the system compiler, everything works; the clang backend simply unlocks LLVM-specific commands such as `analyze`, `jit`, and LLVM-IR `emit-ir`.

Add `--strict` to make `doctor` exit `1` when anything is missing — useful as a CI preflight:

```bash
polyglot doctor --strict
```

Run `polyglot targets` to see which target triples are usable with your backend.

## The Five Commands

Everything below was executed with the GCC 13 backend; your output may differ in tool paths only.

### 1. `run` — build and execute in one step

```console
$ polyglot run src/main.c
Hello from Polyglot!
```

Use `--quiet` to hide the underlying compiler invocations, and `-v` to see more of them.

### 2. `build` — produce an executable

```console
$ polyglot build src/main.c -o hello
$ ./hello
Hello from Polyglot!
```

Point it at a directory containing `polyglot.toml` (or run it inside one) and it builds the project's `[[bin]]` target. Multiple source files are compiled in parallel with `-j`.

### 3. `check` — syntax-check without producing binaries

```console
$ polyglot check src/main.c
Checking src/main.c...
OK   src/main.c
All files passed syntax check
```

### 4. `emit-ir` — look inside the compiler

```console
$ polyglot emit-ir src/main.c
note: GCC backend emits GIMPLE (not LLVM IR) -> /path/to/main.gimple
```

The result is written to a file, not stdout: `main.gimple` on the GCC backend (via `-fdump-tree-gimple`), or `main.ll` — true LLVM IR — on the clang backend. Pick the destination with `-o`.

Assembly works the same way with `emit-asm` (add `--intel` for Intel syntax), and `stats` prints a compilation time report.

### 5. `init` — scaffold a new project

```console
$ polyglot init hello --language c
Project created at: hello
Run 'polyglot build' to build.
```

This creates `polyglot.toml`, a sample `src/main.c`, a `README.md`, and a `.gitignore`. Languages: `c` (default), `cpp`, and `rust` (Rust projects build through `rustc`).

Then `cd hello && polyglot build`.

## Common Variations

```bash
# Optimize (global option before the command)
polyglot --opt-level aggressive build hello.c -o hello-fast

# Warnings
polyglot build --warn extra --werror hello.c -o hello

# Debug info + sanitizers
polyglot build --debug --sanitize address,undefined hello.c -o hello-asan

# Force the clang backend even when gcc is first on PATH
polyglot --compiler clang build hello.c -o hello

# Shared / static libraries
polyglot build --shared math.c -o libmath.so
polyglot build --static-lib math.c -o libmath.a
```

## Clean Up

```bash
polyglot clean          # removes artifacts AND .polyglot-cache/
polyglot clean some-dir # cleans a specific project directory
```

## Next Steps

- [All Commands](../user-guide/commands.md) — every command and flag
- [Tool Integrations](../user-guide/tools.md) — fmt, tidy, analyze, mca, jit, repl
- [Cache and Parallelism](../user-guide/cache-and-parallelism.md) — incremental builds and `-j`
- [Configuration](configuration.md) — `polyglot.toml` reference
