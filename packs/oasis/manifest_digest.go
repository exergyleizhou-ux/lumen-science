package oasis

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
)

// ManifestFile is the algorithm manifest filename.
const ManifestFile = "oasis.toml"

// UpdateManifestDigest pins the resolved registry image digest into oasis.toml,
// preserving every other line and comment. It is a line-level edit, NOT a
// re-render: formatTOML reorders keys and drops comments, which would scramble
// the author's hand-written manifest. If an image_digest assignment exists its
// value is replaced; otherwise the assignment is appended.
func UpdateManifestDigest(dir, digest string) error {
	path := filepath.Join(dir, ManifestFile)
	raw, err := os.ReadFile(path)
	if err != nil {
		return err
	}
	return os.WriteFile(path, []byte(setTOMLString(string(raw), "image_digest", digest)), 0o644)
}

// setTOMLString sets `key = "value"` in a flat TOML body, replacing an existing
// assignment (matched on the key left of `=`, ignoring comment lines) or
// appending one with a single trailing newline.
func setTOMLString(body, key, value string) string {
	newline := fmt.Sprintf("%s = %q", key, value)
	lines := strings.Split(body, "\n")
	for i, line := range lines {
		trimmed := strings.TrimSpace(line)
		if trimmed == "" || strings.HasPrefix(trimmed, "#") {
			continue
		}
		if eq := strings.Index(trimmed, "="); eq >= 0 {
			if strings.TrimSpace(trimmed[:eq]) == key {
				lines[i] = newline
				return strings.Join(lines, "\n")
			}
		}
	}
	return strings.TrimRight(body, "\n") + "\n" + newline + "\n"
}
