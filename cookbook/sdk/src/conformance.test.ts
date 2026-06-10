import { describe, expect, it } from "vitest";

import { LeanCtxClient } from "./client.js";
import { COVERED_ROUTES, runConformance } from "./conformance.js";

function openapiPaths(extraRoute?: string): Record<string, unknown> {
  const paths: Record<string, Record<string, unknown>> = {};
  const routes = extraRoute
    ? [...Object.keys(COVERED_ROUTES), extraRoute]
    : Object.keys(COVERED_ROUTES);
  for (const route of routes) {
    const [method, path] = route.split(" ");
    paths[path] ??= {};
    paths[path][method.toLowerCase()] = { summary: route };
  }
  return paths;
}

/** A stub server that returns valid /v1 contract responses. */
function okFetch(extraRoute?: string): typeof fetch {
  return async (url, init) => {
    const u = new URL(String(url));
    const json = (body: unknown, status = 200) =>
      new Response(JSON.stringify(body), {
        status,
        headers: { "content-type": "application/json" },
      });

    if (u.pathname.endsWith("/health")) {
      return new Response("ok", { status: 200 });
    }
    if (u.pathname.endsWith("/v1/manifest")) {
      return json({ schema_version: 1, tools: [] });
    }
    if (u.pathname.endsWith("/v1/capabilities")) {
      return json({
        contract_version: 1,
        server: { name: "lean-ctx", version: "3.7.5" },
        plane: "personal",
        transports: ["rest"],
        presets: ["coding"],
        read_modes: ["full"],
        tools: { total: 1, names: ["ctx_read"] },
        features: {},
        extensions: {},
        contracts: { "leanctx.contract.http_mcp.contract_version": 1 },
        contract_status: { "http-mcp": "frozen" },
      });
    }
    if (u.pathname.endsWith("/v1/openapi.json")) {
      return json({
        openapi: "3.0.3",
        info: {},
        paths: openapiPaths(extraRoute),
      });
    }
    if (u.pathname.endsWith("/v1/tools/call") && init?.method === "POST") {
      return json({ error: "unknown tool", error_code: "unknown_tool" }, 404);
    }
    if (u.pathname.endsWith("/v1/tools")) {
      return json({ tools: [], total: 0, offset: 0, limit: 1 });
    }
    if (u.pathname.endsWith("/v1/events/search")) {
      return json({ query: "x", results: [], count: 0 });
    }
    if (u.pathname.endsWith("/v1/events/lineage")) {
      return json({ eventId: 1, chain: [], depth: 0 });
    }
    if (u.pathname.endsWith("/v1/events")) {
      return new Response("", {
        status: 200,
        headers: { "content-type": "text/event-stream" },
      });
    }
    if (u.pathname.endsWith("/v1/context/summary")) {
      return json({
        workspaceId: "default",
        channelId: "default",
        totalEvents: 0,
        latestVersion: 0,
        activeAgents: [],
        recentDecisions: [],
        knowledgeDelta: [],
        conflictAlerts: [],
        eventCountsByKind: {},
      });
    }
    if (u.pathname.endsWith("/v1/metrics")) {
      return json({ events_published: 0 });
    }
    return new Response("not found", { status: 404 });
  };
}

describe("runConformance", () => {
  it("passes against a conformant server", async () => {
    const c = new LeanCtxClient({
      baseUrl: "http://127.0.0.1:9",
      fetchImpl: okFetch(),
    });
    const card = await runConformance(c);
    expect(
      card.checks.filter((x) => !x.passed).map((x) => `${x.name}: ${x.detail}`)
    ).toEqual([]);
    expect(card.allPassed).toBe(true);
    expect(card.total).toBe(14);
  });

  it("records a failure when capabilities are malformed", async () => {
    const fetchImpl: typeof fetch = async (url, init) => {
      const u = String(url);
      if (u.endsWith("/v1/capabilities")) {
        return new Response(JSON.stringify({ wrong: true }), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }
      return okFetch()(url, init);
    };
    const c = new LeanCtxClient({ baseUrl: "http://127.0.0.1:9", fetchImpl });
    const card = await runConformance(c);
    expect(card.allPassed).toBe(false);
    for (const name of [
      "capabilities_shape",
      "contract_status_map",
      "engine_compat",
    ]) {
      expect(card.checks.find((x) => x.name === name)?.passed).toBe(false);
    }
  });

  it("catches endpoint drift within one run (GL #395)", async () => {
    const c = new LeanCtxClient({
      baseUrl: "http://127.0.0.1:9",
      fetchImpl: okFetch("GET /v1/brand-new-route"),
    });
    const card = await runConformance(c);
    const coverage = card.checks.find((x) => x.name === "route_coverage");
    expect(coverage?.passed).toBe(false);
    expect(coverage?.detail).toContain("GET /v1/brand-new-route");
  });
});
