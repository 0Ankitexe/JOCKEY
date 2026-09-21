FROM python:3.12-slim AS development

ENV PYTHONDONTWRITEBYTECODE=1 \
    PYTHONUNBUFFERED=1 \
    PYTHONPATH=/app/src

WORKDIR /app

COPY pyproject.toml ./
COPY src ./src

RUN python -m pip install --no-cache-dir .

EXPOSE 8000

CMD ["uvicorn", "jocky_controller.main:app", "--host", "0.0.0.0", "--port", "8000"]
