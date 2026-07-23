package counter

// Counter counts events. BUG: not safe for concurrent use.
type Counter struct{ n int }

func (c *Counter) Inc()       { c.n++ }
func (c *Counter) Value() int { return c.n }
