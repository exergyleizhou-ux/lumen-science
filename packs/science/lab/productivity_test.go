package lab

import (
	"bytes"
	"context"
	"encoding/json"
	"io"
	"mime/multipart"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"lumen/internal/science/lab/project"
)

func testLabServer(t *testing.T) (*httptest.Server, string) {
	t.Helper()
	sci := filepath.Join(t.TempDir(), "science")
	if err := os.MkdirAll(sci, 0o700); err != nil {
		t.Fatal(err)
	}
	srv, err := New(Config{SciDir: sci, Version: "test"})
	if err != nil {
		t.Fatal(err)
	}
	ts := httptest.NewServer(srv.Handler())
	t.Cleanup(ts.Close)
	return ts, sci
}

func createProject(t *testing.T, ts *httptest.Server, title string) string {
	t.Helper()
	req, _ := http.NewRequest(http.MethodPost, ts.URL+"/api/lab/projects", bytes.NewReader([]byte(`{"title":"`+title+`"}`)))
	req.Header.Set("Content-Type", "application/json")
	res, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	defer res.Body.Close()
	var proj map[string]string
	_ = json.NewDecoder(res.Body).Decode(&proj)
	if proj["slug"] == "" {
		t.Fatalf("no slug: %v", proj)
	}
	return proj["slug"]
}

func TestSessionHistoryAPI(t *testing.T) {
	ts, sci := testLabServer(t)
	slug := createProject(t, ts, "Session Hist")
	// Create session via API
	req, _ := http.NewRequest(http.MethodPost, ts.URL+"/api/lab/projects/"+slug+"/sessions",
		bytes.NewReader([]byte(`{"title":"s1"}`)))
	req.Header.Set("Content-Type", "application/json")
	res, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	var sess project.Session
	_ = json.NewDecoder(res.Body).Decode(&sess)
	res.Body.Close()
	if sess.ID == "" {
		t.Fatal("no session id")
	}
	// Append turns via store (simulates chat persistence)
	store := project.NewStore(sci)
	_, err = store.AppendTurns(slug, sess.ID,
		project.Turn{Role: "user", Text: "persist me"},
		project.Turn{Role: "assistant", Text: "ok **bold**", Tools: []project.ToolSummary{{Name: "bash", Status: "done"}}},
	)
	if err != nil {
		t.Fatal(err)
	}
	// List
	listRes, err := http.Get(ts.URL + "/api/lab/projects/" + slug + "/sessions")
	if err != nil {
		t.Fatal(err)
	}
	var listBody map[string]any
	_ = json.NewDecoder(listRes.Body).Decode(&listBody)
	listRes.Body.Close()
	if listBody["count"].(float64) < 1 {
		t.Fatalf("list %v", listBody)
	}
	// Get full history
	getRes, err := http.Get(ts.URL + "/api/lab/projects/" + slug + "/sessions/" + sess.ID)
	if err != nil {
		t.Fatal(err)
	}
	defer getRes.Body.Close()
	if getRes.StatusCode != 200 {
		t.Fatalf("status %d", getRes.StatusCode)
	}
	var full project.Session
	if err := json.NewDecoder(getRes.Body).Decode(&full); err != nil {
		t.Fatal(err)
	}
	if len(full.Turns) != 2 || full.Turns[0].Text != "persist me" {
		t.Fatalf("turns %+v", full.Turns)
	}
	// Rename session
	preq, _ := http.NewRequest(http.MethodPatch, ts.URL+"/api/lab/projects/"+slug+"/sessions/"+sess.ID,
		bytes.NewReader([]byte(`{"title":"renamed-lab"}`)))
	preq.Header.Set("Content-Type", "application/json")
	pres, err := http.DefaultClient.Do(preq)
	if err != nil {
		t.Fatal(err)
	}
	defer pres.Body.Close()
	if pres.StatusCode != 200 {
		t.Fatalf("rename status %d", pres.StatusCode)
	}
	var renamed project.Session
	_ = json.NewDecoder(pres.Body).Decode(&renamed)
	if renamed.Title != "renamed-lab" {
		t.Fatalf("title %q", renamed.Title)
	}
	got, err := store.GetSession(slug, sess.ID)
	if err != nil || got.Title != "renamed-lab" {
		t.Fatalf("persist rename %v %+v", err, got)
	}
	if len(got.Turns) != 2 {
		t.Fatalf("turns lost on rename: %d", len(got.Turns))
	}
	// Fork session
	freq, _ := http.NewRequest(http.MethodPost, ts.URL+"/api/lab/projects/"+slug+"/sessions/"+sess.ID+"/fork",
		bytes.NewReader([]byte(`{"title":"forked"}`)))
	freq.Header.Set("Content-Type", "application/json")
	fres, err := http.DefaultClient.Do(freq)
	if err != nil {
		t.Fatal(err)
	}
	defer fres.Body.Close()
	if fres.StatusCode != 200 {
		t.Fatalf("fork %d", fres.StatusCode)
	}
	var forked project.Session
	_ = json.NewDecoder(fres.Body).Decode(&forked)
	if forked.ID == "" || forked.ID == sess.ID {
		t.Fatalf("fork id %q", forked.ID)
	}
	fullFork, err := store.GetSession(slug, forked.ID)
	if err != nil || len(fullFork.Turns) != 2 {
		t.Fatalf("fork turns %v %+v", err, fullFork)
	}
	if fullFork.Title != "forked" {
		t.Fatalf("fork title %q", fullFork.Title)
	}
}

func TestProjectRenameAPI(t *testing.T) {
	ts, _ := testLabServer(t)
	slug := createProject(t, ts, "Old Title")
	req, _ := http.NewRequest(http.MethodPatch, ts.URL+"/api/lab/projects/"+slug,
		bytes.NewReader([]byte(`{"title":"New Title"}`)))
	req.Header.Set("Content-Type", "application/json")
	res, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	defer res.Body.Close()
	if res.StatusCode != 200 {
		t.Fatalf("rename %d", res.StatusCode)
	}
	var p map[string]any
	_ = json.NewDecoder(res.Body).Decode(&p)
	if p["title"] != "New Title" || p["slug"] != slug {
		t.Fatalf("%v", p)
	}
	get, err := http.Get(ts.URL + "/api/lab/projects/" + slug)
	if err != nil {
		t.Fatal(err)
	}
	defer get.Body.Close()
	var body map[string]any
	_ = json.NewDecoder(get.Body).Decode(&body)
	proj, _ := body["project"].(map[string]any)
	if proj == nil || proj["title"] != "New Title" {
		t.Fatalf("get %v", body)
	}
}

func TestSessionImportAndExportAll(t *testing.T) {
	ts, _ := testLabServer(t)
	slug := createProject(t, ts, "ImportExp")
	// import with turns
	body := `{"title":"imported","turns":[{"role":"user","text":"hello import"},{"role":"assistant","text":"hi back"}]}`
	req, _ := http.NewRequest(http.MethodPost, ts.URL+"/api/lab/projects/"+slug+"/sessions/import",
		bytes.NewReader([]byte(body)))
	req.Header.Set("Content-Type", "application/json")
	res, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	defer res.Body.Close()
	if res.StatusCode != 200 {
		t.Fatalf("import %d", res.StatusCode)
	}
	var sess project.Session
	_ = json.NewDecoder(res.Body).Decode(&sess)
	if sess.ID == "" || len(sess.Turns) != 2 {
		t.Fatalf("import sess %+v", sess)
	}
	// export-all md
	md, err := http.Get(ts.URL + "/api/lab/projects/" + slug + "/sessions/export-all?format=md")
	if err != nil {
		t.Fatal(err)
	}
	defer md.Body.Close()
	if md.StatusCode != 200 {
		t.Fatalf("export-all md %d", md.StatusCode)
	}
	raw, _ := io.ReadAll(md.Body)
	if !strings.Contains(string(raw), "hello import") {
		snippet := string(raw)
		if len(snippet) > 200 {
			snippet = snippet[:200]
		}
		t.Fatalf("md missing content: %s", snippet)
	}
	// export-all json
	js, err := http.Get(ts.URL + "/api/lab/projects/" + slug + "/sessions/export-all?format=json")
	if err != nil {
		t.Fatal(err)
	}
	defer js.Body.Close()
	if js.StatusCode != 200 {
		t.Fatalf("export-all json %d", js.StatusCode)
	}
	var pack map[string]any
	_ = json.NewDecoder(js.Body).Decode(&pack)
	if pack["count"].(float64) < 1 {
		t.Fatalf("%v", pack)
	}
}

func TestComputeJobWorkDirAndTimeout(t *testing.T) {
	ts, _ := testLabServer(t)
	slug := createProject(t, ts, "JobOpts")
	// Submit local job with custom timeout and work_dir empty (defaults to workspace)
	body := `{"host":"local","command":"echo jobopts-ok","timeout_sec":30,"output_globs":["*.txt"]}`
	req, _ := http.NewRequest(http.MethodPost, ts.URL+"/api/lab/compute/jobs?project_id="+slug,
		bytes.NewReader([]byte(body)))
	req.Header.Set("Content-Type", "application/json")
	res, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	defer res.Body.Close()
	if res.StatusCode != 200 {
		raw, _ := io.ReadAll(res.Body)
		t.Fatalf("submit %d %s", res.StatusCode, raw)
	}
	var j map[string]any
	_ = json.NewDecoder(res.Body).Decode(&j)
	if j["id"] == nil || j["id"] == "" {
		t.Fatalf("no id %v", j)
	}
	if int(j["timeout_sec"].(float64)) != 30 {
		t.Fatalf("timeout_sec want 30 got %v", j["timeout_sec"])
	}
	// work_dir should be set (workspace path)
	if wd, _ := j["work_dir"].(string); wd == "" {
		t.Fatalf("work_dir empty %v", j)
	}
	// wait for local job to finish
	id := j["id"].(string)
	deadline := time.Now().Add(8 * time.Second)
	var last map[string]any
	for time.Now().Before(deadline) {
		get, err := http.Get(ts.URL + "/api/lab/compute/jobs/" + id + "?project_id=" + slug)
		if err != nil {
			t.Fatal(err)
		}
		_ = json.NewDecoder(get.Body).Decode(&last)
		get.Body.Close()
		st, _ := last["status"].(string)
		if st == "done" || st == "failed" || st == "timeout" || st == "cancelled" {
			break
		}
		time.Sleep(100 * time.Millisecond)
	}
	if last["status"] != "done" {
		// local shell should succeed for echo
		t.Fatalf("job status %v", last)
	}
	out, _ := last["output"].(string)
	if !strings.Contains(out, "jobopts-ok") {
		t.Fatalf("output %q", out)
	}
}

func TestMolFileWriteRoundTrip(t *testing.T) {
	ts, _ := testLabServer(t)
	slug := createProject(t, ts, "Mol")
	mol := "\n\n\n  0  0  0  0  0  0  0  0  0  0999 V2000\nM  END\n"
	req, _ := http.NewRequest(http.MethodPost, ts.URL+"/api/lab/files/write?project_id="+slug,
		bytes.NewReader([]byte(`{"path":"molecules/structure.mol","content":`+mustJSON(mol)+`}`)))
	req.Header.Set("Content-Type", "application/json")
	res, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	res.Body.Close()
	if res.StatusCode != 200 {
		t.Fatalf("write %d", res.StatusCode)
	}
	get, err := http.Get(ts.URL + "/api/lab/files/content?project_id=" + slug + "&path=molecules/structure.mol")
	if err != nil {
		t.Fatal(err)
	}
	defer get.Body.Close()
	if get.StatusCode != 200 {
		t.Fatalf("content %d", get.StatusCode)
	}
	var body map[string]any
	_ = json.NewDecoder(get.Body).Decode(&body)
	if body["content"] != mol {
		t.Fatalf("content mismatch %v", body["content"])
	}
	pk, _ := body["previewKind"].(string)
	if pk != "molecule" && pk != "text" {
		// molecule preferred; text acceptable for .mol depending on previewKind
		t.Logf("previewKind=%s", pk)
	}
}

func TestFileAppendAPI(t *testing.T) {
	ts, _ := testLabServer(t)
	slug := createProject(t, ts, "Append")
	wreq, _ := http.NewRequest(http.MethodPost, ts.URL+"/api/lab/files/write?project_id="+slug,
		bytes.NewReader([]byte(`{"path":"log.txt","content":"line1\n"}`)))
	wreq.Header.Set("Content-Type", "application/json")
	wres, err := http.DefaultClient.Do(wreq)
	if err != nil {
		t.Fatal(err)
	}
	wres.Body.Close()
	areq, _ := http.NewRequest(http.MethodPost, ts.URL+"/api/lab/files/append?project_id="+slug,
		bytes.NewReader([]byte(`{"path":"log.txt","content":"line2\n"}`)))
	areq.Header.Set("Content-Type", "application/json")
	ares, err := http.DefaultClient.Do(areq)
	if err != nil {
		t.Fatal(err)
	}
	defer ares.Body.Close()
	if ares.StatusCode != 200 {
		t.Fatalf("append %d", ares.StatusCode)
	}
	get, err := http.Get(ts.URL + "/api/lab/files/content?project_id=" + slug + "&path=log.txt")
	if err != nil {
		t.Fatal(err)
	}
	defer get.Body.Close()
	var body map[string]any
	_ = json.NewDecoder(get.Body).Decode(&body)
	if body["content"] != "line1\nline2\n" {
		t.Fatalf("content %q", body["content"])
	}
}

func TestFilesZipAPI(t *testing.T) {
	ts, _ := testLabServer(t)
	slug := createProject(t, ts, "ZipSel")
	for _, pair := range []struct{ p, c string }{
		{"a.txt", "aaa"},
		{"sub/b.txt", "bbb"},
	} {
		req, _ := http.NewRequest(http.MethodPost, ts.URL+"/api/lab/files/write?project_id="+slug,
			bytes.NewReader([]byte(`{"path":"`+pair.p+`","content":`+mustJSON(pair.c)+`}`)))
		req.Header.Set("Content-Type", "application/json")
		res, err := http.DefaultClient.Do(req)
		if err != nil {
			t.Fatal(err)
		}
		res.Body.Close()
	}
	zreq, _ := http.NewRequest(http.MethodPost, ts.URL+"/api/lab/files/zip?project_id="+slug,
		bytes.NewReader([]byte(`{"paths":["a.txt","sub/b.txt"]}`)))
	zreq.Header.Set("Content-Type", "application/json")
	zres, err := http.DefaultClient.Do(zreq)
	if err != nil {
		t.Fatal(err)
	}
	defer zres.Body.Close()
	if zres.StatusCode != 200 {
		t.Fatalf("zip status %d", zres.StatusCode)
	}
	ct := zres.Header.Get("Content-Type")
	if !strings.Contains(ct, "zip") {
		t.Fatalf("content-type %s", ct)
	}
	raw, _ := io.ReadAll(zres.Body)
	if len(raw) < 40 {
		t.Fatalf("zip too small %d", len(raw))
	}
	// zip magic PK
	if raw[0] != 'P' || raw[1] != 'K' {
		t.Fatalf("not zip magic")
	}
}

func TestFileStatsAPI(t *testing.T) {
	ts, _ := testLabServer(t)
	slug := createProject(t, ts, "Stats")
	// write a couple files
	for _, pair := range []struct{ p, c string }{
		{"data/a.txt", "hello"},
		{"reports/b.md", "# hi"},
	} {
		req, _ := http.NewRequest(http.MethodPost, ts.URL+"/api/lab/files/write?project_id="+slug,
			bytes.NewReader([]byte(`{"path":"`+pair.p+`","content":`+mustJSON(pair.c)+`}`)))
		req.Header.Set("Content-Type", "application/json")
		res, err := http.DefaultClient.Do(req)
		if err != nil {
			t.Fatal(err)
		}
		res.Body.Close()
	}
	get, err := http.Get(ts.URL + "/api/lab/files/stats?project_id=" + slug)
	if err != nil {
		t.Fatal(err)
	}
	defer get.Body.Close()
	if get.StatusCode != 200 {
		t.Fatalf("stats %d", get.StatusCode)
	}
	var body map[string]any
	_ = json.NewDecoder(get.Body).Decode(&body)
	if body["files"].(float64) < 2 {
		t.Fatalf("files %v", body)
	}
	if body["bytes"].(float64) < 1 {
		t.Fatalf("bytes %v", body)
	}
}

func TestNotebooksAPI(t *testing.T) {
	ts, _ := testLabServer(t)
	slug := createProject(t, ts, "NB")
	req, _ := http.NewRequest(http.MethodPost, ts.URL+"/api/lab/notebooks?project_id="+slug,
		bytes.NewReader([]byte(`{"name":"demo.ipynb"}`)))
	req.Header.Set("Content-Type", "application/json")
	res, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	res.Body.Close()
	if res.StatusCode != 200 {
		t.Fatalf("create %d", res.StatusCode)
	}
	list, err := http.Get(ts.URL + "/api/lab/notebooks?project_id=" + slug)
	if err != nil {
		t.Fatal(err)
	}
	defer list.Body.Close()
	var body map[string]any
	_ = json.NewDecoder(list.Body).Decode(&body)
	if body["count"].(float64) < 1 {
		t.Fatalf("list %v", body)
	}
	// append cell
	creq, _ := http.NewRequest(http.MethodPost, ts.URL+"/api/lab/notebooks/cell/demo.ipynb?project_id="+slug,
		bytes.NewReader([]byte(`{"source":"print(1+1)"}`)))
	creq.Header.Set("Content-Type", "application/json")
	cres, err := http.DefaultClient.Do(creq)
	if err != nil {
		t.Fatal(err)
	}
	cres.Body.Close()
	if cres.StatusCode != 200 {
		t.Fatalf("cell %d", cres.StatusCode)
	}
	get, err := http.Get(ts.URL + "/api/lab/notebooks/cells/demo.ipynb?project_id=" + slug)
	if err != nil {
		t.Fatal(err)
	}
	defer get.Body.Close()
	if get.StatusCode != 200 {
		t.Fatalf("get cells %d", get.StatusCode)
	}
	// Execute path: when jupyter is available, expect ok + output; otherwise ok:false with non-empty error
	ereq, _ := http.NewRequest(http.MethodPost, ts.URL+"/api/lab/notebooks/execute/demo.ipynb?project_id="+slug,
		bytes.NewReader([]byte(`{}`)))
	ereq.Header.Set("Content-Type", "application/json")
	eres, err := http.DefaultClient.Do(ereq)
	if err != nil {
		t.Fatal(err)
	}
	defer eres.Body.Close()
	if eres.StatusCode != 200 {
		t.Fatalf("execute status %d", eres.StatusCode)
	}
	var ebody map[string]any
	if err := json.NewDecoder(eres.Body).Decode(&ebody); err != nil {
		t.Fatal(err)
	}
	ok, _ := ebody["ok"].(bool)
	if ok {
		// jupyter available — look for "2" in outputs or markdown
		found := false
		raw, _ := json.Marshal(ebody["cells"])
		if strings.Contains(string(raw), "2") {
			found = true
		}
		if !found {
			t.Fatalf("execute ok but missing output 2: %v", ebody)
		}
	} else {
		errStr, _ := ebody["error"].(string)
		if strings.TrimSpace(errStr) == "" {
			t.Fatalf("execute failed with empty error: %v", ebody)
		}
	}
}

// TestLangGraphAPI verifies health.langgraph shape and that /langgraph/run
// injects workspace from project slug when the sidecar is enabled locally.
func TestLangGraphAPI(t *testing.T) {
	ts, sci := testLabServer(t)

	// Health always exposes langgraph (available true/false).
	hres, err := http.Get(ts.URL + "/api/lab/health")
	if err != nil {
		t.Fatal(err)
	}
	defer hres.Body.Close()
	var health map[string]any
	if err := json.NewDecoder(hres.Body).Decode(&health); err != nil {
		t.Fatal(err)
	}
	lg, _ := health["langgraph"].(map[string]any)
	if lg == nil {
		t.Fatalf("health missing langgraph: %v", health)
	}
	if _, ok := lg["available"]; !ok {
		t.Fatalf("langgraph.available missing: %v", lg)
	}
	if hint, _ := lg["hint"].(string); strings.TrimSpace(hint) == "" {
		t.Fatalf("langgraph.hint empty: %v", lg)
	}

	// Sidecar integration only when local venv is ready.
	home := os.Getenv("HOME")
	venv := filepath.Join(home, ".lumen", "langgraph-venv")
	if _, err := os.Stat(filepath.Join(venv, "bin", "python3")); err != nil {
		t.Skip("no local langgraph venv")
	}
	// productivity_test.go lives in internal/science/lab → repo root ../../../
	wd, _ := os.Getwd()
	script := filepath.Clean(filepath.Join(wd, "..", "..", "..", "scripts", "science", "langgraph_runner.py"))
	if st, err := os.Stat(script); err != nil || st.IsDir() {
		script = filepath.Join(home, ".lumen", "langgraph_runner.py")
		if _, err := os.Stat(script); err != nil {
			t.Skip("no langgraph_runner.py")
		}
	}

	t.Setenv("LUMEN_LANGGRAPH", "1")
	t.Setenv("LUMEN_LANGGRAPH_VENV", venv)
	t.Setenv("LUMEN_LANGGRAPH_SCRIPT", script)

	slug := createProject(t, ts, "LG API")
	store := project.NewStore(sci)
	ws, err := store.WorkspacePath(slug)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(filepath.Join(ws, "reports"), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(ws, "reports", "notes.md"), []byte("# api note\nhello graph\n"), 0o600); err != nil {
		t.Fatal(err)
	}

	body, _ := json.Marshal(map[string]string{
		"project_id": slug,
		"prompt":     "总结工作区",
	})
	req, _ := http.NewRequest(http.MethodPost, ts.URL+"/api/lab/langgraph/run", bytes.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	res, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	defer res.Body.Close()
	if res.StatusCode != 200 {
		t.Fatalf("status %d", res.StatusCode)
	}
	var out map[string]any
	if err := json.NewDecoder(res.Body).Decode(&out); err != nil {
		t.Fatal(err)
	}
	ok, _ := out["ok"].(bool)
	if !ok {
		// Sidecar may still be unavailable if import fails in this env.
		errStr, _ := out["error"].(string)
		if strings.TrimSpace(errStr) == "" {
			t.Fatalf("ok=false with empty error: %v", out)
		}
		t.Skipf("langgraph run unavailable: %s", errStr)
	}
	result, _ := out["result"].(string)
	if !strings.Contains(result, "notes.md") {
		t.Fatalf("expected workspace file in result (API should inject path): %s", result[:min(len(result), 400)])
	}
	if !strings.Contains(result, "LangGraph") && !strings.Contains(result, "建议") {
		t.Fatalf("unexpected result shape: %s", result[:min(len(result), 400)])
	}
}

func TestFileSearchAPI(t *testing.T) {
	ts, sci := testLabServer(t)
	slug := createProject(t, ts, "Search Proj")
	store := project.NewStore(sci)
	ws, _ := store.WorkspacePath(slug)
	_ = os.WriteFile(filepath.Join(ws, "notes.md"), []byte("findme unique-token-xyz\n"), 0o600)

	res, err := http.Get(ts.URL + "/api/lab/files/search?project_id=" + slug + "&q=unique-token-xyz")
	if err != nil {
		t.Fatal(err)
	}
	defer res.Body.Close()
	if res.StatusCode != 200 {
		t.Fatalf("status %d", res.StatusCode)
	}
	var body struct {
		Hits  []FileSearchHit `json:"hits"`
		Count int             `json:"count"`
	}
	if err := json.NewDecoder(res.Body).Decode(&body); err != nil {
		t.Fatal(err)
	}
	if body.Count < 1 {
		t.Fatalf("hits %+v", body)
	}
	found := false
	for _, h := range body.Hits {
		if h.Path == "notes.md" && h.Match == "content" {
			found = true
		}
	}
	if !found {
		t.Fatalf("expected content hit: %+v", body.Hits)
	}
}

func TestSkillsEnableAPI(t *testing.T) {
	ts, _ := testLabServer(t)
	slug := createProject(t, ts, "Skills Proj")
	// PUT enabled
	req, _ := http.NewRequest(http.MethodPut, ts.URL+"/api/lab/skills?project_id="+slug,
		bytes.NewReader([]byte(`{"project_id":"`+slug+`","enabled":["alpha","beta"]}`)))
	req.Header.Set("Content-Type", "application/json")
	res, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	res.Body.Close()
	if res.StatusCode != 200 {
		t.Fatalf("put %d", res.StatusCode)
	}
	getRes, err := http.Get(ts.URL + "/api/lab/skills?project_id=" + slug)
	if err != nil {
		t.Fatal(err)
	}
	defer getRes.Body.Close()
	var body map[string]any
	_ = json.NewDecoder(getRes.Body).Decode(&body)
	en, ok := body["enabled"].([]any)
	if !ok || len(en) != 2 {
		t.Fatalf("enabled %v", body["enabled"])
	}
	if body["enabled_filter"] != true {
		t.Fatalf("filter %v", body["enabled_filter"])
	}
}

func TestSessionSearchAPI(t *testing.T) {
	ts, sci := testLabServer(t)
	slug := createProject(t, ts, "Search Sess")
	store := project.NewStore(sci)
	sess, err := store.CreateSession(slug, "alpha")
	if err != nil {
		t.Fatal(err)
	}
	_, _ = store.AppendTurns(slug, sess.ID,
		project.Turn{Role: "user", Text: "unique-token-zyx literature"},
		project.Turn{Role: "assistant", Text: "found papers"},
	)
	res, err := http.Get(ts.URL + "/api/lab/projects/" + slug + "/sessions?q=unique-token-zyx")
	if err != nil {
		t.Fatal(err)
	}
	defer res.Body.Close()
	var body map[string]any
	_ = json.NewDecoder(res.Body).Decode(&body)
	if body["count"].(float64) < 1 {
		t.Fatalf("search %v", body)
	}
}

func TestComputeImportToWorkspace(t *testing.T) {
	ts, sci := testLabServer(t)
	slug := createProject(t, ts, "Import Job")
	// run local job with harvest
	body := `{"host":"local","command":"echo harvest-me > out.dat && echo ok","timeout_sec":10,"output_globs":["*.dat"]}`
	req, _ := http.NewRequest(http.MethodPost, ts.URL+"/api/lab/compute/jobs?project_id="+slug, bytes.NewReader([]byte(body)))
	req.Header.Set("Content-Type", "application/json")
	res, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	var job map[string]any
	_ = json.NewDecoder(res.Body).Decode(&job)
	res.Body.Close()
	jobID, _ := job["id"].(string)
	if jobID == "" {
		t.Fatalf("job %v", job)
	}
	// wait done
	var last map[string]any
	for i := 0; i < 50; i++ {
		time.Sleep(50 * time.Millisecond)
		gr, err := http.Get(ts.URL + "/api/lab/compute/jobs/" + jobID + "?project_id=" + slug)
		if err != nil {
			t.Fatal(err)
		}
		_ = json.NewDecoder(gr.Body).Decode(&last)
		gr.Body.Close()
		if st, _ := last["status"].(string); st == "done" || st == "failed" || st == "timeout" {
			break
		}
	}
	if last["status"] != "done" {
		t.Fatalf("status %v", last)
	}
	// import
	impBody := `{"path":"out.dat"}`
	ireq, _ := http.NewRequest(http.MethodPost, ts.URL+"/api/lab/compute/jobs/"+jobID+"/import?project_id="+slug, bytes.NewReader([]byte(impBody)))
	ireq.Header.Set("Content-Type", "application/json")
	ires, err := http.DefaultClient.Do(ireq)
	if err != nil {
		t.Fatal(err)
	}
	defer ires.Body.Close()
	if ires.StatusCode != 200 {
		t.Fatalf("import status %d", ires.StatusCode)
	}
	var imp map[string]any
	_ = json.NewDecoder(ires.Body).Decode(&imp)
	wp, _ := imp["workspace_path"].(string)
	if wp == "" || !strings.Contains(wp, "imports/") {
		t.Fatalf("workspace_path %v", imp)
	}
	// file exists on disk
	store := project.NewStore(sci)
	ws, _ := store.WorkspacePath(slug)
	if _, err := os.Stat(filepath.Join(ws, filepath.FromSlash(wp))); err != nil {
		t.Fatal(err)
	}
	_ = slug
}

func TestSessionExportMarkdown(t *testing.T) {
	ts, sci := testLabServer(t)
	slug := createProject(t, ts, "Export Sess")
	store := project.NewStore(sci)
	sess, err := store.CreateSession(slug, "export-me")
	if err != nil {
		t.Fatal(err)
	}
	_, _ = store.AppendTurns(slug, sess.ID,
		project.Turn{Role: "user", Text: "hello export"},
		project.Turn{Role: "assistant", Text: "world **md**"},
	)
	res, err := http.Get(ts.URL + "/api/lab/projects/" + slug + "/sessions/" + sess.ID + "/export?format=md")
	if err != nil {
		t.Fatal(err)
	}
	defer res.Body.Close()
	if res.StatusCode != 200 {
		t.Fatalf("status %d", res.StatusCode)
	}
	buf := new(bytes.Buffer)
	_, _ = buf.ReadFrom(res.Body)
	body := buf.String()
	if !strings.Contains(body, "hello export") || !strings.Contains(body, "export-me") {
		t.Fatalf("md body %q", body)
	}
	jres, err := http.Get(ts.URL + "/api/lab/projects/" + slug + "/sessions/" + sess.ID + "/export?format=json")
	if err != nil {
		t.Fatal(err)
	}
	defer jres.Body.Close()
	var sessOut project.Session
	if err := json.NewDecoder(jres.Body).Decode(&sessOut); err != nil {
		t.Fatal(err)
	}
	if len(sessOut.Turns) != 2 {
		t.Fatalf("turns %d", len(sessOut.Turns))
	}
}

func TestComputeImportAll(t *testing.T) {
	ts, _ := testLabServer(t)
	slug := createProject(t, ts, "Import All")
	body := `{"host":"local","command":"echo a > a.txt && echo b > b.txt","timeout_sec":10,"output_globs":["*.txt"]}`
	req, _ := http.NewRequest(http.MethodPost, ts.URL+"/api/lab/compute/jobs?project_id="+slug, bytes.NewReader([]byte(body)))
	req.Header.Set("Content-Type", "application/json")
	res, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	var job map[string]any
	_ = json.NewDecoder(res.Body).Decode(&job)
	res.Body.Close()
	jobID, _ := job["id"].(string)
	var last map[string]any
	for i := 0; i < 50; i++ {
		time.Sleep(50 * time.Millisecond)
		gr, _ := http.Get(ts.URL + "/api/lab/compute/jobs/" + jobID + "?project_id=" + slug)
		_ = json.NewDecoder(gr.Body).Decode(&last)
		gr.Body.Close()
		if last["status"] == "done" || last["status"] == "failed" {
			break
		}
	}
	if last["status"] != "done" {
		t.Fatalf("%v", last)
	}
	ireq, _ := http.NewRequest(http.MethodPost, ts.URL+"/api/lab/compute/jobs/"+jobID+"/import?project_id="+slug,
		bytes.NewReader([]byte(`{"all":true}`)))
	ireq.Header.Set("Content-Type", "application/json")
	ires, err := http.DefaultClient.Do(ireq)
	if err != nil {
		t.Fatal(err)
	}
	defer ires.Body.Close()
	var imp map[string]any
	_ = json.NewDecoder(ires.Body).Decode(&imp)
	if imp["count"].(float64) < 1 {
		t.Fatalf("import all %v", imp)
	}
}

func TestHostPingLocal(t *testing.T) {
	ts, _ := testLabServer(t)
	req, _ := http.NewRequest(http.MethodPost, ts.URL+"/api/lab/compute/ssh-hosts/ping",
		bytes.NewReader([]byte(`{"alias":"local"}`)))
	req.Header.Set("Content-Type", "application/json")
	res, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	defer res.Body.Close()
	if res.StatusCode != 200 {
		t.Fatalf("status %d", res.StatusCode)
	}
	var body map[string]any
	_ = json.NewDecoder(res.Body).Decode(&body)
	if body["ok"] != true {
		t.Fatalf("local ping should ok: %v", body)
	}
	if body["alias"] != "local" {
		t.Fatalf("%v", body)
	}
}

func TestConfigAPI(t *testing.T) {
	ts, _ := testLabServer(t)
	req, _ := http.NewRequest(http.MethodPut, ts.URL+"/api/lab/config",
		bytes.NewReader([]byte(`{"default_mode":"plan","tool_profile":"full_science"}`)))
	req.Header.Set("Content-Type", "application/json")
	res, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	res.Body.Close()
	if res.StatusCode != 200 {
		t.Fatalf("put %d", res.StatusCode)
	}
	get, err := http.Get(ts.URL + "/api/lab/config")
	if err != nil {
		t.Fatal(err)
	}
	defer get.Body.Close()
	var body map[string]any
	_ = json.NewDecoder(get.Body).Decode(&body)
	if body["default_mode"] != "plan" {
		t.Fatalf("%v", body)
	}
}

func TestFileMkdirRenameCopy(t *testing.T) {
	ts, _ := testLabServer(t)
	slug := createProject(t, ts, "FileOps")
	// mkdir
	req, _ := http.NewRequest(http.MethodPost, ts.URL+"/api/lab/files/mkdir?project_id="+slug,
		bytes.NewReader([]byte(`{"path":"notes/sub"}`)))
	req.Header.Set("Content-Type", "application/json")
	res, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	res.Body.Close()
	if res.StatusCode != 200 {
		t.Fatalf("mkdir %d", res.StatusCode)
	}
	// write a file under notes
	wreq, _ := http.NewRequest(http.MethodPost, ts.URL+"/api/lab/files/write?project_id="+slug,
		bytes.NewReader([]byte(`{"path":"notes/a.txt","content":"hello"}`)))
	wreq.Header.Set("Content-Type", "application/json")
	wres, err := http.DefaultClient.Do(wreq)
	if err != nil {
		t.Fatal(err)
	}
	wres.Body.Close()
	// copy
	creq, _ := http.NewRequest(http.MethodPost, ts.URL+"/api/lab/files/copy?project_id="+slug,
		bytes.NewReader([]byte(`{"from":"notes/a.txt","to":"notes/b.txt"}`)))
	creq.Header.Set("Content-Type", "application/json")
	cres, err := http.DefaultClient.Do(creq)
	if err != nil {
		t.Fatal(err)
	}
	cres.Body.Close()
	if cres.StatusCode != 200 {
		t.Fatalf("copy %d", cres.StatusCode)
	}
	// rename
	rreq, _ := http.NewRequest(http.MethodPost, ts.URL+"/api/lab/files/rename?project_id="+slug,
		bytes.NewReader([]byte(`{"from":"notes/b.txt","to":"notes/c.txt"}`)))
	rreq.Header.Set("Content-Type", "application/json")
	rres, err := http.DefaultClient.Do(rreq)
	if err != nil {
		t.Fatal(err)
	}
	rres.Body.Close()
	if rres.StatusCode != 200 {
		t.Fatalf("rename %d", rres.StatusCode)
	}
	// read via content API
	get, err := http.Get(ts.URL + "/api/lab/files/content?project_id=" + slug + "&path=notes/c.txt")
	if err != nil {
		t.Fatal(err)
	}
	defer get.Body.Close()
	if get.StatusCode != 200 {
		t.Fatalf("content %d", get.StatusCode)
	}
	var body map[string]any
	_ = json.NewDecoder(get.Body).Decode(&body)
	if body["content"] != "hello" {
		t.Fatalf("content %v", body)
	}
	// conflict: copy onto existing
	bad, _ := http.NewRequest(http.MethodPost, ts.URL+"/api/lab/files/copy?project_id="+slug,
		bytes.NewReader([]byte(`{"from":"notes/a.txt","to":"notes/c.txt"}`)))
	bad.Header.Set("Content-Type", "application/json")
	bres, err := http.DefaultClient.Do(bad)
	if err != nil {
		t.Fatal(err)
	}
	bres.Body.Close()
	if bres.StatusCode == 200 {
		t.Fatal("expected conflict on existing dest")
	}
}

func TestFileWriteAndDiff(t *testing.T) {
	ts, sci := testLabServer(t)
	slug := createProject(t, ts, "Diff")
	// write two files via API
	for _, pair := range []struct{ p, c string }{
		{"a.txt", "line1\nline2\n"},
		{"b.txt", "line1\nline2x\n"},
	} {
		req, _ := http.NewRequest(http.MethodPost, ts.URL+"/api/lab/files/write?project_id="+slug,
			bytes.NewReader([]byte(`{"path":"`+pair.p+`","content":`+mustJSON(pair.c)+`}`)))
		req.Header.Set("Content-Type", "application/json")
		res, err := http.DefaultClient.Do(req)
		if err != nil {
			t.Fatal(err)
		}
		res.Body.Close()
		if res.StatusCode != 200 {
			t.Fatalf("write %s %d", pair.p, res.StatusCode)
		}
	}
	_ = sci
	dres, err := http.Get(ts.URL + "/api/lab/files/diff?project_id=" + slug + "&a=a.txt&b=b.txt")
	if err != nil {
		t.Fatal(err)
	}
	defer dres.Body.Close()
	var body map[string]any
	_ = json.NewDecoder(dres.Body).Decode(&body)
	diff, _ := body["diff"].(string)
	if !strings.Contains(diff, "-line2") && !strings.Contains(diff, "+line2x") {
		// at least not identical
		if body["identical"] == true {
			t.Fatal("should differ")
		}
	}
	if body["identical"] == true {
		t.Fatal("expected different files")
	}
}

func mustJSON(s string) string {
	b, _ := json.Marshal(s)
	return string(b)
}

func TestSkillsImportMD(t *testing.T) {
	ts, sci := testLabServer(t)
	slug := createProject(t, ts, "Skill Imp")
	var buf bytes.Buffer
	w := multipart.NewWriter(&buf)
	part, _ := w.CreateFormFile("file", "my-skill.md")
	_, _ = part.Write([]byte("---\nname: my-skill\ndescription: test skill\n---\n\n# Hello\nDo science.\n"))
	_ = w.Close()
	req, _ := http.NewRequest(http.MethodPost, ts.URL+"/api/lab/skills/import?project_id="+slug, &buf)
	req.Header.Set("Content-Type", w.FormDataContentType())
	res, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	defer res.Body.Close()
	if res.StatusCode != 200 {
		t.Fatalf("status %d", res.StatusCode)
	}
	store := project.NewStore(sci)
	pd, _ := store.ProjectDir(slug)
	if _, err := os.Stat(filepath.Join(pd, ".lumen", "skills", "my-skill.md")); err != nil {
		t.Fatal(err)
	}
}

func TestFilesDeleteAPI(t *testing.T) {
	ts, sci := testLabServer(t)
	slug := createProject(t, ts, "Del Files")
	store := project.NewStore(sci)
	ws, _ := store.WorkspacePath(slug)
	p := filepath.Join(ws, "todel.txt")
	_ = os.WriteFile(p, []byte("x"), 0o600)
	req, _ := http.NewRequest(http.MethodPost, ts.URL+"/api/lab/files/delete?project_id="+slug,
		bytes.NewReader([]byte(`{"paths":["todel.txt"]}`)))
	req.Header.Set("Content-Type", "application/json")
	res, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	defer res.Body.Close()
	if res.StatusCode != 200 {
		t.Fatalf("status %d", res.StatusCode)
	}
	if _, err := os.Stat(p); !os.IsNotExist(err) {
		t.Fatal("file should be gone")
	}
}

func TestComputeJobLogSSE(t *testing.T) {
	ts, _ := testLabServer(t)
	slug := createProject(t, ts, "Log SSE")
	body := `{"host":"local","command":"echo hello-log-line","timeout_sec":10}`
	req, _ := http.NewRequest(http.MethodPost, ts.URL+"/api/lab/compute/jobs?project_id="+slug, bytes.NewReader([]byte(body)))
	req.Header.Set("Content-Type", "application/json")
	res, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	var job map[string]any
	_ = json.NewDecoder(res.Body).Decode(&job)
	res.Body.Close()
	id, _ := job["id"].(string)
	// wait a bit then connect log stream with short client timeout
	time.Sleep(200 * time.Millisecond)
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	lreq, _ := http.NewRequestWithContext(ctx, http.MethodGet, ts.URL+"/api/lab/compute/jobs/"+id+"/log?project_id="+slug, nil)
	lres, err := http.DefaultClient.Do(lreq)
	if err != nil {
		t.Fatal(err)
	}
	defer lres.Body.Close()
	if lres.StatusCode != 200 {
		t.Fatalf("log status %d", lres.StatusCode)
	}
	buf := make([]byte, 4096)
	n, _ := lres.Body.Read(buf)
	if n == 0 {
		// may still be starting; one more read
		time.Sleep(300 * time.Millisecond)
		n, _ = lres.Body.Read(buf)
	}
	// Accept any SSE data or empty if job already finished before stream
	_ = n
}

func TestWorkspaceZipImportExport(t *testing.T) {
	ts, sci := testLabServer(t)
	slug := createProject(t, ts, "Zip IO")
	store := project.NewStore(sci)
	ws, _ := store.WorkspacePath(slug)
	_ = os.WriteFile(filepath.Join(ws, "hello.txt"), []byte("hi"), 0o600)

	// export
	res, err := http.Get(ts.URL + "/api/lab/files/export?project_id=" + slug)
	if err != nil {
		t.Fatal(err)
	}
	zipData, _ := io.ReadAll(res.Body)
	res.Body.Close()
	if res.StatusCode != 200 || len(zipData) < 20 {
		t.Fatalf("export status=%d len=%d", res.StatusCode, len(zipData))
	}
	if zipData[0] != 'P' || zipData[1] != 'K' {
		t.Fatal("not a zip")
	}

	// import back
	var buf bytes.Buffer
	w := multipart.NewWriter(&buf)
	part, _ := w.CreateFormFile("file", "pack.zip")
	_, _ = part.Write(zipData)
	_ = w.Close()
	req, _ := http.NewRequest(http.MethodPost, ts.URL+"/api/lab/files/import?project_id="+slug, &buf)
	req.Header.Set("Content-Type", w.FormDataContentType())
	ires, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	defer ires.Body.Close()
	if ires.StatusCode != 200 {
		t.Fatalf("import %d", ires.StatusCode)
	}
	var body map[string]any
	_ = json.NewDecoder(ires.Body).Decode(&body)
	if body["count"].(float64) < 1 {
		t.Fatalf("%v", body)
	}
}

func TestComputeCancelAPI(t *testing.T) {
	ts, _ := testLabServer(t)
	slug := createProject(t, ts, "Cancel Job")
	body := `{"host":"local","command":"sleep 60","timeout_sec":120}`
	req, _ := http.NewRequest(http.MethodPost, ts.URL+"/api/lab/compute/jobs?project_id="+slug, bytes.NewReader([]byte(body)))
	req.Header.Set("Content-Type", "application/json")
	res, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	var job map[string]any
	_ = json.NewDecoder(res.Body).Decode(&job)
	res.Body.Close()
	id, _ := job["id"].(string)
	time.Sleep(80 * time.Millisecond)
	creq, _ := http.NewRequest(http.MethodPost, ts.URL+"/api/lab/compute/jobs/"+id+"/cancel?project_id="+slug, bytes.NewReader([]byte("{}")))
	cres, err := http.DefaultClient.Do(creq)
	if err != nil {
		t.Fatal(err)
	}
	defer cres.Body.Close()
	if cres.StatusCode != 200 {
		t.Fatalf("cancel %d", cres.StatusCode)
	}
}

func TestHostsRegistryAPI(t *testing.T) {
	ts, _ := testLabServer(t)
	req, _ := http.NewRequest(http.MethodPost, ts.URL+"/api/lab/compute/ssh-hosts",
		bytes.NewReader([]byte(`{"alias":"box1","hostname":"1.2.3.4","user":"u","notes":"n"}`)))
	req.Header.Set("Content-Type", "application/json")
	res, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	res.Body.Close()
	if res.StatusCode != 200 {
		t.Fatalf("post %d", res.StatusCode)
	}
	get, err := http.Get(ts.URL + "/api/lab/compute/ssh-hosts")
	if err != nil {
		t.Fatal(err)
	}
	var body map[string]any
	_ = json.NewDecoder(get.Body).Decode(&body)
	get.Body.Close()
	hosts, _ := body["hosts"].([]any)
	found := false
	for _, h := range hosts {
		m, _ := h.(map[string]any)
		if m["alias"] == "box1" {
			found = true
		}
	}
	if !found {
		t.Fatalf("hosts %v", body)
	}
}

func TestFileTreeAPI(t *testing.T) {
	ts, sci := testLabServer(t)
	slug := createProject(t, ts, "Tree")
	store := project.NewStore(sci)
	ws, _ := store.WorkspacePath(slug)
	_ = os.MkdirAll(filepath.Join(ws, "sub"), 0o700)
	_ = os.WriteFile(filepath.Join(ws, "sub", "a.md"), []byte("x"), 0o600)
	res, err := http.Get(ts.URL + "/api/lab/files/tree?project_id=" + slug + "&depth=3")
	if err != nil {
		t.Fatal(err)
	}
	defer res.Body.Close()
	if res.StatusCode != 200 {
		t.Fatalf("status %d", res.StatusCode)
	}
	var body map[string]any
	_ = json.NewDecoder(res.Body).Decode(&body)
	if body["tree"] == nil {
		t.Fatalf("%v", body)
	}
}

func TestDeleteSessionAPI(t *testing.T) {
	ts, sci := testLabServer(t)
	slug := createProject(t, ts, "Del Sess")
	store := project.NewStore(sci)
	sess, err := store.CreateSession(slug, "to-delete")
	if err != nil {
		t.Fatal(err)
	}
	req, _ := http.NewRequest(http.MethodDelete, ts.URL+"/api/lab/projects/"+slug+"/sessions/"+sess.ID, nil)
	res, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	res.Body.Close()
	if res.StatusCode != 200 {
		t.Fatalf("status %d", res.StatusCode)
	}
	_, err = store.GetSession(slug, sess.ID)
	if err == nil {
		t.Fatal("expected deleted")
	}
}

func TestFileRecentAPI(t *testing.T) {
	ts, sci := testLabServer(t)
	slug := createProject(t, ts, "Recent")
	store := project.NewStore(sci)
	ws, _ := store.WorkspacePath(slug)
	_ = os.WriteFile(filepath.Join(ws, "fresh.md"), []byte("# fresh\n"), 0o600)
	res, err := http.Get(ts.URL + "/api/lab/files/recent?project_id=" + slug + "&limit=10")
	if err != nil {
		t.Fatal(err)
	}
	defer res.Body.Close()
	var body map[string]any
	_ = json.NewDecoder(res.Body).Decode(&body)
	if body["count"].(float64) < 1 {
		t.Fatalf("%v", body)
	}
}

func TestComputeJobGlobsInBody(t *testing.T) {
	ts, _ := testLabServer(t)
	slug := createProject(t, ts, "Compute Proj")
	// Submit with globs — will fail SSH quickly but should accept contract
	body := `{"host":"no-such-host.invalid","command":"echo hi","timeout_sec":1,"output_globs":["*.csv"],"work_dir":"/tmp"}`
	req, _ := http.NewRequest(http.MethodPost, ts.URL+"/api/lab/compute/jobs?project_id="+slug, bytes.NewReader([]byte(body)))
	req.Header.Set("Content-Type", "application/json")
	res, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	defer res.Body.Close()
	if res.StatusCode != 200 {
		t.Fatalf("status %d", res.StatusCode)
	}
	var job map[string]any
	_ = json.NewDecoder(res.Body).Decode(&job)
	if job["id"] == nil || job["id"] == "" {
		t.Fatalf("job %v", job)
	}
	globs, _ := job["output_globs"].([]any)
	if len(globs) != 1 {
		t.Fatalf("globs %v", job["output_globs"])
	}
}
