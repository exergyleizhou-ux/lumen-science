package proxy

import (
	"fmt"
	"os"
	"path/filepath"
	"sync"
	"time"
)

type fileLogger struct {
	mu   sync.Mutex
	file *os.File
}

func newFileLogger(path string) (*fileLogger, error) {
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return nil, err
	}
	f, err := os.OpenFile(path, os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0o600)
	if err != nil {
		return nil, err
	}
	return &fileLogger{file: f}, nil
}

func (l *fileLogger) write(msg string) {
	if l == nil || l.file == nil {
		return
	}
	l.mu.Lock()
	defer l.mu.Unlock()
	ts := time.Now().Format("15:04:05")
	_, _ = fmt.Fprintf(l.file, "[%s] %s\n", ts, msg)
}
