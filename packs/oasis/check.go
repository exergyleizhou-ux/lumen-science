package oasis

import (
	"archive/zip"
	"bytes"
	"context"
	"errors"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
)

// sentinelDataset is the synthetic dataset the contract self-check seeds. The
// numeric columns (v, w, x) exercise the algorithm; the non-numeric _sentinel
// column carries a unique recognizable token per row. An aggregates-only C2D
// algorithm never emits those tokens, so echoing them back reveals a raw-row leak.
func sentinelDataset() ([]byte, []string) { return sentinelDatasetN(30) }

// sentinelDatasetN builds the sentinel dataset with n rows. Every row gets a
// distinct, high-entropy _sentinel token, so a larger n yields more *distinct*
// rows (not repeats) — that is what lets the row-scaling lint distinguish a
// per-row leak (incompressible, grows with n) from an aggregate (constant size).
func sentinelDatasetN(n int) ([]byte, []string) {
	var b strings.Builder
	b.WriteString("v,w,x,_sentinel\n")
	var sentinels []string
	for i := 0; i < n; i++ {
		s := fmt.Sprintf("ZqLeakTok%04d", i)
		sentinels = append(sentinels, s)
		fmt.Fprintf(&b, "%d,%d,%d,%s\n", i+1, (i*7)%50+1, (i*3)%40+2, s)
	}
	return []byte(b.String()), sentinels
}

// rowScaleFactor is how many times more rows the second privacy probe uses. The
// large probe has rowScaleFactor× the rows with distinct high-entropy values, so
// a true aggregate's output stays ~constant while a per-row leak grows ~linearly.
const rowScaleFactor = 10

// scalingViolation reports an output that scaled with the row count. An
// aggregate's serialized size is invariant to N (a mean is one number whether
// N=30 or N=300), so any >2× expansion across a rowScaleFactor× row increase
// means per-row data is leaking. 2× is a generous floor: a full row-dump grows
// ~rowScaleFactor×, so this catches even partial leaks while leaving wide margin
// for count-digit growth (e.g. "30" → "300") in a legitimate aggregate.
func scalingViolation(smallSize, largeSize int) bool {
	if smallSize == 0 {
		return false
	}
	return largeSize > smallSize*2
}

// leakCount returns how many per-row sentinel tokens appear verbatim in the
// output (a zip of model.json + metrics.json).
func leakCount(outputBin []byte, sentinels []string) int {
	zr, err := zip.NewReader(bytes.NewReader(outputBin), int64(len(outputBin)))
	if err != nil {
		return 0
	}
	var text strings.Builder
	for _, f := range zr.File {
		rc, err := f.Open()
		if err != nil {
			continue
		}
		bs, _ := io.ReadAll(rc)
		rc.Close()
		text.Write(bs)
	}
	s := text.String()
	n := 0
	for _, sen := range sentinels {
		if strings.Contains(s, sen) {
			n++
		}
	}
	return n
}

// ── C2D runtime contract self-check ────────────────────────
//
// The marketplace runner executes an algorithm image with strict isolation
// (`docker run --network none --read-only -v <data>:/data:ro -v <params>:/params.json:ro -v <out>:/out`),
// mounts the params at /params.json, and reads **/out/output.bin** as the single
// result object — which it then hashes + Ed25519-attests. So a contract-compliant
// algorithm must: read its dataset from /data and params from /params.json, run
// with no network and a read-only root, and write its result to /out/output.bin.
// For a "model" output_kind that file is a zip of model.json (+ metrics.json).
// CheckContract runs the image exactly that way and validates the result, so
// authors catch violations before pushing to Oasis.

// CheckOutput validates the bytes of /out/output.bin against the contract: it
// must be non-empty (the runner reads exactly this file). For output_kind
// "model" it must be a zip archive containing model.json. Returns violations
// (empty = OK).
func CheckOutput(data []byte, kind string) []string {
	if len(data) == 0 {
		return []string{"output is empty — the algorithm must write its result to /out/output.bin (the runner reads exactly that file)"}
	}
	if kind == "model" {
		zr, err := zip.NewReader(bytes.NewReader(data), int64(len(data)))
		if err != nil {
			return []string{`output_kind "model" expects /out/output.bin to be a zip of model.json (+ metrics.json), but it is not a valid zip`}
		}
		has := map[string]bool{}
		for _, f := range zr.File {
			has[f.Name] = true
		}
		if !has["model.json"] {
			return []string{`model output.bin (zip) must contain model.json`}
		}
	}
	return nil
}

// SandboxRunner runs an algorithm image under the C2D isolation contract; the
// algorithm writes its result to <outDir>/output.bin. Abstracted so the
// self-check can be unit-tested without Docker.
type SandboxRunner interface {
	Run(ctx context.Context, image, dataDir, paramsFile, outDir string) (logs string, err error)
}

// dockerSandbox runs the image exactly as the marketplace runner does.
type dockerSandbox struct{}

func (dockerSandbox) Run(ctx context.Context, image, dataDir, paramsFile, outDir string) (string, error) {
	return runDockerSandbox(ctx, image, dataDir, paramsFile, outDir)
}

// runDockerSandbox runs the image with exactly the marketplace runner's
// isolation: no network, read-only root, dataset read-only, /out writable.
func runDockerSandbox(ctx context.Context, image, dataDir, paramsFile, outDir string) (string, error) {
	cmd := exec.CommandContext(ctx, "docker", "run", "--rm",
		"--network", "none",
		"--read-only",
		"--tmpfs", "/tmp",
		"-v", dataDir+":/data:ro",
		"-v", paramsFile+":/params.json:ro",
		"-v", outDir+":/out",
		image,
	)
	var stderr bytes.Buffer
	cmd.Stderr = &stderr
	err := cmd.Run()
	return stderr.String(), err
}

// CheckResult is the outcome of a contract self-check.
type CheckResult struct {
	OK         bool
	Violations []string
	Logs       string
}

// DockerAvailable reports whether the docker CLI is on PATH.
func DockerAvailable() bool {
	_, err := exec.LookPath("docker")
	return err == nil
}

// RunContractCheck runs the contract self-check against the real Docker sandbox.
func RunContractCheck(ctx context.Context, image, kind string, sampleData []byte) CheckResult {
	return CheckContract(ctx, dockerSandbox{}, image, kind, sampleData)
}

// errNoOutput signals that a probe ran but the algorithm wrote no /out/output.bin.
var errNoOutput = errors.New("algorithm did not write /out/output.bin")

// runProbe seeds dataset.csv + an empty params.json into fresh scratch dirs, runs
// the image under the C2D isolation, and returns the bytes the algorithm wrote to
// /out/output.bin. err is errNoOutput when nothing was written, the run error
// when the container failed, or a setup error otherwise.
func runProbe(ctx context.Context, run SandboxRunner, image string, dataset []byte) (out []byte, logs string, err error) {
	dataDir, err := os.MkdirTemp("", "oasis-data-*")
	if err != nil {
		return nil, "", fmt.Errorf("scratch data dir: %w", err)
	}
	defer os.RemoveAll(dataDir)
	outDir, err := os.MkdirTemp("", "oasis-out-*")
	if err != nil {
		return nil, "", fmt.Errorf("scratch out dir: %w", err)
	}
	defer os.RemoveAll(outDir)
	paramsDir, err := os.MkdirTemp("", "oasis-params-*")
	if err != nil {
		return nil, "", fmt.Errorf("scratch params dir: %w", err)
	}
	defer os.RemoveAll(paramsDir)

	if err := os.WriteFile(filepath.Join(dataDir, "dataset.csv"), dataset, 0o644); err != nil {
		return nil, "", fmt.Errorf("seed dataset: %w", err)
	}
	paramsFile := filepath.Join(paramsDir, "params.json")
	if err := os.WriteFile(paramsFile, []byte(`{}`), 0o644); err != nil {
		return nil, "", fmt.Errorf("seed params.json: %w", err)
	}

	logs, runErr := run.Run(ctx, image, dataDir, paramsFile, outDir)
	if runErr != nil {
		return nil, logs, runErr
	}
	// The runner reads exactly /out/output.bin — so do we.
	out, rerr := os.ReadFile(filepath.Join(outDir, "output.bin"))
	if rerr != nil {
		return nil, logs, errNoOutput
	}
	return out, logs, nil
}

// CheckContract runs the algorithm image under the real C2D isolation against a
// synthetic dataset and validates that it produces a contract-compliant
// /out/output.bin. run is the sandbox (use dockerSandbox{} in production, a fake
// in tests). sampleData seeds /data; kind is the declared output_kind.
//
// With no author-supplied data it seeds the synthetic sentinel dataset and runs
// two complementary privacy lints: (1) raw-row echo — the output reproduces
// seeded per-row tokens; and (2) row-count scaling — a second probe with
// rowScaleFactor× the rows must not balloon the output (aggregate output is
// invariant to N, so linear growth means per-row data is leaking, even when it
// is derived/hashed and the echo lint cannot see it).
func CheckContract(ctx context.Context, run SandboxRunner, image, kind string, sampleData []byte) CheckResult {
	var sentinels []string
	small := sampleData
	if len(small) == 0 {
		small, sentinels = sentinelDataset()
	}

	out, logs, err := runProbe(ctx, run, image, small)
	if err != nil {
		if errors.Is(err, errNoOutput) {
			return CheckResult{
				Violations: []string{"the algorithm did not write /out/output.bin — that is the single result file the runner reads"},
				Logs:       logs,
			}
		}
		return CheckResult{
			Violations: []string{fmt.Sprintf("algorithm did not run cleanly under `--network none --read-only`: %v", err)},
			Logs:       logs,
		}
	}

	violations := CheckOutput(out, kind)

	// Privacy lints only run on the synthetic dataset (we control its shape).
	if len(sentinels) > 0 {
		// Lint 1 — raw-row echo: the output reproduces a majority of the seeded
		// per-row tokens, i.e. it is dumping raw rows rather than aggregates.
		if n := leakCount(out, sentinels); n*2 > len(sentinels) {
			violations = append(violations, fmt.Sprintf(
				"possible PRIVACY LEAK: the output echoes %d/%d seeded per-row values — a C2D algorithm must emit only aggregates, never raw rows",
				n, len(sentinels)))
		}
		// Lint 2 — row-count scaling: a second probe with rowScaleFactor× the
		// rows (distinct high-entropy values) must not balloon the output.
		large, _ := sentinelDatasetN(len(sentinels) * rowScaleFactor)
		if largeOut, _, lerr := runProbe(ctx, run, image, large); lerr == nil {
			if scalingViolation(len(out), len(largeOut)) {
				violations = append(violations, fmt.Sprintf(
					"possible PRIVACY LEAK: output grew from %d to %d bytes (>2×) when the row count grew %d× — aggregate output must not scale with the number of rows",
					len(out), len(largeOut), rowScaleFactor))
			}
		}
	}
	return CheckResult{OK: len(violations) == 0, Violations: violations, Logs: logs}
}
