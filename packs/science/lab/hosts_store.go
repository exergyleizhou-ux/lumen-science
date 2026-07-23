package lab

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"
)

// RegisteredHost is a Lab-managed SSH/local host (beyond ~/.ssh/config).
type RegisteredHost struct {
	Alias    string    `json:"alias"`
	Hostname string    `json:"hostname,omitempty"`
	User     string    `json:"user,omitempty"`
	Port     string    `json:"port,omitempty"`
	Notes    string    `json:"notes,omitempty"`
	Created  time.Time `json:"created_at,omitempty"`
}

type hostsFile struct {
	Hosts []RegisteredHost `json:"hosts"`
}

var hostsMu sync.Mutex

func hostsPath(sciDir string) string {
	return filepath.Join(sciDir, "lab", "hosts.json")
}

// LoadRegisteredHosts reads sciDir/lab/hosts.json.
func LoadRegisteredHosts(sciDir string) ([]RegisteredHost, error) {
	hostsMu.Lock()
	defer hostsMu.Unlock()
	data, err := os.ReadFile(hostsPath(sciDir))
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, err
	}
	var f hostsFile
	if err := json.Unmarshal(data, &f); err != nil {
		return nil, err
	}
	return f.Hosts, nil
}

// SaveRegisteredHosts replaces the registry.
func SaveRegisteredHosts(sciDir string, hosts []RegisteredHost) error {
	hostsMu.Lock()
	defer hostsMu.Unlock()
	path := hostsPath(sciDir)
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return err
	}
	// normalize aliases
	out := make([]RegisteredHost, 0, len(hosts))
	seen := map[string]bool{}
	for _, h := range hosts {
		a := strings.TrimSpace(h.Alias)
		if a == "" || a == "local" {
			continue
		}
		if seen[a] {
			continue
		}
		seen[a] = true
		if h.Created.IsZero() {
			h.Created = time.Now().UTC()
		}
		h.Alias = a
		out = append(out, h)
	}
	data, err := json.MarshalIndent(hostsFile{Hosts: out}, "", "  ")
	if err != nil {
		return err
	}
	return os.WriteFile(path, append(data, '\n'), 0o600)
}

// UpsertRegisteredHost adds or updates one host.
func UpsertRegisteredHost(sciDir string, h RegisteredHost) ([]RegisteredHost, error) {
	list, err := LoadRegisteredHosts(sciDir)
	if err != nil {
		return nil, err
	}
	a := strings.TrimSpace(h.Alias)
	if a == "" {
		return nil, os.ErrInvalid
	}
	found := false
	for i := range list {
		if list[i].Alias == a {
			h.Alias = a
			if h.Created.IsZero() {
				h.Created = list[i].Created
			}
			list[i] = h
			found = true
			break
		}
	}
	if !found {
		if h.Created.IsZero() {
			h.Created = time.Now().UTC()
		}
		h.Alias = a
		list = append(list, h)
	}
	if err := SaveRegisteredHosts(sciDir, list); err != nil {
		return nil, err
	}
	return list, nil
}

// DeleteRegisteredHost removes by alias.
func DeleteRegisteredHost(sciDir, alias string) ([]RegisteredHost, error) {
	list, err := LoadRegisteredHosts(sciDir)
	if err != nil {
		return nil, err
	}
	out := list[:0]
	for _, h := range list {
		if h.Alias != alias {
			out = append(out, h)
		}
	}
	if err := SaveRegisteredHosts(sciDir, out); err != nil {
		return nil, err
	}
	return out, nil
}
