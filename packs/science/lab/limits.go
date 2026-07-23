package lab

import "time"

// Production limits for national-lab-grade stability.
// Tunable via lab.json later; constants keep offline tests deterministic.
const (
	// MaxConcurrentTurns caps simultaneous agent turns across all projects.
	MaxConcurrentTurns = 4
	// MaxControllers is the in-memory per-project controller pool size.
	MaxControllers = 8
	// ApprovalTimeout is how long the hub waits for browser allow/deny.
	ApprovalTimeout = 10 * time.Minute
	// ChatBodyMaxBytes allows long scientific prompts without opening all POSTs.
	ChatBodyMaxBytes = 8 << 20 // 8 MiB
	// DefaultTurnTimeout bounds one chat turn.
	DefaultTurnTimeout = 5 * time.Minute
	// ConfigureTimeout bounds controller setup.
	ConfigureTimeout = 30 * time.Second
)
