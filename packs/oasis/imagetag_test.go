package oasis

import "testing"

// ImageTag must produce the docker reference for build/push/check. The hard part:
// a registry host can carry a colon (127.0.0.1:5000/...) which is NOT a tag, so a
// tag is only the ":" in the FINAL path segment. An already-tagged image is
// returned unchanged (else we'd get the invalid double-tag "repo:1:1").
func TestImageTag(t *testing.T) {
	cases := []struct {
		name    string
		image   string
		version int
		want    string
	}{
		{"untagged with registry port", "127.0.0.1:5000/vo-gnb", 1, "127.0.0.1:5000/vo-gnb:1"},
		{"already tagged keeps tag", "127.0.0.1:5000/vo-gnb:1", 1, "127.0.0.1:5000/vo-gnb:1"},
		{"already tagged latest untouched", "registry.example.com/algo/gnb:latest", 7, "registry.example.com/algo/gnb:latest"},
		{"bare name", "vo-gnb", 2, "vo-gnb:2"},
		{"registry host no port", "ghcr.io/org/algo", 3, "ghcr.io/org/algo:3"},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			if got := ImageTag(c.image, c.version); got != c.want {
				t.Errorf("ImageTag(%q, %d) = %q, want %q", c.image, c.version, got, c.want)
			}
		})
	}
}
