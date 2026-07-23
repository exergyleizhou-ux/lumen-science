package launcher

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"net"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"time"

	"lumen/internal/science/guard"
	"lumen/internal/science/oauth"
	"lumen/internal/science/research"
)

// Config for sandbox launch.
type Config struct {
	SandboxHome string
	DataDir     string
	RealDir     string
	Bin         string
	Port        int
	ProxyURL    string
	LogPath     string
}

// Start ensures runtime assets, virtual login, keychain, and launches detached Science.
func Start(cfg Config) error {
	if err := guard.AssertPortSafe(cfg.Port); err != nil {
		return err
	}
	if err := guard.AssertDataDirIsolated(cfg.DataDir, cfg.RealDir); err != nil {
		return err
	}
	if _, err := os.Stat(cfg.Bin); err != nil {
		return fmt.Errorf("Science binary not found: %s", cfg.Bin)
	}

	if err := ensureRuntimeAssets(cfg.DataDir, cfg.RealDir); err != nil {
		return err
	}
	if runtime.GOOS == "darwin" {
		if err := ensureSandboxKeychain(cfg.SandboxHome); err != nil {
			return err
		}
	}

	fr, action, err := oauth.EnsureVirtualLogin(cfg.DataDir, cfg.SandboxHome, cfg.RealDir)
	if err != nil {
		return fmt.Errorf("virtual login: %w", err)
	}
	if err := research.EnsureOrgPack(cfg.DataDir, fr.OrgUUID); err != nil {
		return fmt.Errorf("research pack: %w", err)
	}
	appendLog(cfg.LogPath, fmt.Sprintf("[oauth] virtual login ready action=%s auth_dir=%s org=%s", action, cfg.DataDir, fr.OrgUUID))
	if rep, err := research.Scan(cfg.DataDir); err == nil && rep.Healthy() {
		appendLog(cfg.LogPath, fmt.Sprintf("[research] ok: %d db clients, %d domain MCP, %d skills, %d domain tools, seeds=%d",
			rep.BioLibPackages, rep.DomainMCPServers, len(rep.Skills), rep.TotalDomainTools, len(rep.SeedExamples)))
	} else if err != nil {
		appendLog(cfg.LogPath, fmt.Sprintf("[research] runtime scan pending: %v (first start clones assets)", err))
	}

	proxyHostPort := strings.TrimPrefix(strings.TrimPrefix(cfg.ProxyURL, "http://"), "https://")
	if i := strings.Index(proxyHostPort, "/"); i >= 0 {
		proxyHostPort = proxyHostPort[:i]
	}
	noProxy := "127.0.0.1,localhost,::1"
	fastFailProxy := "http://" + proxyHostPort

	cmd := exec.Command(cfg.Bin, "serve",
		"--data-dir", cfg.DataDir,
		"--port", fmt.Sprintf("%d", cfg.Port),
		"--no-browser", "--no-auto-update", "--detached",
		"--dangerously-no-sandbox",
	)
	cmd.Env = append(os.Environ(),
		"HOME="+cfg.SandboxHome,
		"ANTHROPIC_BASE_URL="+cfg.ProxyURL,
		"https_proxy="+fastFailProxy,
		"HTTPS_PROXY="+fastFailProxy,
		"no_proxy="+noProxy,
		"NO_PROXY="+noProxy,
	)
	if cfg.LogPath != "" {
		if err := os.MkdirAll(filepath.Dir(cfg.LogPath), 0o700); err == nil {
			f, err := os.OpenFile(cfg.LogPath, os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0o600)
			if err == nil {
				cmd.Stdout = f
				cmd.Stderr = f
			}
		}
	}

	appendLog(cfg.LogPath, fmt.Sprintf("launch sandbox HOME=%s port=%d proxy=%s", cfg.SandboxHome, cfg.Port, redactProxyURL(cfg.ProxyURL)))
	if err := cmd.Run(); err != nil {
		return fmt.Errorf("launch sandbox: %w", err)
	}
	return nil
}

// Stop stops the sandbox daemon for the given data-dir.
func Stop(sandboxHome, dataDir, bin string) error {
	if _, err := os.Stat(dataDir); os.IsNotExist(err) {
		return nil
	}
	cmd := exec.Command(bin, "stop", "--data-dir", dataDir)
	cmd.Env = append(os.Environ(), "HOME="+sandboxHome)
	out, err := cmd.CombinedOutput()
	if err != nil {
		return fmt.Errorf("stop sandbox: %w: %s", err, strings.TrimSpace(string(out)))
	}
	return nil
}

// Running reports whether our sandbox Science is running.
// Prefers HTTP health check (most reliable) over CLI status.
func Running(sandboxHome, dataDir, bin string, port int) bool {
	// HTTP health check is the gold standard — if it responds, sandbox is up
	if HTTPHealth(port, 400) {
		return true
	}
	if _, err := os.Stat(bin); err != nil {
		return false
	}
	cmd := exec.Command(bin, "status", "--data-dir", dataDir)
	cmd.Env = append(os.Environ(), "HOME="+sandboxHome)
	out, err := cmd.Output()
	if err != nil {
		return HTTPHealth(port, 400)
	}
	s := string(out)
	// Strict parse (avoid fragile contains): prefer json, fallback to status line tokens.
	running := false
	var st struct {
		Running bool `json:"running"`
	}
	if json.Unmarshal([]byte(s), &st) == nil {
		running = st.Running
	} else {
		// fallback token parse like proc http status nth(1)
		for _, line := range strings.Split(s, "\n") {
			if strings.Contains(line, "running") {
				if strings.Contains(line, "true") {
					running = true
					break
				}
			}
		}
	}
	if !running {
		return false
	}
	// Self-heal gate: daemon "alive" but virtual login broken → treat not running (force repair on next ensure/start)
	if !oauth.IsLoginIntact(dataDir) {
		return false
	}
	return HTTPHealth(port, 400)
}

// URL returns the sandbox UI URL.
func URL(sandboxHome, dataDir, bin string, port int) string {
	if _, err := os.Stat(bin); err == nil {
		cmd := exec.Command(bin, "url", "--data-dir", dataDir)
		cmd.Env = append(os.Environ(), "HOME="+sandboxHome)
		out, err := cmd.Output()
		if err == nil {
			// first_http_url: take only the first valid http(s) line (robust to multi-line output + notes)
			// first_http_url: first valid http(s) line only (robust to multi-line output + notes).
			for _, line := range strings.Split(string(out), "\n") {
				s := strings.TrimSpace(line)
				if strings.HasPrefix(s, "http://") || strings.HasPrefix(s, "https://") {
					return s
				}
			}
		}
	}
	return fmt.Sprintf("http://127.0.0.1:%d", port)
}

// WaitHealthy polls sandbox /health until ready or timeout.
func WaitHealthy(port int, timeout time.Duration) bool {
	deadline := time.Now().Add(timeout)
	for time.Now().Before(deadline) {
		if HTTPHealth(port, 400) {
			return true
		}
		time.Sleep(100 * time.Millisecond)
	}
	return false
}

// TCPReachable checks upstream host:port connectivity.
func TCPReachable(host string, port int, timeoutMs int) bool {
	conn, err := net.DialTimeout("tcp", fmt.Sprintf("%s:%d", host, port), time.Duration(timeoutMs)*time.Millisecond)
	if err != nil {
		return false
	}
	conn.Close()
	return true
}

// HTTPHealth GETs /health on loopback.
// Strict status line parse (nth token == "200") to avoid false positives on reason phrases.
func HTTPHealth(port int, timeoutMs int) bool {
	conn, err := net.DialTimeout("tcp", fmt.Sprintf("127.0.0.1:%d", port), time.Duration(timeoutMs)*time.Millisecond)
	if err != nil {
		return false
	}
	defer conn.Close()
	_ = conn.SetDeadline(time.Now().Add(time.Duration(timeoutMs) * time.Millisecond))
	req := fmt.Sprintf("GET /health HTTP/1.0\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
	if _, err := conn.Write([]byte(req)); err != nil {
		return false
	}
	buf := make([]byte, 4096)
	n, _ := conn.Read(buf)
	head := string(buf[:n])
	if idx := strings.Index(head, "\r\n"); idx > 0 {
		statusLine := head[:idx]
		parts := strings.Fields(statusLine)
		if len(parts) >= 2 && parts[1] == "200" {
			return true
		}
	}
	return false
}

// HTTPPostStatus posts to local proxy with optional secret prefix.
func HTTPPostStatus(port int, secret, pathSuffix string, body []byte, timeoutMs int) (int, bool) {
	path := pathSuffix
	if secret != "" {
		path = "/" + secret + pathSuffix
	}
	client := &http.Client{Timeout: time.Duration(timeoutMs) * time.Millisecond}
	resp, err := client.Post(fmt.Sprintf("http://127.0.0.1:%d%s", port, path), "application/json", bytes.NewReader(body))
	if err != nil {
		return 0, false
	}
	defer resp.Body.Close()
	return resp.StatusCode, true
}

func ensureRuntimeAssets(dataDir, realDir string) error {
	if _, err := os.Stat(filepath.Join(dataDir, "bin")); err == nil {
		return nil
	}
	if err := os.MkdirAll(dataDir, 0o700); err != nil {
		return err
	}
	for _, asset := range []string{"bin", "conda", "runtime", "seed-assets"} {
		src := filepath.Join(realDir, asset)
		if _, err := os.Stat(src); err != nil {
			continue
		}
		dst := filepath.Join(dataDir, asset)
		if runtime.GOOS == "darwin" {
			if err := cloneAPFS(src, dst); err != nil {
				return err
			}
		} else {
			if err := copyDir(src, dst); err != nil {
				return err
			}
		}
	}
	return nil
}

func cloneAPFS(src, dst string) error {
	if _, err := os.Stat(dst); err == nil {
		return nil
	}
	cmd := exec.Command("cp", "-Rc", src, dst)
	return cmd.Run()
}

func copyDir(src, dst string) error {
	return exec.Command("cp", "-R", src, dst).Run()
}

func ensureSandboxKeychain(sandboxHome string) error {
	kc := filepath.Join(sandboxHome, "Library", "Keychains", "login.keychain-db")
	if _, err := os.Stat(kc); os.IsNotExist(err) {
		_ = os.MkdirAll(filepath.Dir(kc), 0o700)
		cmd := exec.Command("security", "create-keychain", "-p", "", kc)
		cmd.Env = append(os.Environ(), "HOME="+sandboxHome)
		_ = cmd.Run()
	}
	for _, args := range [][]string{
		{"list-keychains", "-d", "user", "-s", kc},
		{"default-keychain", "-d", "user", "-s", kc},
		{"unlock-keychain", "-p", "", kc},
		{"set-keychain-settings", kc},
	} {
		cmd := exec.Command("security", args...)
		cmd.Env = append(os.Environ(), "HOME="+sandboxHome)
		_ = cmd.Run()
	}
	return nil
}

func appendLog(path, line string) {
	if path == "" {
		return
	}
	_ = os.MkdirAll(filepath.Dir(path), 0o700)
	f, err := os.OpenFile(path, os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0o600)
	if err != nil {
		return
	}
	defer f.Close()
	ts := time.Now().Format("15:04:05")
	_, _ = io.WriteString(f, fmt.Sprintf("[%s] %s\n", ts, line))
}

func redactProxyURL(u string) string {
	if i := strings.Index(u, "://"); i >= 0 {
		rest := u[i+3:]
		if j := strings.Index(rest, "/"); j >= 0 {
			return u[:i+3] + rest[:j] + "/****"
		}
	}
	return u
}

// OpenBrowser opens a URL in the default browser (macOS).
func OpenBrowser(url string) error {
	if runtime.GOOS == "darwin" {
		return exec.Command("open", url).Run()
	}
	return exec.Command("xdg-open", url).Run()
}

// OpenOfficial launches the real Claude Science app without proxy env.
func OpenOfficial() error {
	// Strip ALL Anthropic/proxy env vars that could confuse official Science
	cleanEnv := filterEnv(os.Environ(), []string{
		"ANTHROPIC_BASE_URL", "ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN",
		"CSSWITCH_PROVIDER", "CSSWITCH_RELAY_BASE_URL", "CSSWITCH_RELAY_KEY",
		"DEEPSEEK_API_KEY", "DASHSCOPE_API_KEY", "MOONSHOT_API_KEY",
		"ZHIPU_API_KEY", "MINIMAX_API_KEY",
		"https_proxy", "HTTPS_PROXY", "http_proxy", "HTTP_PROXY",
	})
	app := "/Applications/Claude Science.app"
	var cmd *exec.Cmd
	if _, err := os.Stat(app); err == nil {
		cmd = exec.Command("open", app)
	} else if _, err := os.Stat("/Applications/Claude.app"); err == nil {
		cmd = exec.Command("open", "/Applications/Claude.app")
	} else {
		cmd = exec.Command("open", "-a", "Claude Science")
	}
	cmd.Env = cleanEnv
	return cmd.Run()
}

func filterEnv(env []string, remove []string) []string {
	rm := map[string]bool{}
	for _, k := range remove {
		rm[k] = true
	}
	out := make([]string, 0, len(env))
	for _, e := range env {
		if k, _, ok := strings.Cut(e, "="); ok && rm[k] {
			continue
		}
		out = append(out, e)
	}
	return out
}

// TailLog returns the last n bytes of a log file.
func TailLog(path string, maxBytes int) string {
	data, err := os.ReadFile(path)
	if err != nil {
		return ""
	}
	if len(data) > maxBytes {
		data = data[len(data)-maxBytes:]
	}
	return string(data)
}

// VerifyKeyViaProxy sends a minimal request through the local proxy.
func VerifyKeyViaProxy(port int, secret string) (bool, string) {
	body := []byte(`{"model":"claude-opus-4-8","max_tokens":1,"messages":[{"role":"user","content":"ping"}]}`)
	code, ok := HTTPPostStatus(port, secret, "/v1/messages", body, 15000)
	if !ok {
		return false, "no response from proxy"
	}
	switch code {
	case 200:
		return true, "upstream accepted key"
	case 401, 403:
		return false, fmt.Sprintf("upstream rejected key (HTTP %d)", code)
	default:
		return false, fmt.Sprintf("upstream returned HTTP %d", code)
	}
}

// MarshalState helper for logging.
func MarshalState(v any) string {
	b, _ := json.Marshal(v)
	return string(b)
}
