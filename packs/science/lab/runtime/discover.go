package runtime

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"

	"lumen/internal/science/paths"
	"lumen/internal/science/research"
)

// DomainInfo describes one bio-tools domain from the research pack.
type DomainInfo struct {
	Slug      string   `json:"slug"`
	Label     string   `json:"label"`
	Package   string   `json:"package"`
	ToolCount int      `json:"tool_count"`
	Tools     []string `json:"tools,omitempty"`
}

// Discover loads domain metadata from the cloned CS research pack.
func Discover(sciDir string) ([]DomainInfo, string, error) {
	dataDir := paths.DataDir(sciDir)
	rtRoot, _, err := findRuntimeRoot(dataDir)
	if err != nil {
		return nil, "", err
	}
	domainsPath := filepath.Join(rtRoot, "mcp-servers", "bio-tools", "lib", "mcp_bio", "domains.json")
	raw, err := os.ReadFile(domainsPath)
	if err != nil {
		return nil, "", err
	}
	var domains map[string][]string
	if err := json.Unmarshal(raw, &domains); err != nil {
		return nil, "", err
	}
	bioTools := filepath.Join(rtRoot, "mcp-servers", "bio-tools")
	var out []DomainInfo
	for slug, tools := range domains {
		label := research.DomainLabels[slug]
		if label == "" {
			label = slug
		}
		out = append(out, DomainInfo{
			Slug:      slug,
			Label:     label,
			Package:   domainPackage(slug),
			ToolCount: len(tools),
			Tools:     tools,
		})
	}
	sortDomains(out)
	return out, bioTools, nil
}

func domainPackage(slug string) string {
	return "mcp_" + strings.ReplaceAll(slug, "-", "_")
}

func findRuntimeRoot(dataDir string) (string, string, error) {
	rt := filepath.Join(dataDir, "runtime")
	entries, err := os.ReadDir(rt)
	if err != nil {
		return "", "", err
	}
	for _, e := range entries {
		if e.IsDir() {
			return filepath.Join(rt, e.Name()), e.Name(), nil
		}
	}
	return "", "", os.ErrNotExist
}

// SkillsDir returns runtime/skills from the cloned research pack, if present.
func SkillsDir(sciDir string) string {
	dataDir := paths.DataDir(sciDir)
	rtRoot, _, err := findRuntimeRoot(dataDir)
	if err != nil {
		return ""
	}
	dir := filepath.Join(rtRoot, "skills")
	if st, err := os.Stat(dir); err != nil || !st.IsDir() {
		return ""
	}
	return dir
}

func sortDomains(d []DomainInfo) {
	for i := 1; i < len(d); i++ {
		for j := i; j > 0 && d[j].Slug < d[j-1].Slug; j-- {
			d[j], d[j-1] = d[j-1], d[j]
		}
	}
}
