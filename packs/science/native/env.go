package native

import sciconfig "lumen/internal/science/config"

// OasisEnv returns OASIS_* environment variables for MCP subprocesses.
func OasisEnv(cfg sciconfig.File) []string {
	out := []string{
		"OASIS_BASE_URL=" + cfg.OasisBaseURL(),
	}
	if tok := cfg.OasisToken(); tok != "" {
		out = append(out, "OASIS_API_TOKEN="+tok)
	}
	return out
}

// FleetEnv returns extra env vars for a fleet member (oasis gets token injection).
func FleetEnv(memberID string, cfg sciconfig.File) []string {
	if memberID == "oasis" || memberID == "c2d" {
		return OasisEnv(cfg)
	}
	return nil
}

// OasisEnvFromDir loads science config and returns oasis MCP env.
func OasisEnvFromDir(sciDir string) ([]string, error) {
	cfg, err := sciconfig.Load(sciDir)
	if err != nil {
		return nil, err
	}
	return OasisEnv(cfg), nil
}
