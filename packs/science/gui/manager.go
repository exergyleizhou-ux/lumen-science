package gui

import (
	"sync"

	"lumen/internal/config"
	sciruntime "lumen/internal/science/runtime"
)

// Shared manager survives across HTTP handlers in the GUI process so proxy
// lifecycle (start/stop/status) stays coherent under concurrent panel actions.
var (
	mgrMu        sync.Mutex
	sharedMgr    *sciruntime.Manager
	sharedSciDir string
)

func panelManager(sciDir string, lumenCfg *config.File) (*sciruntime.Manager, error) {
	mgrMu.Lock()
	defer mgrMu.Unlock()
	if sharedMgr != nil && sharedSciDir == sciDir {
		// Hot path: avoid disk reload per request (health/SSE hammer).
		// Mutating handlers call mgr.Reload() after config writes.
		return sharedMgr, nil
	}
	mgr, err := sciruntime.New(sciDir, lumenCfg)
	if err != nil {
		return nil, err
	}
	sharedMgr = mgr
	sharedSciDir = sciDir
	return mgr, nil
}

func resetPanelManager() {
	mgrMu.Lock()
	sharedMgr = nil
	sharedSciDir = ""
	mgrMu.Unlock()
}
