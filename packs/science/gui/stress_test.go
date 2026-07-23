package gui

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"
)

func stressHandler(t *testing.T) *httptest.Server {
	t.Helper()
	t.Cleanup(resetPanelManager)
	srv, _ := testServer(t)
	return httptest.NewServer(securityHeaders(srv.cors(rateLimitMutations(srv.wrapMiddleware(srv.mux)))))
}

func skipStress(t *testing.T) {
	t.Helper()
	if testing.Short() {
		t.Skip("skipping stress test in -short mode (use full test for load checks)")
	}
}

func hammerGET(t *testing.T, url string, workers, each int) (ok, fail int64) {
	t.Helper()
	var wg sync.WaitGroup
	wg.Add(workers)
	for w := 0; w < workers; w++ {
		go func() {
			defer wg.Done()
			client := &http.Client{Timeout: 10 * time.Second}
			for i := 0; i < each; i++ {
				resp, err := client.Get(url)
				if err != nil {
					atomic.AddInt64(&fail, 1)
					continue
				}
				if resp.StatusCode == http.StatusOK {
					atomic.AddInt64(&ok, 1)
				} else {
					atomic.AddInt64(&fail, 1)
				}
				resp.Body.Close()
			}
		}()
	}
	wg.Wait()
	return
}

func TestStressHealthConcurrent(t *testing.T) {
	skipStress(t)
	ts := stressHandler(t)
	defer ts.Close()
	ok, fail := hammerGET(t, ts.URL+"/api/health", 16, 25)
	if fail > 0 {
		t.Fatalf("health failures: %d (ok %d)", fail, ok)
	}
	if ok < 350 {
		t.Fatalf("expected >=350 ok responses, got %d", ok)
	}
}

func TestStressConfigAndDoctorConcurrent(t *testing.T) {
	skipStress(t)
	ts := stressHandler(t)
	defer ts.Close()
	paths := []string{"/api/config", "/api/doctor", "/api/status", "/api/version"}
	var wg sync.WaitGroup
	var fail int64
	for _, path := range paths {
		for w := 0; w < 16; w++ {
			wg.Add(1)
			go func(p string) {
				defer wg.Done()
				client := &http.Client{Timeout: 15 * time.Second}
				for i := 0; i < 25; i++ {
					resp, err := client.Get(ts.URL + p)
					if err != nil || resp.StatusCode != http.StatusOK {
						atomic.AddInt64(&fail, 1)
						if resp != nil {
							resp.Body.Close()
						}
						continue
					}
					resp.Body.Close()
				}
			}(path)
		}
	}
	wg.Wait()
	if fail > 0 {
		t.Fatalf("concurrent read failures: %d", fail)
	}
}

func TestStressSSEClients(t *testing.T) {
	skipStress(t)
	ts := stressHandler(t)
	defer ts.Close()
	const clients = 12
	var wg sync.WaitGroup
	var events int64
	var fail int64
	ctx, cancel := context.WithTimeout(context.Background(), 6*time.Second)
	defer cancel()

	for c := 0; c < clients; c++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			req, err := http.NewRequestWithContext(ctx, http.MethodGet, ts.URL+"/api/events", nil)
			if err != nil {
				atomic.AddInt64(&fail, 1)
				return
			}
			resp, err := http.DefaultClient.Do(req)
			if err != nil {
				return // context timeout expected
			}
			defer resp.Body.Close()
			if resp.StatusCode != http.StatusOK {
				atomic.AddInt64(&fail, 1)
				return
			}
			buf := make([]byte, 4096)
			for {
				n, err := resp.Body.Read(buf)
				if n > 0 && strings.Contains(string(buf[:n]), "data:") {
					atomic.AddInt64(&events, 1)
				}
				if err != nil {
					break
				}
			}
		}()
	}
	wg.Wait()
	if fail > 0 {
		t.Fatalf("sse client failures: %d", fail)
	}
	if events < int64(clients) {
		t.Fatalf("expected events from each client, got %d", events)
	}
}

func TestStressMutateRateLimit(t *testing.T) {
	skipStress(t)
	ts := stressHandler(t)
	defer ts.Close()
	var limited int64
	var ok int64
	body := strings.NewReader(`{"provider":"deepseek"}`)
	for i := 0; i < 40; i++ {
		req, _ := http.NewRequest(http.MethodPut, ts.URL+"/api/config", body)
		req.Header.Set("Content-Type", "application/json")
		resp, err := http.DefaultClient.Do(req)
		if err != nil {
			t.Fatal(err)
		}
		switch resp.StatusCode {
		case http.StatusOK, http.StatusBadRequest:
			atomic.AddInt64(&ok, 1)
		case http.StatusTooManyRequests:
			atomic.AddInt64(&limited, 1)
		default:
			t.Fatalf("unexpected status %d", resp.StatusCode)
		}
		resp.Body.Close()
	}
	if limited == 0 {
		t.Fatal("expected some 429 under burst mutate load")
	}
	if ok == 0 {
		t.Fatal("expected some successful mutate responses")
	}
}

func TestHealthUptimeField(t *testing.T) {
	ts := stressHandler(t)
	defer ts.Close()
	resp, err := http.Get(ts.URL + "/api/health")
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	var body map[string]any
	if err := json.NewDecoder(resp.Body).Decode(&body); err != nil {
		t.Fatal(err)
	}
	uptime, ok := body["uptime_ms"].(float64)
	if !ok || uptime < 0 {
		t.Fatalf("missing uptime_ms: %v", body)
	}
	if body["panel"] != "lumen://science" {
		t.Fatalf("panel id: %v", body["panel"])
	}
}

func TestStaticCacheHeaders(t *testing.T) {
	srv, _ := testServer(t)
	ts := httptest.NewServer(srv.mux)
	defer ts.Close()
	resp, err := http.Get(ts.URL + "/assets/app.css")
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	cc := resp.Header.Get("Cache-Control")
	if cc == "" || !strings.Contains(cc, "immutable") {
		t.Fatalf("expected immutable cache on assets, got %q", cc)
	}
	data, _ := io.ReadAll(resp.Body)
	if len(data) < 100 {
		t.Fatal("css too small")
	}
	_ = fmt.Sprintf("%d", len(data))
}
