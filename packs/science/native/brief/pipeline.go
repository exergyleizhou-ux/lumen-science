package brief

import (
	"context"
	"encoding/json"
	"fmt"
	"strings"
	"time"

	sciconfig "lumen/internal/science/config"
	"lumen/internal/science/native"
)

// Request parameters for brief generation.
type Request struct {
	Topic        string `json:"topic"`
	DatasetQuery string `json:"dataset_query,omitempty"`
	MaxArticles  int    `json:"max_articles,omitempty"`
	MaxDatasets  int    `json:"max_datasets,omitempty"`
}

const pipelineTimeout = 60 * time.Second

// Generate runs PubMed + Oasis native MCP tools and returns a provenance-linked brief.
func Generate(ctx context.Context, sciDir string, req Request) (Result, error) {
	topic := strings.TrimSpace(req.Topic)
	if topic == "" {
		return Result{}, fmt.Errorf("topic is required")
	}
	if req.MaxArticles <= 0 {
		req.MaxArticles = 5
	}
	if req.MaxDatasets <= 0 {
		req.MaxDatasets = 5
	}
	datasetQ := strings.TrimSpace(req.DatasetQuery)
	if datasetQ == "" {
		datasetQ = topic
	}

	pctx, cancel := context.WithTimeout(ctx, pipelineTimeout)
	defer cancel()

	cfg, err := sciconfig.Load(sciDir)
	if err != nil {
		return Result{}, err
	}

	mgr, err := native.NewManagerFromDir(sciDir)
	if err != nil {
		return Result{}, err
	}
	defer mgr.Close()
	if _, err := mgr.ConnectAll(); err != nil {
		return Result{}, fmt.Errorf("connect fleet: %w", err)
	}

	articles, err := fetchArticles(pctx, mgr, topic, req.MaxArticles)
	if err != nil {
		return Result{}, fmt.Errorf("pubmed: %w", err)
	}
	compounds, _ := fetchCompounds(pctx, mgr, topic, 3)
	geoRows, _ := fetchGEO(pctx, mgr, topic, 3)
	datasets, err := fetchDatasets(pctx, mgr, datasetQ, req.MaxDatasets)
	if err != nil {
		return Result{}, fmt.Errorf("oasis: %w", err)
	}
	if len(datasets) == 0 {
		datasets, err = fetchVerifiedList(pctx, mgr, req.MaxDatasets)
		if err != nil {
			return Result{}, fmt.Errorf("oasis verified list: %w", err)
		}
	}

	return Build(Input{
		Topic:       topic,
		Gathered:    time.Now().UTC(),
		Articles:    articles,
		Compounds:   compounds,
		GEODatasets: geoRows,
		Datasets:    datasets,
		BaseURL:     cfg.OasisBaseURL(),
	}), nil
}

func fetchGEO(ctx context.Context, mgr *native.Manager, query string, max int) ([]GEORow, error) {
	text, err := mgr.CallToolCtx(ctx, "geo", "search_datasets", map[string]any{
		"query": query, "limit": max,
	})
	if err != nil {
		return nil, err
	}
	var payload struct {
		Results []GEORow `json:"results"`
	}
	if err := json.Unmarshal([]byte(text), &payload); err != nil {
		return nil, err
	}
	return payload.Results, nil
}

func fetchCompounds(ctx context.Context, mgr *native.Manager, query string, max int) ([]CompoundRow, error) {
	text, err := mgr.CallToolCtx(ctx, "chembl", "search_compounds", map[string]any{
		"query": query, "limit": max,
	})
	if err != nil {
		return nil, err
	}
	var payload struct {
		Results []CompoundRow `json:"results"`
	}
	if err := json.Unmarshal([]byte(text), &payload); err != nil {
		return nil, err
	}
	return payload.Results, nil
}

func fetchArticles(ctx context.Context, mgr *native.Manager, query string, max int) ([]ArticleRow, error) {
	text, err := mgr.CallToolCtx(ctx, "pubmed", "search_articles", map[string]any{
		"query": query, "max_results": max,
	})
	if err != nil {
		return nil, err
	}
	var payload struct {
		Results []ArticleRow `json:"results"`
	}
	if err := json.Unmarshal([]byte(text), &payload); err != nil {
		return nil, err
	}
	return payload.Results, nil
}

func fetchDatasets(ctx context.Context, mgr *native.Manager, query string, max int) ([]DatasetRow, error) {
	text, err := mgr.CallToolCtx(ctx, "oasis", "search_datasets", map[string]any{
		"q": query, "limit": max + 10,
	})
	if err != nil {
		return nil, err
	}
	var payload struct {
		Results struct {
			Datasets []struct {
				ID               string `json:"id"`
				Title            string `json:"title"`
				Domain           string `json:"domain,omitempty"`
				AuthenticityBand string `json:"authenticity_band,omitempty"`
				QualityVerified  *bool  `json:"quality_verified,omitempty"`
			} `json:"datasets"`
		} `json:"results"`
	}
	if err := json.Unmarshal([]byte(text), &payload); err != nil {
		return nil, err
	}
	out := make([]DatasetRow, 0, max)
	for _, d := range payload.Results.Datasets {
		verified := d.QualityVerified != nil && *d.QualityVerified
		if !verified {
			continue
		}
		out = append(out, DatasetRow{
			ID: d.ID, Title: d.Title, Domain: d.Domain,
			AuthenticityBand: d.AuthenticityBand, QualityVerified: true,
		})
		if len(out) >= max {
			break
		}
	}
	return out, nil
}

func fetchVerifiedList(ctx context.Context, mgr *native.Manager, max int) ([]DatasetRow, error) {
	text, err := mgr.CallToolCtx(ctx, "oasis", "list_verified_datasets", map[string]any{"limit": max})
	if err != nil {
		return nil, err
	}
	var payload struct {
		Results struct {
			Datasets []struct {
				ID               string `json:"id"`
				Title            string `json:"title"`
				Domain           string `json:"domain,omitempty"`
				AuthenticityBand string `json:"authenticity_band,omitempty"`
				QualityVerified  *bool  `json:"quality_verified,omitempty"`
			} `json:"datasets"`
		} `json:"results"`
	}
	if err := json.Unmarshal([]byte(text), &payload); err != nil {
		return nil, err
	}
	out := make([]DatasetRow, 0, max)
	for _, d := range payload.Results.Datasets {
		out = append(out, DatasetRow{
			ID: d.ID, Title: d.Title, Domain: d.Domain,
			AuthenticityBand: d.AuthenticityBand, QualityVerified: true,
		})
		if len(out) >= max {
			break
		}
	}
	return out, nil
}
