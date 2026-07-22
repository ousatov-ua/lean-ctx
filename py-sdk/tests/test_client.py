from __future__ import annotations

from typing import Any

import httpx

from leanctx import HealthResponse, LeanCtxClient


class MockClient:
    def __init__(self, routes: dict[tuple[str, str], dict[str, Any]]):
        self.routes = routes
        self.calls: list[tuple[str, str, dict[str, Any] | None]] = []
        self.closed = False

    def get(self, path: str) -> httpx.Response:
        self.calls.append(("GET", path, None))
        return self._response("GET", path)

    def post(self, path: str, json: dict[str, Any]) -> httpx.Response:
        self.calls.append(("POST", path, json))
        return self._response("POST", path)

    def close(self) -> None:
        self.closed = True

    def _response(self, method: str, path: str) -> httpx.Response:
        request = httpx.Request(method, f"http://test{path}")
        return httpx.Response(200, json=self.routes[(method, path)], request=request)


def install_mock(
    monkeypatch: Any,
    routes: dict[tuple[str, str], dict[str, Any]],
) -> MockClient:
    mock = MockClient(routes)
    monkeypatch.setattr(httpx, "Client", lambda **kwargs: mock)
    return mock


def test_health(monkeypatch: Any) -> None:
    mock = install_mock(
        monkeypatch,
        {("GET", "/ocla/v1/health"): {"status": "ok", "version": "1"}},
    )

    with LeanCtxClient("http://test/") as client:
        assert client.health() == HealthResponse(status="ok", version="1")

    assert mock.calls == [("GET", "/ocla/v1/health", None)]
    assert mock.closed


def test_envelope(monkeypatch: Any) -> None:
    envelope = {"version": "1", "agent_id": "test-agent"}
    mock = install_mock(
        monkeypatch,
        {("POST", "/ocla/v1/envelope"): {"valid": True, "errors": []}},
    )

    result = LeanCtxClient().envelope(envelope)

    assert result == {"valid": True, "errors": []}
    assert mock.calls == [("POST", "/ocla/v1/envelope", envelope)]


def test_ledger_summary(monkeypatch: Any) -> None:
    summary = {"events": 4, "tokens": 120, "usd": 0.02}
    mock = install_mock(
        monkeypatch,
        {("GET", "/ocla/v1/ledger/summary"): summary},
    )

    result = LeanCtxClient().ledger_summary()

    assert result == summary
    assert mock.calls == [("GET", "/ocla/v1/ledger/summary", None)]
