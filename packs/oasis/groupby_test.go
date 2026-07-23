package oasis

import (
	"archive/zip"
	"bytes"
	"encoding/json"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"testing"
)

// The groupby template must enforce k-anonymity: groups with fewer than k rows
// are SUPPRESSED (never emitted), so small groups can't be re-identified.
func TestGroupbyTemplate_SuppressesSmallGroups(t *testing.T) {
	py, err := exec.LookPath("python3")
	if err != nil {
		t.Skip("python3 not in PATH")
	}
	tmpl, ok := TemplateByName("groupby")
	if !ok {
		t.Fatal("groupby template should exist")
	}
	dir := t.TempDir()
	if err := ScaffoldTemplate(dir, DefaultManifest("g"), tmpl); err != nil {
		t.Fatal(err)
	}
	// city A has 5 rows (>= k=5, kept), city B has 1 row (< k, suppressed).
	data, out := t.TempDir(), t.TempDir()
	csv := "city,spend\nA,10\nA,20\nA,30\nA,40\nA,50\nB,999\n"
	os.WriteFile(filepath.Join(data, "d.csv"), []byte(csv), 0o644)
	cmd := exec.Command(py, filepath.Join(dir, "train.py"))
	cmd.Env = append(os.Environ(),
		"VO_DATA_DIR="+data, "VO_OUT_DIR="+out,
		"VO_PARAMS="+writeParams(t, `{"by":"city","min_group_size":5}`))
	if o, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("groupby run failed: %v\n%s", err, o)
	}

	model := readZipEntry(t, filepath.Join(out, "output.bin"), "model.json")
	var m struct {
		Groups map[string]map[string]float64 `json:"groups"`
	}
	if err := json.Unmarshal(model, &m); err != nil {
		t.Fatalf("model.json: %v\n%s", err, model)
	}
	if _, ok := m.Groups["A"]; !ok {
		t.Errorf("group A (5 rows >= k) should be present, got groups %v", m.Groups)
	}
	if _, ok := m.Groups["B"]; ok {
		t.Errorf("group B (1 row < k=5) must be SUPPRESSED for k-anonymity, but it leaked: %v", m.Groups)
	}
	// And the suppressed singleton value must not appear anywhere in the output.
	full := readZipEntry(t, filepath.Join(out, "output.bin"), "model.json")
	if bytes.Contains(full, []byte("999")) {
		t.Error("the suppressed group's raw value (999) leaked into the output")
	}
}

func writeParams(t *testing.T, body string) string {
	t.Helper()
	p := filepath.Join(t.TempDir(), "params.json")
	if err := os.WriteFile(p, []byte(body), 0o644); err != nil {
		t.Fatal(err)
	}
	return p
}

func readZipEntry(t *testing.T, zipPath, name string) []byte {
	t.Helper()
	raw, err := os.ReadFile(zipPath)
	if err != nil {
		t.Fatal(err)
	}
	zr, err := zip.NewReader(bytes.NewReader(raw), int64(len(raw)))
	if err != nil {
		t.Fatalf("output.bin not a zip: %v", err)
	}
	for _, f := range zr.File {
		if f.Name == name {
			rc, _ := f.Open()
			defer rc.Close()
			b, _ := io.ReadAll(rc)
			return b
		}
	}
	t.Fatalf("%s not found in output.bin", name)
	return nil
}
