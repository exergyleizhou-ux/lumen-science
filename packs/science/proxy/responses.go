package proxy

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"strings"
)

// handleResponses translates Anthropic Messages ↔ OpenAI Responses (non-stream upstream,
// local Anthropic SSE replay when client requested stream). Aligned with CSSwitch responses_compat.
func (s *Server) handleResponses(w http.ResponseWriter, ctx context.Context, areq map[string]any) {
	modelID, _ := areq["model"].(string)
	stream, _ := areq["stream"].(bool)
	oreq := AnthropicToResponses(s.spec, areq)
	toolCount := 0
	if tools, ok := oreq["tools"].([]any); ok {
		toolCount = len(tools)
	}
	s.log(fmt.Sprintf("POST /v1/messages %s->%s stream=%v tools=%d (responses mode, %s)",
		modelID, oreq["model"], stream, toolCount, s.spec.Name))

	headers := map[string]string{
		"Authorization": "Bearer " + s.cfg.APIKey,
		"Content-Type":  "application/json",
	}
	// Responses path always non-stream upstream; replay SSE locally if needed.
	payload, _ := json.Marshal(oreq)
	raw, _, _, err := s.upstream.PostJSON(ctx, s.spec.URL, payload, headers)
	if err != nil {
		s.writeUpstreamError(w, err, false)
		return
	}
	var oresp map[string]any
	if err := json.Unmarshal(raw, &oresp); err != nil {
		s.sendJSON(w, http.StatusBadGateway, errorBody("api_error", err.Error()))
		return
	}
	aresp := ResponsesToAnthropic(oresp, modelID)
	if stream {
		s.replayAsSSE(w, aresp)
	} else {
		s.sendJSON(w, http.StatusOK, aresp)
	}
	s.log(fmt.Sprintf("  <- %s responses OK stop=%v", s.spec.Name, aresp["stop_reason"]))
}

// AnthropicToResponses converts Anthropic Messages request to OpenAI Responses body.
func AnthropicToResponses(spec ProviderSpec, req map[string]any) map[string]any {
	src, _ := req["model"].(string)
	target := ResolveModel(spec, src)

	// Build input messages (Responses uses "input" array).
	var input []map[string]any
	if sys := req["system"]; sys != nil {
		switch v := sys.(type) {
		case string:
			if v != "" {
				input = append(input, map[string]any{"role": "system", "content": v})
			}
		case []any:
			var parts []string
			for _, blk := range v {
				if m, ok := blk.(map[string]any); ok && m["type"] == "text" {
					if t, _ := m["text"].(string); t != "" {
						parts = append(parts, t)
					}
				}
			}
			if len(parts) > 0 {
				input = append(input, map[string]any{"role": "system", "content": strings.Join(parts, "\n")})
			}
		}
	}
	rawMsgs, _ := req["messages"].([]any)
	for _, item := range rawMsgs {
		m, ok := item.(map[string]any)
		if !ok {
			continue
		}
		role, _ := m["role"].(string)
		content := m["content"]
		if s, ok := content.(string); ok {
			input = append(input, map[string]any{"role": role, "content": s})
			continue
		}
		// Flatten blocks to text for Responses minimal path.
		var textParts []string
		blocks, _ := content.([]any)
		for _, b := range blocks {
			blk, ok := b.(map[string]any)
			if !ok {
				continue
			}
			if blk["type"] == "text" {
				if t, _ := blk["text"].(string); t != "" {
					textParts = append(textParts, t)
				}
			}
		}
		input = append(input, map[string]any{"role": role, "content": strings.Join(textParts, "")})
	}

	out := map[string]any{
		"model": target,
		"input": input,
	}

	maxTok := 0
	switch v := req["max_tokens"].(type) {
	case float64:
		maxTok = int(v)
	case int:
		maxTok = v
	}
	maxTok = ClampMaxTokens(spec, maxTok, target)
	isDash := strings.Contains(spec.URL, "dashscope.aliyuncs.com")
	tools, _ := req["tools"].([]any)
	hasTools := len(tools) > 0
	if hasTools && isDash && maxTok > 8192 {
		maxTok = 8192
	}
	if maxTok > 0 {
		out["max_output_tokens"] = maxTok
	}

	if hasTools {
		mapped := mapResponsesTools(tools, isDash)
		if len(mapped) > 0 {
			out["tools"] = mapped
		}
		// Force tool_choice → auto for Responses compatibility.
		out["tool_choice"] = "auto"
	}
	return out
}

func mapResponsesTools(tools []any, isDashscope bool) []map[string]any {
	var out []map[string]any
	for _, t := range tools {
		tm, ok := t.(map[string]any)
		if !ok {
			continue
		}
		name, _ := tm["name"].(string)
		if name == "" {
			continue
		}
		if isDashscope && name == "web_search" {
			continue
		}
		schema, _ := tm["input_schema"].(map[string]any)
		params := normalizeToolParameters(schema)
		desc, _ := tm["description"].(string)
		out = append(out, map[string]any{
			"type":        "function",
			"name":        name,
			"description": desc,
			"parameters":  params,
		})
	}
	return out
}

func normalizeToolParameters(schema map[string]any) map[string]any {
	if schema == nil {
		return map[string]any{"type": "object", "properties": map[string]any{}}
	}
	out := make(map[string]any, len(schema)+1)
	for k, v := range schema {
		out[k] = v
	}
	if _, ok := out["properties"]; ok {
		if _, hasType := out["type"]; !hasType {
			out["type"] = "object"
		}
	}
	if typ, _ := out["type"].(string); typ != "object" {
		return map[string]any{"type": "object", "properties": map[string]any{}}
	}
	if _, ok := out["properties"].(map[string]any); !ok {
		out["properties"] = map[string]any{}
	}
	return out
}

// ResponsesToAnthropic converts an OpenAI Responses JSON body to Anthropic message shape.
func ResponsesToAnthropic(resp map[string]any, modelID string) map[string]any {
	var text strings.Builder
	// output: array of items with content parts
	if output, ok := resp["output"].([]any); ok {
		for _, item := range output {
			im, ok := item.(map[string]any)
			if !ok {
				continue
			}
			// message type with content array
			if content, ok := im["content"].([]any); ok {
				for _, c := range content {
					cm, ok := c.(map[string]any)
					if !ok {
						continue
					}
					if t, _ := cm["text"].(string); t != "" {
						text.WriteString(t)
					}
					if t, _ := cm["type"].(string); t == "output_text" {
						if v, _ := cm["text"].(string); v != "" {
							// already written if field is text
							_ = v
						}
					}
				}
			}
			if t, _ := im["text"].(string); t != "" {
				text.WriteString(t)
			}
		}
	}
	// fallback: output_text top-level
	if text.Len() == 0 {
		if t, _ := resp["output_text"].(string); t != "" {
			text.WriteString(t)
		}
	}

	id, _ := resp["id"].(string)
	if id == "" {
		id = "msg_responses"
	}
	usageIn, usageOut := 0, 0
	if u, ok := resp["usage"].(map[string]any); ok {
		if v, ok := u["input_tokens"].(float64); ok {
			usageIn = int(v)
		}
		if v, ok := u["output_tokens"].(float64); ok {
			usageOut = int(v)
		}
	}

	return map[string]any{
		"id":            id,
		"type":          "message",
		"role":          "assistant",
		"model":         modelID,
		"content":       []map[string]any{{"type": "text", "text": text.String()}},
		"stop_reason":   "end_turn",
		"stop_sequence": nil,
		"usage": map[string]any{
			"input_tokens":  usageIn,
			"output_tokens": usageOut,
		},
	}
}

