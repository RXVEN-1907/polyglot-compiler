"""Shared utilities for binary inspection commands."""

import subprocess
from pathlib import Path
from typing import Dict, Any, List, Optional, Tuple

from polyglot_lib.output import PolyglotError
from polyglot_lib.toolchain import Toolchain


def _run_inspection_tool(tc: Toolchain, tool_name: str, paths: List[Path],
                        opt_level: Optional[str] = None,
                        extra_flags: Optional[List[str]] = None) -> Dict[str, Any]:
    """Run a specific inspection tool and return structured results.

    Args:
        tc: Toolchain instance
        tool_name: Name of the tool ("nm", "size", "objdump", "readelf")
        paths: List of file paths to inspect
        opt_level: Optimization level (optional)
        extra_flags: Additional flags to pass to the tool (optional)

    Returns:
        Dictionary with tool execution results
    """
    # Map tool names to their actual commands
    tool_map = {
        "nm": "llvm-nm",
        "size": "llvm-size",
        "objdump": "llvm-objdump",
        "readelf": "llvm-readelf",
    }

    if tool_name not in tool_map:
        raise PolyglotError(f"Unsupported inspection tool: {tool_name}")

    tool_path = tc.tool_or_error(tool_map[tool_name], f"running {tool_name} inspection")

    # Build command based on tool
    cmd = [str(tool_path)]

    # Add common flags based on tool
    if tool_name == "size":
        cmd.extend(["-A"])  # Berkeley format by default
        if opt_level:
            # Size doesn't take opt level directly, but we could pass through compiler opts
            pass
    elif tool_name == "nm":
        cmd.extend(["-P"])  # POSIX format for easier parsing
    elif tool_name == "objdump":
        cmd.extend(["-h"])  # Section headers
    elif tool_name == "readelf":
        cmd.extend(["-S"])  # Section headers

    # Add extra flags if provided
    if extra_flags:
        cmd.extend(extra_flags)

    # Add file paths
    cmd.extend([str(p) for p in paths])

    try:
        result = subprocess.run(cmd, capture_output=True, text=True, check=True)
        return {
            "tool": tool_name,
            "command": " ".join(cmd),
            "stdout": result.stdout,
            "stderr": result.stderr,
            "return_code": result.returncode,
            "success": True
        }
    except subprocess.CalledProcessError as e:
        return {
            "tool": tool_name,
            "command": " ".join(cmd),
            "stdout": e.stdout,
            "stderr": e.stderr,
            "return_code": e.returncode,
            "success": False
        }
    except OSError as e:
        return {
            "tool": tool_name,
            "command": " ".join(cmd),
            "error": str(e),
            "success": False
        }


def _parse_nm_output(output: str) -> Dict[str, Any]:
    """Parse NM output into structured format."""
    symbols = []
    lines = output.strip().split('\n') if output.strip() else []

    for line in lines:
        if line.strip():
            parts = line.split()
            if len(parts) >= 3:
                try:
                    symbol = {
                        "value": parts[0] if parts[0] != '0000000000000000' and parts[0] != 'U' else None,
                        "type": parts[1],
                        "name": ' '.join(parts[2:])
                    }
                    symbols.append(symbol)
                except (ValueError, IndexError):
                    pass
            elif len(parts) == 2:
                try:
                    symbol = {
                        "value": None,
                        "type": parts[0],
                        "name": parts[1]
                    }
                    symbols.append(symbol)
                except (ValueError, IndexError):
                    pass

    return {
        "tool": "nm",
        "symbols": symbols,
        "symbol_count": len(symbols)
    }


def _parse_size_output(output: str) -> Dict[str, Any]:
    """Parse size output into structured format."""
    lines = output.strip().split('\n') if output.strip() else []
    files = []

    for line in lines:
        if line.strip() and not line.startswith('   '):  # Skip header lines
            parts = line.split()
            if len(parts) >= 6:
                try:
                    file_data = {
                        "file": parts[-1],
                        "text": int(parts[-6]) if parts[-6].isdigit() else 0,
                        "data": int(parts[-5]) if parts[-5].isdigit() else 0,
                        "bss": int(parts[-4]) if parts[-4].isdigit() else 0,
                        "total": int(parts[-3]) if parts[-3].isdigit() else 0
                    }
                    files.append(file_data)
                except (ValueError, IndexError):
                    # If parsing fails, store raw line
                    files.append({"raw_line": line})

    return {
        "tool": "size",
        "files": files
    }


def _parse_objdump_output(output: str) -> Dict[str, Any]:
    """Parse objdump output into structured format."""
    # Simplified parsing
    lines = output.strip().split('\n') if output.strip() else []
    sections = []
    current_section = None

    for line in lines:
        if line.startswith("Disassembly of section"):
            if current_section:
                sections.append(current_section)
            current_section = line.split()[-1].strip(':')
        elif current_section and line.strip() and not line.startswith(' '):
            # End of section
            sections.append(current_section)
            current_section = None

    if current_section:
        sections.append(current_section)

    return {
        "tool": "objdump",
        "sections": sections,
        "section_count": len(sections)
    }


def _parse_readelf_output(output: str) -> Dict[str, Any]:
    """Parse readelf output into structured format."""
    # Simplified parsing
    lines = output.strip().split('\n') if output.strip() else []
    sections = []
    symbols = []

    in_section_header = False
    in_symbol_table = False

    for line in lines:
        if "Section Headers:" in line:
            in_section_header = True
            in_symbol_table = False
        elif "Symbol table" in line:
            in_symbol_table = True
            in_section_header = False
        elif in_section_header and line.startswith(' ['):
            # Section header line
            parts = line.split()
            if len(parts) >= 6:
                sections.append({
                    "index": parts[0].strip('[]'),
                    "name": parts[1],
                    "type": parts[2],
                    "address": parts[3] if len(parts) > 3 else "",
                    "size": parts[4] if len(parts) > 4 else ""
                })
        elif in_symbol_table and line.startswith(' ['):
            # Symbol table line
            parts = line.split()
            if len(parts) >= 8:
                symbols.append({
                    "index": parts[0].strip('[]'),
                    "value": parts[1],
                    "size": parts[2],
                    "type": parts[3],
                    "bind": parts[4],
                    "vis": parts[5],
                    "ndx": parts[6],
                    "name": ' '.join(parts[7:]) if len(parts) > 7 else ""
                })

    return {
        "tool": "readelf",
        "sections": sections,
        "symbols": symbols,
        "section_count": len(sections),
        "symbol_count": len(symbols)
    }


def get_parser_for_tool(tool_name: str):
    """Get the appropriate parser function for a tool name.

    Args:
        tool_name: Name of the tool ("nm", "size", "objdump", "readelf")

    Returns:
        Parser function for the tool
    """
    parsers = {
        "nm": _parse_nm_output,
        "size": _parse_size_output,
        "objdump": _parse_objdump_output,
        "readelf": _parse_readelf_output,
    }
    return parsers.get(tool_name)