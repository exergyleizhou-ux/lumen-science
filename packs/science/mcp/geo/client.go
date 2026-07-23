// Package geo implements NCBI GEO DataSets clients for Lumen Science MCP.
package geo

import (
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"strconv"
	"strings"
	"time"
)

const (
	eutilsBase   = "https://eutils.ncbi.nlm.nih.gov/entrez/eutils"
	apiVersion   = "geo-gds-v1"
	defaultAgent = "lumen-science/1.0 (mailto:support@oasisdata2026.xyz)"
)

// Client queries GEO DataSets (db=gds) via NCBI E-utilities.
type Client struct {
	BaseURL    string
	HTTPClient *http.Client
	UserAgent  string
}

// DatasetHit is a compact GEO DataSet row.
type DatasetHit struct {
	GDSID     string `json:"gds_id"`
	Accession string `json:"accession,omitempty"`
	Title     string `json:"title"`
	Organism  string `json:"organism,omitempty"`
	Samples   int    `json:"n_samples,omitempty"`
	Platform  string `json:"platform,omitempty"`
}

// NewClient returns a GEO E-utilities client.
func NewClient() *Client {
	return &Client{
		BaseURL:    eutilsBase,
		HTTPClient: &http.Client{Timeout: 30 * time.Second},
		UserAgent:  defaultAgent,
	}
}

// SearchDatasets finds GEO DataSets by keyword (anonymous).
func (c *Client) SearchDatasets(query string, limit int) ([]DatasetHit, error) {
	query = strings.TrimSpace(query)
	if query == "" {
		return nil, fmt.Errorf("query is required")
	}
	if limit <= 0 {
		limit = 10
	}
	if limit > 25 {
		limit = 25
	}
	ids, err := c.esearch(query, limit)
	if err != nil {
		return nil, err
	}
	if len(ids) == 0 {
		return []DatasetHit{}, nil
	}
	return c.esummary(ids)
}

// GetDataset fetches one GEO DataSet by numeric GDS id or accession.
func (c *Client) GetDataset(gdsID string) (DatasetHit, error) {
	gdsID = strings.TrimSpace(gdsID)
	if gdsID == "" {
		return DatasetHit{}, fmt.Errorf("gds_id is required")
	}
	id := strings.TrimPrefix(strings.ToUpper(gdsID), "GDS")
	u := fmt.Sprintf("%s/esummary.fcgi?db=gds&id=%s&retmode=json",
		c.BaseURL, url.QueryEscape(id))
	body, err := c.get(u)
	if err != nil {
		return DatasetHit{}, err
	}
	hits, err := parseESummary(body, []string{id})
	if err != nil {
		return DatasetHit{}, err
	}
	if len(hits) == 0 {
		return DatasetHit{}, fmt.Errorf("gds %s not found", gdsID)
	}
	return hits[0], nil
}

func (c *Client) esearch(query string, retmax int) ([]string, error) {
	u := fmt.Sprintf("%s/esearch.fcgi?db=gds&term=%s&retmax=%d&retmode=json",
		c.BaseURL, url.QueryEscape(query), retmax)
	body, err := c.get(u)
	if err != nil {
		return nil, err
	}
	var out struct {
		ESearchResult struct {
			IDList []string `json:"idlist"`
		} `json:"esearchresult"`
	}
	if err := json.Unmarshal(body, &out); err != nil {
		return nil, fmt.Errorf("parse esearch: %w", err)
	}
	return out.ESearchResult.IDList, nil
}

func (c *Client) esummary(ids []string) ([]DatasetHit, error) {
	u := fmt.Sprintf("%s/esummary.fcgi?db=gds&id=%s&retmode=json",
		c.BaseURL, url.QueryEscape(strings.Join(ids, ",")))
	body, err := c.get(u)
	if err != nil {
		return nil, err
	}
	return parseESummary(body, ids)
}

func parseESummary(body []byte, ids []string) ([]DatasetHit, error) {
	var out struct {
		Result map[string]json.RawMessage `json:"result"`
	}
	if err := json.Unmarshal(body, &out); err != nil {
		return nil, fmt.Errorf("parse esummary: %w", err)
	}
	hits := make([]DatasetHit, 0, len(ids))
	for _, id := range ids {
		raw, ok := out.Result[id]
		if !ok || string(raw) == "null" {
			continue
		}
		var row map[string]any
		if err := json.Unmarshal(raw, &row); err != nil {
			continue
		}
		hits = append(hits, DatasetHit{
			GDSID:     id,
			Accession: asString(row["accession"]),
			Title:     asString(row["title"]),
			Organism:  asString(row["taxon"]),
			Samples:   asInt(row["n_samples"]),
			Platform:  asString(row["platform"]),
		})
	}
	return hits, nil
}

func asString(v any) string {
	if v == nil {
		return ""
	}
	switch x := v.(type) {
	case string:
		return x
	default:
		return fmt.Sprint(x)
	}
}

func asInt(v any) int {
	if v == nil {
		return 0
	}
	switch x := v.(type) {
	case float64:
		return int(x)
	case string:
		n, _ := strconv.Atoi(strings.TrimSpace(x))
		return n
	default:
		return 0
	}
}

func (c *Client) get(rawURL string) ([]byte, error) {
	req, err := http.NewRequest(http.MethodGet, rawURL, nil)
	if err != nil {
		return nil, err
	}
	req.Header.Set("User-Agent", c.UserAgent)
	resp, err := c.HTTPClient.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	body, err := io.ReadAll(io.LimitReader(resp.Body, 4<<20))
	if err != nil {
		return nil, err
	}
	if resp.StatusCode >= 400 {
		return nil, fmt.Errorf("geo http %d", resp.StatusCode)
	}
	return body, nil
}
