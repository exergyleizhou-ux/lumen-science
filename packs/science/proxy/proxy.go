package proxy

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"log"
	"net"
	"net/http"
	"os"
	"strings"
	"time"
)

// Config controls the Science translation proxy.
type Config struct {
	Provider    ProviderSpec
	APIKey      string
	Addr        string // listen address, default 127.0.0.1:18991
	AuthSecret  string // path prefix secret; empty = disabled
	UpstreamURL string // optional override
	LogPath     string
	Logger      func(string)
	CacheBoost  bool            // inject cache_control on system/tools for DeepSeek prefix cache
	ToolUseShim ToolUseShimMode // off (default) | detect | rewrite — DSML leak recovery
}

// Server is an Anthropic-compatible HTTP proxy for Claude Science.
type Server struct {
	cfg        Config
	spec       ProviderSpec
	upstream   *Upstream
	logger     *fileLogger
	httpSrv    *http.Server
	CacheStats CacheStats
}

// New builds a proxy server from cfg. Provider and APIKey are required.
func New(cfg Config) (*Server, error) {
	if cfg.APIKey == "" {
		return nil, fmt.Errorf("science proxy: API key is required")
	}
	spec := cfg.Provider
	if cfg.UpstreamURL != "" {
		spec.URL = cfg.UpstreamURL
	}
	if cfg.Addr == "" {
		cfg.Addr = "127.0.0.1:18991"
	}
	host, _, err := net.SplitHostPort(cfg.Addr)
	if err != nil {
		return nil, fmt.Errorf("science proxy: invalid addr %q: %w", cfg.Addr, err)
	}
	if host != "127.0.0.1" && host != "localhost" && host != "::1" {
		return nil, fmt.Errorf("science proxy: only loopback bind is allowed (got %q)", host)
	}

	var fl *fileLogger
	if cfg.LogPath != "" {
		fl, err = newFileLogger(cfg.LogPath)
		if err != nil {
			return nil, err
		}
	}

	logFn := cfg.Logger
	if logFn == nil {
		logFn = func(msg string) { log.Println(msg) }
	}
	if fl != nil {
		prev := logFn
		logFn = func(msg string) {
			prev(msg)
			fl.write(msg)
		}
	}

	return &Server{
		cfg:  cfg,
		spec: spec,
		upstream: &Upstream{
			Log: logFn,
		},
		logger: fl,
	}, nil
}

// ListenAndServe blocks until the server stops or errors.
func (s *Server) ListenAndServe() error {
	s.log(fmt.Sprintf("lumen science proxy listening on %s provider=%s upstream=%s",
		s.cfg.Addr, s.spec.Name, s.spec.URL))

	handler := http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method == http.MethodConnect {
			s.handleConnect(w, r)
			return
		}
		s.handleHTTP(w, r)
	})

	srv := &http.Server{
		Addr:              s.cfg.Addr,
		Handler:           handler,
		ReadHeaderTimeout: 30 * time.Second,
	}
	s.httpSrv = srv

	var lastErr error
	for attempt := 0; attempt < 10; attempt++ {
		ln, err := net.Listen("tcp", s.cfg.Addr)
		if err != nil {
			lastErr = err
			time.Sleep(300 * time.Millisecond)
			continue
		}
		return srv.Serve(ln)
	}
	return fmt.Errorf("science proxy: cannot bind %s: %w", s.cfg.Addr, lastErr)
}

// Shutdown gracefully stops the proxy server.
func (s *Server) Shutdown(ctx context.Context) error {
	if s.httpSrv == nil {
		return nil
	}
	return s.httpSrv.Shutdown(ctx)
}

func (s *Server) log(msg string) {
	if s.cfg.Logger != nil {
		s.cfg.Logger(msg)
		return
	}
	log.Println(msg)
}

func (s *Server) handleHTTP(w http.ResponseWriter, r *http.Request) {
	if !s.authOK(w, r) {
		return
	}
	switch {
	case r.Method == http.MethodGet && strings.HasPrefix(r.URL.Path, "/v1/models"):
		s.handleModels(w)
	case r.Method == http.MethodGet && strings.HasPrefix(r.URL.Path, "/health"):
		s.handleHealth(w)
	case r.Method == http.MethodPost && strings.HasPrefix(r.URL.Path, "/v1/messages"):
		s.handleMessages(w, r)
	default:
		s.sendJSON(w, http.StatusNotFound, errorBody("not_found_error", r.URL.Path))
	}
}

func (s *Server) authOK(w http.ResponseWriter, r *http.Request) bool {
	// Health check is always public
	if r.URL.Path == "/health" || r.URL.Path == "/" {
		return true
	}
	secret := s.cfg.AuthSecret
	if secret == "" {
		return true
	}
	prefix := "/" + secret
	if r.URL.Path == prefix || strings.HasPrefix(r.URL.Path, prefix+"/") {
		r.URL.Path = strings.TrimPrefix(r.URL.Path, prefix)
		if r.URL.Path == "" {
			r.URL.Path = "/"
		}
		return true
	}
	w.Header().Set("Connection", "close")
	s.sendJSON(w, http.StatusForbidden, errorBody("permission_error", "forbidden"))
	return false
}

func (s *Server) handleHealth(w http.ResponseWriter) {
	// Identity fields mirror CSSwitch 0.4 Rust gateway /health so orchestrators
	// can tell which data plane is live (gateway=go | rust) without guessing.
	shim := string(s.cfg.ToolUseShim)
	if shim == "" {
		shim = string(ResolveToolUseShim(s.spec, ""))
	}
	out := map[string]any{
		"status":          "ok",
		"gateway":         "go",
		"csswitch_parity": CSSwitchParity,
		"provider":        s.spec.Name,
		"mode":            string(s.spec.Mode),
		"shim":            shim,
	}
	if s.spec.ForceModelOverride {
		out["force_model"] = s.spec.ForceModel
	}
	if s.spec.ThinkingPolicy != "" {
		out["thinking_policy"] = s.spec.ThinkingPolicy
	}
	if rules := MatchCapabilityRules(s.spec); len(rules) > 0 {
		out["capability_rules"] = rules
	}
	for k, v := range s.CacheStats.Snapshot() {
		out["cache_"+k] = v
	}
	s.sendJSON(w, http.StatusOK, out)
}

func (s *Server) handleModels(w http.ResponseWriter) {
	models := s.spec.Models
	// Force-shell (CSSwitch contract): one claude-opus-4-8 shell whose
	// display_name is the real upstream model id (Science shows real name).
	if s.spec.ForceModelOverride && s.spec.ForceModel != "" {
		models = []ModelEntry{{ID: "claude-opus-4-8", DisplayName: s.spec.ForceModel}}
	}
	data := make([]map[string]any, 0, len(models))
	for _, m := range models {
		data = append(data, map[string]any{
			"type":           "model",
			"id":             m.ID,
			"display_name":   m.DisplayName,
			"supports_tools": nil,
			"created_at":     "2026-01-01T00:00:00Z",
		})
	}
	var firstID, lastID string
	if len(data) > 0 {
		firstID, _ = data[0]["id"].(string)
		lastID, _ = data[len(data)-1]["id"].(string)
	}
	ids := make([]string, len(models))
	for i, m := range models {
		ids[i] = m.ID
	}
	s.log(fmt.Sprintf("GET /v1/models -> %s: %s", s.spec.Name, strings.Join(ids, ", ")))
	s.sendJSON(w, http.StatusOK, map[string]any{
		"data":     data,
		"has_more": false,
		"first_id": firstID,
		"last_id":  lastID,
	})
}

func (s *Server) handleMessages(w http.ResponseWriter, r *http.Request) {
	// Defensive: reject malformed Content-Length (from CSSwitch)
	if r.ContentLength < 0 {
		s.sendJSON(w, http.StatusBadRequest, errorBody("invalid_request_error", "invalid Content-Length"))
		return
	}
	r.Body = http.MaxBytesReader(w, r.Body, 32<<20)
	raw, err := io.ReadAll(r.Body)
	if err != nil {
		s.sendJSON(w, http.StatusBadRequest, errorBody("invalid_request_error", "invalid body"))
		return
	}
	var areq map[string]any
	if err := json.Unmarshal(raw, &areq); err != nil {
		s.sendJSON(w, http.StatusBadRequest, errorBody("invalid_request_error", err.Error()))
		return
	}
	msgs, ok := areq["messages"].([]any)
	if !ok {
		s.sendJSON(w, http.StatusBadRequest, errorBody("invalid_request_error",
			"request body must be a JSON object with a 'messages' array"))
		return
	}
	_ = msgs // validated non-nil array type above

	switch s.spec.Mode {
	case ModeAnthropic:
		s.handleAnthropic(w, r.Context(), raw, areq)
	case ModeResponses:
		s.handleResponses(w, r.Context(), areq)
	default:
		s.handleOpenAI(w, r.Context(), areq)
	}
}

func (s *Server) handleAnthropic(w http.ResponseWriter, ctx context.Context, raw []byte, areq map[string]any) {
	src, _ := areq["model"].(string)
	payload, err := PatchAnthropicBodyRaw(raw, s.spec, s.cfg.CacheBoost)
	if err != nil {
		s.sendJSON(w, http.StatusBadRequest, errorBody("invalid_request_error", err.Error()))
		return
	}
	var patched map[string]any
	_ = json.Unmarshal(payload, &patched)
	target, _ := patched["model"].(string)
	stream, _ := patched["stream"].(bool)
	tools, _ := patched["tools"].([]any)
	s.log(fmt.Sprintf("POST /v1/messages %s->%s stream=%v tools=%d (raw-preserve, %s)",
		src, target, stream, len(tools), s.spec.Name))

	headers := upstreamHeaders(s.spec, s.cfg.APIKey)
	headers["content-type"] = "application/json"

	shimMode := s.cfg.ToolUseShim
	if shimMode == "" {
		shimMode = ResolveToolUseShim(s.spec, "")
	}
	knownTools := ToolsSchemaFromRequest(areq)
	shimOn := len(knownTools) > 0 && (shimMode == ShimDetect || shimMode == ShimRewrite)

	if stream {
		resp, first, err := s.upstream.OpenStream(ctx, s.spec.URL, payload, headers)
		if err != nil {
			s.writeUpstreamError(w, err, false)
			return
		}
		defer resp.Body.Close()
		w.Header().Set("Content-Type", resp.Header.Get("Content-Type"))
		w.Header().Set("Cache-Control", "no-cache")
		w.WriteHeader(http.StatusOK)
		flusher, _ := w.(http.Flusher)
		var rw *DsmlStreamRewriter
		var det *DsmlDetector
		if shimOn && shimMode == ShimRewrite {
			rw = NewDsmlStreamRewriter(knownTools, s.cfg.AuthSecret)
		}
		if shimOn && shimMode == ShimDetect {
			det = NewDsmlDetector()
		}
		writeChunk := func(chunk []byte) {
			if len(chunk) == 0 {
				return
			}
			s.CacheStats.ScanSSEChunk(chunk)
			var out []byte
			if rw != nil {
				out = rw.Feed(chunk)
			} else if det != nil {
				det.Feed(chunk)
				out = chunk
			} else {
				out = chunk
			}
			if len(out) > 0 {
				if _, werr := w.Write(out); werr != nil {
					s.log(fmt.Sprintf("  !! stream write error: %v", werr))
					return
				}
				if flusher != nil {
					flusher.Flush()
				}
			}
		}
		if len(first) > 0 {
			writeChunk(first)
		}
		buf := make([]byte, 4096)
		for {
			n, readErr := resp.Body.Read(buf)
			if n > 0 {
				writeChunk(buf[:n])
			}
			if readErr != nil {
				if readErr != io.EOF {
					s.log(fmt.Sprintf("  !! stream interrupted: %v", readErr))
					s.writeSSEErrorAndTerminate(w, readErr.Error())
				} else if rw != nil {
					if tail := rw.Finalize(); len(tail) > 0 {
						s.CacheStats.ScanSSEChunk(tail)
						_, _ = w.Write(tail)
						if flusher != nil {
							flusher.Flush()
						}
					}
				}
				break
			}
		}
		if shimOn && shimMode == ShimDetect && det != nil && det.found {
			s.log("  <- DSML detect: leak marker seen in stream")
		}
		s.log(fmt.Sprintf("  <- %s stream OK cache_hit=%d%% shim=%s", s.spec.Name, s.CacheStats.Snapshot()["last_hit_rate_pct"], shimMode))
		return
	}

	data, ct, _, err := s.upstream.PostJSON(ctx, s.spec.URL, payload, headers)
	if err != nil {
		s.writeUpstreamError(w, err, false)
		return
	}
	if shimOn && shimMode == ShimRewrite {
		rewritten := RewriteNonstreamBody(data, knownTools, s.cfg.AuthSecret)
		if len(rewritten) != len(data) || string(rewritten) != string(data) {
			s.log("  <- DSML rewrite: recovered tool_use from text leak")
			data = rewritten
		}
	} else if shimOn && shimMode == ShimDetect {
		det := NewDsmlDetector()
		det.Feed(data)
		if det.found {
			s.log("  <- DSML detect: leak marker seen in non-stream body")
		}
	}
	var respObj map[string]any
	if json.Unmarshal(data, &respObj) == nil {
		if usage, ok := respObj["usage"].(map[string]any); ok {
			s.CacheStats.RecordFromJSONUsage(usage)
		}
	}
	if ct == "" {
		ct = "application/json"
	}
	w.Header().Set("Content-Type", ct)
	w.WriteHeader(http.StatusOK)
	_, _ = w.Write(data)
	s.log(fmt.Sprintf("  <- %s non-stream OK cache_hit=%d%% shim=%s", s.spec.Name, s.CacheStats.Snapshot()["last_hit_rate_pct"], shimMode))
}

func (s *Server) handleOpenAI(w http.ResponseWriter, ctx context.Context, areq map[string]any) {
	modelID, _ := areq["model"].(string)
	stream, _ := areq["stream"].(bool)
	oreq := AnthropicToOpenAI(s.spec, areq)
	toolCount := 0
	if tools, ok := oreq["tools"].([]any); ok {
		toolCount = len(tools)
	}
	s.log(fmt.Sprintf("POST /v1/messages %s->%s stream=%v tools=%d (inbound auth stripped, %s)",
		modelID, oreq["model"], stream, toolCount, s.spec.Name))

	headers := map[string]string{
		"Authorization": "Bearer " + s.cfg.APIKey,
		"Content-Type":  "application/json",
	}
	payload, _ := json.Marshal(oreq)
	raw, _, _, err := s.upstream.PostJSON(ctx, s.spec.URL, payload, headers)
	if err != nil {
		s.writeUpstreamError(w, err, false)
		return
	}
	var oresp map[string]any
	if err := json.Unmarshal(raw, &oresp); err != nil {
		s.sendJSON(w, http.StatusBadGateway, errorBody("api_error", err.Error()))
		return
	}
	aresp := OpenAIToAnthropic(oresp, modelID)
	if stream {
		s.replayAsSSE(w, aresp)
	} else {
		s.sendJSON(w, http.StatusOK, aresp)
	}
	blockCount := 0
	if blocks, ok := aresp["content"].([]map[string]any); ok {
		blockCount = len(blocks)
	}
	s.log(fmt.Sprintf("  <- %s OK (blocks=%d stop=%v)", s.spec.Name, blockCount, aresp["stop_reason"]))
}

func (s *Server) writeUpstreamError(w http.ResponseWriter, err error, headersSent bool) {
	if hs, ok := err.(*HTTPStatusError); ok {
		detail := string(hs.Body)
		if len(detail) > 400 {
			detail = detail[:400]
		}
		s.log(fmt.Sprintf("  !! upstream HTTP %d: %s", hs.Code, detail))
		code := hs.Code
		if code != 401 && code != 403 && code != 429 {
			code = http.StatusBadGateway
		}
		if !headersSent {
			s.sendJSON(w, code, errorBody("api_error", fmt.Sprintf("upstream %d: %s", hs.Code, detail)))
		}
		return
	}
	s.log(fmt.Sprintf("  !! proxy error: %v", err))
	if !headersSent {
		s.sendJSON(w, http.StatusBadGateway, errorBody("api_error", err.Error()))
	}
}

func (s *Server) sendJSON(w http.ResponseWriter, code int, obj any) {
	body, _ := json.Marshal(obj)
	w.Header().Set("Content-Type", "application/json")
	w.Header().Set("Content-Length", fmt.Sprintf("%d", len(body)))
	w.WriteHeader(code)
	_, _ = w.Write(body)
}

func errorBody(typ, msg string) map[string]any {
	return map[string]any{
		"type": "error",
		"error": map[string]any{
			"type":    typ,
			"message": msg,
		},
	}
}

// GenerateAuthSecret returns a random path-secret token.
func GenerateAuthSecret() (string, error) {
	b := make([]byte, 16)
	if _, err := rand.Read(b); err != nil {
		return "", err
	}
	return hex.EncodeToString(b), nil
}

// ResolveAPIKey picks an API key from the explicit value, env var, or provider table.
func ResolveAPIKey(providerName, explicit string) (string, error) {
	if explicit != "" {
		return explicit, nil
	}
	spec, ok := LookupProvider(providerName)
	if !ok {
		return "", fmt.Errorf("unknown provider %q", providerName)
	}
	if v := strings.TrimSpace(osGetenv(spec.KeyEnv)); v != "" {
		return v, nil
	}
	return "", fmt.Errorf("missing API key: set %s or pass --api-key", spec.KeyEnv)
}

// osGetenv is a test seam.
var osGetenv = func(key string) string {
	return strings.TrimSpace(os.Getenv(key))
}
