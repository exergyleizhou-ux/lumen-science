package research

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"
)

// EnsureOrgPack creates org workspace stubs and preferences so all bundled
// research MCP connectors + skills work under virtual OAuth.
func EnsureOrgPack(dataDir, orgUUID string) error {
	if orgUUID == "" {
		return fmt.Errorf("research pack: missing org uuid")
	}
	if err := assertNotSymlink(dataDir); err != nil {
		return err
	}
	orgRoot := filepath.Join(dataDir, "orgs", orgUUID)
	if err := os.MkdirAll(orgRoot, 0o700); err != nil {
		return err
	}
	if err := writePreferences(orgRoot); err != nil {
		return err
	}
	if err := writeMarketplaceManifest(orgRoot, orgUUID); err != nil {
		return err
	}
	if err := ensureWorkspaces(orgRoot); err != nil {
		return err
	}
	if err := os.MkdirAll(filepath.Join(orgRoot, "skills"), 0o700); err != nil {
		return err
	}
	return nil
}

func writePreferences(orgRoot string) error {
	path := filepath.Join(orgRoot, "preferences.json")
	if _, err := os.Stat(path); err == nil {
		return nil
	}
	skip := map[string]bool{}
	for _, id := range SkipMCPApprovalIDs() {
		skip[id] = true
	}
	prefs := map[string]any{
		"userAllowedDomains":             []string{},
		"builtinAllowlistDisabled":       []string{},
		"builtinAllowlistDisabledGroups": []string{},
		"allowlistOnboardingSeen":        true,
		"dashboardSeenSessionsBaselined": true,
		"firstRunOnboardingComplete":     true,
		"installDate":                    time.Now().Format("2006-01-02"),
		"skipMcpApprovals":               skip,
		"ambientBackdrop":                false,
		"allowMcpHeadersHelper":          true,
		"autoMode":                       false,
		"autoModeRules": map[string]any{
			"allow":       []string{},
			"soft_deny":   []string{},
			"environment": []string{},
		},
		"mcpToolGrants": map[string]any{},
		"hostGrants":    []string{},
		"approvalGrants": map[string]any{
			"always": map[string]any{
				"allow": map[string]any{
					"local_exec": []string{"python", "r", "bash"},
					"customize_mutation": []string{
						"agent_create", "agent_update", "skill_publish", "skill_edit",
						"agent_attach_skill", "agent_detach_skill",
						"agent_attach_connector", "agent_detach_connector",
					},
				},
				"deny": map[string]any{},
				"ask":  map[string]any{},
			},
			"project":       map[string]any{},
			"alwaysOrigins": map[string]any{},
		},
		"_migratedToApprovalGrants": true,
		"_bioSplitConsumedKeys":     []string{},
	}
	data, err := json.MarshalIndent(prefs, "", "  ")
	if err != nil {
		return err
	}
	return safeWrite(path, append(data, '\n'), 0o600)
}

func writeMarketplaceManifest(orgRoot, orgUUID string) error {
	dir := filepath.Join(orgRoot, "marketplace-plugins")
	if err := os.MkdirAll(dir, 0o700); err != nil {
		return err
	}
	path := filepath.Join(dir, "manifest.json")
	if _, err := os.Stat(path); err == nil {
		return nil
	}
	manifest := map[string]any{
		"version":              1,
		"lastSuccessfulSyncMs": time.Now().UnixMilli(),
		"orgUuid":              orgUUID,
		"plugins":              []any{},
	}
	data, err := json.MarshalIndent(manifest, "", "  ")
	if err != nil {
		return err
	}
	return safeWrite(path, append(data, '\n'), 0o600)
}

func ensureWorkspaces(orgRoot string) error {
	wsRoot := filepath.Join(orgRoot, "workspaces")
	if err := os.MkdirAll(wsRoot, 0o700); err != nil {
		return err
	}
	for _, name := range WorkspaceDirs {
		if !strings.HasPrefix(name, "_mcp-") {
			continue
		}
		if err := os.MkdirAll(filepath.Join(wsRoot, name, ".cache"), 0o700); err != nil {
			return err
		}
	}
	return os.MkdirAll(filepath.Join(wsRoot, "__byoc_helper__", ".cache"), 0o700)
}

func assertNotSymlink(path string) error {
	fi, err := os.Lstat(path)
	if os.IsNotExist(err) {
		return nil
	}
	if err != nil {
		return err
	}
	if fi.Mode()&os.ModeSymlink != 0 {
		return fmt.Errorf("refuse symlink: %s", path)
	}
	return nil
}

func safeWrite(path string, data []byte, mode os.FileMode) error {
	if err := assertNotSymlink(path); err != nil {
		return err
	}
	tmp := path + ".tmp"
	if err := os.WriteFile(tmp, data, mode); err != nil {
		return err
	}
	if err := os.Rename(tmp, path); err != nil {
		os.Remove(tmp)
		return err
	}
	return os.Chmod(path, mode)
}
