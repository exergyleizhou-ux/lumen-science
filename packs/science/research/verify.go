package research

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
)

// Report summarizes all research assets in a sandbox data-dir.
type Report struct {
	RuntimePresent   bool        `json:"runtime_present"`
	RuntimeVersion   string      `json:"runtime_version,omitempty"`
	CloneAssets      []string    `json:"clone_assets_present"`
	MCPServers       []MCPServer `json:"mcp_servers"`
	BioLibPackages   int         `json:"bio_lib_packages"`
	DomainMCPServers int         `json:"domain_mcp_servers"`
	Domains          []Domain    `json:"domains"`
	TotalDomainTools int         `json:"total_domain_tools"`
	Skills           []Skill     `json:"skills"`
	SeedExamples     []string    `json:"seed_examples"`
	OrgPackSeeded    bool        `json:"org_pack_seeded"`
	Workspaces       int         `json:"workspaces"`
	MissingSkills    []string    `json:"missing_skills,omitempty"`
}

// Scan inspects cloned Science research assets under dataDir.
func Scan(dataDir string) (Report, error) {
	r := Report{}
	for _, asset := range CloneAssets {
		if st, err := os.Stat(filepath.Join(dataDir, asset)); err == nil && st.IsDir() {
			r.CloneAssets = append(r.CloneAssets, asset)
		}
	}
	rtRoot, ver, err := findRuntimeRoot(dataDir)
	if err != nil {
		return r, fmt.Errorf("runtime not cloned — run lumen science start once: %w", err)
	}
	r.RuntimePresent = true
	r.RuntimeVersion = ver
	r.MCPServers = discoverMCPServers(rtRoot)
	r.BioLibPackages, r.DomainMCPServers = countBioLib(rtRoot)
	r.Domains = domainsFromRuntime(rtRoot)
	for _, d := range r.Domains {
		r.TotalDomainTools += d.ToolCount
	}
	r.Skills = discoverSkills(rtRoot)
	r.SeedExamples = discoverSeedExamples(dataDir, rtRoot)

	known := map[string]bool{}
	for _, sk := range r.Skills {
		known[sk.ID] = true
	}
	for _, sk := range KnownSkills {
		if !known[sk.ID] {
			r.MissingSkills = append(r.MissingSkills, sk.ID)
		}
	}

	orgUUID, _ := readActiveOrg(dataDir)
	if orgUUID != "" {
		orgRoot := filepath.Join(dataDir, "orgs", orgUUID)
		if st, err := os.Stat(filepath.Join(orgRoot, "preferences.json")); err == nil && !st.IsDir() {
			r.OrgPackSeeded = true
		}
		if entries, err := os.ReadDir(filepath.Join(orgRoot, "workspaces")); err == nil {
			for _, e := range entries {
				if e.IsDir() && strings.HasPrefix(e.Name(), "_mcp-") {
					r.Workspaces++
				}
			}
		}
	}
	return r, nil
}

// Healthy returns true when the full research stack is present.
func (r Report) Healthy() bool {
	return len(r.CloneAssets) >= 3 &&
		r.RuntimePresent &&
		r.BioLibPackages >= 40 &&
		r.DomainMCPServers >= 5 &&
		len(r.Skills) >= 20 &&
		r.TotalDomainTools >= 100
}

// Catalog returns a static+dynamic manifest for CLI/GUI listing.
func Catalog(dataDir string) map[string]any {
	cat := map[string]any{
		"clone_assets":    CloneAssets,
		"mcp_servers":     TopLevelMCPServers,
		"bundled_mcp_ids": BundledMCPIDs,
		"workspaces":      WorkspaceDirs,
		"seed_examples":   SeedExamples,
		"skills":          KnownSkills,
		"note":            "本地 runtime 包含全部科研 MCP/技能/示例；Anthropic 托管远程 MCP 由 bio-tools 本地替代。",
	}
	if dataDir != "" {
		if rep, err := Scan(dataDir); err == nil {
			cat["discovered"] = rep
		}
	}
	if dataDir != "" {
		if rtRoot, _, err := findRuntimeRoot(dataDir); err == nil {
			if domains := domainsFromRuntime(rtRoot); len(domains) > 0 {
				cat["domains"] = domains
			}
		}
	}
	if _, ok := cat["domains"]; !ok {
		// fallback labels only
		var domains []Domain
		for slug, label := range DomainLabels {
			domains = append(domains, Domain{Slug: slug, Label: label, BundledID: "bundled:" + slug})
		}
		cat["domains"] = domains
	}
	return cat
}

func readActiveOrg(dataDir string) (string, error) {
	data, err := os.ReadFile(filepath.Join(dataDir, "active-org.json"))
	if err != nil {
		return "", err
	}
	var m map[string]string
	if err := json.Unmarshal(data, &m); err != nil {
		return "", err
	}
	return m["org_uuid"], nil
}
