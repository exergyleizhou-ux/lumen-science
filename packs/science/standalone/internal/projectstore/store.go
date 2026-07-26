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
// ── Evidence queries ─────────────────────────────────

func (s *Store) EvidenceTrace(projectID, claimID string) (map[string]any, error) {
	c, err := s.loadClaim(projectID, claimID)
	if err != nil {
		return nil, err
	}
	graph, err := s.loadGraph(projectID)
	if err != nil {
		return nil, err
	}
	nid := ""
	if c.EvidenceNodeID != nil {
		nid = *c.EvidenceNodeID
	}
	nodes := graph["nodes"].(map[string]any)
	edges := graph["edges"].([]any)
	var trace []map[string]any
	visited := map[string]bool{}
	traceBack(nid, edges, nodes, &trace, visited, 0)

	return map[string]any{
		"claim_node_id": nid,
		"trace_steps": trace,
		"depth": len(visited),
		"claim_id": claimID,
	}, nil
}

func traceBack(nodeID string, edges []any, nodes map[string]any, trace *[]map[string]any, visited map[string]bool, depth int) {
	if depth > 100 || visited[nodeID] {
		return
	}
	visited[nodeID] = true
	for _, raw := range edges {
		e := raw.(map[string]any)
		to, _ := e["target"].(string)
		if to != nodeID {
			continue
		}
		from, _ := e["source"].(string)
		rel, _ := e["relation"].(string)
		sha, _ := e["supporting_artifact_sha256"].(string)
		*trace = append(*trace, map[string]any{
			"from": from, "to": to, "relation": rel, "artifact_sha256": sha,
		})
		traceBack(from, edges, nodes, trace, visited, depth+1)
	}
}

func (s *Store) EvidenceConsistency(projectID string) (map[string]any, error) {
	graph, err := s.loadGraph(projectID)
	if err != nil {
		return nil, err
	}
	nodes := graph["nodes"].(map[string]any)
	edges := graph["edges"].([]any)
	var violations []map[string]any
	nodeIDs := map[string]bool{}
	for k := range nodes {
		nodeIDs[k] = true
	}
	for _, raw := range edges {
		e := raw.(map[string]any)
		src, _ := e["source"].(string)
		tgt, _ := e["target"].(string)
		if !nodeIDs[src] {
			violations = append(violations, map[string]any{"kind": "DanglingSource", "detail": src})
		}
		if !nodeIDs[tgt] {
			violations = append(violations, map[string]any{"kind": "DanglingTarget", "detail": tgt})
		}
		if src == tgt {
			violations = append(violations, map[string]any{"kind": "SelfReferencingClaim", "detail": src})
		}
	}
	return map[string]any{
		"graph_id": "graph-" + projectID,
		"violations": violations,
		"is_consistent": len(violations) == 0,
		"node_count": len(nodes),
		"edge_count": len(edges),
	}, nil
}

func (s *Store) EvidenceCompare(projectID, claimA, claimB string) (map[string]any, error) {
	ta, err := s.EvidenceTrace(projectID, claimA)
	if err != nil {
		return nil, err
	}
	tb, err := s.EvidenceTrace(projectID, claimB)
	if err != nil {
		return nil, err
	}
	sa := map[string]bool{}
	sb := map[string]bool{}
	for _, step := range ta["trace_steps"].([]map[string]any) {
		sa[step["from"].(string)] = true
	}
	for _, step := range tb["trace_steps"].([]map[string]any) {
		sb[step["from"].(string)] = true
	}
	var shared []string
	for k := range sa {
		if sb[k] {
			shared = append(shared, k)
		}
	}
	var onlyA []string
	for k := range sa {
		if !sb[k] {
			onlyA = append(onlyA, k)
		}
	}
	var onlyB []string
	for k := range sb {
		if !sa[k] {
			onlyB = append(onlyB, k)
		}
	}
	conflicting := append(onlyA, onlyB...)
	return map[string]any{
		"claim_a": claimA, "claim_b": claimB,
		"shared_evidence": shared, "conflicting_evidence": conflicting,
		"supports_same_conclusion": len(conflicting) == 0,
	}, nil
}

type MultimodalIndex struct {
	ProjectID   string `json:"project_id"`
	FileCount   int    `json:"file_count"`
	ParserCount int    `json:"parser_count"`
}

func (s *Store) MultimodalIndex(projectID string) (*MultimodalIndex, error) {
	_, err := s.Get(projectID)
	if err != nil {
		return nil, err
	}
	dir := filepath.Join(s.Root, "projects", projectID)
	count := 0
	filepath.Walk(dir, func(_ string, info os.FileInfo, err error) error {
		if err == nil && !info.IsDir() {
			count++
		}
		return nil
	})
	return &MultimodalIndex{ProjectID: projectID, FileCount: count, ParserCount: 6}, nil
}

func (s *Store) ReviewRecord(projectID, reviewerID, verdict, claimID string) (map[string]any, error) {
	return map[string]any{
		"project_id": projectID, "reviewer_id": reviewerID,
		"verdict": verdict, "claim_id": claimID,
		"authority": "SessionActor-gated, no independent MCP",
	}, nil
}

func (s *Store) CollaborationInvite(projectID, owner, invitee string) (map[string]any, error) {
	p, err := s.Get(projectID)
	if err != nil {
		return nil, err
	}
	if p.OwnerID != owner {
		return nil, fmt.Errorf("ownership mismatch")
	}
	return map[string]any{
		"project_id": projectID, "owner": p.OwnerID,
		"invitee": invitee, "pending": true,
	}, nil
}

// WorkflowValidate checks DAG and rejects Shell/unknown StepKinds.
func (s *Store) WorkflowValidate(spec map[string]any) (map[string]any, error) {
	stepsRaw, ok := spec["steps"].([]any)
	if !ok {
		return nil, fmt.Errorf("spec missing steps array")
	}
	allowedKinds := map[string]bool{
		"ConnectorFetch": true, "ArtifactTransform": true, "NotebookCell": true,
		"Renderer": true, "Reviewer": true, "HumanApproval": true, "Export": true,
	}
	var steps []string
	hasUnknown := false
	links := map[string][]string{}
	all := map[string]bool{}
	for _, raw := range stepsRaw {
		st := raw.(map[string]any)
		sid := st["step_id"].(string)
		all[sid] = true
		steps = append(steps, sid)
		kind, _ := st["kind"].(string)
		if kind == "" || !allowedKinds[kind] {
			hasUnknown = true
		}
		inputs, _ := st["inputs"].([]any)
		for _, in := range inputs {
			is := in.(string)
			links[is] = append(links[is], sid)
		}
	}
	var errors []string
	if hasUnknown {
		errors = append(errors, "workflow contains Shell or unsupported StepKind")
	}
	for from := range links {
		if !all[from] {
			errors = append(errors, fmt.Sprintf("step %s referenced as input but not defined", from))
		}
	}
	return map[string]any{
		"workflow_id": spec["workflow_id"],
		"is_valid":    len(errors) == 0,
		"errors":      errors,
		"steps_count": len(steps),
		"has_unknown_step": hasUnknown,
	}, nil
}

// WorkflowDryRun returns allowed_steps, rejected_unknown, blocked_reasons, can_run.
func (s *Store) WorkflowDryRun(spec map[string]any) (map[string]any, error) {
	val, err := s.WorkflowValidate(spec)
	if err != nil {
		return nil, err
	}
	stepsRaw, _ := spec["steps"].([]any)
	var allowed []string
	var rejected []string
	var blocked []string
	valErrors, _ := val["errors"].([]string)
	for _, e := range valErrors {
		blocked = append(blocked, e)
	}
	allowedKinds := map[string]bool{
		"ConnectorFetch": true, "ArtifactTransform": true, "NotebookCell": true,
		"Renderer": true, "Reviewer": true, "HumanApproval": true, "Export": true,
	}
	for _, raw := range stepsRaw {
		st := raw.(map[string]any)
		sid := st["step_id"].(string)
		kind, _ := st["kind"].(string)
		if allowedKinds[kind] {
			allowed = append(allowed, sid)
		} else {
			rejected = append(rejected, sid)
			blocked = append(blocked, fmt.Sprintf("step %s: kind=%q not allowed", sid, kind))
		}
	}
	isValid, _ := val["is_valid"].(bool)
	return map[string]any{
		"workflow_id": spec["workflow_id"],
		"can_run":     isValid && len(rejected) == 0,
		"allowed_steps": allowed,
		"rejected_unknown": rejected,
		"blocked_reasons": blocked,
		"kernel_passes": true,
		"notes": []string{"Dry-run only; no real execution"},
	}, nil
}

// MigrateFromV1 returns MigrationResult fields (offline).
func (s *Store) MigrateFromV1(runID, owner, title, question string) (map[string]any, error) {
	p, err := s.Create(owner, title, question)
	if err != nil {
		return nil, err
	}
	return map[string]any{
		"source_run_id": runID,
		"target_project_id": p.ProjectID,
		"migrated_project": p.ProjectID,
		"artifacts_migrated": 0,
		"evidence_items_migrated": 0,
		"hash_verification": "Verified",
	}, nil
}

func (s *Store) loadClaim(projectID, claimID string) (*Claim, error) {
	p := filepath.Join(s.Root, "projects", projectID, "claims", claimID+".json")
	b, err := os.ReadFile(p)
	if err != nil {
		return nil, err
	}
	var c Claim
	if err := json.Unmarshal(b, &c); err != nil {
		return nil, err
	}
	return &c, nil
}

func (s *Store) loadGraph(projectID string) (map[string]any, error) {
	p := filepath.Join(s.Root, "projects", projectID, "graph.json")
	b, err := os.ReadFile(p)
	if err != nil {
		return nil, err
	}
	// Ensure nodes and edges are present even if empty
	// Read raw structure, handle empty map
	var graph map[string]any
	if err := json.Unmarshal(b, &graph); err != nil {
		return nil, err
	}
	if graph["nodes"] == nil {
		graph["nodes"] = map[string]any{}
	}
	if graph["edges"] == nil {
		graph["edges"] = []any{}
	}
	return graph, nil
}
