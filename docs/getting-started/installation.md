# Installation

## Prerequisites

| Requirement | Version | Notes |
|-------------|---------|-------|
| Python | 3.8+ | Required for the driver |
| toml (Python) | Latest | Installed automatically by pipx/pip; `pip install toml` when running from source |
| A C compiler | any recent version | gcc or clang — the driver auto-discovers whichever is available, including clang installs outside `PATH` |

That is all the Python driver needs: the standard library, the `toml` package, and any one C compiler. clang is **not** required.

## Quick Install (pipx)

The driver is a proper Python package, so the fastest route is pipx (isolated, on your `PATH`, stdlib + `toml` only):

```bash
pipx install git+https://github.com/RXVEN-1907/polyglot-compiler

# Verify
polyglot version
# polyglot 0.3.0
```

Without pipx, plain pip works the same way: `pip install git+https://github.com/RXVEN-1907/polyglot-compiler`.

## Alternative: install.sh (Linux/macOS)

The repo ships an installer that places the launcher plus the `polyglot_lib` package into a user prefix (default `~/.local/bin`, no superuser required):

```bash
curl -fsSL https://raw.githubusercontent.com/RXVEN-1907/polyglot-compiler/master/install.sh -o install.sh
./install.sh          # add --prefix DIR to choose a location, --uninstall to remove
```

It checks `python3 >= 3.8`, offers to install the `toml` package when missing, and looks for a C backend (clang or gcc; `POLYGLOT_CC` overrides).

## From Source

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
# polyglot 0.3.0
```

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

## Building the Rust Driver (Optional)

The Rust workspace (7 crates + 1 plugin) implements an in-process pipeline (parse → sema → lower → optimize → LLVM IR emission). Its **default build has zero system dependencies** — no system LLVM is needed.

```bash
# Build all workspace crates
cargo build --workspace

# Run the in-process Rust pipeline
cargo run -p polyglot-driver -- emit-ir examples/01_hello.c
```

> **Note**: The Python driver works independently and provides full C/C++ compilation functionality. The Rust components are a self-contained compiler lab: `--backend native` uses the system `llc`/`cc` only when real object code is wanted.

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
