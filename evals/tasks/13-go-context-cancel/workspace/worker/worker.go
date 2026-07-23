package worker

import "context"

func DoWork(ctx context.Context) error {
	// BUG: ignores context cancellation
	select {}
}
