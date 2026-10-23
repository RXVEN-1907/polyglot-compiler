# Cache and Parallelism

## The Incremental Build Cache

Every `build` (and every `run`) first compiles to an object file. Those objects are cached in `.polyglot-cache/` inside your project directory, keyed so a rebuild is skipped whenever nothing relevant changed.

### What the cache key covers

Each object is stored as `.polyglot-cache/<sha256>.o` where the SHA-256 key is computed over a payload containing:

- the compiler path and version (switching gcc ↔ clang, or upgrading, invalidates),
- the exact flag list (e.g. `-std=c17 -O2`; changing `--opt-level` or `--std` produces a different key),
- the source file name and its bytes (hashed directly for files under 4 MiB, or hashed-of-hash for larger ones),
- the resolved project directory.

The companion `manifest.json` records, per key, the creation timestamp, the flags, and the source name:

```json
{
  "901e3fdcec11...": {
    "created": "2026-09-27T16:02:00",
    "flags": ["-std=c17", "-O2"],
    "source": "src/main.c"
  }
}
```

New objects are moved into place with an atomic rename, so an interrupted build never leaves a half-written object behind.

### Seeing it work

`--verbose` prints cache decisions:

```console
$ polyglot --verbose build src/main.c -o hello
cache MISS src/main.c
Running: /bin/gcc -std=c17 -O2 -c src/main.c -o .polyglot-cache/901e3f....tmp.o
...
$ polyglot --verbose build src/main.c -o hello
cache HIT  src/main.c
Running: /bin/gcc -std=c17 -O2 .polyglot-cache/901e3f....o -o hello
Built src/main.c -> hello
```

### Managing the cache

```bash
polyglot clean             # removes artifacts AND clears .polyglot-cache/
polyglot build --no-cache  # one-off full recompile, cache untouched
```

`clean` also sweeps stray `.polyglot-cache/*.tmp` files. `doctor` reports where the cache lives and how big it is: `build cache /path/.polyglot-cache (4 object(s), 6.0 KiB)`. The cache is safe to delete at any time; the next build simply repopulates it.

## Parallel Builds

`build -j N` compiles independent translation units in parallel (per-TU), then links once:

```bash
polyglot build -j 8 main.c util.c math.c -o myapp
```

Multiple `-I`/`-D`/`--cc-arg` options are passed to every TU. Linking always happens serially after all objects exist.

## JSON Output for CI

`polyglot stats --json` emits machine-readable compilation statistics, one stable document per invocation, suitable for parsing in CI or scripts:

```json
{
  "source": ["src/main.c"],
  "size_bytes": 87,
  "lines": 6,
  "target": "native",
  "opt_level": "default",
  "backend": "gcc",
  "compiler_version": "gcc (Ubuntu 13.3.0-6ubuntu2~24.04.1) 13.3.0",
  "total_ms": 58,
  "binary_size_bytes": 2038,
  "timings": [
    { "phase": "phase setup", "seconds": 0.0 },
    { "phase": "phase parsing", "seconds": 0.0 },
    { "phase": "phase opt and generate", "seconds": 0.01 }
  ]
}
```

The `timings` array carries the compiler's own time report (GCC `-ftime-report` phases or the clang equivalent), so you can trend parse/opt/codegen time per phase. `binary_size_bytes` reflects the compiled object/binary for the given `--opt-level`, which makes size-regression checks a `jq` away:

```bash
polyglot stats --json --opt-level size src/main.c | jq .binary_size_bytes
```

For a CI preflight of the toolchain itself, pair it with `polyglot doctor --strict` (exit `1` when a required tool is missing).

## Next Steps

- [Commands Reference](commands.md): `build`, `stats`, `clean` flags
- [Tool Integrations](tools.md): fmt/tidy/analyze/mca/jit/repl
- [Optimization Guide](optimization.md): what `--opt-level` changes
