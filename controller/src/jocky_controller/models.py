"""Typed HTTP response contracts for the Version 0 controller."""

from enum import StrEnum
from typing import Literal

from pydantic import BaseModel, ConfigDict, Field

from jocky_controller.constants import SERVICE_NAME, VERSION


class StrictResponseModel(BaseModel):
    """Base model that prevents accidental response-contract expansion."""

    model_config = ConfigDict(extra="forbid", frozen=True)


class HealthResponse(StrictResponseModel):
    """Controller liveness response."""

    status: Literal["ok"] = "ok"


class VersionResponse(StrictResponseModel):
    """Controller service identity and semantic version."""

    name: Literal["JOCKY Controller"] = SERVICE_NAME
    version: str = Field(default=VERSION, pattern=r"^\d+\.\d+\.\d+$")


class ErrorCode(StrEnum):
    """Stable machine-readable error codes."""

    HTTP_ERROR = "http_error"
    METHOD_NOT_ALLOWED = "method_not_allowed"
    NOT_FOUND = "not_found"
    VALIDATION_ERROR = "validation_error"


class ErrorDetail(StrictResponseModel):
    """Typed API error detail."""

    code: ErrorCode
    message: str = Field(min_length=1)


class ErrorResponse(StrictResponseModel):
    """Envelope returned for all known HTTP errors."""

    error: ErrorDetail
