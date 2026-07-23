package shapes

import "math"

// Circle is a correct implementation — leave it alone.
type Circle struct{ R float64 }

func (c Circle) Area() float64 { return math.Pi * c.R * c.R }
