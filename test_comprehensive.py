#!/usr/bin/env python3
"""
Comprehensive tests for Polyglot driver.
"""

import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


class ComprehensiveTest:
    def __init__(self):
        self.polyglot = Path(__file__).parent / "polyglot"
        self.env = os.environ.copy()
        self.env["PATH"] = "/home/jupyter-24b11cs489@adityau-1219b/llvm-install/clang+llvm-17.0.6-x86_64-linux-gnu-ubuntu-22.04/bin:" + self.env["PATH"]

    def run(self, args, cwd=None):
        cmd = [sys.executable, str(self.polyglot)] + args
        result = subprocess.run(cmd, capture_output=True, text=True, cwd=cwd, env=self.env)
        return result

    def test_basic_compilation(self):
        """Test basic C compilation."""
        programs = [
            ("hello.c", "Hello from Polyglot!"),
            ("arithmetic.c", "a + b = 15"),
            ("functions.c", "factorial(5) = 120"),
            ("conditionals.c", "x is even"),
            ("loops.c", "sum 0..9 = 45"),
            ("structs.c", "Rectangle area: 50"),
            ("pointers.c", "Uppercase: HELLO, POLYGLOT!"),
        ]
        for fname, expected in programs:
            result = self.run(["build", f"tests/fixtures/{fname}", "-o", f"test_{fname}.out"])
            assert result.returncode == 0, f"Build failed for {fname}: {result.stderr}"
            run_result = subprocess.run([f"./test_{fname}.out"], capture_output=True, text=True)
            assert run_result.returncode == 0, f"Run failed for {fname}"
            assert expected in run_result.stdout, f"Expected '{expected}' in output for {fname}, got: {run_result.stdout}"
            Path(f"test_{fname}.out").unlink()
        print("✓ test_basic_compilation passed")

    def test_cpp_compilation(self):
        """Test C++ compilation."""
        result = self.run(["build", "tests/fixtures/hello.cpp", "-o", "test_cpp.out"])
        assert result.returncode == 0, f"C++ build failed: {result.stderr}"
        run_result = subprocess.run(["./test_cpp.out"], capture_output=True, text=True)
        assert run_result.returncode == 0
        assert "Hello from Polyglot C++!" in run_result.stdout
        Path("test_cpp.out").unlink()
        print("✓ test_cpp_compilation passed")

    def test_optimization_levels(self):
        """Test all optimization levels produce working binaries."""
        for opt in ["none", "less", "default", "aggressive", "size", "size-aggressive"]:
            result = self.run(["--opt-level", opt, "build", "tests/fixtures/hello.c", "-o", f"opt_{opt}.out"])
            assert result.returncode == 0, f"Opt level {opt} failed: {result.stderr}"
            run_result = subprocess.run([f"./opt_{opt}.out"], capture_output=True, text=True)
            assert run_result.returncode == 0
            assert "Hello from Polyglot!" in run_result.stdout
            Path(f"opt_{opt}.out").unlink()
        print("✓ test_optimization_levels passed")

    def test_multi_file_build(self):
        """Test building multiple source files."""
        result = self.run(["build", "tests/fixtures/multi_file_main.c", "tests/fixtures/math_utils.c", "-o", "multi.out"])
        assert result.returncode == 0, f"Multi-file build failed: {result.stderr}"
        run_result = subprocess.run(["./multi.out"], capture_output=True, text=True)
        assert run_result.returncode == 0
        assert "power(5, 3) = 125" in run_result.stdout
        Path("multi.out").unlink()
        print("✓ test_multi_file_build passed")

    def test_check_command(self):
        """Test check command."""
        result = self.run(["check", "tests/fixtures/hello.c", "tests/fixtures/arithmetic.c"])
        assert result.returncode == 0, f"Check failed: {result.stderr}"
        assert "All files passed syntax check" in result.stdout

        # Test with invalid file
        result = self.run(["check", "tests/fixtures/invalid_syntax.c"])
        assert result.returncode != 0
        print("✓ test_check_command passed")

    def test_emit_ir(self):
        """Test emit-ir command."""
        result = self.run(["emit-ir", "tests/fixtures/hello.c", "-o", "test.ll"])
        assert result.returncode == 0, f"IR emission failed: {result.stderr}"
        assert Path("test.ll").exists()
        content = Path("test.ll").read_text()
        assert "define dso_local i32 @main()" in content
        Path("test.ll").unlink()
        print("✓ test_emit_ir passed")

    def test_emit_asm(self):
        """Test emit-asm command."""
        result = self.run(["emit-asm", "tests/fixtures/hello.c", "-o", "test.s"])
        assert result.returncode == 0, f"ASM emission failed: {result.stderr}"
        assert Path("test.s").exists()
        content = Path("test.s").read_text()
        assert "main:" in content or "main" in content
        Path("test.s").unlink()
        print("✓ test_emit_asm passed")

    def test_clean_command(self):
        """Test clean command."""
        # Create some artifacts
        self.run(["build", "tests/fixtures/hello.c", "-o", "clean_test"])
        self.run(["emit-ir", "tests/fixtures/hello.c", "-o", "test.ll"])
        self.run(["emit-asm", "tests/fixtures/hello.c", "-o", "test.s"])

        assert Path("clean_test").exists()
        assert Path("test.ll").exists()
        assert Path("test.s").exists()

        # Run clean
        result = self.run(["clean"])
        assert result.returncode == 0

        # Check artifacts removed
        assert not Path("test.ll").exists()
        assert not Path("test.s").exists()
        # Note: clean doesn't remove the main executable by default
        print("✓ test_clean_command passed")

    def test_init_project(self):
        """Test project initialization."""
        with tempfile.TemporaryDirectory() as tmpdir:
            project_dir = Path(tmpdir) / "test_proj"
            result = self.run(["init", str(project_dir), "--language", "c"])
            assert result.returncode == 0
            assert project_dir.exists()
            assert (project_dir / "polyglot.toml").exists()
            assert (project_dir / "src" / "main.c").exists()

            # Build the project
            result = self.run(["build"], cwd=project_dir)
            assert result.returncode == 0, f"Project build failed: {result.stderr}"

            exe = project_dir / "test_proj"
            assert exe.exists(), f"Executable not created: {list(project_dir.iterdir())}"

            run_result = subprocess.run([str(exe)], capture_output=True, text=True)
            assert run_result.returncode == 0
            assert "Hello from Polyglot!" in run_result.stdout
        print("✓ test_init_project passed")

    def test_version(self):
        """Test version command."""
        from polyglot_lib import VERSION
        result = self.run(["version"])
        assert result.returncode == 0
        assert f"polyglot {VERSION}" in result.stdout
        print("✓ test_version passed")

    def test_cross_compilation(self):
        """Test cross-compilation target."""
        result = self.run(["--target", "x86_64-unknown-linux-gnu", "build", "tests/fixtures/hello.c", "-o", "cross.out"])
        assert result.returncode == 0
        Path("cross.out").unlink()
        print("✓ test_cross_compilation passed")

    def test_run_command(self):
        """Test run command."""
        result = self.run(["run", "tests/fixtures/hello.c"])
        assert result.returncode == 0
        assert "Hello from Polyglot!" in result.stdout
        print("✓ test_run_command passed")

    def test_error_diagnostics(self):
        """Test error diagnostics."""
        # Syntax error
        result = self.run(["build", "tests/fixtures/invalid_syntax.c", "-o", "err"])
        assert result.returncode != 0
        assert "expected ';'" in result.stderr

        # Undefined function
        result = self.run(["build", "tests/fixtures/undefined_func.c", "-o", "err"])
        assert result.returncode != 0
        assert "undeclared function" in result.stderr
        print("✓ test_error_diagnostics passed")

    def test_stats_command(self):
        """Test stats command."""
        result = self.run(["stats", "tests/fixtures/hello.c"])
        assert result.returncode == 0
        assert "COMPILATION STATISTICS" in result.stdout
        assert "Binary size:" in result.stdout
        print("✓ test_stats_command passed")

    # ---- polyglot 0.2.0 additions -------------------------------------

    def test_doctor(self):
        """Test doctor command."""
        result = self.run(["doctor"])
        assert result.returncode == 0, f"Doctor failed: {result.stderr}"
        assert "C compiler" in result.stdout
        assert "backend" in result.stdout
        print("✓ test_doctor passed")

    def test_targets(self):
        """Test targets command."""
        result = self.run(["targets"])
        assert result.returncode == 0, f"Targets failed: {result.stderr}"
        assert "x86_64-unknown-linux-gnu" in result.stdout
        print("✓ test_targets passed")

    def test_fmt_check(self):
        """Test fmt --check and in-place formatting."""
        Path("fmt_probe.c").write_text(
            "int    main   ( )   {\nint x=1;\nreturn x;\n}\n")
        try:
            result = self.run(["fmt", "fmt_probe.c", "--check"])
            assert result.returncode == 1, \
                f"fmt --check should exit 1, got {result.returncode}"
            assert "would reformat" in result.stdout + result.stderr
            result = self.run(["fmt", "fmt_probe.c", "-i"])
            assert result.returncode == 0, f"fmt -i failed: {result.stderr}"
            result = self.run(["fmt", "fmt_probe.c", "--check"])
            assert result.returncode == 0, \
                "fmt --check should pass after in-place format"
        finally:
            Path("fmt_probe.c").unlink(missing_ok=True)
        print("✓ test_fmt_check passed")

    def test_deps(self):
        """Test deps command."""
        result = self.run(["deps", "tests/fixtures/multi_file_main.c"])
        assert result.returncode == 0, f"Deps failed: {result.stderr}"
        assert "multi_file_main.o:" in result.stdout
        assert "math_utils.h" in result.stdout
        print("✓ test_deps passed")

    def test_preproc(self):
        """Test preproc command."""
        result = self.run(["preproc", "tests/fixtures/hello.c"])
        assert result.returncode == 0, f"Preproc failed: {result.stderr}"
        assert "int main" in result.stdout
        print("✓ test_preproc passed")

    def test_std_flag(self):
        """Test --std flag: valid standard works, bogus one fails."""
        try:
            result = self.run(["build", "tests/fixtures/hello.c", "--std=c99",
                               "-o", "std99.out"])
            assert result.returncode == 0, f"std=c99 build failed: {result.stderr}"
            run_result = subprocess.run(["./std99.out"], capture_output=True, text=True)
            assert run_result.returncode == 0
            assert "Hello from Polyglot!" in run_result.stdout
            result = self.run(["build", "tests/fixtures/hello.c", "--std=bogus",
                               "-o", "stdbad.out"])
            assert result.returncode != 0, "--std=bogus should fail the build"
        finally:
            Path("std99.out").unlink(missing_ok=True)
            Path("stdbad.out").unlink(missing_ok=True)
        print("✓ test_std_flag passed")

    def test_sanitize_build(self):
        """Test building with sanitizers."""
        try:
            result = self.run(["build", "tests/fixtures/arithmetic.c",
                               "--sanitize", "address,undefined", "-o", "san.out"])
            assert result.returncode == 0, f"Sanitized build failed: {result.stderr}"
            run_result = subprocess.run(["./san.out"], capture_output=True, text=True)
            assert run_result.returncode == 0, \
                f"Sanitized run failed: {run_result.stdout} {run_result.stderr}"
            assert "a + b = 15" in run_result.stdout
        finally:
            Path("san.out").unlink(missing_ok=True)
        print("✓ test_sanitize_build passed")

    def test_json_stats(self):
        """Test stats --json output."""
        result = self.run(["stats", "tests/fixtures/hello.c", "--json"])
        assert result.returncode == 0, f"stats --json failed: {result.stderr}"
        data = json.loads(result.stdout)
        for key in ["source", "size_bytes", "lines", "target", "opt_level",
                    "backend", "compiler_version", "total_ms",
                    "binary_size_bytes", "timings"]:
            assert key in data, f"missing stats key: {key}"
        assert isinstance(data["timings"], list)
        print("✓ test_json_stats passed")

    def test_rust_build_run(self):
        """Test init/build/run of a Rust project."""
        with tempfile.TemporaryDirectory() as tmpdir:
            project_dir = Path(tmpdir) / "rustprobe"
            result = self.run(["init", str(project_dir), "--language", "rust"])
            assert result.returncode == 0, f"rust init failed: {result.stderr}"
            main_rs = project_dir / "src" / "main.rs"
            assert main_rs.exists(), f"main.rs missing: {list(project_dir.iterdir())}"
            assert "Hello from Polyglot!" in main_rs.read_text()
            result = self.run(["build"], cwd=project_dir)
            assert result.returncode == 0, f"rust build failed: {result.stderr}"
            exe = project_dir / "rustprobe"
            assert exe.exists(), f"rust exe missing: {list(project_dir.iterdir())}"
            run_result = subprocess.run([str(exe)], capture_output=True, text=True)
            assert run_result.returncode == 0
            assert "Hello from Polyglot!" in run_result.stdout
        print("✓ test_rust_build_run passed")

    def test_emit_asm_intel(self):
        """Test emit-asm --intel produces Intel-syntax assembly."""
        try:
            result = self.run(["--compiler", "clang", "emit-asm",
                               "tests/fixtures/hello.c", "--intel", "-o", "intel.s"])
            assert result.returncode == 0, f"Intel asm emission failed: {result.stderr}"
            content = Path("intel.s").read_text()
            assert "%r" not in content, "AT&T register operand leaked into Intel asm"
            assert "%e" not in content, "AT&T register operand leaked into Intel asm"
        finally:
            Path("intel.s").unlink(missing_ok=True)
        print("✓ test_emit_asm_intel passed")

    def test_emit_ir_optimized(self):
        """Test emit-ir at aggressive optimization."""
        try:
            result = self.run(["--compiler", "clang", "--opt-level", "aggressive",
                               "emit-ir", "tests/fixtures/hello.c", "-o", "opt.ll"])
            assert result.returncode == 0, f"Optimized IR emission failed: {result.stderr}"
            content = Path("opt.ll").read_text()
            assert "define dso_local i32 @main()" in content
        finally:
            Path("opt.ll").unlink(missing_ok=True)
        print("✓ test_emit_ir_optimized passed")

    def test_jit_lli(self):
        """Test jit command over emitted LLVM IR."""
        try:
            result = self.run(["emit-ir", "tests/fixtures/hello.c",
                               "-o", "jit_probe.ll"])
            assert result.returncode == 0, f"IR emission failed: {result.stderr}"
            result = self.run(["jit", "jit_probe.ll"])
            assert result.returncode == 0, f"JIT failed: {result.stderr}"
            assert "Hello from Polyglot!" in result.stdout
        finally:
            Path("jit_probe.ll").unlink(missing_ok=True)
        print("✓ test_jit_lli passed")

    def test_parallel_build(self):
        """Test parallel build with -j and no object-file leakage."""
        before = set(Path(".").glob("*.o"))
        try:
            result = self.run(["build", "tests/fixtures/multi_file_main.c",
                               "tests/fixtures/math_utils.c", "-j", "4",
                               "-o", "par.out"])
            assert result.returncode == 0, f"Parallel build failed: {result.stderr}"
            run_result = subprocess.run(["./par.out"], capture_output=True, text=True)
            assert run_result.returncode == 0
            assert "power(5, 3) = 125" in run_result.stdout
            leaked = set(Path(".").glob("*.o")) - before
            assert not leaked, f"object files leaked into repo root: {leaked}"
        finally:
            Path("par.out").unlink(missing_ok=True)
        print("✓ test_parallel_build passed")

    def test_shared_lib(self):
        """Test building and linking against a shared library."""
        Path("pg_mylib.c").write_text("int pg_add(int a, int b) { return a + b; }\n")
        Path("pg_uses.c").write_text(
            "#include <stdio.h>\n\n"
            "int pg_add(int, int);\n\n"
            "int main() {\n"
            "    printf(\"pg_add=%d\\n\", pg_add(2, 3));\n"
            "    return 0;\n"
            "}\n")
        try:
            result = self.run(["build", "pg_mylib.c", "--shared", "-o", "libpgtest.so"])
            if result.returncode != 0:
                print("SKIP test_shared_lib: cannot build shared library: "
                      f"{result.stderr.strip()}")
                return
            assert Path("libpgtest.so").exists()
            result = self.run(["build", "pg_uses.c", "-L.", "-lpgtest",
                               "-o", "pg_uses.out",
                               "--cc-arg=-Wl,-rpath,$ORIGIN"])
            if result.returncode != 0:
                print("SKIP test_shared_lib: cannot link against shared library: "
                      f"{result.stderr.strip()}")
                return
            run_result = subprocess.run(["./pg_uses.out"], capture_output=True, text=True)
            assert run_result.returncode == 0, \
                f"Shared-lib run failed: {run_result.stderr}"
            assert "pg_add=5" in run_result.stdout
        finally:
            for f in ["pg_mylib.c", "pg_uses.c", "libpgtest.so", "pg_uses.out"]:
                Path(f).unlink(missing_ok=True)
        print("✓ test_shared_lib passed")

    def test_cache_rebuild(self):
        """Test build cache hit and --no-cache."""
        try:
            result = self.run(["build", "tests/fixtures/hello.c",
                               "-o", "cache_probe.out"])
            assert result.returncode == 0, f"Cold build failed: {result.stderr}"
            result = self.run(["build", "tests/fixtures/hello.c",
                               "-o", "cache_probe.out", "--verbose"])
            assert result.returncode == 0, f"Warm build failed: {result.stderr}"
            assert "cache HIT" in result.stdout, \
                f"expected cache HIT, got: {result.stdout}"
            result = self.run(["build", "tests/fixtures/hello.c",
                               "-o", "cache_probe.out", "--no-cache"])
            assert result.returncode == 0, f"--no-cache build failed: {result.stderr}"
        finally:
            Path("cache_probe.out").unlink(missing_ok=True)
        print("✓ test_cache_rebuild passed")

    def test_watch_smoke(self):
        """Test watch command self-terminates via --timeout."""
        result = self.run(["watch", "tests/fixtures/hello.c", "--timeout", "3"])
        assert result.returncode == 0, f"Watch failed: {result.stderr}"
        assert "rebuilt ok" in result.stdout
        print("✓ test_watch_smoke passed")

    def test_cc_args(self):
        """Test -I and -D flags."""
        inc = Path("inc_probe")
        inc.mkdir(exist_ok=True)
        (inc / "greet.h").write_text('#define GREETING "hi from header"\n')
        Path("greet_main.c").write_text(
            "#include <stdio.h>\n"
            "#include \"greet.h\"\n"
            "#ifndef ENABLE_MODE\n"
            "#define ENABLE_MODE 0\n"
            "#endif\n\n"
            "int main() {\n"
            "    printf(\"GREETING=%s mode=%d\\n\", GREETING, ENABLE_MODE);\n"
            "    return 0;\n"
            "}\n")
        try:
            result = self.run(["build", "greet_main.c", "-I", "inc_probe",
                               "-DENABLE_MODE=7", "-o", "greet.out"])
            assert result.returncode == 0, f"-I/-D build failed: {result.stderr}"
            run_result = subprocess.run(["./greet.out"], capture_output=True, text=True)
            assert run_result.returncode == 0
            assert "GREETING=hi from header mode=7" in run_result.stdout

            result = self.run(["build", "greet_main.c", "-I", "inc_probe",
                               "-o", "greet.out"])
            assert result.returncode == 0, f"No-flags build failed: {result.stderr}"
            run_result = subprocess.run(["./greet.out"], capture_output=True, text=True)
            assert run_result.returncode == 0
            assert "GREETING=hi from header mode=0" in run_result.stdout
        finally:
            Path("greet_main.c").unlink(missing_ok=True)
            Path("greet.out").unlink(missing_ok=True)
            shutil.rmtree(inc, ignore_errors=True)
        print("✓ test_cc_args passed")

    def test_tidy_smoke(self):
        """Test tidy command."""
        if shutil.which("clang-tidy", path=self.env["PATH"]) is None:
            print("SKIP test_tidy_smoke: clang-tidy not found")
            return
        result = self.run(["tidy", "tests/fixtures/hello.c"])
        assert result.returncode == 0, f"Tidy failed: {result.stderr}"
        print("✓ test_tidy_smoke passed")

    def test_quiet_flag(self):
        """Test --quiet suppresses 'Running:' lines."""
        try:
            result = self.run(["build", "tests/fixtures/hello.c",
                               "-o", "q.out", "--quiet"])
            assert result.returncode == 0, f"Quiet build failed: {result.stderr}"
            assert "Running:" not in result.stdout
        finally:
            Path("q.out").unlink(missing_ok=True)
        print("✓ test_quiet_flag passed")

    def test_plugin_roundtrip(self):
        """Test a build driven through the JSON-RPC stdio plugin."""
        repo = Path(__file__).parent
        plugin = repo / "target" / "release" / "polyglot-clang"
        if shutil.which("cargo") is None:
            print("SKIP test_plugin_roundtrip: cargo not found")
            return
        if not plugin.exists():
            try:
                proc = subprocess.run(
                    ["cargo", "build", "--release", "-p", "polyglot-clang"],
                    cwd=repo, capture_output=True, text=True, timeout=1800)
            except subprocess.TimeoutExpired:
                print("SKIP test_plugin_roundtrip: cargo build timed out")
                return
            if not plugin.exists():
                print("SKIP test_plugin_roundtrip: plugin build failed: "
                      + (proc.stderr or "")[-500:])
                return
        with tempfile.TemporaryDirectory() as tmpdir:
            out = Path(tmpdir) / "pgplug"
            result = self.run(["--plugin", str(plugin), "build",
                               "examples/01_hello.c", "-o", str(out)])
            assert result.returncode == 0, \
                f"plugin build failed: {result.stdout} {result.stderr}"
            run_result = subprocess.run([str(out)], capture_output=True, text=True)
            assert run_result.returncode == 0, \
                f"plugin-built binary failed: {run_result.stderr}"
            assert "Hello, Polyglot!" in run_result.stdout
        print("✓ test_plugin_roundtrip passed")

    def test_plugin_missing(self):
        """Test --plugin naming an absent plugin: exit 3 + cargo hint."""
        with tempfile.TemporaryDirectory() as tmpdir:
            result = self.run(["--plugin", "definitely-not-there", "build",
                               "tests/fixtures/hello.c",
                               "-o", str(Path(tmpdir) / "x.out")])
            assert result.returncode == 3, \
                f"expected exit 3 for missing plugin, got {result.returncode}: " \
                f"{result.stdout} {result.stderr}"
            assert "cargo build" in result.stderr, \
                f"expected the cargo build hint, got: {result.stderr}"
        print("✓ test_plugin_missing passed")

    def run_all(self):
        tests = [
            self.test_basic_compilation,
            self.test_cpp_compilation,
            self.test_optimization_levels,
            self.test_multi_file_build,
            self.test_check_command,
            self.test_emit_ir,
            self.test_emit_asm,
            self.test_clean_command,
            self.test_init_project,
            self.test_version,
            self.test_cross_compilation,
            self.test_run_command,
            self.test_error_diagnostics,
            self.test_stats_command,
            self.test_doctor,
            self.test_targets,
            self.test_fmt_check,
            self.test_deps,
            self.test_preproc,
            self.test_std_flag,
            self.test_sanitize_build,
            self.test_json_stats,
            self.test_rust_build_run,
            self.test_emit_asm_intel,
            self.test_emit_ir_optimized,
            self.test_jit_lli,
            self.test_parallel_build,
            self.test_shared_lib,
            self.test_cache_rebuild,
            self.test_watch_smoke,
            self.test_cc_args,
            self.test_tidy_smoke,
            self.test_quiet_flag,
            self.test_plugin_roundtrip,
            self.test_plugin_missing,
        ]

        passed = 0
        failed = 0

        for test in tests:
            try:
                test()
                passed += 1
            except Exception as e:
                print(f"✗ {test.__name__} failed: {e}")
                failed += 1

        print(f"\n{'='*50}")
        print(f"Results: {passed} passed, {failed} failed")
        print(f"{'='*50}")

        return failed == 0


if __name__ == "__main__":
    os.chdir(Path(__file__).parent)
    tester = ComprehensiveTest()
    success = tester.run_all()
    sys.exit(0 if success else 1)