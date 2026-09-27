# Target Triples Reference

## The `polyglot targets` Command

`polyglot targets` lists the known triples and their availability for the discovered backend:

```console
$ polyglot targets
TARGET TRIPLE                      STATUS           NOTES
------------------------------------------------------------------------------------------------
x86_64-unknown-linux-gnu           native           this host's architecture
aarch64-unknown-linux-gnu          needs-cross-gcc  GCC requires 'aarch64-linux-gnu-gcc' (apt install gcc-aarch64-linux-gnu)
arm-unknown-linux-gnueabihf        needs-cross-gcc  GCC requires 'arm-linux-gnu-gcc' (apt install gcc-arm-linux-gnu)
riscv64-unknown-linux-gnu          needs-cross-gcc  GCC requires 'riscv64-linux-gnu-gcc' (apt install gcc-riscv64-linux-gnu)
wasm32-wasip1                      unavailable      wasm linking requires wasm-ld/lld
```

Statuses:

- `native` — this triple matches the host; the discovered backend handles it directly.
- `needs-cross-gcc` — the triple is recognized, but a matching cross toolchain must be installed (see the GCC limitation below).
- `unavailable` — recognized, but the linker/tools it needs are missing (wasm needs `wasm-ld`/lld).

## Supported Targets

| Target Triple | Architecture | OS | ABI | Notes |
|---------------|--------------|----|-----|-------|
| `x86_64-unknown-linux-gnu` | x86_64 | Linux | GNU | Native on x86_64 Linux hosts |
| `aarch64-unknown-linux-gnu` | ARM64 | Linux | GNU | Cross |
| `arm-unknown-linux-gnueabihf` | ARM32 | Linux | GNU hard-float | Cross |
| `riscv64-unknown-linux-gnu` | RISC-V 64 | Linux | GNU | Cross |
| `wasm32-wasip1` | WebAssembly 32 | WASI preview 1 | - | Requires `wasm-ld`/lld |

These are the exact strings the driver knows; pass them with the global `--target` option:

```bash
polyglot --target aarch64-unknown-linux-gnu build hello.c -o hello-arm64
```

The driver does not expand short aliases into full triples. On the gcc backend a non-host string such as `--target arm64` is rejected (see the limitation below); on the clang backend the string is passed through verbatim (`--target=arm64`), and clang itself accepts some short names.

## GCC Cross-Compilation Limitation

The GCC backend can only produce code for the host triple. GCC would need a separately installed, target-specific cross compiler (for example `aarch64-linux-gnu-gcc` from `apt install gcc-aarch64-linux-gnu`). When you ask the GCC backend for a non-host target, the build stops with exit code 1:

```text
error: GCC backend cannot target 'aarch64-unknown-linux-gnu': GCC needs a matching cross
toolchain (e.g. aarch64-linux-gnu-gcc).
Hint: use clang for cross-compilation (--compiler clang).
```

With the clang backend, `--target` is passed through as `--target=<triple>` to clang itself. Compiling and linking for a foreign target additionally requires that target's sysroot and libc:

```console
$ polyglot --compiler clang --target aarch64-unknown-linux-gnu build hello.c -o hello-arm64
/usr/include/stdio.h:28:10: fatal error: 'bits/libc-header-start.h' file not found
error: compilation failed with exit code 1
```

Install an aarch64 sysroot (e.g. `apt install gcc-aarch64-linux-gnu` for the headers/libs) or point clang at one with `--cc-arg --sysroot=...`. If a matching cross GCC *is* installed (e.g. `aarch64-linux-gnu-gcc`), the driver can discover it via `POLYGLOT_CC=/usr/bin/aarch64-linux-gnu-gcc`.

## Target Details

### `x86_64-unknown-linux-gnu`

**Native target on x86_64 Linux.**

```bash
polyglot build hello.c -o hello
polyglot --target x86_64-unknown-linux-gnu build hello.c -o hello
```

**CPU features:** SSE2 baseline; AVX/AVX2/AVX-512 depending on the CPU.

### `aarch64-unknown-linux-gnu`

**ARM64 Linux (ARM servers, Raspberry Pi 5, Apple Silicon under Linux).**

```bash
# Cross toolchain + sysroot
sudo apt-get install gcc-aarch64-linux-gnu libc6-dev-arm64-cross

# Then either force clang (passes --target to clang) ...
polyglot --compiler clang --target aarch64-unknown-linux-gnu \
    --cc-arg --sysroot=/usr/aarch64-linux-gnu build hello.c -o hello-arm64

# ... or point the driver at the cross GCC directly
POLYGLOT_CC=/usr/bin/aarch64-linux-gnu-gcc polyglot build hello.c -o hello-arm64
```

**CPU features:** NEON; SVE on supported cores.

### `arm-unknown-linux-gnueabihf`

**ARM32 Linux with hardware floating point (Raspberry Pi 3, older ARM boards).**

```bash
sudo apt-get install gcc-arm-linux-gnueabihf libc6-dev-armhf-cross
POLYGLOT_CC=/usr/bin/arm-linux-gnueabihf-gcc polyglot build hello.c -o hello-arm32
```

**Note:** hard-float ABI only. Softfloat (`arm-unknown-linux-gnueabi`) is not a known target.

### `riscv64-unknown-linux-gnu`

**RISC-V 64-bit Linux (SiFive, StarFive boards).**

```bash
sudo apt-get install gcc-riscv64-linux-gnu libc6-dev-riscv64-cross
POLYGLOT_CC=/usr/bin/riscv64-linux-gnu-gcc polyglot build hello.c -o hello-riscv64
```

**CPU features:** RV64GC (M, A, F, D, C extensions).

### `wasm32-wasip1`

**WebAssembly with WASI preview 1.** Reported `unavailable` until `wasm-ld`/lld is on the PATH or in a scan directory — a full LLVM install provides it.

```bash
polyglot --compiler clang --target wasm32-wasip1 build hello.c -o hello.wasm
wasmtime hello.wasm
```

## Toolchain Requirements Summary

| Target | Ubuntu/Debian packages |
|--------|------------------------|
| `x86_64-unknown-linux-gnu` | any native gcc or clang |
| `aarch64-unknown-linux-gnu` | `gcc-aarch64-linux-gnu`, `libc6-dev-arm64-cross` (or clang + sysroot) |
| `arm-unknown-linux-gnueabihf` | `gcc-arm-linux-gnueabihf`, `libc6-dev-armhf-cross` (or clang + sysroot) |
| `riscv64-unknown-linux-gnu` | `gcc-riscv64-linux-gnu`, `libc6-dev-riscv64-cross` (or clang + sysroot) |
| `wasm32-wasip1` | LLVM with `wasm-ld`/lld; wasi-sysroot for libc |

## Verification Commands

```bash
file hello-arm64                          # check binary architecture
readelf -h hello-arm64                    # ELF header details
readelf -d hello-arm64 | grep NEEDED      # dynamic dependencies
```

## Common Issues

| Issue | Solution |
|-------|----------|
| `GCC backend cannot target '...'` | Install the cross GCC and use `POLYGLOT_CC`, or use `--compiler clang` |
| `bits/libc-header-start.h not found` under clang cross | Install the target sysroot (`libc6-dev-arm64-cross` etc.) or pass `--cc-arg --sysroot=...` |
| `wasm32-wasip1` reported `unavailable` | Install an LLVM with `wasm-ld`/lld |
| `stdio.h not found` | Install the cross libc-dev package |

## Next Steps

- [Cross Compilation Guide](../user-guide/cross-compilation.md)
- [CLI Reference](cli.md)
- [Tool Integrations](../user-guide/tools.md)
