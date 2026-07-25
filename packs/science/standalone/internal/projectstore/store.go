// Package projectstore is a Go-side durable ResearchProject store matching
// the Rust ProjectStore layout for offline CLI use (WP-2 product path).
//
// Layout: {root}/projects/{id}/project.json, graph.json, claims/*.json
package projectstore

import (
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"time"
)

type Project struct {
	ProjectID         string    `json:"project_id"`
	OwnerID           string    `json:"owner_id"`
	Title             string    `json:"title"`
	ResearchQuestion  string    `json:"research_question"`
	Hypotheses        []any     `json:"hypotheses"`
	Sessions          []string  `json:"sessions"`
	Datasets          []string  `json:"datasets"`
	Workflows         []string  `json:"workflows"`
	EvidenceGraphID   string    `json:"evidence_graph_id"`
	ReviewPolicy      string    `json:"review_policy"`
	RetentionPolicy   string    `json:"retention_policy"`
	Status            string    `json:"status"`
	SchemaVersion     int       `json:"schema_version"`
	CreatedAt         time.Time `json:"created_at"`
	UpdatedAt         time.Time `json:"updated_at"`
}

// Marshal with nested id shape compatible with Rust ProjectId(String) serde
type projectWire struct {
	ProjectID        idWire    `json:"project_id"`
	OwnerID          idWire    `json:"owner_id"`
	Title            string    `json:"title"`
	ResearchQuestion string    `json:"research_question"`
	Hypotheses       []any     `json:"hypotheses"`
	Sessions         []string  `json:"sessions"`
	Datasets         []string  `json:"datasets"`
	Workflows        []string  `json:"workflows"`
	EvidenceGraphID  *string   `json:"evidence_graph_id"`
	ReviewPolicy     string    `json:"review_policy"`
	RetentionPolicy  string    `json:"retention_policy"`
	Status           string    `json:"status"`
	SchemaVersion    int       `json:"schema_version"`
	CreatedAt        time.Time `json:"created_at"`
	UpdatedAt        time.Time `json:"updated_at"`
}

type idWire struct {
	// Rust ProjectId is a newtype tuple struct: "proj-id" or {"0":"..."} depending on serde
	// We use transparent string via custom encoding as plain string matching #[serde(transparent)] if present.
	// model.rs uses ProjectId(pub String) without transparent — typically {"0":"uuid"} or just string.
	// Looking at model: no transparent on ProjectId in project/model — serde for tuple struct is sequence or map.
	// Simplest compatible form for Rust serde: ProjectId serializes as a single-element array or map.
	// Actually in Rust without transparent: ProjectId("x") serializes as "x" if newtype, or as ["x"].
	// Tuple struct ProjectId(String) with derive Serialize = single-field tuple → JSON string if newtype-like.
	// In serde, tuple struct with one field is newtype → serializes as inner type → plain string.
	V string
}

// Use plain string JSON for project_id/owner_id (serde newtype).
func (p Project) toWire() projectWire {
	eg := p.EvidenceGraphID
	var egp *string
	if eg != "" {
		egp = &eg
	}
	return projectWire{
		ProjectID:        idWire{p.ProjectID},
		OwnerID:          idWire{p.OwnerID},
		Title:            p.Title,
		ResearchQuestion: p.ResearchQuestion,
		Hypotheses:       p.Hypotheses,
		Sessions:         p.Sessions,
		Datasets:         p.Datasets,
		Workflows:        p.Workflows,
		EvidenceGraphID:  egp,
		ReviewPolicy:     p.ReviewPolicy,
		RetentionPolicy:  p.RetentionPolicy,
		Status:           p.Status,
		SchemaVersion:    p.SchemaVersion,
		CreatedAt:        p.CreatedAt,
		UpdatedAt:        p.UpdatedAt,
	}
}

// Simpler approach: store as plain JSON matching Rust with string fields via custom marshal
type projectJSON struct {
	ProjectID        string    `json:"project_id"`
	OwnerID          string    `json:"owner_id"`
	Title            string    `json:"title"`
	ResearchQuestion string    `json:"research_question"`
	Hypotheses       []any     `json:"hypotheses"`
	Sessions         []string  `json:"sessions"`
	Datasets         []string  `json:"datasets"`
	Workflows        []string  `json:"workflows"`
	EvidenceGraphID  *string   `json:"evidence_graph_id"`
	ReviewPolicy     string    `json:"review_policy"`
	RetentionPolicy  string    `json:"retention_policy"`
	Status           string    `json:"status"`
	SchemaVersion    int       `json:"schema_version"`
	CreatedAt        time.Time `json:"created_at"`
	UpdatedAt        time.Time `json:"updated_at"`
}

// Rust ProjectId(String) without transparent serializes as a JSON array with one element? 
// Actually for `struct ProjectId(pub String)` with Serialize, serde treats it as newtype struct → serializes as the string itself.
// Same for OwnerId. Status is enum → string variant name.

func (p Project) marshal() ([]byte, error) {
	eg := p.EvidenceGraphID
	var egp *string
	if eg != "" {
		egp = &eg
	}
	j := projectJSON{
		ProjectID: p.ProjectID, OwnerID: p.OwnerID, Title: p.Title,
		ResearchQuestion: p.ResearchQuestion, Hypotheses: p.Hypotheses,
		Sessions: p.Sessions, Datasets: p.Datasets, Workflows: p.Workflows,
		EvidenceGraphID: egp, ReviewPolicy: p.ReviewPolicy, RetentionPolicy: p.RetentionPolicy,
		Status: p.Status, SchemaVersion: p.SchemaVersion,
		CreatedAt: p.CreatedAt, UpdatedAt: p.UpdatedAt,
	}
	// Rust enums are externally tagged by default as strings for unit variants: "Draft"
	return json.MarshalIndent(j, "", "  ")
}

// Fix: Rust ProjectId serializes as plain string only with #[serde(transparent)].
// Without it, tuple struct is: {"0": "uuid"} in some versions... serde newtype = transparent behavior for 1-tuple.
// From serde docs: newtype struct serializes as the inner value. Good — plain string.

func parseProject(b []byte) (Project, error) {
	// Accept plain string ids or map form
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(b, &raw); err != nil {
		return Project{}, err
	}
	getStr := func(k string) string {
		v, ok := raw[k]
		if !ok {
			return ""
		}
		var s string
		if json.Unmarshal(v, &s) == nil {
			return s
		}
		var m map[string]string
		if json.Unmarshal(v, &m) == nil {
			if x, ok := m["0"]; ok {
				return x
			}
		}
		return strings.Trim(string(v), "\"")
	}
	var status string
	_ = json.Unmarshal(raw["status"], &status)
	var schema int
	_ = json.Unmarshal(raw["schema_version"], &schema)
	var created, updated time.Time
	_ = json.Unmarshal(raw["created_at"], &created)
	_ = json.Unmarshal(raw["updated_at"], &updated)
	eg := getStr("evidence_graph_id")
	p := Project{
		ProjectID: getStr("project_id"), OwnerID: getStr("owner_id"),
		Title: getStr("title"), ResearchQuestion: getStr("research_question"),
		EvidenceGraphID: eg, Status: status, SchemaVersion: schema,
		CreatedAt: created, UpdatedAt: updated,
		ReviewPolicy: getStr("review_policy"), RetentionPolicy: getStr("retention_policy"),
		Hypotheses: []any{}, Sessions: []string{}, Datasets: []string{}, Workflows: []string{},
	}
	if p.ReviewPolicy == "" {
		p.ReviewPolicy = "default"
	}
	if p.RetentionPolicy == "" {
		p.RetentionPolicy = "default"
	}
	return p, nil
}

type Claim struct {
	ClaimID         string    `json:"claim_id"`
	ProjectID       string    `json:"project_id"`
	Statement       string    `json:"statement"`
	Status          string    `json:"status"`
	EvidenceNodeID  *string   `json:"evidence_node_id"`
	Limitations     []string  `json:"limitations"`
	CreatedAt       time.Time `json:"created_at"`
	UpdatedAt       time.Time `json:"updated_at"`
	ProposedBy      string    `json:"proposed_by"`
}

type Store struct {
	Root string
}

func New(root string) *Store {
	return &Store{Root: root}
}

func (s *Store) projectDir(id string) string {
	return filepath.Join(s.Root, "projects", id)
}

func newID() string {
	var b [16]byte
	_, _ = rand.Read(b[:])
	return hex.EncodeToString(b[:])
}

func writeAtomic(path string, data []byte) error {
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return err
	}
	tmp := path + ".tmp"
	if err := os.WriteFile(tmp, data, 0o644); err != nil {
		return err
	}
	return os.Rename(tmp, path)
}

func (s *Store) Create(owner, title, question string) (*Project, error) {
	if owner == "" || title == "" {
		return nil, fmt.Errorf("owner and title required")
	}
	id := newID()
	now := time.Now().UTC()
	eg := "graph-" + id
	p := &Project{
		ProjectID: id, OwnerID: owner, Title: title, ResearchQuestion: question,
		Hypotheses: []any{}, Sessions: []string{}, Datasets: []string{}, Workflows: []string{},
		EvidenceGraphID: eg, ReviewPolicy: "default", RetentionPolicy: "default",
		Status: "Draft", SchemaVersion: 1, CreatedAt: now, UpdatedAt: now,
	}
	dir := s.projectDir(id)
	if err := os.MkdirAll(filepath.Join(dir, "claims"), 0o755); err != nil {
		return nil, err
	}
	b, err := p.marshal()
	if err != nil {
		return nil, err
	}
	// Rust expects ProjectId as newtype string — re-marshal with correct shape for Rust interop
	// Use raw JSON that matches Rust serde for ProjectId(String): just the string value
	// But ResearchProject fields project_id: ProjectId — newtype → JSON string. OK.
	// Status: ProjectStatus enum unit → "Draft" as string. OK if we use same names.
	// Actually Rust enum without rename is "Draft" etc. Good.
	if err := writeAtomic(filepath.Join(dir, "project.json"), b); err != nil {
		return nil, err
	}
	// Empty graph matching Rust EvidenceGraph
	graph := map[string]any{
		"project_id":     id,
		"schema_version": 1,
		"nodes":          map[string]any{},
		"edges":          []any{},
	}
	// Rust ProjectId in graph also newtype string
	gb, _ := json.MarshalIndent(graph, "", "  ")
	if err := writeAtomic(filepath.Join(dir, "graph.json"), gb); err != nil {
		return nil, err
	}
	return p, nil
}

func (s *Store) List() ([]Project, error) {
	root := filepath.Join(s.Root, "projects")
	ents, err := os.ReadDir(root)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, err
	}
	var out []Project
	for _, e := range ents {
		if !e.IsDir() {
			continue
		}
		b, err := os.ReadFile(filepath.Join(root, e.Name(), "project.json"))
		if err != nil {
			continue
		}
		p, err := parseProject(b)
		if err != nil {
			continue
		}
		out = append(out, p)
	}
	sort.Slice(out, func(i, j int) bool { return out[i].ProjectID < out[j].ProjectID })
	return out, nil
}

func (s *Store) Get(id string) (*Project, error) {
	b, err := os.ReadFile(filepath.Join(s.projectDir(id), "project.json"))
	if err != nil {
		return nil, err
	}
	p, err := parseProject(b)
	if err != nil {
		return nil, err
	}
	return &p, nil
}

func (s *Store) ProposeClaim(projectID, owner, statement, by string) (*Claim, error) {
	p, err := s.Get(projectID)
	if err != nil {
		return nil, err
	}
	if p.OwnerID != owner {
		return nil, fmt.Errorf("ownership mismatch")
	}
	if statement == "" {
		return nil, fmt.Errorf("statement required")
	}
	cid := "claim-" + newID()
	nid := "node-" + cid
	now := time.Now().UTC()
	c := &Claim{
		ClaimID: cid, ProjectID: projectID, Statement: statement,
		Status: "Proposed", EvidenceNodeID: &nid, Limitations: []string{},
		CreatedAt: now, UpdatedAt: now, ProposedBy: by,
	}
	// update graph
	gpath := filepath.Join(s.projectDir(projectID), "graph.json")
	gb, err := os.ReadFile(gpath)
	if err != nil {
		return nil, err
	}
	var graph map[string]any
	if err := json.Unmarshal(gb, &graph); err != nil {
		return nil, err
	}
	nodes, _ := graph["nodes"].(map[string]any)
	if nodes == nil {
		nodes = map[string]any{}
	}
	nodes[nid] = map[string]any{
		"node_id": nid, "kind": "Claim", "project_id": projectID,
		"label": statement, "artifact_sha256": nil, "run_id": nil,
		"created_by": by, "created_at": now, "metadata": map[string]string{},
	}
	graph["nodes"] = nodes
	nb, _ := json.MarshalIndent(graph, "", "  ")
	if err := writeAtomic(gpath, nb); err != nil {
		return nil, err
	}
	cb, _ := json.MarshalIndent(c, "", "  ")
	if err := writeAtomic(filepath.Join(s.projectDir(projectID), "claims", cid+".json"), cb); err != nil {
		return nil, err
	}
	return c, nil
}
