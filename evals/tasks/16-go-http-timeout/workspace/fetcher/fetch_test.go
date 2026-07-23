package fetcher

import (
	"testing"
	"time"
)

func TestClientHasTimeout(t *testing.T) {
	if client.Timeout == 0 {
		t.Error("http.Client should have a timeout set")
	}
	// Just test that a reasonable timeout is configured
	if client.Timeout > 30*time.Second {
		t.Error("http.Client timeout is too large")
	}
}
