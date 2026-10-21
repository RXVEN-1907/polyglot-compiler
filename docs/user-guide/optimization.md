# Optimization Guide

## Optimization Levels

Polyglot provides 6 optimization levels, mapped to the same flags on both the GCC and clang backends:

| Level | Flag | Compiler flag | Best For |
|-------|------|-------|----------|
| `none` | `--opt-level none` | `-O0` | Debugging, fastest compile |
| `less` | `--opt-level less` | `-O1` | Quick builds, some optimization |
| `default` | `--opt-level default` | `-O2` | **Production releases** (recommended) |
| `aggressive` | `--opt-level aggressive` | `-O3` | Maximum performance |
| `size` | `--opt-level size` | `-Os` | Size-constrained environments |
| `size-aggressive` | `--opt-level size-aggressive` | `-Oz` | Extreme size optimization |

---

## When to Use Each Level

### `-O0` (none) - Debugging
```bash
polyglot --opt-level none build hello.c -o hello-debug
```
- No optimizations
- Best debug experience
- Variables not optimized away
- Fastest compilation

### `-O1` (less) - Quick Development
```bash
polyglot --opt-level less build hello.c -o hello-quick
```
- Basic optimizations
- Good balance for development
- Faster than `-O2`

### `-O2` (default) - **Production Standard**
```bash
polyglot build hello.c -o hello-release  # Uses -O2 by default
```
- Most optimizations enabled
- Good compile time / runtime balance
- Safe for all code
- **Recommended for releases**

### `-O3` (aggressive) - Maximum Speed
```bash
polyglot --opt-level aggressive build hello.c -o hello-fast
```
- All `-O2` optimizations plus:
  - Function inlining more aggressive
  - Loop unrolling
  - Vectorization attempts
- Longer compile time
- May increase binary size
- Use for compute-intensive code

### `-Os` (size) - Size Optimized
```bash
polyglot --opt-level size build hello.c -o hello-small
```
- All `-O2` optimizations that don't increase size
- Prefers smaller code over speed
- Good for embedded systems
- May be slower than `-O2`

### `-Oz` (size-aggressive) - Minimum Size
```bash
polyglot --opt-level size-aggressive build hello.c -o hello-tiny
```
- Aggressive size reduction
- May sacrifice significant performance
- For extremely constrained environments

!!! note
    `-Oz` requires GCC 12 or newer, or any recent clang. On the GCC 13 backend it is passed through verbatim (`-Oz`); on older GCC versions the compiler rejects the flag.

---

## Optimization Pipeline

The Clang/LLVM optimization pipeline at each level:

### `-O0` (none)
```
Frontend → No optimizations → CodeGen
```

### `-O1` (less)
```
Frontend → 
  EarlyCSE → 
  SimplifyCFG → 
  SROA → 
  EarlyCSE → 
  Inline (tiny) → 
  CodeGen
```

### `-O2` (default) - **Full Pipeline**
```
Frontend → 
  Target-independent passes:
    - Canonicalization
    - CSE (Common Subexpression Elimination)
    - LICM (Loop Invariant Code Motion)
    - GVN (Global Value Numbering)
    - SCCP (Sparse Conditional Constant Propagation)
    - DCE (Dead Code Elimination)
    - Loop optimizations (unroll, vectorize)
    - Inlining
  CodeGen
```

### `-O3` (aggressive)
```
-O2 passes + 
  - More aggressive inlining
  - Loop unrolling (higher thresholds)
  - Vectorization (more aggressive)
  - Function attributes (hot/cold)
```

### `-Os` / `-Oz` (size)
```
-O2 passes + 
  - Optimize for size heuristics
  - Disable loop unrolling
  - Disable inlining (size threshold)
  - Optimize instruction selection for size
```

---

## Per-Project Optimization

The optimization level is a CLI-only option (`polyglot --opt-level ...`). The `opt_level` key that `polyglot init` writes into `polyglot.toml` under `[target]` is reserved but not yet honored. Per-file flags can be passed with `--cc-arg`:

```bash
polyglot build --cc-arg=-O3 --cc-arg=-march=native hot.c -o hot
```

---

## Benchmarking Your Code

Use `polyglot stats` to compare:

```bash
# Compare optimization levels
for level in none less default aggressive size size-aggressive; do
    echo "=== $level ==="
    polyglot stats --json --opt-level "$level" bench.c | jq '{opt_level, total_ms, binary_size_bytes}'
done
```

`stats` (human-readable) prints the compiler's time report per phase; `--json` gives `total_ms` and `binary_size_bytes` for scripting. See [Cache and Parallelism](cache-and-parallelism.md#json-output-for-ci).

---

## Target-Specific Optimization

### Native CPU Optimization

```bash
# Optimize for the build host's CPU
polyglot build --cc-arg=-march=native hello.c -o hello-native
```

### Vectorization Control

```bash
# Enable explicit vectorization
polyglot build --cc-arg=-fvectorize --cc-arg=-fslp-vectorize hello.c

# Disable vectorization
polyglot build --cc-arg=-fno-vectorize --cc-arg=-fno-slp-vectorize hello.c
```

### Link-Time Optimization (LTO)

```bash
# Pass LTO to both compile and link
polyglot build --cc-arg=-flto --cc-arg=-flto hello.c -o hello-lto
```

---

## Profile-Guided Optimization (PGO)

> **Note**: PGO support is planned for future releases.

```bash
# Phase 1: Instrument
polyglot build --cc-arg=-fprofile-generate hello.c -o hello-pgo

# Phase 2: Run training workload
./hello-pgo < training_input.txt

# Phase 3: Optimize with profile
polyglot build --cc-arg=-fprofile-use hello.c -o hello-optimized
```

---

## Common Optimization Flags

| Flag | Description |
|------|-------------|
| `-march=native` | Optimize for current CPU |
| `-mtune=native` | Tune for current CPU |
| `-ffast-math` | Aggressive FP math (may break IEEE) |
| `-flto` | Link-time optimization |
| `-fomit-frame-pointer` | Omit frame pointer (smaller, harder debug) |
| `-funroll-loops` | Unroll loops |
| `-fno-unroll-loops` | Disable loop unrolling |
| `-fvectorize` | Enable vectorization |
| `-fslp-vectorize` | Enable SLP vectorization |

---

## Troubleshooting

### Binary Not Faster at `-O3`

- Profile first: `polyglot stats` to see where time is spent
- Check for memory-bound code (optimization won't help)
- Verify hot paths are actually being optimized

### Debugging Optimized Code

```bash
# Build with debug info + optimization
polyglot build --debug hello.c -o hello-debug   # --debug adds -g

# Use GDB
gdb ./hello-debug
```

### Size Not Decreasing at `-Os`

- Strip symbols: `strip hello-small`
- Check for static linking: `ldd hello-small`
- Use `-ffunction-sections -fdata-sections` + `-Wl,--gc-sections`

---

## Next Steps

- [Cross Compilation](cross-compilation.md) - Optimize for specific targets
- [Examples](examples.md) - See optimization in action
- [Statistics](../reference/cli.md#stats) - Detailed stats command