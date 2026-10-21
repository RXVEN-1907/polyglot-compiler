#!/usr/bin/env bash
# ---------------------------------------------------------------------------
# install.sh — installer for the Polyglot compiler driver.
#
# Installs the Python 3 driver 'polyglot' into a user prefix: the launcher
# plus the 'polyglot_lib' package directory it imports (copied alongside).
# No superuser privileges are required.
#
# Alternative install route: with pipx available, you can install the driver
# as a proper Python package instead:
#
#   pipx install git+https://github.com/RXVEN-1907/polyglot-compiler
#
# Usage:
#   ./install.sh [--prefix DIR] [--yes]       install
#   ./install.sh --uninstall [--prefix DIR]   remove the installed driver
#
# The driver is placed in PREFIX itself when PREFIX ends in /bin, otherwise
# in PREFIX/bin (e.g. --prefix /opt/pg installs /opt/pg/bin/polyglot and
# /opt/pg/bin/polyglot_lib).
# Default prefix: ~/.local/bin
#
# What is checked:
#   * python3 >= 3.8
#   * python 'toml' package (offered to install via pip --user when missing)
#   * a C backend: clang or gcc (POLYGLOT_CC overrides, as in the driver)
#
# Exit codes: 0 success, 1 failure, 2 usage error.
# ---------------------------------------------------------------------------

# Re-exec under bash when invoked with a POSIX shell (e.g. `sh install.sh`).
if [ -z "${BASH_VERSION:-}" ]; then
  exec bash "$0" "$@"
fi

set -euo pipefail

REPO_URL="https://github.com/RXVEN-1907/polyglot-compiler"
RAW_URL="https://raw.githubusercontent.com/RXVEN-1907/polyglot-compiler/master"

info() { printf '==> %s\n' "$*"; }
warn() { printf 'WARNING: %s\n' "$*" >&2; }
die()  { printf 'ERROR: %s\n' "$1" >&2; exit "${2:-1}"; }

usage() {
  cat <<EOF
install.sh - install the Polyglot compiler driver ('polyglot')

Alternative (pipx): install the driver as a Python package instead:

  pipx install git+https://github.com/RXVEN-1907/polyglot-compiler

Usage:
  install.sh [--prefix DIR] [--yes]      install the driver
  install.sh --uninstall [--prefix DIR]  remove the driver

Options:
  --prefix DIR     install directory. The driver is placed in DIR itself
                   when DIR ends in /bin, otherwise in DIR/bin.
                   Default: $HOME/.local/bin
  --yes, -y        never prompt; assume yes (non-interactive / CI)
  --uninstall      remove the installed driver and exit
  -h, --help       show this help and exit

Environment:
  POLYGLOT_CC      preferred C compiler (same override the driver honors)

Exit codes: 0 success, 1 failure, 2 usage error.
EOF
}

# ---------------------------------------------------------------------------
# Argument parsing
# ---------------------------------------------------------------------------
PREFIX=""
UNINSTALL=0
ASSUME_YES=0

while [ $# -gt 0 ]; do
  case "$1" in
    --prefix)
      [ $# -ge 2 ] || die "--prefix requires a directory argument" 2
      PREFIX="$2"; shift 2
      ;;
    --prefix=*)
      PREFIX="${1#--prefix=}"; shift
      ;;
    --yes|-y)
      ASSUME_YES=1; shift
      ;;
    --uninstall)
      UNINSTALL=1; shift
      ;;
    -h|--help)
      usage; exit 0
      ;;
    --)
      shift; break
      ;;
    *)
      printf 'ERROR: unknown option: %s\n' "$1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

# Normalize the prefix: expand a leading ~, drop trailing slashes.
PREFIX="${PREFIX:-$HOME/.local/bin}"
while [ "$PREFIX" != "/" ] && [ "${PREFIX%/}" != "$PREFIX" ]; do
  PREFIX="${PREFIX%/}"
done
case "$PREFIX" in
  "~")   PREFIX="$HOME" ;;
  "~/"*) PREFIX="$HOME/${PREFIX#\~/}" ;;
esac

# The driver lives in PREFIX itself when it ends in /bin, else PREFIX/bin.
case "$PREFIX" in
  */bin) BIN_DIR="$PREFIX" ;;
  *)     BIN_DIR="$PREFIX/bin" ;;
esac
TARGET="$BIN_DIR/polyglot"
LIB_TARGET="$BIN_DIR/polyglot_lib"

# ---------------------------------------------------------------------------
# Locate the 'polyglot' source: next to this script when install.sh exists on
# disk (a clone); otherwise (curl | bash) it is downloaded from the repo.
# A local source is used only when BOTH the launcher and the polyglot_lib
# package directory are present.
# ---------------------------------------------------------------------------
SCRIPT_SRC=""
LIB_SRC=""
SELF_PATH="${BASH_SOURCE[0]:-}"
if [ -n "$SELF_PATH" ] && [ -f "$SELF_PATH" ]; then
  SCRIPT_DIR="$(cd -- "$(dirname -- "$SELF_PATH")" && pwd -P)"
  if [ -f "$SCRIPT_DIR/polyglot" ] && [ -d "$SCRIPT_DIR/polyglot_lib" ]; then
    SCRIPT_SRC="$SCRIPT_DIR/polyglot"
    LIB_SRC="$SCRIPT_DIR/polyglot_lib"
  fi
fi

# ---------------------------------------------------------------------------
# --uninstall: remove the driver (no environment checks needed)
# ---------------------------------------------------------------------------
if [ "$UNINSTALL" -eq 1 ]; then
  if [ -e "$TARGET" ]; then
    rm -f -- "$TARGET"
    info "removed $TARGET"
  else
    info "$TARGET not found (already uninstalled?)"
  fi
  if [ -d "$LIB_TARGET" ]; then
    rm -rf -- "$LIB_TARGET"
    info "removed $LIB_TARGET"
  else
    info "$LIB_TARGET not found (already uninstalled?)"
  fi
  # Tidy an install dir we may have created, but never remove ~/.local/bin.
  if [ "$BIN_DIR" != "$HOME/.local/bin" ]; then
    rmdir -- "$BIN_DIR" 2>/dev/null || true
  fi
  [ -e "$TARGET" ] && die "uninstall failed: $TARGET still present" 1
  [ -d "$LIB_TARGET" ] && die "uninstall failed: $LIB_TARGET still present" 1
  exit 0
fi

# ---------------------------------------------------------------------------
# Environment checks
# ---------------------------------------------------------------------------

# 1. python3 >= 3.8
if ! command -v python3 >/dev/null 2>&1; then
  die "python3 not found on PATH. Install Python 3.8+ and re-run."
fi
PY_DESC="$(python3 -V 2>&1)"
if ! python3 -c 'import sys; sys.exit(0 if sys.version_info >= (3, 8) else 1)' >/dev/null 2>&1; then
  die "$PY_DESC is too old; polyglot requires Python >= 3.8." 1
fi
info "python3 OK: $PY_DESC"

# 2. python 'toml' package (offer to install when missing)
if python3 -c 'import toml' >/dev/null 2>&1; then
  info "python 'toml' package OK"
else
  info "python 'toml' package missing"
  if [ "$ASSUME_YES" -eq 1 ]; then
    ANSWER=y
  elif [ -t 0 ]; then
    printf 'Install it now with "python3 -m pip install --user toml"? [Y/n] '
    if read -r ANSWER; then ANSWER="${ANSWER:-y}"; else ANSWER=y; fi
  else
    ANSWER=n
    warn "non-interactive without --yes: skipping the 'toml' install"
  fi
  case "$ANSWER" in
    y|Y|yes|YES|Yes)
      info "running: python3 -m pip install --user toml"
      if ! python3 -m pip install --user toml; then
        warn "pip install failed; retrying with PIP_BREAK_SYSTEM_PACKAGES=1 (PEP 668 / Debian)"
        if ! PIP_BREAK_SYSTEM_PACKAGES="${PIP_BREAK_SYSTEM_PACKAGES:-1}" \
             python3 -m pip install --user toml; then
          die "could not install 'toml'. Install it manually: python3 -m pip install --user toml" 1
        fi
      fi
      python3 -c 'import toml' >/dev/null 2>&1 \
        || die "'toml' is still not importable after installation." 1
      info "python 'toml' package installed"
      ;;
    *)
      warn "continuing without 'toml': polyglot.toml project-file support is limited"
      ;;
  esac
fi

# 3. C backend: clang or gcc (POLYGLOT_CC wins, mirroring the driver)
BACKEND_LABEL=""
if [ -n "${POLYGLOT_CC:-}" ] && command -v "${POLYGLOT_CC}" >/dev/null 2>&1; then
  BACKEND_LABEL="clang/LLVM backend via POLYGLOT_CC=${POLYGLOT_CC}"
else
  for c in clang clang-21 clang-20 clang-19 clang-18 clang-17 gcc cc; do
    if command -v "$c" >/dev/null 2>&1; then
      case "$c" in
        clang*) BACKEND_LABEL="clang/LLVM backend ($("$c" --version 2>/dev/null | head -n 1))" ;;
        *)      BACKEND_LABEL="GCC backend ($("$c" --version 2>/dev/null | head -n 1))" ;;
      esac
      break
    fi
  done
fi
if [ -n "$BACKEND_LABEL" ]; then
  info "compiler found: $BACKEND_LABEL"
else
  warn "no clang or gcc found on PATH; builds will fail until a C compiler is installed or POLYGLOT_CC points at one"
fi

# ---------------------------------------------------------------------------
# Install
# ---------------------------------------------------------------------------

# Package files that ship next to the launcher (keep in sync with the repo).
POLYGLOT_LIB_FILES=(
  __init__.py
  cli.py
  toolchain.py
  backends.py
  cache.py
  plugin_client.py
  output.py
  config.py
  commands/__init__.py
  commands/analyze.py
  commands/build.py
  commands/check.py
  commands/clean.py
  commands/deps.py
  commands/doctor.py
  commands/emit_asm.py
  commands/emit_ir.py
  commands/fmt.py
  commands/init.py
  commands/jit.py
  commands/mca.py
  commands/plugin.py
  commands/preproc.py
  commands/repl.py
  commands/run.py
  commands/stats.py
  commands/targets.py
  commands/tidy.py
  commands/version.py
  commands/watch.py
)

# curl | bash: download the driver ourselves (stdin belongs to the script).
fetch_url() {
  # fetch_url URL DEST
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$1" -o "$2" || return 1
  elif command -v wget >/dev/null 2>&1; then
    wget -qO "$2" "$1" || return 1
  else
    die "need curl or wget to download 'polyglot'" 1
  fi
}

TMP_SRC=""
TMP_LIB=""
if [ -z "$SCRIPT_SRC" ]; then
  TMP_SRC="$(mktemp)"
  TMP_LIB="$(mktemp -d)"
  info "downloading 'polyglot' from $RAW_URL/polyglot"
  fetch_url "$RAW_URL/polyglot" "$TMP_SRC" || die "download failed (curl)" 1
  info "downloading the 'polyglot_lib' package from $RAW_URL/polyglot_lib"
  for rel in "${POLYGLOT_LIB_FILES[@]}"; do
    dest="$TMP_LIB/polyglot_lib/$rel"
    mkdir -p -- "$(dirname -- "$dest")"
    fetch_url "$RAW_URL/polyglot_lib/$rel" "$dest" \
      || die "download failed: polyglot_lib/$rel" 1
  done
  SCRIPT_SRC="$TMP_SRC"
  LIB_SRC="$TMP_LIB/polyglot_lib"
fi

# Sanity check: every installed file must be valid Python (catches truncated
# downloads).
PYC_DIR="$(mktemp -d)"
check_syntax() {
  python3 -c 'import py_compile, sys; py_compile.compile(sys.argv[1], cfile=sys.argv[2] + "/check.pyc", doraise=True)' \
       "$1" "$PYC_DIR" >/dev/null 2>&1
}
cleanup_tmp() {
  rm -rf -- "$PYC_DIR"
  if [ -n "$TMP_SRC" ]; then rm -f -- "$TMP_SRC"; fi
  if [ -n "$TMP_LIB" ]; then rm -rf -- "$TMP_LIB"; fi
}
if ! check_syntax "$SCRIPT_SRC"; then
  cleanup_tmp
  die "'polyglot' failed a python syntax check; source is corrupt" 1
fi
while IFS= read -r pyfile; do
  if ! check_syntax "$pyfile"; then
    cleanup_tmp
    die "'$pyfile' failed a python syntax check; source is corrupt" 1
  fi
done < <(find "$LIB_SRC" -name '*.py' -type f | sort)

mkdir -p -- "$BIN_DIR"
cp -- "$SCRIPT_SRC" "$TARGET"
chmod +x -- "$TARGET"
rm -rf -- "$LIB_TARGET"
cp -R -- "$LIB_SRC" "$LIB_TARGET"
# do not ship bytecode caches from the source tree
find "$LIB_TARGET" -type d -name '__pycache__' -prune -exec rm -rf -- {} +
cleanup_tmp
info "installed $TARGET"
info "installed $LIB_TARGET"
info "installed: $("$TARGET" version 2>/dev/null | head -n 1 || echo 'polyglot (version unknown)')"

# Warn (with the exact fix) when the install dir is not on PATH.
case ":$PATH:" in
  *":$BIN_DIR:"*) : ;;
  *)
    warn "$BIN_DIR is not on your PATH"
    printf '\nAdd it to your shell profile:\n\n  export PATH="%s:$PATH"\n\n' "$BIN_DIR"
    ;;
esac

# Final environment report (non-strict: missing optional tools only degrade).
info "running 'polyglot doctor' (non-strict) ..."
if "$TARGET" doctor; then
  :
else
  warn "polyglot doctor exited non-zero; see the report above"
fi

info "done. Try: $TARGET --help"
