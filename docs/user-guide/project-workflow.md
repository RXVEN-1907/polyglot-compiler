# Project Workflow

## Overview

Polyglot provides a complete project workflow from initialization to building and running.

---

## Project Lifecycle

```
┌─────────┐     ┌─────────┐     ┌─────────┐     ┌─────────┐     ┌─────────┐
│  init   │ ──▶ │  edit   │ ──▶ │  build  │ ──▶ │  test   │ ──▶ │  run    │
└─────────┘     └─────────┘     └─────────┘     └─────────┘     └─────────┘
```

---

## 1. Initialize Project

```bash
# C project
polyglot init my-app --language c

# C++ project
polyglot init my-app --language cpp

# Rust project (builds via rustc)
polyglot init my-app --language rust
```

Creates:
```
my-app/
├── polyglot.toml
├── README.md
├── .gitignore
└── src/
    └── main.c          # or main.cpp / main.rs
```

---

## 2. Project Structure

```
my-app/
├── polyglot.toml       # Configuration
├── src/                # Source files
│   ├── main.c
│   ├── utils.c
│   └── utils.h
├── include/            # Headers (optional)
│   └── myapp.h
├── tests/              # Test files (optional)
│   └── test_main.c
└── build/              # Build output (gitignored)
    └── my-app
```

---

## 3. Configuration

Edit `polyglot.toml`:

```toml
[project]
name = "my-app"
version = "1.0.0"
description = "My awesome application"
authors = ["Your Name <you@example.com>"]
license = "MIT"

[target]
triple = "x86_64-unknown-linux-gnu"
opt_level = 2

[plugins]
clang = "0.1"

[[bin]]
name = "my-app"
source = "src/main.c"
cflags = ["-Wall", "-Wextra", "-Wpedantic"]
ldflags = ["-lm"]

[[bin]]
name = "my-app-test"
source = "tests/test_main.c"
cflags = ["-DTEST_MODE", "-O0", "-g"]
```

> **Note**: in v0.2.0 the driver honors `[[bin]]` `name`/`source`, `[language]` and
> `[compiler]`; the other keys shown above (`cflags`, `ldflags`, `[target]`,
> `[plugins]`) are reserved and not yet applied at build time — pass those flags
> on the command line instead (see [Configuration](../reference/configuration.md)).

---

## 4. Development Workflow

### Building

```bash
# Build the project (reads polyglot.toml, builds the first [[bin]])
polyglot build

# Build with different optimization
polyglot --opt-level aggressive build

# Parallel build of multiple explicit sources
polyglot build -j 8 src/main.c src/util.c -o my-app
```

### Running

```bash
# Build and run a source file
polyglot run src/main.c

# Pass program arguments after --
polyglot run src/main.c -- --verbose --config config.toml
```

### Testing

```bash
# Build the test binary from its source
polyglot build tests/test_main.c -o test-runner
./test-runner
```

### Checking Syntax

```bash
# Check all source files
polyglot check src/*.c

# Check with target
polyglot check --target aarch64-unknown-linux-gnu src/*.c
```

### Inspecting Output

```bash
# See LLVM IR for main binary
polyglot emit-ir src/main.c -o main.ll

# See assembly
polyglot emit-asm src/main.c -o main.s

# Compilation statistics
polyglot stats src/main.c --opt-level aggressive
```

---

## 5. Multi-Binary Projects

Configure multiple binaries in `polyglot.toml`:

```toml
[[bin]]
name = "server"
source = "src/server.c"
cflags = ["-DSERVER_MODE"]

[[bin]]
name = "client"
source = "src/client.c"
cflags = ["-DCLIENT_MODE"]

[[bin]]
name = "shared-tool"
source = "src/tool.c"
```

Build the configured (first `[[bin]]`) target:
```bash
polyglot build
# Built src/server.c -> server
```

Build the others explicitly:
```bash
polyglot build src/client.c -o client
polyglot build src/tool.c -o shared-tool
```

---

## 6. Clean and Rebuild

```bash
# Clean all artifacts
polyglot clean

# Clean and rebuild
polyglot clean && polyglot build
```

---

## 7. CI/CD Integration

### GitHub Actions

```yaml
# .github/workflows/build.yml
name: Build

on: [push, pull_request]

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Setup
        run: |
          sudo apt-get install -y gcc   # or clang; polyglot discovers either
          pip install toml
      - name: Build
        run: |
          chmod +x polyglot
          polyglot build
      - name: Test
        run: |
          ./my-app
```

### GitLab CI

```yaml
# .gitlab-ci.yml
stages:
  - build
  - test

build:
  stage: build
  image: ubuntu:22.04
  before_script:
    - apt-get update && apt-get install -y gcc python3-pip
    - pip install toml
  script:
    - chmod +x polyglot
    - polyglot build

test:
  stage: test
  script:
    - ./my-app
```

---

## 8. Distribution

### Release Binaries

```bash
# Build release version
polyglot --opt-level aggressive build

# Strip symbols (smaller)
strip my-app

# Package
tar -czf my-app-linux-x86_64.tar.gz my-app
```

### Cross-Platform Release

```bash
#!/bin/bash
# release.sh -- one cross GCC per target (see Cross-Compilation Guide)

TARGETS=(
    "x86_64-unknown-linux-gnu gcc"
    "aarch64-unknown-linux-gnu aarch64-linux-gnu-gcc"
)

for entry in "${TARGETS[@]}"; do
    target="${entry%% *}"
    cross_cc="${entry##* }"
    POLYGLOT_CC="/usr/bin/$cross_cc" polyglot --opt-level aggressive build src/main.c -o my-app
    strip my-app
    tar -czf "my-app-${target//\//-}.tar.gz" my-app
done
```

---

## Best Practices

### 1. Use `.gitignore`

```
# Build artifacts
build/
*.o
*.exe
a.out

# Polyglot
.polyglot-cache/
```

### 2. Pin the Compiler

In `polyglot.toml`:
```toml
[compiler]
cc = "/usr/bin/clang-17"
cxx = "/usr/bin/clang++-17"
```

`POLYGLOT_CC` / `POLYGLOT_CXX` environment variables override the config file.

### 3. Separate Debug/Release Configs

```bash
# Debug build (flags are CLI options in v0.2.0)
polyglot build --debug --sanitize address src/main.c -o my-app-debug

# Release build
polyglot --opt-level aggressive build src/main.c -o my-app
```

### 4. Use Environment Variables for CI

```bash
# Only POLYGLOT_CC / POLYGLOT_CXX are read in v0.2.0
export POLYGLOT_CC="/usr/bin/aarch64-linux-gnu-gcc"

polyglot doctor --strict   # preflight the toolchain
polyglot build             # uses the pinned compiler
```

---

## Troubleshooting

### `polyglot.toml not found`

Run from project root or specify path:
```bash
polyglot build /path/to/project
```

### Build fails with "source file not found"

Check `polyglot.toml` paths are relative to project root:
```toml
[[bin]]
source = "src/main.c"  # Relative to polyglot.toml
```

### Multiple binaries with same name

Each `[[bin]]` must have unique `name`:
```toml
[[bin]]
name = "server"    # Unique
source = "src/server.c"

[[bin]]
name = "client"    # Unique
source = "src/client.c"
```

---

## Next Steps

- [Commands Reference](../reference/cli.md) - All commands
- [Configuration](../getting-started/configuration.md) - Full config reference
- [Examples](examples.md) - Complete project examples