# Contributing to Polyglot Compiler

Thank you for your interest in contributing to Polyglot! This document provides guidelines and information for contributors.

## 📋 Table of Contents

- [Code of Conduct](#code-of-conduct)
- [How Can I Contribute?](#how-can-i-contribute)
- [Development Setup](#development-setup)
- [Coding Standards](#coding-standards)
- [Testing](#testing)
- [Pull Request Process](#pull-request-process)
- [Issue Reporting](#issue-reporting)
- [Architecture Overview](#architecture-overview)

---

## 📜 Code of Conduct

This project adheres to a [Code of Conduct](CODE_OF_CONDUCT.md). By participating, you are expected to uphold this code. Please report unacceptable behavior to the project maintainers.

---

## 🤝 How Can I Contribute?

### Types of Contributions

1. **Bug Fixes** - Fix issues in the Python driver or Rust crates
2. **Feature Development** - Implement new commands, optimization passes, or targets
3. **Documentation** - Improve README, add examples, write tutorials
4. **Testing** - Add test cases, improve test coverage
5. **Performance** - Optimize compilation speed, reduce binary size
6. **Infrastructure** - CI/CD improvements, dependency updates

### Good First Issues

Look for issues labeled [`good first issue`](https://github.com/RXVEN-1907/polyglot-compiler/labels/good%20first%20issue) or [`help wanted`](https://github.com/RXVEN-1907/polyglot-compiler/labels/help%20wanted).

---

## 🛠️ Development Setup

### Prerequisites

```bash
# System dependencies (Ubuntu/Debian)
sudo apt-get update && sudo apt-get install -y \
    python3 python3-pip \
    clang-17 llvm-17 lld-17 \
    libzstd-dev libtinfo-dev libxml2-dev \
    cargo rustc  # For Rust development

# Python dependencies
pip install toml pytest

# Verify Clang installation
clang --version
# Should show 17.x or later
```

### Clone and Setup

```bash
# Fork and clone your fork
git clone https://github.com/YOUR_USERNAME/polyglot-compiler
cd polyglot-compiler

# Add upstream remote
git remote add upstream https://github.com/RXVEN-1907/polyglot-compiler

# Make driver executable
chmod +x polyglot

# Run tests to verify setup
python3 test_comprehensive.py
```

### Running Tests

```bash
# Python integration tests
python3 test_comprehensive.py

# Rust library tests
cargo test --lib --workspace

# Run benchmarks
python3 benchmark.py

# Run specific test
python3 -m pytest test_comprehensive.py::ComprehensiveTest::test_build_c -v
```

### Building Rust Components

```bash
# Build all library crates
cargo build --lib --workspace

# Build with all features
cargo build --lib --workspace --all-features

# Build binaries (requires static libraries)
cargo build --release --bin polyglot-driver --bin polyglot-clang

# Check for warnings
cargo clippy --workspace --all-targets -- -D warnings

# Format code
cargo fmt --all
```

---

## 📝 Coding Standards

### Python (Driver)

- **Style**: Follow [PEP 8](https://pep8.org/) with [Black](https://black.readthedocs.io/) formatting
- **Type Hints**: Use type hints for all function signatures
- **Docstrings**: Google-style docstrings for public functions
- **Error Handling**: Use explicit error messages, exit with appropriate codes

```python
# Example function signature
def build(self, args: argparse.Namespace) -> None:
    """Build source file(s) or project.
    
    Args:
        args: Parsed command-line arguments containing paths, output, target, opt_level.
        
    Raises:
        SystemExit: If compilation fails or source files not found.
    """
```

### Rust (Crates)

- **Edition**: Rust 2021
- **Style**: `rustfmt` with default settings
- **Linting**: `clippy` with `-D warnings`
- **Documentation**: `///` doc comments for all public items
- **Error Handling**: Use `thiserror` for error types, `anyhow` for application errors

```rust
// Example function signature
/// Build the project with the given configuration.
/// 
/// # Arguments
/// * `config` - Project configuration from polyglot.toml
/// * `target` - Optional target triple override
/// 
/// # Returns
/// * `Result<BuildOutput, DriverError>` - Build artifacts or error
pub fn build(config: &ProjectConfig, target: Option<TargetTriple>) -> Result<BuildOutput, DriverError> {
    // Implementation
}
```

### Commit Messages

Follow [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <description>

[optional body]

[optional footer]
```

**Types:**
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation only
- `style`: Formatting, missing semicolons, etc.
- `refactor`: Code change that neither fixes a bug nor adds a feature
- `perf`: Performance improvement
- `test`: Adding missing tests
- `chore`: Maintenance tasks

**Examples:**
```
feat(driver): add --emit-obj flag for object file output
fix(mlir): resolve memory leak in context creation
docs(readme): update cross-compilation examples
perf(cache): optimize content-addressable storage lookup
```

---

## ✅ Testing

### Test Requirements

- All new features must include tests
- Bug fixes must include regression tests
- Tests should be deterministic and fast

### Test Categories

| Category | Location | Command |
|----------|----------|---------|
| Python Integration | `test_comprehensive.py` | `python3 test_comprehensive.py` |
| Rust Unit Tests | `crates/*/src/*.rs` | `cargo test --lib --workspace` |
| Rust Integration | `crates/*/tests/*.rs` | `cargo test --workspace` |
| Benchmarks | `benchmark.py` | `python3 benchmark.py` |

### Writing Tests

**Python Test Example:**
```python
def test_new_feature(self):
    """Test the new feature works correctly."""
    result = self.run(["new-command", "input.c", "-o", "output"])
    assert result.returncode == 0
    assert Path("output").exists()
    # Verify output behavior
    run_result = subprocess.run(["./output"], capture_output=True, text=True)
    assert "expected output" in run_result.stdout
```

**Rust Test Example:**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_optimization_pass() {
        let ctx = Context::new();
        let module = ctx.parse_module(r#"..."#).unwrap();
        let result = optimize(&module, OptimizationLevel::Default);
        assert!(result.is_ok());
    }
}
```

---

## 🔄 Pull Request Process

### Before Submitting

1. **Sync with upstream**:
   ```bash
   git fetch upstream
   git rebase upstream/main
   ```

2. **Run all tests**:
   ```bash
   python3 test_comprehensive.py
   cargo test --lib --workspace
   cargo clippy --workspace --all-targets -- -D warnings
   cargo fmt --all --check
   ```

3. **Update documentation** if needed

### PR Template

When creating a PR, please fill out the template:

- **Description**: What does this PR do?
- **Type**: Bug fix / Feature / Docs / Refactor / Performance
- **Testing**: How was this tested?
- **Breaking Changes**: Any breaking changes?
- **Related Issues**: Link to related issues

### Review Process

1. **Automated Checks**: CI must pass (tests, linting, formatting)
2. **Code Review**: At least one maintainer review required
3. **Merge**: Squash and merge after approval

### PR Checklist

- [ ] Tests pass locally
- [ ] Code follows style guidelines
- [ ] Documentation updated
- [ ] No breaking changes (or documented)
- [ ] Commit messages follow convention
- [ ] PR description is clear

---

## 🐛 Issue Reporting

### Bug Reports

Use the [Bug Report Template](.github/ISSUE_TEMPLATE/bug_report.md) and include:

- **Environment**: OS, Python version, Clang version, Rust version
- **Steps to Reproduce**: Minimal, complete example
- **Expected Behavior**: What should happen
- **Actual Behavior**: What actually happens
- **Logs/Output**: Relevant error messages

### Feature Requests

Use the [Feature Request Template](.github/ISSUE_TEMPLATE/feature_request.md) and include:

- **Problem**: What problem does this solve?
- **Solution**: Proposed approach
- **Alternatives**: Other approaches considered
- **Use Cases**: Who would benefit?

---

## 🏗️ Architecture Overview

### Python Driver (`polyglot`)

The main entry point. Handles:
- CLI parsing (`argparse`)
- Project configuration (`polyglot.toml`)
- Build orchestration (calls Clang)
- Diagnostics formatting

Key files:
- `polyglot` - Main executable (~460 lines)

### Rust Crates

| Crate | Responsibility | Key Types |
|-------|----------------|-----------|
| `polyglot-protocol` | JSON-RPC 2.0 communication | `Request`, `Response`, `Transport` |
| `polyglot-mlir` | MLIR C API bindings | `Context`, `Module`, `PassManager` |
| `polyglot-codegen` | Cranelift codegen | `CodegenBackend`, `TargetConfig` |
| `polyglot-driver` | Build orchestration | `BuildEngine`, `PluginManager` |
| `polyglot-cache` | Incremental compilation | `Cache`, `ContentAddressableStore` |
| `polyglot-registry` | Plugin registry client | `RegistryClient`, `PluginMetadata` |
| `polyglot-syntax` | Lexer/parser/AST | `Lexer`, `Parser`, `AST` |
| `polyglot-sema` | Semantic analysis | `Analyzer`, `TypeChecker` |
| `polyglot-core` | Core types | `Span`, `Diagnostic`, `Symbol` |
| `polyglot-ir` | MLIR lowering/optimization | `LoweringContext`, `Optimizer` |

### Plugin Architecture

Plugins communicate via **JSON-RPC 2.0 over stdio**:

```
Driver (Python)          Plugin (Rust binary)
    │                         │
    ├── Initialize ─────────▶ │
    ◀── Initialized ───────── │
    │                         │
    ├── Compile ────────────▶ │
    ◀── Compiled ──────────── │
    │                         │
    ├── Shutdown ───────────▶ │
```

---

## 📚 Resources

- [LLVM Documentation](https://llvm.org/docs/)
- [MLIR Reference](https://mlir.llvm.org/docs/)
- [Cranelift Documentation](https://github.com/bytecodealliance/wasmtime/tree/main/cranelift/docs)
- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- [Python Style Guide](https://pep8.org/)

---

## 💬 Getting Help

- **GitHub Discussions**: For questions and ideas
- **GitHub Issues**: For bugs and feature requests
- **Discord**: (Coming soon)

---

Thank you for contributing to Polyglot! 🎉