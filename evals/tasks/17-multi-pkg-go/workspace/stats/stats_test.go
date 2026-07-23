package stats

import "testing"

func TestMeanEmpty(t *testing.T) {
	if got := Mean([]int{}); got != 0 {
		t.Fatalf("Mean([]) = %v, want 0", got)
	}
}

func TestSum(t *testing.T) {
	if got := Sum([]int{1, 2, 3}); got != 6 {
		t.Fatalf("Sum([1,2,3]) = %d, want 6", got)
	}
}
