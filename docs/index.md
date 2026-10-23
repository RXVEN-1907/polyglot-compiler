# Polyglot Compiler

**A universal compiler driver for C and C++ with real compiler inspection capabilities**

---

## 🎯 What is Polyglot?

Polyglot is a **compiler driver** that provides a unified, developer-friendly interface for compiling C and C++ (and Rust) programs. It wraps whichever toolchain it finds (GCC or clang, on or off `PATH`) and delivers a consistent CLI with inspection capabilities. You can see what the compiler is doing at every stage.

### Why Polyglot?

| Problem | Polyglot Solution |
|---------|-------------------|
| Different flags for every compiler | One consistent CLI |
| No unified IR/assembly inspection | `polyglot emit-ir` / `emit-asm` |
| Project config scattered everywhere | Simple `polyglot.toml` |
| Cross-compilation is complex | Named targets: `polyglot --target aarch64-unknown-linux-gnu` |
| Compiler internals are opaque | `polyglot stats` shows time reports |

---

## ✨ Key Features

- 🔨 **Multi-language**: C, C++, and Rust projects, with GCC/clang toolchain auto-discovery (works with clang installed off `PATH`)
- ⚡ **6 Optimization Levels**: `none`, `less`, `default`, `aggressive`, `size`, `size-aggressive`
- 🎯 **5 Named Targets**: x86_64, ARM64, ARM32, RISC-V, WASM. `polyglot targets` shows availability
- 🔍 **Syntax Check & Static Analysis**: `check`, plus clang's `--analyze` via `analyze`
- 📄 **IR Emission**: LLVM IR (clang) or GIMPLE (gcc) with `emit-ir`
- ⚙️ **Assembly & Preprocessor**: `emit-asm --intel`, `preproc`, `deps`
- 📊 **Statistics**: compile time reports and binary size, with `stats --json` for CI
- 🧰 **LLVM Tooling Built In**: `fmt` (clang-format), `tidy` (clang-tidy), `mca` (llvm-mca), `jit` (lli), `repl` (clang-repl)
- 🏗️ **Project Management**: `init`, `build`, `run`, `watch`, incremental build cache, `-j` parallel builds
- 🩺 **Diagnostics**: `polyglot doctor` reports the whole toolchain environment

---

## 🚀 Quick Example

```bash
# Build and run in one command
polyglot run hello.c

# Check the environment first
polyglot doctor

# See what the compiler produces
polyglot emit-ir hello.c
polyglot emit-asm hello.c --intel -o hello.s

# Get compilation statistics
polyglot stats hello.c

# Create a new project
polyglot init my-app --language c
cd my-app && polyglot build && ./my-app
```

---

## 📦 Installation

```bash
# Install with pipx (stdlib + toml only)
pipx install git+https://github.com/RXVEN-1907/polyglot-compiler

# ... or clone and run from source
git clone https://github.com/RXVEN-1907/polyglot-compiler
cd polyglot-compiler
pip install toml
chmod +x polyglot

# Verify
polyglot version
# Output: polyglot 0.3.0
```

[Get Started →](getting-started/installation.md)

---

## 🏗️ Architecture

Polyglot uses a **dual-layer architecture**:

```
┌─────────────────────────────────────────────────────────────┐
│                    PYTHON DRIVER (User-facing)               │
│  CLI • Config • Build Orchestration • Diagnostics           │
└────────────────────────────┬────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────┐
│                 GCC / CLANG BACKEND (System)                 │
│  Frontend → Optimizations → CodeGen → Link                  │
└─────────────────────────────────────────────────────────────┘
```

**Advanced**: a 7-crate Rust workspace (+1 reference plugin) implements an in-process pipeline (parse → sema → lower → optimize → LLVM IR emission) with a clap CLI driver. The experimental MLIR/Cranelift paths were removed in 0.3.0; the active path is LLVM IR emission, and the default Rust build needs no system LLVM.

[Architecture Overview →](architecture/overview.md)

---

## 📚 Documentation

| Section | Description |
|---------|-------------|
| [Getting Started](getting-started/installation.md) | Installation and first steps |
| [User Guide](user-guide/commands.md) | All commands and workflows |
| [Reference](reference/cli.md) | Complete CLI reference |
| [Architecture](architecture/overview.md) | System design and internals |
| [Development](https://github.com/RXVEN-1907/polyglot-compiler/blob/master/CONTRIBUTING.md) | Contributing guidelines |

---

## 🌟 Project Status

| Component | Status |
|-----------|--------|
| Python Driver | ✅ Working |
| C/C++ Compilation | ✅ Full support |
| Optimization Levels | ✅ All 6 levels |
| Cross-compilation | ✅ 5 targets |
| IR/ASM Emission | ✅ Working |
| Statistics | ✅ Working |
| Project Config | ✅ Working |
| Rust Workspace | ✅ 7 crates + 1 plugin build clean |
| LLVM IR Emission | ✅ Working (Rust pipeline) |
| Plugin System | ✅ Working (`polyglot-clang`) |

---

## 🤝 Contributing

Contributions are welcome. See the [Contributing Guide](https://github.com/RXVEN-1907/polyglot-compiler/blob/master/CONTRIBUTING.md).

- 🐛 [Report Bug](https://github.com/RXVEN-1907/polyglot-compiler/issues/new?template=bug_report.md)
- ✨ [Request Feature](https://github.com/RXVEN-1907/polyglot-compiler/issues/new?template=feature_request.md)
- 💬 [Discussions](https://github.com/RXVEN-1907/polyglot-compiler/discussions)

---

## 📄 License

MIT License - see [LICENSE](https://github.com/RXVEN-1907/polyglot-compiler/blob/master/LICENSE) for details.

---

## 🙏 Acknowledgments

- **LLVM Team** - Compiler infrastructure
- **Clang Team** - C/C++ frontend
- **Rust Community** - Ecosystem and tooling