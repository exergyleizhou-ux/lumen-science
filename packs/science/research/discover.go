package research

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
)

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

func loadDomainsJSON(rtRoot string) (map[string][]string, error) {
	path := filepath.Join(rtRoot, "mcp-servers", "bio-tools", "lib", "mcp_bio", "domains.json")
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	var domains map[string][]string
	if err := json.Unmarshal(data, &domains); err != nil {
		return nil, err
	}
	return domains, nil
}

func domainsFromRuntime(rtRoot string) []Domain {
	raw, err := loadDomainsJSON(rtRoot)
	if err != nil {
		return nil
	}
	slugs := make([]string, 0, len(raw))
	for s := range raw {
		slugs = append(slugs, s)
	}
	sortStrings(slugs)
	out := make([]Domain, 0, len(slugs))
	for _, slug := range slugs {
		tools := raw[slug]
		label := DomainLabels[slug]
		if label == "" {
			label = slug
		}
		d := Domain{
			Slug:      slug,
			Label:     label,
			BundledID: "bundled:" + slug,
			ToolCount: len(tools),
		}
		if len(tools) > 0 {
			n := 3
			if len(tools) < n {
				n = len(tools)
			}
			d.Highlights = append([]string(nil), tools[:n]...)
		}
		out = append(out, d)
	}
	return out
}

func discoverSkills(rtRoot string) []Skill {
	dir := filepath.Join(rtRoot, "skills")
	entries, err := os.ReadDir(dir)
	if err != nil {
		return nil
	}
	known := map[string]Skill{}
	for _, sk := range KnownSkills {
		known[sk.ID] = sk
	}
	var out []Skill
	for _, e := range entries {
		if !e.IsDir() {
			continue
		}
		id := e.Name()
		if sk, ok := known[id]; ok {
			out = append(out, sk)
			continue
		}
		desc := parseSkillDescription(filepath.Join(dir, id, "SKILL.md"))
		out = append(out, Skill{ID: id, Label: id, Category: "discovered", Description: desc})
	}
	sortSkills(out)
	return out
}

func parseSkillDescription(path string) string {
	data, err := os.ReadFile(path)
	if err != nil {
		return ""
	}
	text := string(data)
	for _, line := range strings.Split(text, "\n") {
		line = strings.TrimSpace(line)
		if strings.HasPrefix(line, "description:") {
			return strings.TrimSpace(strings.TrimPrefix(line, "description:"))
		}
		if strings.HasPrefix(line, "description: >") {
			return ""
		}
	}
	return ""
}

func discoverMCPServers(rtRoot string) []MCPServer {
	dir := filepath.Join(rtRoot, "mcp-servers")
	entries, err := os.ReadDir(dir)
	if err != nil {
		return nil
	}
	known := map[string]MCPServer{}
	for _, s := range TopLevelMCPServers {
		known[s.ID] = s
	}
	var out []MCPServer
	for _, e := range entries {
		if !e.IsDir() {
			continue
		}
		id := e.Name()
		if s, ok := known[id]; ok {
			out = append(out, s)
		} else {
			out = append(out, MCPServer{ID: id, Label: id})
		}
	}
	return out
}

func countBioLib(rtRoot string) (packages, mcpServers int) {
	lib := filepath.Join(rtRoot, "mcp-servers", "bio-tools", "lib")
	entries, err := os.ReadDir(lib)
	if err != nil {
		return 0, 0
	}
	for _, e := range entries {
		if !e.IsDir() {
			continue
		}
		packages++
		if st, err := os.Stat(filepath.Join(lib, e.Name(), "server.py")); err == nil && !st.IsDir() {
			mcpServers++
		}
	}
	return packages, mcpServers
}

func discoverSeedExamples(dataDir, rtRoot string) []string {
	seen := map[string]bool{}
	var out []string
	for _, base := range []string{
		filepath.Join(dataDir, "seed-assets"),
		filepath.Join(rtRoot, "seed"),
	} {
		entries, err := os.ReadDir(base)
		if err != nil {
			continue
		}
		for _, e := range entries {
			name := e.Name()
			if strings.HasPrefix(name, "example_") || strings.HasPrefix(name, "manifest_") {
				key := strings.TrimPrefix(name, "manifest_")
				key = strings.TrimSuffix(key, ".json")
				key = strings.TrimSuffix(key, ".tar.gz")
				if key != "" && !seen[key] {
					seen[key] = true
					out = append(out, key)
				}
			}
		}
	}
	sortStrings(out)
	return out
}

func sortStrings(s []string) {
	for i := 1; i < len(s); i++ {
		for j := i; j > 0 && s[j] < s[j-1]; j-- {
			s[j], s[j-1] = s[j-1], s[j]
		}
	}
}

func sortSkills(s []Skill) {
	for i := 1; i < len(s); i++ {
		for j := i; j > 0 && s[j].ID < s[j-1].ID; j-- {
			s[j], s[j-1] = s[j-1], s[j]
		}
	}
}
