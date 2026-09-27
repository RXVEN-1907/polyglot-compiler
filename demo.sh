#!/usr/bin/env bash
# demo.sh - Polyglot Compiler Demonstration
# Run this script to see Polyglot in action!

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
MAGENTA='\033[0;35m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m' # No Color

POLYGLOT="./polyglot"

# Check if polyglot exists
if [ ! -f "$POLYGLOT" ]; then
    echo -e "${RED}Error: polyglot not found. Run from project root.${NC}"
    exit 1
fi

# Make sure it's executable
chmod +x "$POLYGLOT"

echo -e "${CYAN}${BOLD}"
cat << 'EOF'
╔══════════════════════════════════════════════════════════════════════════════╗
║                                                                              ║
║   ██████╗ ██████╗ ██████╗ ███████╗██████╗  ██████╗ ███████╗██╗  ██╗         ║
║   ██╔══██╗██╔══██╗██╔══██╗██╔════╝██╔══██╗██╔═══██╗██╔════╝██║  ██║         ║
║   ██████╔╝██████╔╝██████╔╝█████╗  ██████╔╝██║   ██║███████╗███████║         ║
║   ██╔═══╝ ██╔══██╗██╔══██╗██╔══╝  ██╔══██╗██║   ██║╚════██║██╔══██║         ║
║   ██║     ██║  ██║██║  ██║███████╗██║  ██║╚██████╔╝███████║██║  ██║         ║
║   ╚═╝     ╚═╝  ╚═╝╚═╝  ╚═╝╚══════╝╚═╝  ╚═╝ ╚═════╝ ╚══════╝╚═╝  ╚═╝         ║
║                                                                              ║
║        Universal Compiler Driver for C/C++ with Compiler Inspection        ║
║                                                                              ║
╚══════════════════════════════════════════════════════════════════════════════╝
EOF
echo -e "${NC}"

pause() {
    echo -e "${YELLOW}Press Enter to continue...${NC}"
    read -r
}

run_cmd() {
    echo -e "${BLUE}${BOLD}$ $1${NC}"
    eval "$1"
    echo
}

# Demo 1: Version
echo -e "${GREEN}${BOLD}=== Demo 1: Version ===${NC}"
run_cmd "$POLYGLOT version"
pause

# Demo 2: Doctor (environment report)
echo -e "${GREEN}${BOLD}=== Demo 2: Doctor - Toolchain Environment Report ===${NC}"
run_cmd "$POLYGLOT doctor | head -11"
pause

# Demo 3: Build and run hello world
echo -e "${GREEN}${BOLD}=== Demo 3: Build & Run Hello World ===${NC}"
run_cmd "$POLYGLOT run examples/01_hello.c"
pause

# Demo 4: Build flags
echo -e "${GREEN}${BOLD}=== Demo 4: Build Flags (--std / --warn / --debug) ===${NC}"
run_cmd "$POLYGLOT build --std c17 --warn extra --debug examples/01_hello.c -o demo_flags"
pause

# Demo 5: Build with optimization
echo -e "${GREEN}${BOLD}=== Demo 5: Build with Aggressive Optimization ===${NC}"
run_cmd "$POLYGLOT --opt-level aggressive build examples/04_functions.c -o demo_func_O3"
run_cmd "./demo_func_O3"
pause

# Demo 6: Incremental build cache
echo -e "${GREEN}${BOLD}=== Demo 6: Incremental Build Cache (MISS then HIT) ===${NC}"
echo -e "${CYAN}--verbose shows cache HIT/MISS; first build compiles, second reuses the object${NC}"
run_cmd "$POLYGLOT clean"
run_cmd "$POLYGLOT -v build examples/01_hello.c -o demo_cache"
run_cmd "$POLYGLOT -v build examples/01_hello.c -o demo_cache"
pause

# Demo 7: Syntax check
echo -e "${GREEN}${BOLD}=== Demo 7: Syntax Check (Valid) ===${NC}"
run_cmd "$POLYGLOT check examples/01_hello.c examples/02_arithmetic.c"
pause

# Demo 8: Syntax check (invalid)
echo -e "${GREEN}${BOLD}=== Demo 8: Syntax Check (Invalid - Shows Error) ===${NC}"
run_cmd "$POLYGLOT check examples/07_diagnostics/bad_syntax.c || true"
pause

# Demo 9: Emit IR (GIMPLE on the GCC backend)
echo -e "${GREEN}${BOLD}=== Demo 9: Emit IR (GIMPLE on GCC backend, LLVM IR on clang) ===${NC}"
run_cmd "$POLYGLOT emit-ir examples/08_ir_asm/simple.c -o demo.ll"
run_cmd "head -25 demo.ll"
pause

# Demo 10: Emit Assembly
echo -e "${GREEN}${BOLD}=== Demo 10: Emit Assembly ===${NC}"
run_cmd "$POLYGLOT emit-asm examples/08_ir_asm/simple.c -o demo.s"
run_cmd "head -30 demo.s"
pause

# Demo 11: Preprocessed output
echo -e "${GREEN}${BOLD}=== Demo 11: Preprocessed Output (-E) ===${NC}"
run_cmd "$POLYGLOT preproc examples/01_hello.c | head -12"
pause

# Demo 12: Dependency files
echo -e "${GREEN}${BOLD}=== Demo 12: Make-Style Dependencies (-MM) ===${NC}"
run_cmd "$POLYGLOT deps examples/04_functions.c"
pause

# Demo 13: Machine code analysis
echo -e "${GREEN}${BOLD}=== Demo 13: Machine Code Analysis (llvm-mca) ===${NC}"
run_cmd "$POLYGLOT mca examples/01_hello.c | head -10"
pause

# Demo 14: Format check
echo -e "${GREEN}${BOLD}=== Demo 14: Format Check (clang-format) ===${NC}"
run_cmd "$POLYGLOT fmt examples/01_hello.c --check"
pause

# Demo 15: Compilation statistics
echo -e "${GREEN}${BOLD}=== Demo 15: Compilation Statistics ===${NC}"
run_cmd "$POLYGLOT stats examples/04_functions.c --opt-level aggressive"
pause

# Demo 16: Machine-readable statistics
echo -e "${GREEN}${BOLD}=== Demo 16: Machine-Readable Statistics (--json) ===${NC}"
run_cmd "$POLYGLOT stats examples/04_functions.c --json | head -16"
pause

# Demo 17: Cross-compilation targets
echo -e "${GREEN}${BOLD}=== Demo 17: Cross-Compilation Targets ===${NC}"
run_cmd "$POLYGLOT targets"
pause

# Demo 18: Multi-file build
echo -e "${GREEN}${BOLD}=== Demo 18: Multi-File Build ===${NC}"
run_cmd "$POLYGLOT build examples/06_multi_file/main.c examples/06_multi_file/math.c -o demo_multi"
run_cmd "./demo_multi"
pause

# Demo 19: Project initialization
echo -e "${GREEN}${BOLD}=== Demo 19: Project Initialization ===${NC}"
run_cmd "$POLYGLOT init demo-project --language c"
run_cmd "cat demo-project/polyglot.toml"
run_cmd "cat demo-project/src/main.c"
pause

# Demo 20: Build project
echo -e "${GREEN}${BOLD}=== Demo 20: Build Project ===${NC}"
run_cmd "(cd demo-project && ../polyglot build && ./demo-project)"
pause

# Cleanup
echo -e "${GREEN}${BOLD}=== Cleanup ===${NC}"
run_cmd "rm -f demo_func_O3 demo_flags demo_cache demo.ll demo.s demo_multi"
run_cmd "rm -rf demo-project"
run_cmd "$POLYGLOT clean"

echo -e "${CYAN}${BOLD}"
cat << 'EOF'
╔═══════════════════════════════════════════════════════════════════════════════╗
║                                                                              ║
║   Thanks for watching the Polyglot demo!                                    ║
║                                                                              ║
║   Get started:                                                               ║
║   $ git clone https://github.com/RXVEN-1907/polyglot-compiler               ║
║   $ cd polyglot-compiler && chmod +x polyglot && pip install toml           ║
║   $ polyglot run examples/01_hello.c                                        ║
║                                                                              ║
║   Documentation: https://github.com/RXVEN-1907/polyglot-compiler/docs       ║
║   Issues: https://github.com/RXVEN-1907/polyglot-compiler/issues            ║
║   Discussions: https://github.com/RXVEN-1907/polyglot-compiler/discussions  ║
║                                                                              ║
╚═══════════════════════════════════════════════════════════════════════════════╗
EOF
echo -e "${NC}"
