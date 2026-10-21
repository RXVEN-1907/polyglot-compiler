# Architecture Overview

!!! note
    Updated for the v0.3.0 rebuild. The Python driver auto-discovers GCC *or* clang (on or off `PATH`), caches compiled objects incrementally, and wraps the LLVM auxiliary tools. The Rust workspace is a self-contained pipeline whose default build needs no system LLVM. See [Python Driver](python-driver.md) and the [Commands Reference](../user-guide/commands.md).

## High-Level Architecture

Polyglot is designed as a **dual-layer architecture** with a clean separation between the user-facing driver and the experimental Rust compiler pipeline.

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
│  │  build │ run │ check │ emit-ir │ emit-asm │ preproc │ deps │ doctor  │   │
│  │  targets │ fmt │ tidy │ repl │ jit │ mca │ analyze │ watch │ stats   │   │
│  │  inspect │ query │ nm │ objdump │ readelf │ ... (49 commands total)  │   │
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
│   -fsyntax-only     │ │  -fsyntax-only      │ │  (in-process        │
│  -S (asm), GIMPLE   │ │  -S -emit-llvm      │ │  pipeline, below)   │
│  -O0 through -Oz    │ │  -O0 through -Oz    │ │                     │
│  host target only   │ │  --target=...       │ │  Future plugins:    │
└─────────────────────┘ └─────────────────────┘ │  Swift, Zig, etc.   │
          plus auxiliary LLVM tools:            └─────────────────────┘
          clang-format, clang-tidy, llvm-mca,
          lli, clang-repl, llvm-nm/size/objdump/...
```

---

## Component Responsibilities

### Python Driver (`polyglot`)

**Files:** `polyglot` (thin launcher) + the `polyglot_lib/` package (~2,500 lines across `toolchain.py`, `backends.py`, `cache.py`, `config.py`, `output.py`, `plugin_client.py`, `inspection_cache.py`, `preview_assets.py`, and one module per command under `polyglot_lib/commands/`)

**Responsibilities:**
- Command-line argument parsing (49 subcommands)
- Project configuration management (`polyglot.toml`)
- Build orchestration (invokes the discovered gcc/clang, via an incremental object cache)
- Diagnostic formatting and reporting
- Temporary file management
- Wrapping the LLVM auxiliary tools (clang-format, clang-tidy, llvm-mca, lli, clang-repl) and the binutils/LLVM binary tools (nm, objdump, readelf, size, strings, strip, addr2line, c++filt)

**Key structure** (the driver is function-oriented, not class-based — see [Python Driver](python-driver.md)):
```python
class Toolchain:            # compiler + auxiliary tool discovery
class BuildCache:           # sha256-keyed incremental object cache
def cmd_build(tc, args)     # one cmd_* function per subcommand
def cmd_run(tc, args)
def cmd_check(tc, args)     # ... 49 commands total
def main(argv)              # argparse dispatch, exit codes 0/1/2/3
```

### Rust Workspace (7 crates + 1 plugin)

The Rust side is a self-contained compiler pipeline for C sources. The experimental MLIR/Cranelift paths were removed in 0.3.0; the active path is LLVM IR emission, and the default build needs no system LLVM.

| Crate | Purpose |
|-------|---------|
| `polyglot-core` | Spans, diagnostics, source files, core types (`Span`, `Diagnostic`, `SourceFile`, `Type`) |
| `polyglot-syntax` | Lexer and parser for C |
| `polyglot-sema` | Name resolution and type checking (`CheckedProgram`) |
| `polyglot-ir` | Lowering, optimization, printing (`lower_program`, `optimize_module`, `print_module`) |
| `polyglot-protocol` | JSON-RPC 2.0 types shared with plugins |
| `polyglot-codegen` | `polyglot-ir` → LLVM IR text (`emit_ll`); objects via `llc -filetype=obj` (`emit_object`) |
| `polyglot-driver` | clap CLI (`build`/`run`/`check`/`emit-ir`/`transmute-internal`) + in-process pipeline |
| `plugins/polyglot-clang` | Reference JSON-RPC stdio plugin server (compiles via `clang -S -emit-llvm`) |

The in-process pipeline (`crates/polyglot-driver/src/pipeline.rs`):

```
parse_source (syntax) → check (sema) → lower_program (IR)
  → optimize_module → { print_module (textual Polyglot IR),
                        emit_ll → LLVM IR text }
```

`--backend native` turns the emitted LLVM IR into a real object file with `llc -filetype=obj` (graceful fallback to writing the `.ll`) and links with the system `cc`.

---

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
Source Files → Driver → JSON-RPC → Plugin (polyglot-clang) → LLVM IR text → Driver links → Binary
```

---

## Extension Points

### 1. New Language Frontend
Implement a plugin with:
- `initialize` - Declare language support
- `compile` - Parse source → return IR text (JSON-RPC)

### 2. New Optimization Pass
Add it to the pass pipeline in `crates/polyglot-ir/src/opt.rs`.

### 3. New Backend Emitter
Extend `crates/polyglot-codegen`: `emit_ll` renders a `polyglot-ir` module as LLVM IR text, `emit_object` shells out to `llc -filetype=obj`.

### 4. New Driver Command
Add a module under `polyglot_lib/commands/` and register it in `polyglot_lib/cli.py`:
```python
def new_command(args):
    # Implementation
```

---

## Performance Considerations

### Python Driver
- Minimal overhead (direct compiler invocation, sha256-keyed object cache)
- No persistent processes
- Temporary directory cleanup

### Rust Pipeline
- Fully in-process frontend (no external compiler compiles the C source)
- Textual LLVM IR as the exchange format; the LLVM backend reconstructs SSA from allocas

### Caching Strategy
- Content-addressable storage (`.polyglot-cache/` in the project directory)
- Key: hash(source + config + compiler_version)
- Invalidation: config change, compiler update

---

## Security Model

### Trust Boundaries
```
User Code ──▶ Driver (trusted) ──▶ Compiler (trusted) ──▶ Binary
                    │
                    ▼
              Plugins (separate processes)
              - stdio-only communication
              - JSON-RPC input validation
              - Handshake failure → exit 3
```

### Mitigations
- Input validation on all paths
- No shell=True in subprocess
- Temporary files with secure random names
- Plugin isolation via process boundaries

---

## What Is Next

- **Plugin surface growth** — `polyglot plugin list/install/remove/info/publish` already speaks to a registry URL or JSON index; more capability reporting is planned.
- **Rust driver parity** — the in-process pipeline covers a growing subset of the Python driver's commands.
- **LSP integration** — `polyglot lsp` launches clangd with project flags today; deeper integration may follow.

---

## Testing Strategy

### Unit Tests (Rust)
- Per-crate tests in `crates/*/src/`
- Run with `cargo test --workspace`; `cargo clippy --workspace --all-targets -- -D warnings` must stay clean

### Integration Tests (Python)
- `test_comprehensive.py` - End-to-end driver tests across the command surface

### Benchmarks
- `benchmark.py` - Performance regression detection
- Tracks: compile time, binary size, optimization levels

---

## Deployment

### Python Driver
- `polyglot_lib/` package with a `polyglot` entry point (`pyproject.toml`)
- Dependencies: Python 3.8+, toml, plus any discovered gcc/clang
- Distribution: `pipx install git+https://github.com/RXVEN-1907/polyglot-compiler`, `install.sh`, or run from source

### Rust Binaries
- Default build has **zero system dependencies** (no system LLVM needed)
- `--backend native` uses the system `llc`/`cc` only when object code is wanted
- Distribution: `cargo build --release` (CI artifacts)
