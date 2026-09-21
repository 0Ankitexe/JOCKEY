"""Version 0 controller routes."""

from fastapi import APIRouter, Request

from jocky_controller.constants import SERVICE_NAME, VERSION
from jocky_controller.errors import ApiError
from jocky_controller.models import ErrorCode, HealthResponse, VersionResponse

router = APIRouter()


def _reject_query_parameters(request: Request) -> None:
    if request.query_params:
        raise ApiError(
            status_code=422,
            code=ErrorCode.VALIDATION_ERROR,
            message="This endpoint does not accept query parameters",
        )


@router.get("/health", response_model=HealthResponse)
async def get_health(request: Request) -> HealthResponse:
    """Report controller process liveness without probing future dependencies."""

    _reject_query_parameters(request)
    return HealthResponse()


@router.get("/version", response_model=VersionResponse)
async def get_version(request: Request) -> VersionResponse:
    """Return stable controller name and Version 0 package version."""

    _reject_query_parameters(request)
    return VersionResponse(name=SERVICE_NAME, version=VERSION)
