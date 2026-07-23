package lab

import (
	"sync"

	"lumen/internal/event"
	"lumen/internal/science/lab/project"
)

// historySink records text/tools for session persistence while forwarding to inner.
type historySink struct {
	mu       sync.Mutex
	inner    event.Sink
	text     string
	tools    map[string]project.ToolSummary
	toolOrder []string
}

func newHistorySink(inner event.Sink) *historySink {
	return &historySink{
		inner: inner,
		tools: make(map[string]project.ToolSummary),
	}
}

func (h *historySink) Emit(e event.Event) {
	h.mu.Lock()
	switch e.Kind {
	case event.Text:
		h.text += e.Text
	case event.Reasoning:
		// keep reasoning out of main assistant text; optional prefix
	case event.ToolDispatch:
		if e.Tool.ID != "" {
			h.tools[e.Tool.ID] = project.ToolSummary{
				ID: e.Tool.ID, Name: e.Tool.Name, Args: e.Tool.Args, Status: "running",
			}
			h.toolOrder = appendUnique(h.toolOrder, e.Tool.ID)
		}
	case event.ToolResult:
		if e.Tool.ID != "" {
			prev := h.tools[e.Tool.ID]
			if prev.Name == "" {
				prev.Name = e.Tool.Name
			}
			prev.ID = e.Tool.ID
			prev.Output = e.Tool.Output
			prev.Err = e.Tool.Err
			if e.Tool.Err != "" {
				prev.Status = "error"
			} else {
				prev.Status = "done"
			}
			h.tools[e.Tool.ID] = prev
			h.toolOrder = appendUnique(h.toolOrder, e.Tool.ID)
		}
	}
	h.mu.Unlock()
	if h.inner != nil {
		h.inner.Emit(e)
	}
}

func (h *historySink) snapshot() (text string, tools []project.ToolSummary) {
	h.mu.Lock()
	defer h.mu.Unlock()
	text = h.text
	for _, id := range h.toolOrder {
		if t, ok := h.tools[id]; ok {
			tools = append(tools, t)
		}
	}
	return text, tools
}

func appendUnique(order []string, id string) []string {
	for _, x := range order {
		if x == id {
			return order
		}
	}
	return append(order, id)
}
