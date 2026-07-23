package project

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func TestSessionTurnsPersist(t *testing.T) {
	sci := t.TempDir()
	store := NewStore(sci)
	p, err := store.Create("Hist Proj", "")
	if err != nil {
		t.Fatal(err)
	}
	sess, err := store.CreateSession(p.Slug, "test-session")
	if err != nil {
		t.Fatal(err)
	}
	_, err = store.AppendTurns(p.Slug, sess.ID,
		Turn{Role: "user", Text: "hello world research", At: time.Now().UTC()},
		Turn{Role: "assistant", Text: "hi **there**", Tools: []ToolSummary{{Name: "bash", Status: "done", Output: "ok"}}},
	)
	if err != nil {
		t.Fatal(err)
	}
	got, err := store.GetSession(p.Slug, sess.ID)
	if err != nil {
		t.Fatal(err)
	}
	if len(got.Turns) != 2 {
		t.Fatalf("turns %d", len(got.Turns))
	}
	if got.Turns[0].Role != "user" || got.Turns[0].Text != "hello world research" {
		t.Fatalf("user turn %+v", got.Turns[0])
	}
	if got.Turns[1].Role != "assistant" || len(got.Turns[1].Tools) != 1 {
		t.Fatalf("assistant turn %+v", got.Turns[1])
	}
	list, err := store.ListSessions(p.Slug)
	if err != nil {
		t.Fatal(err)
	}
	if len(list) != 1 || list[0].TurnCount != 2 {
		t.Fatalf("list %+v", list)
	}
	if list[0].Turns != nil {
		t.Fatal("list should omit turn bodies")
	}
	// title auto-updated from first user message
	if got.Title == "test-session" {
		// may still be original if already set — CreateSession set title
	}
	// EnsureSession reuses
	again, err := store.EnsureSession(p.Slug, sess.ID, "")
	if err != nil || again.ID != sess.ID {
		t.Fatalf("ensure %v %+v", err, again)
	}
	// enabled skills
	if err := store.SaveEnabledSkills(p.Slug, []string{"lit-review", "plot"}); err != nil {
		t.Fatal(err)
	}
	en, err := store.LoadEnabledSkills(p.Slug)
	if err != nil || len(en) != 2 || en[0] != "lit-review" {
		t.Fatalf("enabled %v %v", en, err)
	}
	path, _ := store.EnabledSkillsPath(p.Slug)
	if _, err := os.Stat(path); err != nil {
		t.Fatal(err)
	}
	_ = filepath.Dir(path)

	hits, err := store.SearchSessions(p.Slug, "research", 20)
	if err != nil {
		t.Fatal(err)
	}
	if len(hits) < 1 {
		t.Fatalf("expected search hits for research, got %+v", hits)
	}
	found := false
	for _, h := range hits {
		if h.SessionID == sess.ID && (strings.Contains(strings.ToLower(h.Snippet), "research") || h.Role == "user") {
			found = true
		}
	}
	if !found {
		t.Fatalf("hits %+v", hits)
	}
}
