package oauth

import (
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"time"
)

const virtualEmail = "virtual@localhost.invalid"

var keyNames = []string{
	"ANTHROPIC_API_KEY_ENCRYPTION_KEY",
	"OAUTH_ENCRYPTION_KEY",
	"JWT_SIGNING_SECRET",
	"USER_SECRET_ENCRYPTION_KEY",
}

// LoginAction describes what ensure did on this invocation.
type LoginAction string

const (
	ActionReused   LoginAction = "reused"
	ActionRepaired LoginAction = "repaired"
	ActionCreated  LoginAction = "created"
)

// ForgeResult summarizes a successful virtual login write.
type ForgeResult struct {
	AuthDir     string
	AccountUUID string
	OrgUUID     string
	EncFile     string
}

// EnsureVirtualLogin writes or reuses virtual OAuth credentials in the sandbox auth dir.
func EnsureVirtualLogin(authDir, sandboxRoot, realCredDir string) (ForgeResult, LoginAction, error) {
	resolved, err := resolveGuarded(authDir, virtualEmail, sandboxRoot, realCredDir)
	if err != nil {
		return ForgeResult{}, "", err
	}
	if fr, ok := readIntactLogin(resolved, virtualEmail); ok {
		return fr, ActionReused, nil
	}

	var priorOrg *string
	var action LoginAction
	if o, ok := readActiveOrg(resolved); ok {
		priorOrg = &o
		action = ActionRepaired
	} else if o, ok := readTokenOrg(resolved); ok {
		priorOrg = &o
		action = ActionRepaired
	} else {
		dirs := scanOrgDirs(resolved)
		switch len(dirs) {
		case 0:
			action = ActionCreated
		case 1:
			priorOrg = &dirs[0]
			action = ActionRepaired
		default:
			return ForgeResult{}, "", fmt.Errorf(
				"found %d historical orgs but cannot determine active org; write org_uuid to %s/active-org.json and retry",
				len(dirs), resolved)
		}
	}
	var priorAccount *string
	if a, ok := readPriorAccount(resolved); ok {
		priorAccount = &a
	}
	fr, err := writeLogin(resolved, virtualEmail, priorOrg, priorAccount)
	if err != nil {
		return ForgeResult{}, "", err
	}
	return fr, action, nil
}

func writeLogin(resolved, email string, preferOrg, preferAccount *string) (ForgeResult, error) {
	if err := os.MkdirAll(resolved, 0o700); err != nil {
		return ForgeResult{}, fmt.Errorf("create auth_dir: %w", err)
	}
	chmodBestEffort(resolved, 0o700)

	keyFile := filepath.Join(resolved, "encryption.key")
	if err := assertNotSymlink(keyFile); err != nil {
		return ForgeResult{}, err
	}
	keys := map[string]string{}
	if data, err := os.ReadFile(keyFile); err == nil {
		for _, line := range strings.Split(string(data), "\n") {
			if eq := strings.IndexByte(line, '='); eq > 0 {
				k := strings.TrimSpace(line[:eq])
				v := strings.TrimSpace(line[eq+1:])
				if v != "" {
					keys[k] = v
				}
			}
		}
	}
	if k, ok := keys["OAUTH_ENCRYPTION_KEY"]; ok {
		if raw, err := decodeB64(k); err != nil || len(raw) < 16 {
			delete(keys, "OAUTH_ENCRYPTION_KEY")
		}
	}
	for _, name := range keyNames {
		if _, ok := keys[name]; !ok {
			b, err := randBytes(32)
			if err != nil {
				return ForgeResult{}, err
			}
			keys[name] = base64Encode(b)
		}
	}
	var keyLines []string
	for _, name := range keyNames {
		keyLines = append(keyLines, fmt.Sprintf("%s=%s", name, keys[name]))
	}
	if err := safeWrite(keyFile, []byte(strings.Join(keyLines, "\n")+"\n"), 0o600); err != nil {
		return ForgeResult{}, err
	}

	accountUUID := uuidV4()
	if preferAccount != nil {
		accountUUID = *preferAccount
	}
	orgUUID := uuidV4()
	if preferOrg != nil {
		orgUUID = *preferOrg
	}
	access := "sk-ant-virtual-" + hexEncode(randBytesMust(24))
	blob := map[string]any{
		"access_token":            access,
		"refresh_token":           "",
		"api_key":                 nil,
		"token_expires_at":        "2099-01-01T00:00:00.000Z",
		"provider":                "claude_ai",
		"scopes":                  "user:inference user:file_upload user:profile user:mcp_servers user:plugins",
		"email":                   email,
		"account_uuid":            accountUUID,
		"subscription_type":       "max",
		"rate_limit_tier":         nil,
		"seat_tier":               nil,
		"org_uuid":                orgUUID,
		"billing_type":            nil,
		"has_extra_usage_enabled": false,
	}
	plaintext, err := json.Marshal(blob)
	if err != nil {
		return ForgeResult{}, err
	}
	oauthKey := keys["OAUTH_ENCRYPTION_KEY"]
	encBody, err := encryptTokenV2(plaintext, oauthKey)
	if err != nil {
		return ForgeResult{}, err
	}

	tokDir := filepath.Join(resolved, ".oauth-tokens")
	if err := assertNotSymlink(tokDir); err != nil {
		return ForgeResult{}, err
	}
	if err := os.MkdirAll(tokDir, 0o700); err != nil {
		return ForgeResult{}, err
	}
	chmodBestEffort(tokDir, 0o700)
	entries, _ := os.ReadDir(tokDir)
	for _, e := range entries {
		if strings.HasSuffix(e.Name(), ".enc") {
			p := filepath.Join(tokDir, e.Name())
			if err := assertNotSymlink(p); err != nil {
				return ForgeResult{}, err
			}
			if err := os.Remove(p); err != nil {
				return ForgeResult{}, fmt.Errorf("remove old token %s: %w", p, err)
			}
		}
	}
	userID := sanitizeID(accountUUID)
	encFile := filepath.Join(tokDir, userID+".enc")
	if err := safeWrite(encFile, []byte(encBody), 0o600); err != nil {
		return ForgeResult{}, err
	}

	roundtrip, err := decryptTokenV2(encBody, oauthKey)
	if err != nil {
		return ForgeResult{}, fmt.Errorf("self-check decrypt: %w", err)
	}
	var rt map[string]any
	if err := json.Unmarshal(roundtrip, &rt); err != nil {
		return ForgeResult{}, fmt.Errorf("self-check parse: %w", err)
	}
	if rt["email"] != email {
		return ForgeResult{}, fmt.Errorf("self-check email mismatch")
	}

	orgJSON, _ := json.MarshalIndent(map[string]string{"org_uuid": orgUUID}, "", "  ")
	if err := safeWrite(filepath.Join(resolved, "active-org.json"), append(orgJSON, '\n'), 0o600); err != nil {
		return ForgeResult{}, err
	}

	return ForgeResult{
		AuthDir:     resolved,
		AccountUUID: accountUUID,
		OrgUUID:     orgUUID,
		EncFile:     encFile,
	}, nil
}

func readIntactLogin(resolved, email string) (ForgeResult, bool) {
	for _, p := range []string{"encryption.key", ".oauth-tokens", "active-org.json"} {
		if isSymlink(filepath.Join(resolved, p)) {
			return ForgeResult{}, false
		}
	}
	key, ok := parseOAuthKey(resolved)
	if !ok {
		return ForgeResult{}, false
	}
	enc, ok := singleEnc(resolved)
	if !ok || isSymlink(enc) {
		return ForgeResult{}, false
	}
	activeOrg, ok := readActiveOrg(resolved)
	if !ok {
		return ForgeResult{}, false
	}
	body, err := os.ReadFile(enc)
	if err != nil {
		return ForgeResult{}, false
	}
	plain, err := decryptTokenV2(string(body), key)
	if err != nil {
		return ForgeResult{}, false
	}
	var blob map[string]any
	if err := json.Unmarshal(plain, &blob); err != nil {
		return ForgeResult{}, false
	}
	blobOrg, _ := blob["org_uuid"].(string)
	blobEmail, _ := blob["email"].(string)
	account, _ := blob["account_uuid"].(string)
	provider, _ := blob["provider"].(string)
	access, _ := blob["access_token"].(string)
	expires, _ := blob["token_expires_at"].(string)
	if blobOrg != activeOrg || blobEmail != email || !strings.HasSuffix(blobEmail, "localhost.invalid") ||
		!looksLikeUUID(account) || provider != "claude_ai" || access == "" || !tokenNotExpired(expires) {
		return ForgeResult{}, false
	}
	return ForgeResult{
		AuthDir:     resolved,
		AccountUUID: account,
		OrgUUID:     activeOrg,
		EncFile:     enc,
	}, true
}

// IsLoginIntact reports whether the sandbox auth dir has intact virtual login
// (no symlinks on critical files + parseable). If "daemon alive" but login broken → force repair.
func IsLoginIntact(authDir string) bool {
	sandboxRoot := filepath.Dir(authDir)
	realDir, err := guardRealScienceDir()
	if err != nil {
		return false
	}
	resolved, err := resolveGuarded(authDir, virtualEmail, sandboxRoot, realDir)
	if err != nil {
		return false
	}
	_, ok := readIntactLogin(resolved, virtualEmail)
	return ok
}

func guardRealScienceDir() (string, error) {
	home := strings.TrimSpace(os.Getenv("SCIENCE_REAL_HOME"))
	if home == "" {
		var err error
		home, err = os.UserHomeDir()
		if err != nil {
			return "", err
		}
	}
	return filepath.Join(home, ".claude-science"), nil
}

func parseOAuthKey(resolved string) (string, bool) {
	data, err := os.ReadFile(filepath.Join(resolved, "encryption.key"))
	if err != nil {
		return "", false
	}
	for _, line := range strings.Split(string(data), "\n") {
		if v, ok := strings.CutPrefix(line, "OAUTH_ENCRYPTION_KEY="); ok {
			v = strings.TrimSpace(v)
			if v != "" {
				return v, true
			}
		}
	}
	return "", false
}

func singleEnc(resolved string) (string, bool) {
	entries, err := os.ReadDir(filepath.Join(resolved, ".oauth-tokens"))
	if err != nil {
		return "", false
	}
	var found string
	for _, e := range entries {
		if strings.HasSuffix(e.Name(), ".enc") {
			if found != "" {
				return "", false
			}
			found = filepath.Join(resolved, ".oauth-tokens", e.Name())
		}
	}
	if found == "" {
		return "", false
	}
	return found, true
}

func readActiveOrg(resolved string) (string, bool) {
	data, err := os.ReadFile(filepath.Join(resolved, "active-org.json"))
	if err != nil {
		return "", false
	}
	var v map[string]any
	if err := json.Unmarshal(data, &v); err != nil {
		return "", false
	}
	o, _ := v["org_uuid"].(string)
	if looksLikeUUID(o) {
		return o, true
	}
	return "", false
}

func readPriorAccount(resolved string) (string, bool) {
	key, ok := parseOAuthKey(resolved)
	if !ok {
		return "", false
	}
	enc, ok := singleEnc(resolved)
	if !ok {
		return "", false
	}
	body, err := os.ReadFile(enc)
	if err != nil {
		return "", false
	}
	plain, err := decryptTokenV2(string(body), key)
	if err != nil {
		return "", false
	}
	var blob map[string]any
	if err := json.Unmarshal(plain, &blob); err != nil {
		return "", false
	}
	a, _ := blob["account_uuid"].(string)
	if looksLikeUUID(a) {
		return a, true
	}
	return "", false
}

func readTokenOrg(resolved string) (string, bool) {
	key, ok := parseOAuthKey(resolved)
	if !ok {
		return "", false
	}
	enc, ok := singleEnc(resolved)
	if !ok {
		return "", false
	}
	body, err := os.ReadFile(enc)
	if err != nil {
		return "", false
	}
	plain, err := decryptTokenV2(string(body), key)
	if err != nil {
		return "", false
	}
	var blob map[string]any
	if err := json.Unmarshal(plain, &blob); err != nil {
		return "", false
	}
	o, _ := blob["org_uuid"].(string)
	if looksLikeUUID(o) {
		return o, true
	}
	return "", false
}

func scanOrgDirs(resolved string) []string {
	entries, err := os.ReadDir(filepath.Join(resolved, "orgs"))
	if err != nil {
		return nil
	}
	var out []string
	for _, e := range entries {
		if e.IsDir() && looksLikeUUID(e.Name()) {
			out = append(out, e.Name())
		}
	}
	sort.Strings(out)
	return out
}

func tokenNotExpired(expiresAt string) bool {
	if len(expiresAt) < 10 {
		return false
	}
	date := expiresAt[:10]
	if date[4] != '-' || date[7] != '-' {
		return false
	}
	for _, i := range []int{0, 1, 2, 3, 5, 6, 8, 9} {
		if date[i] < '0' || date[i] > '9' {
			return false
		}
	}
	return date >= time.Now().UTC().Format("2006-01-02")
}

func looksLikeUUID(s string) bool {
	if len(s) != 36 {
		return false
	}
	for i, c := range s {
		switch i {
		case 8, 13, 18, 23:
			if c != '-' {
				return false
			}
		default:
			if !((c >= '0' && c <= '9') || (c >= 'a' && c <= 'f') || (c >= 'A' && c <= 'F')) {
				return false
			}
		}
	}
	return true
}

func uuidV4() string {
	b := randBytesMust(16)
	b[6] = (b[6] & 0x0f) | 0x40
	b[8] = (b[8] & 0x3f) | 0x80
	return fmt.Sprintf("%02x%02x%02x%02x-%02x%02x-%02x%02x-%02x%02x-%02x%02x%02x%02x%02x%02x",
		b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7], b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15])
}

func sanitizeID(s string) string {
	var b strings.Builder
	for _, c := range s {
		if (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') || c == '_' || c == '-' {
			b.WriteRune(c)
		}
	}
	return b.String()
}

func randBytes(n int) ([]byte, error) {
	b := make([]byte, n)
	_, err := io.ReadFull(rand.Reader, b)
	return b, err
}

func randBytesMust(n int) []byte {
	b, err := randBytes(n)
	if err != nil {
		panic(err)
	}
	return b
}

func hexEncode(b []byte) string {
	return hex.EncodeToString(b)
}

func base64Encode(b []byte) string {
	return base64Std(b)
}

func decodeB64(s string) ([]byte, error) {
	return base64Decode(strings.TrimSpace(s))
}
