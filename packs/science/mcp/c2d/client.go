// Package c2d implements Oasis C2D compute MCP tools (user token required).
package c2d

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"strings"
	"time"
)

const apiVersion = "oasis-c2d-v1"

// Client calls Oasis compute APIs.
type Client struct {
	BaseURL    string
	Token      string
	HTTPClient *http.Client
}

// NewClient creates a C2D API client.
func NewClient(baseURL, token string) *Client {
	baseURL = strings.TrimRight(strings.TrimSpace(baseURL), "/")
	if baseURL == "" {
		baseURL = "https://demo.oasisdata2026.xyz"
	}
	return &Client{
		BaseURL:    baseURL,
		Token:      strings.TrimSpace(token),
		HTTPClient: &http.Client{Timeout: 45 * time.Second},
	}
}

func (c *Client) requireToken() error {
	if c.Token == "" {
		return fmt.Errorf("oasis: user OAuth token required for C2D — sign in to 绿洲")
	}
	return nil
}

// ListAlgorithms returns algorithms available for a dataset (buyer auth).
func (c *Client) ListAlgorithms(datasetID string) (any, error) {
	if err := c.requireToken(); err != nil {
		return nil, err
	}
	path := "/api/v1/compute/algorithms"
	if strings.TrimSpace(datasetID) != "" {
		path += "?dataset_id=" + url.QueryEscape(datasetID)
	}
	return c.getJSON(path)
}

// ListOfferSignals returns public compute discovery signals (anonymous).
func (c *Client) ListOfferSignals() (any, error) {
	return c.getJSON("/api/v1/compute/offers/signals")
}

// SubmitJob starts a C2D compute job.
func (c *Client) SubmitJob(datasetID, algorithmID, entitlementID string, params map[string]any) (any, error) {
	if err := c.requireToken(); err != nil {
		return nil, err
	}
	body := map[string]any{
		"dataset_id":     datasetID,
		"algorithm_id":   algorithmID,
		"entitlement_id": entitlementID,
		"params":         params,
	}
	return c.postJSON("/api/v1/compute/jobs", body)
}

// GetJobStatus fetches job status by id.
func (c *Client) GetJobStatus(jobID string) (any, error) {
	if err := c.requireToken(); err != nil {
		return nil, err
	}
	jobID = strings.TrimSpace(jobID)
	if jobID == "" {
		return nil, fmt.Errorf("job_id is required")
	}
	return c.getJSON("/api/v1/compute/jobs/" + url.PathEscape(jobID))
}

func (c *Client) getJSON(path string) (any, error) {
	req, err := http.NewRequest(http.MethodGet, c.BaseURL+path, nil)
	if err != nil {
		return nil, err
	}
	return c.do(req, false)
}

func (c *Client) postJSON(path string, body map[string]any) (any, error) {
	b, err := json.Marshal(body)
	if err != nil {
		return nil, err
	}
	req, err := http.NewRequest(http.MethodPost, c.BaseURL+path, bytes.NewReader(b))
	if err != nil {
		return nil, err
	}
	req.Header.Set("Content-Type", "application/json")
	return c.do(req, true)
}

func (c *Client) do(req *http.Request, needAuth bool) (any, error) {
	req.Header.Set("Accept", "application/json")
	if needAuth || c.Token != "" {
		if c.Token == "" {
			return nil, fmt.Errorf("oasis: auth required")
		}
		req.Header.Set("Authorization", "Bearer "+c.Token)
	}
	resp, err := c.HTTPClient.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	raw, err := io.ReadAll(io.LimitReader(resp.Body, 4<<20))
	if err != nil {
		return nil, err
	}
	if resp.StatusCode >= 400 {
		return nil, fmt.Errorf("c2d http %d: %s", resp.StatusCode, strings.TrimSpace(string(raw)))
	}
	var env struct {
		Code int             `json:"code"`
		Data json.RawMessage `json:"data"`
	}
	if err := json.Unmarshal(raw, &env); err != nil {
		return nil, err
	}
	if env.Code != 0 {
		return nil, fmt.Errorf("c2d api code %d", env.Code)
	}
	if len(env.Data) == 0 {
		return map[string]any{}, nil
	}
	var data any
	if err := json.Unmarshal(env.Data, &data); err != nil {
		return nil, err
	}
	return data, nil
}
