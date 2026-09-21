"""Negative HTTP tests for typed errors and the intentionally small API surface."""

import pytest

from jocky_controller.models import ErrorCode, ErrorResponse
from tests.support import request


@pytest.mark.integration
@pytest.mark.parametrize("path", ["/health", "/version"])
def test_public_endpoints_reject_unknown_query_parameters(path: str) -> None:
    response = request("GET", path, params={"unexpected": "value"})

    body = ErrorResponse.model_validate(response.json())
    assert response.status_code == 422
    assert body.error.code is ErrorCode.VALIDATION_ERROR
    assert body.error.message == "This endpoint does not accept query parameters"


@pytest.mark.integration
@pytest.mark.parametrize("path", ["/health", "/version"])
def test_public_endpoints_reject_unsupported_methods(path: str) -> None:
    response = request("POST", path)

    body = ErrorResponse.model_validate(response.json())
    assert response.status_code == 405
    assert response.headers["allow"] == "GET"
    assert body.error.code is ErrorCode.METHOD_NOT_ALLOWED


@pytest.mark.integration
def test_unknown_route_returns_typed_not_found() -> None:
    response = request("GET", "/not-a-controller-route")

    body = ErrorResponse.model_validate(response.json())
    assert response.status_code == 404
    assert body.error.code is ErrorCode.NOT_FOUND
    assert body.error.message == "Resource not found"


@pytest.mark.integration
@pytest.mark.parametrize("path", ["/docs", "/redoc", "/openapi.json"])
def test_framework_documentation_routes_are_disabled(path: str) -> None:
    response = request("GET", path)

    body = ErrorResponse.model_validate(response.json())
    assert response.status_code == 404
    assert body.error.code is ErrorCode.NOT_FOUND


@pytest.mark.integration
def test_unapproved_origin_receives_no_cors_permission() -> None:
    response = request("GET", "/health", headers={"Origin": "https://unapproved.example.test"})

    assert response.status_code == 200
    assert "access-control-allow-origin" not in response.headers
    assert "access-control-allow-credentials" not in response.headers
