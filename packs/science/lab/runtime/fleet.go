package runtime

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sync"
	"time"

	"lumen/internal/mcplife"
	"lumen/internal/science/mcp"
	"lumen/internal/science/native"
	"lumen/internal/science/paths"
	"lumen/internal/science/research"
)

const connectTimeout = 20 * time.Second

// FleetManager connects bio-tools domain MCP servers and the Lumen 5-ship fleet.
type FleetManager struct {
	sciDir     string
	dataDir    string
	bioTools   string
	python     string
	mu         sync.Mutex
	domains    map[string]*mcplife.Client
	domainMeta []DomainInfo
	native     *native.Manager
	errors     map[string]string
}

// NewFleetManager prepares a fleet for the given science directory.
func NewFleetManager(sciDir string) (*FleetManager, error) {
	dataDir := paths.DataDir(sciDir)
	meta, bioTools, err := Discover(sciDir)
	if err != nil {
		return &FleetManager{
			sciDir:  sciDir,
			dataDir: dataDir,
			errors:  map[string]string{"discover": err.Error()},
		}, nil
	}
	return &FleetManager{
		sciDir:     sciDir,
		dataDir:    dataDir,
		bioTools:   bioTools,
		python:     ResolvePython(dataDir),
		domains:    map[string]*mcplife.Client{},
		domainMeta: meta,
		errors:     map[string]string{},
	}, nil
}

// Domains returns discovered domain metadata.
func (f *FleetManager) Domains() []DomainInfo {
	if f == nil {
		return nil
	}
	return f.domainMeta
}

// ResearchHealthy reports whether the cloned pack passes research.Scan.
func (f *FleetManager) ResearchHealthy() bool {
	if f == nil {
		return false
	}
	rep, err := research.Scan(f.dataDir)
	return err == nil && rep.Healthy()
}

// Status returns fleet connection summary.
func (f *FleetManager) Status() map[string]any {
	f.mu.Lock()
	defer f.mu.Unlock()
	connected := len(f.domains)
	total := len(f.domainMeta)
	nativeOK := 0
	if f.native != nil {
		nativeOK = len(native.ShippedFleet())
	}
	// Soften discover noise: missing CS pack is expected on lean VPS deploys.
	errs := map[string]string{}
	for k, v := range f.errors {
		if k == "discover" && nativeOK > 0 {
			continue // native fleet is enough for production lab core
		}
		errs[k] = v
	}
	out := map[string]any{
		"cs_domains":      total,
		"cs_connected":    connected,
		"lumen_native":    nativeOK,
		"connected_total": connected + boolNative(f.native),
		"errors":          errs,
		"degraded":        nativeOK > 0 && total == 0,
	}
	if total == 0 && nativeOK > 0 {
		out["note"] = "CS research pack not installed; Lumen native 5-ship fleet operational"
	}
	return out
}

func boolNative(m *native.Manager) int {
	if m == nil {
		return 0
	}
	return len(native.ShippedFleet())
}

// ConnectDomains eagerly connects the given domain slugs (empty = all discovered).
func (f *FleetManager) ConnectDomains(slugs ...string) error {
	if f == nil || f.bioTools == "" {
		return fmt.Errorf("research pack not available")
	}
	if len(slugs) == 0 {
		return f.ConnectAllCS()
	}
	var firstErr error
	for _, slug := range slugs {
		if err := f.connectDomain(slug); err != nil && firstErr == nil {
			firstErr = err
		}
	}
	_ = f.connectNative()
	return firstErr
}

// ConnectAllCS connects every discovered bio-tools domain (non-blocking on failure).
func (f *FleetManager) ConnectAllCS() error {
	if f == nil || f.bioTools == "" {
		return fmt.Errorf("research pack not available")
	}
	var firstErr error
	for _, d := range f.domainMeta {
		if err := f.connectDomain(d.Slug); err != nil && firstErr == nil {
			firstErr = err
		}
	}
	_ = f.connectNative()
	return firstErr
}

// ConnectAll connects all CS domains + 5-ship Lumen native fleet.
func (f *FleetManager) ConnectAll() error {
	err := f.ConnectAllCS()
	_ = f.connectNative()
	return err
}

func (f *FleetManager) connectNative() error {
	f.mu.Lock()
	defer f.mu.Unlock()
	if f.native != nil {
		return nil
	}
	mgr, err := native.NewManagerFromDir(f.sciDir)
	if err != nil {
		f.errors["native"] = err.Error()
		return err
	}
	if _, err := mgr.ConnectAll(); err != nil {
		f.errors["native"] = err.Error()
		mgr.Close()
		return err
	}
	f.native = mgr
	return nil
}

func (f *FleetManager) connectDomain(slug string) error {
	f.mu.Lock()
	if c := f.domains[slug]; c != nil {
		f.mu.Unlock()
		return nil
	}
	f.mu.Unlock()

	pkg := domainPackage(slug)
	runner := filepath.Join(f.bioTools, "run_server.py")
	if _, err := os.Stat(runner); err != nil {
		f.mu.Lock()
		f.errors[slug] = "run_server.py missing"
		f.mu.Unlock()
		return fmt.Errorf("%s: %w", slug, err)
	}
	c, err := mcplife.NewMCPClientNDJSON(f.python, []string{runner, pkg})
	if err != nil {
		f.mu.Lock()
		f.errors[slug] = err.Error()
		f.mu.Unlock()
		return fmt.Errorf("%s: %w", slug, err)
	}
	initDone := make(chan error, 1)
	go func() { initDone <- c.Initialize("lumen-science-lab", "1.0.0") }()
	select {
	case err := <-initDone:
		if err != nil {
			_ = c.Close()
			f.mu.Lock()
			f.errors[slug] = err.Error()
			f.mu.Unlock()
			return fmt.Errorf("%s init: %w", slug, err)
		}
	case <-time.After(connectTimeout):
		_ = c.Close()
		f.mu.Lock()
		f.errors[slug] = "connect timeout"
		f.mu.Unlock()
		return fmt.Errorf("%s: connect timeout", slug)
	}
	f.mu.Lock()
	f.domains[slug] = c
	delete(f.errors, slug)
	f.mu.Unlock()
	return nil
}

// CallDomain invokes a tool on a domain MCP server (lazy connect).
func (f *FleetManager) CallDomain(_ context.Context, slug, tool string, args map[string]any) (string, error) {
	if err := f.connectDomain(slug); err != nil {
		return "", err
	}
	f.mu.Lock()
	c := f.domains[slug]
	f.mu.Unlock()
	if c == nil {
		return "", fmt.Errorf("domain %q not connected", slug)
	}
	raw, err := json.Marshal(args)
	if err != nil {
		return "", err
	}
	out, err := c.CallToolRaw(tool, raw)
	if err != nil {
		return "", err
	}
	wrapped := mcp.WithProvenance("bio-tools:"+slug, tool, "", out)
	b, _ := json.Marshal(wrapped)
	return string(b), nil
}

// CallNative invokes a Lumen native fleet tool.
func (f *FleetManager) CallNative(fleetID, tool string, args map[string]any) (string, error) {
	_ = f.connectNative()
	f.mu.Lock()
	mgr := f.native
	f.mu.Unlock()
	if mgr == nil {
		return "", fmt.Errorf("native fleet unavailable")
	}
	return mgr.CallTool(fleetID, tool, args)
}

// Close shuts down MCP clients.
func (f *FleetManager) Close() {
	if f == nil {
		return
	}
	f.mu.Lock()
	defer f.mu.Unlock()
	for _, c := range f.domains {
		if c != nil {
			_ = c.Close()
		}
	}
	f.domains = map[string]*mcplife.Client{}
	if f.native != nil {
		f.native.Close()
		f.native = nil
	}
}
