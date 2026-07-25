// Package renderers provides a web-based artifact rendering framework for Lumen Science.
//
// Each renderer is a self-contained HTML page that loads its visualization
// library from CDN and renders artifact data. The framework serves these pages
// and routes artifact data through a local API.
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

// RendererDescriptor describes one registered science renderer.
type RendererDescriptor struct {
	ID          string   `json:"id"`
	Name        string   `json:"name"`
	Description string   `json:"description"`
	MimeTypes   []string `json:"mime_types"`
	PreviewType string   `json:"preview_type"`
	Route       string   `json:"route"`
}

// Registry of all available renderers.
var Registry = []RendererDescriptor{
	{
		ID: "protein-3d", Name: "Protein Structure (3D)",
		Description: "Interactive 3D macromolecule viewer using Mol*",
		MimeTypes:   []string{"chemical/x-pdb", "chemical/x-mmcif"},
		PreviewType: "pdb", Route: "/render/protein-3d",
	},
	{
		ID: "chem-2d", Name: "Chemical Structure (2D)",
		Description: "2D chemical structure renderer using RDKit.js",
		MimeTypes:   []string{"chemical/x-smiles", "chemical/x-mdl-molfile"},
		PreviewType: "smiles", Route: "/render/chem-2d",
	},
	{
		ID: "genome-browser", Name: "Genome Browser",
		Description: "Interactive genome track browser using IGV.js",
		MimeTypes:   []string{"application/x-bed", "application/x-bigwig"},
		PreviewType: "bed", Route: "/render/genome-browser",
	},
	{
		ID: "katex", Name: "LaTeX / Math",
		Description: "Mathematical formula renderer using KaTeX",
		MimeTypes:   []string{"application/x-latex", "text/x-tex"},
		PreviewType: "latex", Route: "/render/katex",
	},
	{
		ID: "pdf-viewer", Name: "PDF Viewer",
		Description: "Inline PDF document viewer",
		MimeTypes:   []string{"application/pdf"},
		PreviewType: "pdf", Route: "/render/pdf",
	},
	{
		ID: "sequence-viewer", Name: "Sequence Viewer",
		Description: "DNA/RNA/protein sequence viewer",
		MimeTypes:   []string{"text/x-fasta", "text/x-fastq"},
		PreviewType: "fasta", Route: "/render/sequence",
	},
	{
		ID: "msa-viewer", Name: "MSA Viewer",
		Description: "Multiple sequence alignment viewer",
		MimeTypes:   []string{"application/x-stockholm", "application/x-clustal"},
		PreviewType: "msa", Route: "/render/msa",
	},
	{
		ID: "image-viewer", Name: "Image Viewer",
		Description: "Scientific image and micrograph viewer",
		MimeTypes:   []string{"image/png", "image/jpeg", "image/tiff", "image/svg+xml"},
		PreviewType: "image", Route: "/render/image",
	},
	{
		ID: "motif", Name: "Motif Molecular Workbench",
		Description: "Interactive molecular biology workbench",
		MimeTypes:   []string{"application/x-motif"},
		PreviewType: "motif", Route: "/render/motif",
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

	// API: get artifact metadata
	mux.HandleFunc("/render/api/artifact", func(w http.ResponseWriter, r *http.Request) {
		project := r.URL.Query().Get("project")
		run := r.URL.Query().Get("run")
		path := r.URL.Query().Get("path")
		if project == "" || run == "" || path == "" {
			http.Error(w, `{"error":"missing project/run/path"}`, http.StatusBadRequest)
			return
		}
		fullPath := filepath.Join(s.artifactsDir, project, run, path)
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
		fullPath := filepath.Join(s.artifactsDir, project, run, path)
		data, err := os.ReadFile(fullPath)
		if err != nil {
			http.Error(w, "not found", http.StatusNotFound)
			return
		}
		w.Write(data)
	})
}
