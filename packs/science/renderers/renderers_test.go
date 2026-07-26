package renderers

// Path confinement for the artifact routes.
//
// The property under test: no combination of the three query-supplied path
// components can read a file outside the artifacts root. Before the fix,
// `path=../../<anything>` walked out of the root and /render/api/raw served
// the bytes — these tests fail against that version (CodeQL
// go/path-injection).

import (
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"testing"
)

func newTestServer(t *testing.T) (*httptest.Server, string) {
	t.Helper()
	root := t.TempDir()

	// A legitimate artifact inside the root…
	inside := filepath.Join(root, "proj1", "run1")
	if err := os.MkdirAll(inside, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(inside, "ok.txt"), []byte("artifact"), 0o644); err != nil {
		t.Fatal(err)
	}
	// …and a secret OUTSIDE it, in the parent, which must stay unreachable.
	if err := os.WriteFile(filepath.Join(filepath.Dir(root), "secret.txt"), []byte("secret"), 0o644); err != nil {
		t.Fatal(err)
	}

	mux := http.NewServeMux()
	NewServer(root).RegisterRoutes(mux)
	ts := httptest.NewServer(mux)
	t.Cleanup(ts.Close)
	return ts, root
}

func get(t *testing.T, url string) int {
	t.Helper()
	resp, err := http.Get(url)
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	return resp.StatusCode
}

func TestRawServesArtifactsInsideTheRoot(t *testing.T) {
	ts, _ := newTestServer(t)
	code := get(t, ts.URL+"/render/api/raw?project=proj1&run=run1&path=ok.txt")
	if code != http.StatusOK {
		t.Fatalf("a legitimate artifact must be served, got %d", code)
	}
}

func TestRawRefusesEveryEscapeShape(t *testing.T) {
	ts, _ := newTestServer(t)
	// Escapes via each of the three components, absolute paths, and a
	// mid-path traversal that a naive prefix-of-uncleaned-string check misses.
	escapes := []string{
		"/render/api/raw?project=proj1&run=run1&path=../../secret.txt",
		"/render/api/raw?project=..&run=..&path=secret.txt",
		"/render/api/raw?project=proj1&run=..%2F..&path=secret.txt",
		"/render/api/raw?project=proj1&run=run1&path=%2Fetc%2Fpasswd",
		"/render/api/raw?project=proj1&run=run1&path=a%2F..%2F..%2F..%2Fsecret.txt",
		"/render/api/artifact?project=..&run=.&path=secret.txt",
	}
	for _, u := range escapes {
		code := get(t, ts.URL+u)
		if code == http.StatusOK {
			t.Fatalf("escape served instead of refused: %s", u)
		}
	}
}

func TestRootItselfIsNotAServableArtifact(t *testing.T) {
	ts, _ := newTestServer(t)
	// project/run/path that clean down to the bare root: a directory read
	// would fail anyway, but the CONFINEMENT must be what refuses it.
	code := get(t, ts.URL+"/render/api/raw?project=.&run=.&path=.")
	if code == http.StatusOK {
		t.Fatal("the artifacts root itself must not be servable")
	}
}
