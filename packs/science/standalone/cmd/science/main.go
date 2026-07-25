package main

import (
	"context"
	"crypto/sha256"
	"encoding/json"
	"flag"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/lumen-ai/lumen-science/mcp/artifacts"
	"github.com/lumen-ai/lumen-science/standalone/internal/brief"
	"github.com/lumen-ai/lumen-science/standalone/internal/pipeline"
	"github.com/lumen-ai/lumen-science/standalone/internal/seqbench"
)

// version is stamped by -ldflags at release builds.
var version = "1.0.0"

const usage = `lumen-science — productive local scientific workbench

Authority model:
  SessionActor (Rust Lumen) is the sole product execution authority.
  This CLI is a local productivity adapter: artifacts are SHA-256 registered,
  Motif-class sequence analysis is offline, live network requires intent.

Usage:
  lumen-science version
  lumen-science doctor [--root PATH]
  lumen-science gates                  # machine honesty gates (repo root)
  lumen-science brief [--out PATH] [--timeout 30s] TOPIC
  lumen-science seq analyze [--json|--md] FILE
  lumen-science artifact put  --project P --run R --path REL [--label L] FILE
  lumen-science artifact list --project P --run R
  lumen-science artifact get  --project P --run R --path REL
  lumen-science artifact verify --project P --run R --path REL --sha256 HEX
  lumen-science pipeline offline --project P --run R FILE
      Offline loop: register FASTA → seqbench → derived artifacts → integrity review

Notes:
  brief talks to PubMed/ChEMBL (live). seq/artifact/pipeline offline are default-safe.
  Not medical advice. Not unsupervised lab control.
`

func main() {
	if len(os.Args) < 2 {
		fmt.Fprint(os.Stderr, usage)
		os.Exit(2)
	}
	var err error
	switch os.Args[1] {
	case "version", "--version", "-V":
		fmt.Printf("lumen-science %s\n", version)
		return
	case "doctor":
		err = runDoctor(os.Args[2:])
	case "gates":
		err = runGates(os.Args[2:])
	case "brief":
		err = runBrief(os.Args[2:])
	case "seq":
		err = runSeq(os.Args[2:])
	case "artifact":
		err = runArtifact(os.Args[2:])
	case "pipeline":
		err = runPipeline(os.Args[2:])
	case "help", "-h", "--help":
		fmt.Print(usage)
		return
	default:
		err = fmt.Errorf("unknown command %q\n\n%s", os.Args[1], usage)
	}
	if err != nil {
		fmt.Fprintln(os.Stderr, "lumen-science:", err)
		os.Exit(1)
	}
}

func runGates(args []string) error {
	flags := flag.NewFlagSet("gates", flag.ContinueOnError)
	flags.SetOutput(os.Stderr)
	root := flags.String("root", "", "repo root containing scripts/science-machine-gates.sh")
	if err := flags.Parse(args); err != nil {
		return err
	}
	repo, err := findRepoRoot(*root)
	if err != nil {
		return err
	}
	script := filepath.Join(repo, "scripts", "science-machine-gates.sh")
	if _, err := os.Stat(script); err != nil {
		return fmt.Errorf("gates script missing: %w", err)
	}
	// Re-implement critical gates in-process so binary works even if bash not ideal.
	return runInProcessGates(repo)
}

func runInProcessGates(repo string) error {
	// 1) lock file
	lockPath := filepath.Join(repo, "docs/science/fusion-sources.lock.json")
	raw, err := os.ReadFile(lockPath)
	if err != nil {
		return err
	}
	var lock struct {
		Items []struct {
			ConnectorID       string  `json:"connector_id"`
			FinalDisposition  *string `json:"final_disposition"`
		} `json:"items"`
		RendererSources []json.RawMessage `json:"renderer_sources"`
	}
	if err := json.Unmarshal(raw, &lock); err != nil {
		return fmt.Errorf("parse lock: %w", err)
	}
	if len(lock.Items) != 42 {
		return fmt.Errorf("lock items want 42 got %d", len(lock.Items))
	}
	for _, it := range lock.Items {
		if it.FinalDisposition == nil {
			return fmt.Errorf("unresolved disposition: %s", it.ConnectorID)
		}
	}
	if len(lock.RendererSources) == 0 {
		return fmt.Errorf("motif renderer_sources missing")
	}
	fmt.Println("PASS  fusion-sources.lock.json 42/0 + motif")

	// 2) skills honesty — approved only with prompt-injection pass + controlled tools
	regPath := filepath.Join(repo, "packs/science/skills/registry.json")
	regRaw, err := os.ReadFile(regPath)
	if err != nil {
		return err
	}
	var reg struct {
		SchemaVersion int `json:"schema_version"`
		Summary       struct {
			Approved int `json:"approved"`
			Total    int `json:"total"`
		} `json:"summary"`
		Skills []map[string]any `json:"skills"`
	}
	if err := json.Unmarshal(regRaw, &reg); err != nil {
		return err
	}
	if reg.SchemaVersion < 2 {
		return fmt.Errorf("skills schema_version < 2")
	}
	approvedCount := 0
	for _, s := range reg.Skills {
		sid, _ := s["skill_id"].(string)
		disp, _ := s["final_disposition"].(string)
		perms, _ := s["runtime_permissions"].(map[string]any)
		if perms == nil {
			return fmt.Errorf("skill %s missing runtime_permissions", sid)
		}
		if ind, ok := perms["independent_execution_authority"].(bool); ok && ind {
			return fmt.Errorf("skill %s claims independent execution authority", sid)
		}
		if disp == "approved" {
			approvedCount++
			audit, _ := s["prompt_injection_audit"].(map[string]any)
			if audit == nil || audit["status"] != "pass" {
				return fmt.Errorf("skill %s approved without prompt_injection_audit.pass", sid)
			}
			tools, _ := perms["controlled_tools"].([]any)
			if len(tools) < 1 {
				return fmt.Errorf("skill %s approved without controlled_tools", sid)
			}
		}
	}
	if approvedCount != reg.Summary.Approved {
		return fmt.Errorf("skills summary.approved=%d but found %d approved dispositions", reg.Summary.Approved, approvedCount)
	}
	fmt.Printf("PASS  skills registry v%d total=%d approved=%d\n", reg.SchemaVersion, reg.Summary.Total, approvedCount)

	// 3) motif contract
	motif := filepath.Join(repo, "packs/science/renderers/static/motif.html")
	mb, err := os.ReadFile(motif)
	if err != nil {
		return err
	}
	if !strings.Contains(string(mb), "Content-Security-Policy") {
		return fmt.Errorf("motif.html missing CSP")
	}
	fmt.Println("PASS  MotifRenderer contract page")
	fmt.Println("OK    science machine gates")
	return nil
}

func runDoctor(args []string) error {
	flags := flag.NewFlagSet("doctor", flag.ContinueOnError)
	flags.SetOutput(os.Stderr)
	root := flags.String("root", "", "packs/science directory; default: auto-detect")
	if err := flags.Parse(args); err != nil {
		return err
	}
	if flags.NArg() != 0 {
		return fmt.Errorf("doctor accepts no positional arguments")
	}
	packRoot, err := findPackRoot(*root)
	if err != nil {
		return err
	}
	required := []string{
		"README.md",
		"proxy",
		"lab",
		"go.mod",
		"standalone/cmd/science/main.go",
		"standalone/internal/seqbench/seqbench.go",
		"standalone/internal/pipeline/offline.go",
		"renderers/static/motif.html",
		"skills/registry.json",
	}
	failures := 0
	for _, relative := range required {
		path := filepath.Join(packRoot, relative)
		if _, err := os.Stat(path); err != nil {
			fmt.Printf("FAIL  %s: %v\n", relative, err)
			failures++
		} else {
			fmt.Printf("PASS  %s\n", relative)
		}
	}
	// productivity self-check
	recs, err := seqbench.ParseFASTA(">doc\nATGC")
	if err != nil || len(recs) != 1 {
		fmt.Println("FAIL  seqbench self-check")
		failures++
	} else {
		fmt.Println("PASS  seqbench self-check")
	}
	if os.Getenv("NCBI_API_KEY") == "" {
		fmt.Println("WARN  NCBI_API_KEY unset (live brief rate-limited)")
	} else {
		fmt.Println("PASS  NCBI_API_KEY present (value hidden)")
	}
	fmt.Printf("INFO  version %s\n", version)
	if failures > 0 {
		return fmt.Errorf("doctor found %d blocking issue(s)", failures)
	}
	fmt.Println("OK    lumen-science productivity path ready")
	return nil
}

func runBrief(args []string) error {
	flags := flag.NewFlagSet("brief", flag.ContinueOnError)
	flags.SetOutput(os.Stderr)
	outPath := flags.String("out", "", "write Markdown atomically; default: stdout")
	timeout := flags.Duration("timeout", 30*time.Second, "overall timeout")
	maxArticles := flags.Int("max-articles", 5, "maximum PubMed records")
	maxCompounds := flags.Int("max-compounds", 3, "maximum ChEMBL compounds")
	if err := flags.Parse(args); err != nil {
		return err
	}
	topic := strings.TrimSpace(strings.Join(flags.Args(), " "))
	if topic == "" {
		return fmt.Errorf("brief requires a topic")
	}
	if *timeout <= 0 {
		return fmt.Errorf("--timeout must be greater than zero")
	}
	ctx, cancel := context.WithTimeout(context.Background(), *timeout)
	defer cancel()
	result, err := brief.NewClient(*timeout).Generate(ctx, topic, *maxArticles, *maxCompounds)
	if err != nil {
		return err
	}
	markdown := result.Markdown()
	if *outPath == "" {
		fmt.Print(markdown)
		return nil
	}
	if err := writeAtomic(*outPath, []byte(markdown)); err != nil {
		return err
	}
	fmt.Printf("wrote %s (PubMed=%d ChEMBL=%d warnings=%d)\n", *outPath, len(result.Articles), len(result.Compounds), len(result.Warnings))
	return nil
}

func runSeq(args []string) error {
	if len(args) < 1 {
		return fmt.Errorf("seq requires subcommand: analyze")
	}
	switch args[0] {
	case "analyze":
		return runSeqAnalyze(args[1:])
	default:
		return fmt.Errorf("unknown seq subcommand %q", args[0])
	}
}

func runSeqAnalyze(args []string) error {
	flags := flag.NewFlagSet("seq analyze", flag.ContinueOnError)
	flags.SetOutput(os.Stderr)
	asJSON := flags.Bool("json", false, "emit analysis JSON")
	asMD := flags.Bool("md", true, "emit markdown report (default)")
	out := flags.String("out", "", "write to file")
	if err := flags.Parse(args); err != nil {
		return err
	}
	if flags.NArg() != 1 {
		return fmt.Errorf("usage: lumen-science seq analyze [--json|--md] [--out PATH] FILE")
	}
	path := flags.Arg(0)
	raw, err := os.ReadFile(path)
	if err != nil {
		return err
	}
	recs, err := seqbench.ParseFASTA(string(raw))
	if err != nil {
		return err
	}
	analysis := seqbench.Analyze(recs)
	var body []byte
	if *asJSON {
		body, err = json.MarshalIndent(analysis, "", "  ")
		if err != nil {
			return err
		}
		body = append(body, '\n')
	} else {
		_ = asMD
		body = []byte(seqbench.MarkdownReport(analysis, filepath.Base(path)))
	}
	if *out == "" {
		os.Stdout.Write(body)
		return nil
	}
	return writeAtomic(*out, body)
}

func runArtifact(args []string) error {
	if len(args) < 1 {
		return fmt.Errorf("artifact requires subcommand: put|list|get|verify")
	}
	switch args[0] {
	case "put":
		return artifactPut(args[1:])
	case "list":
		return artifactList(args[1:])
	case "get":
		return artifactGet(args[1:])
	case "verify":
		return artifactVerify(args[1:])
	default:
		return fmt.Errorf("unknown artifact subcommand %q", args[0])
	}
}

func artifactStore(root string) (*artifacts.Store, error) {
	if root != "" {
		return artifacts.NewStoreAt(root)
	}
	return artifacts.NewStore()
}

func artifactPut(args []string) error {
	flags := flag.NewFlagSet("artifact put", flag.ContinueOnError)
	flags.SetOutput(os.Stderr)
	project := flags.String("project", "", "project id")
	run := flags.String("run", "", "run id")
	rel := flags.String("path", "", "relative artifact path")
	label := flags.String("label", "", "label")
	mime := flags.String("mime", "application/octet-stream", "mime type")
	storeRoot := flags.String("store", "", "artifact store root")
	if err := flags.Parse(args); err != nil {
		return err
	}
	if *project == "" || *run == "" || *rel == "" || flags.NArg() != 1 {
		return fmt.Errorf("usage: artifact put --project P --run R --path REL FILE")
	}
	data, err := os.ReadFile(flags.Arg(0))
	if err != nil {
		return err
	}
	store, err := artifactStore(*storeRoot)
	if err != nil {
		return err
	}
	meta, err := store.Write(*project, *run, *rel, *label, *mime, data)
	if err != nil {
		return err
	}
	enc := json.NewEncoder(os.Stdout)
	enc.SetIndent("", "  ")
	return enc.Encode(meta)
}

func artifactList(args []string) error {
	flags := flag.NewFlagSet("artifact list", flag.ContinueOnError)
	flags.SetOutput(os.Stderr)
	project := flags.String("project", "", "project id")
	run := flags.String("run", "", "run id")
	storeRoot := flags.String("store", "", "artifact store root")
	if err := flags.Parse(args); err != nil {
		return err
	}
	if *project == "" || *run == "" {
		return fmt.Errorf("usage: artifact list --project P --run R")
	}
	store, err := artifactStore(*storeRoot)
	if err != nil {
		return err
	}
	list, err := store.List(*project, *run)
	if err != nil {
		return err
	}
	enc := json.NewEncoder(os.Stdout)
	enc.SetIndent("", "  ")
	return enc.Encode(list)
}

func artifactGet(args []string) error {
	flags := flag.NewFlagSet("artifact get", flag.ContinueOnError)
	flags.SetOutput(os.Stderr)
	project := flags.String("project", "", "project id")
	run := flags.String("run", "", "run id")
	rel := flags.String("path", "", "relative path")
	storeRoot := flags.String("store", "", "store root")
	if err := flags.Parse(args); err != nil {
		return err
	}
	if *project == "" || *run == "" || *rel == "" {
		return fmt.Errorf("usage: artifact get --project P --run R --path REL")
	}
	store, err := artifactStore(*storeRoot)
	if err != nil {
		return err
	}
	data, meta, err := store.Read(*project, *run, *rel)
	if err != nil {
		return err
	}
	if meta != nil {
		fmt.Fprintf(os.Stderr, "sha256=%s bytes=%d\n", meta.SHA256, meta.Bytes)
	}
	_, err = os.Stdout.Write(data)
	return err
}

func artifactVerify(args []string) error {
	flags := flag.NewFlagSet("artifact verify", flag.ContinueOnError)
	flags.SetOutput(os.Stderr)
	project := flags.String("project", "", "project id")
	run := flags.String("run", "", "run id")
	rel := flags.String("path", "", "relative path")
	want := flags.String("sha256", "", "expected sha256 hex")
	storeRoot := flags.String("store", "", "store root")
	if err := flags.Parse(args); err != nil {
		return err
	}
	if *project == "" || *run == "" || *rel == "" || *want == "" {
		return fmt.Errorf("usage: artifact verify --project P --run R --path REL --sha256 HEX")
	}
	store, err := artifactStore(*storeRoot)
	if err != nil {
		return err
	}
	data, meta, err := store.Read(*project, *run, *rel)
	if err != nil {
		return err
	}
	sum := fmt.Sprintf("%x", sha256.Sum256(data))
	if !strings.EqualFold(sum, *want) || (meta != nil && !strings.EqualFold(meta.SHA256, *want)) {
		return fmt.Errorf("VERIFY FAIL: got %s want %s", sum, *want)
	}
	fmt.Println("VERIFY PASS")
	return nil
}

func runPipeline(args []string) error {
	if len(args) < 1 || args[0] != "offline" {
		return fmt.Errorf("usage: lumen-science pipeline offline --project P --run R FILE")
	}
	flags := flag.NewFlagSet("pipeline offline", flag.ContinueOnError)
	flags.SetOutput(os.Stderr)
	project := flags.String("project", "local", "project id")
	run := flags.String("run", "", "run id (default: timestamp)")
	storeRoot := flags.String("store", "", "artifact store root")
	if err := flags.Parse(args[1:]); err != nil {
		return err
	}
	if flags.NArg() != 1 {
		return fmt.Errorf("pipeline offline requires a FASTA file")
	}
	if *run == "" {
		*run = time.Now().UTC().Format("20060102T150405Z")
	}
	raw, err := os.ReadFile(flags.Arg(0))
	if err != nil {
		return err
	}
	root := *storeRoot
	if root == "" {
		root, err = pipeline.DefaultStoreRoot()
		if err != nil {
			return err
		}
	}
	res, err := pipeline.RunSeqOffline(root, *project, *run, filepath.Base(flags.Arg(0)), raw)
	if err != nil {
		return err
	}
	enc := json.NewEncoder(os.Stdout)
	enc.SetIndent("", "  ")
	if err := enc.Encode(res); err != nil {
		return err
	}
	if res.Review.Status != "pass" {
		return fmt.Errorf("pipeline review status=%s failed=%v", res.Review.Status, res.Review.Failed)
	}
	fmt.Fprintf(os.Stderr, "OK pipeline offline project=%s run=%s report=%s\n",
		res.ProjectID, res.RunID, res.ReportArtifact.Path)
	return nil
}

func findPackRoot(explicit string) (string, error) {
	if explicit != "" {
		return filepath.Abs(explicit)
	}
	candidates := []string{"."}
	if executable, err := os.Executable(); err == nil {
		candidates = append(candidates, filepath.Dir(executable), filepath.Dir(filepath.Dir(executable)))
	}
	// walk up from cwd
	if cwd, err := os.Getwd(); err == nil {
		dir := cwd
		for i := 0; i < 8; i++ {
			candidates = append(candidates, dir, filepath.Join(dir, "packs", "science"))
			parent := filepath.Dir(dir)
			if parent == dir {
				break
			}
			dir = parent
		}
	}
	for _, candidate := range candidates {
		absolute, err := filepath.Abs(candidate)
		if err != nil {
			continue
		}
		if _, err := os.Stat(filepath.Join(absolute, "go.mod")); err == nil {
			if _, err2 := os.Stat(filepath.Join(absolute, "standalone", "cmd", "science", "main.go")); err2 == nil {
				return absolute, nil
			}
		}
	}
	return "", fmt.Errorf("cannot locate packs/science; run from that directory or pass --root")
}

func findRepoRoot(explicit string) (string, error) {
	if explicit != "" {
		return filepath.Abs(explicit)
	}
	if cwd, err := os.Getwd(); err == nil {
		dir := cwd
		for i := 0; i < 10; i++ {
			if _, err := os.Stat(filepath.Join(dir, "docs", "science", "fusion-sources.lock.json")); err == nil {
				return dir, nil
			}
			parent := filepath.Dir(dir)
			if parent == dir {
				break
			}
			dir = parent
		}
	}
	pack, err := findPackRoot("")
	if err == nil {
		return filepath.Abs(filepath.Join(pack, "..", ".."))
	}
	return "", fmt.Errorf("cannot locate lumen-science repo root")
}

func writeAtomic(path string, data []byte) error {
	absolute, err := filepath.Abs(path)
	if err != nil {
		return fmt.Errorf("resolve output path: %w", err)
	}
	if err := os.MkdirAll(filepath.Dir(absolute), 0o755); err != nil {
		return fmt.Errorf("create output directory: %w", err)
	}
	tmp, err := os.CreateTemp(filepath.Dir(absolute), ".science-out-*.tmp")
	if err != nil {
		return fmt.Errorf("create temporary file: %w", err)
	}
	tmpPath := tmp.Name()
	defer os.Remove(tmpPath)
	if _, err := tmp.Write(data); err != nil {
		tmp.Close()
		return err
	}
	if err := tmp.Sync(); err != nil {
		tmp.Close()
		return err
	}
	if err := tmp.Close(); err != nil {
		return err
	}
	if err := os.Chmod(tmpPath, 0o644); err != nil {
		return err
	}
	return os.Rename(tmpPath, absolute)
}
