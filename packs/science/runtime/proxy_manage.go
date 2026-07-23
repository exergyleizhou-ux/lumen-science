package runtime

import (
	"fmt"
	"hash/fnv"
	"os/exec"
	"strconv"
	"strings"
)

// ProxyAction describes whether the proxy was reused or restarted.
type ProxyAction string

const (
	ProxyReused    ProxyAction = "reused"
	ProxyRestarted ProxyAction = "restarted"
)

// KeyFingerprint returns a non-cryptographic fingerprint for API key change detection.
func KeyFingerprint(key string) uint64 {
	h := fnv.New64a()
	_, _ = h.Write([]byte(key))
	return h.Sum64()
}

// KillListenerOnPort best-effort kills processes listening on a loopback port.
func KillListenerOnPort(port int) error {
	out, err := exec.Command("lsof", "-ti", fmt.Sprintf("tcp:%d", port)).Output()
	if err != nil {
		return nil // port free
	}
	for _, line := range strings.Split(strings.TrimSpace(string(out)), "\n") {
		pid, err := strconv.Atoi(strings.TrimSpace(line))
		if err != nil || pid <= 0 {
			continue
		}
		_ = exec.Command("kill", strconv.Itoa(pid)).Run()
	}
	return nil
}
