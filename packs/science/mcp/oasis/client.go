// Package oasis implements the 绿洲 marketplace MCP client (auth combo 1+3).
package oasis

import (
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"strings"
	"time"
)

const apiVersion = "oasis-v1"

// Client calls the Oasis marketplace REST API.
type Client struct {
	BaseURL    string
	Token      string // user OAuth access token (optional for anonymous tools)
	HTTPClient *http.Client
}

// NewClient returns a client with sensible defaults.
func NewClient(baseURL, token string) *Client {
	baseURL = strings.TrimRight(strings.TrimSpace(baseURL), "/")
	if baseURL == "" {
		baseURL = "https://demo.oasisdata2026.xyz"
	}
	return &Client{
		BaseURL: baseURL,
		Token:   strings.TrimSpace(token),
		HTTPClient: &http.Client{
			Timeout: 30 * time.Second,
		},
	}
}

// DatasetSummary is a compact catalog row for agents.
type DatasetSummary struct {
	ID                string `json:"id"`
	Title             string `json:"title"`
	Description       string `json:"description,omitempty"`
	DataType          string `json:"data_type,omitempty"`
	Domain            string `json:"domain,omitempty"`
	LicenseType       string `json:"license_type,omitempty"`
	Status            string `json:"status,omitempty"`
	SampleCount       int64  `json:"sample_count,omitempty"`
	QualityVerified   *bool  `json:"quality_verified,omitempty"`
	AuthenticityBand  string `json:"authenticity_band,omitempty"`
	AuthenticityScore *int   `json:"authenticity_score,omitempty"`
}

// SearchDatasets queries GET /api/v1/datasets (anonymous).
func (c *Client) SearchDatasets(q string, limit, offset int) ([]DatasetSummary, error) {
	if limit <= 0 {
		limit = 20
	}
	if limit > 50 {
		limit = 50
	}
	params := url.Values{}
	if strings.TrimSpace(q) != "" {
		params.Set("q", q)
	}
	params.Set("limit", fmt.Sprintf("%d", limit))
	params.Set("offset", fmt.Sprintf("%d", offset))

	var out struct {
		Items []DatasetSummary `json:"items"`
	}
	if err := c.getJSON("/api/v1/datasets?"+params.Encode(), false, &out); err != nil {
		return nil, err
	}
	if out.Items == nil {
		return []DatasetSummary{}, nil
	}
	return out.Items, nil
}

// ListVerifiedDatasets returns published datasets with quality_verified=true.
func (c *Client) ListVerifiedDatasets(limit int) ([]DatasetSummary, error) {
	items, err := c.SearchDatasets("", 50, 0)
	if err != nil {
		return nil, err
	}
	verified := make([]DatasetSummary, 0, limit)
	for _, d := range items {
		if d.QualityVerified != nil && *d.QualityVerified {
			verified = append(verified, d)
			if limit > 0 && len(verified) >= limit {
				break
			}
		}
	}
	return verified, nil
}

// GetDatasetDetail fetches one dataset by id (anonymous).
func (c *Client) GetDatasetDetail(id string) (DatasetSummary, error) {
	id = strings.TrimSpace(id)
	if id == "" {
		return DatasetSummary{}, fmt.Errorf("dataset_id is required")
	}
	var d DatasetSummary
	if err := c.getJSON("/api/v1/datasets/"+url.PathEscape(id), false, &d); err != nil {
		return DatasetSummary{}, err
	}
	return d, nil
}

// PreviewSchema returns a capped sample preview (requires user token).
func (c *Client) PreviewSchema(datasetID string) (map[string]any, error) {
	datasetID = strings.TrimSpace(datasetID)
	if datasetID == "" {
		return nil, fmt.Errorf("dataset_id is required")
	}
	var prev struct {
		Lines       []string `json:"lines"`
		LineCount   int      `json:"line_count"`
		SampleCount int64    `json:"dataset_sample_count"`
		Truncated   bool     `json:"truncated"`
	}
	if err := c.getJSON("/api/v1/datasets/"+url.PathEscape(datasetID)+"/preview", true, &prev); err != nil {
		return nil, err
	}
	cols := inferColumns(prev.Lines)
	return map[string]any{
		"dataset_id":   datasetID,
		"line_count":   prev.LineCount,
		"sample_count": prev.SampleCount,
		"truncated":    prev.Truncated,
		"columns":      cols,
		"sample_lines": prev.Lines,
	}, nil
}

// GetVerificationCertificate fetches the integrity certificate (user token per Science policy).
func (c *Client) GetVerificationCertificate(datasetID string) (map[string]any, error) {
	datasetID = strings.TrimSpace(datasetID)
	if datasetID == "" {
		return nil, fmt.Errorf("dataset_id is required")
	}
	var cert map[string]any
	if err := c.getJSON("/api/v1/datasets/"+url.PathEscape(datasetID)+"/certificate", true, &cert); err != nil {
		return nil, err
	}
	return cert, nil
}

func (c *Client) getJSON(path string, needAuth bool, dest any) error {
	req, err := http.NewRequest(http.MethodGet, c.BaseURL+path, nil)
	if err != nil {
		return err
	}
	req.Header.Set("Accept", "application/json")
	if needAuth {
		if c.Token == "" {
			return fmt.Errorf("oasis: user OAuth token required — set OASIS_API_TOKEN or sign in to 绿洲")
		}
		req.Header.Set("Authorization", "Bearer "+c.Token)
	}
	resp, err := c.HTTPClient.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	body, err := io.ReadAll(io.LimitReader(resp.Body, 4<<20))
	if err != nil {
		return err
	}
	if resp.StatusCode == http.StatusUnauthorized || resp.StatusCode == http.StatusForbidden {
		return fmt.Errorf("oasis: auth failed (HTTP %d) — refresh 绿洲 login token", resp.StatusCode)
	}
	if resp.StatusCode >= 400 {
		return fmt.Errorf("oasis http %d: %s", resp.StatusCode, strings.TrimSpace(string(body)))
	}
	var env struct {
		Code    int             `json:"code"`
		Message string          `json:"message"`
		Data    json.RawMessage `json:"data"`
	}
	if err := json.Unmarshal(body, &env); err != nil {
		return fmt.Errorf("parse envelope: %w", err)
	}
	if env.Code != 0 {
		return fmt.Errorf("oasis api code %d: %s", env.Code, env.Message)
	}
	if dest == nil {
		return nil
	}
	if len(env.Data) == 0 || string(env.Data) == "null" {
		return nil
	}
	if err := json.Unmarshal(env.Data, dest); err != nil {
		return fmt.Errorf("parse data: %w", err)
	}
	return nil
}

func inferColumns(lines []string) []string {
	if len(lines) == 0 {
		return nil
	}
	first := lines[0]
	if strings.Contains(first, "\t") {
		return strings.Split(first, "\t")
	}
	if strings.Contains(first, ",") {
		return strings.Split(first, ",")
	}
	return strings.Fields(first)
}
