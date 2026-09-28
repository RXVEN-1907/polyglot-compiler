"""The 'init' command: scaffold a new polyglot project (c/cpp/rust)."""

import argparse
from pathlib import Path

from polyglot_lib.commands import command
from polyglot_lib.output import EXIT_USAGE, PolyglotError
from polyglot_lib.toolchain import host_triple

ORDER = 180

GITIGNORE_STUB = """# Polyglot build artifacts
*.o
*.obj
*.ll
*.s
*.bc
a.out
.polyglot-cache/
/target/
"""

# appended to .gitignore when a library target is scaffolded
GITIGNORE_LIB_STUB = """\
*.a
*.so
*.dylib
*.dll
"""

LIB_SAMPLE = '''/* Sample library code for {name}. */

int polyglot_add(int a, int b) {{
    return a + b;
}}
'''


@command("init")
def cmd_init(tc, args):
    name = args.name or "my-project"
    language = args.language or "c"
    if args.lib and language == "rust":
        raise PolyglotError(
            "--lib is not supported for rust projects (rust builds use "
            "src/main.rs only)", EXIT_USAGE)
    project_dir = Path(name)
    project_dir.mkdir(exist_ok=True)
    project_name = project_dir.name
    ext = {"c": "c", "cpp": "cpp", "rust": "rs"}.get(language, "c")

    if language == "rust":
        config = f"""[project]
name = "{project_name}"
version = "0.1.0"

[language]
rust = true

[target]
triple = "{host_triple()}"
opt_level = 2

[[bin]]
name = "{project_name}"
source = "src/main.rs"
"""
        sample = '''fn main() {
    println!("Hello from Polyglot!");
}
'''
    else:
        plugin_name = "clang" if language == "cpp" else "rustc"
        config = f"""[project]
name = "{project_name}"
version = "0.1.0"

[target]
triple = "{host_triple()}"
opt_level = 2

[plugins]
{plugin_name} = "0.1"

[[bin]]
name = "{project_name}"
source = "src/main.{ext}"
"""
        if args.lib:
            config += f"""
[[lib]]
name = "{project_name}"
sources = ["src/lib.{ext}"]
kind = "static"
"""
        if language == "cpp":
            sample = '''#include <iostream>

int main() {
    std::cout << "Hello from Polyglot!" << std::endl;
    return 0;
}
'''
        else:
            sample = '''#include <stdio.h>

int main() {
    printf("Hello from Polyglot!\\n");
    return 0;
}
'''

    (project_dir / "polyglot.toml").write_text(config)
    src_dir = project_dir / "src"
    src_dir.mkdir(exist_ok=True)
    (src_dir / ("main." + ext)).write_text(sample)
    gitignore = GITIGNORE_STUB + (GITIGNORE_LIB_STUB if args.lib else "")
    (project_dir / ".gitignore").write_text(gitignore)
    (project_dir / "README.md").write_text(
        "# %s\n\nA Polyglot project (created by `polyglot init --language %s`).\n"
        "\n## Build\n\n    polyglot build\n\n## Run\n\n    polyglot run src/main.%s\n"
        % (project_name, language, ext))

    print("Project created at: %s" % project_dir)
    if args.lib and language != "rust":
        (src_dir / ("lib." + ext)).write_text(
            LIB_SAMPLE.format(name=project_name))
        print("Added [[lib]] target (src/lib.%s, kind=static)." % ext)
    print("Run 'polyglot build' to build.")


def register(subparsers, global_flags):
    p = subparsers.add_parser("init", help="Initialize a new project")
    p.add_argument("name", nargs="?", help="project name")
    p.add_argument("--language", choices=["c", "cpp", "rust"],
                   help="project language")
    p.add_argument("--lib", action="store_true",
                   help="also scaffold a library target ([[lib]] section "
                        "and src/lib source; not for rust)")
