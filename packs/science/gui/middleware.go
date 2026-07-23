package gui

import (
	"log"
	"net/http"
	"runtime/debug"
	"time"
)

const maxBodyBytes = 1 << 20 // 1 MiB

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

func (s *Server) wrapMiddleware(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		rw := &responseWriter{ResponseWriter: w, status: http.StatusOK}
		start := time.Now()
		defer func() {
			if rec := recover(); rec != nil {
				log.Printf("science gui panic: %v\n%s", rec, debug.Stack())
				writeJSON(rw, http.StatusInternalServerError, map[string]any{
					"error": "internal server error",
				})
			}
		}()
		if r.Method == http.MethodPost || r.Method == http.MethodPut {
			r.Body = http.MaxBytesReader(rw, r.Body, maxBodyBytes)
		}
		next.ServeHTTP(rw, r)
		if rw.status >= 500 {
			log.Printf("science gui %s %s %d %s", r.Method, r.URL.Path, rw.status, time.Since(start))
		}
	})
}

func securityHeaders(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("X-Content-Type-Options", "nosniff")
		w.Header().Set("Referrer-Policy", "no-referrer")
		// frame-src allows embedding same-origin Lab (/lumen-lab/) inside the bridge panel.
		w.Header().Set("Content-Security-Policy", "default-src 'self'; style-src 'self' 'unsafe-inline' https://fonts.googleapis.com; font-src 'self' https://fonts.gstatic.com; script-src 'self'; connect-src 'self'; frame-src 'self'; child-src 'self'; frame-ancestors 'self' https://demo.oasisdata2026.xyz https://*.oasisdata2026.xyz")
		next.ServeHTTP(w, r)
	})
}
