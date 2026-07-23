package proxy

import (
	"bytes"
	"encoding/json"
	"strings"
	"testing"
)

const (
	p2 = "｜｜"
	p1 = "｜"
)

var webSearchSchema = map[string]map[string]any{
	"web_search": {
		"type": "object",
		"properties": map[string]any{
			"query": map[string]any{"type": "string"},
		},
	},
}

func dsmlWrapper(pipe, name string, params [][2]string) string {
	var ps strings.Builder
	for _, p := range params {
		ps.WriteString("<" + pipe + "DSML" + pipe + `parameter name="` + p[0] + `" string="true">`)
		ps.WriteString(p[1])
		ps.WriteString("</" + pipe + "DSML" + pipe + "parameter>")
	}
	return "<" + pipe + "DSML" + pipe + `invoke name="` + name + `">` + ps.String() + "</" + pipe + "DSML" + pipe + "invoke>"
}

func dsmlBlock(pipe, name string, params [][2]string) string {
	return "<" + pipe + "DSML" + pipe + "tool_calls> " + dsmlWrapper(pipe, name, params) + " </" + pipe + "DSML" + pipe + "tool_calls>"
}

func TestSegmentDsmlIssue8ThreeCalls(t *testing.T) {
	q1 := `site:https://www.ncbi.nlm.nih.gov/geo/ "GSE207177"`
	q2 := `"GSE207177" AND ("sepsis" OR "heart")`
	q3 := `https://www.ncbi.nlm.nih.gov/geo/query/acc.cgi?acc=GSE207177`
	blk1 := "<" + p2 + "DSML" + p2 + "tool_calls> " +
		dsmlWrapper(p2, "web_search", [][2]string{{"query", q1}}) + " " +
		dsmlWrapper(p2, "web_search", [][2]string{{"query", q2}}) + " </" + p2 + "DSML" + p2 + "tool_calls>"
	blk2 := dsmlBlock(p2, "web_search", [][2]string{{"query", q3}})
	segs := SegmentDsmlText(blk1+blk2, webSearchSchema)
	var queries []string
	for _, s := range segs {
		if s.Type == "tool_use" {
			queries = append(queries, s.Input["query"].(string))
		}
	}
	if len(queries) != 3 || queries[0] != q1 || queries[1] != q2 || queries[2] != q3 {
		t.Fatalf("queries = %v", queries)
	}
}

func TestSegmentDsmlUnknownToolStaysText(t *testing.T) {
	blk := dsmlBlock(p2, "evil_exec", [][2]string{{"cmd", "rm -rf /"}})
	segs := SegmentDsmlText(blk, webSearchSchema)
	for _, s := range segs {
		if s.Type != "text" {
			t.Fatalf("expected text only, got %v", s)
		}
	}
	if got := segs[0].Text; got != blk {
		t.Fatalf("text mismatch")
	}
}

func TestSegmentDsmlInterleaving(t *testing.T) {
	b := dsmlBlock(p2, "web_search", [][2]string{{"query", "q"}})
	text := "A" + b + "B" + b + "C"
	segs := SegmentDsmlText(text, webSearchSchema)
	if len(segs) != 5 || segs[0].Text != "A" || segs[2].Text != "B" || segs[4].Text != "C" {
		t.Fatalf("interleaving: %#v", segs)
	}
}

func TestRewriteNonstreamPreservesCleanBody(t *testing.T) {
	body := []byte(`{"id":"msg_1","type":"message","role":"assistant","model":"claude-opus-4-8","content":[{"type":"text","text":"hello"}],"stop_reason":"end_turn","usage":{"input_tokens":1,"output_tokens":2}}`)
	out := RewriteNonstreamBody(body, webSearchSchema, "n")
	if !bytes.Equal(out, body) {
		t.Fatal("clean body should be byte-identical")
	}
}

func TestRewriteNonstreamRecoversToolUse(t *testing.T) {
	blk := dsmlBlock(p2, "web_search", [][2]string{{"query", "geo"}})
	body := map[string]any{
		"content":     []any{map[string]any{"type": "text", "text": blk}},
		"stop_reason": "end_turn",
	}
	raw, _ := json.Marshal(body)
	out := RewriteNonstreamBody(raw, webSearchSchema, "t")
	var obj map[string]any
	if json.Unmarshal(out, &obj) != nil {
		t.Fatal(errString("unmarshal"))
	}
	content, _ := obj["content"].([]any)
	if len(content) != 1 {
		t.Fatalf("content len %d", len(content))
	}
	bm, _ := content[0].(map[string]any)
	if bm["type"] != "tool_use" || bm["name"] != "web_search" {
		t.Fatalf("got %#v", bm)
	}
	if obj["stop_reason"] != "tool_use" {
		t.Fatalf("stop_reason = %v", obj["stop_reason"])
	}
}

func TestDsmlDetectorCrossChunk(t *testing.T) {
	d := NewDsmlDetector()
	part1 := []byte("prefix <｜")
	part2 := []byte("｜DSML｜｜tool_calls>")
	if d.Feed(part1) {
		t.Fatal("should not detect on partial marker")
	}
	if !d.Feed(part2) {
		t.Fatal("should detect complete marker across chunks")
	}
}

func TestDsmlStreamRewriterFinalizeFlushesTail(t *testing.T) {
	rw := NewDsmlStreamRewriter(webSearchSchema, "z")
	// message_stop without trailing blank line (EOF sudden)
	frame := `event: message_stop
data: {"type":"message_stop"}`
	out := append(rw.Feed([]byte(frame)), rw.Finalize()...)
	if !bytes.Contains(out, []byte("message_stop")) {
		t.Fatalf("missing message_stop in %q", out)
	}
}

func TestResolveToolUseShimDefaultOff(t *testing.T) {
	spec := BuiltInProviders["deepseek"]
	if ResolveToolUseShim(spec, "") != ShimOff {
		t.Fatal("default should be off")
	}
	if ResolveToolUseShim(spec, "rewrite") != ShimRewrite {
		t.Fatal("explicit rewrite")
	}
}

func errString(s string) error { return &testErr{s} }

type testErr struct{ s string }

func (e *testErr) Error() string { return e.s }
