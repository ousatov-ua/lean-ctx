/** JSON object accepted by the OCLA envelope endpoint. */
export type JsonObject = Record<string, unknown>;
export type OclaApiVersion = "ocla/v1";
export interface HealthResponse {
  status: "ok";
  version: OclaApiVersion | string;
}
export type OclaCapabilityKind =
  | "observation_hook"
  | "usage_sink"
  | "metrics_exporter"
  | "savings_ledger"
  | "intent_classifier"
  | "outcome_tracker"
  | "compression_provider"
  | "response_optimizer"
  | "model_router"
  | "efficiency_analyzer"
  | "config_tuner"
  | "experiment_runner"
  | "connector_scheduler"
  | "agent_gateway";
export type OclaCapabilityStatus = "available" | "degraded" | "unavailable";
export interface OclaCapability {
  kind: OclaCapabilityKind;
  api_version: OclaApiVersion | string;
  status: OclaCapabilityStatus;
  limits: Record<string, number>;
}
export interface CapabilitiesResponse {
  version: OclaApiVersion | string;
  capabilities: OclaCapability[];
}
export interface OclaRequestContext {
  request_id: string;
  session_id: string;
  agent_id: string;
  content_ref: string;
  tenant_id: string | null;
}
export type TokenEnvelopeSurface = "mcp" | "proxy" | "shell" | "agent";
export type TokenFlowDirection = "input" | "output";
export interface TokenBalanceV1 {
  original_tokens: number;
  materialized_tokens: number;
  delivered_tokens: number;
  provider_billed_tokens: number;
}
export interface CanonicalTokenEnvelopeV1 {
  schema_version: 1;
  context: OclaRequestContext;
  surface: TokenEnvelopeSurface;
  direction: TokenFlowDirection;
  provider: string;
  model: string;
  token_balance: TokenBalanceV1;
  route_ref: string | null;
  policy_ref: string | null;
  idempotency_key: string;
}
export interface AgentEnvelopeV1 {
  schema_version: 1;
  relay_id: string;
  context: OclaRequestContext;
  from_agent_id: string;
  to_agent_id: string;
  capsule_ref: string;
  budget_tokens: number;
}
export type EnvelopeResponse = CanonicalTokenEnvelopeV1;
export interface LedgerSummary {
  events: number;
  tokens: number;
  usd: number;
}
export interface OclaErrorResponse {
  error: string;
}

// --- Context Kernel Wire Types ---

export type SensitivityLevel = "public" | "internal" | "confidential" | "restricted";
export type ReceiptOutcome = "accepted" | "rejected" | "partial" | "unknown";

export interface QualitySignal {
  name: string;
  value: number;
}

export interface PlanBudget {
  total: number;
  used: number;
}

export interface PlanEntry {
  object_id: string;
  provider: string;
  view: string;
  tokens: number;
  phi: number;
  reason: string;
}

export interface ExcludedEntry {
  object_id: string;
  reason: string;
}

export interface ContextPlanV1 {
  plan_id: string;
  intent: string;
  budget: PlanBudget;
  selected: PlanEntry[];
  excluded: ExcludedEntry[];
  deferred: ExcludedEntry[];
  provider_stats: Record<string, { candidates: number; selected: number }>;
}

export interface ContextReceiptV1 {
  receipt_id: string;
  plan_id: string;
  delivered_tokens: number;
  cache_hits: number;
  cache_misses: number;
  outcome: ReceiptOutcome;
  quality_signals: QualitySignal[];
  feedback_attribution: Record<string, number>;
}

export interface ContextPolicy {
  max_sensitivity: SensitivityLevel;
  allowed_sources: string[] | null;
  blocked_sources: string[];
  budget_cap_tokens: number | null;
  retention_days: number | null;
}

export interface AttributionEntry {
  provider: string;
  tokens_contributed: number;
  tokens_saved: number;
  efficiency: number;
}

export interface AttributionReport {
  plan_id: string;
  receipt_id: string;
  total_tokens_delivered: number;
  total_tokens_saved: number;
  entries: AttributionEntry[];
}
