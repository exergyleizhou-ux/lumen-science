package tally

import "testing"

func TestTally(t *testing.T) {
	var ta Tally // zero value — Add must work without a constructor
	ta.Add("a")
	ta.Add("a")
	ta.Add("b")
	if got := ta.Count("a"); got != 2 {
		t.Fatalf("Count(a) = %d, want 2", got)
	}
	if got := ta.Count("b"); got != 1 {
		t.Fatalf("Count(b) = %d, want 1", got)
	}
	if got := ta.Count("z"); got != 0 {
		t.Fatalf("Count(z) = %d, want 0", got)
	}
}
