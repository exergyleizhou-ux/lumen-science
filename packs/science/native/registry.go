package native

import (
	"os"
	"os/exec"
	"path/filepath"
)

// FleetMember describes one native MCP binary in the Science fleet.
type FleetMember struct {
	ID          string   `json:"id"`
	Label       string   `json:"label"`
	Binary      string   `json:"binary"`
	Command     string   `json:"command"`
	Args        []string `json:"args"`
	Status      string   `json:"status,omitempty"` // planned | shipped | stub
	Description string   `json:"description,omitempty"`
}

// NativeFleetVersion is the shipped native MCP fleet schema version.
const NativeFleetVersion = "1.0.0"

// DefaultFleet is the native MCP roster for Oasis Science workbench.
func DefaultFleet() []FleetMember {
	return []FleetMember{
		member("pubmed", "PubMed 文献", "lumen-mcp-pubmed", "LUMEN_MCP_PUBMED", "shipped",
			"search_articles, fetch_abstract, fetch_pmc_fulltext"),
		member("chembl", "ChEMBL 化合物", "lumen-mcp-chembl", "LUMEN_MCP_CHEMBL", "shipped",
			"search_compounds, get_molecule, get_target_bioactivities"),
		member("oasis", "绿洲数据集", "lumen-mcp-oasis", "LUMEN_MCP_OASIS", "shipped",
			"anonymous metadata + user-token preview/certificate (auth combo 1+3)"),
		member("c2d", "C2D 算法沙箱", "lumen-mcp-c2d", "LUMEN_MCP_C2D", "shipped",
			"list_offer_signals (anon) + list_algorithms/submit_job (user token)"),
		member("geo", "GEO 表达数据", "lumen-mcp-geo", "LUMEN_MCP_GEO", "shipped",
			"search_datasets, get_dataset (anonymous NCBI GEO)"),
	}
}

func member(id, label, binName, envKey, status, desc string) FleetMember {
	bin := resolveBinary(binName)
	cmd, args := bin, []string(nil)
	if override := os.Getenv(envKey); override != "" {
		cmd, args = override, nil
	}
	return FleetMember{
		ID: id, Label: label, Binary: bin, Command: cmd, Args: args,
		Status: status, Description: desc,
	}
}

// ShippedFleet returns fleet members with status shipped.
func ShippedFleet() []FleetMember {
	var out []FleetMember
	for _, m := range DefaultFleet() {
		if m.Status == "shipped" {
			out = append(out, m)
		}
	}
	return out
}

// ShippedFleetIDs returns IDs of all shipped fleet members (stable ordering).
func ShippedFleetIDs() []string {
	shipped := ShippedFleet()
	ids := make([]string, len(shipped))
	for i, m := range shipped {
		ids[i] = m.ID
	}
	return ids
}

func resolveBinary(name string) string {
	if p, err := exec.LookPath(name); err == nil {
		return p
	}
	exe, err := os.Executable()
	if err != nil {
		return name
	}
	candidate := filepath.Join(filepath.Dir(exe), name)
	if _, err := os.Stat(candidate); err == nil {
		return candidate
	}
	return name
}
