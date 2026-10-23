# Python Driver Architecture

!!! note
    This page describes the driver as of v0.3.0, where the implementation lives in the `polyglot_lib/` package. The full command/flag reference lives in the [Commands Reference](../user-guide/commands.md).

## Overview

The Python driver (`polyglot`) is the user-facing entry point: a thin launcher plus the `polyglot_lib` package (~2,500 lines) that depends only on the standard library plus the `toml` package. It wraps whatever C/C++ toolchain it discovers (GCC or clang) and dispatches to the LLVM auxiliary tools (`clang-format`, `clang-tidy`, `llvm-mca`, `lli`, `clang-repl`) and the binary inspection tools (`nm`, `objdump`, `readelf`, `size`, `strings`, `strip`, `addr2line`, `c++filt`) when those commands need them.

## Module Structure

The `polyglot` launcher only makes the `polyglot_lib` package importable and calls its `main()`. The package is organized by responsibility:

```
polyglot                     # thin launcher (sys.path setup + main())
polyglot_lib/
├── cli.py                   # argparse parser, dispatch, exit-code translation
├── toolchain.py             # Toolchain class: compiler/tool discovery, project config
├── backends.py              # backend-aware flag translation (gcc/clang/rustc) + compile/link machinery
├── cache.py                 # BuildCache: sha256-keyed incremental object cache
├── config.py                # project-dir lookup, [[bin]]/[[lib]]/[language] resolution
├── output.py                # color handling (NO_COLOR), PolyglotError, MissingToolError
├── inspection_cache.py      # shared cache for binary-inspection results
├── preview_assets.py        # assets for the local Compiler Explorer (preview)
├── plugin_client.py         # JSON-RPC stdio client for plugins
└── commands/                # one module per command (auto-registered via register())
```

## Toolchain Discovery (`Toolchain`)

`Toolchain._discover()` resolves the C compiler on the first match:

1. `POLYGLOT_CC` / `POLYGLOT_CXX` environment variables,
2. `polyglot.toml` `[compiler] cc` / `cxx` (applied per project build by `apply_project_config`),
3. `--compiler {auto,clang,gcc}` preference,
4. `PATH` scan: `clang`, `clang-NN`, `gcc`, `cc`,
5. well-known directories: `~/llvm-install/*/bin`, `/usr/lib/llvm-*/bin`, `/usr/local/bin`, `/opt/*/bin`.

The compiler's `--version` output is parsed to classify it as `clang` or `gcc` and record its version. A C++ compiler (`clang++`/`g++` lists) and the auxiliary tools are discovered the same way: `find_tool(name)` checks `PATH`, then clang's own `bin` directory, then the LLVM scan directories, caching results.

`needs_target_ok(target)` enforces the backend rule: the GCC backend rejects any `--target` whose architecture does not match the host, suggesting a cross GCC or `--compiler clang`.

## Flag Construction

`common_compile_args(tc, args, sources)` turns CLI options into backend-appropriate flags: `--std` validated per language (`validate_std`), `--warn` levels to `-w`/`-Wall`/`-Wextra`/`-Wpedantic` families, `--sanitize` lists to `-fsanitize=...`, include/define/library flags, plus per-backend quirks (e.g. the GCC backend's `emit-ir` uses `-fdump-tree-gimple`, clang uses `-S -emit-llvm`).

## Incremental Build Cache (`BuildCache`)

Objects are stored in `<project>/.polyglot-cache/<sha256>.o` with a `manifest.json`. The key hashes the compiler path + version, exact flag list, source name and bytes, and project directory. Lookups check the object file and manifest; stores use an atomic rename. See [Cache and Parallelism](../user-guide/cache-and-parallelism.md).

## Command Dispatch

Each command is a `cmd_*` function taking `(toolchain, args)`:

- `cmd_build`: resolves inputs (sources or a `polyglot.toml` project), applies project config, compiles (via cache, `-j` parallel per TU) and links. Rust projects route to `build_rust_project` (`rustc`).
- `cmd_run`: same pipeline into a temp dir, then executes the program and propagates its exit code.
- `cmd_check`: `-fsyntax-only` per file.
- `cmd_emit_ir` / `cmd_emit_asm` / `cmd_preproc` / `cmd_deps`: single-source inspection commands backed by `-emit-llvm`/`-fdump-tree-gimple`, `-S`, `-E`, `-MM`.
- `cmd_doctor` / `cmd_targets`: environment report and triple availability table.
- `cmd_fmt` / `cmd_tidy` / `cmd_repl` / `cmd_jit` / `cmd_mca` / `cmd_analyze`: wrap the auxiliary tools; `cmd_analyze` requires the clang backend and raises `MissingToolError` (exit 3) otherwise.
- `cmd_watch`: poll-and-rebuild loop (`--timeout`).
- `cmd_stats`: compile with `-ftime-report` (gcc) or the clang equivalent, parse phases via `parse_time_report`, add binary size; `--json` prints the document.
- `cmd_init` / `cmd_clean` / `cmd_version`: scaffolding, artifact + cache removal, version banner.

## Argument Parsing and Exit Codes

`build_parser()` builds one argparse parser with a subcommand per command; common compile flags are attached through `add_common_flags()`. `main()` uses `parse_known_args`. Unrecognized trailing arguments are warned about and ignored (plugins/legacy compatibility), while `--` separators are split by `_split_at_ddash` so programs and tools can receive their own arguments.

Errors raise `PolyglotError` subclasses carrying an exit code, translated in `main()`:

| Exit | Raised by |
|------|-----------|
| `0` | success |
| `1` | compile/link/run failures, `PolyglotError` default |
| `2` | `EXIT_USAGE`: bad flag values, missing arguments, nothing to build |
| `3` | `MissingToolError`: compiler or auxiliary tool absent |
| `130` | KeyboardInterrupt (Ctrl+C) |

## Where the Real Compilation Happens

The driver never parses or optimizes code itself. It composes correct command lines for the discovered backend and interprets their output:

| Task | GCC backend | clang backend |
|------|-------------|---------------|
| Compile | `gcc -std=... -O... -c` | `clang -std=... -O... -c` |
| IR | `-fdump-tree-gimple` (GIMPLE) | `-S -emit-llvm` (LLVM IR) |
| Static analysis | unsupported (exit 3) | `clang --analyze` |
| Cross target | host triple only | `--target=` passthrough + sysroot |
| Rust | `rustc` (either backend) | `rustc` |

The Rust workspace under `crates/` implements a separate in-process pipeline; see [Architecture Overview](overview.md).
