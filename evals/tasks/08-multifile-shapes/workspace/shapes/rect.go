package shapes

// Rect is a rectangle.
type Rect struct{ W, H float64 }

// Area should return the rectangle's area.
func (r Rect) Area() float64 { return 2 * (r.W + r.H) } // BUG: this is the perimeter
