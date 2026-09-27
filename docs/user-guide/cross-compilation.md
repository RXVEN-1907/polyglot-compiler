# Cross-Compilation Guide

## Overview

Pass the global `--target` option with a target triple and polyglot routes it to the backend. The backend matters:

- **clang** passes `--target=<triple>` straight to clang. Compiling for a foreign target also needs that target's sysroot and libc for anything that includes system headers or links.
- **gcc** can only build for the host triple. A non-host target aborts with exit code 1 and a hint to install the matching cross GCC (e.g. `aarch64-linux-gnu-gcc`) or use `--compiler clang`.

Run `polyglot targets` first — it lists every known triple with its status for your current backend (`native`, `needs-cross-gcc`, `unavailable`).

## Supported Targets

| Target | Architecture | OS | ABI | Status on gcc | Status on clang |
|--------|--------------|----|-----|---------------|-----------------|
| `x86_64-unknown-linux-gnu` | x86_64 | Linux | GNU | Native (x86_64 hosts) | Native / cross |
| `aarch64-unknown-linux-gnu` | ARM64 | Linux | GNU | needs cross gcc | needs sysroot to link |
| `arm-unknown-linux-gnueabihf` | ARM32 | Linux | GNU hard-float | needs cross gcc | needs sysroot to link |
| `riscv64-unknown-linux-gnu` | RISC-V 64 | Linux | GNU | needs cross gcc | needs sysroot to link |
| `wasm32-wasip1` | WebAssembly | WASI p1 | - | unsupported | needs `wasm-ld`/lld |

There are no short aliases: pass the full triple.

## Two Working Strategies

### Strategy 1: cross GCC via `POLYGLOT_CC`

Install the cross toolchain and point polyglot's compiler discovery at it — full compile *and* link in one step:

```bash
sudo apt-get install gcc-aarch64-linux-gnu libc6-dev-arm64-cross

POLYGLOT_CC=/usr/bin/aarch64-linux-gnu-gcc polyglot build hello.c -o hello-arm64
file hello-arm64
# hello-arm64: ELF 64-bit LSB executable, ARM aarch64, ...
```

### Strategy 2: clang with `--target`

```bash
polyglot --compiler clang --target aarch64-unknown-linux-gnu build hello.c -o hello-arm64
```

Without an aarch64 sysroot this fails at the first system header:

```text
/usr/include/stdio.h:28:10: fatal error: 'bits/libc-header-start.h' file not found
error: compilation failed with exit code 1
```

Install the sysroot (the cross GCC packages above provide it) and point clang at it:

```bash
polyglot --compiler clang --target aarch64-unknown-linux-gnu \
    --cc-arg=--sysroot=/usr/aarch64-linux-gnu build hello.c -o hello-arm64
```

For header-only inspection, `-S`-style commands work without a sysroot once the headers are available; linking additionally needs the target's libraries.

## Sysroot and Libraries

```bash
# Specify a sysroot (repeat --cc-arg for more flags)
polyglot --compiler clang --target aarch64-unknown-linux-gnu build \
    --cc-arg=--sysroot=/usr/aarch64-linux-gnu \
    hello.c -o hello-arm64

# Library search path and libraries use the regular -L/-l flags
polyglot build -L/usr/aarch64-linux-gnu/lib -lmylib hello.c -o hello-arm64
```

## Target Details

### ARM64 Linux (`aarch64-unknown-linux-gnu`)

```bash
sudo apt-get install gcc-aarch64-linux-gnu libc6-dev-arm64-cross
POLYGLOT_CC=/usr/bin/aarch64-linux-gnu-gcc polyglot build hello.cpp -o hello-arm64
```

For C++ sources set the C++ compiler too — discovery searches `clang++`/`g++` independently of `POLYGLOT_CC`:

```bash
POLYGLOT_CC=/usr/bin/aarch64-linux-gnu-gcc \
POLYGLOT_CXX=/usr/bin/aarch64-linux-gnu-g++ polyglot build hello.cpp -o hello-arm64
```

### ARM32 Linux (`arm-unknown-linux-gnueabihf`)

```bash
sudo apt-get install gcc-arm-linux-gnueabihf libc6-dev-armhf-cross
POLYGLOT_CC=/usr/bin/arm-linux-gnueabihf-gcc polyglot build hello.c -o hello-arm32
```

Hard-float ABI only.

### RISC-V 64 Linux (`riscv64-unknown-linux-gnu`)

```bash
sudo apt-get install gcc-riscv64-linux-gnu libc6-dev-riscv64-cross
POLYGLOT_CC=/usr/bin/riscv64-linux-gnu-gcc polyglot build hello.c -o hello-riscv64
```

### WebAssembly (`wasm32-wasip1`)

Reported `unavailable` by `polyglot targets` until an LLVM with `wasm-ld`/lld is discoverable (a full LLVM install provides it). Linking against WASI libc needs a wasi-sysroot:

```bash
polyglot --compiler clang --target wasm32-wasip1 \
    --cc-arg=--sysroot=/opt/wasi-sysroot build hello.c -o hello.wasm
```

## Verifying Cross-Compiled Binaries

```bash
file hello-arm64
# hello-arm64: ELF 64-bit LSB executable, ARM aarch64, ...

readelf -d hello-arm64 | grep NEEDED
```

## Common Issues

| Symptom | Fix |
|---------|-----|
| `GCC backend cannot target '...'` (exit 1) | Install the cross GCC and set `POLYGLOT_CC`, or use `--compiler clang` |
| `fatal error: 'bits/libc-header-start.h' file not found` | Install the cross libc-dev package or pass `--cc-arg=--sysroot=...` |
| Linker errors with `-lm` / `-lpthread` | Install the cross libraries (`libc6-dev-arm64-cross`, `libstdc++6-arm64-cross`) |
| WASM `imported function not found` | Link against a wasi-sysroot (`--cc-arg=--sysroot=/opt/wasi-sysroot`) |

## CI/CD Cross-Compilation

```yaml
# .github/workflows/cross-compile.yml
name: Cross Compile

on: [push, pull_request]

jobs:
  cross-compile:
    runs-on: ubuntu-latest
    strategy:
      matrix:
        include:
          - target: aarch64-unknown-linux-gnu
            cross_cc: aarch64-linux-gnu-gcc
            pkgs: gcc-aarch64-linux-gnu libc6-dev-arm64-cross
          - target: arm-unknown-linux-gnueabihf
            cross_cc: arm-linux-gnueabihf-gcc
            pkgs: gcc-arm-linux-gnueabihf libc6-dev-armhf-cross
    steps:
      - uses: actions/checkout@v4
      - name: Install cross toolchains
        run: |
          sudo apt-get update
          sudo apt-get install -y ${{ matrix.pkgs }}
      - name: Build for ${{ matrix.target }}
        run: |
          chmod +x polyglot
          POLYGLOT_CC=/usr/bin/${{ matrix.cross_cc }} polyglot build hello.c -o hello-${{ matrix.target }}
      - uses: actions/upload-artifact@v4
        with:
          name: hello-${{ matrix.target }}
          path: hello-${{ matrix.target }}
```

## Next Steps

- [Targets Reference](../reference/targets.md) — triple list and the `polyglot targets` command
- [Optimization](optimization.md) — target-specific optimization
- [Tool Integrations](tools.md) — compiler tooling around the build
