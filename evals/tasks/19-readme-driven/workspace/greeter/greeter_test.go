package greeter

import "testing"

func TestHello(t *testing.T) {
	if got := Hello("Alice"); got != "Hello, Alice!" {
		t.Fatalf("got %q", got)
	}
	if got := Hello(""); got != "Hello, !" {
		t.Fatalf("got %q", got)
	}
}

func TestGoodbye(t *testing.T) {
	if got := Goodbye("Alice"); got != "Goodbye, Alice!" {
		t.Fatalf("got %q", got)
	}
}

func TestIsEmpty(t *testing.T) {
	if !IsEmpty("") {
		t.Error("empty string should be empty")
	}
	if !IsEmpty("   ") {
		t.Error("whitespace should be empty")
	}
	if IsEmpty("hello") {
		t.Error("non-empty should not be empty")
	}
}
