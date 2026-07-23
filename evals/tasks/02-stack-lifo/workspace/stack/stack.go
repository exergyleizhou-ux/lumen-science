package stack

// Stack is a LIFO stack of ints.
type Stack struct{ items []int }

func (s *Stack) Push(v int) { s.items = append(s.items, v) }

// Pop removes and returns the most recently pushed item.
func (s *Stack) Pop() (int, bool) {
	if len(s.items) == 0 {
		return 0, false
	}
	v := s.items[0] // BUG: returns the oldest, not the newest
	s.items = s.items[1:]
	return v, true
}
