package lab

import (
	"embed"
	"io/fs"
	"os"
	"path/filepath"
)

//go:embed static/*
var staticFS embed.FS

//go:embed runtime/skills/science/*
var elevationSkillsFS embed.FS

// SeedElevationSkills copies embedded elevation skills into the lab skills directory.
// Safe to call on every start — existing files are not overwritten.
func SeedElevationSkills(sciDir string) error {
	entries, err := fs.ReadDir(elevationSkillsFS, "runtime/skills/science")
	if err != nil {
		return err
	}
	skillsDir := filepath.Join(sciDir, "skills")
	_ = os.MkdirAll(skillsDir, 0o700)
	for _, e := range entries {
		if e.IsDir() {
			continue
		}
		dst := filepath.Join(skillsDir, e.Name())
		if _, err := os.Stat(dst); err == nil {
			continue // already exists, don't overwrite
		}
		data, err := fs.ReadFile(elevationSkillsFS, "runtime/skills/science/"+e.Name())
		if err != nil {
			continue
		}
		_ = os.WriteFile(dst, data, 0o644)
	}
	return nil
}
