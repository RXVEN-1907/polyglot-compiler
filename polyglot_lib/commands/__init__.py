"""Command registry for the polyglot driver.

Every submodule in this package defines:

  * a handler taking ``(tc, args)``, decorated with ``@command(...)`` so it
    lands in the module-level COMMANDS/HANDLERS mappings, and
  * ``register(subparsers, global_flags)`` which adds its argparse
    subparser and flags, and
  * ``ORDER`` (optional, default: last) controlling the position of the
    command in help output and registration.

The package auto-imports every submodule via pkgutil, so adding a command
never requires editing polyglot_lib.cli - drop a module here and it is
registered automatically.
"""

import importlib
import pkgutil

#: command name -> {"handler", "toolchain", "defer", "require",
#: "pass_through"}; "toolchain" False means the handler is invoked with
#: tc=None (no toolchain discovery at all), "defer"/"require" shape the
#: Toolchain() construction otherwise.
COMMANDS = {}

#: command name -> handler (flat dispatch mapping).
HANDLERS = {}


def command(name, *, toolchain=True, defer=False, require=True,
            pass_through=False):
    """Register the decorated handler as the implementation of `name`.

    toolchain=False  handler receives tc=None (nothing is discovered)
    defer=True       Toolchain discovery is deferred (build reads
                     polyglot.toml first and calls ensure_discovered)
    require=False    missing toolchains degrade instead of exiting 3
    pass_through=True  tokens after '--' belong to the child program
    """
    def deco(fn):
        COMMANDS[name] = {
            "handler": fn,
            "toolchain": toolchain,
            "defer": defer,
            "require": require,
            "pass_through": pass_through,
        }
        HANDLERS[name] = fn
        return fn
    return deco


def _import_all():
    """Import every sibling module and return them in registration order."""
    modules = []
    for m in pkgutil.iter_modules(__path__):
        mod = importlib.import_module("%s.%s" % (__name__, m.name))
        if mod not in modules:
            modules.append(mod)
    modules.sort(key=lambda m: getattr(m, "ORDER", 1 << 30))
    return modules


#: register(subparsers, global_flags) callables in presentation order.
REGISTRARS = [m.register for m in _import_all() if hasattr(m, "register")]
