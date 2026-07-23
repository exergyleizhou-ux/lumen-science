package shape

import "math"

type Shape interface {
	Area() float64
	Perimeter() float64
}

type Circle struct{ R float64 }

func (c Circle) Area() float64 { return math.Pi * c.R * c.R }

// TODO: Circle is missing Perimeter, so it does not satisfy Shape.
