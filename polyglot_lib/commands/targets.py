"""The 'targets' command: list known target triples and availability."""

import argparse
import json
import shutil

from polyglot_lib.commands import command
from polyglot_lib.output import green, yellow
from polyglot_lib.toolchain import KNOWN_TARGETS, host_machine

ORDER = 90


@command("targets")
def cmd_targets(tc, args):
    if getattr(args, "json", False):
        print(json.dumps({"triples": [triple for triple, _ in KNOWN_TARGETS]},
                         indent=2))
        return

    host = host_machine()
    rows = []
    for triple, note in KNOWN_TARGETS:
        arch = triple.split("-")[0]
        if arch == host:
            status, why = green("native"), "this host's architecture"
        elif "wasm" in arch:
            ld = tc.find_tool("wasm-ld") or tc.find_tool("lld")
            if tc.kind == "clang" and ld:
                status, why = green("ok"), "clang cross + %s" % ld.name
            else:
                status, why = yellow("needs-wasm-ld" if tc.kind == "clang"
                                     else "unavailable"), \
                    "wasm linking requires wasm-ld/lld"
        elif tc.kind == "clang":
            status, why = green("ok"), "clang is a cross compiler; link needs " \
                                       "target sysroot/libs"
        else:
            gcc_cross = "%s-linux-gnu-gcc" % arch
            if shutil.which(gcc_cross):
                status, why = green("ok"), "via installed cross toolchain %s" \
                    % gcc_cross
            else:
                status, why = yellow("needs-cross-gcc"), \
                    "GCC requires '%s' (apt install gcc-%s-linux-gnu)" \
                    % (gcc_cross, "aarch64" if arch == "aarch64" else arch)
        rows.append((triple, status, why))

    print("%-34s %-16s %s" % ("TARGET TRIPLE", "STATUS", "NOTES"))
    print("-" * 96)
    for triple, status, why in rows:
        print("%-34s %-16s %s" % (triple, status, why))
    print("\nUse with: polyglot --target TRIPLE build ...")


def register(subparsers, global_flags):
    p = subparsers.add_parser(
        "targets", help="List known target triples and availability")
    p.add_argument("--json", action="store_true",
                   help="machine-readable JSON output")
