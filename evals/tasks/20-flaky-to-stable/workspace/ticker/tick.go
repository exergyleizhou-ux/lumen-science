package ticker

import "time"

// Ticker delivers ticks on C until Stop is called.
// It must use the standard library time.Ticker (not sleep-loops) so tests are deterministic.
type Ticker struct {
	C    <-chan time.Time
	done chan struct{}
}

func NewTicker(d time.Duration) *Ticker {
	ch := make(chan time.Time)
	done := make(chan struct{})
	// BUG: never starts a producer — C never receives (flaky/broken design).
	// Correct fix: use time.NewTicker(d) and forward ticks until Stop.
	_ = d
	return &Ticker{C: ch, done: done}
}

func (t *Ticker) Stop() {
	select {
	case <-t.done:
	default:
		close(t.done)
	}
}
