package gui

import (
	"net/http"
	"sync"
	"sync/atomic"
	"testing"
	"time"
)

func TestStressOasisReadOnly(t *testing.T) {
	skipStress(t)
	ts := stressHandler(t)
	defer ts.Close()
	const workers = 12
	const each = 20
	var fail int64
	var wg sync.WaitGroup
	wg.Add(workers)
	for w := 0; w < workers; w++ {
		go func() {
			defer wg.Done()
			client := &http.Client{Timeout: 5 * time.Second}
			for i := 0; i < each; i++ {
				resp, err := client.Get(ts.URL + "/api/oasis")
				if err != nil || resp.StatusCode != http.StatusOK {
					atomic.AddInt64(&fail, 1)
					if resp != nil {
						resp.Body.Close()
					}
					continue
				}
				resp.Body.Close()
			}
		}()
	}
	wg.Wait()
	if fail > 0 {
		t.Fatalf("oasis read failures: %d", fail)
	}
}
