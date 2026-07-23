package proxy

import (
	"bytes"
	"encoding/json"
	"os"
	"regexp"
	"strconv"
	"strings"
)

// DSML markers DeepSeek may leak as plain text instead of tool_use blocks.
var dsmlMarkerBytes = [][]byte{
	[]byte("｜DSML｜"),
	[]byte("｜｜DSML｜｜"),
}

// ToolUseShimMode controls DSML leak handling: off (default), detect, rewrite.
type ToolUseShimMode string

const (
	ShimOff     ToolUseShimMode = "off"
	ShimDetect  ToolUseShimMode = "detect"
	ShimRewrite ToolUseShimMode = "rewrite"
)

// ResolveToolUseShim reads LUMEN_TOOLUSE_SHIM (or legacy CSSWITCH_TOOLUSE_SHIM).
func ResolveToolUseShim(spec ProviderSpec, explicit string) ToolUseShimMode {
	if explicit != "" {
		switch ToolUseShimMode(strings.ToLower(strings.TrimSpace(explicit))) {
		case ShimDetect, ShimRewrite:
			return ToolUseShimMode(strings.ToLower(strings.TrimSpace(explicit)))
		default:
			return ShimOff
		}
	}
	if !spec.DsmlCapable || spec.Name == "relay" {
		return ShimOff
	}
	for _, k := range []string{"LUMEN_TOOLUSE_SHIM", "CSSWITCH_TOOLUSE_SHIM"} {
		m := strings.ToLower(strings.TrimSpace(os.Getenv(k)))
		switch ToolUseShimMode(m) {
		case ShimDetect, ShimRewrite:
			return ToolUseShimMode(m)
		}
	}
	return ShimOff
}

var (
	dsmlPipe      = `[｜]{1,2}`
	dsmlWrap      = `(?:tool_calls|function_calls)`
	dsmlOpenRe    = regexp.MustCompile(`<` + dsmlPipe + `DSML` + dsmlPipe + dsmlWrap + `>`)
	dsmlToolCalls = regexp.MustCompile(`<` + dsmlPipe + `DSML` + dsmlPipe + dsmlWrap + `>(.*?)</` + dsmlPipe + `DSML` + dsmlPipe + dsmlWrap + `>`)
	dsmlInvoke    = regexp.MustCompile(`<` + dsmlPipe + `DSML` + dsmlPipe + `invoke\s+name="([^"]+)"\s*>(.*?)</` + dsmlPipe + `DSML` + dsmlPipe + `invoke>`)
	dsmlParam     = regexp.MustCompile(`<` + dsmlPipe + `DSML` + dsmlPipe + `parameter\s+name="([^"]+)"(?:\s+string="(true|false)")?\s*>(.*?)</` + dsmlPipe + `DSML` + dsmlPipe + `parameter>`)
)

// DsmlDetector scans upstream bytes for DSML leak markers without rewriting.
type DsmlDetector struct {
	found bool
	tail  []byte
	maxK  int
}

// NewDsmlDetector builds a cross-chunk DSML marker detector.
func NewDsmlDetector() *DsmlDetector {
	maxK := 0
	for _, m := range dsmlMarkerBytes {
		if len(m) > maxK {
			maxK = len(m)
		}
	}
	return &DsmlDetector{maxK: maxK}
}

// Feed scans a chunk; returns whether a marker was found.
func (d *DsmlDetector) Feed(data []byte) bool {
	if d.found || len(data) == 0 {
		return d.found
	}
	buf := append(append([]byte(nil), d.tail...), data...)
	for _, mk := range dsmlMarkerBytes {
		if bytes.Contains(buf, mk) {
			d.found = true
			d.tail = nil
			return true
		}
	}
	if len(buf) >= d.maxK {
		d.tail = append([]byte(nil), buf[len(buf)-(d.maxK-1):]...)
	} else {
		d.tail = append([]byte(nil), buf...)
	}
	return false
}

type dsmlSegment struct {
	Type  string
	Text  string
	Name  string
	Input map[string]any
}

func coerceDsmlParam(stringAttr, raw string, propSchema map[string]any) any {
	if stringAttr == "true" {
		return raw
	}
	typ, _ := propSchema["type"].(string)
	switch typ {
	case "integer":
		if n, err := strconv.Atoi(strings.TrimSpace(raw)); err == nil {
			return n
		}
	case "number":
		if f, err := strconv.ParseFloat(strings.TrimSpace(raw), 64); err == nil {
			return f
		}
	case "boolean":
		low := strings.ToLower(strings.TrimSpace(raw))
		switch low {
		case "true", "1", "yes":
			return true
		case "false", "0", "no":
			return false
		default:
			return raw
		}
	case "object", "array":
		var v any
		if json.Unmarshal([]byte(raw), &v) == nil {
			return v
		}
	}
	var v any
	if json.Unmarshal([]byte(raw), &v) == nil {
		return v
	}
	return raw
}

func dsmlTypeOK(val any, typ string) bool {
	switch typ {
	case "", "string":
		if typ == "string" {
			_, ok := val.(string)
			return ok
		}
		return true
	case "integer":
		if _, ok := val.(int); ok && !isBool(val) {
			return true
		}
		if s, ok := val.(string); ok {
			t := strings.TrimSpace(s)
			if t == "" {
				return false
			}
			_, err := strconv.Atoi(t)
			return err == nil
		}
		return false
	case "number":
		if isBool(val) {
			return false
		}
		switch val.(type) {
		case int, int64, float32, float64:
			return true
		}
		if s, ok := val.(string); ok {
			_, err := strconv.ParseFloat(strings.TrimSpace(s), 64)
			return err == nil
		}
		return false
	case "boolean":
		if _, ok := val.(bool); ok {
			return true
		}
		if s, ok := val.(string); ok {
			low := strings.ToLower(strings.TrimSpace(s))
			return low == "true" || low == "false" || low == "1" || low == "0" || low == "yes" || low == "no"
		}
		return false
	case "object":
		_, ok := val.(map[string]any)
		return ok
	case "array":
		_, ok := val.([]any)
		return ok
	default:
		return true
	}
}

func isBool(v any) bool {
	_, ok := v.(bool)
	return ok
}

func validateDsmlInput(inp map[string]any, schema map[string]any) bool {
	if schema == nil {
		return true
	}
	if reqs, ok := schema["required"].([]any); ok {
		for _, r := range reqs {
			if rs, ok := r.(string); ok {
				if _, ok := inp[rs]; !ok {
					return false
				}
			}
		}
	}
	props, _ := schema["properties"].(map[string]any)
	for k, v := range inp {
		if ps, ok := props[k].(map[string]any); ok {
			typ, _ := ps["type"].(string)
			if !dsmlTypeOK(v, typ) {
				return false
			}
		}
	}
	return true
}

func parseDsmlInvoke(name, body string, knownTools map[string]map[string]any) *dsmlSegment {
	schema := knownTools[name]
	if schema == nil {
		return nil
	}
	schemaProps, _ := schema["properties"].(map[string]any)
	inp := map[string]any{}
	for _, m := range dsmlParam.FindAllStringSubmatch(body, -1) {
		pn, sattr, raw := m[1], m[2], m[3]
		var propSchema map[string]any
		if ps, ok := schemaProps[pn].(map[string]any); ok {
			propSchema = ps
		}
		inp[pn] = coerceDsmlParam(sattr, raw, propSchema)
	}
	if len(inp) == 1 {
		only := ""
		for k := range inp {
			only = k
		}
		if (only == "arguments" || only == "input") && schemaProps[only] == nil {
			val := inp[only]
			if s, ok := val.(string); ok {
				var parsed map[string]any
				if json.Unmarshal([]byte(s), &parsed) == nil {
					inp = parsed
				}
			} else if m, ok := val.(map[string]any); ok {
				inp = m
			}
		}
	}
	if !validateDsmlInput(inp, schema) {
		return nil
	}
	return &dsmlSegment{Type: "tool_use", Name: name, Input: inp}
}

func parseDsmlToolCalls(wrapperRegion string, knownTools map[string]map[string]any) []dsmlSegment {
	var out []dsmlSegment
	for _, m := range dsmlToolCalls.FindAllStringSubmatch(wrapperRegion, -1) {
		invokes := dsmlInvoke.FindAllStringSubmatch(m[1], -1)
		if len(invokes) == 0 {
			return nil
		}
		for _, inv := range invokes {
			if _, ok := knownTools[inv[1]]; !ok {
				return nil
			}
			seg := parseDsmlInvoke(inv[1], inv[2], knownTools)
			if seg == nil {
				return nil
			}
			out = append(out, *seg)
		}
	}
	return out
}

// SegmentDsmlText splits text on DSML tool_calls regions into text/tool_use segments.
func SegmentDsmlText(text string, knownTools map[string]map[string]any) []dsmlSegment {
	if text == "" {
		return nil
	}
	var segs []dsmlSegment
	pos := 0
	for _, m := range dsmlToolCalls.FindAllStringSubmatchIndex(text, -1) {
		calls := parseDsmlToolCalls(text[m[0]:m[1]], knownTools)
		if len(calls) == 0 {
			continue
		}
		if m[0] > pos {
			segs = append(segs, dsmlSegment{Type: "text", Text: text[pos:m[0]]})
		}
		segs = append(segs, calls...)
		pos = m[1]
	}
	if pos < len(text) {
		if tail := text[pos:]; tail != "" {
			segs = append(segs, dsmlSegment{Type: "text", Text: tail})
		}
	}
	if len(segs) == 0 {
		return []dsmlSegment{{Type: "text", Text: text}}
	}
	return segs
}

// ToolsSchemaFromRequest extracts tool name → input_schema from an Anthropic request.
func ToolsSchemaFromRequest(areq map[string]any) map[string]map[string]any {
	out := map[string]map[string]any{}
	tools, ok := areq["tools"].([]any)
	if !ok {
		return out
	}
	for _, t := range tools {
		tm, ok := t.(map[string]any)
		if !ok {
			continue
		}
		name, _ := tm["name"].(string)
		if name == "" {
			continue
		}
		if schema, ok := tm["input_schema"].(map[string]any); ok {
			out[name] = schema
		} else {
			out[name] = map[string]any{"type": "object", "properties": map[string]any{}}
		}
	}
	return out
}

// RewriteNonstreamBody rewrites DSML leaks in a non-streaming Anthropic response.
// Returns original bytes when unchanged.
func RewriteNonstreamBody(body []byte, knownTools map[string]map[string]any, nonce string) []byte {
	if nonce == "" {
		nonce = "x"
	}
	var obj map[string]any
	if json.Unmarshal(body, &obj) != nil {
		return body
	}
	content, ok := obj["content"].([]any)
	if !ok {
		return body
	}
	var newContent []any
	changed := false
	n := 0
	for _, blk := range content {
		bm, ok := blk.(map[string]any)
		if !ok {
			newContent = append(newContent, blk)
			continue
		}
		if bm["type"] != "text" {
			newContent = append(newContent, blk)
			continue
		}
		text, _ := bm["text"].(string)
		segs := SegmentDsmlText(text, knownTools)
		hasTool := false
		for _, s := range segs {
			if s.Type == "tool_use" {
				hasTool = true
				break
			}
		}
		if !hasTool {
			newContent = append(newContent, blk)
			continue
		}
		changed = true
		for _, s := range segs {
			switch s.Type {
			case "text":
				newContent = append(newContent, map[string]any{"type": "text", "text": s.Text})
			case "tool_use":
				n++
				newContent = append(newContent, map[string]any{
					"type":  "tool_use",
					"id":    "toolu_dsml_" + nonce + "_" + strconv.Itoa(n),
					"name":  s.Name,
					"input": s.Input,
				})
			}
		}
	}
	if !changed {
		return body
	}
	obj["content"] = newContent
	if sr, _ := obj["stop_reason"].(string); sr == "" || sr == "end_turn" || sr == "stop" {
		obj["stop_reason"] = "tool_use"
	}
	out, err := json.Marshal(obj)
	if err != nil {
		return body
	}
	return out
}
