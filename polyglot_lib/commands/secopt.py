"""The 'secopt' command: security-aware optimization analysis and recommendations."""

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path
from typing import Dict, List, Any, Optional

from polyglot_lib.commands import command
from polyglot_lib.output import (
    EXIT_FAILURE, EXIT_USAGE, PolyglotError, green, red, yellow,
)
from polyglot_lib.toolchain import Toolchain


ORDER = 22  # After recommend (18) but before inspection commands


def _check_security_sanitizer_support(tc):
    """Check which security sanitizers are supported by the current toolchain."""
    supported = []
    all_sanitizers = ["address", "undefined", "thread", "memory", "leak"]

    for sanitizer in all_sanitizers:
        # Create a temporary test program to check if sanitizer is supported
        import tempfile
        with tempfile.NamedTemporaryFile(suffix=".c", delete=False) as tmp:
            tmp.write(b"int main(){return 0;}")
            tmp.flush()

            try:
                # Try to compile with the sanitizer
                flags = ["-fsanitize=" + sanitizer, "-o", tmp.name + ".out"]
                compiler = tc.compiler_for([Path(tmp.name)])
                if compiler is None:
                    continue

                cmd = [str(compiler)] + flags
                result = subprocess.run(cmd, capture_output=True, text=True, timeout=5)

                if result.returncode == 0:
                    supported.append(sanitizer)
            except (subprocess.TimeoutExpired, FileNotFoundError, OSError):
                pass
            finally:
                # Clean up
                try:
                    os.unlink(tmp.name)
                    out_file = tmp.name + ".out"
                    if os.path.exists(out_file):
                        os.unlink(out_file)
                except OSError:
                    pass

    return supported


def _analyze_security_issues(tc, args, sources):
    """Analyze source files for potential security issues using available tools."""
    recommendations = []

    # Check what security sanitizers are available
    supported_sanitizers = _check_security_sanitizer_support(tc)

    # Recommend enabling appropriate sanitizers for development/testing
    if "address" in supported_sanitizers:
        recommendations.append({
            "category": "security",
            "priority": "high",
            "suggestion": "Enable AddressSanitizer for development/testing to detect buffer overflows and use-after-free",
            "explanation": "AddressSanitizer detects memory errors like buffer overflows, use-after-free, and stack-buffer overflows.",
            "command_example": "polyglot build --sanitize=address <sources>"
        })

    if "undefined" in supported_sanitizers:
        recommendations.append({
            "category": "security",
            "priority": "high",
            "suggestion": "Enable UndefinedBehaviorSanitizer to detect undefined behavior that could lead to security vulnerabilities",
            "explanation": "UndefinedBehaviorSanitizer detects various undefined behaviors that can be exploited for security attacks.",
            "command_example": "polyglot build --sanitize=undefined <sources>"
        })

    if "thread" in supported_sanitizers:
        recommendations.append({
            "category": "security",
            "priority": "medium",
            "suggestion": "Enable ThreadSanitizer to detect data races and thread safety issues",
            "explanation": "ThreadSanitizer helps detect race conditions and other thread-related bugs that can lead to security vulnerabilities.",
            "command_example": "polyglot build --sanitize=thread <sources>"
        })

    # Check for format string vulnerabilities with specific flags
    if tc.kind == "clang":
        recommendations.append({
            "category": "security",
            "priority": "medium",
            "suggestion": "Enable format string security warnings (-Wformat-security)",
            "explanation": "Helps detect potential format string vulnerabilities in printf/scanf functions.",
            "command_example": "polyglot build --warn=extra --cc-arg=-Wformat-security <sources>"
        })

    # Recommend stack protection
    if tc.kind in ["clang", "gcc"]:
        recommendations.append({
            "category": "security",
            "priority": "medium",
            "suggestion": "Enable stack protection (-fstack-protector-strong) to detect stack buffer overflows",
            "explanation": "Stack protection adds guards to detect stack buffer overflows that could lead to code execution attacks.",
            "command_example": "polyglot build --cc-arg=-fstack-protector-strong <sources>"
        })

    # Recommend position independent executables for better ASLR
    if tc.kind == "clang":
        recommendations.append({
            "category": "security",
            "priority": "low",
            "suggestion": "Consider building as position independent executable (-pie) for better address space layout randomization",
            "explanation": "PIE executables benefit from ASLR making certain types of attacks more difficult.",
            "command_example": "polyglot build --cc-arg=-pie <sources>"
        })

    # Recommend fortify source
    if tc.kind in ["clang", "gcc"]:
        recommendations.append({
            "category": "security",
            "priority": "medium",
            "suggestion": "Enable fortify source (_FORTIFY_SOURCE=2) for compile-time checks against buffer overflows",
            "explanation": "Fortify source adds checks to certain library functions to prevent buffer overflow exploits.",
            "command_example": "polyglot build --cc-arg=-D_FORTIFY_SOURCE=2 <sources>"
        })

    return recommendations


def _analyze_optimization_security_tradeoffs(tc, args, sources):
    """Analyze how different optimization levels affect security."""
    recommendations = []

    # Check if we're using aggressive optimizations that might affect security
    opt_level = getattr(args, "opt_level", "default")
    if opt_level in ["aggressive", "size-aggressive"]:
        recommendations.append({
            "category": "security",
            "priority": "medium",
            "suggestion": "Consider that aggressive optimizations (-O3, -Oz) may sometimes remove security-related code",
            "explanation": "Some compiler optimizations can inadvertently remove security checks. Consider using -Og for debugging with security features.",
            "command_example": "polyglot build --opt-level=default --sanitize=address,undefined <sources>"
        })

    # Check if debug info is enabled (can be security-sensitive in production)
    if getattr(args, "debug", False):
        recommendations.append({
            "category": "security",
            "priority": "low",
            "suggestion": "Remove debug information (-g) from production builds to avoid leaking sensitive information",
            "explanation": "Debug information can contain symbols and other data that might be useful to attackers.",
            "command_example": "polyglot build <sources>  # without --debug"
        })

    return recommendations


def _generate_security_recommendations(tc, args, sources):
    """Generate all security-related recommendations."""
    # Analyze security issues and sanitizer support
    recommendations = []
    recommendations.extend(_analyze_security_issues(tc, args, sources))

    # Analyze optimization/security tradeoffs
    recommendations.extend(_analyze_optimization_security_tradeoffs(tc, args, sources))

    # Sort by priority: critical > high > medium > low > info
    priority_order = {"critical": 0, "high": 1, "medium": 2, "low": 3, "info": 4}
    recommendations.sort(key=lambda x: priority_order.get(x["priority"], 5))

    return recommendations


def _print_security_recommendations(recommendations, as_json=False):
    """Print recommendations in the specified format."""
    if as_json:
        print(json.dumps(recommendations, indent=2))
        return

    if not recommendations:
        print("No security recommendations available.")
        return

    print("=" * 70)
    print("POLYGLOT SECURITY RECOMMENDATIONS")
    print("=" * 70)
    print()

    # Group by category
    categories = {}
    for rec in recommendations:
        cat = rec["category"]
        if cat not in categories:
            categories[cat] = []
        categories[cat].append(rec)

    # Print each category
    category_names = {
        "security": "SECURITY RECOMMENDATIONS",
    }

    for category, recs in categories.items():
        title = category_names.get(category, category.upper())
        print(title)
        print("-" * len(title))

        for rec in recs:
            priority = rec["priority"]
            priority_colors = {
                "critical": red,
                "high": yellow,
                "medium": lambda x: x,  # No color
                "low": lambda x: x,
                "info": lambda x: x
            }
            color_func = priority_colors.get(priority, lambda x: x)
            priority_text = color_func(f"[{priority.upper()}]")

            print(f"{priority_text} {rec['suggestion']}")
            if rec.get("explanation"):
                print(f"    Explanation: {rec['explanation']}")
            if rec.get("command_example"):
                print(f"    Example: {rec['command_example']}")
            print()

    print("=" * 70)
    print(f"Generated {len(recommendations)} security recommendations.")
    print("Apply suggestions that are relevant to your use case.")
    print("=" * 70)


@command("secopt")
def cmd_secopt(tc, args):
    """Provide security-aware optimization analysis and recommendations."""
    # Resolve inputs similar to build command
    paths = [Path(p) for p in (args.paths or ["."])]

    # Simple input resolution for recommendation purposes
    sources = []
    if not paths or paths == [Path(".")]:
        # Look for source files in current directory
        for ext in [".c", ".cpp", ".cc", ".cxx", ".c++", ".h", ".hpp", ".hxx"]:
            sources.extend(Path(".").glob(f"*{ext}"))
            sources.extend(Path(".").glob(f"*{ext.upper()}"))
        # If no sources found with specific extensions, keep sources empty
        # to trigger proper error - don't fall back to directory scanning
    else:
        for f in paths:
            f = Path(f)
            if f.exists():
                if f.is_file():
                    sources.append(f)
                elif f.is_dir():
                    # Recursively find source files in directory
                    for ext in [".c", ".cpp", ".cc", ".cxx", ".c++", ".h", ".hpp", ".hxx"]:
                        sources.extend(f.glob(f"**/*{ext}"))
                        sources.extend(f.glob(f"**/*{ext.upper()}"))

    # Remove duplicates while preserving order
    seen = set()
    unique_sources = []
    for src in sources:
        if src not in seen:
            seen.add(src)
            unique_sources.append(src)
    sources = unique_sources

    if not sources:
        raise PolyglotError("No source files found to analyze", EXIT_USAGE)

    # Generate recommendations
    recommendations = _generate_security_recommendations(tc, args, sources)

    # Output results
    as_json = getattr(args, "json", False)
    _print_security_recommendations(recommendations, as_json)


def register(subparsers, global_flags):
    p = subparsers.add_parser(
        "secopt",
        help="Provide security-aware optimization analysis and recommendations")
    p.add_argument("paths", nargs="*", help="source files or directories to analyze")
    p.add_argument("--json", action="store_true",
                   help="output in JSON format for programmatic use")
    # Add common flags that affect recommendations
    global_flags.add_common_flags(p, link=False)