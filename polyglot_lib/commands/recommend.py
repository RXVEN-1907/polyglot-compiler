"""The 'recommend' command: provide optimization and configuration recommendations."""

import json
import os
import subprocess
import sys
from pathlib import Path
from typing import Dict, List, Any, Optional

from polyglot_lib import VERSION
from polyglot_lib.commands import command
from polyglot_lib.output import (
    EXIT_FAILURE, EXIT_USAGE, PolyglotError, green, red, yellow,
)
from polyglot_lib.plugin_client import resolve_plugin
from polyglot_lib.toolchain import Toolchain


ORDER = 18  # Early in help output, after build but before inspection commands


def _analyze_build_performance(tc, args, sources):
    """Analyze build performance characteristics."""
    recommendations = []

    # Check if we're using optimization
    opt_level = getattr(args, "opt_level", "default")
    if opt_level == "none":
        recommendations.append({
            "category": "performance",
            "priority": "high",
            "suggestion": "Consider using -O2 or -O3 optimization level for better performance",
            "explanation": "Current optimization level is 'none'. Adding optimization can significantly improve execution speed.",
            "command_example": "polyglot build --opt-level=2 <sources>"
        })
    elif opt_level in ["less", "default"]:
        recommendations.append({
            "category": "performance",
            "priority": "medium",
            "suggestion": "Consider trying higher optimization levels (-O3 or --opt-level=aggressive) for better performance",
            "explanation": f"Current optimization level is '{opt_level}'. Higher levels may provide better performance for compute-intensive code.",
            "command_example": "polyglot build --opt-level=aggressive <sources>"
        })

    # Check for debug flags that might impact performance
    if getattr(args, "debug", False):
        recommendations.append({
            "category": "performance",
            "priority": "medium",
            "suggestion": "Remove -g/--debug flag for production builds to improve performance",
            "explanation": "Debug information increases binary size and can slightly impact performance.",
            "command_example": "polyglot build <sources>  # without --debug"
        })

    return recommendations


def _analyze_binary_characteristics(tc, args, sources):
    """Analyze binary characteristics for optimization opportunities."""
    recommendations = []

    # This would ideally run inspection tools, but for simplicity we'll check what we can
    # In a full implementation, this would integrate with the inspect command

    # Check if we're building static vs shared libraries
    if getattr(args, "static_lib", False):
        recommendations.append({
            "category": "size",
            "priority": "low",
            "suggestion": "Consider if shared libraries would be more appropriate for your use case",
            "explanation": "Static libraries can lead to larger executables due to code duplication.",
            "command_example": "polyglot build --shared <sources>  # for shared library"
        })
    elif getattr(args, "shared", False):
        recommendations.append({
            "category": "performance",
            "priority": "low",
            "suggestion": "Verify that shared libraries are actually needed for your application",
            "explanation": "Shared libraries have loading overhead; static linking can be faster for simple cases.",
            "command_example": "polyglot build --static-lib <sources>  # for static library"
        })

    return recommendations


def _run_inspection_analysis(tc, sources):
    """Run inspection tools and analyze results for recommendations."""
    recommendations = []

    # Convert sources to existing files only
    existing_sources = [s for s in sources if s.exists() and s.is_file()]
    if not existing_sources:
        return recommendations

    try:
        # Import inspection functions locally to avoid circular dependencies
        from polyglot_lib.commands.inspect import (
            _run_inspection_tool,
            _parse_tool_output,
            _correlate_inspection_results,
            _generate_summary
        )

        # Run key inspection tools
        tools_to_run = ["nm", "size"]
        inspection_results = []

        for tool in tools_to_run:
            result = _run_inspection_tool(tc, tool, existing_sources)
            if result.get("success", False):
                parsed_data = _parse_tool_output(tool, result["stdout"])
                inspection_results.append({
                    "tool": tool,
                    "data": parsed_data,
                    "success": True
                })
            else:
                inspection_results.append({
                    "tool": tool,
                    "error": result.get("error", "Unknown error"),
                    "success": False
                })

        # Correlate results if we have successful inspections
        successful_results = [r for r in inspection_results if r["success"]]
        if successful_results:
            # Prepare data for correlation
            files_data = {}
            for result in successful_results:
                tool = result["tool"]
                data = result["data"]

                # Organize data by file (simplified for now)
                if tool == "nm":
                    # NM data applies to all sources
                    for src in existing_sources:
                        file_key = str(src.resolve())
                        if file_key not in files_data:
                            files_data[file_key] = {}
                        files_data[file_key][tool] = data
                elif tool == "size":
                    # Size data is file-specific
                    size_files = data.get("files", [])
                    for size_file in size_files:
                        if isinstance(size_file, dict) and "file" in size_file:
                            file_path = Path(size_file["file"]).resolve()
                            file_key = str(file_path)
                            if file_key not in files_data:
                                files_data[file_key] = {}
                            # Remove file key for storage
                            file_info = {k: v for k, v in size_file.items() if k != "file"}
                            files_data[file_key][tool] = file_info

            if files_data:
                correlated = _correlate_inspection_results(files_data)
                summary = _generate_summary(files_data)

                # Generate recommendations based on analysis
                recommendations.extend(_generate_insight_based_recommendations(correlated, summary))

    except Exception as e:
        # If inspection fails, continue with basic recommendations
        pass

    return recommendations


def _generate_insight_based_recommendations(correlated_data, summary):
    """Generate recommendations based on correlated inspection data."""
    recommendations = []

    # File size analysis
    total_size = summary.get("total_size", 0)
    if total_size > 0:
        # Size-based recommendations
        if total_size > 1024 * 1024:  # > 1MB
            recommendations.append({
                "category": "size",
                "priority": "medium",
                "suggestion": "Consider enabling size optimization (-Os or -Oz) to reduce binary size",
                "explanation": f"Binary size is {total_size} bytes ({total_size / 1024:.1f} KB). Size optimization can significantly reduce footprint.",
                "command_example": "polyglot build --opt-level=small <sources>"
            })
        elif total_size < 1024:  # < 1KB
            recommendations.append({
                "category": "size",
                "priority": "info",
                "suggestion": "Binary size is already very small",
                "explanation": f"Binary size is {total_size} bytes, which is quite compact.",
                "command_example": ""
            })

    # Symbol analysis
    total_symbols = summary.get("total_symbols", 0)
    if total_symbols > 0:
        if total_symbols > 1000:
            recommendations.append({
                "category": "performance",
                "priority": "medium",
                "suggestion": "Consider using visibility hidden or -fvisibility=hidden to reduce symbol count",
                "explanation": f"Binary has {total_symbols} symbols, which may impact loading time and memory usage. Hidden visibility can help.",
                "command_example": "polyglot build --flag=-fvisibility=hidden <sources>"
            })
        elif total_symbols < 10:
            recommendations.append({
                "category": "performance",
                "priority": "info",
                "suggestion": "Symbol count is low, indicating good encapsulation",
                "explanation": f"Binary has only {total_symbols} symbols, suggesting good code organization.",
                "command_example": ""
            })

    # Add general insight about using inspection tools
    recommendations.append({
        "category": "analysis",
        "priority": "info",
        "suggestion": "Run 'polyglot inspect <binary>' for detailed binary analysis",
        "explanation": "The inspect command provides comprehensive analysis using nm, size, objdump, and readelf tools.",
        "command_example": "polyglot inspect <binary-file>"
    })

    return recommendations


def _check_available_plugins(args, sources):
    """Check for potentially useful plugins based on source file characteristics."""
    recommendations = []
    plugin_dir = getattr(args, "plugin_dir", None) or "~/.polyglot/plugins"

    # Determine what languages we're dealing with
    has_c = any(str(s).endswith(('.c', '.h')) for s in sources)
    has_cpp = any(str(s).endswith(('.cpp', '.cc', '.cxx', '.c++', '.hpp', '.hxx')) for s in sources)

    if not (has_c or has_cpp):
        return recommendations  # No C/C++ sources, skip plugin check

    # Check if a plugin is already specified
    explicit_plugin = getattr(args, "plugin", None)
    if explicit_plugin:
        try:
            plugin_path = resolve_plugin(explicit_plugin, plugin_dir)
            recommendations.append({
                "category": "plugin",
                "priority": "info",
                "suggestion": f"Using specified plugin: {explicit_plugin}",
                "explanation": f"Plugin resolved to: {plugin_path}",
                "command_example": f"polyglot --plugin={explicit_plugin} build <sources>"
            })
        except PolyglotError as e:
            recommendations.append({
                "category": "plugin",
                "priority": "high",
                "suggestion": f"Specified plugin '{explicit_plugin}' not found or not usable",
                "explanation": str(e),
                "command_example": f"polyglot plugin install <plugin-source>  # then retry"
            })
    else:
        # No plugin specified, check if we should recommend one
        # In a full implementation, we'd analyze the code to see what optimizations might benefit
        # For now, we'll check if any plugins are installed and suggest using them
        try:
            # Try to resolve a generic clang plugin as an example
            plugin_path = resolve_plugin("clang", plugin_dir)
            recommendations.append({
                "category": "plugin",
                "priority": "medium",
                "suggestion": "Consider using a plugin for potential performance improvements",
                "explanation": "Polyglot supports plugins that can provide language-specific optimizations. "
                             "A clang plugin is available and could be used with --plugin=clang",
                "command_example": "polyglot --plugin=clang build <sources>"
            })
        except PolyglotError:
            # No clang plugin available, suggest installing one
            recommendations.append({
                "category": "plugin",
                "priority": "low",
                "suggestion": "Consider installing plugins for enhanced functionality",
                "explanation": "No plugins found. Plugins can provide additional optimizations and features.",
                "command_example": "polyglot plugin install <plugin-source>"
            })

    return recommendations


def _check_toolchain_health(tc):
    """Check toolchain health and provide recommendations."""
    recommendations = []

    # Check for missing essential tools
    if tc.cc_path is None:
        recommendations.append({
            "category": "toolchain",
            "priority": "critical",
            "suggestion": "Install a C compiler (clang or gcc)",
            "explanation": "No C compiler found. The build command requires a C compiler to function.",
            "command_example": "Install clang: sudo apt-get install clang  # or equivalent for your system"
        })

    if tc.cxx_path is None:
        recommendations.append({
            "category": "toolchain",
            "priority": "high",
            "suggestion": "Install a C++ compiler for C++ support",
            "explanation": "No C++ compiler found. C++ source files cannot be compiled.",
            "command_example": "Install clang++: sudo apt-get install clang++  # or equivalent"
        })

    # Check for useful auxiliary tools
    from polyglot_lib.toolchain import AUX_TOOLS
    missing_tools = []
    for tool_name, purpose in AUX_TOOLS.items():
        if tc.find_tool(tool_name) is None:
            missing_tools.append((tool_name, purpose))

    if missing_tools:
        tool_list = ", ".join([f"{name} ({purpose})" for name, purpose in missing_tools[:3]])
        if len(missing_tools) > 3:
            tool_list += f" and {len(missing_tools) - 3} more"
        recommendations.append({
            "category": "toolchain",
            "priority": "medium",
            "suggestion": "Consider installing additional toolchain components for full functionality",
            "explanation": f"Missing tools: {tool_list}. These tools provide additional inspection and analysis capabilities.",
            "command_example": "Install LLVM tools: sudo apt-get install llvm  # or equivalent"
        })

    return recommendations


def _analyze_project_configuration(project_dir, config):
    """Analyze project configuration for improvement opportunities."""
    recommendations = []

    if config is None:
        return recommendations

    # Check if project uses polyglot.toml effectively
    if not config:
        recommendations.append({
            "category": "configuration",
            "priority": "low",
            "suggestion": "Consider adding a polyglot.toml file for project configuration",
            "explanation": "A polyglot.toml file allows you to configure project-specific settings, "
                         "including compiler preferences, plugins, and custom build targets.",
            "command_example": "Create polyglot.toml with [project] section"
        })
    else:
        # Check for unused configuration sections
        lang_config = config.get("language", {})
        if lang_config and not lang_config.get("rust"):
            # Not a Rust project, check if we could benefit from plugins
            plugins_config = config.get("plugins", {})
            if not plugins_config:
                recommendations.append({
                    "category": "configuration",
                    "priority": "low",
                    "suggestion": "Consider adding plugins configuration to polyglot.toml",
                    "explanation": "The [plugins] section in polyglot.toml allows automatic plugin selection "
                                 "based on your project needs.",
                    "command_example": "Add [plugins] section to polyglot.toml"
                })

    return recommendations


def _generate_recommendations(tc, args, sources, project_dir=None, config=None):
    """Generate all recommendations."""
    recommendations = []

    # Analyze different aspects
    recommendations.extend(_analyze_build_performance(tc, args, sources))
    recommendations.extend(_analyze_binary_characteristics(tc, args, sources))
    recommendations.extend(_run_inspection_analysis(tc, sources))
    recommendations.extend(_check_available_plugins(args, sources))
    recommendations.extend(_check_toolchain_health(tc))
    if project_dir and config:
        recommendations.extend(_analyze_project_configuration(project_dir, config))

    # Sort by priority: critical > high > medium > low > info
    priority_order = {"critical": 0, "high": 1, "medium": 2, "low": 3, "info": 4}
    recommendations.sort(key=lambda x: priority_order.get(x["priority"], 5))

    return recommendations


def _print_recommendations(recommendations, as_json=False):
    """Print recommendations in the specified format."""
    if as_json:
        print(json.dumps(recommendations, indent=2))
        return

    if not recommendations:
        print("No recommendations available.")
        return

    print("=" * 70)
    print("POLYGLOT RECOMMENDATIONS")
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
        "critical": "CRITICAL ISSUES",
        "toolchain": "TOOLCHAIN RECOMMENDATIONS",
        "performance": "PERFORMANCE OPTIMIZATIONS",
        "size": "SIZE OPTIMIZATIONS",
        "plugin": "PLUGIN RECOMMENDATIONS",
        "configuration": "CONFIGURATION IMPROVEMENTS",
        "analysis": "ANALYSIS INSIGHTS"
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
    print(f"Generated {len(recommendations)} recommendations.")
    print("Apply suggestions that are relevant to your use case.")
    print("=" * 70)


@command("recommend")
def cmd_recommend(tc, args):
    """Provide optimization and configuration recommendations."""
    # Resolve inputs similar to build command
    paths = [Path(p) for p in (args.paths or ["."])]
    output = Path(args.output) if getattr(args, "output", None) else None

    # Simple input resolution for recommendation purposes
    sources = []
    if not paths or paths == [Path(".")]:
        # Look for source files in current directory
        for ext in [".c", ".cpp", ".cc", ".cxx", ".c++", ".h", ".hpp", ".hxx"]:
            sources.extend(Path(".").glob(f"*{ext}"))
            sources.extend(Path(".").glob(f"*{ext.upper()}"))
        if not sources:
            sources = [Path(".")]  # Fall back to directory scanning
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

    # Try to get project configuration
    project_dir = None
    config = None
    try:
        from polyglot_lib.config import find_project_dir
        project_dir, config = find_project_dir([str(p) for p in sources])
    except Exception:
        pass  # Continue without project config

    # Generate recommendations
    recommendations = _generate_recommendations(tc, args, sources, project_dir, config)

    # Output results
    as_json = getattr(args, "json", False)
    _print_recommendations(recommendations, as_json)


def register(subparsers, global_flags):
    p = subparsers.add_parser(
        "recommend",
        help="Provide optimization and configuration recommendations")
    p.add_argument("paths", nargs="*", help="source files or directories to analyze")
    p.add_argument("-o", "--output", help="output file (for future use)")
    p.add_argument("--json", action="store_true",
                   help="output in JSON format for programmatic use")
    # Add common flags that affect recommendations
    global_flags.add_common_flags(p, link=False)