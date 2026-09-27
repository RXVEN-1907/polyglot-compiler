# Architecture Overview

!!! note
    Updated for the v0.2.0 driver. The driver now auto-discovers GCC *or* clang (on or off `PATH`), caches compiled objects incrementally, and wraps the LLVM auxiliary tools. See [Python Driver](python-driver.md) and the [Commands Reference](../user-guide/commands.md).

## High-Level Architecture

Polyglot is designed as a **dual-layer architecture** with a clean separation between the user-facing driver and the backend compilation pipeline.

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
│                        POLYGLOT DRIVER (Python)                              │
│  ┌──────────────────────────────────────────────────────────────────────┐   │
│  │                        Command Router                                 │   │
│  │  build │ run │ check │ emit-ir │ emit-asm │ preproc │ deps │ doctor  │   │
│  │  targets │ fmt │ tidy │ repl │ jit │ mca │ analyze │ watch │ stats   │   │
│  │  init │ clean │ version                                              │   │
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
│   GCC / g++         │ │  clang / clang++    │ │  rustc (Rust        │
│   -fsyntax-only     │ │  -fsyntax-only      │ │  projects)          │
│  -S (asm), GIMPLE   │ │  -S -emit-llvm      │ │                     │
│  -O0 through -Oz    │ │  -O0 through -Oz    │ │  Future plugins:    │
│  host target only   │ │  --target=...       │ │  Swift, Zig, etc.   │
└─────────────────────┘ └─────────────────────┘ └─────────────────────┘
          plus auxiliary LLVM tools: clang-format, clang-tidy,
          llvm-mca, lli, clang-repl, llvm-nm/size/objdump/...
```

---

## Component Responsibilities

### Python Driver (`polyglot`)

**File:** `polyglot` (~1,760 lines)

**Responsibilities:**
- Command-line argument parsing
- Project configuration management (`polyglot.toml`)
- Build orchestration (invokes the discovered gcc/clang, via an incremental object cache)
- Diagnostic formatting and reporting
- Temporary file management
- Wrapping the LLVM auxiliary tools (clang-format, clang-tidy, llvm-mca, lli, clang-repl)

**Key structure** (v0.2.0 is function-oriented, not class-based — see [Python Driver](python-driver.md)):
```python
class Toolchain:            # compiler + auxiliary tool discovery
class BuildCache:           # sha256-keyed incremental object cache
def cmd_build(tc, args)     # one cmd_* function per subcommand
def cmd_run(tc, args)
def cmd_check(tc, args)     # ... 20 commands total
def main(argv)              # argparse dispatch, exit codes 0/1/2/3
```

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

### Plugin Flow (Future)
```
Source Files → Driver → JSON-RPC → Plugin → MLIR → Optimizations → Cranelift → Object → Link → Binary
```

---

## Extension Points

### 1. New Language Frontend
Implement a plugin with:
- `initialize` - Declare language support
- `compile` - Parse source → emit MLIR

### 2. New Optimization Pass
Add to MLIR pass pipeline:
```rust
pass_manager.add_pass(create_my_custom_pass());
```

### 3. New Target Backend
Extend Cranelift codegen:
```rust
impl TargetConfig for MyTarget { ... }
```

### 4. New Driver Command
Add subcommand in `polyglot`:
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

### Rust Crates (Library)
- Zero-cost abstractions
- MLIR C API (FFI overhead minimal)
- Cranelift: fast codegen, good optimization

### Caching Strategy
- Content-addressable storage
- Key: hash(source + config + compiler_version)
- Invalidation: config change, compiler update

---

## Security Model

### Trust Boundaries
```
User Code ──▶ Driver (trusted) ──▶ Clang (trusted) ──▶ Binary
                    │
                    ▼
              Plugins (sandboxed)
              - No filesystem access
              - No network access
              - Resource limits
```

### Mitigations
- Input validation on all paths
- No shell=True in subprocess
- Temporary files with secure random names
- Plugin isolation via process boundaries

---

## Future Architecture

### Phase 1: MLIR Integration
```
Python Driver ──▶ JSON-RPC ──▶ polyglot-clang plugin ──▶ MLIR ──▶ polyglot-codegen ──▶ Native
```

### Phase 2: Incremental Compilation
```
Build Engine ──▶ polyglot-cache ──▶ CAS ──▶ Reuse artifacts
```

### Phase 3: Plugin Registry
```
Driver ──▶ polyglot-registry ──▶ Registry API ──▶ Download plugins
```

### Phase 4: LSP Integration
```
Editor ──▶ LSP ──▶ Driver ──▶ Diagnostics/Completions
```

---

## Testing Strategy

### Unit Tests (Rust)
- Per-crate tests in `crates/*/src/`
- Run with `cargo test --lib --workspace`

### Integration Tests (Python)
- `test_comprehensive.py` - End-to-end driver tests
- Tests: build, run, check, emit-ir, emit-asm, clean, init, stats

### Benchmarks
- `benchmark.py` - Performance regression detection
- Tracks: compile time, binary size, optimization levels

---

## Deployment

### Python Driver
- Single file (`polyglot`)
- Dependencies: Python 3.8+, toml, plus any discovered gcc/clang
- Distribution: GitHub Releases, pip (future)

### Rust Binaries
- Statically linked where possible
- Dependencies: libzstd, libtinfo, libxml2 (system)
- Distribution: GitHub Releases, cargo install (future)

### Rust Libraries
- Published to crates.io
- Versioned with workspace version
- MSRV: Rust 1.75+