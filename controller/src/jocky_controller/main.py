"""FastAPI application factory and ASGI entry point."""

from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware

from jocky_controller.api import router
from jocky_controller.config import Settings, get_settings
from jocky_controller.constants import SERVICE_NAME, VERSION
from jocky_controller.errors import install_error_handlers


def create_app(settings: Settings | None = None) -> FastAPI:
    """Build the Version 0 controller application."""

    resolved_settings = settings or get_settings()
    application = FastAPI(
        title=SERVICE_NAME,
        version=VERSION,
        docs_url=None,
        redoc_url=None,
        openapi_url=None,
    )
    application.add_middleware(
        CORSMiddleware,
        allow_origins=list(resolved_settings.allowed_origins),
        allow_credentials=False,
        allow_methods=["GET"],
        allow_headers=["Accept", "Content-Type"],
    )
    install_error_handlers(application)
    application.include_router(router)
    return application


app = create_app()
