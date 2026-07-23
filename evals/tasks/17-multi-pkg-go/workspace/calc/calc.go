package calc

func Mean(nums []int) float64 {
	sum := 0
	for _, n := range nums {
		sum += n
	}
	// BUG: div zero on empty
	return float64(sum) / float64(len(nums))
}
