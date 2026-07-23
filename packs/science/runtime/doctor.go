package runtime

import (
	"fmt"
	"os"
	"strings"

	"lumen/internal/config"
	sciconfig "lumen/internal/science/config"
	"lumen/internal/science/guard"
	"lumen/internal/science/launcher"
	"lumen/internal/science/paths"
	"lumen/internal/science/proxy"
	"lumen/internal/science/research"
)

// DoctorResult is one check line.
type DoctorResult struct {
	Level   string `json:"level"` // pass, warn, fail
	Message string `json:"message"`
}

// RunDoctor performs read-only Science bridge diagnostics.
func RunDoctor(sciDir string, lumenCfg *config.File) ([]DoctorResult, int, int) {
	cfg, err := sciconfig.Load(sciDir)
	if err != nil {
		return []DoctorResult{{Level: "fail", Message: "config: " + err.Error()}}, 0, 1
	}
	var results []DoctorResult
	warns, fails := 0, 0
	add := func(level, msg string) {
		results = append(results, DoctorResult{Level: level, Message: msg})
		switch level {
		case "warn":
			warns++
		case "fail":
			fails++
		}
	}

	add("pass", fmt.Sprintf("science config loaded (provider=%s proxy=%d sandbox=%d)", cfg.Provider, cfg.ProxyPort, cfg.SandboxPort))
	if cfg.CacheBoostEnabled() {
		add("pass", "cache boost enabled (system/tools ephemeral for DeepSeek prefix cache)")
	}

	if _, ok := proxy.LookupProvider(cfg.Provider); !ok {
		add("fail", fmt.Sprintf("unknown provider %q", cfg.Provider))
	} else {
		add("pass", fmt.Sprintf("provider %q supported", cfg.Provider))
	}

	mgr, _ := New(sciDir, lumenCfg)
	if mgr != nil {
		if _, err := mgr.ResolveAPIKey(); err != nil {
			add("warn", err.Error())
		} else {
			add("pass", "upstream API key available (value not shown)")
		}
	}

	if guard.ScienceBinExists() {
		add("pass", "Claude Science binary found")
	} else {
		add("warn", "Claude Science binary not found at "+guard.ScienceBin())
	}

	for name, port := range map[string]int{"proxy": cfg.ProxyPort, "sandbox": cfg.SandboxPort} {
		if err := guard.AssertPortSafe(port); err != nil {
			add("fail", err.Error())
			continue
		}
		if guard.PortInUse(port) {
			add("warn", fmt.Sprintf("%s port %d in use", name, port))
		} else {
			add("pass", fmt.Sprintf("%s port %d free", name, port))
		}
	}

	realDir, err := guard.RealScienceDir()
	if err == nil {
		if _, err := os.Stat(realDir); err == nil {
			add("pass", "real ~/.claude-science exists (read-only check, never modified)")
		} else {
			add("warn", "real ~/.claude-science not found")
		}
		dataDir := strings.Replace(realDir, ".claude-science", ".lumen/science/sandbox/home/.claude-science", 1)
		_ = dataDir
	}

	secret := cfg.Secret
	dataDir := paths.DataDir(sciDir)
	if rep, err := research.Scan(dataDir); err != nil {
		add("warn", "research runtime not cloned — run lumen science start once (needs ~/.claude-science)")
	} else if rep.Healthy() {
		add("pass", fmt.Sprintf("research stack: %d DB clients, %d domain MCP, %d skills, %d tools across %d domains",
			rep.BioLibPackages, rep.DomainMCPServers, len(rep.Skills), rep.TotalDomainTools, len(rep.Domains)))
		add("pass", fmt.Sprintf("clone assets: %s", strings.Join(rep.CloneAssets, ", ")))
		if len(rep.SeedExamples) > 0 {
			add("pass", fmt.Sprintf("seed examples: %s", strings.Join(rep.SeedExamples, ", ")))
		}
		if rep.OrgPackSeeded {
			add("pass", fmt.Sprintf("org pack seeded (%d MCP workspaces)", rep.Workspaces))
		} else {
			add("warn", "org pack not seeded — lumen science research reseed")
		}
	} else {
		add("warn", fmt.Sprintf("research incomplete: lib=%d mcp=%d skills=%d tools=%d",
			rep.BioLibPackages, rep.DomainMCPServers, len(rep.Skills), rep.TotalDomainTools))
	}

	if proxyHealthy(cfg.ProxyPort, secret) {
		add("pass", "proxy responding on /health")
		if secret != "" {
			ok, hint := launcher.VerifyKeyViaProxy(cfg.ProxyPort, secret)
			if ok {
				add("pass", "proxy key verify: "+hint)
			} else {
				add("warn", "proxy key verify: "+hint)
			}
		}
	} else if guard.PortInUse(cfg.ProxyPort) {
		add("warn", "proxy port busy but /health failed")
	}

	return results, warns, fails
}
