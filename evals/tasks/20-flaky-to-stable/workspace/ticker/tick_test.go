package ticker

import (
	"testing"
	"time"
)

func TestTickerFires(t *testing.T) {
	tk := NewTicker(10 * time.Millisecond)
	defer tk.Stop()

	count := 0
	timeout := time.After(200 * time.Millisecond)
	for count < 3 {
		select {
		case <-tk.C:
			count++
		case <-timeout:
			t.Fatalf("only got %d ticks before timeout (Ticker must actually fire)", count)
		}
	}
	if count < 3 {
		t.Fatalf("expected at least 3 ticks, got %d", count)
	}
}
