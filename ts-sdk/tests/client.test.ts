import { afterEach, describe, expect, it, vi } from "vitest";

import { OclaClient } from "../src/client.js";
import type { CanonicalTokenEnvelopeV1 } from "../src/types.js";

const envelope: CanonicalTokenEnvelopeV1 = {
  schema_version: 1,
  context: {
    request_id: "request-1",
    session_id: "session-1",
    agent_id: "agent-1",
    content_ref: "blake3:content",
    tenant_id: null,
  },
  surface: "proxy",
  direction: "input",
  provider: "openai",
  model: "gpt-5",
  token_balance: {
    original_tokens: 100,
    materialized_tokens: 80,
    delivered_tokens: 60,
    provider_billed_tokens: 60,
  },
  route_ref: "route-1",
  policy_ref: null,
  idempotency_key: "request-1:input",
};

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("OclaClient", () => {
  it("normalizes the configured base URL", () => {
    expect(new OclaClient(" https://example.test/// ").baseUrl).toBe(
      "https://example.test",
    );
  });

  it("calls all OCLA endpoints with the expected methods and paths", async () => {
    const responses: Record<string, unknown> = {
      "/ocla/v1/health": { status: "ok", version: "ocla/v1" },
      "/ocla/v1/capabilities": { version: "ocla/v1", capabilities: [] },
      "/ocla/v1/envelope": envelope,
      "/ocla/v1/ledger/summary": { events: 2, tokens: 40, usd: 0.01 },
      "/ocla/v1/capsule": { capsule_ref: "capsule:1" },
      "/ocla/v1/capsule/capsule:1": {
        capsule_ref: "capsule:1",
        data: "capsule data",
      },
      "/ocla/v1/capsule/capsule:1/fork": { capsule_ref: "capsule:2" },
    };
    const calls: Array<{ path: string; init?: RequestInit }> = [];
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
        const url = new URL(String(input));
        calls.push({ path: url.pathname, init });
        return new Response(JSON.stringify(responses[url.pathname]), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }),
    );

    const client = new OclaClient("https://example.test/");
    await expect(client.health()).resolves.toEqual(responses["/ocla/v1/health"]);
    await expect(client.capabilities()).resolves.toEqual(
      responses["/ocla/v1/capabilities"],
    );
    await expect(client.validateEnvelope(envelope)).resolves.toEqual(envelope);
    await expect(client.registerCapsule("capsule data")).resolves.toBe(
      "capsule:1",
    );
    await expect(client.resolveCapsule("capsule:1")).resolves.toEqual(
      responses["/ocla/v1/capsule/capsule:1"],
    );
    await expect(client.forkCapsule("capsule:1", 1000)).resolves.toBe(
      "capsule:2",
    );
    await expect(client.ledgerSummary()).resolves.toEqual(
      responses["/ocla/v1/ledger/summary"],
    );

    expect(calls.map(({ path, init }) => [path, init?.method])).toEqual([
      ["/ocla/v1/health", "GET"],
      ["/ocla/v1/capabilities", "GET"],
      ["/ocla/v1/envelope", "POST"],
      ["/ocla/v1/capsule", "POST"],
      ["/ocla/v1/capsule/capsule:1", "GET"],
      ["/ocla/v1/capsule/capsule:1/fork", "POST"],
      ["/ocla/v1/ledger/summary", "GET"],
    ]);
  });

  it("sends plain capsule data and JSON fork budgets", async () => {
    const fetchMock = vi.fn(
      async (input: RequestInfo | URL, init?: RequestInit) => {
        const path = new URL(String(input)).pathname;
        if (path === "/ocla/v1/capsule") {
          expect(init?.headers).toEqual({
            Accept: "application/json",
            "Content-Type": "text/plain",
          });
          expect(init?.body).toBe("capsule data");
          return new Response(JSON.stringify({ capsule_ref: "capsule:1" }), {
            status: 200,
          });
        }
        expect(init?.headers).toEqual({
          Accept: "application/json",
          "Content-Type": "application/json",
        });
        expect(init?.body).toBe(JSON.stringify({ budget_tokens: 1000 }));
        return new Response(JSON.stringify({ capsule_ref: "capsule:2" }), {
          status: 200,
        });
      },
    );
    vi.stubGlobal("fetch", fetchMock);

    const client = new OclaClient("https://example.test");
    await expect(client.registerCapsule("capsule data")).resolves.toBe(
      "capsule:1",
    );
    await expect(client.forkCapsule("capsule:1", 1000)).resolves.toBe(
      "capsule:2",
    );
  });

  it("serializes envelopes as JSON and sends JSON headers", async () => {
    const fetchMock = vi.fn(async (_input: RequestInfo | URL, init?: RequestInit) => {
      expect(init?.headers).toEqual({
        Accept: "application/json",
        "Content-Type": "application/json",
      });
      expect(init?.body).toBe(JSON.stringify(envelope));
      return new Response(JSON.stringify(envelope), { status: 200 });
    });
    vi.stubGlobal("fetch", fetchMock);

    await expect(new OclaClient("https://example.test").validateEnvelope(envelope)).resolves.toEqual(
      envelope,
    );
    expect(fetchMock).toHaveBeenCalledOnce();
  });

  it("throws an error containing the status and response body", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => new Response("invalid envelope", { status: 400 })),
    );

    await expect(new OclaClient("https://example.test").validateEnvelope({})).rejects.toThrow(
      "OCLA request failed (400): invalid envelope",
    );
  });
});
