// Command skill-migrate converts SKILL.md files to ACP extension descriptors.
//
// Usage:
//
//	skill-migrate --source DIR --output DIR [--dry-run]
//
// The tool walks the source directory for SKILL.md files, extracts metadata
// (title, description, license), and writes ACP skill.json descriptors to
// the output directory.
package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"strings"
)

// SkillDescriptor is the ACP extension descriptor format.
type SkillDescriptor struct {
	SkillID      string   `json:"skill_id"`
	DisplayName  string   `json:"display_name"`
	Version      string   `json:"version"`
	License      string   `json:"license"`
	Category     string   `json:"category"`
	Description  string   `json:"description"`
	Tools        []string `json:"tools"`
	EntryPoint   string   `json:"entry_point"`
	Protocol     string   `json:"protocol"`
	Dependencies []string `json:"dependencies"`
	DataSources  []string `json:"data_sources"`
	Admission    struct {
		Status     string `json:"status"`
		ReviewedAt string `json:"reviewed_at"`
	} `json:"admission"`
}

// Known license patterns to detect.
var licensePatterns = map[string]*regexp.Regexp{
	"MIT":      regexp.MustCompile(`(?i)\bMIT\b`),
	"Apache-2.0": regexp.MustCompile(`(?i)\bApache[ -]2\.0\b`),
	"BSD-3-Clause": regexp.MustCompile(`(?i)\bBSD[ -]3[ -]Clause\b`),
	"BSD-2-Clause": regexp.MustCompile(`(?i)\bBSD[ -]2[ -]Clause\b`),
	"CC-BY-4.0": regexp.MustCompile(`(?i)\bCC[ -]BY[ -]4\.0\b`),
	"CC0":       regexp.MustCompile(`(?i)\bCC0\b`),
	"GPL-3.0":   regexp.MustCompile(`(?i)\bGPL[ -]?3\b`),
	"GPL-2.0":   regexp.MustCompile(`(?i)\bGPL[ -]?2\b`),
}

// Rejected licenses (GPL).
var rejectedLicenses = map[string]bool{
	"GPL-3.0": true,
	"GPL-2.0": true,
}

// Category detection from path.
var categoryMap = map[string]string{
	"biology":    "biology",
	"chemistry":  "chemistry",
	"physics":    "physics",
	"math":       "math",
	"stats":      "analysis",
	"ml":         "analysis",
	"data":       "analysis",
	"viz":        "visualization",
	"visualization": "visualization",
	"pubmed":     "research",
	"literature": "research",
	"review":     "quality",
	"audit":      "quality",
	"compute":    "compute",
	"c2d":        "compute",
}

func main() {
	source := flag.String("source", "", "source directory containing SKILL.md files")
	output := flag.String("output", "", "output directory for skill.json files")
	dryRun := flag.Bool("dry-run", false, "print what would be done without writing")
	flag.Parse()

	if *source == "" || *output == "" {
		fmt.Fprintln(os.Stderr, "usage: skill-migrate --source DIR --output DIR [--dry-run]")
		os.Exit(2)
	}

	var descriptors []SkillDescriptor
	rejected := 0
	pending := 0
	approved := 0

	err := filepath.WalkDir(*source, func(path string, d os.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if d.IsDir() || d.Name() != "SKILL.md" {
			return nil
		}
		content, err := os.ReadFile(path)
		if err != nil {
			fmt.Fprintf(os.Stderr, "WARN: cannot read %s: %v\n", path, err)
			return nil
		}
		desc := parseSkill(path, string(content))
		if desc == nil {
			return nil
		}

		// Determine admission status
		license := detectLicense(string(content))
		desc.License = license
		if rejectedLicenses[license] {
			desc.Admission.Status = "rejected"
			rejected++
			fmt.Fprintf(os.Stderr, "REJECTED (GPL): %s\n", desc.SkillID)
			return nil
		}
		if license == "unknown" {
			desc.Admission.Status = "pending"
			pending++
		} else {
			desc.Admission.Status = "approved"
			approved++
		}

		descriptors = append(descriptors, *desc)
		return nil
	})

	if err != nil {
		fmt.Fprintf(os.Stderr, "ERROR: walk failed: %v\n", err)
		os.Exit(1)
	}

	fmt.Printf("Skills: %d approved, %d pending, %d rejected\n", approved, pending, rejected)

	if *dryRun {
		for _, desc := range descriptors {
			data, _ := json.MarshalIndent(desc, "", "  ")
			fmt.Printf("--- %s ---\n%s\n", desc.SkillID, data)
		}
		return
	}

	if err := os.MkdirAll(*output, 0o755); err != nil {
		fmt.Fprintf(os.Stderr, "ERROR: mkdir output: %v\n", err)
		os.Exit(1)
	}

	registry := map[string]interface{}{
		"schema_version": 1,
		"skills":         descriptors,
		"rejected_count": rejected,
		"pending_count":  pending,
	}

	data, err := json.MarshalIndent(registry, "", "  ")
	if err != nil {
		fmt.Fprintf(os.Stderr, "ERROR: marshal registry: %v\n", err)
		os.Exit(1)
	}

	outputPath := filepath.Join(*output, "registry.json")
	if err := os.WriteFile(outputPath, data, 0o644); err != nil {
		fmt.Fprintf(os.Stderr, "ERROR: write registry: %v\n", err)
		os.Exit(1)
	}
	fmt.Printf("Wrote registry to %s (%d skills)\n", outputPath, len(descriptors))
}

func parseSkill(path, content string) *SkillDescriptor {
	relPath := strings.TrimPrefix(path, filepath.Dir(path))
	relPath = strings.TrimPrefix(relPath, "/")

	// Extract category from path
	category := "general"
	for _, part := range strings.Split(filepath.Dir(relPath), string(filepath.Separator)) {
		if c, ok := categoryMap[strings.ToLower(part)]; ok {
			category = c
		}
	}

	// Extract title from first # heading
	titleRe := regexp.MustCompile(`(?m)^#\s+(.+)$`)
	titleMatch := titleRe.FindStringSubmatch(content)
	displayName := filepath.Base(filepath.Dir(path))
	if titleMatch != nil {
		displayName = strings.TrimSpace(titleMatch[1])
	}

	// Extract description from first paragraph after title
	descRe := regexp.MustCompile(`(?m)^#\s+.+\n\n(.+)`)
	descMatch := descRe.FindStringSubmatch(content)
	description := ""
	if descMatch != nil {
		description = strings.TrimSpace(descMatch[1])
		if len(description) > 200 {
			description = description[:200] + "..."
		}
	}

	// Generate skill_id from path
	skillID := strings.TrimSuffix(relPath, "/SKILL.md")
	skillID = strings.ReplaceAll(skillID, "/", "-")
	skillID = strings.ToLower(skillID)
	if !strings.HasPrefix(skillID, "science/") {
		skillID = "science/" + skillID
	}

	return &SkillDescriptor{
		SkillID:     skillID,
		DisplayName: displayName,
		Version:     "1.0.0",
		Category:    category,
		Description: description,
		EntryPoint:  "SKILL.md",
		Protocol:    "ACP-extension",
		Admission: struct {
			Status     string `json:"status"`
			ReviewedAt string `json:"reviewed_at"`
		}{Status: "pending", ReviewedAt: "2026-07-25"},
	}
}

func detectLicense(content string) string {
	for name, pattern := range licensePatterns {
		if pattern.MatchString(content) {
			return name
		}
	}
	return "unknown"
}
