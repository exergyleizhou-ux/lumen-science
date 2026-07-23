package proxy

import (
	"encoding/json"
	"strings"
)

// AnthropicToOpenAI converts an Anthropic Messages request to OpenAI chat format.
func AnthropicToOpenAI(spec ProviderSpec, req map[string]any) map[string]any {
	msgs := make([]map[string]any, 0, 8)

	sys := req["system"]
	switch v := sys.(type) {
	case string:
		if v != "" {
			msgs = append(msgs, map[string]any{"role": "system", "content": v})
		}
	case []any:
		var parts []string
		for _, blk := range v {
			m, ok := blk.(map[string]any)
			if !ok {
				continue
			}
			if m["type"] == "text" {
				if t, _ := m["text"].(string); t != "" {
					parts = append(parts, t)
				}
			}
		}
		if len(parts) > 0 {
			msgs = append(msgs, map[string]any{"role": "system", "content": strings.Join(parts, "\n")})
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
			msgs = append(msgs, map[string]any{"role": role, "content": s})
			continue
		}

		blocks, _ := content.([]any)
		var textParts []string
		var toolCalls []map[string]any
		var toolResults []map[string]any

		for _, b := range blocks {
			blk, ok := b.(map[string]any)
			if !ok {
				continue
			}
			switch blk["type"] {
			case "text":
				if t, _ := blk["text"].(string); t != "" {
					textParts = append(textParts, t)
				}
			case "tool_use":
				args, _ := json.Marshal(blk["input"])
				toolCalls = append(toolCalls, map[string]any{
					"id":   blk["id"],
					"type": "function",
					"function": map[string]any{
						"name":      blk["name"],
						"arguments": string(args),
					},
				})
			case "tool_result":
				c := blk["content"]
				var contentStr string
				switch cv := c.(type) {
				case string:
					contentStr = cv
				case []any:
					var tp []string
					for _, x := range cv {
						if xm, ok := x.(map[string]any); ok && xm["type"] == "text" {
							if t, _ := xm["text"].(string); t != "" {
								tp = append(tp, t)
							}
						}
					}
					contentStr = strings.Join(tp, "")
				default:
					b, _ := json.Marshal(c)
					contentStr = string(b)
				}
				toolResults = append(toolResults, map[string]any{
					"role":         "tool",
					"tool_call_id": blk["tool_use_id"],
					"content":      contentStr,
				})
			}
		}

		if role == "assistant" && len(toolCalls) > 0 {
			entry := map[string]any{"role": "assistant", "tool_calls": toolCalls}
			if joined := strings.Join(textParts, ""); joined != "" {
				entry["content"] = joined
			} else {
				entry["content"] = nil
			}
			msgs = append(msgs, entry)
		} else if len(toolResults) > 0 {
			msgs = append(msgs, toolResults...)
			if len(textParts) > 0 {
				msgs = append(msgs, map[string]any{"role": role, "content": strings.Join(textParts, "")})
			}
		} else {
			msgs = append(msgs, map[string]any{"role": role, "content": strings.Join(textParts, "")})
		}
	}

	srcModel, _ := req["model"].(string)
	target := ResolveModel(spec, srcModel)
	out := map[string]any{
		"model":    target,
		"messages": msgs,
		"stream":   false,
	}
	if mt, ok := req["max_tokens"].(float64); ok && mt > 0 {
		out["max_tokens"] = ClampMaxTokens(spec, int(mt), target)
	}
	if t, ok := req["temperature"].(float64); ok {
		out["temperature"] = t
	}
	if tools, ok := req["tools"].([]any); ok && len(tools) > 0 {
		oTools := make([]map[string]any, 0, len(tools))
		for _, item := range tools {
			t, ok := item.(map[string]any)
			if !ok || t["name"] == nil {
				continue
			}
			oTools = append(oTools, map[string]any{
				"type": "function",
				"function": map[string]any{
					"name":        t["name"],
					"description": t["description"],
					"parameters":  t["input_schema"],
				},
			})
		}
		out["tools"] = oTools
	}
	if tc := mapToolChoice(req["tool_choice"], req["tools"]); tc != nil {
		out["tool_choice"] = tc
	}
	if stops, ok := req["stop_sequences"].([]any); ok && len(stops) > 0 {
		out["stop"] = stops
	}
	if tp, ok := req["top_p"].(float64); ok {
		out["top_p"] = tp
	}
	return out
}

func mapToolChoice(tc any, tools any) any {
	m, ok := tc.(map[string]any)
	if !ok {
		return nil
	}
	t, _ := m["type"].(string)
	switch t {
	case "auto":
		return "auto"
	case "none":
		return "none"
	case "tool":
		if name, _ := m["name"].(string); name != "" {
			return map[string]any{"type": "function", "function": map[string]any{"name": name}}
		}
	case "any":
		var names []string
		if arr, ok := tools.([]any); ok {
			for _, item := range arr {
				if tm, ok := item.(map[string]any); ok {
					if n, _ := tm["name"].(string); n != "" {
						names = append(names, n)
					}
				}
			}
		}
		if len(names) == 1 {
			return map[string]any{"type": "function", "function": map[string]any{"name": names[0]}}
		}
		return "required"
	}
	return nil
}

// OpenAIToAnthropic converts an OpenAI chat completion response to Anthropic format.
func OpenAIToAnthropic(resp map[string]any, modelID string) map[string]any {
	choices, _ := resp["choices"].([]any)
	var choice map[string]any
	if len(choices) > 0 {
		choice, _ = choices[0].(map[string]any)
	}
	if choice == nil {
		choice = map[string]any{}
	}
	msg, _ := choice["message"].(map[string]any)
	if msg == nil {
		msg = map[string]any{}
	}

	blocks := make([]map[string]any, 0, 4)
	if content, _ := msg["content"].(string); content != "" {
		blocks = append(blocks, map[string]any{"type": "text", "text": content})
	}
	if tcs, ok := msg["tool_calls"].([]any); ok {
		for _, item := range tcs {
			tc, ok := item.(map[string]any)
			if !ok {
				continue
			}
			fn, _ := tc["function"].(map[string]any)
			if fn == nil {
				continue
			}
			argsStr, _ := fn["arguments"].(string)
			var args map[string]any
			_ = json.Unmarshal([]byte(argsStr), &args)
			if args == nil {
				args = map[string]any{}
			}
			blocks = append(blocks, map[string]any{
				"type":  "tool_use",
				"id":    tc["id"],
				"name":  fn["name"],
				"input": args,
			})
		}
	}
	if len(blocks) == 0 {
		blocks = append(blocks, map[string]any{"type": "text", "text": ""})
	}

	fr, _ := choice["finish_reason"].(string)
	stop := map[string]string{
		"stop":       "end_turn",
		"length":     "max_tokens",
		"tool_calls": "tool_use",
	}[fr]
	if stop == "" {
		stop = "end_turn"
	}

	usage, _ := resp["usage"].(map[string]any)
	inTok, _ := usage["prompt_tokens"].(float64)
	outTok, _ := usage["completion_tokens"].(float64)

	id, _ := resp["id"].(string)
	if id == "" {
		id = "msg_proxy"
	}

	return map[string]any{
		"id":            id,
		"type":          "message",
		"role":          "assistant",
		"model":         modelID,
		"content":       blocks,
		"stop_reason":   stop,
		"stop_sequence": nil,
		"usage": map[string]any{
			"input_tokens":  int(inTok),
			"output_tokens": int(outTok),
		},
	}
}

// NormalizeAnthropicBody adjusts a Science request for native Anthropic upstreams.
func NormalizeAnthropicBody(spec ProviderSpec, req map[string]any) map[string]any {
	body := make(map[string]any, len(req)+1)
	for k, v := range req {
		body[k] = v
	}
	src, _ := body["model"].(string)
	target := ResolveModel(spec, src)
	body["model"] = target
	if mt, ok := body["max_tokens"].(float64); ok && mt > 0 {
		body["max_tokens"] = ClampMaxTokens(spec, int(mt), target)
	}
	tc, _ := body["tool_choice"].(map[string]any)
	forcing := false
	if tc != nil {
		t, _ := tc["type"].(string)
		forcing = t == "any" || t == "tool"
	}
	// Per-provider thinking normalization (from CSSwitch real-machine testing):
	// - deepseek: forced tools = unconditionally disable thinking (flash model conflict)
	// - minimax: accept adaptive as-is (MiniMax handles thinking+tool_use natively)
	// - moonshot/kimi: forcing = enabled with budget_tokens (Kimi requires enabled)
	// - zhipu/qwen: forcing = disabled (safe default)
	if forcing {
		switch spec.Name {
		case "moonshot":
			body["thinking"] = map[string]any{
				"type":          "enabled",
				"budget_tokens": 1024,
			}
		default:
			body["thinking"] = map[string]any{"type": "disabled"}
		}
	} else if th, ok := body["thinking"].(map[string]any); ok {
		if th["type"] == "auto" {
			copy := make(map[string]any, len(th))
			for k, v := range th {
				copy[k] = v
			}
			copy["type"] = "adaptive"
			body["thinking"] = copy
		}
	}
	return body
}
