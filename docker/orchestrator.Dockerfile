FROM python:3.12-slim

WORKDIR /app
ENV PYTHONDONTWRITEBYTECODE=1
ENV PYTHONUNBUFFERED=1

COPY pyproject.toml /app/
COPY backend /app/backend
RUN pip install --no-cache-dir . \
    && useradd --create-home --shell /usr/sbin/nologin octobot \
    && mkdir -p /app/.octobot \
    && chown -R octobot:octobot /app

USER octobot

EXPOSE 8787
HEALTHCHECK --interval=30s --timeout=5s --retries=3 CMD python -c "import urllib.request; urllib.request.urlopen('http://127.0.0.1:8787/health')"
CMD ["uvicorn", "backend.octobot_orchestrator.main:app", "--host", "0.0.0.0", "--port", "8787", "--proxy-headers"]
