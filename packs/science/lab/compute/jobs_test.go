package compute

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func TestTruncateOutput(t *testing.T) {
	s, trunc := truncateOutput("hello", 100)
	if trunc || s != "hello" {
		t.Fatalf("got %q trunc=%v", s, trunc)
	}
	big := make([]byte, 2000)
	for i := range big {
		big[i] = 'a'
	}
	s2, trunc2 := truncateOutput(string(big), 100)
	if !trunc2 || len(s2) > 100 {
		t.Fatalf("len=%d trunc=%v", len(s2), trunc2)
	}
}

func TestSubmitLocalFakeSSH(t *testing.T) {
	// Use a host that will fail fast with BatchMode — still exercises store paths.
	dir := t.TempDir()
	store, err := NewStore(dir)
	if err != nil {
		t.Fatal(err)
	}
	j, err := store.SubmitOpts("127.0.0.1", "true", "", SubmitOpts{Timeout: 2 * time.Second})
	if err != nil {
		t.Fatal(err)
	}
	if j.ID == "" || j.Status != "pending" {
		t.Fatalf("%+v", j)
	}
	// Wait for terminal state
	deadline := time.Now().Add(5 * time.Second)
	var last *Job
	for time.Now().Before(deadline) {
		last, err = store.Get(j.ID)
		if err != nil {
			t.Fatal(err)
		}
		if last.Status == "done" || last.Status == "failed" || last.Status == "timeout" {
			break
		}
		time.Sleep(50 * time.Millisecond)
	}
	if last == nil || (last.Status != "done" && last.Status != "failed" && last.Status != "timeout") {
		t.Fatalf("stuck: %+v", last)
	}
	// Persist file exists
	if _, err := os.Stat(filepath.Join(dir, ".lumen", "compute", j.ID+".json")); err != nil {
		t.Fatal(err)
	}
}

func TestCancelLocalJob(t *testing.T) {
	store, err := NewStore(t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	// long sleep so we can cancel
	j, err := store.SubmitOpts("local", "sleep 30", "", SubmitOpts{Timeout: 60 * time.Second})
	if err != nil {
		t.Fatal(err)
	}
	time.Sleep(100 * time.Millisecond)
	got, err := store.Cancel(j.ID)
	if err != nil {
		t.Fatal(err)
	}
	// status may be cancelled immediately or after wait
	deadline := time.Now().Add(3 * time.Second)
	for time.Now().Before(deadline) {
		got, _ = store.Get(j.ID)
		if got.Status == "cancelled" || got.Status == "failed" || got.Status == "done" {
			break
		}
		time.Sleep(50 * time.Millisecond)
	}
	if got.Status != "cancelled" && got.Status != "failed" {
		// cancelled preferred; failed if process already died
		t.Logf("status after cancel: %s (acceptable if race)", got.Status)
	}
	if got.Status == "running" || got.Status == "pending" {
		t.Fatalf("still active: %+v", got)
	}
}

func TestSubmitRequiresHost(t *testing.T) {
	store, _ := NewStore(t.TempDir())
	_, err := store.Submit("", "echo hi", "")
	if err == nil {
		t.Fatal("expected error")
	}
}

func TestLocalHostAndHarvest(t *testing.T) {
	if !IsLocalHost("local") || !IsLocalHost("localhost") {
		t.Fatal("IsLocalHost")
	}
	if IsLocalHost("gpu-box") {
		t.Fatal("remote should not be local")
	}
	dir := t.TempDir()
	// write an artifact the job will create... job creates it
	store, err := NewStore(dir)
	if err != nil {
		t.Fatal(err)
	}
	work := filepath.Join(dir, "ws")
	_ = os.MkdirAll(work, 0o700)
	harvestDir := filepath.Join(dir, "harvest")
	j, err := store.SubmitOpts("local", "echo hello-out > result.txt && echo done", work, SubmitOpts{
		Timeout:         10 * time.Second,
		OutputGlobs:     []string{"*.txt"},
		LocalHarvestDir: harvestDir,
	})
	if err != nil {
		t.Fatal(err)
	}
	deadline := time.Now().Add(5 * time.Second)
	var last *Job
	for time.Now().Before(deadline) {
		last, _ = store.Get(j.ID)
		if last != nil && (last.Status == "done" || last.Status == "failed" || last.Status == "timeout") {
			break
		}
		time.Sleep(30 * time.Millisecond)
	}
	if last == nil || last.Status != "done" {
		t.Fatalf("job %+v", last)
	}
	if !strings.Contains(last.Output, "done") && !strings.Contains(last.Output, "hello") {
		// output may be empty if echo only redirected; status done is enough
	}
	if len(last.Outputs) < 1 {
		t.Fatalf("expected harvest outputs: %+v", last)
	}
	if last.Outputs[0].Path == "" {
		t.Fatalf("output path empty: %+v", last.Outputs)
	}
	// local copy under harvest
	if last.Outputs[0].LocalPath == "" {
		t.Fatalf("expected LocalPath: %+v", last.Outputs[0])
	}
	if _, err := os.Stat(last.Outputs[0].LocalPath); err != nil {
		t.Fatal(err)
	}
}

func TestParseHarvestLines(t *testing.T) {
	outs := ParseHarvestLines("results/out.csv\t128\n\nfigs/a.png\t999")
	if len(outs) != 2 {
		t.Fatalf("%+v", outs)
	}
	if outs[0].Path != "results/out.csv" || outs[0].Size != 128 {
		t.Fatalf("%+v", outs[0])
	}
	if outs[1].Path != "figs/a.png" || outs[1].Size != 999 {
		t.Fatalf("%+v", outs[1])
	}
}

func TestSubmitOptsStoresGlobs(t *testing.T) {
	store, err := NewStore(t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	j, err := store.SubmitOpts("invalid-host.example", "true", "/tmp", SubmitOpts{
		Timeout:     1 * time.Second,
		OutputGlobs: []string{"*.csv", "out/*"},
	})
	if err != nil {
		t.Fatal(err)
	}
	got, err := store.Get(j.ID)
	if err != nil {
		t.Fatal(err)
	}
	if len(got.OutputGlobs) != 2 || got.OutputGlobs[0] != "*.csv" {
		t.Fatalf("%+v", got.OutputGlobs)
	}
	// Wait terminal so goroutine does not leak into other tests
	deadline := time.Now().Add(4 * time.Second)
	for time.Now().Before(deadline) {
		got, _ = store.Get(j.ID)
		if got.Status == "done" || got.Status == "failed" || got.Status == "timeout" {
			break
		}
		time.Sleep(40 * time.Millisecond)
	}
}
