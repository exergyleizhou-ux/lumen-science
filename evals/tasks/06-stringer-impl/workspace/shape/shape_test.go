package shape
import ("math";"testing")
func TestCircle(t *testing.T) {
	var s Shape = Circle{R: 2}
	if math.Abs(s.Perimeter()-4*math.Pi) > 1e-9 {
		t.Fatalf("Perimeter = %v, want %v", s.Perimeter(), 4*math.Pi)
	}
}
