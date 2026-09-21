"""Offline unit and subprocess tests; every temporary file stays in this repository."""

from __future__ import annotations

import shutil
import subprocess
import sys
import tempfile
from collections.abc import Iterator
from pathlib import Path
from unittest.mock import patch

import pytest
from ci import prepare_legacy_ast_schema as tool

REPOSITORY = Path(__file__).resolve().parents[1]
SOURCE = Path("shared/compiler-contracts/ast-v1.schema.json")
DESTINATION = Path("specs/001-language-parser-diagnostics/contracts/ast-json.schema.json")


@pytest.fixture
def repository(monkeypatch: pytest.MonkeyPatch) -> Iterator[Path]:
    temporary = REPOSITORY / "target" / "preparation-tests"
    temporary.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="case-", dir=temporary) as directory:
        root = Path(directory)
        (root / "ci").mkdir()
        shutil.copyfile(
            REPOSITORY / "ci/prepare_legacy_ast_schema.py", root / "ci/prepare_legacy_ast_schema.py"
        )
        (root / SOURCE).parent.mkdir(parents=True)
        (root / SOURCE).write_bytes((REPOSITORY / SOURCE).read_bytes())
        monkeypatch.setattr(tool, "__file__", str(root / "ci/prepare_legacy_ast_schema.py"))
        yield root


def invoke(repository: Path, *arguments: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(repository / "ci/prepare_legacy_ast_schema.py"), *arguments],
        cwd=REPOSITORY,
        capture_output=True,
        text=True,
        check=False,
    )


def test_copy_idempotence_and_mismatch_never_overwrite(repository: Path) -> None:
    assert tool.prepare() is tool.Outcome.COPIED
    original = (repository / SOURCE).read_bytes()
    assert (repository / DESTINATION).read_bytes() == original
    stat = (repository / DESTINATION).stat()
    assert tool.prepare() is tool.Outcome.UNCHANGED
    assert (repository / DESTINATION).stat().st_mtime_ns == stat.st_mtime_ns
    (repository / DESTINATION).write_bytes(b"different user-owned contents")
    assert tool.prepare() is tool.PreparationError.DESTINATION_MISMATCH
    assert (repository / DESTINATION).read_bytes() == b"different user-owned contents"
    assert (repository / SOURCE).read_bytes() == original


def test_missing_unreadable_source_destination_and_write_failures_are_typed(
    repository: Path,
) -> None:
    source = repository / SOURCE
    original = source.read_bytes()
    source.unlink()
    assert tool.prepare() is tool.PreparationError.SOURCE_MISSING
    source.write_bytes(original)
    with patch.object(Path, "read_bytes", side_effect=PermissionError("private native message")):
        assert tool.prepare() is tool.PreparationError.SOURCE_UNREADABLE
    (repository / "specs").write_bytes(b"blocks the destination directory")
    assert tool.prepare() is tool.PreparationError.WRITE_FAILURE
    assert not (repository / DESTINATION).exists()
    (repository / "specs").unlink()
    assert tool.prepare() is tool.Outcome.COPIED
    (repository / DESTINATION).unlink()
    (repository / DESTINATION).mkdir()
    assert tool.prepare() is tool.PreparationError.DESTINATION_UNREADABLE


def test_actual_cli_uses_its_own_location_and_has_stable_channels(repository: Path) -> None:
    copied = invoke(repository)
    assert (copied.returncode, copied.stdout, copied.stderr) == (
        0,
        "prepared: legacy AST schema\n",
        "",
    )
    unchanged = invoke(repository)
    assert (unchanged.returncode, unchanged.stdout, unchanged.stderr) == (
        0,
        "unchanged: legacy AST schema\n",
        "",
    )
    (repository / DESTINATION).write_bytes(b"keep me")
    mismatch = invoke(repository)
    assert mismatch.returncode == 2 and not mismatch.stdout
    assert (
        mismatch.stderr
        == "error[destination-mismatch]: existing legacy AST schema differs; refusing overwrite\n"
    )
    assert (repository / DESTINATION).read_bytes() == b"keep me"
    (repository / SOURCE).unlink()
    missing = invoke(repository)
    assert missing.returncode == 2 and not missing.stdout
    assert missing.stderr == "error[source-missing]: bundled AST schema is missing\n"


@pytest.mark.parametrize(
    "arguments",
    [("--force",), ("--source", "https://invalid.example/schema"), ("--root", "/"), ("extra",)],
)
def test_arbitrary_flags_are_rejected_without_modification(
    repository: Path, arguments: tuple[str, ...]
) -> None:
    result = invoke(repository, *arguments)
    assert result.returncode == 2 and not result.stdout
    assert result.stderr == "error[invalid-arguments]: this tool accepts no arguments\n"
    assert not (repository / DESTINATION).exists()


def test_symlinks_are_rejected_even_if_the_target_is_in_the_test_repository(
    repository: Path,
) -> None:
    source = repository / SOURCE
    saved = source.with_name("saved.json")
    source.rename(saved)
    source.symlink_to(saved)
    assert tool.prepare() is tool.PreparationError.UNSAFE_PATH
    assert not (repository / DESTINATION).exists()
    source.unlink()
    saved.rename(source)
    alternate = repository / "alternate"
    alternate.mkdir()
    (repository / "specs").symlink_to(alternate, target_is_directory=True)
    assert tool.prepare() is tool.PreparationError.UNSAFE_PATH
    assert list(alternate.iterdir()) == []


def test_cli_suppresses_native_read_and_write_error_contents(
    repository: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    with patch.object(tool, "prepare", return_value=tool.PreparationError.SOURCE_UNREADABLE):
        assert tool.main([]) == 2
    assert (
        capsys.readouterr().err == "error[source-unreadable]: bundled AST schema cannot be read\n"
    )
    with patch.object(tool, "prepare", return_value=tool.PreparationError.WRITE_FAILURE):
        assert tool.main([]) == 2
    assert (
        capsys.readouterr().err == "error[write-failure]: legacy AST schema could not be prepared\n"
    )


def test_partial_new_file_is_removed_after_injected_write_failure(repository: Path) -> None:
    destination = repository / DESTINATION
    destination.parent.mkdir(parents=True)
    destination.write_bytes(b"")
    with patch.object(Path, "open") as opened:
        opened.return_value.__enter__.return_value.write.side_effect = OSError(
            "private native details"
        )
        assert tool._copy_new(destination, b"bytes") is tool.PreparationError.WRITE_FAILURE
    assert not destination.exists()


def test_exclusive_creation_race_never_replaces_a_different_file(repository: Path) -> None:
    destination = repository / DESTINATION
    destination.parent.mkdir(parents=True)
    destination.write_bytes(b"another writer")
    assert tool._copy_new(destination, b"ours") is tool.PreparationError.DESTINATION_MISMATCH
    assert destination.read_bytes() == b"another writer"
