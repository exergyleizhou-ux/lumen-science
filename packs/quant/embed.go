package quant

import "embed"

// Embed only the engine modules — not the test_*.py files — so the sandbox image
// carries exactly the runtime that produces a certificate.
//
//go:embed harness/engine.py harness/data.py harness/rules.py harness/metrics.py harness/dataset.py harness/run.py
var harnessFS embed.FS

// fetchScript is the host-side data fetcher. It is embedded separately from the
// engine because it uses akshare/pandas and must NOT be staged into the
// network-isolated sandbox image (which carries only the pure-stdlib engine).
//
//go:embed harness/fetch.py
var fetchScript []byte

// HarnessFiles maps base filename -> file contents for the embedded engine.
var HarnessFiles = loadHarness()

func loadHarness() map[string][]byte {
	names := []string{"engine.py", "data.py", "rules.py", "metrics.py", "dataset.py", "run.py"}
	out := make(map[string][]byte, len(names))
	for _, n := range names {
		b, err := harnessFS.ReadFile("harness/" + n)
		if err != nil {
			panic("quant: embedded harness missing " + n + ": " + err.Error())
		}
		out[n] = b
	}
	return out
}
