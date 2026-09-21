"""Environment-backed controller configuration.

Database and Redis settings establish configuration boundaries for later versions. Version 0
does not connect to either service.
"""

from functools import lru_cache
from typing import Annotated, Literal
from urllib.parse import urlsplit

from pydantic import Field, field_validator
from pydantic_settings import BaseSettings, NoDecode, SettingsConfigDict


class Settings(BaseSettings):
    """Validated process settings loaded from ``JOCKY_*`` environment variables."""

    model_config = SettingsConfigDict(
        env_file=".env",
        env_file_encoding="utf-8",
        env_prefix="JOCKY_",
        extra="ignore",
    )

    environment: Literal["development", "test", "production"] = "development"
    log_level: Literal["DEBUG", "INFO", "WARNING", "ERROR", "CRITICAL"] = "INFO"
    host: str = "127.0.0.1"
    port: int = Field(default=8000, ge=1, le=65535)
    allowed_origins: Annotated[tuple[str, ...], NoDecode] = ("http://localhost:5173",)
    database_url: str | None = None
    redis_url: str | None = None

    @field_validator("allowed_origins", mode="before")
    @classmethod
    def parse_allowed_origins(cls, value: object) -> object:
        """Parse a comma-separated environment value into individual origins."""

        if isinstance(value, str):
            return tuple(origin.strip() for origin in value.split(",") if origin.strip())
        return value

    @field_validator("allowed_origins")
    @classmethod
    def validate_allowed_origins(cls, origins: tuple[str, ...]) -> tuple[str, ...]:
        """Allow only explicit HTTP(S) origins, never wildcard or credentialed URLs."""

        if not origins:
            raise ValueError("at least one explicit CORS origin is required")
        for origin in origins:
            parsed = urlsplit(origin)
            if (
                origin == "*"
                or parsed.scheme not in {"http", "https"}
                or not parsed.netloc
                or parsed.username is not None
                or parsed.password is not None
                or parsed.path not in {"", "/"}
                or parsed.query
                or parsed.fragment
            ):
                raise ValueError(f"invalid CORS origin: {origin}")
        return origins


@lru_cache
def get_settings() -> Settings:
    """Return one validated settings instance per process."""

    return Settings()
