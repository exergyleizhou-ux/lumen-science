package stack

import "testing"

func TestPopIsLIFO(t *testing.T) {
	var s Stack
	s.Push(1); s.Push(2); s.Push(3)
	for _, want := range []int{3, 2, 1} {
		got, ok := s.Pop()
		if !ok || got != want {
			t.Fatalf("Pop() = %d,%v; want %d", got, ok, want)
		}
	}
}
