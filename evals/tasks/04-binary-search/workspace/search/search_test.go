package search
import "testing"
func TestSearch(t *testing.T) {
	xs := []int{1, 3, 5, 7, 9, 11}
	for i, v := range xs {
		if got := Search(xs, v); got != i {
			t.Fatalf("Search(%d) = %d, want %d", v, got, i)
		}
	}
	if got := Search(xs, 4); got != -1 {
		t.Fatalf("Search(4) = %d, want -1", got)
	}
}
