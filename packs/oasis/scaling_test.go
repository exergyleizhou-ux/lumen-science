package oasis

import (
	"archive/zip"
	"bytes"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestSentinelDatasetN(t *testing.T) {
	data, sentinels := sentinelDatasetN(5)
	if len(sentinels) != 5 {
		t.Fatalf("want 5 sentinels, got %d", len(sentinels))
	}
	lines := strings.Split(strings.TrimRight(string(data), "\n"), "\n")
	if len(lines) != 6 { // 1 header + 5 data rows
		t.Fatalf("want 6 lines (header + 5), got %d: %q", len(lines), data)
	}
	if lines[0] != "v,w,x,_sentinel" {
		t.Errorf("unexpected header %q", lines[0])
	}
	seen := map[string]bool{}
	for _, s := range sentinels {
		if seen[s] {
			t.Errorf("sentinel tokens must be unique, saw %q twice", s)
		}
		seen[s] = true
	}
}

func TestScalingViolation(t *testing.T) {
	cases := []struct {
		name          string
		small, large  int
		wantViolation bool
	}{
		{"zero small size cannot be judged", 0, 1000, false},
		{"constant output is fine", 200, 205, false},
		{"modest 1.5x growth is within aggregate noise", 200, 300, false},
		{"output that grew >2x leaks per-row data", 200, 420, true},
		{"output that scaled ~10x with the rows is a clear leak", 200, 2000, true},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			if got := scalingViolation(c.small, c.large); got != c.wantViolation {
				t.Errorf("scalingViolation(%d, %d) = %v, want %v", c.small, c.large, got, c.wantViolation)
			}
		})
	}
}

// derivedLeakSandbox simulates the subtler leak the echo lint cannot catch: it
// emits a per-row DERIVED value (a SHA-256 of each row) rather than the literal
// seeded token. The output carries no sentinel token (echo lint stays silent)
// yet its size scales with the row count (the scaling lint must catch it).
type derivedLeakSandbox struct{}

func (derivedLeakSandbox) Run(ctx context.Context, image, dataDir, paramsFile, outDir string) (string, error) {
	data, _ := os.ReadFile(filepath.Join(dataDir, "dataset.csv"))
	lines := strings.Split(strings.TrimRight(string(data), "\n"), "\n")
	var buf bytes.Buffer
	zw := zip.NewWriter(&buf)
	w, _ := zw.Create("model.json")
	var rows strings.Builder
	for i := 1; i < len(lines); i++ { // skip header
		sum := sha256.Sum256([]byte(lines[i]))
		rows.WriteString(hex.EncodeToString(sum[:]))
		rows.WriteByte('\n')
	}
	w.Write([]byte(rows.String()))
	mw, _ := zw.Create("metrics.json")
	mw.Write([]byte(`{"n":"derived"}`))
	zw.Close()
	os.WriteFile(filepath.Join(outDir, "output.bin"), buf.Bytes(), 0o644)
	return "", nil
}

// aggregatorSandbox emits a fixed-size aggregate regardless of the input rows.
type aggregatorSandbox struct{}

func (aggregatorSandbox) Run(ctx context.Context, image, dataDir, paramsFile, outDir string) (string, error) {
	var buf bytes.Buffer
	zw := zip.NewWriter(&buf)
	w, _ := zw.Create("model.json")
	w.Write([]byte(`{"format":"demo","mean":12.34,"std":5.6}`))
	mw, _ := zw.Create("metrics.json")
	mw.Write([]byte(`{"accuracy":0.91}`))
	zw.Close()
	os.WriteFile(filepath.Join(outDir, "output.bin"), buf.Bytes(), 0o644)
	return "", nil
}

func TestCheckContractRowScaling(t *testing.T) {
	ctx := context.Background()

	// An aggregate output is invariant to N → no scaling violation.
	if r := CheckContract(ctx, aggregatorSandbox{}, "img", "model", nil); !r.OK {
		t.Errorf("an aggregator should pass, got %v", r.Violations)
	}

	// A per-row leak grows with N → must be flagged, even though it never echoes
	// a literal seeded token (so the echo lint alone would miss it).
	r := CheckContract(ctx, derivedLeakSandbox{}, "img", "model", nil)
	if r.OK {
		t.Fatal("output scaling with the row count must be flagged as a privacy leak")
	}
	found := false
	for _, v := range r.Violations {
		if strings.Contains(strings.ToLower(v), "scale") || strings.Contains(strings.ToLower(v), "grew") {
			found = true
		}
		if strings.Contains(v, "echoes") {
			t.Errorf("echo lint should NOT fire on derived (non-literal) leak: %q", v)
		}
	}
	if !found {
		t.Errorf("expected a row-scaling privacy violation, got %v", r.Violations)
	}
}
