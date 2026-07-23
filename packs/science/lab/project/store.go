package project

import (
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"time"
)

var slugRe = regexp.MustCompile(`^[a-z0-9][a-z0-9-]{0,63}$`)

// Store manages lab projects on disk.
type Store struct {
	sciDir string
	root   string
}

// NewStore creates a project store at ~/.lumen/science/lab/projects.
func NewStore(sciDir string) *Store {
	return &Store{sciDir: sciDir, root: filepath.Join(sciDir, "lab", "projects")}
}

// SciDir returns the science config root.
func (s *Store) SciDir() string { return s.sciDir }

// Root returns the projects root directory.
func (s *Store) Root() string { return s.root }

func (s *Store) projectDir(slug string) string {
	return filepath.Join(s.root, slug)
}

// SlugFromTitle converts a title to a URL-safe slug (public for tests).
func SlugFromTitle(title string) string { return slugFromTitle(title) }

func slugFromTitle(title string) string {
	t := strings.ToLower(strings.TrimSpace(title))
	var b strings.Builder
	prevDash := false
	for _, r := range t {
		switch {
		case r >= 'a' && r <= 'z', r >= '0' && r <= '9':
			b.WriteRune(r)
			prevDash = false
		case r == ' ' || r == '_' || r == '-':
			if !prevDash && b.Len() > 0 {
				b.WriteByte('-')
				prevDash = true
			}
		default:
			// skip CJK/other — uniqueness comes from Create's random suffix
		}
	}
	out := strings.Trim(b.String(), "-")
	if out == "" {
		out = "project"
	}
	if len(out) > 48 {
		out = out[:48]
	}
	return out
}

func newID(prefix string) string {
	b := make([]byte, 8)
	_, _ = rand.Read(b)
	return prefix + "_" + hex.EncodeToString(b)
}

// List returns all projects sorted by updated time descending.
func (s *Store) List() ([]Project, error) {
	if err := os.MkdirAll(s.root, 0o700); err != nil {
		return nil, err
	}
	entries, err := os.ReadDir(s.root)
	if err != nil {
		return nil, err
	}
	var out []Project
	for _, e := range entries {
		if !e.IsDir() {
			continue
		}
		p, err := s.load(filepath.Join(s.root, e.Name()))
		if err != nil {
			continue
		}
		out = append(out, p)
	}
	// simple sort by UpdatedAt
	for i := 1; i < len(out); i++ {
		for j := i; j > 0 && out[j].UpdatedAt.Before(out[j-1].UpdatedAt); j-- {
			out[j], out[j-1] = out[j-1], out[j]
		}
	}
	return out, nil
}

func (s *Store) load(dir string) (Project, error) {
	data, err := os.ReadFile(filepath.Join(dir, "project.json"))
	if err != nil {
		return Project{}, err
	}
	var p Project
	if err := json.Unmarshal(data, &p); err != nil {
		return Project{}, err
	}
	return p, nil
}

// Get loads a project by slug.
func (s *Store) Get(slug string) (Project, error) {
	if !slugRe.MatchString(slug) {
		return Project{}, fmt.Errorf("invalid slug %q", slug)
	}
	return s.load(s.projectDir(slug))
}

// Create adds a new project with workspace scaffolding.
func (s *Store) Create(title, template string) (Project, error) {
	title = strings.TrimSpace(title)
	if title == "" {
		return Project{}, fmt.Errorf("title is required")
	}
	base := slugFromTitle(title)
	// Always uniquify: Chinese-only titles collapse to "project"; collisions used to overwrite.
	suffix := newID("")
	if len(suffix) > 8 {
		suffix = suffix[len(suffix)-8:] // last 8 hex of proj_xxxxxxxx or raw
	}
	suffix = strings.TrimPrefix(suffix, "_")
	if len(suffix) > 8 {
		suffix = suffix[:8]
	}
	slug := base + "-" + suffix
	if len(slug) > 64 {
		slug = slug[:64]
	}
	if err := os.MkdirAll(s.root, 0o700); err != nil {
		return Project{}, err
	}
	// Rare collision loop
	dir := s.projectDir(slug)
	for n := 0; n < 5; n++ {
		if _, err := os.Stat(dir); os.IsNotExist(err) {
			break
		}
		slug = base + "-" + newID("x")[2:10]
		if len(slug) > 64 {
			slug = slug[:64]
		}
		dir = s.projectDir(slug)
	}
	now := time.Now().UTC()
	p := Project{
		ID:           newID("proj"),
		Slug:         slug,
		Title:        title,
		Template:     template,
		CreatedAt:    now,
		UpdatedAt:    now,
		WorkspaceRel: "workspace",
	}
	if err := os.MkdirAll(dir, 0o700); err != nil {
		return Project{}, err
	}
	for _, sub := range []string{"workspace", "workspace/data", "workspace/figures", "workspace/reports", "workspace/notebooks", "sessions", ".lumen/skills"} {
		if err := os.MkdirAll(filepath.Join(dir, sub), 0o700); err != nil {
			return Project{}, err
		}
	}
	provPath := filepath.Join(dir, "provenance.jsonl")
	if _, err := os.Stat(provPath); os.IsNotExist(err) {
		if err := os.WriteFile(provPath, nil, 0o600); err != nil {
			return Project{}, err
		}
	}
	if err := s.save(p); err != nil {
		return Project{}, err
	}
	ws := filepath.Join(dir, "workspace")
	if err := ApplySeedTemplate(s.sciDir, template, ws); err != nil {
		return Project{}, err
	}
	return p, nil
}

// ProjectDir returns the absolute project directory for a slug.
func (s *Store) ProjectDir(slug string) (string, error) {
	if !slugRe.MatchString(slug) {
		return "", fmt.Errorf("invalid slug %q", slug)
	}
	return s.projectDir(slug), nil
}


// Rename updates a project's display title (slug unchanged).
func (s *Store) Rename(slug, title string) (Project, error) {
	title = strings.TrimSpace(title)
	if title == "" {
		return Project{}, fmt.Errorf("title required")
	}
	if len(title) > 200 {
		title = title[:200]
	}
	p, err := s.Get(slug)
	if err != nil {
		return Project{}, err
	}
	p.Title = title
	p.UpdatedAt = time.Now().UTC()
	if err := s.save(p); err != nil {
		return Project{}, err
	}
	return p, nil
}

// Delete removes a project and all its data.
func (s *Store) Delete(slug string) error {
	if !slugRe.MatchString(slug) {
		return fmt.Errorf("invalid slug %q", slug)
	}
	return os.RemoveAll(s.projectDir(slug))
}

func (s *Store) save(p Project) error {
	dir := s.projectDir(p.Slug)
	data, err := json.MarshalIndent(p, "", "  ")
	if err != nil {
		return err
	}
	return os.WriteFile(filepath.Join(dir, "project.json"), append(data, '\n'), 0o600)
}

// WorkspacePath returns the absolute workspace directory for a project slug.
func (s *Store) WorkspacePath(slug string) (string, error) {
	if !slugRe.MatchString(slug) {
		return "", fmt.Errorf("invalid slug %q", slug)
	}
	return filepath.Join(s.projectDir(slug), "workspace"), nil
}

// ProvenancePath returns provenance.jsonl for a project.
func (s *Store) ProvenancePath(slug string) (string, error) {
	if !slugRe.MatchString(slug) {
		return "", fmt.Errorf("invalid slug %q", slug)
	}
	return filepath.Join(s.projectDir(slug), "provenance.jsonl"), nil
}

// CreateSession adds a session file under the project.
func (s *Store) CreateSession(slug, title string) (Session, error) {
	p, err := s.Get(slug)
	if err != nil {
		return Session{}, err
	}
	now := time.Now().UTC()
	if title == "" {
		title = "会话 " + now.Format("15:04")
	}
	sess := Session{
		ID:        newID("sess"),
		ProjectID: p.ID,
		Title:     title,
		CreatedAt: now,
		UpdatedAt: now,
	}
	dir := filepath.Join(s.projectDir(slug), "sessions")
	if err := os.MkdirAll(dir, 0o700); err != nil {
		return Session{}, err
	}
	data, err := json.MarshalIndent(sess, "", "  ")
	if err != nil {
		return Session{}, err
	}
	if err := os.WriteFile(filepath.Join(dir, sess.ID+".json"), append(data, '\n'), 0o600); err != nil {
		return Session{}, err
	}
	p.ActiveSession = sess.ID
	p.UpdatedAt = now
	_ = s.save(p)
	return sess, nil
}

// ListSessions returns sessions for a project.
func (s *Store) ListSessions(slug string) ([]Session, error) {
	dir := filepath.Join(s.projectDir(slug), "sessions")
	entries, err := os.ReadDir(dir)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, err
	}
	var out []Session
	for _, e := range entries {
		if e.IsDir() || !strings.HasSuffix(e.Name(), ".json") {
			continue
		}
		data, err := os.ReadFile(filepath.Join(dir, e.Name()))
		if err != nil {
			continue
		}
		var sess Session
		if json.Unmarshal(data, &sess) == nil {
			// List responses omit bulky turn bodies; GetSession returns full turns.
			listCopy := sess
			listCopy.TurnCount = len(sess.Turns)
			listCopy.Turns = nil
			out = append(out, listCopy)
		}
	}
	// newest first
	for i := 1; i < len(out); i++ {
		for j := i; j > 0 && out[j].UpdatedAt.After(out[j-1].UpdatedAt); j-- {
			out[j], out[j-1] = out[j-1], out[j]
		}
	}
	return out, nil
}

// ForkSession copies turns from an existing session into a new session.
func (s *Store) ForkSession(slug, sessionID, title string) (Session, error) {
	src, err := s.GetSession(slug, sessionID)
	if err != nil {
		return Session{}, err
	}
	if strings.TrimSpace(title) == "" {
		title = "分支 · " + src.Title
	}
	if len(title) > 200 {
		title = title[:200]
	}
	dst, err := s.CreateSession(slug, title)
	if err != nil {
		return Session{}, err
	}
	// re-load and attach turns (CreateSession returns empty turns)
	dst, err = s.GetSession(slug, dst.ID)
	if err != nil {
		return Session{}, err
	}
	// deep-ish copy turns without sharing slice
	if len(src.Turns) > 0 {
		dst.Turns = make([]Turn, len(src.Turns))
		copy(dst.Turns, src.Turns)
		dst.TurnCount = len(dst.Turns)
		dst.UpdatedAt = time.Now().UTC()
		if err := s.saveSession(slug, dst); err != nil {
			return Session{}, err
		}
	}
	out := dst
	out.TurnCount = len(dst.Turns)
	out.Turns = nil // list-shaped for clients that only need meta; full via GetSession
	return out, nil
}

// RenameSession updates a session title.
func (s *Store) RenameSession(slug, sessionID, title string) (Session, error) {
	title = strings.TrimSpace(title)
	if title == "" {
		return Session{}, fmt.Errorf("title required")
	}
	if len(title) > 200 {
		title = title[:200]
	}
	sess, err := s.GetSession(slug, sessionID)
	if err != nil {
		return Session{}, err
	}
	sess.Title = title
	sess.UpdatedAt = time.Now().UTC()
	if err := s.saveSession(slug, sess); err != nil {
		return Session{}, err
	}
	// list-shaped response (no bulky turns)
	out := sess
	out.TurnCount = len(sess.Turns)
	out.Turns = nil
	return out, nil
}

// DeleteSession removes a session JSON file.
func (s *Store) DeleteSession(slug, sessionID string) error {
	if !slugRe.MatchString(slug) {
		return fmt.Errorf("invalid slug %q", slug)
	}
	if sessionID == "" || strings.Contains(sessionID, "/") || strings.Contains(sessionID, "..") {
		return fmt.Errorf("invalid session id")
	}
	path := filepath.Join(s.projectDir(slug), "sessions", sessionID+".json")
	if err := os.Remove(path); err != nil {
		return err
	}
	// clear active if needed
	if p, err := s.Get(slug); err == nil && p.ActiveSession == sessionID {
		p.ActiveSession = ""
		p.UpdatedAt = time.Now().UTC()
		_ = s.save(p)
	}
	return nil
}

// GetSession loads one session including turns.
func (s *Store) GetSession(slug, sessionID string) (Session, error) {
	if !slugRe.MatchString(slug) {
		return Session{}, fmt.Errorf("invalid slug %q", slug)
	}
	if sessionID == "" || strings.Contains(sessionID, "/") || strings.Contains(sessionID, "..") {
		return Session{}, fmt.Errorf("invalid session id")
	}
	data, err := os.ReadFile(filepath.Join(s.projectDir(slug), "sessions", sessionID+".json"))
	if err != nil {
		return Session{}, err
	}
	var sess Session
	if err := json.Unmarshal(data, &sess); err != nil {
		return Session{}, err
	}
	return sess, nil
}

// saveSession writes session JSON.
func (s *Store) saveSession(slug string, sess Session) error {
	dir := filepath.Join(s.projectDir(slug), "sessions")
	if err := os.MkdirAll(dir, 0o700); err != nil {
		return err
	}
	data, err := json.MarshalIndent(sess, "", "  ")
	if err != nil {
		return err
	}
	return os.WriteFile(filepath.Join(dir, sess.ID+".json"), append(data, '\n'), 0o600)
}

// ImportSession creates a session with optional preloaded turns (e.g. JSON restore).
func (s *Store) ImportSession(slug, title string, turns []Turn) (Session, error) {
	if strings.TrimSpace(title) == "" {
		title = "导入会话"
	}
	if len(title) > 200 {
		title = title[:200]
	}
	// Cap import size for safety
	if len(turns) > 500 {
		turns = turns[:500]
	}
	sess, err := s.CreateSession(slug, title)
	if err != nil {
		return Session{}, err
	}
	if len(turns) == 0 {
		return sess, nil
	}
	return s.AppendTurns(slug, sess.ID, turns...)
}

// AppendTurns appends turns to a session (creates session file if needed via CreateSession first).
func (s *Store) AppendTurns(slug, sessionID string, turns ...Turn) (Session, error) {
	sess, err := s.GetSession(slug, sessionID)
	if err != nil {
		return Session{}, err
	}
	now := time.Now().UTC()
	for i := range turns {
		if turns[i].At.IsZero() {
			turns[i].At = now
		}
		sess.Turns = append(sess.Turns, turns[i])
	}
	sess.UpdatedAt = now
	// Auto-title from first user turn if still default-ish
	if len(sess.Turns) > 0 && (sess.Title == "" || strings.HasPrefix(sess.Title, "会话 ")) {
		for _, t := range sess.Turns {
			if t.Role == "user" && strings.TrimSpace(t.Text) != "" {
				title := strings.TrimSpace(t.Text)
				if len([]rune(title)) > 40 {
					title = string([]rune(title)[:40]) + "…"
				}
				sess.Title = title
				break
			}
		}
	}
	if err := s.saveSession(slug, sess); err != nil {
		return Session{}, err
	}
	if p, err := s.Get(slug); err == nil {
		p.ActiveSession = sessionID
		p.UpdatedAt = now
		_ = s.save(p)
	}
	return sess, nil
}

// SessionSearchHit is one turn matching a full-text query.
type SessionSearchHit struct {
	SessionID    string `json:"session_id"`
	SessionTitle string `json:"session_title"`
	TurnIndex    int    `json:"turn_index"`
	Role         string `json:"role"`
	Snippet      string `json:"snippet"`
	At           time.Time `json:"at,omitempty"`
}

// SearchSessions scans all session turns for query (case-insensitive substring).
func (s *Store) SearchSessions(slug, query string, limit int) ([]SessionSearchHit, error) {
	q := strings.TrimSpace(strings.ToLower(query))
	if q == "" {
		return nil, nil
	}
	if limit <= 0 {
		limit = 50
	}
	if limit > 200 {
		limit = 200
	}
	// Load full sessions (with turns) from disk
	dir := filepath.Join(s.projectDir(slug), "sessions")
	entries, err := os.ReadDir(dir)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, err
	}
	var hits []SessionSearchHit
	for _, e := range entries {
		if e.IsDir() || !strings.HasSuffix(e.Name(), ".json") {
			continue
		}
		data, err := os.ReadFile(filepath.Join(dir, e.Name()))
		if err != nil {
			continue
		}
		var sess Session
		if json.Unmarshal(data, &sess) != nil {
			continue
		}
		// title match
		if strings.Contains(strings.ToLower(sess.Title), q) {
			hits = append(hits, SessionSearchHit{
				SessionID: sess.ID, SessionTitle: sess.Title,
				TurnIndex: -1, Role: "meta", Snippet: "标题匹配: " + sess.Title, At: sess.UpdatedAt,
			})
			if len(hits) >= limit {
				return hits, nil
			}
		}
		for i, t := range sess.Turns {
			if !strings.Contains(strings.ToLower(t.Text), q) {
				// also search tool names/outputs
				foundTool := false
				for _, tool := range t.Tools {
					blob := strings.ToLower(tool.Name + " " + tool.Args + " " + tool.Output + " " + tool.Err)
					if strings.Contains(blob, q) {
						foundTool = true
						break
					}
				}
				if !foundTool {
					continue
				}
			}
			snip := t.Text
			if snip == "" && len(t.Tools) > 0 {
				snip = "tool:" + t.Tools[0].Name
			}
			// clip snippet around match
			low := strings.ToLower(snip)
			idx := strings.Index(low, q)
			if idx >= 0 {
				start := idx - 40
				if start < 0 {
					start = 0
				}
				end := idx + len(q) + 40
				if end > len(snip) {
					end = len(snip)
				}
				snip = snip[start:end]
				if start > 0 {
					snip = "…" + snip
				}
				if end < len([]rune(t.Text)) || end < len(t.Text) {
					snip = snip + "…"
				}
			}
			if len([]rune(snip)) > 160 {
				snip = string([]rune(snip)[:160]) + "…"
			}
			hits = append(hits, SessionSearchHit{
				SessionID: sess.ID, SessionTitle: sess.Title,
				TurnIndex: i, Role: t.Role, Snippet: snip, At: t.At,
			})
			if len(hits) >= limit {
				return hits, nil
			}
		}
	}
	return hits, nil
}

// EnsureSession returns existing session or creates one.
func (s *Store) EnsureSession(slug, sessionID, title string) (Session, error) {
	if sessionID != "" {
		if sess, err := s.GetSession(slug, sessionID); err == nil {
			return sess, nil
		}
	}
	return s.CreateSession(slug, title)
}

// EnabledSkillsPath returns path to per-project enabled skills list.
func (s *Store) EnabledSkillsPath(slug string) (string, error) {
	dir, err := s.ProjectDir(slug)
	if err != nil {
		return "", err
	}
	return filepath.Join(dir, ".lumen", "skills-enabled.json"), nil
}

// LoadEnabledSkills returns enabled skill names for a project (nil = all allowed / none filtered).
func (s *Store) LoadEnabledSkills(slug string) ([]string, error) {
	path, err := s.EnabledSkillsPath(slug)
	if err != nil {
		return nil, err
	}
	data, err := os.ReadFile(path)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, err
	}
	var body struct {
		Enabled []string `json:"enabled"`
	}
	if err := json.Unmarshal(data, &body); err != nil {
		return nil, err
	}
	return body.Enabled, nil
}

// SaveEnabledSkills writes enabled skill names.
func (s *Store) SaveEnabledSkills(slug string, enabled []string) error {
	path, err := s.EnabledSkillsPath(slug)
	if err != nil {
		return err
	}
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return err
	}
	data, err := json.MarshalIndent(map[string]any{"enabled": enabled}, "", "  ")
	if err != nil {
		return err
	}
	return os.WriteFile(path, append(data, '\n'), 0o600)
}
