package shapes

import (
	"math"
	"testing"
)

func TestAreas(t *testing.T) {
	shapes := []Shape{Circle{R: 2}, Rect{W: 3, H: 4}}
	wants := []float64{math.Pi * 4, 12}
	for i, s := range shapes {
		if got := s.Area(); math.Abs(got-wants[i]) > 1e-9 {
			t.Fatalf("shape %d Area() = %v, want %v", i, got, wants[i])
		}
	}
}
