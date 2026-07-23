// Package chembl implements ChEMBL REST clients for Lumen Science MCP.
package chembl

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
	baseURL    = "https://www.ebi.ac.uk/chembl/api/data"
	apiVersion = "chembl-v1"
	userAgent  = "lumen-science/1.0 (mailto:support@oasisdata2026.xyz)"
)

// Client queries ChEMBL.
type Client struct {
	BaseURL    string
	HTTPClient *http.Client
}

// NewClient returns a ChEMBL HTTP client.
func NewClient() *Client {
	return &Client{
		BaseURL:    baseURL,
		HTTPClient: &http.Client{Timeout: 30 * time.Second},
	}
}

// CompoundHit is a compact molecule row.
type CompoundHit struct {
	ChemblID     string  `json:"chembl_id"`
	PrefName     string  `json:"pref_name,omitempty"`
	MaxPhase     float64 `json:"max_phase,omitempty"`
	MoleculeType string  `json:"molecule_type,omitempty"`
}

// SearchCompounds finds molecules by name/synonym.
func (c *Client) SearchCompounds(query string, limit int) ([]CompoundHit, error) {
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
	u := fmt.Sprintf("%s/molecule/search?q=%s&format=json&limit=%d",
		c.BaseURL, url.QueryEscape(query), limit)
	body, err := c.get(u)
	if err != nil {
		return nil, err
	}
	var out struct {
		Molecules []map[string]any `json:"molecules"`
	}
	if err := json.Unmarshal(body, &out); err != nil {
		return nil, err
	}
	hits := make([]CompoundHit, 0, len(out.Molecules))
	for _, m := range out.Molecules {
		hits = append(hits, CompoundHit{
			ChemblID:     asString(m["molecule_chembl_id"]),
			PrefName:     asString(m["pref_name"]),
			MaxPhase:     asFloat(m["max_phase"]),
			MoleculeType: asString(m["molecule_type"]),
		})
	}
	return hits, nil
}

// GetMolecule fetches one molecule by ChEMBL id.
func (c *Client) GetMolecule(chemblID string) (map[string]any, error) {
	chemblID = strings.TrimSpace(chemblID)
	if chemblID == "" {
		return nil, fmt.Errorf("chembl_id is required")
	}
	u := fmt.Sprintf("%s/molecule/%s.json", c.BaseURL, url.PathEscape(chemblID))
	body, err := c.get(u)
	if err != nil {
		return nil, err
	}
	var row map[string]any
	if err := json.Unmarshal(body, &row); err != nil {
		return nil, err
	}
	return row, nil
}

// TargetActivities returns bioactivity rows for a target ChEMBL id.
func (c *Client) TargetActivities(targetID string, limit int) ([]map[string]any, error) {
	targetID = strings.TrimSpace(targetID)
	if targetID == "" {
		return nil, fmt.Errorf("target_chembl_id is required")
	}
	if limit <= 0 {
		limit = 10
	}
	u := fmt.Sprintf("%s/activity.json?target_chembl_id=%s&limit=%d",
		c.BaseURL, url.QueryEscape(targetID), limit)
	body, err := c.get(u)
	if err != nil {
		return nil, err
	}
	var out struct {
		Activities []map[string]any `json:"activities"`
	}
	if err := json.Unmarshal(body, &out); err != nil {
		return nil, err
	}
	return out.Activities, nil
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

func asFloat(v any) float64 {
	if v == nil {
		return 0
	}
	switch x := v.(type) {
	case float64:
		return x
	case string:
		f, _ := strconv.ParseFloat(strings.TrimSpace(x), 64)
		return f
	default:
		return 0
	}
}

func (c *Client) get(rawURL string) ([]byte, error) {
	req, err := http.NewRequest(http.MethodGet, rawURL, nil)
	if err != nil {
		return nil, err
	}
	req.Header.Set("User-Agent", userAgent)
	req.Header.Set("Accept", "application/json")
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
		return nil, fmt.Errorf("chembl http %d", resp.StatusCode)
	}
	return body, nil
}
