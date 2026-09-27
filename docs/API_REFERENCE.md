# Polyglot API Reference

!!! note
    This document predates the v0.2.0 driver rewrite. The maintained CLI reference lives in the mkdocs tree: `docs/reference/cli.md` (and `docs/user-guide/commands.md`). The global options below have since grown (`--quiet`, `--compiler {auto,clang,gcc}`, and per-command flags such as `build --sanitize/--shared/-j/--no-cache`, `fmt`, `tidy`, `watch`, `deps`, `preproc`).

## Python Driver CLI

### Global Options

| Option | Short | Description | Default |
|--------|-------|-------------|---------|
| `--verbose` | `-v` | Verbose output | `false` |
| `--target TRIPLE` | | Target triple | Native |
| `--opt-level LEVEL` | | Optimization level | `default` |
| `--output FILE` | `-o` | Output file | Auto |
| `--plugin PLUGIN` | | Plugin to use | - |
| `--plugin-dir DIR` | | Plugin directory (legacy) | - |
| `--compiler {auto,clang,gcc}` | | Force a backend | `auto` (env `POLYGLOT_CC`/`POLYGLOT_CXX` override) |
| `--quiet` | | Suppress `Running:` lines | off |

### Optimization Levels

| Level | Clang Flag | Description |
|-------|------------|-------------|
| `none` | `-O0` | No optimization, fastest compile |
| `less` | `-O1` | Basic optimizations |
| `default` | `-O2` | Standard optimizations |
| `aggressive` | `-O3` | Aggressive optimizations |
| `size` | `-Os` | Optimize for size |
| `size-aggressive` | `-Oz` | Aggressive size optimization |

### Target Triples

| Target | Description |
|--------|-------------|
| `x86_64-unknown-linux-gnu` | Linux x86_64 (default on x86_64) |
| `aarch64-unknown-linux-gnu` | Linux ARM64 |
| `arm-unknown-linux-gnueabihf` | Linux ARM32 hard-float |
| `riscv64-unknown-linux-gnu` | Linux RISC-V 64-bit |
| `wasm32-wasip1` | WebAssembly WASI |

---

## Commands

### `build`

Build source file(s) or project.

```bash
polyglot build [OPTIONS] [PATHS...]
```

**Arguments:**
- `PATHS` - Source files or project directory (default: current directory)

**Options:**
- `-o, --output FILE` - Output file path

**Examples:**
```bash
# Build single file
polyglot build hello.c -o hello

# Build multiple files
polyglot build main.c utils.c -o myapp

# Build project (reads polyglot.toml)
polyglot build

# Build with optimization
polyglot build --opt-level aggressive hello.c -o hello-O3

# Cross-compile
polyglot build --target aarch64-unknown-linux-gnu hello.c -o hello-arm64
```

**Exit Codes:**
- `0` - Success
- `1` - Compilation failed
- `2` - Invalid arguments

---

### `run`

Build and run a source file in one command.

```bash
polyglot run [OPTIONS] FILE [ARGS...]
```

**Arguments:**
- `FILE` - Source file to run
- `ARGS` - Arguments passed to the program

**Options:**
- `--target TRIPLE` - Target triple
- `--opt-level LEVEL` - Optimization level

**Examples:**
```bash
# Run C program
polyglot run hello.c

# Run with arguments
polyglot run args.c -- --verbose --input file.txt

# Run with optimization
polyglot run --opt-level aggressive bench.c
```

**Exit Codes:**
- Program's exit code
- `1` - Compilation failed

---

### `check`

Check syntax without building (uses `-fsyntax-only`).

```bash
polyglot check [OPTIONS] PATHS...
```

**Arguments:**
- `PATHS` - Source files to check (one or more)

**Options:**
- `--target TRIPLE` - Target triple

**Examples:**
```bash
# Check single file
polyglot check hello.c

# Check multiple files
polyglot check main.c utils.c math.c

# Check with target
polyglot check --target aarch64-unknown-linux-gnu hello.c
```

**Output:**
- Success: `All files passed syntax check`
- Failure: Clang error messages

**Exit Codes:**
- `0` - All files valid
- `1` - Syntax errors found

---

### `emit-ir`

Emit LLVM IR (uses `-S -emit-llvm`).

```bash
polyglot emit-ir [OPTIONS] PATHS...
```

**Arguments:**
- `PATHS` - Source files (one or more)

**Options:**
- `-o, --output FILE` - Output file (default: `<source>.ll`; one file per command)
- `--target TRIPLE` - Target triple

**Examples:**
```bash
# Emit to file
polyglot emit-ir hello.c -o hello.ll

# Emit to the default file (hello.ll)
polyglot emit-ir hello.c

# Multiple files (no -o: -o with multiple sources is an error)
polyglot emit-ir main.c utils.c
```

**Exit Codes:**
- `0` - Success
- `1` - Compilation failed

---

### `emit-asm`

Emit assembly (uses `-S`).

```bash
polyglot emit-asm [OPTIONS] PATHS...
```

**Arguments:**
- `PATHS` - Source files (one or more)

**Options:**
- `-o, --output FILE` - Output file (default: `<source>.s`; one file per command)
- `--target TRIPLE` - Target triple

**Examples:**
```bash
# Emit to file
polyglot emit-asm hello.c -o hello.s

# Emit to the default file (hello.s)
polyglot emit-asm hello.c

# Intel syntax
polyglot emit-asm hello.c -o hello.intel.s --intel
```

**Exit Codes:**
- `0` - Success
- `1` - Compilation failed

---

### `clean`

Clean build artifacts.

```bash
polyglot clean [PATH]
```

**Arguments:**
- `PATH` - Project directory (default: current directory)

**Removes:**
- `*.o`, `*.obj` - Object files
- `*.exe`, `a.out` - Executables
- `*.ll` - LLVM IR files
- `*.s` - Assembly files
- `*.bc` - Bitcode files
- `.polyglot-cache/` contents

The project executable named in `polyglot.toml` is left in place.

**Examples:**
```bash
# Clean current directory
polyglot clean

# Clean specific project
polyglot clean my-project/
```

**Exit Codes:**
- `0` - Success

---

### `stats`

Show compilation statistics.

```bash
polyglot stats [OPTIONS] PATHS...
```

**Arguments:**
- `PATHS` - Source files (one or more)

**Options:**
- `--target TRIPLE` - Target triple
- `--opt-level LEVEL` - Optimization level

**Output Includes:**
- Source file sizes and line counts
- Target and optimization level
- Clang time report (parse, optimize, codegen)
- Binary size

**Examples:**
```bash
# Basic stats
polyglot stats hello.c

# With optimization level
polyglot stats --opt-level aggressive hello.c

# Multiple files
polyglot stats main.c utils.c
```

**Exit Codes:**
- `0` - Success
- `1` - Compilation failed

---

### `init`

Initialize a new project.

```bash
polyglot init [NAME] [OPTIONS]
```

**Arguments:**
- `NAME` - Project name (default: `my-project`)

**Options:**
- `--language LANG` - Language: `c`, `cpp`, `rust` (default: `c`)

**Creates:**
```
NAME/
├── polyglot.toml      # Project configuration
└── src/
    └── main.c         # Sample source file
```

**Examples:**
```bash
# C project (default)
polyglot init my-app

# C++ project
polyglot init my-app --language cpp

# Rust project (builds via rustc)
polyglot init my-app --language rust
```

**Exit Codes:**
- `0` - Success
- `1` - Directory exists or creation failed

---

### `version`

Show version information.

```bash
polyglot version
```

**Output:**
```
polyglot 0.2.0
```

**Exit Codes:**
- `0` - Success

---

## Configuration File: `polyglot.toml`

### Structure

```toml
[project]
name = "my-app"
version = "0.1.0"

[target]
triple = "x86_64-unknown-linux-gnu"
opt_level = 2

[plugins]
clang = "0.1"

[[bin]]
name = "my-app"
source = "src/main.c"
```

### Sections

#### `[project]`
| Key | Type | Required | Description |
|-----|------|----------|-------------|
| `name` | string | Yes | Project name |
| `version` | string | No | Project version (default: `"0.1.0"`) |

#### `[target]`
| Key | Type | Required | Description |
|-----|------|----------|-------------|
| `triple` | string | No | Target triple (default: native) |
| `opt_level` | integer | No | Optimization level 0-3 (default: `2`) |

#### `[plugins]`
| Key | Type | Required | Description |
|-----|------|----------|-------------|
| `clang` | string | No | Clang plugin version (default: `"0.1"`) |

#### `[[bin]]` (Array of tables)
| Key | Type | Required | Description |
|-----|------|----------|-------------|
| `name` | string | Yes | Binary name |
| `source` | string | Yes | Main source file path |

---

## Environment Variables

| Variable | Description |
|----------|-------------|
| `PATH` | Must include Clang/LLVM bin directory |
| `POLYGLOT_CC` | Absolute path or name of the C compiler (highest precedence) |
| `POLYGLOT_CXX` | Absolute path or name of the C++ compiler (highest precedence) |
| `NO_COLOR` | Disable colored output |

---

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | Success |
| `1` | General error (compilation failed, file not found, etc.) |
| `2` | Invalid command-line arguments |
| `3` | Missing tool or backend (e.g. clang not found, `analyze` without clang) |
| `130` | Interrupted (Ctrl+C) |

---

## Rust Library APIs

### polyglot-protocol

```rust
// JSON-RPC 2.0 types
use polyglot_protocol::{Request, Response, Notification, Transport, Handler};

// Initialize handshake
let params = InitializeParams {
    protocol_version: "2.0".to_string(),
    capabilities: ClientCapabilities::default(),
};

let request = Request::new("initialize", params);
let response = transport.send(request).await?;
```

### polyglot-mlir

```rust
use polyglot_mlir::{Context, Module, PassManager, Dialect};

// Create context with dialects
let ctx = Context::new();
ctx.load_dialect(Dialect::Arith);
ctx.load_dialect(Dialect::Func);
ctx.load_dialect(Dialect::MemRef);

// Parse MLIR module
let module = ctx.parse_module(mlir_text)?;

// Run optimization passes
let pm = PassManager::new(&ctx);
pm.add_pass("canonicalize");
pm.add_pass("cse");
pm.add_pass("licm");
pm.run(&module)?;
```

### polyglot-codegen

```rust
use polyglot_codegen::{CodegenBackend, TargetConfig, OptimizationLevel};

let target = TargetConfig {
    triple: "x86_64-unknown-linux-gnu".parse()?,
    cpu: "generic".to_string(),
    features: "+sse2,+avx".to_string(),
    opt_level: OptimizationLevel::Default,
};

let mut backend = CodegenBackend::new(target)?;
let object_file = backend.compile_module(&module)?;
```

### polyglot-driver

```rust
use polyglot_driver::{BuildEngine, ProjectConfig, PluginManager};

let config = ProjectConfig::from_file("polyglot.toml")?;
let engine = BuildEngine::new(config);
let result = engine.build(BuildOptions::default()).await?;
```

### polyglot-cache

```rust
use polyglot_cache::{Cache, CacheKey};

let cache = Cache::new("/tmp/polyglot-cache")?;
let key = CacheKey::from_source(&source, &config);
if let Some(entry) = cache.get(&key)? {
    // Use cached result
}
```

---

## Error Handling

### Python Driver Errors

All errors print to stderr and exit with non-zero code.

**Common Errors:**
```
Error: clang not found in PATH
Error: Source file not found: hello.c
Error: Compilation failed with exit code 1
Error: Config parse error: ...
```

### Rust Library Errors

All libraries use `thiserror` for structured errors.

```rust
use polyglot_driver::DriverError;

match engine.build(opts).await {
    Ok(result) => println!("Built: {}", result.output),
    Err(DriverError::ConfigError(msg)) => eprintln!("Config: {}", msg),
    Err(DriverError::CompilationFailed(msg)) => eprintln!("Compile: {}", msg),
    Err(e) => eprintln!("Error: {}", e),
}
```

---

## Examples

### Complete Build Script

```bash
#!/bin/bash
# build.sh - Example build script

set -e

PROJECT="my-app"
SRC_DIR="src"
BUILD_DIR="build"

# Create build directory
mkdir -p "$BUILD_DIR"

# Build with stats
echo "Building $PROJECT..."
polyglot build "$SRC_DIR/main.c" "$SRC_DIR/utils.c" \
    -o "$BUILD_DIR/$PROJECT" \
    --opt-level aggressive

# Show stats
polyglot stats "$SRC_DIR/main.c" --opt-level aggressive

# Run
echo "Running..."
"$BUILD_DIR/$PROJECT"

echo "Done!"
```

### Cross-Compilation Script

```bash
#!/bin/bash
# cross-compile.sh

TARGETS=(
    "x86_64-unknown-linux-gnu"
    "aarch64-unknown-linux-gnu"
    "arm-unknown-linux-gnueabihf"
    "riscv64-unknown-linux-gnu"
    "wasm32-wasip1"
)

for TARGET in "${TARGETS[@]}"; do
    echo "Building for $TARGET..."
    polyglot build --target "$TARGET" hello.c -o "hello-${TARGET//\//-}" || echo "Failed for $TARGET"
done
```

### CI Pipeline Example

```yaml
# .github/workflows/build.yml
name: Build

on: [push, pull_request]

jobs:
  build:
    runs-on: ubuntu-latest
    strategy:
      matrix:
        target:
          - x86_64-unknown-linux-gnu
          - aarch64-unknown-linux-gnu
    steps:
      - uses: actions/checkout@v4
      - name: Install Clang
        run: sudo apt-get install -y clang-17
      - name: Build
        run: |
          chmod +x polyglot
          polyglot build --target ${{ matrix.target }} hello.c -o hello-${{ matrix.target }}
```

---

## Version Compatibility

| Polyglot | Python | Clang | Rust | MLIR |
|----------|--------|-------|------|------|
| 0.1.x | 3.8+ | 17+ | 1.75+ | 220 |

---

## Changelog

See [CHANGELOG.md](https://github.com/RXVEN-1907/polyglot-compiler/blob/master/CHANGELOG.md) for version history.