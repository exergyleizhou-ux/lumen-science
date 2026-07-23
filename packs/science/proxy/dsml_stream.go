package proxy

import (
	"bytes"
	"encoding/json"
	"fmt"
	"unicode/utf8"
)

const dsmlMaxOpen = len("<｜｜DSML｜｜function_calls>")
const dsmlCaptureCap = 256 * 1024

// DsmlStreamRewriter rewrites streaming SSE frames to recover leaked DSML tool calls.
type DsmlStreamRewriter struct {
	knownTools  map[string]map[string]any
	nonce       string
	buf         string
	utf8carry   []byte
	nextOut     int
	curOut      *int
	curType     string
	synthesized bool
	toolN       int
	state       string // PASS | CAPTURE
	scanBuf     string
	capBuf      string
}

// NewDsmlStreamRewriter builds a streaming DSML rewriter.
func NewDsmlStreamRewriter(knownTools map[string]map[string]any, nonce string) *DsmlStreamRewriter {
	if nonce == "" {
		nonce = "x"
	}
	return &DsmlStreamRewriter{
		knownTools: knownTools,
		nonce:      nonce,
		state:      "PASS",
	}
}

// Feed processes upstream SSE bytes and returns rewritten output.
func (r *DsmlStreamRewriter) Feed(data []byte) []byte {
	r.utf8carry = append(r.utf8carry, data...)
	for len(r.utf8carry) > 0 {
		rune, size := utf8.DecodeRune(r.utf8carry)
		if rune == utf8.RuneError && size == 1 && len(r.utf8carry) < 4 {
			break
		}
		r.buf += string(r.utf8carry[:size])
		r.utf8carry = r.utf8carry[size:]
	}
	return r.drainFrames(false)
}

// Finalize flushes decoder tail and pending text at stream end.
func (r *DsmlStreamRewriter) Finalize() []byte {
	if len(r.utf8carry) > 0 {
		r.buf += string(r.utf8carry)
		r.utf8carry = nil
	}
	out := r.drainFrames(true)
	out = append(out, r.finalizeText()...)
	return out
}

func (r *DsmlStreamRewriter) drainFrames(flushTail bool) []byte {
	var out []byte
	for {
		iLF := stringsIndex(r.buf, "\n\n")
		iCRLF := stringsIndex(r.buf, "\r\n\r\n")
		idx, sep := -1, 0
		if iLF >= 0 && (iCRLF < 0 || iLF <= iCRLF) {
			idx, sep = iLF, 2
		} else if iCRLF >= 0 {
			idx, sep = iCRLF, 4
		}
		if idx < 0 {
			break
		}
		frame := r.buf[:idx]
		r.buf = r.buf[idx+sep:]
		out = append(out, r.handleFrame(frame)...)
	}
	if flushTail && stringsTrim(r.buf) != "" {
		frame := r.buf
		r.buf = ""
		out = append(out, r.handleFrame(frame)...)
	}
	return out
}

func stringsIndex(s, sub string) int {
	return bytes.Index([]byte(s), []byte(sub))
}

func stringsTrim(s string) string {
	return string(bytes.TrimSpace([]byte(s)))
}

func (r *DsmlStreamRewriter) handleFrame(frame string) []byte {
	event, obj := parseSSEFrame(frame)
	if obj == nil {
		return rawSSEFrame(frame)
	}
	t, _ := obj["type"].(string)
	switch t {
	case "content_block_start":
		if cb, ok := obj["content_block"].(map[string]any); ok {
			r.curType, _ = cb["type"].(string)
		}
		r.curOut = intPtr(r.nextOut)
		r.nextOut++
		obj["index"] = *r.curOut
		return emitSSE(event, obj)
	case "content_block_delta":
		delta, _ := obj["delta"].(map[string]any)
		dtype, _ := delta["type"].(string)
		if r.curType == "text" && dtype == "text_delta" {
			text, _ := delta["text"].(string)
			return r.onTextDelta(text)
		}
		if r.curOut != nil {
			obj["index"] = *r.curOut
		}
		return emitSSE(event, obj)
	case "content_block_stop":
		return r.onBlockStop()
	case "message_delta":
		out := r.flushPending()
		out = append(out, r.onMessageDelta(obj)...)
		return out
	case "message_stop":
		out := r.flushPending()
		out = append(out, rawSSEFrame(frame)...)
		return out
	default:
		return rawSSEFrame(frame)
	}
}

func (r *DsmlStreamRewriter) onTextDelta(text string) []byte {
	if r.state == "PASS" {
		r.scanBuf += text
		return r.passScan()
	}
	r.capBuf += text
	return r.captureScan()
}

func (r *DsmlStreamRewriter) passScan() []byte {
	var out []byte
	for {
		loc := dsmlOpenRe.FindStringIndex(r.scanBuf)
		if loc != nil {
			before := r.scanBuf[:loc[0]]
			if before != "" {
				out = append(out, r.textDelta(before)...)
			}
			if r.curOut != nil {
				out = append(out, emitSSE("content_block_stop", map[string]any{
					"type": "content_block_stop", "index": *r.curOut,
				})...)
				r.curOut = nil
			}
			r.capBuf = r.scanBuf[loc[0]:]
			r.scanBuf = ""
			r.state = "CAPTURE"
			out = append(out, r.captureScan()...)
			return out
		}
		keep := dsmlMaxOpen - 1
		if len(r.scanBuf) > keep {
			emit := r.scanBuf[:len(r.scanBuf)-keep]
			r.scanBuf = r.scanBuf[len(r.scanBuf)-keep:]
			if emit != "" {
				out = append(out, r.textDelta(emit)...)
			}
		}
		return out
	}
}

func (r *DsmlStreamRewriter) captureScan() []byte {
	var out []byte
	if loc := dsmlToolCalls.FindStringSubmatchIndex(r.capBuf); loc != nil {
		region := r.capBuf[loc[0]:loc[1]]
		calls := parseDsmlToolCalls(region, r.knownTools)
		if len(calls) > 0 {
			for _, c := range calls {
				out = append(out, r.toolUseEvents(c)...)
			}
			r.synthesized = true
		} else {
			out = append(out, r.textAsNewBlock(r.capBuf[loc[0]:loc[1]])...)
		}
		rest := r.capBuf[loc[1]:]
		r.capBuf = ""
		r.state = "PASS"
		r.curOut = nil
		if rest != "" {
			r.scanBuf = rest
			out = append(out, r.passScan()...)
		}
		return out
	}
	if len(r.capBuf) > dsmlCaptureCap {
		out = append(out, r.textAsNewBlock(r.capBuf)...)
		r.capBuf = ""
		r.state = "PASS"
		r.curOut = nil
	}
	return out
}

func (r *DsmlStreamRewriter) finalizeText() []byte {
	var out []byte
	if r.state == "CAPTURE" && r.capBuf != "" {
		out = append(out, r.textAsNewBlock(r.capBuf)...)
		r.capBuf = ""
		r.state = "PASS"
	}
	if r.scanBuf != "" {
		out = append(out, r.textDelta(r.scanBuf)...)
		r.scanBuf = ""
	}
	if r.curOut != nil {
		out = append(out, emitSSE("content_block_stop", map[string]any{
			"type": "content_block_stop", "index": *r.curOut,
		})...)
		r.curOut = nil
	}
	return out
}

func (r *DsmlStreamRewriter) onBlockStop() []byte {
	var out []byte
	if r.state == "CAPTURE" {
		if r.capBuf != "" {
			out = append(out, r.textAsNewBlock(r.capBuf)...)
		}
		r.capBuf = ""
		r.state = "PASS"
	} else if r.scanBuf != "" && r.curOut != nil {
		out = append(out, emitSSE("content_block_delta", map[string]any{
			"type":  "content_block_delta",
			"index": *r.curOut,
			"delta": map[string]any{"type": "text_delta", "text": r.scanBuf},
		})...)
		r.scanBuf = ""
	}
	if r.curOut != nil {
		out = append(out, emitSSE("content_block_stop", map[string]any{
			"type": "content_block_stop", "index": *r.curOut,
		})...)
		r.curOut = nil
	}
	return out
}

func (r *DsmlStreamRewriter) flushPending() []byte {
	var out []byte
	if r.state == "CAPTURE" && r.capBuf != "" {
		out = append(out, r.textAsNewBlock(r.capBuf)...)
		r.capBuf = ""
		r.state = "PASS"
	} else if r.scanBuf != "" {
		out = append(out, r.textDelta(r.scanBuf)...)
		r.scanBuf = ""
	}
	if r.curOut != nil {
		out = append(out, emitSSE("content_block_stop", map[string]any{
			"type": "content_block_stop", "index": *r.curOut,
		})...)
		r.curOut = nil
	}
	return out
}

func (r *DsmlStreamRewriter) textDelta(text string) []byte {
	var head []byte
	if r.curOut == nil {
		head = r.openTextBlock()
	}
	return append(head, emitSSE("content_block_delta", map[string]any{
		"type":  "content_block_delta",
		"index": *r.curOut,
		"delta": map[string]any{"type": "text_delta", "text": text},
	})...)
}

func (r *DsmlStreamRewriter) openTextBlock() []byte {
	idx := r.nextOut
	r.curOut = &idx
	r.nextOut++
	r.curType = "text"
	return emitSSE("content_block_start", map[string]any{
		"type":  "content_block_start",
		"index": idx,
		"content_block": map[string]any{
			"type": "text", "text": "",
		},
	})
}

func (r *DsmlStreamRewriter) textAsNewBlock(text string) []byte {
	out := r.textDelta(text)
	if r.curOut != nil {
		out = append(out, emitSSE("content_block_stop", map[string]any{
			"type": "content_block_stop", "index": *r.curOut,
		})...)
		r.curOut = nil
	}
	return out
}

func (r *DsmlStreamRewriter) toolUseEvents(call dsmlSegment) []byte {
	idx := r.nextOut
	r.nextOut++
	r.toolN++
	tid := fmt.Sprintf("toolu_dsml_%s_%d", r.nonce, r.toolN)
	inJSON, _ := json.Marshal(call.Input)
	var out []byte
	out = append(out, emitSSE("content_block_start", map[string]any{
		"type":  "content_block_start",
		"index": idx,
		"content_block": map[string]any{
			"type": "tool_use", "id": tid, "name": call.Name, "input": map[string]any{},
		},
	})...)
	out = append(out, emitSSE("content_block_delta", map[string]any{
		"type":  "content_block_delta",
		"index": idx,
		"delta": map[string]any{"type": "input_json_delta", "partial_json": string(inJSON)},
	})...)
	out = append(out, emitSSE("content_block_stop", map[string]any{
		"type": "content_block_stop", "index": idx,
	})...)
	return out
}

func (r *DsmlStreamRewriter) onMessageDelta(obj map[string]any) []byte {
	if r.synthesized {
		delta, _ := obj["delta"].(map[string]any)
		if delta == nil {
			delta = map[string]any{}
		}
		sr, _ := delta["stop_reason"].(string)
		if sr == "" || sr == "end_turn" || sr == "stop" {
			delta["stop_reason"] = "tool_use"
			obj["delta"] = delta
		}
	}
	return emitSSE("message_delta", obj)
}

func parseSSEFrame(frame string) (string, map[string]any) {
	var event string
	var dataLines []string
	for _, line := range splitLines(frame) {
		line = trimCR(line)
		if len(line) >= 6 && line[:6] == "event:" {
			event = trimSpace(line[6:])
		} else if len(line) >= 5 && line[:5] == "data:" {
			dataLines = append(dataLines, trimSpace(line[5:]))
		}
	}
	if len(dataLines) == 0 {
		return event, nil
	}
	var obj map[string]any
	if json.Unmarshal([]byte(joinLines(dataLines)), &obj) != nil {
		return event, nil
	}
	return event, obj
}

func emitSSE(event string, obj map[string]any) []byte {
	if event == "" {
		if t, ok := obj["type"].(string); ok {
			event = t
		}
	}
	b, _ := json.Marshal(obj)
	return []byte(fmt.Sprintf("event: %s\ndata: %s\n\n", event, b))
}

func rawSSEFrame(frame string) []byte {
	return []byte(frame + "\n\n")
}

func intPtr(n int) *int { return &n }

func splitLines(s string) []string {
	var out []string
	start := 0
	for i := 0; i < len(s); i++ {
		if s[i] == '\n' {
			out = append(out, s[start:i])
			start = i + 1
		}
	}
	if start < len(s) {
		out = append(out, s[start:])
	}
	return out
}

func trimCR(s string) string {
	if len(s) > 0 && s[len(s)-1] == '\r' {
		return s[:len(s)-1]
	}
	return s
}

func trimSpace(s string) string {
	return string(bytes.TrimSpace([]byte(s)))
}

func joinLines(lines []string) string {
	return string(bytes.Join(sliceOfBytes(lines), []byte("\n")))
}

func sliceOfBytes(ss []string) [][]byte {
	out := make([][]byte, len(ss))
	for i, s := range ss {
		out[i] = []byte(s)
	}
	return out
}
