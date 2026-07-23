package dbutil

import (
	"errors"
	"strings"
	"testing"
)

func TestOpenDBWrapsError(t *testing.T) {
	_, err := OpenDB("invalid-dsn")
	if err == nil {
		t.Fatal("expected OpenDB to return an error for invalid open")
	}
	// Bare "database error" is the buggy implementation.
	if err.Error() == "database error" {
		t.Fatal(`error should wrap the original (fmt.Errorf("dbutil: %w", err)), not be bare "database error"`)
	}
	if !strings.HasPrefix(err.Error(), "dbutil:") {
		t.Fatalf("wrapped error should start with %q, got %q", "dbutil:", err.Error())
	}
	// Must be unwrappable (not a bare string error).
	if errors.Unwrap(err) == nil {
		t.Fatal("error must wrap an underlying error via %w")
	}
}
