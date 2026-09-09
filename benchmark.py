#!/usr/bin/env python3
"""
Benchmark script for Polyglot compiler.
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
        self.env["PATH"] = "/home/jupyter-24b11cs489@adityau-1219b/llvm-install/clang+llvm-17.0.6-x86_64-linux-gnu-ubuntu-22.04/bin:" + self.env["PATH"]

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
            self.run(["--opt-level", opt, "build", "tests/fixtures/hello.c", "-o", "bench_size.out"])
            size = self.get_binary_size("bench_size.out")
            print(f"  {opt:15s}: {size:8d} bytes")
            Path("bench_size.out").unlink(missing_ok=True)

        print("\n4. INCREMENTAL COMPILATION (simulated)")
        print("-" * 40)
        # First build
        times, _ = self.time_command(["build", "tests/fixtures/functions.c", "-o", "inc_test.out"])
        if times:
            first = statistics.mean(times) * 1000
            print(f"  First build:     {first:7.2f} ms")

            # Subsequent builds (simulating incremental - just rebuild)
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

        print("\n" + "=" * 60)
        print("BENCHMARK COMPLETE")
        print("=" * 60)


if __name__ == "__main__":
    os.chdir(Path(__file__).parent)
    bench = Benchmark()
    bench.run_benchmarks()