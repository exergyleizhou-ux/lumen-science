package search

// Search returns the index of target in the sorted slice xs, or -1 if absent.
func Search(xs []int, target int) int {
	lo, hi := 0, len(xs)
	for lo < hi {
		mid := (lo + hi) / 2
		if xs[mid] == target {
			return mid
		} else if xs[mid] < target {
			lo = mid // BUG: should be mid+1
		} else {
			hi = mid
		}
	}
	return -1
}
