"""Unit tests for environment-backed controller settings."""

import pytest
from pydantic import ValidationError

from jocky_controller.config import Settings


def test_settings_read_prefixed_environment(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("JOCKY_ENVIRONMENT", "test")
    monkeypatch.setenv("JOCKY_HOST", "127.0.0.2")
    monkeypatch.setenv("JOCKY_PORT", "8123")
    monkeypatch.setenv(
        "JOCKY_ALLOWED_ORIGINS",
        "https://dashboard.example.test, http://127.0.0.1:5173",
    )

    settings = Settings(_env_file=None)

    assert settings.environment == "test"
    assert settings.host == "127.0.0.2"
    assert settings.port == 8123
    assert settings.allowed_origins == (
        "https://dashboard.example.test",
        "http://127.0.0.1:5173",
    )


def test_settings_reject_invalid_port(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("JOCKY_PORT", "70000")

    with pytest.raises(ValidationError, match="less than or equal to 65535"):
        Settings(_env_file=None)


def test_settings_reject_empty_cors_origin_list(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("JOCKY_ALLOWED_ORIGINS", " , ")

    with pytest.raises(ValidationError, match="at least one explicit CORS origin"):
        Settings(_env_file=None)


@pytest.mark.parametrize(
    "origin",
    ["*", "file:///tmp/dashboard", "https://user:secret@example.test", "not-a-url"],
)
def test_settings_reject_unsafe_cors_origins(origin: str) -> None:
    with pytest.raises(ValidationError, match="invalid CORS origin"):
        Settings(allowed_origins=(origin,), _env_file=None)
