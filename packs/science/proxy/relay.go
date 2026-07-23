package proxy

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"strings"
	"time"
)

// RelaySpec builds an Anthropic-compatible relay provider from base URL + key + models.
// When modelOverride is set, Science sees a single claude-opus-4-8 shell whose
// display_name is the real model (CSSwitch force-shell), and outbound model is forced.
func RelaySpec(baseURL, apiKey, modelOverride string, models []ModelEntry) ProviderSpec {
	base := strings.TrimRight(strings.TrimSpace(baseURL), "/")
	url := base + "/v1/messages"
	if strings.HasSuffix(base, "/v1/messages") {
		url = base
	} else if !strings.HasSuffix(base, "/anthropic") && !strings.Contains(base, "/v1") {
		// Common relay layouts: /api, /api/anthropic, bare host
		if strings.HasSuffix(base, "/api") {
			url = base + "/v1/messages"
		} else {
			url = base + "/anthropic/v1/messages"
		}
	} else if strings.HasSuffix(base, "/anthropic") {
		url = base + "/v1/messages"
	}
	spec := ProviderSpec{
		Name:         "relay",
		Mode:         ModeAnthropic,
		URL:          url,
		KeyEnv:       "LUMEN_RELAY_API_KEY",
		Models:       models,
		ModelMap:     map[string]string{},
		DefaultCap:   8192,
		DefaultModel: "",
		DualAuth:     true,
	}
	if modelOverride != "" {
		// Force-shell for Science model selector (issue CSSwitch #11).
		display := modelOverride
		spec.ForceModelOverride = true
		spec.ForceModel = modelOverride
		spec.DefaultModel = modelOverride
		spec.Models = []ModelEntry{{ID: "claude-opus-4-8", DisplayName: display}}
		spec.ModelMap = map[string]string{
			"claude-opus-4-8":   modelOverride,
			"claude-sonnet-5":   modelOverride,
			"claude-sonnet-4-6": modelOverride,
			"claude-haiku-4-5":  modelOverride,
		}
		if strings.Contains(strings.ToLower(modelOverride), "kimi") {
			spec.ThinkingPolicy = ThinkingPolicyEnabled
		}
		return spec
	}
	if len(models) > 0 {
		spec.DefaultModel = models[0].ID
		for _, m := range models {
			spec.ModelMap[m.ID] = m.ID
		}
	}
	return spec
}

// FetchRelayModels lists models from an Anthropic-compatible relay /v1/models.
func FetchRelayModels(ctx context.Context, baseURL, apiKey string) ([]ModelEntry, error) {
	base := strings.TrimRight(strings.TrimSpace(baseURL), "/")
	candidates := []string{
		base + "/v1/models",
		base + "/anthropic/v1/models",
	}
	if strings.HasSuffix(base, "/api") {
		candidates = append([]string{base + "/v1/models"}, candidates...)
	}
	client := &http.Client{Timeout: 15 * time.Second}
	var lastErr error
	for _, u := range candidates {
		req, err := http.NewRequestWithContext(ctx, http.MethodGet, u, nil)
		if err != nil {
			lastErr = err
			continue
		}
		req.Header.Set("x-api-key", apiKey)
		req.Header.Set("Authorization", "Bearer "+apiKey)
		req.Header.Set("anthropic-version", "2023-06-01")
		req.Header.Set("User-Agent", "Lumen/1.3 (+https://github.com/exergyleizhou-ux/lumen)")
		resp, err := client.Do(req)
		if err != nil {
			lastErr = err
			continue
		}
		body, _ := io.ReadAll(io.LimitReader(resp.Body, 2<<20))
		resp.Body.Close()
		if resp.StatusCode != http.StatusOK {
			lastErr = fmt.Errorf("HTTP %d from %s", resp.StatusCode, u)
			continue
		}
		var parsed struct {
			Data []struct {
				ID          string `json:"id"`
				DisplayName string `json:"display_name"`
			} `json:"data"`
		}
		if json.Unmarshal(body, &parsed) != nil || len(parsed.Data) == 0 {
			lastErr = fmt.Errorf("no models in %s", u)
			continue
		}
		out := make([]ModelEntry, 0, len(parsed.Data))
		for _, m := range parsed.Data {
			if m.ID == "" {
				continue
			}
			dn := m.DisplayName
			if dn == "" {
				dn = m.ID
			}
			out = append(out, ModelEntry{ID: m.ID, DisplayName: dn})
		}
		if len(out) > 0 {
			return out, nil
		}
	}
	if lastErr == nil {
		lastErr = fmt.Errorf("no /v1/models endpoint responded")
	}
	return nil, lastErr
}

// ProbeUpstreamKey sends a minimal /v1/messages to the upstream (isolated probe).
func ProbeUpstreamKey(ctx context.Context, spec ProviderSpec, apiKey, model string) (int, string, error) {
	if apiKey == "" {
		return 0, "", fmt.Errorf("empty API key")
	}
	if model == "" {
		model = "claude-opus-4-8"
		if spec.DefaultModel != "" {
			model = spec.DefaultModel
		} else if len(spec.Models) > 0 {
			model = spec.Models[0].ID
		}
	}
	target := ResolveModel(spec, model)
	body := map[string]any{
		"model":      target,
		"max_tokens": 1,
		"messages":   []any{map[string]any{"role": "user", "content": "ping"}},
	}
	payload, _ := json.Marshal(body)
	headers := upstreamHeaders(spec, apiKey)
	up := &Upstream{}
	switch spec.Mode {
	case ModeAnthropic:
		_, _, code, err := up.PostJSON(ctx, spec.URL, payload, headers)
		if err != nil {
			if hs, ok := err.(*HTTPStatusError); ok {
				return hs.Code, string(hs.Body), nil
			}
			return 0, "", err
		}
		return code, "upstream accepted key", nil
	default:
		oreq := AnthropicToOpenAI(spec, map[string]any{
			"model": model, "max_tokens": 1,
			"messages": []any{map[string]any{"role": "user", "content": "ping"}},
		})
		op, _ := json.Marshal(oreq)
		h := map[string]string{
			"Authorization": "Bearer " + apiKey,
			"Content-Type":  "application/json",
		}
		_, _, code, err := up.PostJSON(ctx, spec.URL, op, h)
		if err != nil {
			if hs, ok := err.(*HTTPStatusError); ok {
				return hs.Code, string(hs.Body), nil
			}
			return 0, "", err
		}
		return code, "upstream accepted key", nil
	}
}

func upstreamHeaders(spec ProviderSpec, apiKey string) map[string]string {
	h := map[string]string{
		"x-api-key":         apiKey,
		"content-type":      "application/json",
		"anthropic-version": "2023-06-01",
	}
	if spec.DualAuth {
		h["Authorization"] = "Bearer " + apiKey
	}
	return h
}
