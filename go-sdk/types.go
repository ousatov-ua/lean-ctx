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

// --- Context Kernel Wire Types ---

type SensitivityLevel string

const (
	SensitivityPublic       SensitivityLevel = "public"
	SensitivityInternal     SensitivityLevel = "internal"
	SensitivityConfidential SensitivityLevel = "confidential"
	SensitivityRestricted   SensitivityLevel = "restricted"
)

type ReceiptOutcome string

const (
	OutcomeAccepted ReceiptOutcome = "accepted"
	OutcomeRejected ReceiptOutcome = "rejected"
	OutcomePartial  ReceiptOutcome = "partial"
	OutcomeUnknown  ReceiptOutcome = "unknown"
)

type QualitySignal struct {
	Name  string  `json:"name"`
	Value float64 `json:"value"`
}

type PlanBudget struct {
	Total uint64 `json:"total"`
	Used  uint64 `json:"used"`
}

type PlanEntry struct {
	ObjectID string  `json:"object_id"`
	Provider string  `json:"provider"`
	View     string  `json:"view"`
	Tokens   uint64  `json:"tokens"`
	Phi      float64 `json:"phi"`
	Reason   string  `json:"reason"`
}

type ContextPlanV1 struct {
	PlanID        string                  `json:"plan_id"`
	Intent        string                  `json:"intent"`
	Budget        PlanBudget              `json:"budget"`
	Selected      []PlanEntry             `json:"selected"`
	Excluded      []ExcludedEntry         `json:"excluded"`
	Deferred      []ExcludedEntry         `json:"deferred"`
	ProviderStats map[string]ProviderStat `json:"provider_stats"`
}

type ExcludedEntry struct {
	ObjectID string `json:"object_id"`
	Reason   string `json:"reason"`
}

type ProviderStat struct {
	Candidates uint64 `json:"candidates"`
	Selected   uint64 `json:"selected"`
}

type ContextReceiptV1 struct {
	ReceiptID           string             `json:"receipt_id"`
	PlanID              string             `json:"plan_id"`
	DeliveredTokens     uint64             `json:"delivered_tokens"`
	CacheHits           uint64             `json:"cache_hits"`
	CacheMisses         uint64             `json:"cache_misses"`
	Outcome             ReceiptOutcome     `json:"outcome"`
	QualitySignals      []QualitySignal    `json:"quality_signals"`
	FeedbackAttribution map[string]float64 `json:"feedback_attribution"`
}

type ContextPolicy struct {
	MaxSensitivity  SensitivityLevel `json:"max_sensitivity"`
	AllowedSources  []string         `json:"allowed_sources"`
	BlockedSources  []string         `json:"blocked_sources"`
	BudgetCapTokens *uint64          `json:"budget_cap_tokens"`
	RetentionDays   *uint32          `json:"retention_days"`
}

type AttributionEntry struct {
	Provider          string  `json:"provider"`
	TokensContributed uint64  `json:"tokens_contributed"`
	TokensSaved       uint64  `json:"tokens_saved"`
	Efficiency        float64 `json:"efficiency"`
}

type AttributionReport struct {
	PlanID               string             `json:"plan_id"`
	ReceiptID            string             `json:"receipt_id"`
	TotalTokensDelivered uint64             `json:"total_tokens_delivered"`
	TotalTokensSaved     uint64             `json:"total_tokens_saved"`
	Entries              []AttributionEntry `json:"entries"`
}
