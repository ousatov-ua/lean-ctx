import { describe, expect, it } from "vitest";

import { LeanCtxClient } from "./client.js";

describe("LeanCtxClient", () => {
  it("normalizes trailing slash", () => {
    const c = new LeanCtxClient({ baseUrl: "http://127.0.0.1:8080/" });
    expect(c.baseUrl).toBe("http://127.0.0.1:8080");
  });

  it("posts tool calls to /v1/tools/call", async () => {
    const calls: Array<{ url: string; init?: RequestInit }> = [];
    const fetchImpl: typeof fetch = async (url, init) => {
      calls.push({ url: String(url), init });
      return new Response(
        JSON.stringify({ result: { content: [{ type: "text", text: "ok" }] } }),
        { status: 200, headers: { "content-type": "application/json" } }
      );
    };

    const c = new LeanCtxClient({
      baseUrl: "http://127.0.0.1:8080",
      fetchImpl,
    });
    const r = await c.callToolResult("ctx_read", { path: "README.md" });

    expect(calls).toHaveLength(1);
    expect(calls[0]?.url).toBe("http://127.0.0.1:8080/v1/tools/call");
    expect(calls[0]?.init?.method).toBe("POST");
    expect(r).toEqual({ content: [{ type: "text", text: "ok" }] });
  });

  it("includes workspaceId/channelId in tool calls and headers", async () => {
    const calls: Array<{ url: string; init?: RequestInit; body?: unknown }> =
      [];
    const fetchImpl: typeof fetch = async (url, init) => {
      const body = init?.body ? JSON.parse(String(init.body)) : undefined;
      calls.push({ url: String(url), init, body });
      return new Response(JSON.stringify({ result: { ok: true } }), {
        status: 200,
        headers: { "content-type": "application/json" },
      });
    };

    const c = new LeanCtxClient({
      baseUrl: "http://127.0.0.1:8080",
      fetchImpl,
      workspaceId: "ws1",
      channelId: "ch1",
    });
    await c.callToolResult("ctx_tree", { path: ".", depth: 1 });

    expect(calls).toHaveLength(1);
    expect(calls[0]?.url).toBe("http://127.0.0.1:8080/v1/tools/call");
    expect((calls[0]?.body as any)?.workspaceId).toBe("ws1");
    expect((calls[0]?.body as any)?.channelId).toBe("ch1");
    expect((calls[0]?.init?.headers as any)?.["x-leanctx-workspace"]).toBe(
      "ws1"
    );
  });

  it("subscribes to /v1/events and parses SSE events", async () => {
    const fetchImpl: typeof fetch = async (url, init) => {
      expect(String(url)).toContain("/v1/events");
      expect(String(url)).toContain("since=5");
      expect(String(url)).toContain("limit=1");
      expect((init?.headers as any)?.Accept).toBe("text/event-stream");
      expect((init?.headers as any)?.["x-leanctx-workspace"]).toBe("ws1");

      const sse =
        "id: 1\n" +
        "event: tool_call_recorded\n" +
        'data: {"id":1,"workspaceId":"ws1","channelId":"ch1","kind":"tool_call_recorded","actor":null,"timestamp":"2026-01-01T00:00:00Z","payload":{"tool":"ctx_tree"}}\n' +
        "\n";

      const body = new ReadableStream<Uint8Array>({
        start(controller) {
          controller.enqueue(new TextEncoder().encode(sse));
          controller.close();
        },
      });

      return new Response(body, {
        status: 200,
        headers: { "content-type": "text/event-stream" },
      });
    };

    const c = new LeanCtxClient({
      baseUrl: "http://127.0.0.1:8080",
      fetchImpl,
      workspaceId: "ws1",
      channelId: "ch1",
    });
    const it = c
      .subscribeEvents({ since: 5, limit: 1 })
      [Symbol.asyncIterator]();
    const first = await it.next();
    expect(first.done).toBe(false);
    expect(first.value.kind).toBe("tool_call_recorded");
    expect(first.value.workspaceId).toBe("ws1");
    expect(first.value.channelId).toBe("ch1");
    expect((first.value.payload as any).tool).toBe("ctx_tree");
  });
});
