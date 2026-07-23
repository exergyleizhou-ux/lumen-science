package worker

import (
	"context"
	"testing"
	"time"
)

func TestDoWorkCancelled(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	errCh := make(chan error, 1)
	go func() { errCh <- DoWork(ctx) }()

	select {
	case err := <-errCh:
		if err != context.Canceled {
			t.Fatalf("expected context.Canceled, got %v", err)
		}
	case <-time.After(100 * time.Millisecond):
		t.Fatal("timeout: DoWork did not respect cancellation")
	}
}
