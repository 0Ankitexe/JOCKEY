"""Typed exception handling for the public HTTP boundary."""

from typing import Final

from fastapi import FastAPI, Request
from fastapi.exceptions import RequestValidationError
from fastapi.responses import JSONResponse
from starlette.exceptions import HTTPException as StarletteHTTPException

from jocky_controller.models import ErrorCode, ErrorDetail, ErrorResponse

_HTTP_ERROR_CODES: Final[dict[int, ErrorCode]] = {
    404: ErrorCode.NOT_FOUND,
    405: ErrorCode.METHOD_NOT_ALLOWED,
}


class ApiError(Exception):
    """Expected API failure with an explicit status and stable error code."""

    def __init__(self, status_code: int, code: ErrorCode, message: str) -> None:
        super().__init__(message)
        self.status_code = status_code
        self.code = code
        self.message = message


def _error_response(status_code: int, code: ErrorCode, message: str) -> JSONResponse:
    body = ErrorResponse(error=ErrorDetail(code=code, message=message))
    return JSONResponse(status_code=status_code, content=body.model_dump(mode="json"))


def install_error_handlers(app: FastAPI) -> None:
    """Register handlers that keep error bodies inside the typed API contract."""

    @app.exception_handler(ApiError)
    async def handle_api_error(_request: Request, error: ApiError) -> JSONResponse:
        return _error_response(error.status_code, error.code, error.message)

    @app.exception_handler(RequestValidationError)
    async def handle_validation_error(
        _request: Request, _error: RequestValidationError
    ) -> JSONResponse:
        return _error_response(422, ErrorCode.VALIDATION_ERROR, "Request validation failed")

    @app.exception_handler(StarletteHTTPException)
    async def handle_http_error(_request: Request, error: StarletteHTTPException) -> JSONResponse:
        code = _HTTP_ERROR_CODES.get(error.status_code, ErrorCode.HTTP_ERROR)
        message = {
            ErrorCode.NOT_FOUND: "Resource not found",
            ErrorCode.METHOD_NOT_ALLOWED: "Method not allowed",
        }.get(code, "HTTP request failed")
        response = _error_response(error.status_code, code, message)
        if error.headers is not None:
            response.headers.update(error.headers)
        return response
