package proxy

import "testing"

// Simulates a fixed multi-turn DeepSeek prefix-cache workload (session + last request).
func TestCacheStats_BenchmarkWorkloadHitRate(t *testing.T) {
	var s CacheStats
	// Turn 1: cold prefix — mostly miss
	s.Record(8000, 0, 8000)
	// Turns 2-8: warm prefix — high cache_read ratio (anecdotal 90%+ target)
	for i := 0; i < 7; i++ {
		s.Record(200, 7800, 0)
	}
	snap := s.Snapshot()
	pct := snap["session_hit_rate_pct"]
	if pct < 85 {
		t.Fatalf("session_hit_rate_pct=%d want >=85 on benchmark workload", pct)
	}
	last := snap["last_hit_rate_pct"]
	if last < 90 {
		t.Fatalf("last_hit_rate_pct=%d want >=90 on warm turn", last)
	}
	if snap["cache_hit_tokens"] < 35000 {
		t.Fatalf("cache_hit_tokens=%d too low", snap["cache_hit_tokens"])
	}
}

func TestCacheStats_RecordFromJSONUsage_BenchmarkShape(t *testing.T) {
	var s CacheStats
	usage := map[string]any{
		"input_tokens":                120,
		"cache_read_input_tokens":     6400,
		"cache_creation_input_tokens": 0,
	}
	s.RecordFromJSONUsage(usage)
	if s.LastHitPct.Load() < 95 {
		t.Fatalf("last hit %d", s.LastHitPct.Load())
	}
}
