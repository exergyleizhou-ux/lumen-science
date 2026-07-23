package lab

import (
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"
	"testing"
)

// TestLabUIPureHelpersJS loads shipped static/app.js via Node and asserts
// escHtml / renderMarkdown / reduceSSE behavior (no re-implementation in Go).
func TestLabUIPureHelpersJS(t *testing.T) {
	node, err := exec.LookPath("node")
	if err != nil {
		if bun, berr := exec.LookPath("bun"); berr == nil {
			node = bun
		} else {
			t.Skip("node/bun not in PATH; required to exercise shipped LabUI JS")
		}
	}

	_, thisFile, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatal("runtime.Caller failed")
	}
	dir := filepath.Dir(thisFile)
	testJS := filepath.Join(dir, "static", "labui_test.mjs")
	if _, err := os.Stat(testJS); err != nil {
		t.Fatalf("missing %s: %v", testJS, err)
	}
	appJS := filepath.Join(dir, "static", "app.js")
	if _, err := os.Stat(appJS); err != nil {
		t.Fatalf("missing shipped app.js: %v", err)
	}

	// Run twice as verification plan requires; assert evidence lines each time.
	var combined strings.Builder
	for i := 1; i <= 2; i++ {
		cmd := exec.Command(node, testJS)
		cmd.Dir = filepath.Dir(testJS)
		out, err := cmd.CombinedOutput()
		combined.WriteString("=== go-test node invocation " + strconv.Itoa(i) + " ===\n")
		combined.Write(out)
		combined.WriteByte('\n')
		if err != nil {
			t.Fatalf("labui_test.mjs run %d failed: %v\n%s", i, err, out)
		}
		text := string(out)
		needles := []string{
			"OK escHtml",
			"OK renderMarkdown bold/list/code",
			"OK HTML escaped before markdown",
			"OK tool_dispatch keyed by id",
			"OK tool_result merges by id",
			"OK approval_request shape",
			"PASS labui_test.mjs",
		}
		for _, n := range needles {
			if !strings.Contains(text, n) {
				t.Fatalf("run %d missing evidence %q\n%s", i, n, text)
			}
		}
	}
	t.Logf("LabUI JS pure helpers OK (2 node runs)\n%s", combined.String())
}
