package oasis

import (
	"archive/zip"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

func TestTemplateByName(t *testing.T) {
	if _, ok := TemplateByName("correlation"); !ok {
		t.Error("correlation template should exist")
	}
	if _, ok := TemplateByName("nope"); ok {
		t.Error("unknown template should not resolve")
	}
	if len(Templates()) < 3 {
		t.Errorf("expected at least 3 templates (stats/correlation/linreg), got %d", len(Templates()))
	}
}

// Every template must be a COMPLETE, working example — valid Python, following
// the C2D contract, with no TODO left for the author to fill in.
func TestTemplatesAreCompleteAndValid(t *testing.T) {
	py, err := exec.LookPath("python3")
	if err != nil {
		t.Skip("python3 not in PATH")
	}
	for _, tmpl := range Templates() {
		t.Run(tmpl.Key, func(t *testing.T) {
			dir := t.TempDir()
			if err := ScaffoldTemplate(dir, DefaultManifest("demo"), tmpl); err != nil {
				t.Fatal(err)
			}
			train := filepath.Join(dir, "train.py")
			if out, err := exec.Command(py, "-m", "py_compile", train).CombinedOutput(); err != nil {
				t.Fatalf("%s: train.py is not valid python: %v\n%s", tmpl.Key, err, out)
			}
			src, _ := os.ReadFile(train)
			s := string(src)
			for _, want := range []string{"/data", "/params.json", "/out", "output.bin", "model.json", "metrics.json"} {
				if !strings.Contains(s, want) {
					t.Errorf("%s: train.py missing C2D contract ref %q", tmpl.Key, want)
				}
			}
			if strings.Contains(s, "TODO") {
				t.Errorf("%s: a template must be a working example, not a TODO skeleton", tmpl.Key)
			}
		})
	}
}

// The decisive test: each template actually RUNS on a sample dataset and emits a
// contract-valid output.bin (a zip of model.json + metrics.json) — aggregates only.
func TestTemplatesRunAndProduceValidOutput(t *testing.T) {
	py, err := exec.LookPath("python3")
	if err != nil {
		t.Skip("python3 not in PATH")
	}
	for _, tmpl := range Templates() {
		t.Run(tmpl.Key, func(t *testing.T) {
			dir := t.TempDir()
			if err := ScaffoldTemplate(dir, DefaultManifest("demo"), tmpl); err != nil {
				t.Fatal(err)
			}
			data, out := t.TempDir(), t.TempDir()
			os.WriteFile(filepath.Join(data, "d.csv"),
				[]byte("a,b,c\n1,2,3\n4,5,6\n7,8,9\n2,1,4\n5,3,1\n"), 0o644)
			cmd := exec.Command(py, filepath.Join(dir, "train.py"))
			cmd.Env = append(os.Environ(), "VO_DATA_DIR="+data, "VO_OUT_DIR="+out, "VO_PARAMS=/nonexistent-params")
			if o, err := cmd.CombinedOutput(); err != nil {
				t.Fatalf("%s: run failed: %v\n%s", tmpl.Key, err, o)
			}
			zr, err := zip.OpenReader(filepath.Join(out, "output.bin"))
			if err != nil {
				t.Fatalf("%s: output.bin is not a valid zip: %v", tmpl.Key, err)
			}
			defer zr.Close()
			has := map[string]bool{}
			for _, f := range zr.File {
				has[f.Name] = true
			}
			if !has["model.json"] || !has["metrics.json"] {
				t.Errorf("%s: output.bin must contain model.json + metrics.json, got %v", tmpl.Key, has)
			}
		})
	}
}
