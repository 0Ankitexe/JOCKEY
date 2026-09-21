"""Prepare the preserved V1 test loader's fixed local schema path, offline.

No arguments, environment configuration, downloads, schema rewriting or Git
operations. A differing existing destination is never overwritten.
"""

from __future__ import annotations

import sys
from collections.abc import Sequence
from enum import Enum
from pathlib import Path


class Outcome(Enum):
    COPIED = "prepared: legacy AST schema"
    UNCHANGED = "unchanged: legacy AST schema"


class PreparationError(Enum):
    SOURCE_MISSING = ("source-missing", "bundled AST schema is missing")
    SOURCE_UNREADABLE = ("source-unreadable", "bundled AST schema cannot be read")
    DESTINATION_UNREADABLE = ("destination-unreadable", "existing legacy AST schema cannot be read")
    DESTINATION_MISMATCH = (
        "destination-mismatch",
        "existing legacy AST schema differs; refusing overwrite",
    )
    WRITE_FAILURE = ("write-failure", "legacy AST schema could not be prepared")
    UNSAFE_PATH = ("unsafe-path", "schema paths must not traverse symbolic links")
    INVALID_ARGUMENTS = ("invalid-arguments", "this tool accepts no arguments")


def _has_symlink(root: Path, relative: Path) -> bool:
    current = root
    for part in relative.parts:
        current = current / part
        if current.is_symlink():
            return True
    return False


def _existing(destination: Path, contents: bytes) -> Outcome | PreparationError | None:
    try:
        existing = destination.read_bytes()
    except (FileNotFoundError, NotADirectoryError):
        return None
    except OSError:
        return PreparationError.DESTINATION_UNREADABLE
    return Outcome.UNCHANGED if existing == contents else PreparationError.DESTINATION_MISMATCH


def _copy_new(destination: Path, contents: bytes) -> Outcome | PreparationError:
    created = False
    try:
        destination.parent.mkdir(parents=True, exist_ok=True)
        # Exclusive creation protects existing content even if another preparation
        # process creates the destination after our earlier read.
        with destination.open("xb") as stream:
            created = True
            if stream.write(contents) != len(contents):
                raise OSError("short write")
            stream.flush()
    except FileExistsError:
        return _existing(destination, contents) or PreparationError.WRITE_FAILURE
    except OSError:
        if created:
            try:
                destination.unlink()
            except OSError:
                # A remaining partial file is refused on the next run; never
                # claim preparation succeeded or delete an existing input.
                pass
        return PreparationError.WRITE_FAILURE
    return Outcome.COPIED


def prepare() -> Outcome | PreparationError:
    """Resolve only the two documented paths relative to this script's repository."""
    source_path = Path("shared/compiler-contracts/ast-v1.schema.json")
    destination_path = Path("specs/001-language-parser-diagnostics/contracts/ast-json.schema.json")
    try:
        root = Path(__file__).resolve().parents[1]
        if _has_symlink(root, source_path) or _has_symlink(root, destination_path):
            return PreparationError.UNSAFE_PATH
        contents = (root / source_path).read_bytes()
    except FileNotFoundError:
        return PreparationError.SOURCE_MISSING
    except OSError:
        return PreparationError.SOURCE_UNREADABLE
    destination = root / destination_path
    existing = _existing(destination, contents)
    return existing if existing is not None else _copy_new(destination, contents)


def main(arguments: Sequence[str] | None = None) -> int:
    """CLI success is exit 0; typed setup errors are exit 2 with stable stderr."""
    arguments = sys.argv[1:] if arguments is None else arguments
    result = PreparationError.INVALID_ARGUMENTS if arguments else prepare()
    if isinstance(result, PreparationError):
        code, message = result.value
        print(f"error[{code}]: {message}", file=sys.stderr)
        return 2
    print(result.value)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
