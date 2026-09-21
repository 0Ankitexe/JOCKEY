"""Integration tests for Version 0 HTTP behavior."""

import pytest

from jocky_controller.config import Settings
from tests.support import request


@pytest.mark.integration
def test_health_endpoint_returns_typed_liveness() -> None:
    response = request("GET", "/health")

    assert response.status_code == 200
    assert response.headers["content-type"] == "application/json"
    assert response.json() == {"status": "ok"}


@pytest.mark.integration
def test_approved_dashboard_origin_receives_cors_header() -> None:
    response = request("GET", "/health", headers={"Origin": "http://localhost:5173"})

    assert response.status_code == 200
    assert response.headers["access-control-allow-origin"] == "http://localhost:5173"
    assert "access-control-allow-credentials" not in response.headers


@pytest.mark.integration
def test_environment_loaded_origin_configures_cors(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("JOCKY_ALLOWED_ORIGINS", "https://operator.example.test")
    settings = Settings(_env_file=None)

    response = request(
        "GET",
        "/health",
        headers={"Origin": "https://operator.example.test"},
        settings=settings,
    )

    assert response.status_code == 200
    assert response.headers["access-control-allow-origin"] == "https://operator.example.test"


@pytest.mark.integration
def test_version_endpoint_returns_service_metadata() -> None:
    response = request("GET", "/version")

    assert response.status_code == 200
    assert response.headers["content-type"] == "application/json"
    assert response.json() == {"name": "JOCKY Controller", "version": "0.0.0"}
