package compute

import (
	"bufio"
	"os"
	"path/filepath"
	"strings"
)

// SSHHost represents a Host entry from ~/.ssh/config.
type SSHHost struct {
	Alias      string `json:"alias"`
	Hostname   string `json:"hostname,omitempty"`
	User       string `json:"user,omitempty"`
	Port       string `json:"port,omitempty"`
	IdentityFile string `json:"identity_file,omitempty"`
}

// ParseSSHConfig reads ~/.ssh/config and returns known Host aliases.
func ParseSSHConfig() ([]SSHHost, error) {
	home, err := os.UserHomeDir()
	if err != nil {
		return nil, err
	}
	f, err := os.Open(filepath.Join(home, ".ssh", "config"))
	if err != nil {
		return nil, err
	}
	defer f.Close()

	var hosts []SSHHost
	var cur *SSHHost
	scanner := bufio.NewScanner(f)
	for scanner.Scan() {
		line := strings.TrimSpace(scanner.Text())
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}
		fields := strings.Fields(line)
		if len(fields) < 2 {
			continue
		}
		key := strings.ToLower(fields[0])
		val := strings.Join(fields[1:], " ")

		switch key {
		case "host":
			if cur != nil && cur.Alias != "" {
				hosts = append(hosts, *cur)
			}
			cur = &SSHHost{Alias: val}
		default:
			if cur == nil {
				continue
			}
			switch key {
			case "hostname":
				cur.Hostname = val
			case "user":
				cur.User = val
			case "port":
				cur.Port = val
			case "identityfile":
				cur.IdentityFile = val
			}
		}
	}
	if cur != nil && cur.Alias != "" {
		hosts = append(hosts, *cur)
	}
	return hosts, scanner.Err()
}
