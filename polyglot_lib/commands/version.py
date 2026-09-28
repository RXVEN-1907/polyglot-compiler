"""The 'version' command: show the driver version and the backend found
(so far); runs without requiring a toolchain."""

import argparse

from polyglot_lib import VERSION
from polyglot_lib.commands import command

ORDER = 200


@command("version", require=False)
def cmd_version(tc, args):
    print("polyglot %s" % VERSION)
    if tc.cc_path is None:
        print("backend: no C compiler found")
    else:
        print("backend: %s" % tc.describe())
    print("run polyglot doctor for details")


def register(subparsers, global_flags):
    subparsers.add_parser("version", help="Show version and backend")
