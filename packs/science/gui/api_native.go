package gui

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"strings"
	"time"

	sciconfig "lumen/internal/science/config"
	"lumen/internal/science/mcp/oasis"
	"lumen/internal/science/native"
	"lumen/internal/science/native/brief"
)

const workbenchNativeTimeout = 50 * time.Second

func (a *API) RegisterNative(mux *http.ServeMux) {
	mux.HandleFunc("/api/oasis", a.handleOasis)
	mux.HandleFunc("/api/oasis/token", a.handleOasisToken)
	mux.HandleFunc("/api/oasis/probe", a.handleOasisProbe)
	mux.HandleFunc("/api/science-mode", a.handleScienceMode)
	mux.HandleFunc("/api/native/status", a.handleNativeStatus)
	mux.HandleFunc("/api/native/verify", a.handleNativeVerify)
	mux.HandleFunc("/api/native/brief", a.handleNativeBrief)
	mux.HandleFunc("/api/native/c2d/algorithms", a.handleNativeC2DAlgorithms)
}

func (a *API) handleOasis(w http.ResponseWriter, r *http.Request) {
	switch r.Method {
	case http.MethodGet:
		cfg, err := sciconfig.Load(a.sciDir)
		if err != nil {
			writeErr(w, http.StatusInternalServerError, err)
			return
		}
		writeJSON(w, http.StatusOK, map[string]any{
			"base_url":     cfg.OasisBaseURL(),
			"token_set":    cfg.OasisToken() != "",
			"token_masked": sciconfig.MaskKey(cfg.OasisToken()),
			"science_mode": cfg.ScienceMode,
			"auth_policy":  "combo_1_3",
		})
	default:
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
	}
}

func (a *API) handleOasisToken(w http.ResponseWriter, r *http.Request) {
	switch r.Method {
	case http.MethodPost:
		var body struct {
			AccessToken string `json:"access_token"`
		}
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			writeErr(w, http.StatusBadRequest, err)
			return
		}
		tok := strings.TrimSpace(body.AccessToken)
		if tok == "" {
			writeErr(w, http.StatusBadRequest, fmt.Errorf("access_token required"))
			return
		}
		_, err := sciconfig.Update(a.sciDir, func(c *sciconfig.File) {
			c.Oasis.APIToken = tok
			if c.Oasis.BaseURL == "" {
				c.Oasis.BaseURL = "https://demo.oasisdata2026.xyz"
			}
		})
		if err != nil {
			writeErr(w, http.StatusBadRequest, err)
			return
		}
		writeJSON(w, http.StatusOK, map[string]any{
			"ok":           true,
			"token_masked": sciconfig.MaskKey(tok),
		})
	case http.MethodDelete:
		_, err := sciconfig.Update(a.sciDir, func(c *sciconfig.File) {
			c.Oasis.APIToken = ""
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

func (a *API) handleNativeStatus(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}
	ctx, cancel := context.WithTimeout(r.Context(), workbenchNativeTimeout)
	defer cancel()
	_ = ctx
	mgr, err := native.NewManagerFromDir(a.sciDir)
	if err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return
	}
	defer mgr.Close()
	if _, err := mgr.ConnectAll(); err != nil {
		writeJSON(w, http.StatusOK, map[string]any{
			"connected": false,
			"error":     err.Error(),
			"fleet":     native.DefaultFleet(),
		})
		return
	}
	writeJSON(w, http.StatusOK, map[string]any{
		"connected": true,
		"fleet":     mgr.Status(),
	})
}

func (a *API) handleNativeVerify(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}
	ctx, cancel := context.WithTimeout(r.Context(), workbenchNativeTimeout)
	defer cancel()
	results, err := native.VerifyLiveFromDir(ctx, a.sciDir, native.DefaultLiveChecks)
	if err != nil {
		writeErr(w, http.StatusBadGateway, err)
		return
	}
	ok, _ := native.VerifyLiveReport(results)
	writeJSON(w, http.StatusOK, map[string]any{"ok": ok, "results": results})
}

func (a *API) handleNativeBrief(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}
	var body brief.Request
	if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	ctx, cancel := context.WithTimeout(r.Context(), briefPipelineTimeout())
	defer cancel()
	res, err := brief.Generate(ctx, a.sciDir, body)
	if err != nil {
		writeErr(w, http.StatusBadGateway, err)
		return
	}
	writeJSON(w, http.StatusOK, res)
}

func briefPipelineTimeout() time.Duration {
	return 60 * time.Second
}

func (a *API) handleNativeC2DAlgorithms(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}
	var body struct {
		DatasetID string `json:"dataset_id"`
	}
	if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	cfg, err := sciconfig.Load(a.sciDir)
	if err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return
	}
	if err := native.CheckAuth("list_algorithms", cfg.OasisToken()); err != nil {
		writeJSON(w, http.StatusOK, map[string]any{"ok": false, "error": err.Error(), "need_login": true})
		return
	}
	ctx, cancel := context.WithTimeout(r.Context(), workbenchNativeTimeout)
	defer cancel()
	mgr, err := native.NewManagerFromDir(a.sciDir)
	if err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return
	}
	defer mgr.Close()
	if err := mgr.Connect("c2d"); err != nil {
		writeErr(w, http.StatusBadGateway, err)
		return
	}
	text, err := mgr.CallToolCtx(ctx, "c2d", "list_algorithms", map[string]any{
		"dataset_id": strings.TrimSpace(body.DatasetID),
	})
	if err != nil {
		writeJSON(w, http.StatusOK, map[string]any{"ok": false, "error": err.Error()})
		return
	}
	var payload map[string]any
	_ = json.Unmarshal([]byte(text), &payload)
	writeJSON(w, http.StatusOK, map[string]any{"ok": true, "data": payload})
}

func (a *API) handleOasisProbe(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}
	var body struct {
		DatasetID string `json:"dataset_id"`
	}
	if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	cfg, err := sciconfig.Load(a.sciDir)
	if err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return
	}
	if err := oasis.CheckAuth("preview_schema", cfg.OasisToken()); err != nil {
		writeJSON(w, http.StatusOK, map[string]any{"ok": false, "error": err.Error(), "need_login": true})
		return
	}
	client := oasis.NewClient(cfg.OasisBaseURL(), cfg.OasisToken())
	row, err := client.PreviewSchema(body.DatasetID)
	if err != nil {
		writeErr(w, http.StatusBadGateway, err)
		return
	}
	writeJSON(w, http.StatusOK, map[string]any{"ok": true, "preview": row})
}

// handleScienceMode reads or sets science_mode in config.json.
func (a *API) handleScienceMode(w http.ResponseWriter, r *http.Request) {
	switch r.Method {
	case http.MethodGet:
		cfg, err := sciconfig.Load(a.sciDir)
		if err != nil {
			writeErr(w, http.StatusInternalServerError, err)
			return
		}
		writeJSON(w, http.StatusOK, map[string]any{
			"science_mode": cfg.ScienceMode,
			"valid_values": []string{"hybrid", "native", "bridge"},
		})
	case http.MethodPut:
		var body struct {
			Mode string `json:"mode"`
		}
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			writeErr(w, http.StatusBadRequest, err)
			return
		}
		switch body.Mode {
		case "hybrid", "native", "bridge":
		default:
			writeErr(w, http.StatusBadRequest, fmt.Errorf("science_mode must be hybrid, native, or bridge (got %q)", body.Mode))
			return
		}
		_, err := sciconfig.Update(a.sciDir, func(c *sciconfig.File) {
			c.ScienceMode = body.Mode
		})
		if err != nil {
			writeErr(w, http.StatusBadRequest, err)
			return
		}
		writeJSON(w, http.StatusOK, map[string]any{
			"ok":           true,
			"science_mode": body.Mode,
		})
	default:
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
	}
}
