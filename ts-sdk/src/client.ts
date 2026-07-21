import type {
  CapabilitiesResponse,
  EnvelopeResponse,
  HealthResponse,
  JsonObject,
  LedgerSummary,
} from "./types.js";

export type CapsuleData = { capsule_ref: string; data: string };

export class OclaClient {
  readonly baseUrl: string;

  constructor(baseUrl: string) {
    const normalized = baseUrl.trim().replace(/\/+$/, "");
    if (!normalized) {
      throw new Error("OclaClient: baseUrl is required");
    }
    this.baseUrl = normalized;
  }

  async health(): Promise<HealthResponse> {
    return this.get<HealthResponse>("/ocla/v1/health");
  }
  async capabilities(): Promise<CapabilitiesResponse> {
    return this.get<CapabilitiesResponse>("/ocla/v1/capabilities");
  }
  async validateEnvelope(envelope: object): Promise<EnvelopeResponse> {
    return this.request<EnvelopeResponse>("/ocla/v1/envelope", {
      method: "POST",
      body: JSON.stringify(envelope),
    });
  }
  async registerCapsule(data: string): Promise<string> {
    const response = await this.request<{ capsule_ref: string }>(
      "/ocla/v1/capsule",
      {
        method: "POST",
        body: data,
        headers: { "Content-Type": "text/plain" },
      },
    );
    return response.capsule_ref;
  }
  async resolveCapsule(capsuleRef: string): Promise<CapsuleData> {
    return this.get<CapsuleData>(`/ocla/v1/capsule/${capsuleRef}`);
  }
  async forkCapsule(capsuleRef: string, budgetTokens: number): Promise<string> {
    const response = await this.request<{ capsule_ref: string }>(
      `/ocla/v1/capsule/${capsuleRef}/fork`,
      {
        method: "POST",
        body: JSON.stringify({ budget_tokens: budgetTokens }),
      },
    );
    return response.capsule_ref;
  }
  async ledgerSummary(): Promise<LedgerSummary> {
    return this.get<LedgerSummary>("/ocla/v1/ledger/summary");
  }
  private async get<T>(path: string): Promise<T> {
    return this.request<T>(path, { method: "GET" });
  }
  private async request<T>(path: string, init: RequestInit): Promise<T> {
    const response = await fetch(`${this.baseUrl}${path}`, {
      ...init,
      headers: {
        Accept: "application/json",
        ...(init.body === undefined ? {} : { "Content-Type": "application/json" }),
        ...init.headers,
      },
    });

    if (!response.ok) {
      const detail = await response.text();
      const suffix = detail.trim() ? `: ${detail.trim()}` : "";
      throw new Error(`OCLA request failed (${response.status})${suffix}`);
    }

    return (await response.json()) as T;
  }
}
export type { JsonObject };
