# Polyglot Compiler Architecture

!!! note
    This is the architecture description for the v0.3.0 rebuild. The maintained documentation tree is under `docs/` (built with mkdocs) — start at [Architecture Overview](architecture/overview.md) for the driver side and [Python Driver](architecture/python-driver.md) for internals.

## Overview

Polyglot is designed as a **modular, extensible compiler infrastructure** with a clear separation between the user-facing Python driver (the product) and the self-contained Rust compiler pipeline (the lab).

## High-Level Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              USER LAYER                                      │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐    │
│  │   CLI User   │  │  IDE/Editor  │  │  Build Sys   │  │  Scripts     │    │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘    │
└─────────┼─────────────────┼─────────────────┼─────────────────┼────────────┘
          │                 │                 │                 │
          ▼                 ▼                 ▼                 ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                   POLYGLOT DRIVER (Python, polyglot_lib/)                    │
│  ┌──────────────────────────────────────────────────────────────────────┐   │
│  │                        Command Router                                 │   │
│  │  build │ run │ check │ emit-ir │ emit-asm │ inspect │ query │ ...     │   │
│  │  (49 commands: cached parallel builds, sanitizers, IR/asm emission,   │   │
│  │   binary inspection, fmt/tidy/analyze/fuzz/pgo/mca, doctor, plugins,  │   │
│  │   preview, transmute)                                                 │   │
│  └──────────────────────────────────────────────────────────────────────┘   │
│                                    │                                        │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐   │
│  │ Config       │  │ Build        │  │ Diagnostic   │  │ Plugin       │   │
│  │ Manager      │  │ Orchestrator │  │ Reporter     │  │ Manager      │   │
│  └──────────────┘  └──────────────┘  └──────────────┘  └──────────────┘   │
└──────────────────────────────────┼──────────────────────────────────────────┘
                                   │
           ┌───────────────────────┼───────────────────────┐
           ▼                       ▼                       ▼
┌─────────────────────┐ ┌─────────────────────┐ ┌─────────────────────┐
│   GCC / g++         │ │  clang / clang++    │ │  Rust driver        │
│  -fsyntax-only      │ │  -fsyntax-only      │ │  (in-process        │
│  -S -emit-llvm      │ │  -S -emit-llvm      │ │  pipeline, see      │
│  -O0 through -Oz    │ │  -O0 through -Oz    │ │  below)             │
│  --target=...       │ │  --target=...       │ │                     │
└─────────────────────┘ └─────────────────────┘ └─────────────────────┘
```

## Component Details

### 1. Python Driver (`polyglot`, `polyglot_lib/`)

**Files:** `polyglot` (thin launcher) + the `polyglot_lib/` package (toolchain discovery, backends, cache, config, output, plugin client, and one module per command under `polyglot_lib/commands/`).

**Responsibilities:**
- Command-line argument parsing (49 subcommands)
- Project configuration management (`polyglot.toml`)
- Cached parallel builds (sha256-keyed object cache in `.polyglot-cache/`)
- Sanitizer passthrough, cross-target listing, binary inspection
  (`nm`/`objdump`/`readelf`/`size`/`strings`/`strip`/`addr2line`/`c++filt`,
  correlated `inspect`/`query` views)
- IR/asm emission, fmt/tidy/analyze/fuzz/pgo/mca, doctor, plugin management,
  preview (local Compiler Explorer), transmute (narrated compilation)
- Diagnostic formatting and reporting

Dependencies: Python 3.8+ standard library plus the `toml` package, and any
one C compiler (gcc or clang).

### 2. Rust Workspace (7 crates + 1 plugin)

The Rust side is a self-contained compiler pipeline for C sources. The
default build needs **no system LLVM** (the experimental MLIR/Cranelift
paths were removed in 0.3.0; the active path is LLVM IR emission).

| Crate | Purpose | Key types |
|-------|---------|-----------|
| `polyglot-core` | Shared foundation | `Span`, `Diagnostic`, `DiagnosticEngine`, `SourceFile`, `Type`, `IntKind`, `StructDef` |
| `polyglot-syntax` | Lexer and parser | `Lexer`, `Parser`, AST nodes |
| `polyglot-sema` | Semantic analysis | `CheckedProgram`, name resolution, type checking |
| `polyglot-ir` | Lowering, optimization, printing | `Module`, `Instr`, `lower_program`, `optimize_module`, `print_module`, `OptLevel` |
| `polyglot-protocol` | JSON-RPC 2.0 plugin types | `Request`/`Response`/`Notification`, `Transport`, `Handler` |
| `polyglot-codegen` | `polyglot-ir` → LLVM IR text → objects | `emit_ll`, `emit_ll_named`, `emit_object`, `find_llc` |
| `polyglot-driver` | clap CLI + in-process pipeline | `Cli`, `Commands`, `parse_and_check`, `run_simple_driver` |
| `plugins/polyglot-clang` | Reference JSON-RPC stdio plugin server | `initialize`/`compile`/`shutdown` handling |

**Pipeline (in-process, `crates/polyglot-driver`):**

```
parse_source (syntax) → check (sema) → lower_program (IR)
  → optimize_module → { print_module (textual Polyglot IR),
                        emit_ll → LLVM IR text }
```

`--backend native` compiles the emitted LLVM IR to a real object file with
`llc -filetype=obj` (with a graceful fallback to writing the `.ll`) and links
with the system `cc`.

**Subcommands:** `build`, `run`, `check`, `emit-ir` and the internal
`transmute-internal` (narrated source → AST → sema → IR → object walkthrough).

### 3. Plugin Architecture

Plugins are standalone binaries communicating via **JSON-RPC 2.0 over
stdio** (newline-delimited). The reference plugin is `polyglot-clang`
(Rust), which compiles C/C++ by shelling out to `clang -S -emit-llvm` and
returns the LLVM IR text to the driver.

**Protocol Flow:**
```
Driver                          Plugin
  │                                │
  ├── initialize(InitializeParams)▶│
  │◀── initialize result ──────────┤
  │                                │
  ├── compile(CompileParams) ─────▶│
  │◀── compile result ─────────────┤
  │                                │
  ├── shutdown ───────────────────▶│
  │◀── shutdown result ────────────┤
```

**Plugin Methods:**
- `initialize` - Handshake, capability exchange
- `compile` - Compile source; returns `{mlir_module, diagnostics}` — for
  `polyglot-clang` the `mlir_module` field carries LLVM IR text
- `shutdown` - Cleanup

### 4. Build Pipeline (Python driver)

```
Source Files (.c/.cpp)
       │
       ▼
┌──────────────────┐
│  polyglot.toml   │  (Project config)
└────────┬─────────┘
         │
         ▼
┌──────────────────┐
│  Driver Parses   │
│  Args & Config   │
└────────┬─────────┘
         │
         ▼
┌──────────────────┐
│  Cache Lookup    │  .polyglot-cache/<sha256>.o
└────────┬─────────┘
         │
         ▼
┌──────────────────┐
│  Compile (-j) &  │
│  Link            │
└────────┬─────────┘
         │
         ▼
┌──────────────────┐
│  Binary Output   │
└──────────────────┘
```

## Data Flow

### Configuration Flow
```
polyglot.toml → toml.load → [[bin]] / [language] / [compiler] keys → build inputs → compiler flags
```

### Compilation Flow
```
Source Files → Driver (cache lookup) → gcc/clang → objects (-j parallel) → Link → Binary
```

### Plugin Flow
```
Source Files → Driver → JSON-RPC → polyglot-clang → LLVM IR text → Driver links → Binary
```

## Extension Points

### 1. New Language Frontend
Implement a plugin with:
- `initialize` - Declare language support
- `compile` - Parse source → produce IR text

### 2. New Optimization Pass
Add it to the pass pipeline in `crates/polyglot-ir/src/opt.rs`.

### 3. New Backend Emitter
Extend `crates/polyglot-codegen` (`emit_ll` turns `polyglot-ir` modules into
LLVM IR text; `emit_object` shells out to `llc -filetype=obj`).

### 4. New Driver Command
Add a module under `polyglot_lib/commands/` and register it in
`polyglot_lib/cli.py`.

## Performance Considerations

### Python Driver
- Minimal overhead (direct compiler invocation)
- sha256-keyed incremental object cache, `-j` parallel per-TU compilation
- No persistent processes; temporary directory cleanup

### Rust Pipeline
- Fully in-process frontend (no external compiler for C sources)
- Zero-cost abstractions; textual LLVM IR as the exchange format

### Caching Strategy
- Content-addressable storage in `.polyglot-cache/`
- Key: hash of compiler path + version, exact flag list, source name and bytes, project directory
- Invalidation: config change, compiler update

## Security Model

### Trust Boundaries
```
User Code ──▶ Driver (trusted) ──▶ Compiler (trusted) ──▶ Binary
                    │
                    ▼
              Plugins (separate processes)
              - Communicate over stdio only
              - JSON-RPC input validation
              - Handshake failure → exit 3
```

### Mitigations
- Input validation on all paths
- No shell=True in subprocess
- Temporary files with secure random names
- Plugin isolation via process boundaries

## Testing Strategy

### Unit Tests (Rust)
- Per-crate tests in `crates/*/src/`
- Run with `cargo test --workspace`; `cargo clippy --workspace --all-targets -- -D warnings` must stay clean

### Integration Tests (Python)
- `test_comprehensive.py` - End-to-end driver tests

### Benchmarks
- `benchmark.py` - Performance regression detection
- Tracks: compile time, binary size, optimization levels

## Deployment

### Python Driver
- `polyglot_lib/` package + launcher; pip-installable (`pyproject.toml`)
- Dependencies: Python 3.8+, toml, any one C compiler
- Distribution: `pipx install git+https://github.com/RXVEN-1907/polyglot-compiler`, install.sh, or run from source

### Rust Binaries
- Default build has zero system dependencies (no LLVM needed)
- `--backend native` uses the system `llc`/`cc` when object code is wanted
- Distribution: `cargo build --release` (CI artifacts)
