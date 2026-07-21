package ocla

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"strings"
	"time"
)

type OclaClient struct {
	baseURL    string
	httpClient *http.Client
	apiKey     string
}

type Option func(*OclaClient)

func WithHTTPClient(client *http.Client) Option {
	return func(c *OclaClient) {
		if client != nil {
			c.httpClient = client
		}
	}
}

func WithAPIKey(apiKey string) Option {
	return func(c *OclaClient) { c.apiKey = apiKey }
}

func WithTimeout(timeout time.Duration) Option {
	return func(c *OclaClient) {
		if timeout >= 0 {
			c.httpClient.Timeout = timeout
		}
	}
}

func NewClient(baseURL string, opts ...Option) *OclaClient {
	c := &OclaClient{
		baseURL:    strings.TrimRight(strings.TrimSpace(baseURL), "/"),
		httpClient: &http.Client{},
	}
	for _, opt := range opts {
		if opt != nil {
			opt(c)
		}
	}
	return c
}

func (c *OclaClient) Health() (HealthResponse, error) {
	var response HealthResponse
	err := c.request(http.MethodGet, "/ocla/v1/health", nil, &response)
	return response, err
}

func (c *OclaClient) Capabilities() (CapabilitiesResponse, error) {
	var response CapabilitiesResponse
	err := c.request(http.MethodGet, "/ocla/v1/capabilities", nil, &response)
	return response, err
}

func (c *OclaClient) ValidateEnvelope(envelope EnvelopeRequest) (EnvelopeResponse, error) {
	var response EnvelopeResponse
	err := c.request(http.MethodPost, "/ocla/v1/envelope", envelope, &response)
	return response, err
}

func (c *OclaClient) ValidateEnvelopeBatch(envelopes []json.RawMessage) (BatchResponse, error) {
	var response BatchResponse
	err := c.request(http.MethodPost, "/ocla/v1/envelope/batch", envelopes, &response)
	return response, err
}

func (c *OclaClient) Agents() (AgentsResponse, error) {
	var response AgentsResponse
	err := c.request(http.MethodGet, "/ocla/v1/agents", nil, &response)
	return response, err
}

func (c *OclaClient) Metrics() (MetricsResponse, error) {
	var response MetricsResponse
	err := c.request(http.MethodGet, "/ocla/v1/metrics", nil, &response)
	return response, err
}

func (c *OclaClient) LedgerSummary() (LedgerSummaryResponse, error) {
	var response LedgerSummaryResponse
	err := c.request(http.MethodGet, "/ocla/v1/ledger/summary", nil, &response)
	return response, err
}

func (c *OclaClient) request(method, path string, payload any, target any) error {
	var body io.Reader
	if payload != nil {
		encoded, err := json.Marshal(payload)
		if err != nil {
			return fmt.Errorf("encode OCLA request: %w", err)
		}
		body = bytes.NewReader(encoded)
	}

	req, err := http.NewRequest(method, c.baseURL+path, body)
	if err != nil {
		return fmt.Errorf("create OCLA request: %w", err)
	}
	req.Header.Set("Accept", "application/json")
	if payload != nil {
		req.Header.Set("Content-Type", "application/json")
	}
	if c.apiKey != "" {
		req.Header.Set("Authorization", "Bearer "+c.apiKey)
	}

	response, err := c.httpClient.Do(req)
	if err != nil {
		return fmt.Errorf("send OCLA request: %w", err)
	}
	defer response.Body.Close()

	if response.StatusCode < http.StatusOK || response.StatusCode >= http.StatusMultipleChoices {
		body, readErr := io.ReadAll(response.Body)
		if readErr != nil {
			return &APIError{StatusCode: response.StatusCode, ReadError: readErr}
		}
		apiError := &APIError{StatusCode: response.StatusCode, Body: body}
		_ = json.Unmarshal(body, &apiError.Response)
		return apiError
	}

	if err := json.NewDecoder(response.Body).Decode(target); err != nil {
		return fmt.Errorf("decode OCLA response: %w", err)
	}
	return nil
}

type APIError struct {
	StatusCode int
	Response   ErrorResponse
	Body       []byte
	ReadError  error
}

func (e *APIError) Error() string {
	if e.ReadError != nil {
		return fmt.Sprintf("OCLA request failed with status %d: %v", e.StatusCode, e.ReadError)
	}
	if e.Response.Error != "" {
		return fmt.Sprintf("OCLA request failed with status %d: %s", e.StatusCode, e.Response.Error)
	}
	return fmt.Sprintf("OCLA request failed with status %d", e.StatusCode)
}
