package counter
import ("sync";"testing")
func TestConcurrentInc(t *testing.T) {
	var c Counter
	var wg sync.WaitGroup
	for i := 0; i < 100; i++ {
		wg.Add(1)
		go func() { defer wg.Done(); for j := 0; j < 100; j++ { c.Inc() } }()
	}
	wg.Wait()
	if c.Value() != 10000 {
		t.Fatalf("Value() = %d, want 10000", c.Value())
	}
}
