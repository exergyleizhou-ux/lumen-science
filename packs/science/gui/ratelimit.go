package gui

import (
	"net/http"
	"sync"
	"time"
)

// tokenBucket is a lightweight in-process rate limiter (no external deps).
type tokenBucket struct {
	mu       sync.Mutex
	tokens   float64
	last     time.Time
	rate     float64 // tokens per second
	capacity float64
}

func newTokenBucket(ratePerSec float64, burst int) *tokenBucket {
	return &tokenBucket{
		tokens:   float64(burst),
		last:     time.Now(),
		rate:     ratePerSec,
		capacity: float64(burst),
	}
}

func (b *tokenBucket) allow() bool {
	b.mu.Lock()
	defer b.mu.Unlock()
	now := time.Now()
	elapsed := now.Sub(b.last).Seconds()
	b.last = now
	b.tokens += elapsed * b.rate
	if b.tokens > b.capacity {
		b.tokens = b.capacity
	}
	if b.tokens < 1 {
		return false
	}
	b.tokens--
	return true
}

var mutateLimiter = newTokenBucket(8, 16) // ~8 mutating req/s, burst 16

func rateLimitMutations(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method == http.MethodPost || r.Method == http.MethodPut {
			if !mutateLimiter.allow() {
				w.Header().Set("Retry-After", "1")
				writeJSON(w, http.StatusTooManyRequests, map[string]any{
					"error": "rate limit exceeded, retry shortly",
				})
				return
			}
		}
		next.ServeHTTP(w, r)
	})
}
