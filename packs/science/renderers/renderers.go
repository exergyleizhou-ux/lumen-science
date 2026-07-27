// Package renderers provides a web-based artifact rendering framework for Lumen Science.
//
// Security contract (DS-44 / DS-45):
//   - Renderers are adapters only. SessionActor remains sole execution authority.
//   - Artifact bytes must come from a registered store (project/session/owner + SHA-256).
//   - Prefer CSP-locked, same-origin pages. CDN scripts are legacy debt to eliminate.
//   - Motif is Lumen-managed MotifRenderer only — never an independent MCP authority.
package renderers

import (
	"embed"
	"encoding/json"
	"fmt"
	"net/http"
	"os"
	"path/filepath"
	"strings"
)

//go:embed static/*
var staticFiles embed.FS

// NetworkPolicy for a renderer page.
type NetworkPolicy string

const (
	NetworkNone        NetworkPolicy = "none"
	NetworkSameOrigin  NetworkPolicy = "same-origin"
	NetworkAllowlisted NetworkPolicy = "allowlisted" // legacy; prefer none/same-origin
)

// RendererDescriptor describes one registered science renderer.
type RendererDescriptor struct {
	ID             string        `json:"id"`
	Name           string        `json:"name"`
	Description    string        `json:"description"`
	MimeTypes      []string      `json:"mime_types"`
	PreviewType    string        `json:"preview_type"`
	Route          string        `json:"route"`
	NetworkPolicy  NetworkPolicy `json:"network_policy"`
	ScriptPolicy   string        `json:"script_policy"` // "inline-csp" | "legacy-cdn" | "none"
	MaxInputBytes  int64         `json:"max_input_bytes"`
	RuntimeAuthority string      `json:"runtime_authority"` // always "none" for renderers
	Admission      string        `json:"admission"`
}

// Registry of all available renderers.
var Registry = []RendererDescriptor{
	{
		ID: "protein-3d", Name: "Protein Structure (3D)",
		Description: "Interactive 3D macromolecule viewer using Mol*",
		MimeTypes:   []string{"chemical/x-pdb", "chemical/x-mmcif"},
		PreviewType: "pdb", Route: "/render/protein-3d",
		NetworkPolicy: NetworkAllowlisted, ScriptPolicy: "legacy-cdn", MaxInputBytes: 32 << 20,
		RuntimeAuthority: "none", Admission: "pending-cdn-elimination",
	},
	{
		ID: "chem-2d", Name: "Chemical Structure (2D)",
		Description: "2D chemical structure renderer using RDKit.js",
		MimeTypes:   []string{"chemical/x-smiles", "chemical/x-mdl-molfile"},
		PreviewType: "smiles", Route: "/render/chem-2d",
		NetworkPolicy: NetworkAllowlisted, ScriptPolicy: "legacy-cdn", MaxInputBytes: 2 << 20,
		RuntimeAuthority: "none", Admission: "pending-cdn-elimination",
	},
	{
		ID: "genome-browser", Name: "Genome Browser",
		Description: "Interactive genome track browser using IGV.js",
		MimeTypes:   []string{"application/x-bed", "application/x-bigwig"},
		PreviewType: "bed", Route: "/render/genome-browser",
		NetworkPolicy: NetworkAllowlisted, ScriptPolicy: "legacy-cdn", MaxInputBytes: 64 << 20,
		RuntimeAuthority: "none", Admission: "pending-cdn-elimination",
	},
	{
		ID: "katex", Name: "LaTeX / Math",
		Description: "Mathematical formula renderer using KaTeX",
		MimeTypes:   []string{"application/x-latex", "text/x-tex"},
		PreviewType: "latex", Route: "/render/katex",
		NetworkPolicy: NetworkAllowlisted, ScriptPolicy: "legacy-cdn", MaxInputBytes: 1 << 20,
		RuntimeAuthority: "none", Admission: "pending-cdn-elimination",
	},
	{
		ID: "pdf-viewer", Name: "PDF Viewer",
		Description: "Inline PDF document viewer",
		MimeTypes:   []string{"application/pdf"},
		PreviewType: "pdf", Route: "/render/pdf",
		NetworkPolicy: NetworkSameOrigin, ScriptPolicy: "inline-csp", MaxInputBytes: 64 << 20,
		RuntimeAuthority: "none", Admission: "preview",
	},
	{
		ID: "sequence-viewer", Name: "Sequence Viewer",
		Description: "DNA/RNA/protein sequence viewer",
		MimeTypes:   []string{"text/x-fasta", "text/x-fastq"},
		PreviewType: "fasta", Route: "/render/sequence",
		NetworkPolicy: NetworkSameOrigin, ScriptPolicy: "inline-csp", MaxInputBytes: 16 << 20,
		RuntimeAuthority: "none", Admission: "preview",
	},
	{
		ID: "msa-viewer", Name: "MSA Viewer",
		Description: "Multiple sequence alignment viewer",
		MimeTypes:   []string{"application/x-stockholm", "application/x-clustal"},
		PreviewType: "msa", Route: "/render/msa",
		NetworkPolicy: NetworkSameOrigin, ScriptPolicy: "inline-csp", MaxInputBytes: 32 << 20,
		RuntimeAuthority: "none", Admission: "preview",
	},
	{
		ID: "image-viewer", Name: "Image Viewer",
		Description: "Scientific image and micrograph viewer",
		MimeTypes:   []string{"image/png", "image/jpeg", "image/tiff", "image/svg+xml"},
		PreviewType: "image", Route: "/render/image",
		NetworkPolicy: NetworkSameOrigin, ScriptPolicy: "inline-csp", MaxInputBytes: 64 << 20,
		RuntimeAuthority: "none", Admission: "preview",
	},
	{
		ID: "motif", Name: "Lumen-managed MotifRenderer",
		Description: "CSP-locked Motif contract UI; artifact-bound molecular biology review (not independent MCP)",
		MimeTypes:   []string{"application/x-motif", "text/x-fasta", "text/x-genbank", "application/json"},
		PreviewType: "motif", Route: "/render/motif",
		NetworkPolicy: NetworkSameOrigin, ScriptPolicy: "inline-csp", MaxInputBytes: 32 << 20,
		RuntimeAuthority: "none", Admission: "pending-per-file-and-dependency-review",
	},
	{
		// Vendored self-contained Motif build (jvogan/motif@876a4f9e, Node 22).
		// Still no independent MCP authority — Lumen host must gate artifact I/O.
		ID: "motif-full", Name: "Motif Workbench (vendored)",
		Description: "Self-contained Motif molecular biology workbench artifact (MIT; see third_party/motif)",
		MimeTypes:   []string{"application/x-motif-full", "text/x-fasta", "text/x-genbank"},
		PreviewType: "motif-full", Route: "/render/motif-full",
		NetworkPolicy: NetworkNone, ScriptPolicy: "inline-csp", MaxInputBytes: 32 << 20,
		RuntimeAuthority: "none", Admission: "vendored-build-accepted-with-notices",
	},
}

// FindRenderer returns the best renderer for a given MIME type.
func FindRenderer(mimeType string) *RendererDescriptor {
	for _, r := range Registry {
		for _, m := range r.MimeTypes {
			if strings.EqualFold(m, mimeType) {
				return &r
			}
		}
	}
	return nil
}

// ArtifactData holds the payload sent to a renderer.
type ArtifactData struct {
	MimeType string `json:"mime_type"`
	Content  string `json:"content"`  // base64-encoded
	Filename string `json:"filename"`
	Label    string `json:"label"`
}

// Server serves renderer pages and artifact data.
type Server struct {
	artifactsDir string
}

// NewServer creates a renderer server that reads artifacts from the given directory.
func NewServer(artifactsDir string) *Server {
	return &Server{artifactsDir: artifactsDir}
}

// RegisterRoutes adds renderer routes to a mux.
func (s *Server) RegisterRoutes(mux *http.ServeMux) {
	// Serve static assets (CSS, JS)
	mux.HandleFunc("/render/static/", func(w http.ResponseWriter, r *http.Request) {
		path := strings.TrimPrefix(r.URL.Path, "/render/")
		data, err := staticFiles.ReadFile(path)
		if err != nil {
			http.NotFound(w, r)
			return
		}
		ct := "text/html"
		if strings.HasSuffix(path, ".css") {
			ct = "text/css"
		} else if strings.HasSuffix(path, ".js") {
			ct = "application/javascript"
		}
		w.Header().Set("Content-Type", ct)
		w.Write(data)
	})

	// Index page listing all renderers
	mux.HandleFunc("/render/", func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/render/" && r.URL.Path != "/render" {
			http.NotFound(w, r)
			return
		}
		w.Header().Set("Content-Type", "text/html; charset=utf-8")
		data, _ := staticFiles.ReadFile("static/index.html")
		w.Write(data)
	})

	// Individual renderer pages
	for _, renderer := range Registry {
		r := renderer
		mux.HandleFunc(r.Route, func(w http.ResponseWriter, rq *http.Request) {
			w.Header().Set("Content-Type", "text/html; charset=utf-8")
			page := fmt.Sprintf("static/%s.html", r.ID)
			data, err := staticFiles.ReadFile(page)
			if err != nil {
				data, _ = staticFiles.ReadFile("static/fallback.html")
			}
			w.Write(data)
		})
	}

	// Resolve an artifact path and REFUSE anything outside the artifacts root.
	//
	// All three components come from the query string, and filepath.Join
	// cleans but does not confine: a leading `..` survives cleaning as a path
	// OUTSIDE the root, so `path=../../etc/passwd` walked out of artifactsDir
	// and /render/api/raw served arbitrary local files (CodeQL
	// go/path-injection). Confinement is checked on the final joined path, so
	// no combination of project/run/path can compose an escape.
	confined := func(project, run, rel string) (string, bool) {
		root, err := filepath.Abs(s.artifactsDir)
		if err != nil {
			return "", false
		}
		full := filepath.Join(root, project, run, rel)
		if full != root && !strings.HasPrefix(full, root+string(filepath.Separator)) {
			return "", false
		}
		return full, true
	}

	// API: get artifact metadata
	mux.HandleFunc("/render/api/artifact", func(w http.ResponseWriter, r *http.Request) {
		project := r.URL.Query().Get("project")
		run := r.URL.Query().Get("run")
		path := r.URL.Query().Get("path")
		if project == "" || run == "" || path == "" {
			http.Error(w, `{"error":"missing project/run/path"}`, http.StatusBadRequest)
			return
		}
		fullPath, ok := confined(project, run, path)
		if !ok {
			http.Error(w, `{"error":"path escapes the artifacts root"}`, http.StatusBadRequest)
			return
		}
		if _, err := os.Stat(fullPath); err != nil {
			http.Error(w, `{"error":"artifact not found"}`, http.StatusNotFound)
			return
		}
		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(ArtifactData{
			Filename: filepath.Base(path),
		})
	})

	// API: raw artifact bytes for renderers
	mux.HandleFunc("/render/api/raw", func(w http.ResponseWriter, r *http.Request) {
		project := r.URL.Query().Get("project")
		run := r.URL.Query().Get("run")
		path := r.URL.Query().Get("path")
		if project == "" || run == "" || path == "" {
			http.Error(w, "missing params", http.StatusBadRequest)
			return
		}
		fullPath, ok := confined(project, run, path)
		if !ok {
			http.Error(w, "path escapes the artifacts root", http.StatusBadRequest)
			return
		}
		data, err := os.ReadFile(fullPath)
		if err != nil {
			http.Error(w, "not found", http.StatusNotFound)
			return
		}
		w.Write(data)
	})
}
