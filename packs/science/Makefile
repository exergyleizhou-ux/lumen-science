# Lumen Science — build and release Makefile
# Usage: make all          — build all MCP binaries for current platform
#        make cross        — cross-compile for all platforms
#        make test         — run all tests
#        make lint         — run vet and static analysis
#        make release      — build release binaries with version stamp + SHA-256
#        make smoke        — local CLI smoke (seq + pipeline offline)
#        make clean        — remove build artifacts

.PHONY: all cross test lint release smoke clean cross-macos cross-linux cross-windows

# Prefer component VERSION (Science CLI/MCP); fall back to root then dev.
VERSION ?= $(shell cat VERSION 2>/dev/null || cat ../../VERSION 2>/dev/null || echo "0.1.0-dev")
BUILD_DIR := build
RELEASE_DIR := dist/science-release

GO := go
GOFLAGS := -trimpath -ldflags="-s -w -X main.version=$(VERSION)"

# MCP servers + primary productivity CLI (science)
MCP_BINS := artifacts notebook reviewer http_bridge skill-migrate
# Primary user-facing CLI name
CLI := science

# ── Default build ──────────────────────────────────────────────

all: $(addprefix $(BUILD_DIR)/lumen-mcp-,$(MCP_BINS)) $(BUILD_DIR)/lumen-science

$(BUILD_DIR)/lumen-mcp-%: standalone/cmd/%/main.go
	@mkdir -p $(BUILD_DIR)
	GOOS=$(shell go env GOOS) GOARCH=$(shell go env GOARCH) \
		$(GO) build $(GOFLAGS) -o $@ ./standalone/cmd/$*

$(BUILD_DIR)/lumen-science: standalone/cmd/science/main.go
	@mkdir -p $(BUILD_DIR)
	GOOS=$(shell go env GOOS) GOARCH=$(shell go env GOARCH) \
		$(GO) build $(GOFLAGS) -o $@ ./standalone/cmd/science

# ── Cross-compile ──────────────────────────────────────────────

cross: cross-macos cross-linux cross-windows

define CROSS_ONE
	@mkdir -p $(RELEASE_DIR)
	@for bin in $(MCP_BINS); do \
		ext=""; [ "$(1)" = "windows" ] && ext=".exe"; \
		GOOS=$(1) GOARCH=$(2) $(GO) build $(GOFLAGS) \
			-o $(RELEASE_DIR)/lumen-mcp-$$bin-$(1)-$(2)$$ext \
			./standalone/cmd/$$bin; \
	done
	@ext=""; [ "$(1)" = "windows" ] && ext=".exe"; \
	GOOS=$(1) GOARCH=$(2) $(GO) build $(GOFLAGS) \
		-o $(RELEASE_DIR)/lumen-science-$(1)-$(2)$$ext \
		./standalone/cmd/science
endef

cross-macos:
	$(call CROSS_ONE,darwin,arm64)
	$(call CROSS_ONE,darwin,amd64)
	@echo "macOS builds complete → $(RELEASE_DIR)"

cross-linux:
	$(call CROSS_ONE,linux,amd64)
	$(call CROSS_ONE,linux,arm64)
	@echo "Linux builds complete → $(RELEASE_DIR)"

cross-windows:
	$(call CROSS_ONE,windows,amd64)
	@echo "Windows builds complete → $(RELEASE_DIR)"

# ── Test ────────────────────────────────────────────────────────

test:
	$(GO) test ./mcp/artifacts/... ./mcp/notebook/... ./mcp/reviewer/... ./mcp/http_bridge/... \
		./standalone/internal/seqbench/... ./standalone/internal/pipeline/... \
		./standalone/internal/projectstore/... -count=1 -timeout=60s
	$(GO) build $(GOFLAGS) -o $(BUILD_DIR)/lumen-science ./standalone/cmd/science

test-verbose:
	$(GO) test ./... -v -count=1 -timeout=120s

test-e2e: $(addprefix $(BUILD_DIR)/lumen-mcp-,artifacts notebook reviewer)
	PATH="$(CURDIR)/$(BUILD_DIR):$$PATH" $(GO) test ./e2e/... -v -count=1 -timeout=120s

# Offline CLI smoke — real entry points
smoke: $(BUILD_DIR)/lumen-science
	@tmpdir=$$(mktemp -d); \
	printf '%s\n' '>smoke EcoRI_NotI' 'GAATTCATGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAAAAAGCGGCCGCTAA' > $$tmpdir/in.fa; \
	$(BUILD_DIR)/lumen-science seq analyze --md $$tmpdir/in.fa | tee $$tmpdir/seq.md >/dev/null; \
	grep -q EcoRI $$tmpdir/seq.md && grep -q NotI $$tmpdir/seq.md; \
	$(BUILD_DIR)/lumen-science pipeline offline --project smoke --run r1 --store $$tmpdir/store $$tmpdir/in.fa > $$tmpdir/pipe.json; \
	python3 -c "import json;d=json.load(open('$$tmpdir/pipe.json')); assert d['review']['status']=='pass'"; \
	echo "smoke: OK"; \
	rm -rf $$tmpdir

# ── Lint ────────────────────────────────────────────────────────

lint:
	$(GO) vet ./...
	@echo "vet: OK"

# ── Release ─────────────────────────────────────────────────────

# LS5-R1-01: reproducible archives. Plain `tar -czf` embeds per-file mtimes,
# uid/gid and a gzip header timestamp, so two builds of the same commit produced
# different bytes and the release could not be independently reproduced.
#
# SOURCE_DATE_EPOCH comes from the source commit (CI sets it; this default
# covers local builds). Every input mtime is normalised to it, entries are
# sorted, ownership is zeroed, and `gzip -n` drops the header timestamp.
SOURCE_DATE_EPOCH ?= $(shell git show -s --format=%ct HEAD 2>/dev/null || echo 0)

# Both tar flavours can produce deterministic archives, but not with the same
# flags: GNU tar has --sort/--mtime/--owner, macOS bsdtar has --uid/--gid/
# --uname/--gname and no --sort. Support both rather than hard-failing on the
# platform most of this is developed on. scripts/repro-archive.sh picks the
# right flags and feeds a sorted, mtime-normalised file list.
# ABSOLUTE. Every use is preceded by `cd $(RELEASE_DIR)`, so a path relative to
# this Makefile resolves against dist/science-release instead and lands on
# packs/science/scripts/, which does not exist. $(abspath) is expanded at parse
# time, while the working directory is still packs/science.
REPRO_ARCHIVE := $(abspath ../../scripts/repro-archive.sh)

release: clean cross
	@echo "Building release package version=$(VERSION) SOURCE_DATE_EPOCH=$(SOURCE_DATE_EPOCH)..."
	@mkdir -p $(RELEASE_DIR)
	@# Per-platform archives (primary CLI + MCP helpers)
	@cd $(RELEASE_DIR) && SOURCE_DATE_EPOCH=$(SOURCE_DATE_EPOCH) bash $(REPRO_ARCHIVE) \
		tar lumen-science-$(VERSION)-darwin-arm64.tar.gz \
		lumen-science-darwin-arm64 lumen-mcp-*-darwin-arm64
	@cd $(RELEASE_DIR) && SOURCE_DATE_EPOCH=$(SOURCE_DATE_EPOCH) bash $(REPRO_ARCHIVE) \
		tar lumen-science-$(VERSION)-darwin-amd64.tar.gz \
		lumen-science-darwin-amd64 lumen-mcp-*-darwin-amd64
	@cd $(RELEASE_DIR) && SOURCE_DATE_EPOCH=$(SOURCE_DATE_EPOCH) bash $(REPRO_ARCHIVE) \
		tar lumen-science-$(VERSION)-linux-amd64.tar.gz \
		lumen-science-linux-amd64 lumen-mcp-*-linux-amd64
	@cd $(RELEASE_DIR) && SOURCE_DATE_EPOCH=$(SOURCE_DATE_EPOCH) bash $(REPRO_ARCHIVE) \
		tar lumen-science-$(VERSION)-linux-arm64.tar.gz \
		lumen-science-linux-arm64 lumen-mcp-*-linux-arm64
	@cd $(RELEASE_DIR) && SOURCE_DATE_EPOCH=$(SOURCE_DATE_EPOCH) bash $(REPRO_ARCHIVE) \
		zip lumen-science-$(VERSION)-windows-amd64.zip \
		lumen-science-windows-amd64.exe lumen-mcp-*-windows-amd64.exe
	@# SHA-256 for every binary + archive
	@cd $(RELEASE_DIR) && shasum -a 256 lumen-science-* lumen-mcp-* > SHA256SUMS
	@echo "Release complete → $(RELEASE_DIR)"
	@ls -lh $(RELEASE_DIR)
	@echo "--- SHA256SUMS (first 15) ---"
	@head -15 $(RELEASE_DIR)/SHA256SUMS

# ── Clean ───────────────────────────────────────────────────────

clean:
	rm -rf $(BUILD_DIR) $(RELEASE_DIR)
	@echo "clean: done"
