package proxy

import (
	"encoding/json"
)

var ephemeralCache = json.RawMessage(`{"type":"ephemeral"}`)

// boostSystemCache adds cache_control to the system prompt when absent so DeepSeek
// prefix-cache can anchor on stable system+tools blocks across turns.
func boostSystemCache(raw json.RawMessage) json.RawMessage {
	if len(raw) == 0 {
		return raw
	}
	var s string
	if json.Unmarshal(raw, &s) == nil {
		block := map[string]json.RawMessage{
			"type":          json.RawMessage(`"text"`),
			"text":          json.RawMessage(mustQuote(s)),
			"cache_control": ephemeralCache,
		}
		b, err := json.Marshal([]map[string]json.RawMessage{block})
		if err != nil {
			return raw
		}
		return b
	}
	var blocks []map[string]any
	if json.Unmarshal(raw, &blocks) != nil {
		return raw
	}
	if len(blocks) == 0 {
		return raw
	}
	last := blocks[len(blocks)-1]
	if _, has := last["cache_control"]; has {
		return raw
	}
	last["cache_control"] = map[string]string{"type": "ephemeral"}
	b, err := json.Marshal(blocks)
	if err != nil {
		return raw
	}
	return b
}

// boostToolsCache marks the last tool definition cacheable (Anthropic best practice).
func boostToolsCache(raw json.RawMessage) json.RawMessage {
	if len(raw) == 0 {
		return raw
	}
	var tools []map[string]any
	if json.Unmarshal(raw, &tools) != nil || len(tools) == 0 {
		return raw
	}
	last := tools[len(tools)-1]
	if _, has := last["cache_control"]; has {
		return raw
	}
	last["cache_control"] = map[string]string{"type": "ephemeral"}
	b, err := json.Marshal(tools)
	if err != nil {
		return raw
	}
	return b
}
