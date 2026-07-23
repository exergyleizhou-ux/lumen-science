package calc

import "testing"

func TestAverage(t *testing.T) {
	if got := Average([]int{2, 4, 6}); got != 4 {
		t.Fatalf("Average([2,4,6]) = %v, want 4", got)
	}
}

func TestAverageEmpty(t *testing.T) {
	if got := Average([]int{}); got != 0 {
		t.Fatalf("Average([]) = %v, want 0", got)
	}
}
