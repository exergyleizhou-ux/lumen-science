package oasis

import "strings"

// ManifestWarnings returns non-fatal advisories about a manifest — things that
// pass Validate but will bite the author later (a confusing `docker push`
// failure, or a weaker provenance record). `lumen oasis validate` prints these
// so they get fixed before deploy.
func ManifestWarnings(m Manifest) []string {
	var w []string
	if strings.Contains(m.Image, "registry.example.com") {
		w = append(w, "image still uses the placeholder registry 'registry.example.com' — set a real registry (e.g. 127.0.0.1:5000/algo/"+m.Name+") in oasis.toml, or `docker push` will fail at deploy")
	}
	if strings.TrimSpace(m.SourceRef) == "" {
		w = append(w, "source_ref is empty — set the git repo URL in oasis.toml so the provenance record (and the buyer's result cert) can point back to the source")
	}
	return w
}
