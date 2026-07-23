package oasis

import (
	"strings"
	"testing"
)

func TestManifestWarnings_PlaceholderRegistry(t *testing.T) {
	m := DefaultManifest("logreg") // image = registry.example.com/algo/logreg
	w := ManifestWarnings(m)
	if !hasWarning(w, "registry.example.com") {
		t.Errorf("expected a placeholder-registry warning, got %v", w)
	}
}

func TestManifestWarnings_RealRegistryNoWarning(t *testing.T) {
	m := DefaultManifest("logreg")
	m.Image = "127.0.0.1:5000/algo/logreg"
	m.SourceRef = "https://github.com/me/logreg"
	if w := ManifestWarnings(m); len(w) != 0 {
		t.Errorf("a real, fully-specified manifest should warn nothing, got %v", w)
	}
}

// Provenance nudge: an empty source_ref weakens the verifiable record.
func TestManifestWarnings_MissingSourceRef(t *testing.T) {
	m := DefaultManifest("logreg")
	m.Image = "127.0.0.1:5000/algo/logreg" // fix the registry so only source_ref is at issue
	w := ManifestWarnings(m)
	if !hasWarning(w, "source_ref") {
		t.Errorf("expected a source_ref provenance warning, got %v", w)
	}
}

func hasWarning(ws []string, substr string) bool {
	for _, w := range ws {
		if strings.Contains(w, substr) {
			return true
		}
	}
	return false
}
