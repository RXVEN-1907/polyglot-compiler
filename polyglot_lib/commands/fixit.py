"""The 'fixit' command: automatically apply safe fixes to compilation errors."""

import argparse
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

from polyglot_lib.commands import command
from polyglot_lib.output import (
    EXIT_FAILURE, EXIT_USAGE, MissingToolError, PolyglotError,
)

ORDER = 160  # Place after preview (155) and before less-used commands


def _extract_fixit_hints(stderr):
    """Extract actionable fix-it hints from compiler stderr.

    Returns list of (filename, line_num, original_line, suggested_fix, description)
    """
    hints = []
    lines = stderr.split('\n')

    i = 0
    while i < len(lines):
        line = lines[i]

        # Look for error patterns that include fix-it suggestions
        # Pattern: filename:line:col: error: message
        # Followed by: line content
        # Followed by: pointer line with ^
        # Followed by: suggestion message

        error_match = re.match(r'^(.+?):(\d+):(\d+):\s+error:\s+(.+)$', line)
        if error_match:
            filename, line_num, col, message = error_match.groups()
            line_num = int(line_num)

            # Get the source line (next non-empty line that's not a pointer or note)
            source_line = ""
            pointer_line = ""
            suggestion = ""

            j = i + 1
            while j < len(lines) and j < i + 5:  # Look ahead a few lines
                next_line = lines[j]
                if next_line.startswith(filename + ':') and 'error:' in next_line:
                    break  # Next error
                if not source_line and next_line.strip():
                    # Check if this is a GCC-style line with line number and pipe
                    if re.match(r'^\s*\d+\s+\|', next_line):
                        if ' | ' in next_line:
                            parts = next_line.split(' | ', 1)
                            content = parts[1]      # e.g., "     return 0"
                            if content.strip():
                                source_line = content.rstrip()
                    else:
                        # Non-GCC format (like clang)
                        source_line = next_line.rstrip()
                elif re.match(r'^\s*\d+\s+\|', next_line):
                    # This might be the pointer line
                    if ' | ' in next_line:
                        parts = next_line.split(' | ', 1)
                        content = parts[1]
                        if re.match(r'[\^~\s]*$', content):
                            pointer_line = content.rstrip()
                elif re.match(r'^\s*[\^~]+', next_line):
                    # Non-GCC format pointer line
                    pointer_line = next_line.rstrip()
                elif 'note:' in next_line.lower() or 'warning:' in next_line.lower():
                    # Extract suggestion from note/warning
                    note_match = re.search(r'note:\s*(.+)$', next_line, re.IGNORECASE)
                    if note_match:
                        suggestion = note_match.group(1).strip()
                j += 1

            # Parse common fixable errors
            fix_applicable, suggested_content = _parse_fixable_error(
                message, source_line, pointer_line, suggestion, filename, line_num
            )

            if fix_applicable and suggested_content is not None:
                hints.append({
                    'filename': filename,
                    'line_num': line_num,
                    'original_line': source_line,
                    'suggested_line': suggested_content,
                    'description': message,
                    'note': suggestion
                })

        i += 1

    return hints


def _parse_fixable_error(message, source_line, pointer_line, suggestion, filename, line_num):
    """Parse error message to determine if we can apply a safe fix."""

    # Missing semicolon at end of line
    if ("expected ';' before" in message or
        "expected ‘;’ before" in message or
        "expected ';' at end of input" in message or
        "expected ‘;’ at end of input" in message):
        if source_line and not source_line.rstrip().endswith(';'):
            # Add semicolon at the end, but only if it looks like a statement
            stripped = source_line.rstrip()
            if any(stripped.endswith(op) for op in [')', ']', '}']) or \
               any(kw in stripped for kw in ['return', 'break', 'continue']) or \
               stripped.startswith('return') or '(' in stripped:
                return True, stripped + ';'

    # Missing semicolon in for loop
    if "expected ';' before" in message and 'for' in source_line:
        # Try to fix common for loop issues
        if source_line.count(';') < 2:  # for(init; condition; increment)
            # This is complex to fix safely, skip for now
            pass

    # Missing brace
    if "expected '{' at end of input" in message or "expected '}'" in message:
        # This is too context-sensitive to fix safely without parsing
        pass

    # Missing parenthesis
    if ("expected ')' before" in message or "expected ‘)” before" in message or
        "expected ')' at end of input" in message or "expected ‘)” at end of input" in message):
        if source_line:
            # Count parentheses to see if we're missing a closing one
            open_count = source_line.count('(')
            close_count = source_line.count(')')
            if open_count > close_count:
                needed = open_count - close_count
                return True, source_line + ')' * needed

    # Missing bracket
    if ("expected ']' before" in message or "expected ‘］* before" in message or
        "expected ']' at end of input" in message or "expected ‘］* at end of input" in message):
        if source_line:
            open_count = source_line.count('[')
            close_count = source_line.count(']')
            if open_count > close_count:
                needed = open_count - close_count
                return True, source_line + ']' * needed

    # Suggested include from note
    if suggestion and ('#include' in suggestion or 'include' in suggestion.lower()):
        # Extract suggested include
        include_match = re.search(r'[<"]([^>"]+)[>"]', suggestion)
        if include_match:
            include_directive = f'#include {include_match.group(0)}'
            # Add at the top of the file (we'll handle this separately)
            return ('include', include_directive)

    # Suggestion to add return statement
    if ("control reaches end of non-void function" in message or
        "control reaches end of non-void function" in message):
        if source_line.strip() == '}':
            # We're at the closing brace, suggest adding return before it
            # This needs more context, skip for safety
            pass
        elif source_line and not source_line.strip().startswith('return'):
            # Look for a simple fix: add return 0; before closing brace
            # This is function-specific, skip for now
            pass

    return False, None


def _apply_fixes_to_file(filename, fixes):
    """Apply fixes to a file. Returns (success, backup_path, diff_summary)."""
    filepath = Path(filename)
    if not filepath.exists():
        return False, None, "File not found"

    # Create backup
    backup = filepath.with_suffix(filepath.suffix + '.polyglot-backup')
    shutil.copy2(filepath, backup)

    try:
        content = filepath.read_text(encoding='utf-8')
        lines = content.splitlines()

        # Sort fixes by line number descending to avoid index shifting
        fixes_sorted = sorted(fixes, key=lambda x: x['line_num'], reverse=True)

        applied_fixes = []
        for fix in fixes_sorted:
            line_idx = fix['line_num'] - 1  # Convert to 0-based
            if 0 <= line_idx < len(lines):
                original = lines[line_idx]
                suggested = fix['suggested_line']

                # Verify the line matches what we expect (accounting for possible whitespace)
                if original.rstrip() == fix['original_line'].rstrip():
                    lines[line_idx] = suggested
                    applied_fixes.append(fix)
                else:
                    # Line has changed, skip this fix to be safe
                    pass

        # Write the fixed content
        new_content = '\n'.join(lines)
        if content.endswith('\n'):
            new_content += '\n'
        filepath.write_text(new_content, encoding='utf-8')

        # Generate diff summary
        diff_summary = f"Applied {len(applied_fixes)} fix(es) to {filename}"
        for fix in applied_fixes:
            diff_summary += f"\n  Line {fix['line_num']}: {fix['description']}"

        return True, backup, diff_summary

    except Exception as e:
        # Restore backup on error
        if backup.exists():
            shutil.move(backup, filepath)
        return False, None, f"Failed to apply fixes: {e}"


@command("fixit", pass_through=False)
def cmd_fixit(tc, args):
    """Automatically apply safe fixes to compilation errors."""
    if not args.paths:
        raise PolyglotError("fixit: no input files", EXIT_USAGE)

    # Validate that files exist
    for path_str in args.paths:
        if not Path(path_str).exists():
            raise PolyglotError(f"fixit: no such file: {path_str}", EXIT_FAILURE)

    # Try to compile to get error output
    # We'll reuse the build command's logic but capture stderr
    from polyglot_lib.toolchain import Toolchain
    from polyglot_lib.output import run_cmd

    # Create a temporary args object for compilation
    import types
    compile_args = types.SimpleNamespace(
        std=getattr(args, 'std', None),
        warn=getattr(args, 'warn', 'default'),
        werror=getattr(args, 'werror', False),
        debug=getattr(args, 'debug', False),
        sanitize=getattr(args, 'sanitize', None),
        includes=getattr(args, 'includes', []),
        defines=getattr(args, 'defines', []),
        cc_args=getattr(args, 'cc_args', []),
        opt_level=getattr(args, 'opt_level', 'default'),
        target=getattr(args, 'target', None),
        shared=getattr(args, 'shared', False),
        static_lib=getattr(args, 'static_lib', False),
        output=getattr(args, 'output', None),
        quiet=getattr(args, 'quiet', False),
        verbose=getattr(args, 'verbose', False),
        no_cache=getattr(args, 'no_cache', False),
        jobs=getattr(args, 'jobs', 1),
        _project_dir=None,
    )

    all_fixes = {}
    failed_files = []

    for path_str in args.paths:
        filepath = Path(path_str)
        try:
            # Determine compiler and get compilation command
            compiler = tc.compiler_for([filepath])
            if compiler is None:
                failed_files.append((filepath, "No compiler found"))
                continue

            # Build compilation command (similar to build.py but for single file)
            flags = []
            # Add common compile args (simplified version)
            std = compile_args.std
            if not std:
                # Determine default std based on file extension
                is_cxx = filepath.suffix in ['.cpp', '.cxx', '.cc', '+', '.C', '.cp']
                std = 'c++17' if is_cxx else 'c17'
            flags.append(f'-std={std}')

            # Add warning flags
            warn_map = {
                'none': [],
                'default': [],
                'all': ['-Wall'],
                'extra': ['-Wall', '-Wextra'],
                'pedantic': ['-Wall', '-Wextra', '-Wpedantic'],
            }
            warn = getattr(args, 'warn', 'default') or 'default'
            flags.extend(warn_map.get(warn, warn_map['default']))

            if compile_args.werror:
                flags.append('-Werror')
            if compile_args.debug:
                flags.append('-g')

            # Add sanitizers
            if compile_args.sanitize:
                sanitizers = [s.strip() for s in compile_args.sanitize.split(',') if s.strip()]
                if sanitizers:
                    flags.append('-fsanitize=' + ','.join(sanitizers))

            # Add includes
            for inc in compile_args.includes:
                flags.extend(['-I', str(inc)])

            # Add defines
            for define in compile_args.defines:
                flags.append('-D' + define)

            # Add optimization
            opt_level = compile_args.opt_level or 'default'
            opt_map = {
                'none': '-O0',
                'less': '-O1',
                'default': '-O2',
                'aggressive': '-O3',
                'size': '-Os',
                'size-aggressive': '-Oz',
            }
            opt = opt_map.get(opt_level, '-O2')
            if opt == '-Oz' and tc.kind == 'gcc':
                opt = '-Os'  # GCC fallback
            flags.append(opt)

            # Add target
            target = compile_args.target
            if target and tc.kind == 'clang':
                flags.append(f'--target={target}')

            # Add raw cc args
            for raw in compile_args.cc_args:
                flags.append(raw)

            # Output to temporary file to avoid overwriting original
            import tempfile
            with tempfile.NamedTemporaryFile(suffix=filepath.suffix, delete=False) as tmp:
                tmp_path = tmp.name

            flags.extend(['-c', str(filepath), '-o', tmp_path])

            # Run compilation and capture stderr
            cmd = [str(compiler)] + flags
            result = subprocess.run(cmd, capture_output=True, text=True)

            # Clean up temp file
            try:
                os.unlink(tmp_path)
            except OSError:
                pass

            if result.returncode == 0:
                # No errors, nothing to fix
                print(f"{filepath}: no errors found")
                continue

            # Extract fix-it hints from stderr
            hints = _extract_fixit_hints(result.stderr)

            # Filter to only the fixes we can apply with high confidence
            applicable_fixes = []
            for hint in hints:
                fix_type, suggested_content = _parse_fixable_error(
                    hint['description'],
                    hint['original_line'],
                    '',  # pointer_line not used in current extraction
                    hint.get('note', ''),
                    hint['filename'],
                    hint['line_num']
                )

                if fix_type is True and suggested_content is not None:
                    applicable_fixes.append({
                        'filename': hint['filename'],
                        'line_num': hint['line_num'],
                        'original_line': hint['original_line'],
                        'suggested_line': suggested_content,
                        'description': hint['description']
                    })
                elif fix_type == 'include':
                    # Handle include suggestions separately
                    applicable_fixes.append({
                        'filename': hint['filename'],
                        'line_num': 1,  # Will be added at top
                        'original_line': '',
                        'suggested_line': suggested_content,
                        'description': f"Add missing include: {suggested_content}",
                        'is_include': True
                    })

            if applicable_fixes:
                all_fixes[filepath] = applicable_fixes
            else:
                # No applicable fixes, show the error
                print(f"{filepath}:")
                print(result.stderr.rstrip())

        except Exception as e:
            failed_files.append((filepath, str(e)))

    # Report failures
    for filepath, error in failed_files:
        print(f"fixit: {filepath}: {error}", file=sys.stderr)

    if not all_fixes and not failed_files:
        print("All files compiled successfully - no fixes needed")
        return

    # If we have fixes to apply, show them and ask for confirmation unless --yes
    if all_fixes and not getattr(args, 'yes', False):
        print("\nProposed fixes:")
        for filepath, fixes in all_fixes.items():
            print(f"\n{filepath}:")
            for fix in fixes:
                if fix.get('is_include'):
                    print(f"  Line {fix['line_num']}: {fix['description']}")
                else:
                    print(f"  Line {fix['line_num']}:")
                    print(f"    Before: {fix['original_line']}")
                    print(f"    After:  {fix['suggested_line']}")
                    print(f"    Reason: {fix['description']}")

        response = input("\nApply these fixes? [y/N] ").strip().lower()
        if response not in ('y', 'yes'):
            print("Fixes aborted.")
            return

    # Apply the fixes
    for filepath, fixes in all_fixes.items():
        # Separate regular fixes from include fixes (which need special handling)
        regular_fixes = [f for f in fixes if not f.get('is_include')]
        include_fixes = [f for f in fixes if f.get('is_include')]

        success = True
        backup_path = None
        diff_summary = ""

        # Apply regular fixes
        if regular_fixes:
            success, backup_path, diff_summary = _apply_fixes_to_file(str(filepath), regular_fixes)

        # Apply include fixes (add to top of file)
        if success and include_fixes:
            try:
                content = filepath.read_text(encoding='utf-8')
                lines = content.splitlines()

                # Add includes after any existing includes or at the top
                insert_pos = 0
                # Skip initial comments and whitespace to find first real line
                for i, line in enumerate(lines):
                    stripped = line.strip()
                    if stripped and not stripped.startswith('//') and not stripped.startswith('/*'):
                        if stripped.startswith('#include'):
                            insert_pos = i + 1
                        else:
                            break

                # Insert the include directives
                for inc_fix in reversed(include_fixes):  # Reverse to maintain order
                    lines.insert(insert_pos, inc_fix['suggested_line'])

                new_content = '\n'.join(lines)
                if content.endswith('\n'):
                    new_content += '\n'
                filepath.write_text(new_content, encoding='utf-8')

                if backup_path is None:
                    # Create backup if we didn't already
                    backup = filepath.with_suffix(filepath.suffix + '.polyglot-backup')
                    shutil.copy2(filepath, backup)
                    backup_path = backup

                for inc_fix in include_fixes:
                    diff_summary += f"\n  Added include: {inc_fix['suggested_line']}"

            except Exception as e:
                success = False
                diff_summary = f"Failed to add includes: {e}"

        if success:
            print(f"✓ Fixed {filepath}")
            if getattr(args, 'verbose', False):
                print(diff_summary)
        else:
            print(f"✗ Failed to fix {filepath}: {diff_summary}", file=sys.stderr)

            # Restore backup if we made one
            if backup_path and backup_path.exists():
                try:
                    shutil.move(backup_path, filepath)
                    print(f"  Restored original {filepath}")
                except Exception as e:
                    print(f"  Failed to restore backup: {e}", file=sys.stderr)


def register(subparsers, global_flags):
    p = subparsers.add_parser(
        "fixit",
        help="Automatically apply safe fixes to compilation errors",
        description="Analyze compilation errors and apply common fixes like missing semicolons, brackets, etc."
    )
    p.add_argument("paths", nargs="+", help="source files to check and fix")
    p.add_argument(
        "--yes", "-y",
        action="store_true",
        help="apply fixes without prompting for confirmation"
    )
    p.add_argument(
        "--verbose", "-v",
        action="store_true",
        help="show detailed information about applied fixes"
    )
    # Add common flags that affect compilation
    global_flags.add_common_flags(p, link=False)