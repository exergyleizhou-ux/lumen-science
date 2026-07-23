package strutil

import "testing"

func TestReverse(t *testing.T) {
	if got := Reverse("abc"); got != "cba" {
		t.Fatalf("Reverse(abc) = %q, want cba", got)
	}
}

func TestReverseUnicode(t *testing.T) {
	if got := Reverse("héllo"); got != "olléh" {
		t.Fatalf("Reverse(héllo) = %q, want olléh", got)
	}
}
