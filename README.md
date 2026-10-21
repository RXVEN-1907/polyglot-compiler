<div align="center">
  <img src="docs/logo.svg" alt="polyglot: a bold P with a chevron resting in its counter" width="150">
  <h1>polyglot</h1>
  <p><em>many languages. one shape.</em></p>
  <p><strong>A compiler driver for every C/C++ toolchain you have — and a compiler of its own that shows its work.</strong></p>
  <p>
    <a href="https://github.com/RXVEN-1907/polyglot-compiler/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/RXVEN-1907/polyglot-compiler/actions/workflows/ci.yml/badge.svg"></a>
    <a href="https://github.com/RXVEN-1907/polyglot-compiler/releases/latest"><img alt="Release" src="https://img.shields.io/github/v/release/RXVEN-1907/polyglot-compiler?sort=semver"></a>
    <a href="https://github.com/RXVEN-1907/polyglot-compiler/releases/latest"><img alt="platforms" src="https://img.shields.io/badge/platforms-linux%20%7C%20windows%20%7C%20macos-8a63d2"></a>
    <a href="LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-blue.svg"></a>
    <a href="https://www.rust-lang.org"><img alt="rust" src="https://img.shields.io/badge/rust-stable-dea584.svg"></a>
    <a href="https://www.python.org"><img alt="python" src="https://img.shields.io/badge/python-3.8+-3776ab.svg"></a>
  </p>
</div>

---

You already own the pieces — gcc, clang, llvm-nm, objdump, gdb,
clang-tidy. What nobody gave you was **one command that uses them
together**: cached parallel builds, one vocabulary for binary
inspection, cross targets listed with the exact line that unlocks them,
machine-readable output everywhere. polyglot is that command.

And because a driver that only delegates is half a story, the repo also
ships a **built-in compiler** — a from-scratch Rust pipeline that takes
a subset of C from source text to a running binary and narrates every
stage on the way.

<div align="center"><img src="docs/gifs/forge.gif" alt="terminal session: polyglot typing a build of a 61-TU project, compiling in parallel, then rebuilding from cache in 0.873 seconds" width="820"></div>

## Try it in sixty seconds

```console
$ pipx install git+https://github.com/RXVEN-1907/polyglot-compiler
$ polyglot doctor                    # what toolchains did it find?
$ polyglot build hello.c -o hello    # cached, parallel, -O2
$ polyglot inspect ./hello           # symbols, imports, strings
```

The Python driver needs only the standard library plus `toml` and any
one C compiler. Prebuilt Rust binaries ship on every
[release](#releases). No daemon, no Docker, no config file required.

<div align="center"><img src="docs/svg/div-forge.svg" alt="" width="100%"></div>

## Forge — build and run

`polyglot build` wraps gcc or clang and adds what they don't give you:
a sha256-keyed object cache so unchanged translation units never
recompile, `-j N` parallelism per TU, sanitizers as one flag, warnings
as a level, and honest cross-target reporting (`polyglot targets` shows
each triple's status and the exact apt line to unlock it).

<div align="center"><img src="docs/svg/cache-chart.svg" alt="bar pair: the same 61-TU rebuild at 2278 ms without the cache and 873 ms with it — 2.8x faster" width="700"></div>

Projects are a `polyglot.toml` away — every `[[bin]]` and `[[lib]]`
target builds in parallel from cache with `polyglot build ./project`,
and `polyglot init` scaffolds the layout. `run`, `watch`, `bench`,
`clean`, `check` round out the loop.

<div align="center"><img src="docs/svg/div-inspect.svg" alt="" width="100%"></div>

## Inspect — read any binary

One vocabulary over the binutils zoo, `--json` everywhere:

```console
$ polyglot nm ./engine            # symbol table
$ polyglot objdump ./engine -d    # disassembly
$ polyglot readelf ./engine       # container headers
$ polyglot inspect ./engine       # correlated view: all of it, one pane
$ polyglot query ./engine main --json
```

`inspect` and `query` correlate output across tools and cache it, so
asking five questions about one binary costs one tool pass. `diff-asm`
diffs the same source across optimization levels; `diagnose` compares
gcc's and clang's diagnostics on the same file.

<div align="center"><img src="docs/gifs/inspect.gif" alt="terminal session: inspect summarizing a binary's symbols and imports, then query returning one symbol as JSON" width="820"></div>

<div align="center"><img src="docs/svg/div-transmute.svg" alt="" width="100%"></div>

## Transmute — the built-in pipeline

The Rust workspace is a real compiler for a subset of C — preprocessor,
lexer, parser, name resolution and type checking, IR with optimization
passes, LLVM IR, object files. `polyglot-driver build --backend native`
compiles end to end; its *build* needs no system LLVM (objects go
through `llc`, linked with the system `cc`, with a graceful `.ll`
fallback when `llc` is absent).

<div align="center"><img src="docs/svg/pipeline-metro.svg" alt="metro line with eight stations from source through lexing, parsing, semantic analysis, IR, optimization, LLVM, to a binary" width="860"></div>

`transmute-internal` narrates the whole journey, stage by stage — the
command to reach for when you want to *see* compilation happen:

<div align="center"><img src="docs/gifs/transmute.gif" alt="terminal session: the six pipeline stages narrated for fib.c, then the binary printing 55" width="820"></div>

It is a teaching-grade compiler and says so: functions, structs,
pointers, control flow, a full test suite — production builds go
through gcc/clang. Its job is to make the pipeline legible.

## All 49 commands

| group | commands |
|---|---|
| **build & run** | `build` `run` `check` `watch` `bench` `clean` `init` `cache` |
| **inspection** | `nm` `objdump` `readelf` `size` `strings` `strip` `addr2line` `cxxfilt` `inspect` `query` |
| **compiler views** | `emit-ir` `emit-asm` `preproc` `deps` `explain-ir` `transmute` `diff-asm` `graph` |
| **quality** | `fmt` `tidy` `analyze` `fuzz` `secopt` `verify` `fixit` |
| **tuning** | `pgo` `mca` `recommend` `diagnose` `stats` |
| **cross & config** | `targets` `compile-commands` `lsp` `debug` `config` |
| **tools & meta** | `doctor` `version` `completions` `plugin` `preview` `repl` `jit` |

Every command documents itself with `--help`; most accept `--json`; all
respect [`NO_COLOR`](https://no-color.org). Full reference:
[`docs/reference/cli.md`](docs/reference/cli.md).

## Plugins

Compilers talk JSON-RPC 2.0 over stdio. `plugins/polyglot-clang` ships
as the reference plugin (Rust, ~200 lines); `polyglot plugin
install/list/remove` manages more, and `build --plugin` routes a
compile through one.

```console
$ polyglot plugin list
NAME            VERSION  KIND     PATH
polyglot-clang  0.3.0    builtin  .../polyglot-clang
```

## How it compares

| dimension | polyglot | ccache | bear | Compiler Explorer |
|---|---|---|---|---|
| primary job | drive + inspect + explain | cache compiles | generate compile_commands.json | explore IR/asm in a browser |
| object cache | built-in, sha256 | its whole point | none | none |
| binary inspection | 10 commands + correlated views | none | none | none |
| narrated pipeline | `transmute`, step by step | none | none | side-by-side, web only |
| install | one binary-free pip package | system package | system package | hosted / docker |

ccache is the cache king — polyglot's cache is for when you also want
the inspection, the IR views, and the pipeline in the same CLI.

## CI/CD integration

```yaml
# GitHub Actions: build with caching, then export a clangd database
- run: pipx install git+https://github.com/RXVEN-1907/polyglot-compiler
- run: polyglot build ./src -o app -j 4 --warn extra --werror
- run: polyglot compile-commands ./src -o compile_commands.json
```

`compile-commands` keeps clangd/IDE indexing in sync with the same
flags the build used.

## Releases

Tag a version and CI builds the Rust driver for Linux (x86-64
glibc/musl, aarch64, armv7, riscv64, s390x, loongarch64), Windows
(x86-64 + ARM64), and macOS (Apple silicon, Intel, universal2), plus
the `polyglot-clang` plugin binary — each archive checksummed and
smoke-checked. The Python driver needs no binary: `pipx` or
[`install.sh`](install.sh) is the whole install.

## FAQ

<details>
<summary><strong>Does it replace gcc/clang?</strong></summary>

No — it drives them. Your system compiler does the compiling; polyglot
adds caching, parallelism, uniform flags, inspection, and output
formats. The built-in pipeline is the one place polyglot compiles C
itself (a teaching-grade subset), clearly labeled.
</details>

<details>
<summary><strong>What does it need installed?</strong></summary>

Any one C compiler (gcc or clang) to be useful. Each inspection command
degrades gracefully and `polyglot doctor` tells you exactly what's
missing and the apt/brew line to fix it. The Rust driver's default
build needs no system LLVM at all.
</details>

<details>
<summary><strong>Where does state live?</strong></summary>

`.polyglot-cache/` in your project (delete it and the cache forgets)
plus `~/.polyglot/` for plugins and the inspection cache.
</details>

<details>
<summary><strong>Why is the driver Python if the project is Rust?</strong></summary>

The Rust workspace *is* the compiler — syntax, sema, IR, codegen, the
driver binary, and the plugin are all Rust. The 49-command surface is a
Python CLI because it is glue over system tools, stdlib-only and
instant to extend; the split is deliberate and both doors are
first-class. See [architecture](#architecture).
</details>

## Architecture

<div align="center"><img src="docs/svg/architecture.svg" alt="constellation of the seven crates and the clang plugin orbiting the Rust driver, with polyglot-core beneath" width="860"></div>

Two front doors, one repo. Full map with a paragraph per crate:
[`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md).

## Documentation

[`docs/index.md`](docs/index.md) ·
[`getting-started/`](docs/getting-started/) ·
[`docs/reference/cli.md`](docs/reference/cli.md) ·
[`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) ·
[`docs/user-guide/`](docs/user-guide/) ·
[`docs/API_REFERENCE.md`](docs/API_REFERENCE.md)

## Contributing

CI gates formatting, clippy (`-D warnings`, all targets), both test
suites, the audit, and a strict mkdocs build. Add a command to
`polyglot_lib/commands/`, a pass to `polyglot-ir`, or pick something
from the tracker — see [CONTRIBUTING.md](CONTRIBUTING.md).

## License

MIT — see [LICENSE](LICENSE).
