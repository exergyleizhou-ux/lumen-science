package proxy

import (
	"encoding/json"
	"strings"
	"sync/atomic"
)

// CacheStats tracks DeepSeek prefix-cache usage from upstream responses.
type CacheStats struct {
	Hits       atomic.Int64
	Misses     atomic.Int64
	Created    atomic.Int64
	Requests   atomic.Int64
	LastHitPct atomic.Int64 // 0-100, last request hit rate * 100
}

func (s *CacheStats) Record(input, cacheRead, cacheCreate int64) {
	s.Requests.Add(1)
	s.Hits.Add(cacheRead)
	s.Misses.Add(input)
	s.Created.Add(cacheCreate)
	total := input + cacheRead
	if total > 0 {
		s.LastHitPct.Store(cacheRead * 100 / total)
	}
}

func (s *CacheStats) Snapshot() map[string]int64 {
	h := s.Hits.Load()
	m := s.Misses.Load()
	total := h + m
	var pct int64
	if total > 0 {
		pct = h * 100 / total
	}
	return map[string]int64{
		"requests":             s.Requests.Load(),
		"cache_hit_tokens":     h,
		"cache_miss_tokens":    m,
		"cache_created_tokens": s.Created.Load(),
		"session_hit_rate_pct": pct,
		"last_hit_rate_pct":    s.LastHitPct.Load(),
	}
}

// RecordFromJSONUsage parses Anthropic-style usage object.
func (s *CacheStats) RecordFromJSONUsage(usage map[string]any) {
	if usage == nil {
		return
	}
	in := int64(num(usage["input_tokens"]))
	read := int64(num(usage["cache_read_input_tokens"]))
	create := int64(num(usage["cache_creation_input_tokens"]))
	if read > 0 || create > 0 || in > 0 {
		s.Record(in, read, create)
	}
}

// ScanSSEChunk looks for message_start usage in streamed SSE data.
func (s *CacheStats) ScanSSEChunk(chunk []byte) {
	text := string(chunk)
	for _, line := range strings.Split(text, "\n") {
		if !strings.HasPrefix(line, "data: ") {
			continue
		}
		payload := strings.TrimPrefix(line, "data: ")
		var evt map[string]any
		if json.Unmarshal([]byte(payload), &evt) != nil {
			continue
		}
		if evt["type"] != "message_start" {
			continue
		}
		msg, _ := evt["message"].(map[string]any)
		usage, _ := msg["usage"].(map[string]any)
		s.RecordFromJSONUsage(usage)
	}
}

func num(v any) float64 {
	switch x := v.(type) {
	case float64:
		return x
	case int:
		return float64(x)
	case int64:
		return float64(x)
	default:
		return 0
	}
}
