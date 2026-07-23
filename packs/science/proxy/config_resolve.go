package proxy

import (
	"fmt"
	"strings"

	"lumen/internal/config"
)

// KeyFromLumenConfig returns an API key for the named provider from lumen.toml.
func KeyFromLumenConfig(cfg *config.File, providerName string) string {
	if cfg == nil {
		return ""
	}
	name := strings.ToLower(strings.TrimSpace(providerName))
	for _, p := range cfg.Providers {
		if strings.ToLower(p.Name) == name && strings.TrimSpace(p.APIKey) != "" {
			return strings.TrimSpace(p.APIKey)
		}
	}
	return ""
}

// BuildConfig assembles a proxy Config from CLI flags and optional lumen.toml.
func BuildConfig(providerName, apiKey, addr, authSecret, upstreamURL, logPath string, lumenCfg *config.File) (Config, error) {
	spec, ok := LookupProvider(providerName)
	if !ok {
		return Config{}, fmt.Errorf("unknown science provider %q (supported: deepseek, qwen, moonshot, zhipu)", providerName)
	}
	key := strings.TrimSpace(apiKey)
	if key == "" {
		key = KeyFromLumenConfig(lumenCfg, providerName)
	}
	if key == "" {
		key, _ = ResolveAPIKey(providerName, "")
	}
	if key == "" {
		return Config{}, fmt.Errorf("missing API key for %s: set %s, add [[providers]] in lumen.toml, or pass --api-key",
			providerName, spec.KeyEnv)
	}
	if addr == "" {
		addr = "127.0.0.1:18991"
	}
	return Config{
		Provider:    spec,
		APIKey:      key,
		Addr:        addr,
		AuthSecret:  authSecret,
		UpstreamURL: upstreamURL,
		LogPath:     logPath,
	}, nil
}
