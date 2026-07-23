package oasis

import (
	"archive/zip"
	"bytes"
	"context"
	"errors"
	"os"
	"path/filepath"
	"testing"
)

// makeModelZip builds a model output.bin: a zip with model.json (+ metrics.json).
func makeModelZip(t *testing.T, includeModel bool) []byte {
	t.Helper()
	var buf bytes.Buffer
	zw := zip.NewWriter(&buf)
	if includeModel {
		w, _ := zw.Create("model.json")
		w.Write([]byte(`{"format":"demo","k":3}`))
	}
	w, _ := zw.Create("metrics.json")
	w.Write([]byte(`{"accuracy":0.9}`))
	zw.Close()
	return buf.Bytes()
}

func TestCheckOutput(t *testing.T) {
	modelZip := makeModelZip(t, true)
	zipNoModel := makeModelZip(t, false)

	cases := []struct {
		name    string
		data    []byte
		kind    string
		wantErr bool
	}{
		{"valid model zip", modelZip, "model", false},
		{"empty output", nil, "model", true},
		{"empty bytes", []byte{}, "model", true},
		{"model but not a zip", []byte("not a zip at all"), "model", true},
		{"model zip missing model.json", zipNoModel, "model", true},
		{"aggregate kind accepts any non-empty blob", []byte("123.45"), "aggregate", false},
		{"bytes kind accepts any non-empty blob", []byte{0x00, 0x01, 0x02}, "bytes", false},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			errs := CheckOutput(c.data, c.kind)
			if c.wantErr && len(errs) == 0 {
				t.Errorf("expected a violation for %q (kind=%s)", c.name, c.kind)
			}
			if !c.wantErr && len(errs) != 0 {
				t.Errorf("expected no violation for %q (kind=%s), got %v", c.name, c.kind, errs)
			}
		})
	}
}

type fakeSandbox struct {
	writeBin []byte // written to <outDir>/output.bin when non-nil
	logs     string
	err      error
}

func (f fakeSandbox) Run(ctx context.Context, image, dataDir, paramsFile, outDir string) (string, error) {
	if f.writeBin != nil {
		os.WriteFile(filepath.Join(outDir, "output.bin"), f.writeBin, 0o644)
	}
	return f.logs, f.err
}

func TestCheckContract(t *testing.T) {
	ctx := context.Background()
	modelZip := makeModelZip(t, true)

	if r := CheckContract(ctx, fakeSandbox{writeBin: modelZip}, "img", "model", nil); !r.OK {
		t.Errorf("a compliant algorithm should pass, got %v", r.Violations)
	}
	if r := CheckContract(ctx, fakeSandbox{err: errors.New("exit status 1"), logs: "boom"}, "img", "model", nil); r.OK {
		t.Error("a run that errors under isolation must be a violation")
	}
	if r := CheckContract(ctx, fakeSandbox{writeBin: nil}, "img", "model", nil); r.OK {
		t.Error("not writing /out/output.bin must be a violation")
	}
	if r := CheckContract(ctx, fakeSandbox{writeBin: []byte("not a zip")}, "img", "model", nil); r.OK {
		t.Error("a model output.bin that is not a valid zip must be a violation")
	}
}
