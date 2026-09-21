"""Unit tests for typed controller response contracts."""

import pytest
from pydantic import ValidationError

from jocky_controller.models import ErrorCode, ErrorResponse, HealthResponse, VersionResponse


def test_health_response_has_stable_shape() -> None:
    response = HealthResponse()

    assert response.model_dump(mode="json") == {"status": "ok"}


def test_version_response_has_stable_shape() -> None:
    response = VersionResponse()

    assert response.model_dump(mode="json") == {
        "name": "JOCKY Controller",
        "version": "0.0.0",
    }


def test_response_models_reject_unknown_fields() -> None:
    with pytest.raises(ValidationError, match="Extra inputs are not permitted"):
        HealthResponse.model_validate({"status": "ok", "unexpected": True})


def test_version_response_rejects_non_semantic_version() -> None:
    with pytest.raises(ValidationError, match="String should match pattern"):
        VersionResponse(version="version-zero")


def test_error_response_parses_stable_error_code() -> None:
    response = ErrorResponse.model_validate(
        {"error": {"code": "not_found", "message": "Resource not found"}}
    )

    assert response.error.code is ErrorCode.NOT_FOUND
