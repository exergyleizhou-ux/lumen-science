# Lumen Science — build and release Makefile
# Usage: make all          — build all MCP binaries for current platform
#        make cross        — cross-compile for all platforms
#        make test         — run all tests
#        make lint         — run vet and static analysis
#        make release      — build release binaries with version stamp
#        make clean        — remove build artifacts

.PHONY: all cross test lint release clean

VERSION ?= $(shell cat ../../VERSION 2>/dev/null || echo "0.1.0-dev")
BUILD_DIR := build
RELEASE_DIR := dist/science-release

GO := go
GOFLAGS := -trimpath -ldflags="-s -w -X main.version=$(VERSION)"

BINARIES := artifacts notebook reviewer http_bridge science skill-migrate

# ── Default build ──────────────────────────────────────────────

all: $(addprefix $(BUILD_DIR)/lumen-mcp-,$(BINARIES))

$(BUILD_DIR)/lumen-mcp-%: standalone/cmd/%/main.go
	@mkdir -p $(BUILD_DIR)
	GOOS=$(shell go env GOOS) GOARCH=$(shell go env GOARCH) \
		$(GO) build $(GOFLAGS) -o $@ ./standalone/cmd/$*

# ── Cross-compile ──────────────────────────────────────────────

cross: cross-macos cross-linux cross-windows

cross-macos:
	@mkdir -p $(RELEASE_DIR)
	@for bin in $(BINARIES); do \
		GOOS=darwin GOARCH=arm64 $(GO) build $(GOFLAGS) \
			-o $(RELEASE_DIR)/lumen-mcp-$$bin-darwin-arm64 \
			./standalone/cmd/$$bin; \
		GOOS=darwin GOARCH=amd64 $(GO) build $(GOFLAGS) \
			-o $(RELEASE_DIR)/lumen-mcp-$$bin-darwin-amd64 \
			./standalone/cmd/$$bin; \
	done
	@echo "macOS builds complete → $(RELEASE_DIR)"

cross-linux:
	@mkdir -p $(RELEASE_DIR)
	@for bin in $(BINARIES); do \
		GOOS=linux GOARCH=amd64 $(GO) build $(GOFLAGS) \
			-o $(RELEASE_DIR)/lumen-mcp-$$bin-linux-amd64 \
			./standalone/cmd/$$bin; \
		GOOS=linux GOARCH=arm64 $(GO) build $(GOFLAGS) \
			-o $(RELEASE_DIR)/lumen-mcp-$$bin-linux-arm64 \
			./standalone/cmd/$$bin; \
	done
	@echo "Linux builds complete → $(RELEASE_DIR)"

cross-windows:
	@mkdir -p $(RELEASE_DIR)
	@for bin in $(BINARIES); do \
		GOOS=windows GOARCH=amd64 $(GO) build $(GOFLAGS) \
			-o $(RELEASE_DIR)/lumen-mcp-$$bin-windows-amd64.exe \
			./standalone/cmd/$$bin; \
	done
	@echo "Windows builds complete → $(RELEASE_DIR)"

# ── Test ────────────────────────────────────────────────────────

test:
	$(GO) test ./mcp/artifacts/... ./mcp/notebook/... ./mcp/reviewer/... ./mcp/http_bridge/... -count=1 -timeout=60s

test-verbose:
	$(GO) test ./... -v -count=1 -timeout=120s

test-e2e: $(addprefix $(BUILD_DIR)/lumen-mcp-,artifacts notebook reviewer)
	PATH="$(CURDIR)/$(BUILD_DIR):$$PATH" $(GO) test ./e2e/... -v -count=1 -timeout=120s

# ── Lint ────────────────────────────────────────────────────────

lint:
	$(GO) vet ./...
	@echo "vet: OK"

# ── Release ─────────────────────────────────────────────────────

release: cross
	@echo "Building release package..."
	@mkdir -p $(RELEASE_DIR)
	# macOS universal
	@cd $(RELEASE_DIR) && tar -czf lumen-science-macos-$(VERSION).tar.gz lumen-mcp-*-darwin-*
	# Linux
	@cd $(RELEASE_DIR) && tar -czf lumen-science-linux-$(VERSION).tar.gz lumen-mcp-*-linux-*
	# Windows
	@cd $(RELEASE_DIR) && zip -q lumen-science-windows-$(VERSION).zip lumen-mcp-*-windows-*
	# SHA-256 checksums
	@cd $(RELEASE_DIR) && shasum -a 256 lumen-mcp-* *.tar.gz *.zip > lumen-science-$(VERSION)-checksums.txt
	@echo "Release complete → $(RELEASE_DIR)"
	@echo "Files:"
	@ls -lh $(RELEASE_DIR)

# ── Clean ───────────────────────────────────────────────────────

clean:
	rm -rf $(BUILD_DIR) $(RELEASE_DIR)
	@echo "clean: done"
