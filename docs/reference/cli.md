# CLI Reference

Generated from `polyglot --help` and `polyglot <command> --help` (v0.3.0).

```
polyglot [GLOBAL OPTIONS] <COMMAND> [COMMAND OPTIONS]
```

## Command Summary

All 49 commands, in `polyglot --help` order:

| Command | Description |
|---------|-------------|
| `build` | Build a source file or project |
| `recommend` | Provide optimization and configuration recommendations |
| `inspect` | Provide correlated views of binary information using multiple tools |
| `run` | Build and run a source file (extra args after `--`) |
| `secopt` | Provide security-aware optimization analysis and recommendations |
| `check` | Check syntax without building |
| `emit-ir` | Emit LLVM IR (clang) or GIMPLE (gcc) |
| `explain-ir` | Show IR with the source of each function beside it |
| `emit-asm` | Emit assembly |
| `diff-asm` | Diff assembly across optimization levels |
| `preproc` | Run the preprocessor (`-E`) |
| `deps` | Emit Make-style dependencies (`-MM`) |
| `graph` | Render the `#include` graph of source files |
| `doctor` | Report on the toolchain environment |
| `plugin` | Manage JSON-RPC plugins (`list`/`install`/`remove`/`info`/`publish`) |
| `targets` | List known target triples and availability |
| `fmt` | Format code with clang-format |
| `tidy` | Lint with clang-tidy (extra args after `--`) |
| `repl` | Interactive clang-repl (args after `--`) |
| `jit` | Run a `.ll`/`.bc` with lli (args after `--`) |
| `compile-commands` | Generate compilation database for tooling (clangd, clang-tidy, etc.) |
| `diagnose` | Compare diagnostics from multiple compilers |
| `mca` | Analyze machine code with llvm-mca |
| `lsp` | Launch the clangd language server (project flags applied) |
| `analyze` | clang static analyzer (`--analyze`) |
| `nm` | List symbols in object files |
| `fuzz` | Fuzz a function with libFuzzer (clang only) |
| `preview` | Local Compiler Explorer: edit and compile in a browser tab |
| `size` | Show section sizes of object files |
| `fixit` | Automatically apply safe fixes to compilation errors |
| `objdump` | Display information from object files |
| `watch` | Rebuild on file changes |
| `bench` | Build once, then time repeated program runs |
| `readelf` | Display the contents of ELF files |
| `pgo` | Profile-guided optimization (clang `-fprofile-instr` workflow) |
| `debug` | Build with debug info and launch debugger (lldb/gdb) |
| `stats` | Compilation statistics |
| `strip` | Strip symbols from object files |
| `transmute` | Show the step-by-step compilation process with explanations |
| `strings` | Display printable strings from files |
| `addr2line` | Convert addresses to file names and line numbers |
| `cache` | Manage inspection cache (`stats`/`clear`/`info`) |
| `init` | Initialize a new project |
| `c++filt` | Demangle C++ and Rust symbols |
| `clean` | Remove build artifacts and cache |
| `completions` | Emit shell completion scripts (bash/zsh/fish) |
| `version` | Show version and backend |
| `query` | Unified inspection interface for all polyglot inspection capabilities |

## Global Options

| Option | Description |
|--------|-------------|
| `-h`, `--help` | show help and exit |
| `--version` | show the version and exit |
| `-v`, `--verbose` | verbose output (full command lines, cache HIT/MISS) |
| `--quiet` | suppress `Running:` lines |
| `--target TARGET` | target triple (clang cross or host on gcc) |
| `--opt-level {none,less,default,aggressive,size,size-aggressive}` | optimization level (default: `default`) |
| `-o OUTPUT`, `--output OUTPUT` | output file |
| `--plugin PLUGIN` | JSON-RPC plugin binary (name or path; used by `build`) |
| `--plugin-dir PLUGIN_DIR` | plugin search directory (default: `~/.polyglot/plugins`) |
| `--compiler {auto,clang,gcc}` | force a backend (default: auto-discover; env `POLYGLOT_CC`/`POLYGLOT_CXX` override) |

## Optimization Levels

| Level | Flag | GCC/clang flag |
|-------|------|----------------|
| `none` | `--opt-level none` | `-O0` |
| `less` | `--opt-level less` | `-O1` |
| `default` | `--opt-level default` | `-O2` |
| `aggressive` | `--opt-level aggressive` | `-O3` |
| `size` | `--opt-level size` | `-Os` |
| `size-aggressive` | `--opt-level size-aggressive` | `-Oz` (gcc >= 12 and clang) |

## Shared Compile Options

Accepted by every source-facing command (`build`, `run`, `check`, `emit-ir`, `emit-asm`, `preproc`, `deps`, `mca`, `stats`, `watch`, `recommend`, `secopt`, `explain-ir`, `diff-asm`, `graph`, `compile-commands`, `fixit`, `bench`, `pgo`, `debug`, `transmute`, and friends; each command's `--help` lists exactly which):

| Option | Description |
|--------|-------------|
| `--std STD` | language standard (`c89..c2x` / `gnu*`, `c++98..c++23`) |
| `--warn {none,default,all,extra,pedantic}` | warning level (default: `default`) |
| `--werror` | warnings as errors |
| `--debug` | emit debug info (`-g`) |
| `--sanitize LIST` | sanitizers: `address,undefined,thread,memory,leak` (`memory` requires the clang backend) |
| `-I DIR` | include search path (repeatable) |
| `-D NAME[=VAL]` | preprocessor define (repeatable) |
| `--cc-arg ARG` | raw flag passed to the compiler (repeatable) |
| `-L DIR` | library search path (repeatable) |
| `-l NAME` | link library (repeatable) |

---

## Per-Command Options

### `build`

```
polyglot build [-o OUTPUT] [--std STD] [--warn LEVEL] [--werror] [--debug]
               [--sanitize LIST] [-I DIR] [-D NAME[=VAL]] [--cc-arg ARG]
               [-L DIR] [-l NAME] [--shared] [--static-lib] [-j JOBS] [--no-cache]
               [paths ...]
```

| Option | Description |
|--------|-------------|
| `-o`, `--output OUTPUT` | output file |
| `--shared` | build a shared library |
| `--static-lib` | build a static library (`ar rcs`) |
| `-j`, `--jobs JOBS` | parallel per-TU compilation |
| `--no-cache` | bypass the incremental object cache |
| `paths` | source files or project directory |

### `run`

```
polyglot run [--std STD] ... [--no-cache] file
```

| Argument | Description |
|----------|-------------|
| `file` | source file to run |

Program arguments follow a `--` separator: `polyglot run app.c -- --input f.txt`. The child's exit code is propagated.

### `check`

```
polyglot check [--std STD] ... paths [paths ...]
```

`paths` — source files to check (`-fsyntax-only`).

### `emit-ir`

```
polyglot emit-ir [-o OUTPUT] [--std STD] ... paths [paths ...]
```

clang emits LLVM IR (`-S -emit-llvm`); gcc emits GIMPLE (`-fdump-tree-gimple`).

### `emit-asm`

```
polyglot emit-asm [-o OUTPUT] [--intel] [--std STD] ... paths [paths ...]
```

`--intel` — Intel syntax (`-masm=intel`).

### `preproc`

```
polyglot preproc [-o OUTPUT] [--no-lines] [--std STD] ... file
```

`--no-lines` strips line markers (`-P`); `-o` writes to a file instead of stdout.

### `deps`

```
polyglot deps [-o OUTPUT] [--target-name NAME] [--std STD] ... file
```

`--target-name NAME` renames the make target (`-MT`); `-o` writes the depfile instead of stdout.

### `doctor`

```
polyglot doctor [--strict]
```

`--strict` exits `1` when anything is missing.

### `targets`

```
polyglot targets
```

No options. Prints known triples, their status (`native`, `needs-cross-gcc`, `unavailable`) and notes. See [Targets](targets.md).

### `fmt`

```
polyglot fmt [--style {llvm,google,chromium,mozilla,webkit,microsoft,file}] [--check] [-i]
             [--dump-config] paths [paths ...]
```

| Option | Description |
|--------|-------------|
| `--style` | clang-format style |
| `--check` | exit `1` if any file would change |
| `-i` | format files in place |
| `--dump-config` | print the style config and exit |

### `tidy`

```
polyglot tidy [--checks CHECKS] [--fix] [paths ...]
```

Check strings start with `-`, so use `--checks="-*,modernize-*"`. Extra clang-tidy arguments go after `--`.

### `repl`

```
polyglot repl
```

Interactive clang-repl; extra arguments go after `--`.

### `jit`

```
polyglot jit file
```

`file` — LLVM IR (`.ll`) or bitcode (`.bc`); runs it with `lli`. Extra `lli` arguments go after `--`.

### `mca`

```
polyglot mca [--iterations ITERATIONS] [--mcpu MCPU] [--std STD] ... file
```

`file` — `.c` source or `.s` assembly. `--iterations` defaults to 3.

### `analyze`

```
polyglot analyze paths [paths ...]
```

clang static analyzer; requires the clang backend.

### `watch`

```
polyglot watch [--timeout TIMEOUT] [--std STD] ... file
```

`--timeout N` — exit after N seconds (`0` = watch forever).

### `stats`

```
polyglot stats [--target TARGET] [--opt-level LEVEL] [--json] [--std STD] ... paths [paths ...]
```

`--json` produces machine-readable output (see [Cache and Parallelism](../user-guide/cache-and-parallelism.md#json-output-for-ci)).

### `init`

```
polyglot init [--language {c,cpp,rust}] [name]
```

### `clean`

```
polyglot clean [path]
```

Removes object/executable/IR/assembly artifacts and clears `.polyglot-cache/`.

### `version`

```
polyglot version
```

### `recommend`

```
polyglot recommend [-o OUTPUT] [--json] [--std STD] ... [paths ...]
```

Analyzes source files or directories and suggests optimization and configuration changes. `--json` for machine-readable output.

### `inspect`

```
polyglot inspect [--strings] [--json] [--std STD] ... [paths ...]
```

Correlated view of binary information (symbols, sections, sizes, strings) across the inspection tools in one pass. `paths` — object files or executables.

### `secopt`

```
polyglot secopt [--json] [--std STD] ... [paths ...]
```

Security-aware optimization analysis and recommendations for source files or directories.

### `explain-ir`

```
polyglot explain-ir [--format {pgir,llvm}] [--functions NAME ...] [--std STD] ... FILE
```

Shows the compiler's IR with the source of each function beside it. `--format llvm` (default, clang backend) emits LLVM IR; `--format pgir` shows Polyglot IR from the native Rust driver. `--functions` limits output to the named functions.

### `diff-asm`

```
polyglot diff-asm [--levels LEVELS] [--intel] [--output {text,json}] [--json] [--no-cache] [--std STD] ... paths
```

Compiles each source at multiple optimization levels and diffs the assembly. `--levels` takes a comma-separated list of `0-3`, `s`, `z` or level names (default: `0,2,3`).

### `graph`

```
polyglot graph [--format {svg,ascii,json}] [--out FILE] [--no-system] [--std STD] ... files
```

Extracts the `#include` graph (one compiler `-H` pass per TU, `-MM` fallback) and renders it as an ASCII tree, JSON (`{nodes, edges}`) or a dependency-free SVG. System headers show as gray nodes unless `--no-system`; include cycles are errors.

### `plugin`

```
polyglot plugin [--registry URL|PATH] [--no-push] {list,install,remove,info,publish} [name]
```

Manages JSON-RPC plugins: `list` shows installed, builtin and registry plugins; `install` takes a binary path, git URL or registry name; `remove`, `info`, `publish` operate on plugin names/paths. `--registry` overrides the built-in registry locations.

### `compile-commands`

```
polyglot compile-commands [-o FILE] [--target TARGET] [--shared] [--static-lib] [--std STD] ... [paths ...]
```

Writes a `compile_commands.json` (default name) with the full command line for every source, for clangd, clang-tidy and other tooling. `paths` — source files or directories (default: current directory).

### `diagnose`

```
polyglot diagnose [--std STD] [--warn LEVEL] [--werror] paths
```

Runs each source through every available compiler (clang, gcc) and shows the warnings/errors side-by-side.

### `lsp`

```
polyglot lsp [--roots [DIR ...]]
```

Launches clangd for editor integration. When `polyglot.toml` has a `[flags]` section, a generated `compile_flags.txt` (includes/defines/std) is written under `<root>/.polyglot-cache/lsp` and clangd is pointed at it. The first root hosts `polyglot.toml` (default: the current directory).

### `nm`

```
polyglot nm [-a] [-g] [-u] [--defined-only] [-P] [-f {sysv,bsd,posix}] [-p] [--radix {d,o,x}] [--json] [--no-cache] [paths ...]
```

Lists symbols in object files or executables. `-u` shows only undefined symbols, `-g` only external ones, `--json` for programmatic use.

### `fuzz`

```
polyglot fuzz --entry FN [--corpus DIR] [--runs RUNS] [--json] [--std STD] ... [-L DIR] [-l NAME] file
```

Fuzzes one function with libFuzzer (clang backend only). `FN` must have the exact signature `int FN(const unsigned char *data, size_t size)` — a harness defining `LLVMFuzzerTestOneInput` is generated and compiled together with `file`. `--runs` defaults to 1000; `--corpus` seeds and collects inputs.

### `preview`

```
polyglot preview [--port PORT] [--no-browser] [file]
```

Serves a local Compiler-Explorer page on `127.0.0.1`: edit a snippet, compile it with the driver's in-process pipeline, and inspect the assembly, LLVM IR/GIMPLE and llvm-mca output in browser tabs. `--port` defaults to 8641 (`0` picks a free port); binding is loopback-only.

### `size`

```
polyglot size [-A {d,o,x}] [-B] [-d] [-x] [-o] [--format {berkeley,sysv,both}] [--common] [--json] [--no-cache] [paths ...]
```

Shows section sizes of object files (Berkeley format by default). `--common` lists common symbols separately.

### `fixit`

```
polyglot fixit [--yes] [--verbose] [--std STD] ... paths
```

Analyzes compilation errors and applies common safe fixes (missing semicolons, brackets, ...). `--yes`/`-y` applies fixes without prompting; `--verbose`/`-v` details each applied fix.

### `objdump`

```
polyglot objdump [OBJDUMP FLAGS] [--json] [--no-cache] [paths ...]
```

Wraps `objdump` for object files and executables: `-d`/`-D` (disassemble), `-f`/`-a` (file/archive headers), `--section-headers`, `-j SECTION`, `-r`/`-R` (relocations), `-s`, `-t` (symbol table), `-x`, and the usual flags — passthrough to the underlying tool.

### `bench`

```
polyglot bench [--runs RUNS] [--args ARG] [--timeout TIMEOUT] [--no-cache] [--json] [--opt-level LEVEL] [--std STD] ... file
```

Builds once, then times repeated program runs (`--runs`, default 5). `--args` passes an argument to the program (repeatable); tokens after `--` also pass through. `--timeout` bounds each run (default 60s).

### `readelf`

```
polyglot readelf [READELF FLAGS] [--json] [--no-cache] [paths ...]
```

Wraps `readelf` for ELF files: `-a` (everything), `-h` (file header), `-l` (program headers), `-S` (sections), `-s` (symbols), `-r`/`-R` (relocations), `-d` (dynamic), `-V` (version info), and the usual flags — passthrough to the underlying tool.

### `pgo`

```
polyglot pgo [--args "ARG ..."] [--out PATH] [--json] [--opt-level LEVEL] [--std STD] ... file
```

Profile-guided optimization via the clang `-fprofile-instr` workflow: instrument, run the program to train (`--args` are passed to every run), then recompile with the profile. Output binary defaults to `<source stem>.pgo`.

### `debug`

```
polyglot debug [--std STD] ... file
```

Compiles with `-g` and launches lldb (clang backend) or gdb (GCC backend) on the result. Debugger arguments go after `--`.

### `strip`

```
polyglot strip [STRIP FLAGS] [--json] [--dry-run] paths
```

Wraps `strip` for object files and executables: `-s` (strip all), `-g` (debug symbols only), `-K`/`-N` (keep/remove symbol), `-o FILE`, `-R SECTION`, `-x`/`-X` (discard locals), and the usual flags. `--dry-run` shows what would be done.

### `transmute`

```
polyglot transmute [--opt-level LEVEL] [--target TARGET] [--std STD] ... FILE
```

Narrates the step-by-step compilation of a source file with explanations at each stage.

### `strings`

```
polyglot strings [-a] [-n NUMBER] [-t {d,o,x}] [--format {bsd,sysv,posix}] [-f] [--unwarn] [--json] [--no-cache] [paths ...]
```

Displays printable strings from files. `-n` sets the minimum length; `-t` prefixes offsets in the given radix; `-a` scans whole files instead of data sections only.

### `addr2line`

```
polyglot addr2line [--addresses ADDRESS] [-f] [-p] [-b] [-C] [-fi] [-s] [--json] [--no-cache] [paths ...]
```

Converts hexadecimal addresses to file names and line numbers. `-f` adds function names, `-C` demangles, `-p` pretty-prints.

### `cache`

```
polyglot cache {stats,clear,info} ...
```

Manages the inspection cache: `stats` shows statistics, `clear` removes all entries, `info` shows the configuration. (This is the inspection-results cache; the build object cache lives in `.polyglot-cache/` and is managed with `clean`.)

### `c++filt`

```
polyglot c++filt [--symbols SYMBOL] [-_] [-n] [-p] [-i] [-t] [-s {none,auto,gnu-v3,java,gnat,dlang,rust}] [--json] [--no-cache] [paths ...]
```

Demangles C++ and Rust symbols. Without `--symbols`, reads names from stdin; with paths, reads symbols from the object files.

### `completions`

```
polyglot completions {bash,zsh,fish}
```

Emits a completion script for the chosen shell, generated from the live command registry. Install with `eval "$(polyglot completions bash)"` (or the zsh/fish equivalent).

### `query`

```
polyglot query [--what WHAT] [--json] [--no-cache] [--save FILE] [--max-cache-age SECONDS] [paths ...]
```

Single programmable interface over all inspection capabilities (symbols, sizes, sections, ...). `--what` takes a comma-separated list of aspects (default: all); `--save` writes the JSON to a file. Results are cached (`--max-cache-age`, default 3600s).

### `verify`

```
polyglot verify [--std STD] [--warn LEVEL] [--werror] [--debug] [--sanitize LIST] [-I DIR] [-D NAME[=VAL]] [--cc-arg ARG]
```

Compiles a tiny built-in program to an object file with the resolved toolchain and reports success — the quickest end-to-end health check that the compiler actually produces valid objects (complements `doctor`, which inventories the environment).

---

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | Success |
| `1` | General error: compile/link/run failure, file not found, tool error |
| `2` | Usage error: bad flag value, missing required argument |
| `3` | Missing tool or backend (e.g. `analyze` without clang) |

`run` exits with the child program's exit code; Ctrl+C exits `130`.

## Environment Variables

| Variable | Description |
|----------|-------------|
| `POLYGLOT_CC` | absolute path or name of the C compiler (highest precedence) |
| `POLYGLOT_CXX` | absolute path or name of the C++ compiler (highest precedence) |
| `NO_COLOR` | disable colored output |

Toolchain discovery order: `POLYGLOT_CC`/`POLYGLOT_CXX` > `polyglot.toml` `[compiler] cc/cxx` > `--compiler {auto,clang,gcc}` > `PATH` (clang, clang-NN, gcc, cc) > scan dirs (`~/llvm-install/*/bin`, `/usr/lib/llvm-*/bin`, `/usr/local/bin`, `/opt/*/bin`).

## Configuration File

See [Configuration](../getting-started/configuration.md) for `polyglot.toml`.

## Target Triples

See [Targets](targets.md) for the full list and the `polyglot targets` command.
