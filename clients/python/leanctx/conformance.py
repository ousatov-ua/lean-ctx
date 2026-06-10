"""Shared SDK conformance kit (EPIC 12.4/12.5, industrialized in GL #395).

A client-side check that proves the Python SDK + a live server interoperate over
the **entire** frozen ``/v1`` contract. It is the exact mirror of the TypeScript
SDK's ``runConformance`` and the Rust client's ``run_conformance``, so every
first-party SDK proves the same contract and they stay in lockstep.

Two checks make this a drift gate (GL #395):

* ``route_coverage`` — every path the server's OpenAPI document advertises must
  be covered by an SDK method (``COVERED_ROUTES``). A new server route without
  SDK support fails conformance in the next CI run.
* ``engine_compat`` — the server's ``http_mcp`` contract version must be one
  this SDK release supports (``SUPPORTED_HTTP_CONTRACT_VERSIONS``).
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Callable, Dict, List

from .client import LeanCtxClient
from .errors import LeanCtxHTTPError

#: ``METHOD path`` → client method name. The conformance kit fails if the live
#: server's OpenAPI document lists a route that is missing here.
COVERED_ROUTES: Dict[str, str] = {
    "GET /health": "health",
    "GET /v1/manifest": "manifest",
    "GET /v1/capabilities": "capabilities",
    "GET /v1/openapi.json": "openapi",
    "GET /v1/tools": "list_tools",
    "POST /v1/tools/call": "call_tool",
    "GET /v1/events": "subscribe_events",
    "GET /v1/context/summary": "context_summary",
    "GET /v1/events/search": "search_events",
    "GET /v1/events/lineage": "event_lineage",
    "GET /v1/metrics": "metrics",
}

#: ``http_mcp`` contract versions this SDK release speaks (SemVer coupling:
#: the SDK major follows the engine contract major).
SUPPORTED_HTTP_CONTRACT_VERSIONS = (1,)


@dataclass
class ConformanceCheck:
    name: str
    passed: bool
    detail: str = ""


@dataclass
class ConformanceScorecard:
    checks: List[ConformanceCheck] = field(default_factory=list)

    @property
    def passed(self) -> int:
        return sum(1 for c in self.checks if c.passed)

    @property
    def total(self) -> int:
        return len(self.checks)

    @property
    def all_passed(self) -> bool:
        return all(c.passed for c in self.checks)


def _add(card: ConformanceScorecard, name: str, probe: Callable[[], Any]) -> None:
    """Run one probe; contract/network failures become failed checks."""
    try:
        ok, detail = probe()
        card.checks.append(ConformanceCheck(name, bool(ok), detail))
    except Exception as exc:  # noqa: BLE001 - capture as a failed check
        card.checks.append(ConformanceCheck(name, False, str(exc)))


def run_conformance(client: LeanCtxClient) -> ConformanceScorecard:
    """Run the conformance kit against a live client.

    Network/contract failures become failed checks rather than exceptions, so
    the returned scorecard is always complete and comparable across SDKs.
    """
    card = ConformanceScorecard()

    def health() -> Any:
        return isinstance(client.health(), str), ""

    def manifest_shape() -> Any:
        m = client.manifest()
        return isinstance(m, dict) and bool(m), ""

    def capabilities_shape() -> Any:
        caps = client.capabilities()
        server = caps.get("server", {}) if isinstance(caps, dict) else {}
        ok = (
            isinstance(caps, dict)
            and isinstance(caps.get("contract_version"), int)
            and isinstance(server, dict)
            and bool(server.get("version"))
            and isinstance(caps.get("plane"), str)
            and isinstance(caps.get("transports"), list)
            and isinstance(caps.get("features"), dict)
            and isinstance(caps.get("contracts"), dict)
        )
        return ok, ""

    def contract_status_map() -> Any:
        # GL #394: stability per contract is part of the discovery document.
        status = client.capabilities().get("contract_status")
        ok = isinstance(status, dict) and status.get("http-mcp") in (
            "frozen",
            "stable",
        )
        return ok, "" if ok else f"contract_status={status!r}"

    def engine_compat() -> Any:
        contracts = client.capabilities().get("contracts", {})
        version = contracts.get("leanctx.contract.http_mcp.contract_version")
        ok = version in SUPPORTED_HTTP_CONTRACT_VERSIONS
        return ok, "" if ok else f"server http_mcp contract v{version!r} unsupported"

    def openapi_shape() -> Any:
        doc = client.openapi()
        version = doc.get("openapi", "") if isinstance(doc, dict) else ""
        ok = (
            isinstance(version, str)
            and version.startswith("3.")
            and isinstance(doc.get("paths"), dict)
        )
        return ok, ""

    def route_coverage() -> Any:
        # The drift gate: every advertised route needs an SDK method.
        doc = client.openapi()
        paths = doc.get("paths", {}) if isinstance(doc, dict) else {}
        uncovered = [
            f"{method.upper()} {path}"
            for path, ops in paths.items()
            if isinstance(ops, dict)
            for method in ops
            if f"{method.upper()} {path}" not in COVERED_ROUTES
        ]
        return not uncovered, ", ".join(sorted(uncovered))

    def tools_list() -> Any:
        listing = client.list_tools(limit=1)
        ok = (
            isinstance(listing, dict)
            and isinstance(listing.get("tools"), list)
            and isinstance(listing.get("total"), int)
            and listing["total"] >= 0
        )
        return ok, ""

    def tool_call_error_contract() -> Any:
        # Typed-error semantics: an unknown tool must produce a structured
        # 4xx with a machine-readable error_code, not a 5xx or free text.
        try:
            client.call_tool("definitely_not_a_tool_conformance_probe")
        except LeanCtxHTTPError as exc:
            ok = 400 <= exc.status < 500 and bool(exc.error_code)
            return ok, "" if ok else f"status={exc.status} error_code={exc.error_code!r}"
        return False, "unknown tool call unexpectedly succeeded"

    def events_stream() -> Any:
        content_type = client.events_probe()
        ok = content_type.startswith("text/event-stream")
        return ok, "" if ok else f"content-type={content_type!r}"

    def context_summary_shape() -> Any:
        summary = client.context_summary(limit=1)
        ok = (
            isinstance(summary, dict)
            and isinstance(summary.get("workspaceId"), str)
            and isinstance(summary.get("totalEvents"), int)
            and isinstance(summary.get("eventCountsByKind"), dict)
        )
        return ok, ""

    def events_search_shape() -> Any:
        res = client.search_events("conformance-probe", limit=1)
        ok = (
            isinstance(res, dict)
            and isinstance(res.get("results"), list)
            and isinstance(res.get("count"), int)
        )
        return ok, ""

    def event_lineage_shape() -> Any:
        res = client.event_lineage(1, depth=1)
        ok = (
            isinstance(res, dict)
            and "eventId" in res
            and isinstance(res.get("chain"), list)
        )
        return ok, ""

    def metrics_shape() -> Any:
        return isinstance(client.metrics(), dict), ""

    _add(card, "health", health)
    _add(card, "manifest_shape", manifest_shape)
    _add(card, "capabilities_shape", capabilities_shape)
    _add(card, "contract_status_map", contract_status_map)
    _add(card, "engine_compat", engine_compat)
    _add(card, "openapi_shape", openapi_shape)
    _add(card, "route_coverage", route_coverage)
    _add(card, "tools_list", tools_list)
    _add(card, "tool_call_error_contract", tool_call_error_contract)
    _add(card, "events_stream", events_stream)
    _add(card, "context_summary_shape", context_summary_shape)
    _add(card, "events_search_shape", events_search_shape)
    _add(card, "event_lineage_shape", event_lineage_shape)
    _add(card, "metrics_shape", metrics_shape)
    return card
