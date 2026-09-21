"""Small synchronous facade over HTTPX's in-process ASGI transport."""

import asyncio

from httpx import ASGITransport, AsyncClient, Response

from jocky_controller.config import Settings
from jocky_controller.main import create_app


async def _request(
    method: str,
    path: str,
    *,
    params: dict[str, str] | None,
    headers: dict[str, str] | None,
    settings: Settings | None,
) -> Response:
    transport = ASGITransport(app=create_app(settings))
    async with AsyncClient(transport=transport, base_url="http://testserver") as client:
        return await client.request(method, path, params=params, headers=headers)


def request(
    method: str,
    path: str,
    *,
    params: dict[str, str] | None = None,
    headers: dict[str, str] | None = None,
    settings: Settings | None = None,
) -> Response:
    """Issue one isolated request to the ASGI application."""

    return asyncio.run(_request(method, path, params=params, headers=headers, settings=settings))
