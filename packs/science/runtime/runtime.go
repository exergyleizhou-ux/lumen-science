package runtime

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"os"
	"os/signal"
	"path/filepath"
	"strings"
	"sync"
	"syscall"
	"time"

	"lumen/internal/config"
	sciconfig "lumen/internal/science/config"
	"lumen/internal/science/guard"
	"lumen/internal/science/launcher"
	"lumen/internal/science/paths"
	"lumen/internal/science/proxy"
)

// Manager orchestrates proxy + sandbox lifecycle.
type Manager struct {
	SciDir          string
	LumenCfg        *config.File
	cfg             sciconfig.File
	proxySrv        *proxy.Server
	cancel          context.CancelFunc
	proxyKeyFP      uint64
	proxyProvider   string
	lastProxyAction ProxyAction
	mu              sync.Mutex
}

// State persisted across CLI invocations.
type State struct {
	ProxyPort   int    `json:"proxy_port"`
	SandboxPort int    `json:"sandbox_port"`
	Secret      string `json:"secret"`
	Provider    string `json:"provider"`
	URL         string `json:"url,omitempty"`
}

// New creates a manager.
func New(sciDir string, lumenCfg *config.File) (*Manager, error) {
	cfg, err := sciconfig.Load(sciDir)
	if err != nil {
		return nil, err
	}
	return &Manager{SciDir: sciDir, LumenCfg: lumenCfg, cfg: cfg}, nil
}

// Config returns the current science config snapshot.
func (m *Manager) Config() sciconfig.File { return m.cfg }

// EnsureSecret persists proxy path-secret.
func (m *Manager) EnsureSecret() (string, error) {
	if m.cfg.Secret != "" {
		return m.cfg.Secret, nil
	}
	secret, err := proxy.GenerateAuthSecret()
	if err != nil {
		return "", err
	}
	cfg, err := sciconfig.Update(m.SciDir, func(c *sciconfig.File) { c.Secret = secret })
	if err != nil {
		return "", err
	}
	m.cfg = cfg
	return secret, nil
}

// ResolveAPIKey from active profile, science config, lumen.toml, or env.
func (m *Manager) ResolveAPIKey() (string, error) {
	if resolved, err := ResolveActiveSpec(m.cfg); err == nil && resolved.APIKey != "" {
		return resolved.APIKey, nil
	}
	if k := m.cfg.KeyFor(m.cfg.Provider); k != "" {
		return k, nil
	}
	built, err := proxy.BuildConfig(m.cfg.Provider, "", "", "", "", "", m.LumenCfg)
	if err == nil && built.APIKey != "" {
		return built.APIKey, nil
	}
	return proxy.ResolveAPIKey(m.cfg.Provider, "")
}

// ProxyURL returns the ANTHROPIC_BASE_URL for sandbox.
func (m *Manager) ProxyURL() (string, error) {
	secret, err := m.EnsureSecret()
	if err != nil {
		return "", err
	}
	return fmt.Sprintf("http://127.0.0.1:%d/%s", m.cfg.ProxyPort, secret), nil
}

// StartProxy runs the proxy in a background goroutine (idempotent; restarts on key/provider change).
func (m *Manager) StartProxy() (ProxyAction, error) {
	secret, err := m.EnsureSecret()
	if err != nil {
		return "", err
	}
	apiKey, err := m.ResolveAPIKey()
	if err != nil {
		return "", err
	}
	resolved, err := ResolveActiveSpec(m.cfg)
	if err != nil {
		return "", err
	}
	keyFP := KeyFingerprint(apiKey)
	// Fingerprint includes adapter/api_format/model so protocol switches always restart.
	provider := SpecFingerprint(resolved)

	m.mu.Lock()
	if m.proxySrv != nil &&
		m.proxyProvider == provider &&
		m.proxyKeyFP == keyFP &&
		proxyHealthy(m.cfg.ProxyPort, secret) {
		m.mu.Unlock()
		m.lastProxyAction = ProxyReused
		return ProxyReused, nil
	}
	// Stale listener (e.g. old python proxy) — clear before bind.
	m.mu.Unlock()
	_ = KillListenerOnPort(m.cfg.ProxyPort)
	m.StopProxy()

	m.mu.Lock()
	defer m.mu.Unlock()

	spec := resolved.Spec
	if err := guard.AssertPortSafe(m.cfg.ProxyPort); err != nil {
		return "", err
	}
	if err := guard.AssertPortsDistinct(m.cfg.ProxyPort, m.cfg.SandboxPort); err != nil {
		return "", err
	}
	shim := proxy.ToolUseShimMode(strings.ToLower(strings.TrimSpace(m.cfg.ToolUseShim)))
	addr := fmt.Sprintf("127.0.0.1:%d", m.cfg.ProxyPort)
	srv, err := proxy.New(proxy.Config{
		Provider:    spec,
		APIKey:      apiKey,
		Addr:        addr,
		AuthSecret:  secret,
		LogPath:     paths.ProxyLog(m.SciDir),
		CacheBoost:  m.cfg.CacheBoostEnabled(),
		ToolUseShim: shim,
	})
	if err != nil {
		return "", err
	}
	_, cancel := context.WithCancel(context.Background())
	m.cancel = cancel
	m.proxySrv = srv
	m.proxyKeyFP = keyFP
	m.proxyProvider = provider
	m.lastProxyAction = ProxyRestarted
	go func() { _ = srv.ListenAndServe() }()

	for i := 0; i < 50; i++ {
		if proxyHealthy(m.cfg.ProxyPort, secret) {
			return ProxyRestarted, nil
		}
		time.Sleep(100 * time.Millisecond)
	}
	m.StopProxy()
	tail := RedactSecret(launcher.TailLog(paths.ProxyLog(m.SciDir), 800), secret)
	return "", fmt.Errorf("代理探活超时（端口 %d 可能被占用或 key 无效）\n%s", m.cfg.ProxyPort, tail)
}

func proxyHealthy(port int, secret string) bool {
	path := "/health"
	if secret != "" {
		path = "/" + secret + "/health"
	}
	client := &http.Client{Timeout: 400 * time.Millisecond}
	resp, err := client.Get(fmt.Sprintf("http://127.0.0.1:%d%s", port, path))
	if err != nil {
		return false
	}
	resp.Body.Close()
	return resp.StatusCode == 200
}

// StopProxy stops the in-process proxy if running.
func (m *Manager) StopProxy() {
	m.mu.Lock()
	srv := m.proxySrv
	cancel := m.cancel
	m.proxySrv = nil
	m.cancel = nil
	m.mu.Unlock()
	if cancel != nil {
		cancel()
	}
	if srv != nil {
		ctx, c := context.WithTimeout(context.Background(), 3*time.Second)
		defer c()
		_ = srv.Shutdown(ctx)
	} else {
		// Proxy may have been started by a prior Manager (GUI handlers share process).
		_ = KillListenerOnPort(m.cfg.ProxyPort)
	}
}

// StartSandbox launches or reopens the sandbox Science instance.
func (m *Manager) StartSandbox() (string, string, error) {
	realDir, err := guard.RealScienceDir()
	if err != nil {
		return "", "", err
	}
	bin := guard.ScienceBin()
	sbxHome := paths.SandboxHome(m.SciDir)
	dataDir := paths.DataDir(m.SciDir)
	port := m.cfg.SandboxPort

	sandboxWasRunning := launcher.Running(sbxHome, dataDir, bin, port)

	var action ProxyAction
	if action, err = m.StartProxy(); err != nil {
		return "", "", err
	} else if sandboxWasRunning && action != ProxyRestarted {
		url := launcher.URL(sbxHome, dataDir, bin, port)
		_ = m.saveState(url)
		return url, "reopened", nil
	}
	if sandboxWasRunning && action == ProxyRestarted {
		// Proxy restarted with new secret — kill old sandbox, start fresh
		_ = launcher.Stop(sbxHome, dataDir, bin)
		time.Sleep(1 * time.Second)
	}
	proxyURL, err := m.ProxyURL()
	if err != nil {
		return "", "", err
	}
	if err := launcher.Start(launcher.Config{
		SandboxHome: sbxHome,
		DataDir:     dataDir,
		RealDir:     realDir,
		Bin:         bin,
		Port:        port,
		ProxyURL:    proxyURL,
		LogPath:     paths.SandboxLog(m.SciDir),
	}); err != nil {
		return "", "", err
	}
	if !launcher.WaitHealthy(port, 45*time.Second) {
		_ = launcher.Stop(sbxHome, dataDir, bin)
		secret, _ := m.EnsureSecret()
		tail := RedactSecret(launcher.TailLog(paths.SandboxLog(m.SciDir), 800), secret)
		return "", "", fmt.Errorf("沙箱探活超时（端口 %d）\n%s", port, tail)
	}
	if !launcher.Running(sbxHome, dataDir, bin, port) {
		_ = launcher.Stop(sbxHome, dataDir, bin)
		return "", "", fmt.Errorf("port %d responds but is not our sandbox Science", port)
	}
	url := launcher.URL(sbxHome, dataDir, bin, port)
	_ = m.saveState(url)
	return url, "started", nil
}

func (m *Manager) saveState(url string) error {
	secret, _ := m.EnsureSecret()
	st := State{
		ProxyPort:   m.cfg.ProxyPort,
		SandboxPort: m.cfg.SandboxPort,
		Secret:      secret,
		Provider:    m.cfg.Provider,
		URL:         url,
	}
	data, err := json.MarshalIndent(st, "", "  ")
	if err != nil {
		return err
	}
	path := paths.StateFile(m.SciDir)

	// Extreme hardening for state file (contains secret):
	// lstat refuse symlink, parent 0700, O_EXCL tmp + Sync + rename + 0600 reset.
	if err := assertNotSymlink(path); err != nil {
		return err
	}
	dir := filepath.Dir(path)
	if err := os.MkdirAll(dir, 0o700); err != nil {
		return err
	}
	_ = os.Chmod(dir, 0o700)

	seq := time.Now().UnixNano()
	tmp := filepath.Join(dir, fmt.Sprintf(".state.json.tmp-%d-%d", os.Getpid(), seq))
	writeRes := func() error {
		fh, err := os.OpenFile(tmp, os.O_WRONLY|os.O_CREATE|os.O_EXCL, 0o600)
		if err != nil {
			return err
		}
		if _, err := fh.Write(append(data, '\n')); err != nil {
			fh.Close()
			return err
		}
		if err := fh.Sync(); err != nil {
			fh.Close()
			return err
		}
		return fh.Close()
	}()
	if writeRes != nil {
		_ = os.Remove(tmp)
		return writeRes
	}
	if err := os.Rename(tmp, path); err != nil {
		_ = os.Remove(tmp)
		return err
	}
	return os.Chmod(path, 0o600)
}

func assertNotSymlink(path string) error {
	fi, err := os.Lstat(path)
	if os.IsNotExist(err) {
		return nil
	}
	if err != nil {
		return err
	}
	if fi.Mode()&os.ModeSymlink != 0 {
		return fmt.Errorf("refuse symlink: %s", path)
	}
	return nil
}

// StopSandboxOnly stops only the sandbox daemon (quit keeps sandbox running).
func (m *Manager) StopSandboxOnly() error {
	bin := guard.ScienceBin()
	sbxHome := paths.SandboxHome(m.SciDir)
	dataDir := paths.DataDir(m.SciDir)
	return launcher.Stop(sbxHome, dataDir, bin)
}

// StopAll stops sandbox and in-process proxy.
func (m *Manager) StopAll() error {
	realDir, err := guard.RealScienceDir()
	if err != nil {
		return err
	}
	_ = realDir
	bin := guard.ScienceBin()
	sbxHome := paths.SandboxHome(m.SciDir)
	dataDir := paths.DataDir(m.SciDir)
	var stopErr error
	if err := launcher.Stop(sbxHome, dataDir, bin); err != nil {
		stopErr = err
	}
	m.StopProxy()
	return stopErr
}

// Status returns a human-readable status report.
func (m *Manager) Status() map[string]any {
	secret := m.cfg.Secret
	proxyOK := proxyHealthy(m.cfg.ProxyPort, secret)
	cache := fetchProxyCacheStats(m.cfg.ProxyPort, secret)
	sbxHome := paths.SandboxHome(m.SciDir)
	dataDir := paths.DataDir(m.SciDir)
	bin := guard.ScienceBin()
	sandboxOK := launcher.Running(sbxHome, dataDir, bin, m.cfg.SandboxPort)
	url := ""
	if sandboxOK {
		url = launcher.URL(sbxHome, dataDir, bin, m.cfg.SandboxPort)
	}
	upstreamOK := false
	if spec, ok := proxy.LookupProvider(m.cfg.Provider); ok {
		host := upstreamHost(spec)
		upstreamOK = launcher.TCPReachable(host, 443, 3000)
	}
	return map[string]any{
		"provider":              m.cfg.Provider,
		"mode":                  m.cfg.Mode,
		"proxy_port":            m.cfg.ProxyPort,
		"sandbox_port":          m.cfg.SandboxPort,
		"proxy_healthy":         proxyOK,
		"sandbox_running":       sandboxOK,
		"upstream_reachable":    upstreamOK,
		"url":                   url,
		"cache_session_hit_pct": cache["session_hit_rate_pct"],
		"cache_last_hit_pct":    cache["last_hit_rate_pct"],
		"cache_hit_tokens":      cache["cache_hit_tokens"],
	}
}

func upstreamHost(spec proxy.ProviderSpec) string {
	switch spec.Name {
	case "qwen":
		return "dashscope.aliyuncs.com"
	case "moonshot":
		return "api.moonshot.cn"
	case "zhipu":
		return "open.bigmodel.cn"
	default:
		return "api.deepseek.com"
	}
}

func fetchProxyCacheStats(port int, secret string) map[string]int64 {
	path := "/health"
	if secret != "" {
		path = "/" + secret + "/health"
	}
	client := &http.Client{Timeout: 500 * time.Millisecond}
	resp, err := client.Get(fmt.Sprintf("http://127.0.0.1:%d%s", port, path))
	if err != nil {
		return map[string]int64{}
	}
	defer resp.Body.Close()
	var body map[string]any
	if json.NewDecoder(resp.Body).Decode(&body) != nil {
		return map[string]int64{}
	}
	out := map[string]int64{}
	for k, v := range body {
		if !strings.HasPrefix(k, "cache_") {
			continue
		}
		key := strings.TrimPrefix(k, "cache_")
		switch n := v.(type) {
		case float64:
			out[key] = int64(n)
		}
	}
	return out
}

// VerifyKey checks upstream key via running proxy.
func (m *Manager) VerifyKey() (bool, string, error) {
	if _, err := m.StartProxy(); err != nil {
		return false, "", err
	}
	secret, err := m.EnsureSecret()
	if err != nil {
		return false, "", err
	}
	ok, hint := launcher.VerifyKeyViaProxy(m.cfg.ProxyPort, secret)
	return ok, hint, nil
}

// RunForeground starts proxy+sandbox, opens browser, blocks until signal.
// On exit: stops proxy, keeps sandbox running.
func (m *Manager) RunForeground(openBrowser bool) error {
	url, action, err := m.StartSandbox()
	if err != nil {
		return err
	}
	st := m.Status()
	fmt.Printf("✓ Science %s\n", action)
	fmt.Printf("  访问地址:  %s\n", url)
	fmt.Printf("  模型线路:  %s\n", m.cfg.Provider)
	fmt.Printf("  代理端口:  %d\n", m.cfg.ProxyPort)
	fmt.Printf("  沙箱端口:  %d\n", m.cfg.SandboxPort)
	if m.cfg.CacheBoostEnabled() {
		fmt.Printf("  缓存增强:  开启\n")
	}
	if pct, ok := st["cache_session_hit_pct"].(int64); ok {
		fmt.Printf("  缓存命中:  %d%% (DeepSeek prefix cache)\n", pct)
	}
	if openBrowser {
		if err := launcher.OpenBrowser(url); err != nil {
			fmt.Printf("  浏览器未能自动打开，请手动访问: %s\n", url)
		}
	} else {
		fmt.Printf("  请手动打开: %s\n", url)
	}
	sig := make(chan os.Signal, 1)
	signal.Notify(sig, syscall.SIGINT, syscall.SIGTERM)
	<-sig
	fmt.Fprintln(os.Stderr, "\n停止代理（沙箱保持运行，对话不丢失）…")
	m.StopProxy()
	fmt.Println("已停止代理。沙箱仍在运行；完全停止请执行: lumen science stop")
	return nil
}

// SetProvider updates provider in config.
func SetProvider(sciDir, provider string) error {
	if _, ok := proxy.LookupProvider(provider); !ok {
		return fmt.Errorf("unsupported provider %q", provider)
	}
	_, err := sciconfig.Update(sciDir, func(c *sciconfig.File) {
		c.Provider = provider
		c.Mode = "proxy"
	})
	return err
}

// SetMode sets proxy or official mode.
func SetMode(sciDir, mode string) error {
	if mode != "proxy" && mode != "official" {
		return fmt.Errorf("mode must be proxy or official")
	}
	_, err := sciconfig.Update(sciDir, func(c *sciconfig.File) { c.Mode = mode })
	return err
}

// SwitchToOfficial stops bridge processes and opens real Science (official mode).
func SwitchToOfficial(sciDir string, lumenCfg *config.File) error {
	mgr, err := New(sciDir, lumenCfg)
	if err != nil {
		return err
	}
	_ = mgr.StopAll()
	time.Sleep(500 * time.Millisecond) // let ports release before launching official
	_, _ = sciconfig.Update(sciDir, func(c *sciconfig.File) {
		c.Mode = "official"
		c.Secret = ""
	})
	return launcher.OpenOfficial()
}

// SaveProviderKey stores API key in science config (0600).
func SaveProviderKey(sciDir, provider, key string) error {
	_, err := sciconfig.Update(sciDir, func(c *sciconfig.File) {
		if c.Providers == nil {
			c.Providers = map[string]sciconfig.ProviderCfg{}
		}
		c.Providers[provider] = sciconfig.ProviderCfg{Key: key}
	})
	return err
}
