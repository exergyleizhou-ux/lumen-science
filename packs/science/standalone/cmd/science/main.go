package main

import (
	"context"
	"flag"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/lumen-ai/lumen-science/internal/brief"
)

const usage = `lumen-science — standalone private-beta Science path

Usage:
  lumen-science doctor [--root PATH]
  lumen-science brief [--out PATH] [--timeout 30s] TOPIC

The brief command fetches source metadata from PubMed and ChEMBL. It does not
invent conclusions and is not medical advice.
`

func main() {
	if len(os.Args) < 2 {
		fmt.Fprint(os.Stderr, usage)
		os.Exit(2)
	}
	var err error
	switch os.Args[1] {
	case "doctor":
		err = runDoctor(os.Args[2:])
	case "brief":
		err = runBrief(os.Args[2:])
	case "help", "-h", "--help":
		fmt.Print(usage)
		return
	default:
		err = fmt.Errorf("unknown command %q", os.Args[1])
	}
	if err != nil {
		fmt.Fprintln(os.Stderr, "lumen-science:", err)
		os.Exit(1)
	}
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
		"native/brief",
		"lab",
		"standalone/go.mod",
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
	goFiles := 0
	err = filepath.WalkDir(packRoot, func(path string, entry os.DirEntry, walkErr error) error {
		if walkErr != nil {
			return walkErr
		}
		if !entry.IsDir() && strings.HasSuffix(entry.Name(), ".go") {
			goFiles++
		}
		return nil
	})
	if err != nil {
		return fmt.Errorf("scan Science pack: %w", err)
	}
	if goFiles < 10 {
		fmt.Printf("FAIL  Go source inventory: %d files (want at least 10)\n", goFiles)
		failures++
	} else {
		fmt.Printf("PASS  Go source inventory: %d files\n", goFiles)
	}
	if os.Getenv("NCBI_API_KEY") == "" {
		fmt.Println("WARN  NCBI_API_KEY is unset; public rate limits apply")
	} else {
		fmt.Println("PASS  NCBI_API_KEY is available (value not shown)")
	}
	if failures > 0 {
		return fmt.Errorf("doctor found %d blocking issue(s)", failures)
	}
	fmt.Println("OK    standalone Science private-beta path is structurally ready")
	return nil
}

func runBrief(args []string) error {
	flags := flag.NewFlagSet("brief", flag.ContinueOnError)
	flags.SetOutput(os.Stderr)
	outPath := flags.String("out", "", "write Markdown atomically to this path; default: stdout")
	timeout := flags.Duration("timeout", 30*time.Second, "overall and per-request timeout")
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

func findPackRoot(explicit string) (string, error) {
	if explicit != "" {
		return filepath.Abs(explicit)
	}
	candidates := []string{"."}
	if executable, err := os.Executable(); err == nil {
		candidates = append(candidates, filepath.Dir(executable), filepath.Dir(filepath.Dir(executable)))
	}
	for _, candidate := range candidates {
		absolute, err := filepath.Abs(candidate)
		if err != nil {
			continue
		}
		if _, err := os.Stat(filepath.Join(absolute, "standalone", "go.mod")); err == nil {
			return absolute, nil
		}
	}
	return "", fmt.Errorf("cannot locate packs/science; run from that directory or pass --root")
}

func writeAtomic(path string, data []byte) error {
	absolute, err := filepath.Abs(path)
	if err != nil {
		return fmt.Errorf("resolve output path: %w", err)
	}
	if err := os.MkdirAll(filepath.Dir(absolute), 0o755); err != nil {
		return fmt.Errorf("create output directory: %w", err)
	}
	tmp, err := os.CreateTemp(filepath.Dir(absolute), ".science-brief-*.tmp")
	if err != nil {
		return fmt.Errorf("create temporary brief: %w", err)
	}
	tmpPath := tmp.Name()
	defer os.Remove(tmpPath)
	if _, err := tmp.Write(data); err != nil {
		tmp.Close()
		return fmt.Errorf("write temporary brief: %w", err)
	}
	if err := tmp.Sync(); err != nil {
		tmp.Close()
		return fmt.Errorf("sync temporary brief: %w", err)
	}
	if err := tmp.Close(); err != nil {
		return fmt.Errorf("close temporary brief: %w", err)
	}
	if err := os.Chmod(tmpPath, 0o644); err != nil {
		return fmt.Errorf("chmod temporary brief: %w", err)
	}
	if err := os.Rename(tmpPath, absolute); err != nil {
		return fmt.Errorf("publish brief: %w", err)
	}
	return nil
}
