package compute

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync"
	"time"
)

const (
	// MaxOutputBytes caps captured stdout/stderr stored on disk (anti-OOM).
	MaxOutputBytes = 1 << 20 // 1 MiB
	// DefaultJobTimeout bounds a single SSH job.
	DefaultJobTimeout = 30 * time.Minute
	// MaxJobTimeout is the hard ceiling even if caller asks for more.
	MaxJobTimeout = 2 * time.Hour
)

// JobOutput is one harvested artifact path (remote and optional local copy).
type JobOutput struct {
	Path      string `json:"path"`
	Size      int64  `json:"size,omitempty"`
	LocalPath string `json:"local_path,omitempty"`
	Error     string `json:"error,omitempty"`
}

// Job represents a detached compute job submitted to a remote host.
type Job struct {
	ID              string      `json:"id"`
	Host            string      `json:"host"`
	Command         string      `json:"command"`
	WorkDir         string      `json:"work_dir"`
	Status          string      `json:"status"` // pending, running, done, failed, timeout, cancelled
	CreatedAt       time.Time   `json:"created_at"`
	UpdatedAt       time.Time   `json:"updated_at"`
	PID             int         `json:"pid,omitempty"`
	ExitCode        int         `json:"exit_code,omitempty"`
	Output          string      `json:"output,omitempty"`
	OutputTruncated bool        `json:"output_truncated,omitempty"`
	TimeoutSec      int         `json:"timeout_sec,omitempty"`
	Error           string      `json:"error,omitempty"`
	OutputGlobs     []string    `json:"output_globs,omitempty"`
	Outputs         []JobOutput `json:"outputs,omitempty"`
	LocalOutDir     string      `json:"local_out_dir,omitempty"`
}

// SubmitOpts optional job parameters.
type SubmitOpts struct {
	Timeout     time.Duration
	OutputGlobs []string
	// LocalHarvestDir if set, scp matching files into this directory after success.
	LocalHarvestDir string
}

// Store manages job state on disk.
type Store struct {
	mu      sync.Mutex
	root    string
	cancels map[string]context.CancelFunc
}

// NewStore creates a job store under the project .lumen/compute directory.
func NewStore(projectDir string) (*Store, error) {
	root := filepath.Join(projectDir, ".lumen", "compute")
	if err := os.MkdirAll(root, 0o700); err != nil {
		return nil, err
	}
	return &Store{root: root, cancels: make(map[string]context.CancelFunc)}, nil
}

func (s *Store) jobPath(id string) string {
	return filepath.Join(s.root, id+".json")
}

func newJobID() string {
	b := make([]byte, 6)
	_, _ = rand.Read(b)
	return "job_" + hex.EncodeToString(b)
}

// Submit creates and starts a detached compute job.
func (s *Store) Submit(host, command, workDir string) (*Job, error) {
	return s.SubmitOpts(host, command, workDir, SubmitOpts{})
}

// SubmitOpts starts a job with optional timeout.
func (s *Store) SubmitOpts(host, command, workDir string, opts SubmitOpts) (*Job, error) {
	if host == "" || command == "" {
		return nil, fmt.Errorf("host and command required")
	}
	timeout := opts.Timeout
	if timeout <= 0 {
		timeout = DefaultJobTimeout
	}
	if timeout > MaxJobTimeout {
		timeout = MaxJobTimeout
	}

	s.mu.Lock()
	now := time.Now().UTC()
	j := &Job{
		ID:          newJobID(),
		Host:        host,
		Command:     command,
		WorkDir:     workDir,
		Status:      "pending",
		CreatedAt:   now,
		UpdatedAt:   now,
		TimeoutSec:  int(timeout.Seconds()),
		OutputGlobs: append([]string(nil), opts.OutputGlobs...),
		LocalOutDir: opts.LocalHarvestDir,
	}
	if err := s.saveLocked(j); err != nil {
		s.mu.Unlock()
		return nil, err
	}
	id := j.ID
	globs := append([]string(nil), opts.OutputGlobs...)
	localDir := opts.LocalHarvestDir
	s.mu.Unlock()

	go s.run(id, host, command, workDir, timeout, globs, localDir)
	return j, nil
}

// Cancel aborts a running job (if still active on this process).
func (s *Store) Cancel(id string) (*Job, error) {
	s.mu.Lock()
	cancel, ok := s.cancels[id]
	if ok {
		delete(s.cancels, id)
	}
	s.mu.Unlock()
	if ok && cancel != nil {
		cancel()
	}
	// Mark cancelled if still pending/running
	s.patch(id, func(j *Job) {
		if j.Status == "pending" || j.Status == "running" {
			j.Status = "cancelled"
			j.Error = "cancelled by user"
			j.ExitCode = -1
		}
	})
	return s.Get(id)
}

// IsLocalHost reports hosts that run on this machine without SSH.
func IsLocalHost(host string) bool {
	h := strings.ToLower(strings.TrimSpace(host))
	return h == "local" || h == "localhost" || h == "127.0.0.1" || h == "." || h == "local-shell"
}

func (s *Store) run(id, host, command, workDir string, timeout time.Duration, globs []string, localDir string) {
	s.patch(id, func(j *Job) {
		j.Status = "running"
		j.Output = ""
	})

	ctx, cancel := context.WithTimeout(context.Background(), timeout)
	s.mu.Lock()
	if s.cancels == nil {
		s.cancels = make(map[string]context.CancelFunc)
	}
	s.cancels[id] = cancel
	s.mu.Unlock()
	defer func() {
		cancel()
		s.mu.Lock()
		delete(s.cancels, id)
		s.mu.Unlock()
	}()

	var cmd *exec.Cmd
	if IsLocalHost(host) {
		shell := "sh"
		if _, lookErr := exec.LookPath("bash"); lookErr == nil {
			shell = "bash"
		}
		cmd = exec.CommandContext(ctx, shell, "-lc", command)
		if workDir != "" {
			cmd.Dir = workDir
		}
	} else {
		remote := command
		if workDir != "" {
			remote = "cd " + shellQuote(workDir) + " && " + command
		}
		cmd = exec.CommandContext(ctx, "ssh", "-o", "BatchMode=yes", "-o", "ConnectTimeout=15", host, remote)
	}
	cmd.Stdin = nil
	stdout, err := cmd.StdoutPipe()
	if err != nil {
		s.patch(id, func(j *Job) {
			j.Status = "failed"
			j.Error = err.Error()
			j.ExitCode = 1
		})
		return
	}
	cmd.Stderr = cmd.Stdout // merge

	if err := cmd.Start(); err != nil {
		s.patch(id, func(j *Job) {
			j.Status = "failed"
			j.Error = err.Error()
			j.ExitCode = 1
		})
		return
	}

	// Live-tail: append output as it streams so GET job shows progress.
	var acc strings.Builder
	buf := make([]byte, 4096)
	for {
		n, rerr := stdout.Read(buf)
		if n > 0 {
			acc.Write(buf[:n])
			cur, trunc := truncateOutput(acc.String(), MaxOutputBytes)
			s.patch(id, func(j *Job) {
				j.Output = cur
				j.OutputTruncated = trunc
			})
			if trunc {
				break
			}
		}
		if rerr != nil {
			break
		}
	}
	err = cmd.Wait()
	outStr, truncated := truncateOutput(acc.String(), MaxOutputBytes)

	// If user cancelled, preserve cancelled status
	if cur, gerr := s.Get(id); gerr == nil && cur.Status == "cancelled" {
		s.patch(id, func(j *Job) {
			j.Output = outStr
			j.OutputTruncated = truncated
			j.UpdatedAt = time.Now().UTC()
		})
		return
	}

	status := "done"
	exitCode := 0
	errMsg := ""
	if ctx.Err() == context.Canceled {
		status = "cancelled"
		exitCode = -1
		errMsg = "cancelled by user"
	} else if ctx.Err() == context.DeadlineExceeded {
		status = "timeout"
		exitCode = -1
		errMsg = fmt.Sprintf("exceeded timeout %s", timeout)
	} else if err != nil {
		status = "failed"
		errMsg = err.Error()
		if ee, ok := err.(*exec.ExitError); ok {
			exitCode = ee.ExitCode()
		} else {
			exitCode = 1
		}
	}

	var outputs []JobOutput
	if status == "done" && len(globs) > 0 {
		if IsLocalHost(host) {
			outputs = harvestLocal(workDir, globs, localDir, id)
		} else {
			outputs = harvestOutputs(host, workDir, globs, localDir, id)
		}
	}

	s.patch(id, func(j *Job) {
		// don't overwrite explicit cancelled from Cancel()
		if j.Status == "cancelled" {
			j.Output = outStr
			j.OutputTruncated = truncated
			return
		}
		j.Output = outStr
		j.OutputTruncated = truncated
		j.UpdatedAt = time.Now().UTC()
		j.Status = status
		j.ExitCode = exitCode
		j.Error = errMsg
		if len(outputs) > 0 {
			j.Outputs = outputs
		}
	})
}

// harvestLocal matches globs under workDir and copies into localDir/jobID/.
func harvestLocal(workDir string, globs []string, localDir, jobID string) []JobOutput {
	if len(globs) == 0 {
		return nil
	}
	base := workDir
	if base == "" {
		base, _ = os.Getwd()
	}
	var outs []JobOutput
	var destRoot string
	if localDir != "" {
		destRoot = filepath.Join(localDir, jobID)
		_ = os.MkdirAll(destRoot, 0o700)
	}
	seen := map[string]bool{}
	for _, g := range globs {
		pattern := g
		if !filepath.IsAbs(pattern) {
			pattern = filepath.Join(base, g)
		}
		matches, err := filepath.Glob(pattern)
		if err != nil {
			outs = append(outs, JobOutput{Path: g, Error: err.Error()})
			continue
		}
		for _, m := range matches {
			st, err := os.Stat(m)
			if err != nil || st.IsDir() {
				continue
			}
			rel, err := filepath.Rel(base, m)
			if err != nil {
				rel = filepath.Base(m)
			}
			rel = filepath.ToSlash(rel)
			if seen[rel] {
				continue
			}
			seen[rel] = true
			jo := JobOutput{Path: rel, Size: st.Size()}
			if destRoot != "" {
				dst := filepath.Join(destRoot, filepath.Base(m))
				if data, err := os.ReadFile(m); err != nil {
					jo.Error = err.Error()
				} else if err := os.WriteFile(dst, data, 0o600); err != nil {
					jo.Error = err.Error()
				} else {
					jo.LocalPath = dst
					jo.Size = int64(len(data))
				}
			}
			outs = append(outs, jo)
		}
	}
	return outs
}

// harvestOutputs lists remote files matching globs and optionally scp into localDir/jobID/.
// Pure-ish helper used by run; unit-tested with empty host for path logic via BuildHarvestList.
func harvestOutputs(host, workDir string, globs []string, localDir, jobID string) []JobOutput {
	if host == "" || len(globs) == 0 {
		return nil
	}
	if IsLocalHost(host) {
		return harvestLocal(workDir, globs, localDir, jobID)
	}
	// Remote: printf each match with size via stat when possible.
	// shell: cd workdir && for g in globs; do ls -1 $g 2>/dev/null; done
	var script strings.Builder
	if workDir != "" {
		script.WriteString("cd " + shellQuote(workDir) + " && ")
	}
	script.WriteString("for g in")
	for _, g := range globs {
		script.WriteString(" " + shellQuote(g))
	}
	script.WriteString("; do for f in $g; do [ -f \"$f\" ] && printf '%s\\t%s\\n' \"$f\" \"$(wc -c < \"$f\" 2>/dev/null || echo 0)\"; done; done")
	cmd := exec.Command("ssh", "-o", "BatchMode=yes", "-o", "ConnectTimeout=10", host, script.String())
	out, err := cmd.CombinedOutput()
	if err != nil {
		return []JobOutput{{Path: strings.Join(globs, ","), Error: "harvest list failed: " + err.Error()}}
	}
	lines := strings.Split(strings.TrimSpace(string(out)), "\n")
	var outs []JobOutput
	var destRoot string
	if localDir != "" {
		destRoot = filepath.Join(localDir, jobID)
		_ = os.MkdirAll(destRoot, 0o700)
	}
	for _, line := range lines {
		line = strings.TrimSpace(line)
		if line == "" {
			continue
		}
		parts := strings.SplitN(line, "\t", 2)
		rel := strings.TrimSpace(parts[0])
		if rel == "" {
			continue
		}
		jo := JobOutput{Path: rel}
		if len(parts) > 1 {
			var sz int64
			fmt.Sscanf(strings.TrimSpace(parts[1]), "%d", &sz)
			jo.Size = sz
		}
		if destRoot != "" {
			// scp host:workDir/rel dest
			remotePath := rel
			if workDir != "" {
				remotePath = filepath.Join(workDir, rel)
			}
			localPath := filepath.Join(destRoot, filepath.Base(rel))
			scp := exec.Command("scp", "-o", "BatchMode=yes", "-o", "ConnectTimeout=10",
				host+":"+remotePath, localPath)
			if err := scp.Run(); err != nil {
				jo.Error = "scp: " + err.Error()
			} else {
				jo.LocalPath = localPath
				if st, err := os.Stat(localPath); err == nil {
					jo.Size = st.Size()
				}
			}
		}
		outs = append(outs, jo)
	}
	return outs
}

// ParseHarvestLines is a pure helper for tests: "path\\tsize" lines → JobOutput.
func ParseHarvestLines(raw string) []JobOutput {
	var outs []JobOutput
	for _, line := range strings.Split(raw, "\n") {
		line = strings.TrimSpace(line)
		if line == "" {
			continue
		}
		parts := strings.SplitN(line, "\t", 2)
		jo := JobOutput{Path: strings.TrimSpace(parts[0])}
		if len(parts) > 1 {
			var sz int64
			fmt.Sscanf(strings.TrimSpace(parts[1]), "%d", &sz)
			jo.Size = sz
		}
		outs = append(outs, jo)
	}
	return outs
}

func truncateOutput(s string, max int) (string, bool) {
	if max <= 0 || len(s) <= max {
		return s, false
	}
	// Keep head + note
	keep := max - 80
	if keep < 0 {
		keep = max
	}
	return s[:keep] + "\n…[truncated]…\n", true
}

func shellQuote(s string) string {
	return "'" + strings.ReplaceAll(s, "'", `'"'"'`) + "'"
}

// Get loads a job by ID.
func (s *Store) Get(id string) (*Job, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.loadLocked(id)
}

// List returns all jobs for this project.
func (s *Store) List() ([]*Job, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	entries, err := os.ReadDir(s.root)
	if err != nil {
		return nil, err
	}
	var jobs []*Job
	for _, e := range entries {
		if e.IsDir() || filepath.Ext(e.Name()) != ".json" {
			continue
		}
		id := e.Name()[:len(e.Name())-5]
		j, err := s.loadLocked(id)
		if err != nil {
			continue
		}
		jobs = append(jobs, j)
	}
	return jobs, nil
}

func (s *Store) loadLocked(id string) (*Job, error) {
	data, err := os.ReadFile(s.jobPath(id))
	if err != nil {
		return nil, err
	}
	var j Job
	if err := json.Unmarshal(data, &j); err != nil {
		return nil, err
	}
	return &j, nil
}

func (s *Store) saveLocked(j *Job) error {
	data, err := json.MarshalIndent(j, "", "  ")
	if err != nil {
		return err
	}
	return os.WriteFile(s.jobPath(j.ID), append(data, '\n'), 0o600)
}

func (s *Store) patch(id string, fn func(*Job)) {
	s.mu.Lock()
	defer s.mu.Unlock()
	j, err := s.loadLocked(id)
	if err != nil {
		return
	}
	fn(j)
	j.UpdatedAt = time.Now().UTC()
	_ = s.saveLocked(j)
}
