package native

import (
	"context"
	"encoding/json"
	"fmt"
	"sync"
	"time"

	"lumen/internal/mcplife"
	sciconfig "lumen/internal/science/config"
)

const connectTimeout = 15 * time.Second

// Manager connects to the shipped native MCP fleet via mcplife.
type Manager struct {
	mu      sync.Mutex
	clients map[string]*mcplife.Client
	cfg     sciconfig.File
}

// NewManager creates a native fleet manager.
func NewManager() *Manager {
	return &Manager{clients: map[string]*mcplife.Client{}}
}

// NewManagerFromDir loads science config for oasis token/base URL injection.
func NewManagerFromDir(sciDir string) (*Manager, error) {
	cfg, err := sciconfig.Load(sciDir)
	if err != nil {
		return nil, err
	}
	return &Manager{clients: map[string]*mcplife.Client{}, cfg: cfg}, nil
}

// ConnectAll starts all shipped MCP servers.
func (m *Manager) ConnectAll() ([]FleetMember, error) {
	members := ShippedFleet()
	for _, mem := range members {
		if err := m.connectOne(mem); err != nil {
			return members, fmt.Errorf("%s: %w", mem.ID, err)
		}
	}
	return members, nil
}

// Connect connects one fleet member by id.
func (m *Manager) Connect(id string) error {
	for _, mem := range ShippedFleet() {
		if mem.ID == id {
			return m.connectOne(mem)
		}
	}
	return fmt.Errorf("unknown or unshipped fleet member %q", id)
}

func (m *Manager) connectOne(mem FleetMember) error {
	m.mu.Lock()
	defer m.mu.Unlock()
	if old := m.clients[mem.ID]; old != nil {
		_ = old.Close()
	}
	extraEnv := FleetEnv(mem.ID, m.cfg)
	c, err := mcplife.NewMCPClientEnv(mem.Command, mem.Args, extraEnv)
	if err != nil {
		return err
	}
	initDone := make(chan error, 1)
	go func() { initDone <- c.Initialize("lumen-science", "1.0.0") }()
	select {
	case err := <-initDone:
		if err != nil {
			_ = c.Close()
			return err
		}
	case <-time.After(connectTimeout):
		_ = c.Close()
		return fmt.Errorf("connect timeout after %s", connectTimeout)
	}
	m.clients[mem.ID] = c
	return nil
}

// CallTool invokes a tool on a connected fleet member.
func (m *Manager) CallTool(fleetID, tool string, args map[string]any) (string, error) {
	return m.CallToolCtx(context.Background(), fleetID, tool, args)
}

// CallToolCtx invokes a tool with a context deadline.
func (m *Manager) CallToolCtx(ctx context.Context, fleetID, tool string, args map[string]any) (string, error) {
	m.mu.Lock()
	c := m.clients[fleetID]
	m.mu.Unlock()
	if c == nil {
		return "", fmt.Errorf("fleet %q not connected", fleetID)
	}
	raw, err := json.Marshal(args)
	if err != nil {
		return "", err
	}
	type out struct {
		text string
		err  error
	}
	ch := make(chan out, 1)
	go func() {
		text, err := c.CallToolRaw(tool, raw)
		ch <- out{text, err}
	}()
	select {
	case <-ctx.Done():
		return "", ctx.Err()
	case r := <-ch:
		return r.text, r.err
	}
}

// Close shuts down all MCP clients.
func (m *Manager) Close() {
	m.mu.Lock()
	defer m.mu.Unlock()
	for id, c := range m.clients {
		if c != nil {
			_ = c.Close()
		}
		delete(m.clients, id)
	}
}

// Status returns connection info for shipped fleet members.
func (m *Manager) Status() []map[string]any {
	m.mu.Lock()
	defer m.mu.Unlock()
	out := make([]map[string]any, 0, len(ShippedFleet()))
	for _, mem := range ShippedFleet() {
		row := map[string]any{
			"id":     mem.ID,
			"label":  mem.Label,
			"status": mem.Status,
		}
		if c := m.clients[mem.ID]; c != nil {
			tools, err := c.ListTools()
			if err == nil {
				names := make([]string, 0, len(tools))
				for _, t := range tools {
					names = append(names, t.Name)
				}
				row["connected"] = true
				row["tools"] = names
			} else {
				row["connected"] = true
				row["tools_error"] = err.Error()
			}
		} else {
			row["connected"] = false
		}
		out = append(out, row)
	}
	return out
}
