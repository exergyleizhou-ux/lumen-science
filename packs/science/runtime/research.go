package runtime

import (
	"encoding/json"
	"os"
	"path/filepath"

	"lumen/internal/science/paths"
	"lumen/internal/science/research"
)

// ResearchReport scans sandbox for all research assets.
func ResearchReport(sciDir string) (research.Report, error) {
	return research.Scan(paths.DataDir(sciDir))
}

// ListResearchCatalog returns full MCP/skills/domains manifest.
func ListResearchCatalog(sciDir string) map[string]any {
	return research.Catalog(paths.DataDir(sciDir))
}

// ReseedResearchPack re-applies org workspaces/preferences (non-destructive).
func ReseedResearchPack(sciDir string) error {
	dataDir := paths.DataDir(sciDir)
	org, err := activeOrgUUID(dataDir)
	if err != nil {
		return err
	}
	return research.EnsureOrgPack(dataDir, org)
}

func activeOrgUUID(dataDir string) (string, error) {
	data, err := os.ReadFile(filepath.Join(dataDir, "active-org.json"))
	if err != nil {
		return "", err
	}
	var m map[string]string
	if err := json.Unmarshal(data, &m); err != nil {
		return "", err
	}
	if m["org_uuid"] == "" {
		return "", os.ErrNotExist
	}
	return m["org_uuid"], nil
}
