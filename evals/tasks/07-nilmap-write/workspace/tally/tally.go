package tally

// Tally counts how many times each key has been added.
type Tally struct {
	counts map[string]int
}

// Add records one occurrence of key.
func (t *Tally) Add(key string) {
	t.counts[key]++ // BUG: counts is never initialized → nil map assignment panics
}

// Count returns how many times key was added.
func (t *Tally) Count(key string) int {
	return t.counts[key]
}
