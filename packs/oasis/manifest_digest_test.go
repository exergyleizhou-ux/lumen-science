package oasis

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestSetTOMLStringReplacesExisting(t *testing.T) {
	body := "name = \"algo\"\nimage_digest = \"\"\nversion = 1\n"
	got := setTOMLString(body, "image_digest", "sha256:abc123")
	if !strings.Contains(got, `image_digest = "sha256:abc123"`) {
		t.Errorf("digest not set: %q", got)
	}
	// The other lines must be preserved untouched.
	if !strings.Contains(got, `name = "algo"`) || !strings.Contains(got, "version = 1") {
		t.Errorf("other keys must be preserved, got %q", got)
	}
	// Exactly one image_digest line (replaced, not duplicated).
	if strings.Count(got, "image_digest") != 1 {
		t.Errorf("image_digest must appear once, got %q", got)
	}
}

func TestSetTOMLStringAppendsWhenMissing(t *testing.T) {
	body := "name = \"algo\"\nversion = 1\n"
	got := setTOMLString(body, "image_digest", "sha256:def")
	if !strings.Contains(got, `image_digest = "sha256:def"`) {
		t.Errorf("digest not appended: %q", got)
	}
	if !strings.HasSuffix(got, "\n") {
		t.Errorf("result should end with a single trailing newline: %q", got)
	}
}

func TestSetTOMLStringIgnoresComments(t *testing.T) {
	body := "# image_digest = \"old comment\"\nname = \"algo\"\n"
	got := setTOMLString(body, "image_digest", "sha256:xyz")
	// The comment line must be left intact and a real assignment appended.
	if !strings.Contains(got, `# image_digest = "old comment"`) {
		t.Errorf("comment must be preserved, got %q", got)
	}
	if !strings.Contains(got, `image_digest = "sha256:xyz"`) {
		t.Errorf("real assignment must be appended, got %q", got)
	}
}

func TestUpdateManifestDigestRoundTrip(t *testing.T) {
	dir := t.TempDir()
	raw := "# oasis.toml\nname = \"myalgo\"\nimage = \"registry/myalgo\"\nimage_digest = \"\"\nversion = 2\n"
	if err := os.WriteFile(filepath.Join(dir, ManifestFile), []byte(raw), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := UpdateManifestDigest(dir, "sha256:deadbeef"); err != nil {
		t.Fatalf("UpdateManifestDigest: %v", err)
	}
	out, err := os.ReadFile(filepath.Join(dir, ManifestFile))
	if err != nil {
		t.Fatal(err)
	}
	m, err := ParseManifest(string(out))
	if err != nil {
		t.Fatalf("re-parse: %v", err)
	}
	if m.Digest != "sha256:deadbeef" {
		t.Errorf("digest = %q, want sha256:deadbeef", m.Digest)
	}
	if m.Name != "myalgo" || m.Image != "registry/myalgo" || m.Version != 2 {
		t.Errorf("other fields must survive: name=%q image=%q version=%d", m.Name, m.Image, m.Version)
	}
}

func TestUpdateManifestDigestMissingFile(t *testing.T) {
	if err := UpdateManifestDigest(t.TempDir(), "sha256:x"); err == nil {
		t.Error("missing oasis.toml should error")
	}
}
