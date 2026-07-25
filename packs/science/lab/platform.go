// Package lab — platform-aware utilities for cross-platform Science support.
// Thin wrappers over the internal platform package.

package lab

import "lumen/internal/science/internal/platform"

// OpenBrowser opens a URL in the system default browser.
var OpenBrowser = platform.OpenBrowser

// OpenDir opens a directory in the system file manager.
var OpenDir = platform.OpenDir

// CopyDir recursively copies a directory.
var CopyDir = platform.CopyDir

// EnsureKeychain sets up a sandbox keychain on macOS. No-op on other OS.
var EnsureKeychain = platform.EnsureKeychain
