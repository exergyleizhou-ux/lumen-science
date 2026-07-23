package parser

import "testing"

func TestTrimSpacesNormal(t *testing.T) {
	if got := TrimSpaces("  hello  "); got != "hello" {
		t.Fatalf("got %q, want %q", got, "hello")
	}
}

func TestTrimSpacesEmpty(t *testing.T) {
	if got := TrimSpaces(""); got != "" {
		t.Fatalf("got %q, want empty", got)
	}
}

func TestToUpperStillWorks(t *testing.T) {
	if got := ToUpper("hello"); got != "HELLO" {
		t.Fatalf("ToUpper regression: got %q, want %q", got, "HELLO")
	}
}
