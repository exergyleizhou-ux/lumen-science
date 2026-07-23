package lab

import (
	"log"
	"net/http"
	"runtime/debug"
	"strings"
	"time"
)

const maxBodyBytes = 1 << 20

type responseWriter struct {
	http.ResponseWriter
	status int
}

func (w *responseWriter) WriteHeader(code int) {
	w.status = code
	w.ResponseWriter.WriteHeader(code)
}

func (w *responseWriter) Flush() {
	if f, ok := w.ResponseWriter.(http.Flusher); ok {
		f.Flush()
	}
}

func wrapMiddleware(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		rw := &responseWriter{ResponseWriter: w, status: http.StatusOK}
		start := time.Now()
		defer func() {
			if rec := recover(); rec != nil {
				log.Printf("science lab panic: %v\n%s", rec, debug.Stack())
				writeJSON(rw, http.StatusInternalServerError, map[string]any{"error": "internal server error"})
			}
		}()
		if r.Method == http.MethodPost || r.Method == http.MethodPut {
			// Chat allows larger prompts; other POSTs stay tight.
			limit := int64(maxBodyBytes)
			if strings.HasPrefix(r.URL.Path, "/api/lab/chat") {
				limit = ChatBodyMaxBytes
			}
			if strings.HasPrefix(r.URL.Path, "/api/lab/files") {
				limit = 64 << 20
			}
			r.Body = http.MaxBytesReader(rw, r.Body, limit)
		}
		next.ServeHTTP(rw, r)
		if rw.status >= 500 {
			log.Printf("science lab %s %s %d %s", r.Method, r.URL.Path, rw.status, time.Since(start))
		}
	})
}

func securityHeaders(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("X-Content-Type-Options", "nosniff")
		w.Header().Set("Content-Security-Policy", "frame-ancestors 'self' http://127.0.0.1:18990 https://127.0.0.1:18993 https://demo.oasisdata2026.xyz https://*.oasisdata2026.xyz")
		next.ServeHTTP(w, r)
	})
}
