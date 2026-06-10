"""Conformance-kit tests against the in-process stub server."""

from __future__ import annotations

import json
import threading
from http.server import BaseHTTPRequestHandler, HTTPServer
from typing import Any, Tuple
from urllib.parse import urlparse

from leanctx import COVERED_ROUTES, LeanCtxClient, run_conformance

CAPS = {
    "contract_version": 1,
    "server": {"name": "lean-ctx", "version": "3.7.5"},
    "plane": "personal",
    "transports": ["rest"],
    "presets": ["coding"],
    "read_modes": ["full"],
    "tools": {"total": 1, "names": ["ctx_read"]},
    "features": {},
    "extensions": {},
    "contracts": {"leanctx.contract.http_mcp.contract_version": 1},
    "contract_status": {"http-mcp": "frozen"},
}


def _openapi_paths() -> dict:
    """OpenAPI paths mirroring COVERED_ROUTES (the live server's shape)."""
    paths: dict = {}
    for route in COVERED_ROUTES:
        method, path = route.split(" ", 1)
        paths.setdefault(path, {})[method.lower()] = {"summary": route}
    return paths


def _make_handler(caps: Any, extra_route: str | None = None):
    class _Handler(BaseHTTPRequestHandler):
        def log_message(self, *args: Any) -> None:
            pass

        def _send(self, status: int, body: Any, ct: str = "application/json") -> None:
            payload = body if isinstance(body, bytes) else json.dumps(body).encode()
            self.send_response(status)
            self.send_header("Content-Type", ct)
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

        def do_GET(self) -> None:  # noqa: N802
            path = urlparse(self.path).path
            if path == "/health":
                self._send(200, b"ok", "text/plain")
            elif path == "/v1/manifest":
                self._send(200, {"schema_version": 1, "tools": []})
            elif path == "/v1/capabilities":
                self._send(200, caps)
            elif path == "/v1/openapi.json":
                paths = _openapi_paths()
                if extra_route:
                    method, route_path = extra_route.split(" ", 1)
                    paths.setdefault(route_path, {})[method.lower()] = {}
                self._send(200, {"openapi": "3.0.3", "info": {}, "paths": paths})
            elif path == "/v1/tools":
                self._send(200, {"tools": [], "total": 0, "offset": 0, "limit": 1})
            elif path == "/v1/events":
                self._send(200, b"", "text/event-stream")
            elif path == "/v1/context/summary":
                self._send(
                    200,
                    {
                        "workspaceId": "default",
                        "channelId": "default",
                        "totalEvents": 0,
                        "latestVersion": 0,
                        "activeAgents": [],
                        "recentDecisions": [],
                        "knowledgeDelta": [],
                        "conflictAlerts": [],
                        "eventCountsByKind": {},
                    },
                )
            elif path == "/v1/events/search":
                self._send(200, {"query": "x", "results": [], "count": 0})
            elif path == "/v1/events/lineage":
                self._send(200, {"eventId": 1, "chain": [], "depth": 0})
            elif path == "/v1/metrics":
                self._send(200, {"events_published": 0})
            else:
                self._send(404, {"error": "unknown", "error_code": "not_found"})

        def do_POST(self) -> None:  # noqa: N802
            if urlparse(self.path).path == "/v1/tools/call":
                self._send(
                    404,
                    {"error": "unknown tool", "error_code": "unknown_tool"},
                )
            else:
                self._send(404, {"error": "unknown", "error_code": "not_found"})

    return _Handler


def _serve(caps: Any, extra_route: str | None = None) -> Tuple[str, HTTPServer]:
    httpd = HTTPServer(("127.0.0.1", 0), _make_handler(caps, extra_route))
    threading.Thread(target=httpd.serve_forever, daemon=True).start()
    host, port = httpd.server_address
    return f"http://{host}:{port}", httpd


def test_conformance_passes_against_valid_server() -> None:
    base, httpd = _serve(CAPS)
    try:
        card = run_conformance(LeanCtxClient(base))
        assert card.all_passed, [c for c in card.checks if not c.passed]
        assert card.total == 14
    finally:
        httpd.shutdown()


def test_conformance_flags_malformed_capabilities() -> None:
    base, httpd = _serve({"wrong": True})
    try:
        card = run_conformance(LeanCtxClient(base))
        assert not card.all_passed
        for name in ("capabilities_shape", "contract_status_map", "engine_compat"):
            failed = next(c for c in card.checks if c.name == name)
            assert not failed.passed, name
    finally:
        httpd.shutdown()


def test_route_coverage_catches_endpoint_drift() -> None:
    # GL #395 AC 3: a route the SDK does not cover fails within one run.
    base, httpd = _serve(CAPS, extra_route="GET /v1/brand-new-route")
    try:
        card = run_conformance(LeanCtxClient(base))
        coverage = next(c for c in card.checks if c.name == "route_coverage")
        assert not coverage.passed
        assert "GET /v1/brand-new-route" in coverage.detail
    finally:
        httpd.shutdown()
