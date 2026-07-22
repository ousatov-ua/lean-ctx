from __future__ import annotations

from types import TracebackType
from typing import Any

import httpx

from leanctx.types import HealthResponse


class LeanCtxClient:
    def __init__(self, base_url: str = "http://localhost:3333"):
        self._base_url = base_url.rstrip("/")
        self._client = httpx.Client(base_url=self._base_url, timeout=30.0)

    def health(self) -> HealthResponse:
        data = self._get_json("/ocla/v1/health")
        return HealthResponse(status=data["status"], version=data["version"])

    def capabilities(self) -> dict[str, Any]:
        return self._get_json("/ocla/v1/capabilities")

    def envelope(self, data: dict[str, Any]) -> dict[str, Any]:
        response = self._client.post("/ocla/v1/envelope", json=data)
        response.raise_for_status()
        return response.json()

    def ledger_summary(self) -> dict[str, Any]:
        return self._get_json("/ocla/v1/ledger/summary")

    def close(self) -> None:
        self._client.close()

    def __enter__(self) -> "LeanCtxClient":
        return self

    def __exit__(
        self,
        exc_type: type[BaseException] | None,
        exc_value: BaseException | None,
        traceback: TracebackType | None,
    ) -> None:
        self.close()

    def _get_json(self, path: str) -> dict[str, Any]:
        response = self._client.get(path)
        response.raise_for_status()
        return response.json()
