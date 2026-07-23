package lab

import (
	"context"
	_ "embed"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"sync"

	"lumen/internal/config"
	"lumen/internal/control"
	"lumen/internal/event"
	"lumen/internal/permission"
	"lumen/internal/science/lab/project"
	"lumen/internal/science/lab/provenance"
	labruntime "lumen/internal/science/lab/runtime"
	"lumen/internal/science/lab/tools"
	"lumen/internal/science/lab/workspace"
	"lumen/internal/skill"
)

//go:embed prompts/science_system.txt
var scienceSystemPrompt string

// Controller wraps the Lumen agent for lab chat turns.
type Controller struct {
	mu         sync.Mutex
	sciDir     string
	fleet      *labruntime.FleetManager
	projects   *project.Store
	ctrl       *control.Controller
	slug       string
	sessID     string
	workspace  string
	guard      *workspace.Guard
	provenance *provenance.Recorder
}

// NewController builds a lab agent controller.
func NewController(sciDir string, fleet *labruntime.FleetManager, projects *project.Store) *Controller {
	return &Controller{
		sciDir:   sciDir,
		fleet:    fleet,
		projects: projects,
		ctrl:     control.New(),
	}
}

// Configure prepares the agent for a project workspace.
func (c *Controller) Configure(slug, sessionID string, sink event.Sink, approver permission.Asker) error {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.slug = slug
	c.sessID = sessionID

	lumenCfgPath := config.FindConfig()
	ws, err := c.projects.WorkspacePath(slug)
	if err != nil {
		return err
	}
	c.workspace = ws
	g, err := workspace.NewGuard(ws)
	if err != nil {
		return err
	}
	c.guard = g
	_ = os.Setenv("LUMEN_WORKSPACE_ROOT", ws)

	projDir, err := c.projects.ProjectDir(slug)
	if err != nil {
		return err
	}
	rec, err := provenance.NewRecorder(projDir, sessionID, os.Getenv("LUMEN_SCIENCE_MODEL"))
	if err != nil {
		return err
	}
	c.provenance = rec

	sciCfg, err := scienceConfig(c.sciDir)
	if err != nil {
		return err
	}
	if _, _, err := ApplyScienceProfile(sciCfg); err != nil {
		return err
	}

	_ = os.Setenv("LUMEN_TOOLS_PROFILE", defaultToolProfile)
	// Inject conda/python from the cloned research pack into bash PATH.
	labruntime.InjectLabPath(c.sciDir)

	// Build system prompt: science base + enabled project skills (bodies).
	mem := scienceSystemPrompt
	if skillBlock := c.buildEnabledSkillsPrompt(slug, ws, projDir); skillBlock != "" {
		mem = mem + "\n\n" + skillBlock
	}
	c.ctrl.SetExtraMemoryPrompt(mem)
	sink = wrapProvenanceSink(sink, c.provenance, g)
	if err := c.ctrl.Configure(sink, nil, lumenCfgPath); err != nil {
		return err
	}
	c.ctrl.SetPermissionMode(permission.ModePlan)
	if approver != nil {
		c.ctrl.SetApprover(approver)
	}

	extra := tools.RegisterFleet(c.fleet, c.provenance)
	briefTool := &tools.BriefGenerateTool{
		SciDir:      c.sciDir,
		ProjectRoot: ws,
		Projects:    c.projects,
		Guard:       g,
	}
	extra = append(extra, briefTool)
	c.ctrl.AddExtraTools(extra)
	return nil
}

// buildEnabledSkillsPrompt injects enabled skill bodies into the system prompt.
// If no enable list is saved, injects a short catalog of names only (not full bodies)
// to avoid huge prompts; when enable list is set, inject full body for each enabled skill.
func (c *Controller) buildEnabledSkillsPrompt(slug, ws, projDir string) string {
	home, _ := os.UserHomeDir()
	skillPaths := []string{
		filepath.Join(home, ".lumen", "skills"),
		filepath.Join(home, ".lumen", "imported-skills"),
		filepath.Join(c.sciDir, "skills"),
		filepath.Join(projDir, ".lumen", "skills"),
	}
	if packSkills := labruntime.SkillsDir(c.sciDir); packSkills != "" {
		skillPaths = append(skillPaths, packSkills)
	}
	store := skill.New(skill.Options{
		HomeDir:     home,
		ProjectRoot: ws,
		CustomPaths: skillPaths,
	})
	list := store.List()
	if len(list) == 0 {
		return ""
	}
	enabled, _ := c.projects.LoadEnabledSkills(slug)
	enSet := map[string]bool{}
	for _, n := range enabled {
		enSet[n] = true
	}
	filter := len(enabled) > 0

	var b strings.Builder
	if filter {
		b.WriteString("## Enabled science skills (follow these when relevant)\n")
		for _, sk := range list {
			if !enSet[sk.Name] {
				continue
			}
			b.WriteString("\n### Skill: ")
			b.WriteString(sk.Name)
			b.WriteString("\n")
			if sk.Description != "" {
				b.WriteString(sk.Description)
				b.WriteString("\n")
			}
			body := strings.TrimSpace(sk.Body)
			if body != "" {
				// Cap each skill body to keep prompt bounded
				runes := []rune(body)
				if len(runes) > 6000 {
					body = string(runes[:6000]) + "\n…[skill truncated]…"
				}
				b.WriteString(body)
				b.WriteString("\n")
			}
		}
	} else {
		b.WriteString("## Available science skills (catalog)\n")
		b.WriteString("User has not filtered skills; prefer these when the task matches:\n")
		n := 0
		for _, sk := range list {
			b.WriteString("- **")
			b.WriteString(sk.Name)
			b.WriteString("**: ")
			b.WriteString(sk.Description)
			b.WriteString("\n")
			n++
			if n >= 40 {
				b.WriteString("…\n")
				break
			}
		}
	}
	return strings.TrimSpace(b.String())
}

// Run executes one chat turn. File tools resolve paths via LUMEN_WORKSPACE_ROOT
// (no process-wide chdir — builtins stay isolated to the project workspace).
func (c *Controller) Run(ctx context.Context, prompt, mode string) error {
	c.mu.Lock()
	ctrl := c.ctrl
	c.mu.Unlock()
	if ctrl == nil {
		return fmt.Errorf("lab controller not configured")
	}
	switch mode {
	case "plan", "":
		ctrl.SetPermissionMode(permission.ModePlan)
		return ctrl.Plan(ctx, prompt)
	case "bypass":
		ctrl.SetPermissionMode(permission.ModeBypass)
		return ctrl.Run(ctx, prompt)
	case "agent", "default":
		ctrl.SetPermissionMode(permission.ModeDefault)
		return ctrl.Run(ctx, prompt)
	default:
		ctrl.SetPermissionMode(permission.ParseMode(mode))
		return ctrl.Run(ctx, prompt)
	}
}

// PermissionMode returns the current agent permission mode (for approval hub).
func (c *Controller) PermissionMode() permission.Mode {
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.ctrl == nil {
		return permission.ModeDefault
	}
	return c.ctrl.PermissionMode()
}

// makeApprover builds an SSE approval handler that blocks until /api/lab/approve.
// May return edited args JSON when the user modifies the approval card.
func (a *API) makeApprover(emit func(kind string, payload map[string]any)) permission.Asker {
	return func(ctx context.Context, toolName string, args json.RawMessage) (bool, json.RawMessage, error) {
		if a.approvals == nil {
			return false, nil, fmt.Errorf("approval hub not configured")
		}
		return a.approvals.decide(ctx, toolName, args, func(kind string, payload map[string]any) {
			if kind == "error" {
				if t, ok := payload["text"].(string); ok {
					emit("error", map[string]any{"text": t})
					return
				}
			}
			emit(kind, payload)
		})
	}
}
