package dbutil

import (
	"database/sql"
	"errors"
)

// OpenDB opens a database handle for dsn.
// On failure the error must wrap the underlying driver error with
// fmt.Errorf("dbutil: %w", err) so callers can use errors.Is / Unwrap.
func OpenDB(dsn string) (*sql.DB, error) {
	// Force an immediate open error with an unregistered driver name.
	db, err := sql.Open("not-a-registered-driver", dsn)
	if err != nil {
		// BUG: bare string — loses the original error (cannot unwrap).
		return nil, errors.New("database error")
	}
	return db, nil
}
