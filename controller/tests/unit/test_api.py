"""Unit tests for the two Version 0 route functions."""

import asyncio

from starlette.requests import Request
from starlette.types import Scope

from jocky_controller.api import get_health, get_version
from jocky_controller.config import Settings
from jocky_controller.main import create_app


def _request(path: str) -> Request:
    scope: Scope = {
        "type": "http",
        "asgi": {"version": "3.0"},
        "http_version": "1.1",
        "method": "GET",
        "scheme": "http",
        "path": path,
        "raw_path": path.encode(),
        "query_string": b"",
        "root_path": "",
        "headers": [],
        "server": ("testserver", 80),
        "client": ("127.0.0.1", 50000),
        "state": {},
    }
    return Request(scope)


def test_get_health_returns_health_model() -> None:
    response = asyncio.run(get_health(_request("/health")))

    assert response.status == "ok"


def test_get_version_returns_version_model() -> None:
    response = asyncio.run(get_version(_request("/version")))

    assert response.name == "JOCKY Controller"
    assert response.version == "0.0.0"


def test_openapi_model_describes_only_the_two_version_zero_paths() -> None:
    schema = create_app(Settings(_env_file=None)).openapi()

    assert set(schema["paths"]) == {"/health", "/version"}
    assert set(schema["paths"]["/health"]) == {"get"}
    assert set(schema["paths"]["/version"]) == {"get"}
