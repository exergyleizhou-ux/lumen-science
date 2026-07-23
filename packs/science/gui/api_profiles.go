package gui

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"strings"
	"time"

	sciconfig "lumen/internal/science/config"
	"lumen/internal/science/proxy"
	sciruntime "lumen/internal/science/runtime"
)

// RegisterProfiles mounts multi-profile + relay + updates routes.
func (a *API) RegisterProfiles(mux *http.ServeMux) {
	mux.HandleFunc("/api/templates", a.handleTemplates)
	mux.HandleFunc("/api/profiles", a.handleProfiles)
	mux.HandleFunc("/api/profiles/switch", a.handleProfileSwitch)
	mux.HandleFunc("/api/profiles/probe", a.handleProfileProbe)
	mux.HandleFunc("/api/relay/models", a.handleRelayModels)
	mux.HandleFunc("/api/check-update", a.handleCheckUpdate)
}

func (a *API) handleTemplates(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}
	tpls := sciconfig.ListTemplates()
	out := make([]map[string]any, 0, len(tpls))
	for _, t := range tpls {
		out = append(out, map[string]any{
			"id": t.ID, "name": t.Name, "category": t.Category,
			"api_format": t.APIFormat, "adapter": t.Adapter,
			"base_url": t.BaseURL, "base_url_editable": t.BaseURLEditable,
			"requires_model_override": t.RequiresModelOverride,
			"builtin_models":          t.BuiltinModels, "website_url": t.WebsiteURL,
			"icon": t.Icon, "icon_color": t.IconColor,
		})
	}
	writeJSON(w, http.StatusOK, map[string]any{"templates": out})
}

func (a *API) handleProfiles(w http.ResponseWriter, r *http.Request) {
	switch r.Method {
	case http.MethodGet:
		cfg, err := sciconfig.Load(a.sciDir)
		if err != nil {
			writeErr(w, http.StatusInternalServerError, err)
			return
		}
		list := make([]map[string]any, 0, len(cfg.Profiles))
		for _, p := range cfg.Profiles {
			list = append(list, profileJSON(p, p.ID == cfg.ActiveProfileID))
		}
		writeJSON(w, http.StatusOK, map[string]any{
			"profiles": list, "active_id": cfg.ActiveProfileID,
			"tooluse_shim": cfg.ToolUseShim, "schema_version": cfg.SchemaVersion,
		})
	case http.MethodPost:
		var body struct {
			Name       string `json:"name"`
			TemplateID string `json:"template_id"`
			BaseURL    string `json:"base_url"`
			APIKey     string `json:"api_key"`
			Model      string `json:"model"`
		}
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			writeErr(w, http.StatusBadRequest, err)
			return
		}
		if strings.TrimSpace(body.APIKey) == "" {
			writeErr(w, http.StatusBadRequest, fmt.Errorf("API key 不能为空"))
			return
		}
		tid := strings.TrimSpace(body.TemplateID)
		if tid == "" {
			tid = "custom"
		}
		tpl, ok := sciconfig.TemplateByID(tid)
		if !ok {
			writeErr(w, http.StatusBadRequest, fmt.Errorf("unknown template %q", tid))
			return
		}
		name := strings.TrimSpace(body.Name)
		if name == "" {
			name = tpl.Name
		}
		p := sciconfig.Profile{
			ID: sciconfig.NewProfileID(), Name: name, TemplateID: tid,
			APIKey: strings.TrimSpace(body.APIKey),
			Model:  strings.TrimSpace(body.Model), CreatedAt: time.Now().UnixMilli(),
			Icon: tpl.Icon, IconColor: tpl.IconColor,
		}
		if tpl.BaseURLEditable {
			p.BaseURL = strings.TrimSpace(body.BaseURL)
			if p.BaseURL == "" {
				p.BaseURL = tpl.BaseURL
			}
		} else {
			p.BaseURL = tpl.BaseURL
		}
		if ok, hint, err := sciruntime.ProbeProfile(p); err == nil {
			p.Verified = ok
			p.VerifiedHint = hint
			if !ok && (strings.Contains(hint, "401") || strings.Contains(hint, "403")) {
				writeErr(w, http.StatusBadRequest, fmt.Errorf("API key 无效或被拒：%s", hint))
				return
			}
			if !ok {
				p.VerifiedHint = "未校验，激活时再验"
			}
		}
		cfg, err := sciconfig.Update(a.sciDir, func(c *sciconfig.File) {
			c.Profiles = append(c.Profiles, p)
			if c.ActiveProfileID == "" {
				c.ActiveProfileID = p.ID
				c.Provider = tpl.Adapter
			}
		})
		if err != nil {
			writeErr(w, http.StatusBadRequest, err)
			return
		}
		writeJSON(w, http.StatusCreated, map[string]any{
			"profile": profileJSON(p, p.ID == cfg.ActiveProfileID),
		})
	case http.MethodPut:
		var body struct {
			ID      string `json:"id"`
			Name    string `json:"name"`
			APIKey  string `json:"api_key"`
			BaseURL string `json:"base_url"`
			Model   string `json:"model"`
			Notes   string `json:"notes"`
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
		p := cfg.ProfileByID(body.ID)
		if p == nil {
			writeErr(w, http.StatusNotFound, fmt.Errorf("profile not found"))
			return
		}
		candidate := *p
		if s := strings.TrimSpace(body.Name); s != "" {
			candidate.Name = s
		}
		if s := strings.TrimSpace(body.APIKey); s != "" {
			candidate.APIKey = s
		}
		if body.BaseURL != "" {
			tpl, _ := sciconfig.TemplateByID(candidate.TemplateID)
			if tpl.BaseURLEditable {
				candidate.BaseURL = strings.TrimSpace(body.BaseURL)
			}
		}
		if body.Model != "" {
			candidate.Model = strings.TrimSpace(body.Model)
		}
		if body.Notes != "" {
			candidate.Notes = strings.TrimSpace(body.Notes)
		}
		if strings.TrimSpace(body.APIKey) != "" {
			ok, hint, err := sciruntime.ProbeProfile(candidate)
			if err != nil {
				writeErr(w, http.StatusBadRequest, err)
				return
			}
			if !ok {
				if strings.Contains(hint, "401") || strings.Contains(hint, "403") {
					writeErr(w, http.StatusBadRequest, fmt.Errorf("API key 无效或被拒：%s", hint))
					return
				}
				candidate.Verified = false
				candidate.VerifiedHint = "未校验，激活时再验"
			} else {
				candidate.Verified = true
				candidate.VerifiedHint = hint
			}
		}
		_, err = sciconfig.Update(a.sciDir, func(c *sciconfig.File) {
			dst := c.ProfileByID(body.ID)
			if dst == nil {
				return
			}
			*dst = candidate
		})
		if err != nil {
			writeErr(w, http.StatusBadRequest, err)
			return
		}
		writeJSON(w, http.StatusOK, map[string]any{"ok": true, "verified": candidate.Verified, "hint": candidate.VerifiedHint})
	case http.MethodDelete:
		id := r.URL.Query().Get("id")
		if id == "" {
			writeErr(w, http.StatusBadRequest, fmt.Errorf("id required"))
			return
		}
		_, err := sciconfig.Update(a.sciDir, func(c *sciconfig.File) {
			var kept []sciconfig.Profile
			for _, p := range c.Profiles {
				if p.ID != id {
					kept = append(kept, p)
				}
			}
			c.Profiles = kept
			if c.ActiveProfileID == id {
				c.ActiveProfileID = ""
				if len(c.Profiles) > 0 {
					c.ActiveProfileID = c.Profiles[0].ID
				}
			}
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

func profileJSON(p sciconfig.Profile, active bool) map[string]any {
	return map[string]any{
		"id": p.ID, "name": p.Name, "template_id": p.TemplateID,
		"base_url": p.BaseURL, "model": p.Model,
		"key_masked": sciconfig.MaskKey(p.APIKey), "active": active,
		"created_at": p.CreatedAt, "notes": p.Notes,
		"icon": p.Icon, "icon_color": p.IconColor,
		"verified": p.Verified, "verified_hint": p.VerifiedHint,
	}
}

func (a *API) handleProfileSwitch(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}
	var body struct {
		ID string `json:"id"`
	}
	if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	result, err := a.withAction(func(mgr *sciruntime.Manager) (any, error) {
		return mgr.SwitchProfile(body.ID)
	})
	if err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	writeJSON(w, http.StatusOK, result)
}

func (a *API) handleProfileProbe(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}
	var body struct {
		ID string `json:"id"`
	}
	if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	ok, hint, err := sciruntime.ProbeProfileKey(a.sciDir, body.ID)
	if err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	writeJSON(w, http.StatusOK, map[string]any{"ok": ok, "hint": hint})
}

func (a *API) handleRelayModels(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}
	var body struct {
		BaseURL string `json:"base_url"`
		APIKey  string `json:"api_key"`
	}
	if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	ctx, cancel := context.WithTimeout(r.Context(), 20*time.Second)
	defer cancel()
	models, err := proxy.FetchRelayModels(ctx, body.BaseURL, body.APIKey)
	if err != nil {
		writeErr(w, http.StatusBadRequest, err)
		return
	}
	out := make([]map[string]any, 0, len(models))
	for _, m := range models {
		out = append(out, map[string]any{"id": m.ID, "display_name": m.DisplayName})
	}
	writeJSON(w, http.StatusOK, map[string]any{"models": out})
}

func (a *API) handleCheckUpdate(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}
	ctx, cancel := context.WithTimeout(r.Context(), 10*time.Second)
	defer cancel()
	req, err := http.NewRequestWithContext(ctx, http.MethodGet,
		"https://api.github.com/repos/exergyleizhou-ux/lumen/releases/latest", nil)
	if err != nil {
		writeErr(w, http.StatusInternalServerError, err)
		return
	}
	req.Header.Set("Accept", "application/vnd.github+json")
	req.Header.Set("User-Agent", "lumen-science-gui")
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		writeJSON(w, http.StatusOK, map[string]any{
			"ok": false, "error": err.Error(), "current": a.version,
		})
		return
	}
	defer resp.Body.Close()
	var rel struct {
		TagName string `json:"tag_name"`
		HTMLURL string `json:"html_url"`
		Name    string `json:"name"`
	}
	_ = json.NewDecoder(resp.Body).Decode(&rel)
	writeJSON(w, http.StatusOK, map[string]any{
		"ok": true, "current": a.version, "latest": rel.TagName,
		"url": rel.HTMLURL, "name": rel.Name,
		"update_available": rel.TagName != "" && rel.TagName != a.version,
	})
}
