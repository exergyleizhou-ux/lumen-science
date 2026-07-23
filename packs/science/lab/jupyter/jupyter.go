// Package jupyter provides interactive notebook support for the science lab.
// When Jupyter is available (via conda or system), notebook cells can be
// created, executed, and read from the lab UI. Otherwise, a read-only JSON
// notebook viewer is provided.
package jupyter

import (
	"bytes"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"time"
)

// Cell represents a notebook cell (nbformat 4.x compatible).
type Cell struct {
	CellType       string   `json:"cell_type"`                 // code | markdown
	Metadata       map[string]any `json:"metadata"`            // at minimum {}
	Source         []string `json:"source"`
	ExecutionCount *int     `json:"execution_count"`           // null for unexecuted code cells
	Outputs        []Output `json:"outputs"` // always present for code cells ([]); nil for markdown (null)
}

// Output is one cell execution output.
// Handles Jupyter's text-as-array-of-strings format during Unmarshal.
type Output struct {
	OutputType string `json:"output_type"` // stream | execute_result | error
	Text       string `json:"text,omitempty"`
	Name       string `json:"name,omitempty"` // stdout | stderr
}

func (o *Output) UnmarshalJSON(data []byte) error {
	// Parse into a flexible map first.
	var raw map[string]any
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	o.OutputType, _ = raw["output_type"].(string)
	o.Name, _ = raw["name"].(string)

	// text can be string or []string (Jupyter nbformat uses []string).
	switch v := raw["text"].(type) {
	case string:
		o.Text = v
	case []any:
		var parts []string
		for _, item := range v {
			if s, ok := item.(string); ok {
				parts = append(parts, s)
			}
		}
		o.Text = strings.TrimRight(strings.Join(parts, ""), "\n")
	}
	return nil
}

// Notebook represents an .ipynb file.
type Notebook struct {
	Metadata      map[string]any `json:"metadata"`
	Nbformat      int            `json:"nbformat"`
	NbformatMinor int            `json:"nbformat_minor"`
	Cells         []Cell         `json:"cells"`

	// Path is the on-disk location for Save/Load; never serialized into .ipynb.
	Path string `json:"-"`
}

// New creates an empty notebook with a title markdown cell.
func New(title string) *Notebook {
	nb := &Notebook{
		Metadata: map[string]any{
			"kernelspec": map[string]any{
				"display_name": "Python 3",
				"language":     "python",
				"name":         "python3",
			},
			"language_info": map[string]any{
				"name": "python",
			},
		},
		Nbformat:      4,
		NbformatMinor: 5,
		Cells: []Cell{{
			CellType: "markdown",
			Metadata: map[string]any{},
			Source:   []string{"# " + title + "\n"},
		}},
	}
	return nb
}

// Load reads a .ipynb file and normalizes cell metadata.
func Load(path string) (*Notebook, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	var nb Notebook
	if err := json.Unmarshal(data, &nb); err != nil {
		return nil, err
	}
	nb.Path = path
	nb.Normalize()
	return &nb, nil
}

// Save writes to a .ipynb file.
func (nb *Notebook) Save(path string) error {
	nb.Normalize()
	data, err := json.MarshalIndent(nb, "", "  ")
	if err != nil {
		return err
	}
	// Ensure trailing newline (nbformat convention)
	data = append(data, '\n')
	nb.Path = path
	return os.WriteFile(path, data, 0o600)
}

// Normalize ensures all cells have required nbformat fields.
// Idempotent; safe to call multiple times.
func (nb *Notebook) Normalize() {
	if nb.Metadata == nil {
		nb.Metadata = map[string]any{}
	}
	if nb.Nbformat == 0 {
		nb.Nbformat = 4
	}
	if nb.NbformatMinor == 0 {
		nb.NbformatMinor = 5
	}
	// Ensure kernelspec + language_info
	if _, ok := nb.Metadata["kernelspec"]; !ok {
		nb.Metadata["kernelspec"] = map[string]any{
			"display_name": "Python 3",
			"language":     "python",
			"name":         "python3",
		}
	}
	if _, ok := nb.Metadata["language_info"]; !ok {
		nb.Metadata["language_info"] = map[string]any{"name": "python"}
	}

	for i := range nb.Cells {
		if nb.Cells[i].Metadata == nil {
			nb.Cells[i].Metadata = map[string]any{}
		}
		if nb.Cells[i].CellType == "code" {
			if nb.Cells[i].Outputs == nil {
				nb.Cells[i].Outputs = []Output{}
			}
			// execution_count stays nil (null) unless executed
		}
	}
}

// AddCode appends a code cell with valid nbformat metadata.
func (nb *Notebook) AddCode(source string) {
	nb.Cells = append(nb.Cells, Cell{
		CellType: "code",
		Metadata: map[string]any{},
		Source:   []string{source},
		Outputs:  []Output{},
		// execution_count stays nil (null)
	})
}

// AddMarkdown appends a markdown cell with valid nbformat metadata.
func (nb *Notebook) AddMarkdown(source string) {
	nb.Cells = append(nb.Cells, Cell{
		CellType: "markdown",
		Metadata: map[string]any{},
		Source:   []string{source},
	})
}

// IsAvailable checks if jupyter is on PATH or via common science conda envs.
func IsAvailable() bool {
	if _, err := exec.LookPath("jupyter"); err == nil {
		return true
	}
	candidates := []string{
		os.Getenv("LUMEN_JUPYTER"),
		filepath.Join(os.Getenv("HOME"), ".lumen/science/sandbox/home/.claude-science/conda/envs/operon-mcp/bin/jupyter"),
		"/root/.lumen/science/sandbox/home/.claude-science/conda/envs/operon-mcp/bin/jupyter",
		"/usr/local/bin/jupyter",
	}
	for _, c := range candidates {
		if c == "" {
			continue
		}
		if st, err := os.Stat(c); err == nil && !st.IsDir() {
			return true
		}
	}
	return false
}

// JupyterBin returns the jupyter executable path if available.
func JupyterBin() string {
	if p, err := exec.LookPath("jupyter"); err == nil {
		return p
	}
	candidates := []string{
		os.Getenv("LUMEN_JUPYTER"),
		filepath.Join(os.Getenv("HOME"), ".lumen/science/sandbox/home/.claude-science/conda/envs/operon-mcp/bin/jupyter"),
		"/root/.lumen/science/sandbox/home/.claude-science/conda/envs/operon-mcp/bin/jupyter",
		"/usr/local/bin/jupyter",
	}
	for _, c := range candidates {
		if c == "" {
			continue
		}
		if st, err := os.Stat(c); err == nil && !st.IsDir() {
			return c
		}
	}
	return ""
}

// Execute runs all code cells using jupyter nbconvert --execute.
// Captures both stdout and stderr from nbconvert; error details are included
// in the returned error so the API can surface them.
func (nb *Notebook) Execute(path, python string) error {
	if python == "" {
		python = "python3"
	}
	nb.Normalize()
	if err := nb.Save(path); err != nil {
		return err
	}

	var stderr bytes.Buffer
	var cmd *exec.Cmd
	if jbin := JupyterBin(); jbin != "" {
		cmd = exec.Command(jbin, "nbconvert", "--to", "notebook",
			"--execute", "--inplace", "--ExecutePreprocessor.timeout=120", path)
	} else {
		cmd = exec.Command(python, "-m", "jupyter", "nbconvert", "--to", "notebook",
			"--execute", "--inplace", "--ExecutePreprocessor.timeout=120", path)
	}
	cmd.Stderr = &stderr
	out, err := cmd.Output()
	if err != nil {
		errDetail := stderr.String()
		if errDetail == "" {
			errDetail = string(out)
		}
		if errDetail == "" {
			errDetail = err.Error()
		}
		return fmt.Errorf("%s", strings.TrimSpace(errDetail))
	}
	_ = out // consumed by nbconvert logging; real results are in the reloaded file

	// Reload with outputs
	reloaded, err := Load(path)
	if err != nil {
		return fmt.Errorf("reload after execute: %w", err)
	}
	nb.Cells = reloaded.Cells
	nb.Path = reloaded.Path
	nb.Metadata = reloaded.Metadata
	return nil
}

// ToMarkdown renders notebook as markdown for chat display.
func (nb *Notebook) ToMarkdown() string {
	var b strings.Builder
	for _, c := range nb.Cells {
		switch c.CellType {
		case "markdown":
			for _, s := range c.Source {
				b.WriteString(s)
			}
			b.WriteString("\n\n")
		case "code":
			b.WriteString("```python\n")
			for _, s := range c.Source {
				b.WriteString(s)
			}
			b.WriteString("\n```\n")
			for _, o := range c.Outputs {
				if o.Text != "" {
					b.WriteString("```\n" + o.Text + "\n```\n")
				}
			}
		}
	}
	return b.String()
}

// NotebookInfo is metadata returned by the lab API.
type NotebookInfo struct {
	Name     string    `json:"name"`
	Path     string    `json:"path"`
	Size     int64     `json:"size"`
	Modified time.Time `json:"modified"`
	Cells    int       `json:"cells"`
}

// ListNotebooks returns .ipynb files under a workspace directory.
func ListNotebooks(workspace string) ([]NotebookInfo, error) {
	dir := filepath.Join(workspace, "notebooks")
	entries, err := os.ReadDir(dir)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, err
	}
	var out []NotebookInfo
	for _, e := range entries {
		if e.IsDir() || !strings.HasSuffix(e.Name(), ".ipynb") {
			continue
		}
		info, _ := e.Info()
		ni := NotebookInfo{Name: e.Name(), Path: filepath.Join(dir, e.Name())}
		if info != nil {
			ni.Size = info.Size()
			ni.Modified = info.ModTime()
		}
		if nb, err := Load(ni.Path); err == nil {
			ni.Cells = len(nb.Cells)
		}
		out = append(out, ni)
	}
	return out, nil
}
