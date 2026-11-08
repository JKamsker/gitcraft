"""Native b4 entrypoint with a narrowly gated GUI confirmation, not a mail transport.

Supported source contract: mricon/b4 ce4c1158edb292caca0ce4a6087e5edecc1b0371,
version 0.17-dev. Refuse other builds instead of guessing their prompt semantics.
"""

SOURCE = {
    "__init__.py": "51b3fb51100eaeb3a9db83fa1950f5d6384d9359c7ee4fd4804c30d6b5f8f008",
    "command.py": "b0cd9e08fa1788f5f8d957bb8cf77dce095d7d2f004a4774298b35798c76dcde",
    "ez.py": "93a4f4a4edc9adb4707e0b09423bb19510591aa7e8671497cc4a7e225c954873",
    "mbox.py": "1d676cf48487c2eced20b0a1416ec86aeea4736a168079e498fddb80cc68e132",
}
FINAL_PROMPT = "Press Enter to proceed or Ctrl-C to abort"


def check_contract(version, sources):
    import hashlib

    if version != "0.17-dev" or set(sources) != set(SOURCE):
        raise RuntimeError("Install the supported b4 source build ce4c1158 (0.17-dev)")
    for name, expected in SOURCE.items():
        if hashlib.sha256(sources[name]).hexdigest() != expected:
            raise RuntimeError("b4 source differs from supported ce4c1158: " + name)


def native_arguments(mode, args):
    if not args:
        raise RuntimeError("Missing native b4 operation")
    if mode == "confirmed-send":
        # Only the existing GUI's final confirmed send path can opt into answering.
        if args[0] != "send" or len(args) % 2 != 1:
            raise RuntimeError("Invalid confirmed send invocation")
        for index in range(1, len(args), 2):
            if args[index] not in ("--to", "--cc"):
                raise RuntimeError("Unexpected confirmed send option")
    elif mode != "no-send":
        raise RuntimeError("Unknown confirmation mode")
    elif args[0] == "send" and "-o" not in args[1:]:
        raise RuntimeError("Unconfirmed invocation cannot send mail")
    return ["b4", "--no-interactive", "--no-stdin", "-c", "b4.save-maildirs=no"] + args


def confirm_prompt(mode, prompt):
    if mode == "confirmed-send" and prompt == FINAL_PROMPT:
        return ""
    # In particular, the separate failing-preflight prompt must never be accepted.
    raise KeyboardInterrupt("Native b4 requires review: " + str(prompt))


def main():
    import builtins
    import pathlib
    import sys
    import b4
    import b4.command
    import b4.ez
    import b4.mbox

    modules = {"__init__.py": b4, "command.py": b4.command, "ez.py": b4.ez, "mbox.py": b4.mbox}
    sources = {}
    for name, module in modules.items():
        with pathlib.Path(module.__file__).open("rb") as source:
            sources[name] = source.read(2 * 1024 * 1024 + 1)
    check_contract(b4.__VERSION__, sources)
    mode = sys.argv[1]
    args = native_arguments(mode, sys.argv[2:])
    builtins.input = lambda prompt="": confirm_prompt(mode, prompt)
    sys.argv = args
    b4.command.cmd()


if __name__ == "__main__":
    main()
