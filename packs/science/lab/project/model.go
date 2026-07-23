package project

import "time"

// Project is a science lab workspace.
type Project struct {
	ID            string    `json:"id"`
	Slug          string    `json:"slug"`
	Title         string    `json:"title"`
	Template      string    `json:"template,omitempty"`
	CreatedAt     time.Time `json:"created_at"`
	UpdatedAt     time.Time `json:"updated_at"`
	ActiveSession string    `json:"active_session,omitempty"`
	WorkspaceRel  string    `json:"workspace_rel"` // always "workspace"
}

// Turn is one persisted chat message (user or assistant summary).
type Turn struct {
	Role    string         `json:"role"` // user | assistant | system
	Text    string         `json:"text"`
	Tools   []ToolSummary  `json:"tools,omitempty"`
	At      time.Time      `json:"at"`
	Mode    string         `json:"mode,omitempty"`
	Stopped bool           `json:"stopped,omitempty"`
}

// ToolSummary is a compact tool call for history restore.
type ToolSummary struct {
	ID     string `json:"id,omitempty"`
	Name   string `json:"name"`
	Args   string `json:"args,omitempty"`
	Output string `json:"output,omitempty"`
	Err    string `json:"err,omitempty"`
	Status string `json:"status,omitempty"` // running | done | error
}

// Session is a conversation within a project.
type Session struct {
	ID        string    `json:"id"`
	ProjectID string    `json:"project_id"`
	Title     string    `json:"title"`
	CreatedAt time.Time `json:"created_at"`
	UpdatedAt time.Time `json:"updated_at"`
	Turns     []Turn    `json:"turns,omitempty"`
	TurnCount int       `json:"turn_count,omitempty"` // set on list views without embedding turns
}
