# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Comprehensive GitHub repository setup with CI/CD
- Issue templates and PR templates
- Contributing guidelines and code of conduct
- Security policy documentation

## [0.2.0] - 2026-09-27

### Added
- **New commands**
  - `doctor` - Toolchain environment report (`--strict` exits 1 when components are missing)
  - `targets` - List known target triples and their availability
  - `fmt` - Format code with clang-format (`--style`, `--check`, `-i`, `--dump-config`)
  - `tidy` - Lint with clang-tidy (`--checks`, `--fix`)
  - `repl` - Interactive clang-repl session
  - `jit` - Execute LLVM IR/bitcode with `lli`
  - `mca` - Machine-code analysis with `llvm-mca` (accepts `.c` source or `.s` assembly)
  - `analyze` - Clang static analyzer (`--analyze`)
  - `preproc` - Preprocessor output (`-E`, `--no-lines` to strip line markers)
  - `deps` - Make-style dependency files (`-MM`, `--target-name`, `-o`)
  - `watch` - Poll-and-rebuild loop (`--timeout`)
- **Build flags** (available on `build`, `run`, and the analysis commands)
  - `--std` (c89..c2x / gnu*, c++98..c++23)
  - `--warn {none,default,all,extra,pedantic}` and `--werror`
  - `--debug` (emit debug info)
  - `--sanitize address,undefined,thread,memory,leak`
  - `--shared` and `--static-lib` (static library via `ar rcs`)
  - `-I` / `-D` / `-L` / `-l` (repeatable include, define, library path, library)
  - `--cc-arg` (raw flag passthrough to the compiler)
  - `-j N` parallel per-TU compilation
- **Incremental build cache** - sha256-keyed object cache in `.polyglot-cache/` with
  `manifest.json`; bypass with `--no-cache`
- **GCC backend** - `--compiler {auto,clang,gcc}`; on the GCC backend `emit-ir` emits
  GIMPLE dumps (with a note), `stats` reports GCC-style phase timings
- **Rust support** - `init --language rust` scaffolds a rustc-built project
- **Machine-readable output** - `stats --json`
- **`emit-asm --intel`** - Intel-syntax assembly output
- **Output control** - `--quiet`, `--verbose` (full command lines plus cache HIT/MISS
  lines), `NO_COLOR`
- **Plugin architecture is real** - `--plugin NAME|PATH` (with `--plugin-dir`, default
  `~/.polyglot/plugins`) drives a JSON-RPC 2.0 over stdio plugin during `build`:
  `initialize` handshake, per-TU `compile` requests, IR text linked into the requested
  output; `polyglot.toml` `[plugins]` entries are used best-effort with fallback to the
  direct compiler; `polyglot doctor` lists detected plugins; the shipped
  `polyglot-clang` plugin now logs to stderr so its stdout stays a clean JSON-RPC stream
- **Defined exit codes** - 0 success, 1 compile/run failure, 2 usage, 3 missing tool
- Driver test suite grown from 14 to 35 passing tests

### Changed
- **Toolchain discovery replaces the hard clang requirement** - resolution order:
  `POLYGLOT_CC`/`POLYGLOT_CXX` env vars, then `--compiler {auto,clang,gcc}`, then
  `PATH` (clang, clang-NN..., gcc, cc), then scan dirs (`~/llvm-install/*/bin`,
  `/usr/lib/llvm-*/bin`, `/usr/local/bin`, `/opt/*/bin`)
- `init --language rust` now produces a functional build (via rustc)
- `clean` also clears `.polyglot-cache`
- Driver rewritten as a single-file Python 3 executable (stdlib + `toml` only);
  version string bumped to 0.2.0
- Rust workspace: the 4 orphaned crates (`polyglot-core`, `polyglot-syntax`,
  `polyglot-sema`, `polyglot-ir`) are wired back in and all 11 members build;
  MLIR crates compile again (`MLIR_SYS_220_PREFIX` baked into `.cargo/config.toml`)

### Fixed
- Driver was unusable when clang was not on `PATH` - it now discovers LLVM installs
  (e.g. under `~/llvm-install/`) and falls back to gcc/g++

### Compatibility
- The full 0.1.0 CLI surface is unchanged: `build`, `run`, `check`, `emit-ir`,
  `emit-asm`, `clean`, `stats`, `init`, `version`, plus the global `--opt-level`,
  `--target`, `-o` and plugin flags.

## [0.1.0] - 2024-08-31

### Added
- **Python Driver** (`polyglot`) - Universal compiler driver for C/C++
  - `build` command - Compile source files or projects
  - `run` command - Build and execute in one step
  - `check` command - Syntax validation without binary output
  - `emit-ir` command - LLVM IR emission
  - `emit-asm` command - Assembly output generation
  - `clean` command - Remove build artifacts
  - `stats` command - Compilation statistics and time reports
  - `init` command - Project scaffolding with `polyglot.toml`
  - `version` command - Version information
- **Optimization Levels** - Support for `-O0` through `-O3`, `-Os`, `-Oz`
- **Cross-compilation** - Target triples for x86_64, aarch64, arm, riscv64, wasm32
- **Project Configuration** - `polyglot.toml` for project settings
- **Multi-file Compilation** - Build multiple source files in one command
- **Automatic Compiler Detection** - Chooses clang/clang++ based on file extension

### Rust Library Architecture (10 crates)
- **polyglot-protocol** - JSON-RPC 2.0 implementation for driver-plugin communication
- **polyglot-mlir** - MLIR C API bindings with optimization passes
- **polyglot-codegen** - Cranelift codegen backend (MLIR → CLIF → native)
- **polyglot-driver** - Build orchestration and plugin management
- **polyglot-cache** - Incremental compilation cache (content-addressable)
- **polyglot-registry** - Plugin registry client with search/download
- **polyglot-syntax** - Lexer, parser, AST for C-like languages
- **polyglot-sema** - Semantic analysis and type checking
- **polyglot-core** - Core types, diagnostics, source spans
- **polyglot-ir** - MLIR lowering, optimization, and printing

### Plugin
- **polyglot-clang** - Clang plugin for C/C++ frontend (code complete)

### Testing & Quality
- 7/7 Python integration tests passing
- Comprehensive benchmark suite
- Example programs covering C language features
- Error diagnostics for syntax errors and undefined functions

### Documentation
- Complete README with usage examples
- Architecture diagrams and component overview
- Configuration reference
- Benchmark results

### Known Limitations
- Rust binaries require static system libraries (`libzstd.a`, `libtinfo.a`, `libxml2.a`)
- MLIR pipeline not yet integrated with Python driver
- Incremental compilation cache not connected to driver
- Plugin registry not yet operational

---

## Version History

### Pre-Release Development

#### 2024-08-17 - Project Initialization
- Initial project structure
- Cargo workspace setup
- Basic Python driver skeleton

#### 2024-08-22 - Core Crates Implementation
- polyglot-protocol: JSON-RPC 2.0 implementation
- polyglot-core: Core types and diagnostics
- polyglot-syntax: Lexer and parser framework

#### 2024-08-23 - MLIR and Driver Crates
- polyglot-mlir: MLIR C API bindings
- polyglot-driver: Build orchestration
- polyglot-registry: Plugin registry client

#### 2024-08-24 - Codegen and Cache
- polyglot-codegen: Cranelift backend
- polyglot-cache: Incremental compilation
- polyglot-ir: MLIR optimization passes

#### 2024-08-31 - Integration and Testing
- Complete Python driver implementation
- Clang plugin implementation
- Comprehensive test suite
- Benchmark infrastructure
- Documentation

---

## Release Process

1. Update version in `Cargo.toml` and `polyglot` script
2. Update `CHANGELOG.md` with release notes
3. Create git tag: `git tag v0.1.0`
4. Push tag: `git push origin v0.1.0`
5. GitHub Actions creates release automatically
6. Publish to crates.io (for Rust crates)

---

## Links

- [GitHub Releases](https://github.com/RXVEN-1907/polyglot-compiler/releases)
- [Issues](https://github.com/RXVEN-1907/polyglot-compiler/issues)
- [Pull Requests](https://github.com/RXVEN-1907/polyglot-compiler/pulls)