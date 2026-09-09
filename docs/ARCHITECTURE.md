# Polyglot Compiler Architecture

## Overview

Polyglot is designed as a **modular, extensible compiler infrastructure** with a clear separation between the user-facing driver and the backend compilation pipeline.

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
│                        POLYGLOT DRIVER (Python)                              │
│  ┌──────────────────────────────────────────────────────────────────────┐   │
│  │                        Command Router                                 │   │
│  │  build │ run │ check │ emit-ir │ emit-asm │ clean │ stats │ init    │   │
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
│   Clang (C)         │ │  Clang++ (C++)      │ │  Future Plugins     │
│   -fsyntax-only     │ │  -fsyntax-only      │ │  (Rust, Swift,      │
│  -S -emit-llvm      │ │  -S -emit-llvm      │ │   Zig, etc.)        │
│  -O0 through -O3    │ │  -O0 through -O3    │ │                     │
│  --target=...       │ │  --target=...       │ │                     │
└─────────────────────┘ └─────────────────────┘ └─────────────────────┘
```

## Component Details

### 1. Python Driver (`polyglot`)

**File:** `polyglot` (~460 lines)

**Responsibilities:**
- Command-line argument parsing
- Project configuration management (`polyglot.toml`)
- Build orchestration (invokes Clang)
- Diagnostic formatting and reporting
- Temporary file management

**Key Classes:**
```python
class PolyglotDriver:
    def build(self, args)        # Build source files or project
    def run(self, args)          # Build and execute
    def check(self, args)        # Syntax check only
    def emit_ir(self, args)      # Emit LLVM IR
    def emit_asm(self, args)     # Emit assembly
    def clean(self, args)        # Clean artifacts
    def init(self, args)         # Initialize project
    def stats(self, args)        # Compilation statistics
    def version(self)            # Show version
```

### 2. Rust Library Crates

#### polyglot-protocol
**Purpose:** JSON-RPC 2.0 communication between driver and plugins

**Key Types:**
- `Request/Response/Notification` - Protocol messages
- `Transport` - Stdio-based transport
- `Handler` - Request/response handler
- `InitializeParams/InitializeResult` - Handshake

#### polyglot-mlir
**Purpose:** MLIR C API bindings and optimization passes

**Key Types:**
- `Context` - MLIR context with dialect loading
- `Module` - MLIR module representation
- `PassManager` - Optimization pass pipeline
- `Dialect` - Builtin dialects (arith, func, memref, scf, vector, llvm, cf)

**Optimization Passes:**
- Canonicalization
- CSE (Common Subexpression Elimination)
- LICM (Loop Invariant Code Motion)
- Inlining
- SCCP (Sparse Conditional Constant Propagation)

#### polyglot-codegen
**Purpose:** Cranelift codegen backend (MLIR → CLIF → native)

**Key Types:**
- `CodegenBackend` - Main codegen interface
- `TargetConfig` - Target triple, CPU, features
- `OptimizationLevel` - Mapping to Cranelift levels
- `ObjectFile` - Emitted object file

**Supported Targets:**
- x86_64 (Intel/AMD 64-bit)
- aarch64 (ARM 64-bit)
- riscv64 (RISC-V 64-bit)
- wasm32 (WebAssembly)

#### polyglot-driver
**Purpose:** Build orchestration and plugin management

**Key Types:**
- `BuildEngine` - Main build orchestration
- `PluginManager` - Plugin discovery and lifecycle
- `ProjectConfig` - polyglot.toml parsing
- `BuildPlan` - Compilation plan

#### polyglot-cache
**Purpose:** Incremental compilation cache

**Key Types:**
- `Cache` - Main cache interface
- `ContentAddressableStore` - CAS for artifacts
- `CacheKey` - Content-based keys
- `CacheEntry` - Cached compilation results

#### polyglot-registry
**Purpose:** Plugin registry client

**Key Types:**
- `RegistryClient` - HTTP client for registry
- `PluginMetadata` - Plugin information
- `SearchQuery` - Plugin search parameters

#### polyglot-syntax
**Purpose:** Lexer, parser, AST for C-like languages

**Key Types:**
- `Lexer` - Tokenizer
- `Parser` - Recursive descent parser
- `AST` - Abstract syntax tree nodes
- `Preprocessor` - C preprocessor handling

#### polyglot-sema
**Purpose:** Semantic analysis and type checking

**Key Types:**
- `Analyzer` - Semantic analysis driver
- `TypeChecker` - Type inference and checking
- `SymbolTable` - Scope management
- `DiagnosticEmitter` - Error/warning reporting

#### polyglot-core
**Purpose:** Core types shared across crates

**Key Types:**
- `Span` - Source location tracking
- `Diagnostic` - Structured diagnostics
- `Symbol` - Interned identifiers
- `FileId` - Source file identification

#### polyglot-ir
**Purpose:** MLIR lowering, optimization, printing

**Key Types:**
- `LoweringContext` - AST to MLIR lowering
- `Optimizer` - MLIR optimization pipeline
- `Printer` - MLIR textual output

### 3. Plugin Architecture

Plugins are standalone binaries communicating via **JSON-RPC 2.0 over stdio**.

**Protocol Flow:**
```
Driver                          Plugin
  │                                │
  ├── Initialize(InitializeParams)▶│
  │◀── InitializeResult ───────────┤
  │                                │
  ├── Compile(CompileParams) ─────▶│
  │◀── CompileResult ──────────────┤
  │                                │
  ├── Shutdown() ─────────────────▶│
  │◀── ShutdownResult ─────────────┤
```

**Plugin Methods:**
- `initialize` - Handshake, capability exchange
- `compile` - Compile source to MLIR/object
- `shutdown` - Cleanup

### 4. Build Pipeline

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
│  Select Compiler │
│  clang/clang++   │
└────────┬─────────┘
         │
         ▼
┌──────────────────┐
│  Build Command   │
│  -O<level>       │
│  --target=...    │
│  -o output       │
└────────┬─────────┘
         │
         ▼
┌──────────────────┐
│  Execute Clang   │
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
polyglot.toml → TOML Parser → ProjectConfig → BuildEngine → Clang Args
```

### Compilation Flow
```
Source Files → Driver → Clang → LLVM IR → Optimizations → Machine Code → Link → Binary
```

### Plugin Flow (Future)
```
Source Files → Driver → JSON-RPC → Plugin → MLIR → Optimizations → Cranelift → Object → Link → Binary
```

## Extension Points

### 1. New Language Frontend
Implement a plugin with:
- `initialize` - Declare language support
- `compile` - Parse source → emit MLIR

### 2. New Optimization Pass
Add to `polyglot-mlir` pass pipeline:
```rust
pass_manager.add_pass(create_my_custom_pass());
```

### 3. New Target Backend
Extend `polyglot-codegen`:
```rust
impl TargetConfig for MyTarget { ... }
```

### 4. New Driver Command
Add subcommand in `polyglot`:
```python
def new_command(self, args):
    # Implementation
```

## Performance Considerations

### Python Driver
- Minimal overhead (direct Clang invocation)
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

## Deployment

### Python Driver
- Single file (`polyglot`)
- Dependencies: Python 3.8+, toml, Clang
- Distribution: GitHub Releases, pip (future)

### Rust Binaries
- Statically linked where possible
- Dependencies: libzstd, libtinfo, libxml2 (system)
- Distribution: GitHub Releases, cargo install (future)

### Rust Libraries
- Published to crates.io
- Versioned with workspace version
- MSRV: Rust 1.75+