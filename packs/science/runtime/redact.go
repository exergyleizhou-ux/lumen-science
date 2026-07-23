package runtime

import "strings"

// RedactSecret masks path-secret tokens in log tails before showing users.
func RedactSecret(text, secret string) string {
	if secret == "" {
		return text
	}
	return strings.ReplaceAll(text, secret, "****")
}
