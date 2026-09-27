# Polyglot Compiler

<div align="center">

![Polyglot Compiler](https://img.shields.io/badge/Polyglot-Compiler-blueviolet?style=for-the-badge&logo=llvm&logoColor=white)

**A universal compiler driver for C and C++ with real compiler inspection capabilities**

[![Build Status](https://img.shields.io/github/actions/workflow/status/RXVEN-1907/polyglot-compiler/ci.yml?branch=master&style=flat-square)](https://github.com/RXVEN-1907/polyglot-compiler/actions)
[![Release](https://img.shields.io/github/v/release/RXVEN-1907/polyglot-compiler?style=flat-square&logo=github)](https://github.com/RXVEN-1907/polyglot-compiler/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg?style=flat-square)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/Rust-1.75+-orange.svg?style=flat-square&logo=rust)](https://www.rust-lang.org)
[![Python](https://img.shields.io/badge/Python-3.8+-blue.svg?style=flat-square&logo=python)](https://www.python.org)
[![Clang](https://img.shields.io/badge/Clang-17+-brightgreen.svg?style=flat-square&logo=llvm)](https://clang.llvm.org)
[![MLIR](https://img.shields.io/badge/MLIR-Enabled-purple.svg?style=flat-square)](https://mlir.llvm.org)
[![Cranelift](https://img.shields.io/badge/Cranelift-Codegen-red.svg?style=flat-square)](https://github.com/bytecodealliance/wasmtime/tree/main/cranelift)

[![GitHub Stars](https://img.shields.io/github/stars/RXVEN-1907/polyglot-compiler?style=social)](https://github.com/RXVEN-1907/polyglot-compiler)
[![GitHub Forks](https://img.shields.io/github/forks/RXVEN-1907/polyglot-compiler?style=social)](https://github.com/RXVEN-1907/polyglot-compiler/network/members)
[![GitHub Issues](https://img.shields.io/github/issues/RXVEN-1907/polyglot-compiler?style=flat-square)](https://github.com/RXVEN-1907/polyglot-compiler/issues)
[![GitHub Discussions](https://img.shields.io/github/discussions/RXVEN-1907/polyglot-compiler?style=flat-square)](https://github.com/RXVEN-1907/polyglot-compiler/discussions)

</div>

---

## 🌟 Why Polyglot?

<table>
<tr>
<td width="50%">

### The Problem
Compiling C/C++ is fragmented:
- **Different flags** for every compiler (clang, gcc, msvc)
- **No unified interface** for IR/assembly inspection
- **Project config** scattered across Makefiles, CMake, build scripts
- **Cross-compilation** requires memorizing target triples
- **Compiler internals** are opaque—hard to learn what's happening

</td>
<td width="50%">

### The Polyglot Solution
```bash
# One tool, consistent interface
polyglot build main.c -o app           # Build
polyglot run main.c                    # Build + run
polyglot check main.c                  # Syntax check
polyglot emit-ir main.c -o out.ll      # See LLVM IR / GIMPLE
polyglot emit-asm main.c -o out.s      # See assembly
polyglot stats main.c --json           # Compile time report
polyglot fmt src/*.c --check           # clang-format gate
polyglot doctor                        # Environment report
polyglot init myproj --language c      # Scaffold project
```

| Feature | `clang` | `gcc` | `CMake` | **Polyglot** |
|---------|---------|-------|---------|--------------|
| Unified CLI | ❌ | ❌ | ❌ | ✅ |
| IR/ASM inspection | Manual | Manual | ❌ | **One flag** |
| Project config | ❌ | ❌ | Complex | **Simple TOML** |
| Cross-compile targets | Manual | Manual | Complex | **Named targets** |
| Optimization levels | `-O0`-`-O3` | `-O0`-`-O3` | Manual | **Named levels** |
| Plugin architecture | ❌ | ❌ | ❌ | **JSON-RPC 2.0** |

</td>
</tr>
</table>

---

## ✨ Key Features

| Feature | Description |
|---------|-------------|
| 🔀 **Multi-Backend** | Auto-discovers `clang`/`clang++` **or** `gcc`/`g++`; force with `--compiler clang\|gcc` or `POLYGLOT_CC`/`POLYGLOT_CXX` |
| 🧭 **Toolchain Discovery** | Env vars → `--compiler` flag → `PATH` → well-known LLVM install dirs; `polyglot doctor` reports what it found |
| ⚡ **Optimization Levels** | `none` (`-O0`), `less` (`-O1`), `default` (`-O2`), `aggressive` (`-O3`), `size` (`-Os`), `size-aggressive` (`-Oz`) |
| 🩺 **Sanitizers** | `--sanitize address,undefined,thread,memory,leak` mapped to `-fsanitize=...` (memory is clang-only) |
| 💾 **Incremental Build Cache** | sha256-keyed objects in `.polyglot-cache/`; rebuilds only what changed; `--no-cache` bypasses, `clean` clears |
| 🚀 **Parallel Compilation** | `-j N` compiles translation units in parallel |
| 🎯 **Cross-compilation** | Target `x86_64`, `aarch64`, `arm`, `riscv64`, `wasm32` via named targets |
| 🔍 **Syntax Checking** | Fast validation without producing binaries (`polyglot check`) |
| 📄 **IR Emission** | LLVM IR with clang; GIMPLE dumps with GCC (`polyglot emit-ir`) |
| ⚙️ **Assembly Output** | Generate assembly listings, AT&T or `--intel` syntax (`polyglot emit-asm`) |
| 📊 **Compilation Statistics** | Time/memory reports, binary size analysis, machine-readable `--json` (`polyglot stats`) |
| 🧰 **Source Tooling** | `fmt` (clang-format), `tidy` (clang-tidy), `analyze` (static analyzer), `preproc`, `deps`, `mca` (llvm-mca) |
| 🖥️ **REPL & JIT** | Interactive `clang-repl` session (`repl`) and `.ll`/`.bc` execution via `lli` (`jit`) |
| 👀 **Watch Mode** | `polyglot watch file.c` rebuilds on every file change |
| 🏗️ **Project Management** | `polyglot.toml` configuration with `init`, `build`, `run` workflow; scaffold C, C++, **or Rust** projects |
| 🔌 **Plugin Architecture** | Extensible JSON-RPC 2.0 protocol for language frontends |

---

## 🎬 See It In Action

### Build & Run in Seconds
```bash
$ polyglot run examples/01_hello.c
Hello, Polyglot!
Running: /bin/gcc -std=c17 -O2 -c examples/01_hello.c -o .polyglot-cache/504674...tmp.o
Running: /bin/gcc -std=c17 -O2 .polyglot-cache/504674....o -o /tmp/.../polyglot_run
```

### Inspect What The Compiler Does
```bash
$ polyglot emit-ir examples/01_hello.c -o hello.ll --compiler clang
Running: clang -std=c17 -O2 -fno-discard-value-names -S -emit-llvm examples/01_hello.c -o hello.ll

$ head -16 hello.ll
; ModuleID = 'examples/01_hello.c'
source_filename = "examples/01_hello.c"
target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-f80:128-n8:16:32:64-S128"
target triple = "x86_64-unknown-linux-gnu"

@str = private unnamed_addr constant [17 x i8] c"Hello, Polyglot!\00", align 1

; Function Attrs: nofree nounwind uwtable
define dso_local i32 @main() local_unnamed_addr #0 {
entry:
  %puts = tail call i32 @puts(ptr nonnull dereferenceable(1) @str)
  ret i32 0
}

; Function Attrs: nofree nounwind
declare noundef i32 @puts(ptr nocapture noundef readonly) local_unnamed_addr #1
```

With the GCC backend, `emit-ir` produces a GIMPLE dump instead:

```bash
$ polyglot emit-ir examples/01_hello.c
Running: /bin/gcc -std=c17 -O2 -fdump-tree-gimple -S .../examples/01_hello.c -o /tmp/.../out
note: GCC backend emits GIMPLE (not LLVM IR) -> 01_hello.gimple

$ head -8 01_hello.gimple
int main ()
{
  int D.2564;

  {
    printf ("Hello, Polyglot!\n");
    D.2564 = 0;
    return D.2564;
```

```bash
$ polyglot emit-asm examples/08_ir_asm/simple.c | head -14
	.file	"simple.c"
	.text
	.section .rodata
.LC0:
	.string	"Hello, %s!"
	.section .text.unlikely
	.section .text.startup
	.section .text.hot
	.p2align 4
	.globl	main
	.type	main, @function
main:
.LFB0:
	.cfi_startproc
```

### Compilation Statistics
```bash
$ polyglot stats examples/04_functions.c --opt-level aggressive
Running: /bin/gcc -std=c17 -O3 -ftime-report examples/04_functions.c -o /dev/null
============================================================
COMPILATION STATISTICS
============================================================
Source files: 1
  examples/04_functions.c: 663 bytes, 34 lines

Target: native
Optimization: aggressive
Backend: gcc (gcc (Ubuntu 13.3.0-6ubuntu2~24.04.1) 13.3.0)

------------------------------------------------------------
COMPILATION TIME REPORT (gcc format)
------------------------------------------------------------
  phase setup                                0.0000 s
  phase parsing                              0.0000 s
  phase opt and generate                     0.0300 s
  callgraph functions expansion              0.0200 s
  callgraph ipa passes                       0.0100 s
  ...

Binary size: 2,590 bytes
Total wall time: 78 ms
============================================================
```

### Project Workflow
```bash
$ polyglot init my-app --language cpp
Project created at: my-app
Run 'polyglot build' to build.

$ cd my-app && polyglot build
Built src/main.cpp -> my-app

$ ./my-app
Hello from Polyglot!
```

### New in 0.2.0

```bash
$ polyglot doctor
==============================================================
POLYGLOT ENVIRONMENT REPORT (doctor)
==============================================================
  driver          polyglot 0.2.0 (python 3.12.6)
  ok   C compiler     /bin/gcc [gcc (Ubuntu 13.3.0-6ubuntu2~24.04.1) 13.3.0]
  ok   C++ compiler   /bin/g++
  backend         gcc
  hint            clang is not on PATH but exists at:
                    ~/llvm-install/clang+llvm-17.0.6-x86_64-linux-gnu-ubuntu-22.04/bin/clang
                  consider: export PATH=~/llvm-install/clang+llvm-17.0.6-...:/bin:$PATH
--------------------------------------------------------------
Auxiliary tools:
  ok   clang-format   ~/llvm-install/clang+llvm-17.0.6-.../bin/clang-format
  ok   clang-tidy     ~/llvm-install/clang+llvm-17.0.6-.../bin/clang-tidy
  ok   lli            ~/llvm-install/clang+llvm-17.0.6-.../bin/lli
  ok   llvm-mca       ~/llvm-install/clang+llvm-17.0.6-.../bin/llvm-mca
  ok   rustc          ~/.cargo/bin/rustc
  ...
--------------------------------------------------------------
  build cache     .polyglot-cache (10 object(s), 18.5 KiB)
```

```bash
$ polyglot fmt --check examples/01_hello.c
all files already formatted
```

```bash
$ polyglot deps examples/06_multi_file/main.c
main.o: examples/06_multi_file/main.c examples/06_multi_file/math.h
```

```bash
$ polyglot stats examples/01_hello.c --json | head -14
{
  "source": [
    "examples/01_hello.c"
  ],
  "size_bytes": 161,
  "lines": 10,
  "target": "native",
  "opt_level": "default",
  "backend": "gcc",
  "compiler_version": "gcc (Ubuntu 13.3.0-6ubuntu2~24.04.1) 13.3.0",
  "total_ms": 55,
  "binary_size_bytes": 2034,
  "timings": [
    {
```

---

## 🏗️ Architecture

Polyglot is built as a **dual-layer architecture**:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        POLYGLOT DRIVER (Python)                               │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐        │
│  │   CLI       │  │  Config     │  │  Build      │  │  Diagnostics│        │
│  │   Parser    │  │  Manager    │  │  Orchestrator│ │  Reporter   │        │
│  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘        │
└─────────┼────────────────┼────────────────┼────────────────┼───────────────┘
          │                │                │                │
          ▼                ▼                ▼                ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                 CLANG / GCC BACKEND (System Toolchain)                        │
│  ┌─────────────┐  ┌─────────────────────────────────────────────────────┐   │
│  │   Clang /   │  │  LLVM / GCC Optimization Pipeline                   │   │
│  │   GCC       │──▶  Canonicalize → CSE → LICM → Inline → SCCP → DCE    │   │
│  │  (C/C++)    │  │  Loop Opts → Vectorization → CodeGen                │   │
│  └─────────────┘  └─────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────────┘
```

The Python driver is a single-file executable (`polyglot`, stdlib + `toml` only) that orchestrates the system toolchain: compile, cache, run, inspect, format, lint, analyze.

### 🦀 Rust Library Architecture (Advanced)

For power users, Polyglot provides an **MLIR-based compilation pipeline** implemented as Rust libraries:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        RUST LIBRARY CRATES                                    │
│                                                                              │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐    │
│  │ polyglot-    │  │ polyglot-    │  │ polyglot-    │  │ polyglot-    │    │
│  │ protocol     │  │ mlir         │  │ codegen      │  │ driver       │    │
│  │ (JSON-RPC)   │  │ (MLIR C API) │  │ (Cranelift)  │  │ (Orchestr.)  │    │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘    │
│         │                 │                 │                 │             │
│  ┌──────┴───────┐  ┌──────┴───────┐  ┌──────┴───────┐  ┌──────┴───────┐    │
│  │ polyglot-    │  │ polyglot-    │  │ polyglot-    │  │ polyglot-    │    │
│  │ cache        │  │ registry     │  │ syntax       │  │ sema         │    │
│  │ (Incremental)│  │ (Plugin Reg) │  │ (Parser)     │  │ (Semantic)   │    │
│  └──────────────┘  └──────────────┘  └──────────────┘  └──────────────┘    │
│                 + polyglot-core (types) + polyglot-ir (lowering)             │
└─────────────────────────────────────────────────────────────────────────────┘
```

The `polyglot-driver` binary now runs a **real in-process pipeline** — `parse_source → sema::check → lower_program → optimize_module → print_module` — and can print textual Polyglot IR:

```bash
$ cargo run -p polyglot-driver -- emit-ir examples/01_hello.c
; Polyglot IR

; ---- globals ----
const @.LC0: char size=18 init="Hello, Polyglot!\n" (18)

func @printf(char* %r0) -> int;
```

**MLIR Pipeline Flow:**
```
Source Code → Clang Plugin → MLIR → Optimization Passes → LLVM Dialect → Cranelift IR → Native Code
                 │              │         │                    │              │            │
                 ▼              ▼         ▼                    ▼              ▼            ▼
            JSON-RPC      mlir-sys   Canonicalize,         Progressive    CLIF →       Object
            Protocol      Bindings   CSE, LICM,            Lowering       x86_64/      Files
                                                 Inline, SCCP                           ARM64/
                                                                                      RISC-V/
                                                                                      WASM
```

> **Status (0.2.0)**: all 11 workspace crates compile, including the MLIR crates (point `MLIR_SYS_220_PREFIX` at an LLVM install — already baked into `.cargo/config.toml` here). The `polyglot-codegen` `emit_object`/`mlir_to_clif` steps remain stubs. The `polyglot-clang` plugin defaults to `-S -emit-llvm`; MLIR emission is gated behind `POLYGLOT_CLANG_EMIT_MLIR=1`.

---

## 🚀 Quick Start

### Prerequisites

- **Python 3.8+**
- **Clang/LLVM 17+** or **GCC** (either backend works; GCC is the fallback on most Linux systems)
- **toml** Python package — `install.sh` offers to install it automatically when missing

### Install Script (recommended)

`install.sh` verifies your environment (Python ≥ 3.8, the `toml` package, a
clang/gcc backend), copies the single-file `polyglot` driver into your prefix,
and finishes with a `polyglot doctor` report. It never needs superuser
privileges:

```bash
git clone https://github.com/RXVEN-1907/polyglot-compiler
cd polyglot-compiler
./install.sh --yes
```

Sample output on a GCC-only Linux host:

```text
==> python3 OK: Python 3.12.6
==> python 'toml' package OK
==> compiler found: GCC backend (gcc (Ubuntu 13.3.0-6ubuntu2~24.04.1) 13.3.0)
==> installed /home/you/.local/bin/polyglot
==> installed: polyglot 0.2.0
==> running 'polyglot doctor' (non-strict) ...
==============================================================
POLYGLOT ENVIRONMENT REPORT (doctor)
...
```

Useful options:

```bash
./install.sh --prefix ~/.local/bin   # install location (default: ~/.local/bin;
                                     # a prefix not ending in /bin gets /bin appended)
./install.sh --yes                   # never prompt (non-interactive / CI)
./install.sh --uninstall             # remove the installed driver
```

Verify the installation:

```bash
polyglot version
# polyglot 0.2.0
# backend: /bin/gcc (gcc, gcc (Ubuntu 13.3.0-6ubuntu2~24.04.1) 13.3.0)
# run polyglot doctor for details

polyglot run examples/01_hello.c
# Hello, Polyglot!
```

If the prefix is not on your `PATH`, the installer says so and prints the exact
line to add, e.g.:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

### One-Line Install

No clone needed — the piped script downloads the `polyglot` driver itself:

```bash
curl -fsSL https://raw.githubusercontent.com/RXVEN-1907/polyglot-compiler/master/install.sh | bash
```

> **Security note**: `curl | bash` executes a remote script with your user's
> privileges without review. If you prefer to inspect it first:
>
> ```bash
> curl -fsSL https://raw.githubusercontent.com/RXVEN-1907/polyglot-compiler/master/install.sh -o install.sh
> less install.sh      # review, then run
> bash install.sh
> ```

### Manual Installation (fallback)

```bash
# Clone the repository
git clone https://github.com/RXVEN-1907/polyglot-compiler
cd polyglot-compiler

# Install Python dependencies
pip install toml

# Make polyglot executable
chmod +x polyglot

# Add to PATH (optional)
export PATH="$PWD:$PATH"

# Verify installation
polyglot version
# Output: polyglot 0.2.0
#         backend: /bin/gcc (gcc, gcc (Ubuntu 13.3.0-...) 13.3.0)
#         run polyglot doctor for details
```

### Building from Source (Rust Components)

The Rust library crates implement an MLIR-based compilation pipeline but require static system libraries and an LLVM installation:

```bash
# Install system dependencies (Ubuntu/Debian)
sudo apt-get update && sudo apt-get install -y \
    libzstd-dev libtinfo-dev libxml2-dev \
    clang-17 llvm-17 lld-17

# Point the MLIR bindings at your LLVM install (see .cargo/config.toml)
export MLIR_SYS_220_PREFIX=/path/to/llvm-install

# Build the whole workspace (all 11 crates)
cargo build --workspace

# Run the Rust-side tests
cargo test --workspace
```

> **Note**: The Python driver works independently and provides full C/C++ compilation functionality using the system toolchain directly. The Rust components are for advanced MLIR/Cranelift integration.

---

## 📖 Usage Guide

### Basic Commands

```bash
# Build a C program
polyglot build hello.c -o hello

# Build and run in one command
polyglot run hello.c

# Build a C++ program
polyglot build hello.cpp -o hello

# Check syntax without building
polyglot check hello.c

# Emit LLVM IR (clang) or GIMPLE (gcc)
polyglot emit-ir hello.c -o hello.ll

# Emit assembly (Intel syntax)
polyglot emit-asm hello.c -o hello.s --intel

# Show compilation statistics (human or JSON)
polyglot stats hello.c
polyglot stats hello.c --json

# Run the preprocessor / emit Make-style dependencies
polyglot preproc hello.c --no-lines
polyglot deps hello.c --target-name hello.o -o hello.d

# Static analysis, formatting, linting
polyglot analyze hello.c
polyglot fmt src/*.c --style llvm --check
polyglot tidy hello.c "--checks=-*,modernize-*" --fix

# Machine-code analysis of a .c or .s file
polyglot mca hot_loop.c --mcpu skylake

# Interactive REPL / run LLVM IR or bitcode
polyglot repl
polyglot jit hello.ll

# Rebuild on every file change
polyglot watch hello.c

# Initialize a new project (C, C++, or Rust)
polyglot init my-app --language c
polyglot init rs-app --language rust

# Multi-file compilation with parallelism
polyglot build main.c utils.c -o myapp -j 4

# Warnings, standards, debug info, sanitizers
polyglot build hello.c --std c17 --warn pedantic --werror --debug -o hello
polyglot build hello.c --sanitize address,undefined -o hello-asan

# Different optimization levels
polyglot --opt-level aggressive build hello.c -o hello-O3
polyglot --opt-level size build hello.c -o hello-Os

# Cross-compilation
polyglot --target aarch64-unknown-linux-gnu build hello.c -o hello-arm64

# Clean build artifacts and the incremental cache
polyglot clean
```

### Command Reference

| Command | Description | Key Options |
|---------|-------------|-------------|
| `build [files...] -o output` | Build source file(s) or project | `-o`, `--std`, `--warn {none,default,all,extra,pedantic}`, `--werror`, `--debug`, `--sanitize`, `--shared`, `--static-lib`, `-j N`, `--no-cache` |
| `run file` | Build and run in one step (args pass through) | Same compile flags as `build` |
| `check files...` | Syntax check only (no binary) | `--std`, `--warn`, `--werror`, `-I/-D/--cc-arg` |
| `emit-ir files...` | Emit LLVM IR (clang) or GIMPLE (gcc) | `-o`, compile flags |
| `emit-asm files...` | Emit assembly | `-o`, `--intel`, compile flags |
| `preproc file` | Run the preprocessor (`-E`) | `-o`, `--no-lines` (`-P`) |
| `deps file` | Emit Make-style dependencies (`-MM`) | `-o`, `--target-name NAME` (`-MT`) |
| `mca file` | Machine-code analysis with `llvm-mca` | `--iterations N`, `--mcpu CPU`, `.c` or `.s` input |
| `analyze files...` | Clang static analyzer (`--analyze`) | - |
| `fmt files...` | Format code with `clang-format` | `--style {llvm,google,chromium,mozilla,webkit,microsoft,file}`, `--check`, `-i`, `--dump-config` |
| `tidy files...` | Lint with `clang-tidy` | `--checks`, `--fix` |
| `repl` | Interactive `clang-repl` session | - |
| `jit file` | Run a `.ll`/`.bc` file with `lli` | - |
| `watch file` | Poll-rebuild loop | `--timeout SECONDS` (0 = forever) |
| `doctor` | Report on the toolchain environment | `--strict` (exit 1 when anything is missing) |
| `targets` | List known target triples and availability | - |
| `stats files...` | Compilation statistics | `--json`, `--target`, `--opt-level` |
| `init [name]` | Initialize a new project | `--language {c,cpp,rust}` |
| `clean [dir]` | Remove build artifacts and `.polyglot-cache/` | - |
| `version` | Show version and backend | - |

### Global Options

| Option | Description | Values |
|--------|-------------|--------|
| `-v, --verbose` | Verbose output (full command lines, cache HIT/MISS) | - |
| `--quiet` | Suppress `Running:` lines | - |
| `--target TRIPLE` | Target triple | `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, etc. |
| `--opt-level LEVEL` | Optimization level | `none`, `less`, `default`, `aggressive`, `size`, `size-aggressive` |
| `-o, --output FILE` | Output file | - |
| `--compiler BACKEND` | Force a backend (default: auto-discover) | `auto`, `clang`, `gcc` |
| `--plugin PLUGIN` | JSON-RPC plugin binary (name or path; used by `build`) | `target/release/polyglot-clang` |
| `--plugin-dir DIR` | Plugin search directory | `~/.polyglot/plugins` |

### Common Compile Flags

Shared by `build`, `run`, `check`, `emit-ir`, `emit-asm`, `preproc`, `deps`, `mca`, `watch` and `stats`:

| Option | Description |
|--------|-------------|
| `--std STD` | Language standard (`c89..c2x` / `gnu*`, `c++98..c++23`) |
| `--warn LEVEL` | Warning level: `none`, `default`, `all`, `extra`, `pedantic` |
| `--werror` | Warnings as errors |
| `--debug` | Emit debug info (`-g`) |
| `--sanitize LIST` | `address`, `undefined`, `thread`, `memory`, `leak` (comma-separated; `memory` is clang-only) |
| `-I DIR` | Include search path (repeatable) |
| `-D NAME[=VAL]` | Preprocessor define (repeatable) |
| `--cc-arg ARG` | Raw flag passed straight to the compiler (repeatable) |
| `-L DIR` / `-l NAME` | Library search path / link library (build & run; repeatable) |

### Exit Codes

| Code | Meaning |
|------|---------|
| `0` | Success |
| `1` | Compile or run failure |
| `2` | Usage error |
| `3` | Missing tool |

`--quiet`, `--verbose` and `NO_COLOR` are honoured globally.

---

## ⚙️ Configuration

### `polyglot.toml`

```toml
[project]
name = "my-app"
version = "0.2.0"

# Optional: pin the compilers for this project
# (POLYGLOT_CC / POLYGLOT_CXX keep absolute priority over these)
[compiler]
cc = "clang"
cxx = "clang++"

[target]
triple = "x86_64-unknown-linux-gnu"
opt_level = 2

[plugins]
clang = "0.1"

[[bin]]
name = "my-app"
source = "src/main.c"
```

Rust projects scaffolded with `polyglot init --language rust` use a `[language]` section and build with `rustc`:

```toml
[project]
name = "rs-app"
version = "0.1.0"

[language]
rust = true

[target]
triple = "x86_64-unknown-linux-gnu"
opt_level = 2

[[bin]]
name = "rs-app"
source = "src/main.rs"
```

### Configuration Options

| Section | Key | Description | Default |
|---------|-----|-------------|---------|
| `project` | `name` | Project name | Required |
| `project` | `version` | Project version | `"0.1.0"` |
| `compiler` | `cc` | C compiler for this project | `POLYGLOT_CC` / auto-discovery |
| `compiler` | `cxx` | C++ compiler for this project | `POLYGLOT_CXX` / auto-discovery |
| `language` | `rust` | Treat the project as Rust (build with `rustc`) | `false` |
| `target` | `triple` | Target triple | Native |
| `target` | `opt_level` | Optimization level (0-3) | `2` |
| `plugins` | `clang` | Clang plugin version | `"0.1"` |
| `bin` | `name` | Binary name | Project name |
| `bin` | `source` | Main source file | Required |

---

## 🧭 Toolchain Discovery

Polyglot needs a C/C++ compiler and (optionally) LLVM auxiliary tools. It finds them in this order:

1. **`POLYGLOT_CC` / `POLYGLOT_CXX`** environment variables (absolute paths win over everything).
2. **`--compiler clang|gcc`** — forced selection, searched on `PATH` and the scan dirs below.
3. **Auto discovery** — `PATH` candidates in preference order (`clang`, `clang-17`…, `gcc`, `cc`), then well-known LLVM install directories: `~/llvm-install/*/bin`, `/usr/lib/llvm-*/bin`, `/usr/local/bin`, `/opt/*/bin`.

If clang exists only off `PATH` (e.g. `~/llvm-install/clang+llvm-17.0.6-x86_64-linux-gnu-ubuntu-22.04/bin`), the driver still finds it and `polyglot doctor` tells you so.

### GCC vs Clang Behavior

| Behavior | Clang | GCC |
|----------|-------|-----|
| `emit-ir` output | LLVM IR (`.ll`) | GIMPLE dump (a note points at the file) |
| `--sanitize memory` | ✅ | ❌ rejected (MemorySanitizer is clang-only) |
| Cross targets | Native `-target` support | Needs the per-arch cross compiler (`aarch64-linux-gnu-gcc`, …) |
| Auxiliary tools | clang-format/tidy/repl, llvm-mca, lli work regardless of the C backend | same |

### First Troubleshooting Step

```bash
polyglot doctor          # what did the driver find, and what is missing?
polyglot doctor --strict # exit 1 when anything is missing (CI-friendly)
```

---

## 🧪 Examples

The `examples/` directory contains working programs demonstrating various features:

```
examples/
├── 01_hello.c              # Basic hello world
├── 02_arithmetic.c         # Arithmetic operations
├── 03_control_flow.c       # If/else, loops, switch
├── 04_functions.c          # Functions, recursion
├── 05_structs.c            # Structs, pointers
├── 06_multi_file/          # Multi-file project
│   ├── main.c
│   ├── math.c
│   └── math.h
├── 07_diagnostics/         # Error examples
│   ├── bad_syntax.c
│   └── undefined_func.c
└── 08_ir_asm/
    └── simple.c            # IR/Assembly demo
```

### Running Examples

```bash
# Build and run hello world
polyglot build examples/01_hello.c -o hello
./hello

# With optimization
polyglot --opt-level aggressive build examples/04_functions.c -o func-O3

# Show IR (LLVM IR with clang, GIMPLE with gcc)
polyglot emit-ir examples/08_ir_asm/simple.c -o simple.ll

# Show assembly
polyglot emit-asm examples/08_ir_asm/simple.c -o simple.s

# Check syntax (will show error)
polyglot check examples/07_diagnostics/bad_syntax.c

# Multi-file build (4-way parallel)
polyglot build examples/06_multi_file/main.c examples/06_multi_file/math.c -o mathapp -j 4

# Address-sanitized build
polyglot build examples/05_structs.c --sanitize address -o structs-asan
```

---

## 📈 Benchmarks

Measured with `polyglot stats --json` on the reference machine (GCC 13.3.0 backend, Ubuntu 24.04, `-O2` unless noted). Run `polyglot stats <file> --json` to measure on your own hardware.

### Compilation Time (default `-O2`)

| Program | Time | Binary Size |
|---------|------|-------------|
| Hello World | ~52 ms | ~2.0 KB |
| Arithmetic | ~61 ms | ~2.1 KB |
| Control Flow | ~68 ms | ~2.4 KB |
| Functions | ~76 ms | ~2.4 KB |
| Structs | ~70 ms | ~2.6 KB |

### Optimization Levels (hello.c)

| Level | Time | Binary Size |
|-------|------|-------------|
| `-O0` (none) | ~50 ms | ~1.9 KB |
| `-O1` (less) | ~56 ms | ~2.0 KB |
| `-O2` (default) | ~53 ms | ~2.0 KB |
| `-O3` (aggressive) | ~60 ms | ~2.0 KB |
| `-Os` (size) | ~46 ms | ~2.0 KB |
| `-Oz` (size-aggressive) | ~54 ms | ~2.0 KB |

> **Note**: Times include one full compile; the incremental cache makes *re*-builds of unchanged sources near-instant. Binary sizes for tiny programs are nearly flat across levels; larger programs show real differences at `-Os`/`-Oz`.

---

## 🎯 Supported Languages & Targets

### Languages

| Language | Status | Notes |
|----------|--------|-------|
| C | ✅ Full Support | Clang or GCC frontend |
| C++ | ✅ Full Support | Clang++ or G++ frontend |
| Rust | ✅ Scaffold & Build | `polyglot init --language rust`, builds via `rustc` |

### Targets

| Target | Status | Notes |
|--------|--------|-------|
| `x86_64-unknown-linux-gnu` | ✅ Native | Default on x86_64 Linux |
| `aarch64-unknown-linux-gnu` | ✅ Cross-compile | Clang: native; GCC: needs `gcc-aarch64-linux-gnu` |
| `arm-unknown-linux-gnueabihf` | ✅ Cross-compile | Clang: native; GCC: needs `gcc-arm-linux-gnueabihf` |
| `riscv64-unknown-linux-gnu` | ✅ Cross-compile | Clang: native; GCC: needs `gcc-riscv64-linux-gnu` |
| `wasm32-wasip1` | ⚠️ Tooling-dependent | Requires `wasm-ld`/`lld` on PATH |

Run `polyglot targets` to see availability on your machine.

---

## 🔧 Optimization Pipeline

The Clang/LLVM optimization pipeline includes:

1. **Canonicalization** - Simplify IR patterns
2. **Common Subexpression Elimination (CSE)** - Remove redundant computations
3. **Loop Invariant Code Motion (LICM)** - Hoist loop-invariant code
4. **Function Inlining** - Inline small functions
5. **Sparse Conditional Constant Propagation (SCCP)** - Propagate constants
6. **Dead Code Elimination (DCE)** - Remove unreachable code
7. **Loop Optimizations** - Unrolling, vectorization, etc.

---

## 🦀 Rust Crate Details

### Library Crates (All 11 Compile Successfully)

| Crate | Purpose | Lines of Code | Status |
|-------|---------|---------------|--------|
| `polyglot-core` | Core types, diagnostics, spans | ~510 | ✅ |
| `polyglot-syntax` | Lexer, parser, AST for C-like languages | ~2,290 | ✅ (4 parser tests) |
| `polyglot-sema` | Semantic analysis, type checking | ~1,100 | ✅ |
| `polyglot-ir` | MLIR lowering, optimization, printing | ~2,090 | ✅ |
| `polyglot-driver` | Build orchestration; **real pipeline** parse → sema → lower → optimize → print | ~830 | ✅ |
| `polyglot-protocol` | JSON-RPC 2.0 driver-plugin communication | ~870 | ✅ |
| `polyglot-mlir` | MLIR C API bindings, optimization passes | ~320 | ✅ (needs `MLIR_SYS_220_PREFIX`) |
| `polyglot-codegen` | Cranelift codegen (MLIR → CLIF → native) | ~60 | ✅ compiles; `emit_object`/`mlir_to_clif` stubs |
| `polyglot-cache` | Incremental compilation cache | ~110 | ✅ |
| `polyglot-registry` | Plugin registry client | ~110 | ✅ (client; registry publish TODO) |

### Plugin Crate

| Plugin | Language | Status |
|--------|----------|--------|
| `polyglot-clang` | C/C++ | ✅ Builds; defaults to `-S -emit-llvm`, MLIR emission gated behind `POLYGLOT_CLANG_EMIT_MLIR=1` |

---

## 🛠️ Development

### Running Tests

```bash
# Python integration tests (33 tests, all passing)
python3 test_comprehensive.py

# Rust library tests
cargo test --workspace
```

### Test Results

```
✓ test_basic_compilation passed
✓ test_cpp_compilation passed
✓ test_optimization_levels passed
✓ test_multi_file_build passed
✓ test_check_command passed
✓ test_emit_ir passed
✓ test_emit_asm passed
✓ test_sanitize_build passed
✓ test_json_stats passed
✓ test_rust_build_run passed
✓ test_parallel_build passed
✓ test_cache_rebuild passed
...
✓ test_watch_smoke passed
✓ test_cc_args passed
✓ test_tidy_smoke passed
✓ test_quiet_flag passed

Results: 33 passed, 0 failed
```

### Project Structure

```
polyglot-compiler/
├── polyglot                 # Python driver (single-file executable)
├── Cargo.toml               # Rust workspace (11 members)
├── .cargo/config.toml       # MLIR_SYS_220_PREFIX etc.
├── README.md                # This file
├── LICENSE                  # MIT License
├── CONTRIBUTING.md          # Contribution guidelines
├── CODE_OF_CONDUCT.md       # Code of conduct
├── SECURITY.md              # Security policy
├── CHANGELOG.md             # Version history
├── test_comprehensive.py    # Integration tests (33)
├── benchmark.py             # Performance benchmarks
├── crates/                  # Rust library crates (10)
│   ├── polyglot-core/
│   ├── polyglot-syntax/
│   ├── polyglot-sema/
│   ├── polyglot-ir/
│   ├── polyglot-protocol/
│   ├── polyglot-mlir/
│   ├── polyglot-driver/
│   ├── polyglot-codegen/
│   ├── polyglot-cache/
│   └── polyglot-registry/
├── plugins/
│   └── polyglot-clang/      # Clang plugin
├── examples/                # Example programs
├── tests/
│   ├── fixtures/            # Test source files
│   └── integration/         # Integration tests
├── docs/                    # Documentation (mkdocs site + ARCHITECTURE/API reference)
├── mkdocs.yml               # Docs navigation
└── .github/
    ├── workflows/           # CI/CD pipelines
    ├── ISSUE_TEMPLATE/      # Issue templates
    └── PULL_REQUEST_TEMPLATE.md
```

---

## 📋 Roadmap

### Phase 1: Environment & Integration
- [x] Install static libraries (`libzstd-dev`, `libtinfo-dev`, `libxml2-dev`)
- [x] Build Rust binaries — `cargo build --workspace`, MLIR crates included via `MLIR_SYS_220_PREFIX`
- [x] Real in-process pipeline in `polyglot-driver` (parse → sema → lower → optimize → print)
- [ ] Complete MLIR → CLIF lowering in `polyglot-codegen` (stubs remain)

### Phase 2: Core Features
- [x] Integrate `polyglot-clang` plugin via JSON-RPC (`polyglot --plugin ... build` drives it over stdio; see docs/user-guide/commands.md "Plugin architecture")
- [x] Incremental compilation (`.polyglot-cache/` object cache, `--no-cache` bypass)
- [~] Plugin registry — `polyglot-registry` client exists; registry publish service TODO

### Phase 3: Language Expansion
- [x] Basic Rust support (`polyglot init --language rust`, builds via `rustc`)
- [ ] Full Rust frontend plugin (`polyglot-rustc`)
- [ ] Add Swift frontend plugin
- [ ] Add Zig frontend plugin

### Phase 4: Advanced Features
- [x] Parallel compilation (`build -j N`)
- [x] JIT/REPL (`polyglot repl` via clang-repl, `polyglot jit` via lli)
- [ ] Language server protocol (LSP) integration
- [ ] WebAssembly target improvements
- [ ] Profile-guided optimization (PGO)

---

## 🤝 Contributing

We welcome contributions! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for details.

### Quick Contribution Guide

1. **Fork** the repository
2. **Create** a feature branch: `git checkout -b feature/amazing-feature`
3. **Make** your changes
4. **Run tests**: `python3 test_comprehensive.py && cargo test --workspace`
5. **Run benchmarks**: `python3 benchmark.py`
6. **Commit** your changes: `git commit -m 'Add amazing feature'`
7. **Push** to branch: `git push origin feature/amazing-feature`
8. **Open** a Pull Request

### Good First Issues

Look for issues labeled [`good first issue`](https://github.com/RXVEN-1907/polyglot-compiler/labels/good%20first%20issue) or [`help wanted`](https://github.com/RXVEN-1907/polyglot-compiler/labels/help%20wanted).

### Development Setup

```bash
# Clone your fork
git clone https://github.com/YOUR_USERNAME/polyglot-compiler
cd polyglot-compiler

# Install dependencies
pip install toml

# Run tests to verify setup
python3 test_comprehensive.py
```

---

## 📄 License

This project is licensed under the **MIT License** - see the [LICENSE](LICENSE) file for details.

```
MIT License

Copyright (c) 2024 Polyglot Team

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

---

## 🙏 Acknowledgments

- **LLVM/MLIR Team** - For the incredible compiler infrastructure
- **Cranelift Team** - For the fast, safe codegen backend
- **Clang Team** - For the production-grade C/C++ frontend
- **Rust Community** - For the excellent ecosystem and tooling

---

## 📞 Support & Community

- **Issues**: [GitHub Issues](https://github.com/RXVEN-1907/polyglot-compiler/issues)
- **Discussions**: [GitHub Discussions](https://github.com/RXVEN-1907/polyglot-compiler/discussions)
- **Security**: See [SECURITY.md](SECURITY.md) for reporting vulnerabilities
- **Documentation**: [docs/](docs/) for architecture and API reference

---

<div align="center">

**Made with ❤️ by the Polyglot Team**

[⭐ Star this repo](https://github.com/RXVEN-1907/polyglot-compiler) • [🐛 Report Bug](https://github.com/RXVEN-1907/polyglot-compiler/issues/new?template=bug_report.md) • [✨ Request Feature](https://github.com/RXVEN-1907/polyglot-compiler/issues/new?template=feature_request.md) • [💬 Discuss](https://github.com/RXVEN-1907/polyglot-compiler/discussions)

</div>
