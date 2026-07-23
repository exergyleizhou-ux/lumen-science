package runtime

import (
	"context"
	"fmt"
	"strings"
	"time"

	sciconfig "lumen/internal/science/config"
	"lumen/internal/science/proxy"
)

// SwitchProfileResult describes a transactional profile switch outcome.
type SwitchProfileResult struct {
	ProfileID   string `json:"profile_id"`
	ProfileName string `json:"profile_name"`
	Action      string `json:"action"` // activated | unchanged
	Message     string `json:"message"`
}

// SwitchProfile probes candidate upstream, commits on success, rolls back on failure.
func (m *Manager) SwitchProfile(profileID string) (SwitchProfileResult, error) {
	m.mu.Lock()
	prevID := m.cfg.ActiveProfileID
	prevProvider := m.cfg.Provider
	m.mu.Unlock()

	cfg, err := sciconfig.Load(m.SciDir)
	if err != nil {
		return SwitchProfileResult{}, err
	}
	p := cfg.ProfileByID(profileID)
	if p == nil {
		return SwitchProfileResult{}, fmt.Errorf("profile %q not found", profileID)
	}
	if cfg.ActiveProfileID == profileID {
		return SwitchProfileResult{
			ProfileID: profileID, ProfileName: p.Name,
			Action: "unchanged", Message: "已是当前配置",
		}, nil
	}

	resolved, err := resolveProfile(*p)
	if err != nil {
		return SwitchProfileResult{}, err
	}
	ctx, cancel := context.WithTimeout(context.Background(), 20*time.Second)
	defer cancel()
	code, hint, err := proxy.ProbeUpstreamKey(ctx, resolved.Spec, resolved.APIKey, strings.TrimSpace(p.Model))
	if err != nil {
		return SwitchProfileResult{}, fmt.Errorf("上游探测失败: %w", err)
	}
	if code == 401 || code == 403 {
		return SwitchProfileResult{}, fmt.Errorf("切换未生效：API key 无效或被拒 (HTTP %d)，仍使用原配置", code)
	}
	if code < 200 || code >= 500 {
		return SwitchProfileResult{}, fmt.Errorf("切换未生效：上游 HTTP %d (%s)，仍使用原配置", code, hint)
	}

	// Commit: persist active pointer + legacy provider field for compatibility
	cfg, err = sciconfig.Update(m.SciDir, func(c *sciconfig.File) {
		c.ActiveProfileID = profileID
		c.Provider = resolved.Adapter
	})
	if err != nil {
		return SwitchProfileResult{}, fmt.Errorf("写盘失败，未切换: %w", err)
	}
	m.mu.Lock()
	m.cfg = cfg
	m.mu.Unlock()

	// Restart proxy with new spec; rollback active on failure
	if _, err := m.StartProxy(); err != nil {
		_, _ = sciconfig.Update(m.SciDir, func(c *sciconfig.File) {
			c.ActiveProfileID = prevID
			c.Provider = prevProvider
		})
		_ = m.Reload()
		return SwitchProfileResult{}, fmt.Errorf("代理重启失败，已回滚: %w", err)
	}

	return SwitchProfileResult{
		ProfileID: profileID, ProfileName: p.Name,
		Action:  "activated",
		Message: fmt.Sprintf("已切换到 %s（上游 HTTP %d）", p.Name, code),
	}, nil
}

// ProbeProfile validates a profile's credentials against upstream (no config write).
func ProbeProfile(p sciconfig.Profile) (bool, string, error) {
	resolved, err := resolveProfile(p)
	if err != nil {
		return false, "", err
	}
	ctx, cancel := context.WithTimeout(context.Background(), 20*time.Second)
	defer cancel()
	code, hint, err := proxy.ProbeUpstreamKey(ctx, resolved.Spec, resolved.APIKey, strings.TrimSpace(p.Model))
	if err != nil {
		return false, "", err
	}
	switch code {
	case 200:
		return true, hint, nil
	case 401, 403:
		return false, fmt.Sprintf("key rejected (HTTP %d)", code), nil
	default:
		return false, fmt.Sprintf("upstream HTTP %d: %s", code, hint), nil
	}
}

// ProbeProfileKey validates a profile's key against upstream without switching.
func ProbeProfileKey(sciDir, profileID string) (bool, string, error) {
	cfg, err := sciconfig.Load(sciDir)
	if err != nil {
		return false, "", err
	}
	p := cfg.ProfileByID(profileID)
	if p == nil {
		return false, "", fmt.Errorf("profile not found")
	}
	return ProbeProfile(*p)
}
