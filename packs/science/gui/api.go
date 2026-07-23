package gui

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"os"
	"os/exec"
	"runtime"
	"strings"
	"sync"
	"time"

	"lumen/internal/config"
	sciconfig "lumen/internal/science/config"
	"lumen/internal/science/launcher"
	"lumen/internal/science/paths"
	"lumen/internal/science/proxy"
	"lumen/internal/science/research"
	sciruntime "lumen/internal/science/runtime"
)

const actionTimeout = 120 * time.Second

// API wraps runtime.Manager with serialized mutating operations.
type API struct {
	sciDir    string
	lumenCfg  *config.File
	version   string
	startedAt time.Time
	mu        sync.Mutex
	lastURL   string
}

// NewAPI builds the GUI backend.
func NewAPI(sciDir string, lumenCfg *config.File, version string, startedAt time.Time) *API {
	if startedAt.IsZero() {
		startedAt = time.Now()
	}
	return &API{sciDir: sciDir, lumenCfg: lumenCfg, version: version, startedAt: startedAt}
}

// Register mounts JSON + SSE routes.
func (a *API) Register(mux *http.ServeMux) {
	mux.HandleFunc("/api/health", a.handleHealth)
	mux.HandleFunc("/api/config", a.handleConfig)
	mux.HandleFunc("/api/mode", a.handleMode)
	mux.HandleFunc("/api/key", a.handleKey)
	mux.HandleFunc("/api/verify", a.handleVerify)
	mux.HandleFunc("/api/start", a.handleStart)
	mux.HandleFunc("/api/stop", a.handleStop)
	mux.HandleFunc("/api/official", a.handleOfficial)
	mux.HandleFunc("/api/status", a.handleStatus)
	mux.HandleFunc("/api/events", a.handleEvents)
	mux.HandleFunc("/api/doctor", a.handleDoctor)
	mux.HandleFunc("/api/logs", a.handleLogs)
	mux.HandleFunc("/api/research", a.handleResearch)
	mux.HandleFunc("/api/version", a.handleVersion)
	mux.HandleFunc("/api/open-browser", a.handleOpenBrowser)
	mux.HandleFunc("/api/open-logs-dir", a.handleOpenLogsDir)
	mux.HandleFunc("/api/quit-proxy", a.handleQuitProxy)
	a.RegisterNative(mux)
	a.RegisterProfiles(mux)
}

func (a *API) StopProxyOnly() {
	mgr, _ := panelManager(a.sciDir, a.lumenCfg)
	if mgr != nil {
		mgr.StopProxy()
	}
}

func (a *API) manager() (*sciruntime.Manager, error) {
	return panelManager(a.sciDir, a.lumenCfg)
}

func (a *API) withAction(fn func(*sciruntime.Manager) (any, error)) (any, error) {
	a.mu.Lock()
	defer a.mu.Unlock()
	ctx, cancel := context.WithTimeout(context.Background(), actionTimeout)
	defer cancel()
	ch := make(chan struct {
		v   any
		err error
	}, 1)
	go func() {
		mgr, err := a.manager()
		if err != nil {
			ch <- struct {
				v   any
				err error
			}{nil, err}
			return
		}
		v, err := fn(mgr)
		ch <- struct {
			v   any
			err error
		}{v, err}
	}()
	select {
	case <-ctx.Done():
		return nil, fmt.Errorf("操作超时，请稍后重试")
	case r := <-ch:
		return r.v, r.err
	}
}

func (a *API) handleHealth(w http.ResponseWriter, r *http.Request) {
	mgr, err := a.manager()
	healthy := err == nil
	st := map[string]any{}
	if healthy {
		st = mgr.Status()
	}
	writeJSON(w, http.StatusOK, map[string]any{
		"status":          "ok",
		"panel":           "lumen://science",
		"https_url":       fmt.Sprintf("https://127.0.0.1:%d", 18993),
		"version":         a.version,
		"healthy":         healthy,
		"uptime_ms":       time.Since(a.startedAt).Milliseconds(),
		"proxy":           st["proxy_healthy"],
		"sandbox":         st["sandbox_running"],
		"provider":        st["provider"],
		"mode":            st["mode"],
		"proxy_port":      st["proxy_port"],
		"sandbox_port":    st["sandbox_port"],
		// Behavioral parity target (CSSwitch ships Rust gateway; Lumen uses Go).
		"csswitch_parity": proxy.CSSwitchParity,
		"gateway":         "go",
	})
}

func (a *API) handleConfig(w http.ResponseWriter, r *http.Request) {
	switch r.Method {
	case http.MethodGet:
		cfg, err := sciconfig.Load(a.sciDir)
		if err != nil {
			writeErr(w, http.StatusInternalServerError, err)
			return
		}
		keys := map[string]string{}
		for name, p := range cfg.Providers {
			if p.Key != "" {
				keys[name] = sciconfig.MaskKey(p.Key)
			}
		}
		profiles := make([]map[string]any, 0, len(cfg.Profiles))
		for _, p := range cfg.Profiles {
			profiles = append(profiles, profileJSON(p, p.ID == cfg.ActiveProfileID))
		}
		writeJSON(w, http.StatusOK, map[string]any{
			"provider": cfg.Provider, "proxy_port": cfg.ProxyPort,
			"sandbox_port": cfg.SandboxPort, "mode": cfg.Mode,
			"cache_boost": cfg.CacheBoostEnabled(), "keys": keys,
			"providers": providerList(),
			"profiles":  profiles, "active_profile_id": cfg.ActiveProfileID,
			"tooluse_shim": cfg.ToolUseShim, "schema_version": cfg.SchemaVersion,
		})
	case http.MethodPut:
		var body struct {
			Provider    string `json:"provider"`
			ProxyPort   int    `json:"proxy_port"`
			SandboxPort int    `json:"sandbox_port"`
			CacheBoost  *bool  `json:"cache_boost"`
			ToolUseShim string `json:"tooluse_shim"`
		}
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			writeErr(w, http.StatusBadRequest, err)
			return
		}
		_, err := a.withAction(func(mgr *sciruntime.Manager) (any, error) {
			_, err := sciconfig.Update(a.sciDir, func(c *sciconfig.File) {
				if body.Provider != "" {
					if _, ok := proxy.LookupProvider(body.Provider); ok {
						c.Provider = body.Provider
					}
				}
				if body.ProxyPort > 0 {
					c.ProxyPort = body.ProxyPort
				}
				if body.SandboxPort > 0 {
					c.SandboxPort = body.SandboxPort
				}
				if body.CacheBoost != nil {
					c.CacheBoost = body.CacheBoost
				}
				if s := strings.TrimSpace(body.ToolUseShim); s != "" {
					c.ToolUseShim = s
				}
			})
			if err == nil {
				_ = mgr.Reload()
			}
			return map[string]any{"ok": true}, err
		})
		if err != nil {
			writeErr(w, http.StatusBadRequest, err)
			return
		}
		writeJSON(w, http.StatusOK, map[string]any{"ok": true})
	default:
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
	}
}

func (a *API) handleMode(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPut {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}
	var body struct {
		Mode string `json:"mode"`
	}
	if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	switch body.Mode {
	case "official":
		_, err := a.withAction(func(mgr *sciruntime.Manager) (any, error) {
			if err := mgr.StopAll(); err != nil {
				return nil, err
			}
			_, err := sciconfig.Update(a.sciDir, func(c *sciconfig.File) {
				c.Mode = "official"
				c.Secret = ""
			})
			return map[string]any{"ok": true}, err
		})
		if err != nil {
			writeErr(w, http.StatusBadRequest, err)
			return
		}
		_ = launcher.OpenOfficial()
		writeJSON(w, http.StatusOK, map[string]any{"ok": true})
	case "proxy":
		if err := sciruntime.SetMode(a.sciDir, "proxy"); err != nil {
			writeErr(w, http.StatusBadRequest, err)
			return
		}
		writeJSON(w, http.StatusOK, map[string]any{"ok": true})
	default:
		writeErr(w, http.StatusBadRequest, fmt.Errorf("mode must be proxy or official"))
	}
}

func (a *API) handleKey(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}
	var body struct {
		Provider string `json:"provider"`
		Key      string `json:"key"`
	}
	if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	if strings.TrimSpace(body.Key) == "" {
		writeErr(w, http.StatusBadRequest, fmt.Errorf("key 不能为空"))
		return
	}
	masked, err := a.withAction(func(mgr *sciruntime.Manager) (any, error) {
		if err := sciruntime.SaveProviderKey(a.sciDir, body.Provider, body.Key); err != nil {
			return nil, err
		}
		return sciconfig.MaskKey(body.Key), nil
	})
	if err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	writeJSON(w, http.StatusOK, map[string]any{"masked": masked})
}

func (a *API) handleVerify(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}
	result, err := a.withAction(func(mgr *sciruntime.Manager) (any, error) {
		ok, hint, err := mgr.VerifyKey()
		if err != nil {
			return nil, err
		}
		return map[string]any{"ok": ok, "hint": hint}, nil
	})
	if err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	writeJSON(w, http.StatusOK, result)
}

func (a *API) handleStart(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}
	result, err := a.withAction(func(mgr *sciruntime.Manager) (any, error) {
		url, action, msg, err := mgr.OneClick(true)
		if err != nil {
			return nil, err
		}
		a.lastURL = url
		return map[string]any{"url": url, "action": action, "msg": msg}, nil
	})
	if err != nil {
		// Friendlier copy when Claude Science desktop is absent (common on Linux/VPS/Oasis web).
		msg := err.Error()
		if strings.Contains(msg, "Science binary not found") || strings.Contains(msg, "claude-science") {
			writeErr(w, http.StatusBadRequest, fmt.Errorf(
				"未找到 Claude Science 桌面程序，无法启动沙箱。绿洲网页请打开 /lumen-lab/ ；Mac 请安装 Claude Science.app 后再试"))
			return
		}
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	writeJSON(w, http.StatusOK, result)
}

func (a *API) handleStop(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}
	_, err := a.withAction(func(mgr *sciruntime.Manager) (any, error) {
		return nil, mgr.StopAll()
	})
	if err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	writeJSON(w, http.StatusOK, map[string]any{"ok": true})
}

func (a *API) handleOfficial(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}
	if err := sciruntime.SwitchToOfficial(a.sciDir, a.lumenCfg); err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	writeJSON(w, http.StatusOK, map[string]any{"ok": true})
}

func (a *API) handleStatus(w http.ResponseWriter, r *http.Request) {
	mgr, err := a.manager()
	if err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return
	}
	st := mgr.Status()
	writeJSON(w, http.StatusOK, lightsFromStatus(st))
}

func (a *API) handleEvents(w http.ResponseWriter, r *http.Request) {
	flusher, ok := w.(http.Flusher)
	if !ok {
		http.Error(w, "streaming unsupported", http.StatusInternalServerError)
		return
	}
	w.Header().Set("Content-Type", "text/event-stream")
	w.Header().Set("Cache-Control", "no-cache")
	w.Header().Set("Connection", "keep-alive")
	tick := time.NewTicker(2 * time.Second)
	defer tick.Stop()
	ctx := r.Context()
	for {
		select {
		case <-ctx.Done():
			return
		case <-tick.C:
			mgr, err := a.manager()
			if err != nil {
				continue
			}
			payload, _ := json.Marshal(lightsFromStatus(mgr.Status()))
			fmt.Fprintf(w, "data: %s\n\n", payload)
			flusher.Flush()
		}
	}
}

func lightsFromStatus(st map[string]any) map[string]any {
	light := func(ok bool) string {
		if ok {
			return "green"
		}
		return "amber"
	}
	proxyOK, _ := st["proxy_healthy"].(bool)
	sbxOK, _ := st["sandbox_running"].(bool)
	upOK, _ := st["upstream_reachable"].(bool)
	return map[string]any{
		"proxy": light(proxyOK), "sandbox": light(sbxOK), "upstream": light(upOK),
		"provider": st["provider"], "mode": st["mode"],
		"proxy_port": st["proxy_port"], "sandbox_port": st["sandbox_port"],
		"url":                   st["url"],
		"cache_session_hit_pct": st["cache_session_hit_pct"],
		"cache_last_hit_pct":    st["cache_last_hit_pct"],
		"cache_hit_tokens":      st["cache_hit_tokens"],
	}
}

func (a *API) handleDoctor(w http.ResponseWriter, r *http.Request) {
	results, warns, fails := sciruntime.RunDoctor(a.sciDir, a.lumenCfg)
	writeJSON(w, http.StatusOK, map[string]any{
		"results": results, "warnings": warns, "failures": fails,
	})
}

func (a *API) handleLogs(w http.ResponseWriter, r *http.Request) {
	which := r.URL.Query().Get("which")
	if which == "" {
		which = "proxy"
	}
	var path string
	switch which {
	case "sandbox":
		path = paths.SandboxLog(a.sciDir)
	default:
		path = paths.ProxyLog(a.sciDir)
	}
	data, err := os.ReadFile(path)
	if err != nil {
		writeJSON(w, http.StatusOK, map[string]any{"text": "", "error": err.Error()})
		return
	}
	if len(data) > 12000 {
		data = data[len(data)-12000:]
	}
	writeJSON(w, http.StatusOK, map[string]any{"text": string(data)})
}

func (a *API) handleResearch(w http.ResponseWriter, r *http.Request) {
	dataDir := paths.DataDir(a.sciDir)
	if r.URL.Query().Get("verify") == "1" {
		rep, err := research.Scan(dataDir)
		if err != nil {
			writeErr(w, http.StatusBadRequest, err)
			return
		}
		writeJSON(w, http.StatusOK, map[string]any{
			"healthy":          rep.Healthy(),
			"runtime_version":  rep.RuntimeVersion,
			"bio_lib_packages": rep.BioLibPackages,
			"domain_mcp":       rep.DomainMCPServers,
			"skills":           len(rep.Skills),
			"domain_tools":     rep.TotalDomainTools,
			"domains":          len(rep.Domains),
			"org_pack_seeded":  rep.OrgPackSeeded,
			"workspaces":       rep.Workspaces,
			"missing_skills":   rep.MissingSkills,
			"clone_assets":     rep.CloneAssets,
		})
		return
	}
	writeJSON(w, http.StatusOK, research.Catalog(dataDir))
}

func (a *API) handleVersion(w http.ResponseWriter, r *http.Request) {
	writeJSON(w, http.StatusOK, map[string]any{
		"version": a.version,
		"release": "https://github.com/exergyleizhou-ux/lumen/releases/latest",
		"issues":  "https://github.com/exergyleizhou-ux/lumen/issues/new",
	})
}

func (a *API) handleOpenBrowser(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}
	url := a.lastURL
	mgr, _ := a.manager()
	if mgr != nil {
		if u, _ := mgr.Status()["url"].(string); u != "" {
			url = u
		}
	}
	if url == "" {
		writeErr(w, http.StatusBadRequest, fmt.Errorf("沙箱未运行，请先一键开始"))
		return
	}
	if err := launcher.OpenBrowser(url); err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	writeJSON(w, http.StatusOK, map[string]any{"url": url})
}

func (a *API) handleOpenLogsDir(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}
	dir := a.sciDir
	if runtime.GOOS == "darwin" {
		_ = exec.Command("open", dir).Run()
	}
	writeJSON(w, http.StatusOK, map[string]any{"path": dir})
}

func (a *API) handleQuitProxy(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}
	a.StopProxyOnly()
	writeJSON(w, http.StatusOK, map[string]any{"ok": true})
}

func providerList() []map[string]string {
	names := []string{"deepseek", "qwen", "moonshot", "zhipu", "minimax"}
	labels := map[string]string{
		"deepseek": "DeepSeek", "qwen": "通义千问", "moonshot": "Moonshot Kimi", "zhipu": "智谱 GLM", "minimax": "MiniMax",
	}
	var out []map[string]string
	for _, n := range names {
		if _, ok := proxy.LookupProvider(n); ok {
			out = append(out, map[string]string{"id": n, "label": labels[n]})
		}
	}
	return out
}

func writeJSON(w http.ResponseWriter, code int, v any) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(code)
	_ = json.NewEncoder(w).Encode(v)
}

func writeErr(w http.ResponseWriter, code int, err error) {
	writeJSON(w, code, map[string]any{"error": err.Error()})
}
