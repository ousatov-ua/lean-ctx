package ocla

import (
	"context"
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"reflect"
	"strings"
	"testing"
)

var testEnvelope = EnvelopeRequest{
	SchemaVersion: 1,
	Context: OclaRequestContext{
		RequestID: "request-1", SessionID: "session-1", AgentID: "agent-1",
		ContentRef: "blake3:content",
	},
	Surface: "proxy", Direction: "input", Provider: "openai", Model: "gpt-5",
	TokenBalance: TokenBalance{
		OriginalTokens: 100, MaterializedTokens: 80, DeliveredTokens: 60,
		ProviderBilledTokens: 60,
	},
	RouteRef: stringPointer("route-1"), IdempotencyKey: "request-1:input",
}

func stringPointer(value string) *string { return &value }

func TestClientCallsEveryEndpoint(t *testing.T) {
	requests := make([]string, 0, 7)
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		requests = append(requests, r.Method+" "+r.URL.Path)
		w.Header().Set("Content-Type", "application/json")
		switch r.URL.Path {
		case "/ocla/v1/health":
			_, _ = io.WriteString(w, `{"overall":"healthy","components":[],"uptime_seconds":4,"version":"ocla/v1"}`)
		case "/ocla/v1/capabilities":
			_, _ = io.WriteString(w, `{"version":"ocla/v1","capabilities":[]}`)
		case "/ocla/v1/envelope":
			if r.Method != http.MethodPost {
				t.Errorf("envelope method = %s", r.Method)
			}
			_ = json.NewEncoder(w).Encode(testEnvelopeResponse())
		case "/ocla/v1/envelope/batch":
			if r.Method != http.MethodPost {
				t.Errorf("batch method = %s", r.Method)
			}
			var envelopes []json.RawMessage
			if err := json.NewDecoder(r.Body).Decode(&envelopes); err != nil {
				t.Errorf("decode batch body: %v", err)
			}
			if len(envelopes) != 1 || string(envelopes[0]) != `{"schema_version":1}` {
				t.Errorf("batch body = %s", envelopes)
			}
			_ = json.NewEncoder(w).Encode([]BatchEnvelopeResult{{Valid: true}})
		case "/ocla/v1/agents":
			_, _ = io.WriteString(w, `{"agents":[]}`)
		case "/ocla/v1/metrics":
			_, _ = io.WriteString(w, `{"total_events":2,"saved_tokens":40,"saved_usd":0.01,"trait_adoption_count":14}`)
		case "/ocla/v1/ledger/summary":
			_, _ = io.WriteString(w, `{"events":2,"tokens":40,"usd":0.01}`)
		default:
			http.NotFound(w, r)
		}
	}))
	defer server.Close()

	client := NewClient(server.URL)
	if _, err := client.Health(); err != nil {
		t.Fatal(err)
	}
	if _, err := client.Capabilities(); err != nil {
		t.Fatal(err)
	}
	if response, err := client.ValidateEnvelope(testEnvelope); err != nil || response.Provider != "openai" {
		t.Fatalf("envelope = %#v, err = %v", response, err)
	}
	batch := []json.RawMessage{json.RawMessage(`{"schema_version":1}`)}
	if response, err := client.ValidateEnvelopeBatch(batch); err != nil || len(response) != 1 || !response[0].Valid {
		t.Fatalf("batch = %#v, err = %v", response, err)
	}
	if response, err := client.Agents(); err != nil || response.Agents == nil {
		t.Fatalf("agents = %#v, err = %v", response, err)
	}
	if response, err := client.Metrics(); err != nil || response.SavedTokens != 40 {
		t.Fatalf("metrics = %#v, err = %v", response, err)
	}
	if response, err := client.LedgerSummary(); err != nil || response.Tokens != 40 {
		t.Fatalf("ledger = %#v, err = %v", response, err)
	}

	want := []string{
		"GET /ocla/v1/health", "GET /ocla/v1/capabilities",
		"POST /ocla/v1/envelope", "POST /ocla/v1/envelope/batch",
		"GET /ocla/v1/agents", "GET /ocla/v1/metrics",
		"GET /ocla/v1/ledger/summary",
	}
	if !reflect.DeepEqual(requests, want) {
		t.Fatalf("requests = %#v, want %#v", requests, want)
	}
}

func TestClientCallsCapsuleEndpoints(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		switch r.URL.Path {
		case "/ocla/v1/capsule":
			if r.Method != http.MethodPost {
				t.Errorf("register method = %s", r.Method)
			}
			body, err := io.ReadAll(r.Body)
			if err != nil {
				t.Fatal(err)
			}
			if string(body) != "capsule data" {
				t.Errorf("register body = %q", body)
			}
			if got := r.Header.Get("Content-Type"); got != "text/plain" {
				t.Errorf("register content type = %q", got)
			}
			_, _ = io.WriteString(w, `{"capsule_ref":"capsule:1"}`)
		case "/ocla/v1/capsule/capsule:1":
			if r.Method != http.MethodGet {
				t.Errorf("resolve method = %s", r.Method)
			}
			_ = json.NewEncoder(w).Encode(CapsuleData{
				CapsuleRef: "capsule:1", Data: "capsule data",
			})
		case "/ocla/v1/capsule/capsule:1/fork":
			if r.Method != http.MethodPost {
				t.Errorf("fork method = %s", r.Method)
			}
			var payload map[string]int64
			if err := json.NewDecoder(r.Body).Decode(&payload); err != nil {
				t.Fatal(err)
			}
			if payload["budget_tokens"] != 1000 {
				t.Errorf("fork payload = %#v", payload)
			}
			_, _ = io.WriteString(w, `{"capsule_ref":"capsule:2"}`)
		default:
			http.NotFound(w, r)
		}
	}))
	defer server.Close()

	client := NewClient(server.URL)
	registered, err := client.RegisterCapsule(context.Background(), "capsule data")
	if err != nil || registered != "capsule:1" {
		t.Fatalf("registered = %q, err = %v", registered, err)
	}
	resolved, err := client.ResolveCapsule(context.Background(), registered)
	if err != nil || resolved.CapsuleRef != registered || resolved.Data != "capsule data" {
		t.Fatalf("resolved = %#v, err = %v", resolved, err)
	}
	forked, err := client.ForkCapsule(context.Background(), registered, 1000)
	if err != nil || forked != "capsule:2" {
		t.Fatalf("forked = %q, err = %v", forked, err)
	}
}

func TestClientSendsJSONAndBearerAPIKey(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if got := r.Header.Get("Authorization"); got != "Bearer secret" {
			t.Errorf("authorization = %q", got)
		}
		if got := r.Header.Get("Content-Type"); got != "application/json" {
			t.Errorf("content type = %q", got)
		}
		body, err := io.ReadAll(r.Body)
		if err != nil {
			t.Fatal(err)
		}
		var got EnvelopeRequest
		if err := json.Unmarshal(body, &got); err != nil {
			t.Fatal(err)
		}
		if !reflect.DeepEqual(got, testEnvelope) {
			t.Fatalf("body = %#v, want %#v", got, testEnvelope)
		}
		w.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(w).Encode(testEnvelopeResponse())
	}))
	defer server.Close()

	if _, err := NewClient(server.URL, WithAPIKey("secret")).ValidateEnvelope(testEnvelope); err != nil {
		t.Fatal(err)
	}
}

func TestClientOptionsConfigureTransport(t *testing.T) {
	custom := &http.Client{}
	client := NewClient(" https://example.test/// ", WithHTTPClient(custom), WithTimeout(3))
	if client.baseURL != "https://example.test" {
		t.Fatalf("baseURL = %q", client.baseURL)
	}
	if client.httpClient != custom || client.httpClient.Timeout != 3 {
		t.Fatalf("options did not configure client: %#v", client)
	}
}

func TestClientReturnsTypedAPIError(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusBadRequest)
		_, _ = io.WriteString(w, `{"error":"invalid envelope","code":"invalid_request"}`)
	}))
	defer server.Close()

	_, err := NewClient(server.URL).Health()
	var apiError *APIError
	if !strings.Contains(err.Error(), "invalid envelope") || !reflect.TypeOf(err).AssignableTo(reflect.TypeOf(apiError)) {
		t.Fatalf("error = %T %v", err, err)
	}
	if !strings.Contains(err.Error(), "400") {
		t.Fatalf("error = %v", err)
	}
}

func testEnvelopeResponse() EnvelopeResponse {
	return EnvelopeResponse{
		SchemaVersion: testEnvelope.SchemaVersion, Context: testEnvelope.Context,
		Surface: testEnvelope.Surface, Direction: testEnvelope.Direction,
		Provider: testEnvelope.Provider, Model: testEnvelope.Model,
		TokenBalance: testEnvelope.TokenBalance, RouteRef: testEnvelope.RouteRef,
		PolicyRef: testEnvelope.PolicyRef, IdempotencyKey: testEnvelope.IdempotencyKey,
	}
}
