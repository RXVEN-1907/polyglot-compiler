# Installation

## Prerequisites

| Requirement | Version | Notes |
|-------------|---------|-------|
| Python | 3.8+ | Required for driver |
| GCC or clang | any recent version | For C/C++ compilation — clang is **not** required; the driver auto-discovers whichever is available, including clang installs outside `PATH` |
| toml (Python) | Latest | `pip install toml` |
| Rust | 1.75+ | Optional, for `--language rust` projects and library crates |

### System Dependencies (Ubuntu/Debian)

```bash
sudo apt-get update && sudo apt-get install -y \
    python3 python3-pip \
    clang-17 llvm-17 lld-17 \
    libzstd-dev libtinfo-dev libxml2-dev  # For Rust binaries
```

### System Dependencies (macOS)

```bash
brew install python3 llvm@17
pip install toml
```

### System Dependencies (Windows)

Use WSL2 with Ubuntu, or install:
- Python from python.org
- LLVM from llvm.org
- `pip install toml`

---

## Quick Install

```bash
# Clone repository
git clone https://github.com/RXVEN-1907/polyglot-compiler
cd polyglot-compiler

# Install Python dependency
pip install toml

# Make executable
chmod +x polyglot

# Optional: Add to PATH
export PATH="$PWD:$PATH"

# Verify
polyglot version
# polyglot 0.2.0
```

---

## One-Line Install (Linux/macOS)

```bash
curl -fsSL https://raw.githubusercontent.com/RXVEN-1907/polyglot-compiler/master/polyglot -o polyglot && chmod +x polyglot && pip install toml
```

---

## Verify Installation

```bash
# Check version
polyglot version

# Test compilation
polyglot run examples/01_hello.c
# Should output: Hello, Polyglot!

# Test IR emission (clang emits LLVM IR; gcc emits GIMPLE)
polyglot emit-ir examples/01_hello.c | head -5

# Test statistics
polyglot stats examples/01_hello.c

# Full environment report
polyglot doctor
```

---

## Building Rust Components (Optional)

The Rust workspace crates implement an in-process pipeline (parse -> sema -> lower -> optimize -> print IR) plus MLIR/Cranelift library scaffolding.

```bash
# Install additional dependencies
sudo apt-get install -y libzstd-dev libtinfo-dev libxml2-dev

# Build all workspace crates
cargo build --workspace

# Run the in-process Rust pipeline
cargo run -p polyglot-driver -- emit-ir examples/01_hello.c
```

> **Note**: The Python driver works independently and provides full C/C++ compilation functionality. The Rust components are for advanced MLIR/Cranelift integration.

---

## Troubleshooting

### `clang not found in PATH`

Not an error — the driver falls back to gcc and also scans well-known install locations (`~/llvm-install/*/bin`, `/usr/lib/llvm-*/bin`, `/usr/local/bin`, `/opt/*/bin`). Run `polyglot doctor` to see what was found and where. To use a specific compiler:

```bash
# Pin by environment variable (highest precedence)
export POLYGLOT_CC=/usr/lib/llvm-17/bin/clang
export POLYGLOT_CXX=/usr/lib/llvm-17/bin/clang++

# ... or add clang to PATH
export PATH="/usr/lib/llvm-17/bin:$PATH"

# ... or force a backend per invocation
polyglot --compiler clang build hello.c
```

### `permission denied: polyglot`

```bash
chmod +x polyglot
```

### `ModuleNotFoundError: No module named 'toml'`

```bash
pip install toml
```

### Cross-compilation fails

Ensure the target toolchain is installed:
```bash
# For ARM64
sudo apt-get install gcc-aarch64-linux-gnu

# For RISC-V
sudo apt-get install gcc-riscv64-linux-gnu
```

---

## Next Steps

- [Quick Start Guide](quickstart.md) - Your first Polyglot project
- [Configuration](configuration.md) - Project setup with `polyglot.toml`
- [Commands Reference](../reference/cli.md) - All available commands