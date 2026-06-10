import type { LeanCtxClient } from "./client.js";
import { LeanCtxHttpError } from "./errors.js";

/**
 * Shared SDK conformance kit (EPIC 12.5, industrialized in GL #395).
 *
 * A language-agnostic client-side check that any lean-ctx SDK can run against a
 * live server to prove it speaks the **entire** frozen `/v1` contract. It is
 * the exact mirror of the Python SDK's `run_conformance` and the Rust client's
 * `run_conformance`, so every first-party SDK proves the same contract and they
 * stay in lockstep.
 *
 * Two checks make this a drift gate (GL #395):
 *
 * - `route_coverage` — every path the server's OpenAPI document advertises must
 *   be covered by an SDK method (`COVERED_ROUTES`). A new server route without
 *   SDK support fails conformance in the next CI run.
 * - `engine_compat` — the server's `http_mcp` contract version must be one this
 *   SDK release supports (`SUPPORTED_HTTP_CONTRACT_VERSIONS`).
 */

/** `METHOD path` → client method. Fails conformance when the server adds a route missing here. */
export const COVERED_ROUTES: Readonly<Record<string, string>> = {
  "GET /health": "health",
  "GET /v1/manifest": "manifest",
  "GET /v1/capabilities": "capabilities",
  "GET /v1/openapi.json": "openapi",
  "GET /v1/tools": "listTools",
  "POST /v1/tools/call": "callToolResult",
  "GET /v1/events": "subscribeEvents",
  "GET /v1/context/summary": "contextSummary",
  "GET /v1/events/search": "searchEvents",
  "GET /v1/events/lineage": "eventLineage",
  "GET /v1/metrics": "metrics",
};

/** `http_mcp` contract versions this SDK release speaks (SDK major follows engine contract major). */
export const SUPPORTED_HTTP_CONTRACT_VERSIONS: readonly number[] = [1];

export interface ConformanceCheck {
  name: string;
  passed: boolean;
  detail: string;
}

export interface ConformanceScorecard {
  passed: number;
  total: number;
  allPassed: boolean;
  checks: ConformanceCheck[];
}

type Probe = () => Promise<[boolean, string?]>;

async function add(
  checks: ConformanceCheck[],
  name: string,
  probe: Probe
): Promise<void> {
  try {
    const [passed, detail] = await probe();
    checks.push({ name, passed, detail: detail ?? "" });
  } catch (e) {
    checks.push({ name, passed: false, detail: String(e) });
  }
}

/**
 * Run the conformance kit against a live client. Network/contract failures are
 * captured as failed checks rather than thrown, so the scorecard is always
 * complete and comparable across SDKs.
 */
export async function runConformance(
  client: LeanCtxClient
): Promise<ConformanceScorecard> {
  const checks: ConformanceCheck[] = [];

  await add(checks, "health", async () => {
    const h = await client.health();
    return [typeof h === "string"];
  });

  await add(checks, "manifest_shape", async () => {
    const m = await client.manifest();
    return [!!m && typeof m === "object"];
  });

  await add(checks, "capabilities_shape", async () => {
    const caps = await client.capabilities();
    const ok =
      typeof caps.contract_version === "number" &&
      !!caps.server?.version &&
      typeof caps.plane === "string" &&
      Array.isArray(caps.transports) &&
      typeof caps.features === "object" &&
      typeof caps.contracts === "object";
    return [ok];
  });

  await add(checks, "contract_status_map", async () => {
    // GL #394: stability per contract is part of the discovery document.
    const status = (await client.capabilities()).contract_status;
    const ok =
      !!status &&
      typeof status === "object" &&
      ["frozen", "stable"].includes(status["http-mcp"] ?? "");
    return [ok, ok ? "" : `contract_status=${JSON.stringify(status)}`];
  });

  await add(checks, "engine_compat", async () => {
    const contracts = (await client.capabilities()).contracts ?? {};
    const version = contracts["leanctx.contract.http_mcp.contract_version"];
    const ok =
      typeof version === "number" &&
      SUPPORTED_HTTP_CONTRACT_VERSIONS.includes(version);
    return [
      ok,
      ok ? "" : `server http_mcp contract v${String(version)} unsupported`,
    ];
  });

  await add(checks, "openapi_shape", async () => {
    const doc = await client.openapi();
    const version = typeof doc.openapi === "string" ? doc.openapi : "";
    return [version.startsWith("3.") && typeof doc.paths === "object"];
  });

  await add(checks, "route_coverage", async () => {
    // The drift gate: every advertised route needs an SDK method.
    const doc = await client.openapi();
    const paths =
      doc.paths && typeof doc.paths === "object" && !Array.isArray(doc.paths)
        ? (doc.paths as Record<string, unknown>)
        : {};
    const uncovered: string[] = [];
    for (const [path, ops] of Object.entries(paths)) {
      if (!ops || typeof ops !== "object") continue;
      for (const method of Object.keys(ops)) {
        const route = `${method.toUpperCase()} ${path}`;
        if (!(route in COVERED_ROUTES)) uncovered.push(route);
      }
    }
    return [uncovered.length === 0, uncovered.sort().join(", ")];
  });

  await add(checks, "tools_list", async () => {
    const list = await client.listTools({ limit: 1 });
    return [
      Array.isArray(list.tools) &&
        typeof list.total === "number" &&
        list.total >= 0,
    ];
  });

  await add(checks, "tool_call_error_contract", async () => {
    // Typed-error semantics: an unknown tool must produce a structured 4xx
    // with a machine-readable error_code, not a 5xx or free text.
    try {
      await client.callToolResult("definitely_not_a_tool_conformance_probe");
    } catch (e) {
      if (e instanceof LeanCtxHttpError) {
        const ok = e.status >= 400 && e.status < 500 && !!e.errorCode;
        return [
          ok,
          ok ? "" : `status=${e.status} errorCode=${String(e.errorCode)}`,
        ];
      }
      return [false, String(e)];
    }
    return [false, "unknown tool call unexpectedly succeeded"];
  });

  await add(checks, "events_stream", async () => {
    const contentType = await client.eventsProbe();
    return [
      contentType.startsWith("text/event-stream"),
      contentType.startsWith("text/event-stream")
        ? ""
        : `content-type=${contentType}`,
    ];
  });

  await add(checks, "context_summary_shape", async () => {
    const summary = await client.contextSummary({ limit: 1 });
    const ok =
      typeof summary.workspaceId === "string" &&
      typeof summary.totalEvents === "number" &&
      !!summary.eventCountsByKind &&
      typeof summary.eventCountsByKind === "object";
    return [ok];
  });

  await add(checks, "events_search_shape", async () => {
    const res = await client.searchEvents("conformance-probe", { limit: 1 });
    return [Array.isArray(res.results) && typeof res.count === "number"];
  });

  await add(checks, "event_lineage_shape", async () => {
    const res = await client.eventLineage(1, { depth: 1 });
    return ["eventId" in res && Array.isArray(res.chain)];
  });

  await add(checks, "metrics_shape", async () => {
    const m = await client.metrics();
    return [!!m && typeof m === "object"];
  });

  const passed = checks.filter((c) => c.passed).length;
  return {
    passed,
    total: checks.length,
    allPassed: passed === checks.length,
    checks,
  };
}
