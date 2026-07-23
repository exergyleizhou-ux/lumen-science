package proxy

import (
	"encoding/json"
	"fmt"
	"net/http"
)

func (s *Server) replayAsSSE(w http.ResponseWriter, aresp map[string]any) {
	w.Header().Set("Content-Type", "text/event-stream")
	w.Header().Set("Cache-Control", "no-cache")
	w.WriteHeader(http.StatusOK)
	flusher, _ := w.(http.Flusher)

	blocks, _ := aresp["content"].([]map[string]any)
	if len(blocks) == 0 {
		blocks = []map[string]any{{"type": "text", "text": ""}}
	}

	usage, _ := aresp["usage"].(map[string]any)
	inTok, _ := usage["input_tokens"].(int)
	outTok, _ := usage["output_tokens"].(int)

	writeSSE := func(event string, data any) {
		b, _ := json.Marshal(data)
		fmt.Fprintf(w, "event: %s\ndata: %s\n\n", event, b)
		if flusher != nil {
			flusher.Flush()
		}
	}

	writeSSE("message_start", map[string]any{
		"type": "message_start",
		"message": map[string]any{
			"id":            aresp["id"],
			"type":          "message",
			"role":          "assistant",
			"model":         aresp["model"],
			"content":       []any{},
			"stop_reason":   nil,
			"stop_sequence": nil,
			"usage": map[string]any{
				"input_tokens":  inTok,
				"output_tokens": 0,
			},
		},
	})
	writeSSE("ping", map[string]any{"type": "ping"})

	for idx, blk := range blocks {
		switch blk["type"] {
		case "tool_use":
			writeSSE("content_block_start", map[string]any{
				"type":  "content_block_start",
				"index": idx,
				"content_block": map[string]any{
					"type":  "tool_use",
					"id":    blk["id"],
					"name":  blk["name"],
					"input": map[string]any{},
				},
			})
			inputJSON, _ := json.Marshal(blk["input"])
			writeSSE("content_block_delta", map[string]any{
				"type":  "content_block_delta",
				"index": idx,
				"delta": map[string]any{
					"type":         "input_json_delta",
					"partial_json": string(inputJSON),
				},
			})
		default:
			writeSSE("content_block_start", map[string]any{
				"type":  "content_block_start",
				"index": idx,
				"content_block": map[string]any{
					"type": "text",
					"text": "",
				},
			})
			writeSSE("content_block_delta", map[string]any{
				"type":  "content_block_delta",
				"index": idx,
				"delta": map[string]any{
					"type": "text_delta",
					"text": blk["text"],
				},
			})
		}
		writeSSE("content_block_stop", map[string]any{
			"type":  "content_block_stop",
			"index": idx,
		})
	}

	writeSSE("message_delta", map[string]any{
		"type": "message_delta",
		"delta": map[string]any{
			"stop_reason":   aresp["stop_reason"],
			"stop_sequence": nil,
		},
		"usage": map[string]any{"output_tokens": outTok},
	})
	writeSSE("message_stop", map[string]any{"type": "message_stop"})
}
