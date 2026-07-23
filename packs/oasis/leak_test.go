package oasis

import (
	"archive/zip"
	"bytes"
	"context"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"testing"
)

// echoSandbox simulates a LEAKY algorithm: it dumps the seeded dataset verbatim
// into the output instead of emitting aggregates — exactly the privacy failure
// the leak lint must catch.
type echoSandbox struct{}

func (echoSandbox) Run(ctx context.Context, image, dataDir, paramsFile, outDir string) (string, error) {
	raw, _ := os.ReadFile(filepath.Join(dataDir, "dataset.csv"))
	var buf bytes.Buffer
	zw := zip.NewWriter(&buf)
	w, _ := zw.Create("model.json")
	w.Write([]byte(`{"rows": ` + strconv.Quote(string(raw)) + `}`))
	zw.Close()
	os.WriteFile(filepath.Join(outDir, "output.bin"), buf.Bytes(), 0o644)
	return "", nil
}

func TestCheckContract_FlagsRawRowLeak(t *testing.T) {
	ctx := context.Background()

	// A clean aggregate output (no seeded sentinels) must pass the leak lint.
	if r := CheckContract(ctx, fakeSandbox{writeBin: makeModelZip(t, true)}, "img", "model", nil); !r.OK {
		t.Errorf("aggregate output should pass the leak lint, got %v", r.Violations)
	}

	// An algorithm that echoes the raw seeded rows must be flagged.
	r := CheckContract(ctx, echoSandbox{}, "img", "model", nil)
	if r.OK {
		t.Fatal("an algorithm that dumps raw rows must be flagged as a privacy leak")
	}
	found := false
	for _, v := range r.Violations {
		lv := strings.ToLower(v)
		if strings.Contains(lv, "leak") || strings.Contains(lv, "raw") {
			found = true
		}
	}
	if !found {
		t.Errorf("expected a privacy-leak violation, got %v", r.Violations)
	}
}

// When the AUTHOR supplies their own sample data, the leak lint is skipped (we
// can't seed sentinels into arbitrary data), so it must not false-positive.
func TestCheckContract_AuthorDataSkipsLeakLint(t *testing.T) {
	ctx := context.Background()
	r := CheckContract(ctx, echoSandbox{}, "img", "model", []byte("a,b\n1,2\n3,4\n"))
	if !r.OK {
		t.Errorf("with author-supplied data the leak lint is skipped; got %v", r.Violations)
	}
}
