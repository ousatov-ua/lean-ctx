package ocla

import "encoding/json"

const OclaAPIVersion = "ocla/v1"

type HealthResponse struct {
	Overall        json.RawMessage   `json:"overall"`
	Components     []ComponentHealth `json:"components"`
	UptimeSeconds  uint64            `json:"uptime_seconds"`
	Version        string            `json:"version"`
}

type ComponentHealth struct {
	Name      string          `json:"name"`
	Status    json.RawMessage `json:"status"`
	LatencyMS *uint64         `json:"latency_ms"`
}

type CapabilitiesResponse struct {
	Version      string       `json:"version"`
	Capabilities []Capability `json:"capabilities"`
}

type Capability struct {
	Kind       string            `json:"kind"`
	APIVersion string            `json:"api_version"`
	Status     string            `json:"status"`
	Limits     map[string]uint64 `json:"limits"`
}

type OclaRequestContext struct {
	RequestID  string  `json:"request_id"`
	SessionID  string  `json:"session_id"`
	AgentID    string  `json:"agent_id"`
	ContentRef string  `json:"content_ref"`
	TenantID   *string `json:"tenant_id"`
	TraceID    string  `json:"trace_id,omitempty"`
}

type TokenBalance struct {
	OriginalTokens       uint64 `json:"original_tokens"`
	MaterializedTokens   uint64 `json:"materialized_tokens"`
	DeliveredTokens      uint64 `json:"delivered_tokens"`
	ProviderBilledTokens uint64 `json:"provider_billed_tokens"`
}

type EnvelopeRequest struct {
	SchemaVersion  uint16              `json:"schema_version"`
	Context        OclaRequestContext   `json:"context"`
	Surface        string              `json:"surface"`
	Direction      string              `json:"direction"`
	Provider       string              `json:"provider"`
	Model          string              `json:"model"`
	TokenBalance   TokenBalance        `json:"token_balance"`
	RouteRef       *string             `json:"route_ref"`
	PolicyRef      *string             `json:"policy_ref"`
	IdempotencyKey string              `json:"idempotency_key"`
}

type EnvelopeResponse struct {
	SchemaVersion  uint16              `json:"schema_version"`
	Context        OclaRequestContext   `json:"context"`
	Surface        string              `json:"surface"`
	Direction      string              `json:"direction"`
	Provider       string              `json:"provider"`
	Model          string              `json:"model"`
	TokenBalance   TokenBalance        `json:"token_balance"`
	RouteRef       *string             `json:"route_ref"`
	PolicyRef      *string             `json:"policy_ref"`
	IdempotencyKey string              `json:"idempotency_key"`
}

type BatchEnvelopeResult struct {
	Valid    bool              `json:"valid"`
	Envelope *EnvelopeResponse `json:"envelope,omitempty"`
	Error    string            `json:"error,omitempty"`
}

type BatchResponse []BatchEnvelopeResult

type AgentsResponse struct {
	Agents []json.RawMessage `json:"agents"`
}

type MetricsResponse struct {
	TotalEvents        uint64  `json:"total_events"`
	SavedTokens        uint64  `json:"saved_tokens"`
	SavedUSD           float64 `json:"saved_usd"`
	TraitAdoptionCount uint64  `json:"trait_adoption_count"`
}

type LedgerSummaryResponse struct {
	Events uint64  `json:"events"`
	Tokens uint64  `json:"tokens"`
	USD    float64 `json:"usd"`
}

type CapsuleData struct {
	CapsuleRef string `json:"capsule_ref"`
	Data       string `json:"data"`
}

type ErrorResponse struct {
	Error string `json:"error"`
	Code  string `json:"code,omitempty"`
}
