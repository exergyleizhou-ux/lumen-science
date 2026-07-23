package native

import (
	"testing"

	sciconfig "lumen/internal/science/config"
)

func TestOasisEnvInjection(t *testing.T) {
	cfg := sciconfig.Default()
	cfg.Oasis.BaseURL = "https://demo.example.com"
	cfg.Oasis.APIToken = "secret-token"
	env := OasisEnv(cfg)
	if len(env) != 2 {
		t.Fatalf("env: %v", env)
	}
	if env[0] != "OASIS_BASE_URL=https://demo.example.com" {
		t.Fatalf("base: %s", env[0])
	}
	if env[1] != "OASIS_API_TOKEN=secret-token" {
		t.Fatalf("token: %s", env[1])
	}
}
