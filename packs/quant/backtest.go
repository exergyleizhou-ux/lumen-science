package quant

import (
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"time"
)

// SandboxMode selects how the backtest engine is executed.
type SandboxMode string

const (
	// SandboxDocker runs run.py in a hardened --network=none container — the
	// default, because cutting the network removes the "fetch future data"
	// attack surface on top of the engine's structural no-lookahead.
	SandboxDocker SandboxMode = "docker"
	// SandboxLocal runs run.py directly with python3 (dev convenience, no
	// network isolation). Results are identical; only the isolation differs.
	SandboxLocal SandboxMode = "local"
)

// results is the bundle emitted by harness/run.py.
type results struct {
	EngineVersion   string             `json:"engine_version"`
	DataHash        string             `json:"data_hash"`
	EquityCurveHash string             `json:"equity_curve_hash"`
	Metrics         map[string]float64 `json:"metrics"`
	NTrades         int                `json:"n_trades"`
}

// BacktestOptions configures a run.
type BacktestOptions struct {
	Mode     SandboxMode
	DataPath string // defaults to <dir>/data.csv
	PythonAt string // python executable for local mode (default "python3")
}

// RunBacktest runs the strategy in dir and writes quant-cert.json + the lock.
func RunBacktest(dir string, opts BacktestOptions) (Cert, error) {
	m, err := LoadManifest(dir)
	if err != nil {
		return Cert{}, err
	}
	res, imageRef, err := runHarness(dir, m, opts)
	if err != nil {
		return Cert{}, err
	}
	srcHash, err := ComputeSrcHash(dir)
	if err != nil {
		return Cert{}, err
	}
	cert, err := WriteCert(dir, Cert{
		EngineVersion:   res.EngineVersion,
		SrcHash:         srcHash,
		DataHash:        res.DataHash,
		ImageDigest:     imageRef,
		EquityCurveHash: res.EquityCurveHash,
		Metrics:         res.Metrics,
		CreatedAt:       time.Now().UTC().Format(time.RFC3339),
		Recompute:       "lumen quant verify .",
	})
	if err != nil {
		return Cert{}, err
	}
	lock := Lock{
		Manifest: m, BuiltAt: cert.CreatedAt, Image: "quant:" + m.Name,
		Digest: imageRef, SrcHash: srcHash, DataHash: res.DataHash,
		EquityCurveHash: res.EquityCurveHash, CertID: cert.ID,
	}
	if err := WriteLock(dir, lock); err != nil {
		return Cert{}, err
	}
	return cert, nil
}

// runHarness stages the embedded engine + the strategy + the config in a temp
// workdir and runs run.py, returning the parsed results and an image reference.
// Used by both backtest (which writes the cert) and verify (which compares).
func runHarness(dir string, m Manifest, opts BacktestOptions) (results, string, error) {
	dataPath := opts.DataPath
	if dataPath == "" {
		dataPath = filepath.Join(dir, "data.csv")
	}
	absData, err := filepath.Abs(dataPath)
	if err != nil {
		return results{}, "", err
	}
	if _, err := os.Stat(absData); err != nil {
		return results{}, "", fmt.Errorf("dataset not found: %s (run `lumen quant data` or add data.csv)", dataPath)
	}

	work, err := os.MkdirTemp("", "quant-run-")
	if err != nil {
		return results{}, "", err
	}
	defer os.RemoveAll(work)

	// Stage the embedded engine.
	for name, body := range HarnessFiles {
		if err := os.WriteFile(filepath.Join(work, name), body, 0o644); err != nil {
			return results{}, "", err
		}
	}
	// Stage the strategy and the engine config derived from the manifest.
	stratSrc, err := os.ReadFile(filepath.Join(dir, "strategy.py"))
	if err != nil {
		return results{}, "", fmt.Errorf("read strategy.py: %w", err)
	}
	if err := os.WriteFile(filepath.Join(work, "strategy.py"), stratSrc, 0o644); err != nil {
		return results{}, "", err
	}
	cfgJSON, _ := json.Marshal(m.Config())
	if err := os.WriteFile(filepath.Join(work, "config.json"), cfgJSON, 0o644); err != nil {
		return results{}, "", err
	}

	mode := opts.Mode
	if mode == "" {
		mode = SandboxDocker
	}
	var imageRef string
	switch mode {
	case SandboxLocal:
		imageRef = "local"
		if err := runLocal(work, absData, opts.PythonAt); err != nil {
			return results{}, "", err
		}
	case SandboxDocker:
		ref, err := runDocker(work, absData)
		if err != nil {
			return results{}, "", err
		}
		imageRef = ref
	default:
		return results{}, "", fmt.Errorf("unknown sandbox mode %q", mode)
	}

	res, err := readResults(filepath.Join(work, "results.json"))
	return res, imageRef, err
}

func runLocal(work, absData, python string) error {
	if python == "" {
		python = "python3"
	}
	cmd := exec.Command(python, "run.py",
		"--data", absData,
		"--strategy", filepath.Join(work, "strategy.py"),
		"--config", filepath.Join(work, "config.json"),
		"--out", filepath.Join(work, "results.json"),
	)
	cmd.Dir = work
	cmd.Env = append(os.Environ(), "PYTHONHASHSEED=0", "PYTHONDONTWRITEBYTECODE=1")
	if out, err := cmd.CombinedOutput(); err != nil {
		return fmt.Errorf("backtest failed: %v\n%s", err, out)
	}
	return nil
}

// runDocker builds a pinned image from the staged context and runs the backtest
// with no network and a read-only dataset mount (mirrors oasis check.go).
func runDocker(work, absData string) (string, error) {
	dockerfile := "FROM python:3.11-slim\nWORKDIR /app\nCOPY . /app/\n" +
		"ENV PYTHONHASHSEED=0 PYTHONDONTWRITEBYTECODE=1\n"
	if err := os.WriteFile(filepath.Join(work, "Dockerfile"), []byte(dockerfile), 0o644); err != nil {
		return "", err
	}
	tag := "quant-sandbox:build"
	build := exec.Command("docker", "build", "-q", "-t", tag, work)
	if out, err := build.CombinedOutput(); err != nil {
		return "", fmt.Errorf("docker build failed: %v\n%s", err, out)
	}
	digest := imageID(tag)
	// No network; read-only dataset mount. run.py writes results inside the
	// container and we capture it from stdout (avoids needing a writable bind).
	run := exec.Command("docker", "run", "--rm",
		"--network=none",
		"-v", absData+":/data/data.csv:ro",
		tag, "sh", "-c",
		"python /app/run.py --data /data/data.csv --strategy /app/strategy.py "+
			"--config /app/config.json --out /app/results.json >/dev/null 2>&1 && cat /app/results.json",
	)
	out, err := run.Output()
	if err != nil {
		return "", fmt.Errorf("docker backtest failed: %v", err)
	}
	if err := os.WriteFile(filepath.Join(work, "results.json"), out, 0o644); err != nil {
		return "", err
	}
	return digest, nil
}

func imageID(tag string) string {
	out, err := exec.Command("docker", "image", "inspect", "-f", "{{.Id}}", tag).Output()
	if err != nil {
		return tag
	}
	return strings.TrimSpace(string(out))
}

func readResults(path string) (results, error) {
	b, err := os.ReadFile(path)
	if err != nil {
		return results{}, fmt.Errorf("read results.json: %w", err)
	}
	var r results
	if err := json.Unmarshal(b, &r); err != nil {
		return results{}, fmt.Errorf("parse results.json: %w", err)
	}
	return r, nil
}

// LoadManifest reads and parses dir/quant.toml.
func LoadManifest(dir string) (Manifest, error) {
	b, err := os.ReadFile(filepath.Join(dir, ManifestFile))
	if err != nil {
		return Manifest{}, fmt.Errorf("read %s: %w (run `lumen quant init` first)", ManifestFile, err)
	}
	return ParseManifest(string(b))
}
