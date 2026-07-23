package proxy

import (
	"bytes"
	"context"
	"fmt"
	"io"
	"net/http"
	"time"
)

const upstreamAttempts = 4

// Upstream posts JSON to an upstream API with connection-level retries.
type Upstream struct {
	Client *http.Client
	Log    func(string)
}

func (u *Upstream) client() *http.Client {
	if u.Client != nil {
		return u.Client
	}
	return &http.Client{Timeout: 300 * time.Second}
}

func (u *Upstream) log(msg string) {
	if u.Log != nil {
		u.Log(msg)
	}
}

// PostJSON sends a POST and returns the full response body.
func (u *Upstream) PostJSON(ctx context.Context, url string, body []byte, headers map[string]string) ([]byte, string, int, error) {
	var lastErr error
	for i := 0; i < upstreamAttempts; i++ {
		req, err := http.NewRequestWithContext(ctx, http.MethodPost, url, bytes.NewReader(body))
		if err != nil {
			return nil, "", 0, err
		}
		for k, v := range headers {
			req.Header.Set(k, v)
		}
		resp, err := u.client().Do(req)
		if err != nil {
			lastErr = err
			if i < upstreamAttempts-1 {
				u.log(fmt.Sprintf("  ~ upstream jitter, retry %d/%d: %v", i+1, upstreamAttempts-1, err))
				time.Sleep(time.Duration(800*(i+1)) * time.Millisecond)
				continue
			}
			return nil, "", 0, err
		}
		data, readErr := io.ReadAll(resp.Body)
		resp.Body.Close()
		ct := resp.Header.Get("Content-Type")
		if readErr != nil {
			lastErr = readErr
			if i < upstreamAttempts-1 {
				u.log(fmt.Sprintf("  ~ upstream read jitter, retry %d/%d: %v", i+1, upstreamAttempts-1, readErr))
				time.Sleep(time.Duration(800*(i+1)) * time.Millisecond)
				continue
			}
			return nil, ct, resp.StatusCode, readErr
		}
		if resp.StatusCode >= 400 {
			return data, ct, resp.StatusCode, &HTTPStatusError{Code: resp.StatusCode, Body: data}
		}
		return data, ct, resp.StatusCode, nil
	}
	return nil, "", 0, lastErr
}

// OpenStream opens a streaming POST and returns the response after reading the first chunk.
func (u *Upstream) OpenStream(ctx context.Context, url string, body []byte, headers map[string]string) (*http.Response, []byte, error) {
	var lastErr error
	for i := 0; i < upstreamAttempts; i++ {
		req, err := http.NewRequestWithContext(ctx, http.MethodPost, url, bytes.NewReader(body))
		if err != nil {
			return nil, nil, err
		}
		for k, v := range headers {
			req.Header.Set(k, v)
		}
		resp, err := u.client().Do(req)
		if err != nil {
			lastErr = err
			if i < upstreamAttempts-1 {
				u.log(fmt.Sprintf("  ~ upstream jitter, retry %d/%d: %v", i+1, upstreamAttempts-1, err))
				time.Sleep(time.Duration(800*(i+1)) * time.Millisecond)
				continue
			}
			return nil, nil, err
		}
		if resp.StatusCode >= 400 {
			data, _ := io.ReadAll(resp.Body)
			resp.Body.Close()
			return nil, nil, &HTTPStatusError{Code: resp.StatusCode, Body: data}
		}
		buf := make([]byte, 4096)
		n, readErr := resp.Body.Read(buf)
		if readErr != nil && readErr != io.EOF {
			resp.Body.Close()
			lastErr = readErr
			if i < upstreamAttempts-1 {
				u.log(fmt.Sprintf("  ~ upstream stream jitter, retry %d/%d: %v", i+1, upstreamAttempts-1, readErr))
				time.Sleep(time.Duration(800*(i+1)) * time.Millisecond)
				continue
			}
			return nil, nil, readErr
		}
		if n == 0 {
			resp.Body.Close()
			lastErr = fmt.Errorf("upstream 200 but empty body")
			if i < upstreamAttempts-1 {
				u.log(fmt.Sprintf("  ~ upstream empty body, retry %d/%d", i+1, upstreamAttempts-1))
				time.Sleep(time.Duration(800*(i+1)) * time.Millisecond)
				continue
			}
			return nil, nil, lastErr
		}
		return resp, buf[:n], nil
	}
	return nil, nil, lastErr
}

// HTTPStatusError is a non-retryable upstream HTTP failure.
type HTTPStatusError struct {
	Code int
	Body []byte
}

func (e *HTTPStatusError) Error() string {
	return fmt.Sprintf("upstream HTTP %d", e.Code)
}
