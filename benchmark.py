#!/usr/bin/env python3
"""
Benchmark script for Polyglot compiler.

Toolchain discovery is left to the driver (POLYGLOT_CC/POLYGLOT_CXX, --compiler,
PATH, then scan dirs); this script only passes the environment through.
"""

import os
import subprocess
import sys
import time
import statistics
from pathlib import Path


class Benchmark:
    def __init__(self):
        self.polyglot = Path(__file__).parent / "polyglot"
        self.env = os.environ.copy()

    def run(self, args):
        cmd = [sys.executable, str(self.polyglot)] + args
        start = time.perf_counter()
        result = subprocess.run(cmd, capture_output=True, text=True, env=self.env)
        elapsed = time.perf_counter() - start
        return result, elapsed

    def time_command(self, args, iterations=5):
        times = []
        for _ in range(iterations):
            result, elapsed = self.run(args)
            if result.returncode != 0:
                return None, f"Command failed: {result.stderr}"
            times.append(elapsed)
        return times, None

    @staticmethod
    def _cache_status(stdout):
        """Extract the driver's cache MISS/HIT line from verbose output."""
        for line in stdout.splitlines():
            if "cache MISS" in line:
                return "cache MISS"
            if "cache HIT" in line:
                return "cache HIT"
        return "cache status unknown"

    def get_binary_size(self, path):
        return Path(path).stat().st_size

    def run_benchmarks(self):
        print("=" * 60)
        print("POLYGLOT BENCHMARK RESULTS")
        print("=" * 60)

        # Test programs
        programs = [
            ("hello.c", "Hello World"),
            ("arithmetic.c", "Arithmetic"),
            ("functions.c", "Functions"),
            ("conditionals.c", "Conditionals"),
            ("loops.c", "Loops"),
            ("structs.c", "Structs"),
            ("pointers.c", "Pointers"),
        ]

        print("\n1. COMPILATION TIME (O2)")
        print("-" * 40)
        for fname, desc in programs:
            times, err = self.time_command(["--opt-level", "default", "build", f"tests/fixtures/{fname}", "-o", f"bench_{fname}.out"])
            if err:
                print(f"  {desc:20s}: FAILED - {err}")
            else:
                avg = statistics.mean(times) * 1000
                stdev = statistics.stdev(times) * 1000 if len(times) > 1 else 0
                print(f"  {desc:20s}: {avg:7.2f} ms ± {stdev:.2f} ms")
                Path(f"bench_{fname}.out").unlink(missing_ok=True)

        print("\n2. COMPILATION TIME BY OPTIMIZATION LEVEL")
        print("-" * 40)
        opt_levels = ["none", "less", "default", "aggressive", "size", "size-aggressive"]
        for opt in opt_levels:
            times, err = self.time_command(["--opt-level", opt, "build", "tests/fixtures/hello.c", "-o", "bench_opt.out"])
            if err:
                print(f"  {opt:15s}: FAILED - {err}")
            else:
                avg = statistics.mean(times) * 1000
                print(f"  {opt:15s}: {avg:7.2f} ms")
            Path("bench_opt.out").unlink(missing_ok=True)

        print("\n3. BINARY SIZE BY OPTIMIZATION LEVEL")
        print("-" * 40)
        for opt in opt_levels:
            result, _ = self.run(["--opt-level", opt, "build", "tests/fixtures/hello.c", "-o", "bench_size.out"])
            if result.returncode != 0 or not Path("bench_size.out").exists():
                print(f"  {opt:15s}: FAILED - {result.stderr.strip()}")
            else:
                size = self.get_binary_size("bench_size.out")
                print(f"  {opt:15s}: {size:8d} bytes")
            Path("bench_size.out").unlink(missing_ok=True)

        print("\n4. REBUILD WITH NO SOURCE CHANGE (cache-assisted)")
        print("-" * 40)
        # First build
        times, _ = self.time_command(["build", "tests/fixtures/functions.c", "-o", "inc_test.out"])
        if times:
            first = statistics.mean(times) * 1000
            print(f"  First build:     {first:7.2f} ms")

            # Subsequent builds (no source change; the object cache skips the compile)
            times, _ = self.time_command(["build", "tests/fixtures/functions.c", "-o", "inc_test.out"])
            if times:
                subsequent = statistics.mean(times) * 1000
                print(f"  Rebuild (no change): {subsequent:7.2f} ms")
                speedup = first / subsequent if subsequent > 0 else 0
                print(f"  Speedup factor:    {speedup:.2f}x")
            Path("inc_test.out").unlink(missing_ok=True)

        print("\n5. MULTI-FILE BUILD")
        print("-" * 40)
        times, _ = self.time_command(["build", "tests/fixtures/multi_file_main.c", "tests/fixtures/math_utils.c", "-o", "multi.out"])
        if times:
            avg = statistics.mean(times) * 1000
            print(f"  Multi-file build: {avg:7.2f} ms")
            Path("multi.out").unlink(missing_ok=True)

        print("\n6. IR/ASM EMISSION TIME")
        print("-" * 40)
        times, _ = self.time_command(["emit-ir", "tests/fixtures/hello.c", "-o", "ir_test.ll"])
        if times:
            avg = statistics.mean(times) * 1000
            print(f"  IR emission:   {avg:7.2f} ms")
            Path("ir_test.ll").unlink(missing_ok=True)

        times, _ = self.time_command(["emit-asm", "tests/fixtures/hello.c", "-o", "asm_test.s"])
        if times:
            avg = statistics.mean(times) * 1000
            print(f"  ASM emission:  {avg:7.2f} ms")
            Path("asm_test.s").unlink(missing_ok=True)

        print("\n7. CACHED REBUILD (cache MISS vs cache HIT)")
        print("-" * 40)
        # Build a nonce'd copy of hello.c twice with the cache enabled: the
        # unique nonce guarantees the first build is a cache MISS (real
        # compile) and the second a cache HIT (link only). Report what the
        # driver actually did.
        src = Path("bench_cache_src.c")
        src.write_text(Path("tests/fixtures/hello.c").read_text() + f"\n/* bench nonce {time.time()} */\n")
        try:
            first, t_miss = self.run(["-v", "build", str(src), "-o", "bench_cache.out"])
            if first.returncode != 0:
                print(f"  FAILED - {first.stderr.strip()}")
            else:
                print(f"  First build: {t_miss * 1000:7.2f} ms  ({self._cache_status(first.stdout)})")
                second, t_hit = self.run(["-v", "build", str(src), "-o", "bench_cache.out"])
                if second.returncode != 0:
                    print(f"  Rebuild FAILED - {second.stderr.strip()}")
                else:
                    print(f"  Rebuild:     {t_hit * 1000:7.2f} ms  ({self._cache_status(second.stdout)})")
                    if t_hit < t_miss and "cache HIT" in second.stdout:
                        speedup = t_miss / t_hit
                        print(f"  Speedup:     {speedup:.2f}x  (compile skipped; the cached build still links)")
                    else:
                        print("  NOTE: rebuild was not faster; the cached build still links the object.")
        finally:
            src.unlink(missing_ok=True)
            Path("bench_cache.out").unlink(missing_ok=True)

        print("\n8. PARALLEL MULTI-FILE BUILD (-j 1 vs -j 2)")
        print("-" * 40)
        # --no-cache forces a full recompile of both TUs on every run, so the
        # comparison measures per-TU parallelism rather than cache warmth.
        pair = ["tests/fixtures/multi_file_main.c", "tests/fixtures/math_utils.c"]
        j1, err1 = self.time_command(["build"] + pair + ["--no-cache", "-j", "1", "-o", "bench_par.out"], iterations=3)
        j2, err2 = self.time_command(["build"] + pair + ["--no-cache", "-j", "2", "-o", "bench_par.out"], iterations=3)
        if err1 or err2:
            print(f"  FAILED - {err1 or err2}")
        else:
            avg1 = statistics.mean(j1) * 1000
            avg2 = statistics.mean(j2) * 1000
            print(f"  -j 1 (serial):   {avg1:7.2f} ms")
            print(f"  -j 2 (parallel): {avg2:7.2f} ms")
            if avg2 < avg1:
                print(f"  Parallel speedup: {avg1 / avg2:.2f}x")
            else:
                print("  NOTE: -j 2 was not faster here (only 2 small TUs; process startup can dominate).")
        Path("bench_par.out").unlink(missing_ok=True)

        print("\n" + "=" * 60)
        print("BENCHMARK COMPLETE")
        print("=" * 60)


if __name__ == "__main__":
    os.chdir(Path(__file__).parent)
    bench = Benchmark()
    bench.run_benchmarks()
