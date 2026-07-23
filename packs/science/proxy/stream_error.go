package proxy

import (
	"encoding/json"
	"fmt"
	"net/http"
)

func (s *Server) writeSSEErrorAndTerminate(w http.ResponseWriter, msg string) {
	flusher, _ := w.(http.Flusher)
	frame := map[string]any{
		"type": "error",
		"error": map[string]any{
			"type":    "api_error",
			"message": msg,
		},
	}
	b, _ := json.Marshal(frame)
	_, _ = fmt.Fprintf(w, "event: error\ndata: %s\n\n", b)
	if flusher != nil {
		flusher.Flush()
	}
}
