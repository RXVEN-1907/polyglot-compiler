# Development

How to set up a working tree and verify your changes.

## Setup

```bash
# Clone
git clone https://github.com/RXVEN-1907/polyglot-compiler
cd polyglot-compiler

# Python driver dependencies (stdlib + toml; pytest for the test suite)
pip install toml pytest

# Rust toolchain (stable is enough; the default build needs no system LLVM)
rustup default stable
```

A C compiler (gcc or clang) must be discoverable — that is the only runtime dependency of the Python driver.

## Layout

```
polyglot              # thin launcher
polyglot_lib/         # the Python driver package (cli.py + commands/)
crates/               # Rust workspace: core, syntax, sema, ir, protocol, codegen, driver
plugins/polyglot-clang/  # reference JSON-RPC stdio plugin (Rust)
scripts/              # visual-identity generators + release packaging
docs/                 # this documentation tree (mkdocs)
test_comprehensive.py # end-to-end Python test suite
```

## Tests

```bash
# End-to-end Python driver suite
python3 test_comprehensive.py

# Rust workspace unit tests
cargo test --workspace

# Rust lint gate (CI fails on warnings)
cargo clippy --workspace --all-targets -- -D warnings
```

## Documentation

The docs build with mkdocs (Material theme) and CI builds with `--strict`, so a nav entry pointing at a missing file fails the build:

```bash
pip install mkdocs-material mkdocs-git-revision-date-localized-plugin mkdocs-minify-plugin
mkdocs build --strict
```

## Visuals

The logo, mode banners, social preview, demo GIFs and the cache chart are generated from committed scripts in `scripts/` — regenerate and commit the output after changing them:

```bash
python3 scripts/gen_logo.py          # logo (self-adapting)
python3 scripts/gen_mode_banners.py  # FORGE / INSPECT / TRANSMUTE banners
python3 scripts/gen_social.py        # social preview
python3 scripts/gen_demo_gif.py      # demo GIFs
python3 scripts/gen_charts.py        # cache chart
```

## Release

`scripts/package_release.py` packages a release; the Python driver is pip-installable straight from the repository (`pipx install git+https://github.com/RXVEN-1907/polyglot-compiler` via `pyproject.toml`).

## Plugin Development

Plugins are separate executables speaking newline-delimited JSON-RPC 2.0 over stdio (`crates/polyglot-protocol` defines the types; protocol version `0.1.0`). Build the reference plugin and try it:

```bash
cargo build --release -p polyglot-clang
polyglot --plugin target/release/polyglot-clang build examples/01_hello.c -o /tmp/pgplug
```

See [Commands Reference: Plugin Architecture](../user-guide/commands.md#plugin-architecture) for the wire protocol.
