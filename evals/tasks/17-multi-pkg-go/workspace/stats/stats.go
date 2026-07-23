package stats

// BUG: should implement its own mean but imports the broken one
import "evaltask/calc"

func Mean(nums []int) float64 {
	return calc.Mean(nums)
}

func Sum(nums []int) int {
	total := 0
	for _, n := range nums {
		total += n
	}
	return total
}
