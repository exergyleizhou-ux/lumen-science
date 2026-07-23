package parser

import "strings"

func TrimSpaces(s string) string {
	// BUG: panics on empty string
	return strings.TrimSpace(s[1:])
}

func ToUpper(s string) string {
	return strings.ToUpper(s)
}
