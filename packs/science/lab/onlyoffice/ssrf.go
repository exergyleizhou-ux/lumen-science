package onlyoffice

import (
	"fmt"
	"net"
	"net/url"
	"os"
	"strings"
)

// ValidateDownloadURL rejects obvious SSRF targets when fetching the DS save URL.
// Allows:
//   - hosts matching LUMEN_ONLYOFFICE_URL
//   - localhost / 127.0.0.1 / host.docker.internal (local Docker Desktop flows)
func ValidateDownloadURL(raw string) error {
	u, err := url.Parse(strings.TrimSpace(raw))
	if err != nil || u.Scheme == "" || u.Host == "" {
		return fmt.Errorf("invalid download url")
	}
	scheme := strings.ToLower(u.Scheme)
	if scheme != "http" && scheme != "https" {
		return fmt.Errorf("unsupported scheme %q", scheme)
	}
	host := strings.ToLower(u.Hostname())
	if host == "" {
		return fmt.Errorf("empty host")
	}
	// Block cloud metadata / most *.internal hosts (allow host.docker.internal only).
	if host == "metadata.google.internal" || (strings.HasSuffix(host, ".internal") && host != "host.docker.internal") {
		return fmt.Errorf("host not allowed")
	}
	if ip := net.ParseIP(host); ip != nil {
		if ip.IsLinkLocalUnicast() || ip.IsLinkLocalMulticast() {
			return fmt.Errorf("link-local not allowed")
		}
	}

	allowed := map[string]bool{
		"localhost":             true,
		"127.0.0.1":             true,
		"::1":                   true,
		"host.docker.internal":  true,
	}
	if ds := strings.TrimSpace(os.Getenv("LUMEN_ONLYOFFICE_URL")); ds != "" {
		if du, err := url.Parse(ds); err == nil && du.Hostname() != "" {
			allowed[strings.ToLower(du.Hostname())] = true
		}
	}
	// Also allow extra hosts via env (comma-separated)
	for _, h := range strings.Split(os.Getenv("LUMEN_ONLYOFFICE_DOWNLOAD_HOSTS"), ",") {
		h = strings.ToLower(strings.TrimSpace(h))
		if h != "" {
			allowed[h] = true
		}
	}
	if !allowed[host] {
		return fmt.Errorf("download host %q not in allowlist", host)
	}
	return nil
}
