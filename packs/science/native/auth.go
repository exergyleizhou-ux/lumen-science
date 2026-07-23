// Package native hosts Lumen-owned Science MCP fleet and Oasis auth policy.
package native

// AuthLevel describes required credentials for an Oasis marketplace tool.
type AuthLevel int

const (
	// AuthAnonymous — public verified dataset metadata, no login required.
	AuthAnonymous AuthLevel = iota
	// AuthUserToken — user OAuth access token (preview, certificates, C2D jobs).
	AuthUserToken
)

// OasisToolAuth maps oasis MCP tool names to auth policy (combo 1+3).
//
// Anonymous (3): browse/search public verified catalog metadata.
// User token (1): sample preview, full certificates, C2D submission.
var OasisToolAuth = map[string]AuthLevel{
	"search_datasets":              AuthAnonymous,
	"get_dataset_detail":           AuthAnonymous,
	"list_verified_datasets":       AuthAnonymous,
	"list_offer_signals":           AuthAnonymous,
	"list_algorithms":              AuthUserToken,
	"preview_schema":               AuthUserToken,
	"get_verification_certificate": AuthUserToken,
	"submit_c2d_job":               AuthUserToken,
	"get_job_status":               AuthUserToken,
	"fetch_job_report":             AuthUserToken,
}

// RequiredAuth returns the auth level for an Oasis tool (defaults to user token).
func RequiredAuth(tool string) AuthLevel {
	if lvl, ok := OasisToolAuth[tool]; ok {
		return lvl
	}
	return AuthUserToken
}

// AuthError is returned when a tool needs a user token but none is configured.
type AuthError struct {
	Tool    string
	Message string
}

func (e *AuthError) Error() string {
	if e.Message != "" {
		return e.Message
	}
	return "oasis tool " + e.Tool + " requires user OAuth token — sign in to 绿洲 and link Science"
}

// CheckAuth returns an error if tool requires user token but none is configured.
func CheckAuth(tool string, token string) error {
	if RequiredAuth(tool) == AuthUserToken && token == "" {
		return &AuthError{Tool: tool}
	}
	return nil
}
