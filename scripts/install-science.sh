#!/usr/bin/env bash
# Install, verify, upgrade, roll back or remove the Lumen Science CLI (LS5-F1-3).
#
# What this replaces and why
# --------------------------
# The previous installer compiled ./standalone/cmd/science out of the working
# tree and copied the result to ~/.local/bin, calling that an install. Three
# problems, each independently disqualifying:
#
#   1. It never touched a published release, so "install v1.0.1" installed
#      whatever the working tree happened to contain, including uncommitted
#      edits. Nothing a user installed was traceable to anything published.
#   2. It stamped the version from the root VERSION file (1.0.0) while the
#      product was 1.0.1, so `lumen-science version` reported a version that
#      was never released.
#   3. No hash verification, no rollback, no uninstall. A failed install had
#      already overwritten the working binary.
#
# This consumes the release instead: download, verify against SHA256SUMS and
# MANIFEST.json, smoke-test the extracted binary BEFORE it becomes current,
# then switch atomically while keeping the previous version for rollback.
#
# Layout:
#   <prefix>/versions/<version>/          unpacked releases
#   <prefix>/current  -> versions/<v>     symlink
#   <prefix>/previous -> versions/<v>     symlink
#   <bindir>/lumen-science -> <prefix>/current/lumen-science
#
# Usage:
#   install-science.sh install [<version>]   # default: latest release
#   install-science.sh verify                # re-verify what is installed
#   install-science.sh rollback              # switch back to previous
#   install-science.sh status
#   install-science.sh uninstall
#
# Env:
#   LUMEN_SCIENCE_PREFIX   install root (default ~/.local/share/lumen-science)
#   LUMEN_SCIENCE_BIN_DIR  symlink dir  (default ~/.local/bin)
#   LUMEN_SCIENCE_REPO     override the release repo

set -euo pipefail

REPO="${LUMEN_SCIENCE_REPO:-exergyleizhou-ux/lumen-science}"
PREFIX="${LUMEN_SCIENCE_PREFIX:-$HOME/.local/share/lumen-science}"
BIN_DIR="${LUMEN_SCIENCE_BIN_DIR:-$HOME/.local/bin}"
VERSIONS_DIR="$PREFIX/versions"
CURRENT_LINK="$PREFIX/current"
PREVIOUS_LINK="$PREFIX/previous"
BIN_LINK="$BIN_DIR/lumen-science"

die() { echo "FAIL: $*" >&2; exit 1; }
info() { echo "  $*"; }
need() { command -v "$1" >/dev/null 2>&1 || die "required tool not found: $1"; }

# ── platform ─────────────────────────────────────────────────────

detect_platform() {
  local os arch
  case "$(uname -s)" in
    Darwin) os=darwin ;;
    Linux)  os=linux ;;
    *) die "unsupported OS: $(uname -s). Supported: darwin, linux." ;;
  esac
  case "$(uname -m)" in
    arm64|aarch64) arch=arm64 ;;
    x86_64|amd64)  arch=amd64 ;;
    *) die "unsupported architecture: $(uname -m). Supported: arm64, amd64." ;;
  esac
  printf '%s-%s\n' "$os" "$arch"
}

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | awk '{print $1}'
  else shasum -a 256 "$1" | awk '{print $1}'
  fi
}

# ── install ──────────────────────────────────────────────────────

cmd_install() {
  need tar; need python3; need gh
  local requested="${1:-}"
  local platform tag version workdir
  platform="$(detect_platform)"

  if [[ -n "$requested" ]]; then
    tag="v${requested#v}"
  else
    tag="$(gh release list --repo "$REPO" --limit 20 --json tagName,isLatest \
            --jq 'map(select(.isLatest))[0].tagName' 2>/dev/null || true)"
    [[ -n "$tag" && "$tag" != "null" ]] || die "could not resolve the latest release; pass a version explicitly"
  fi
  version="${tag#v}"

  echo "Installing Lumen Science CLI ${tag} (${platform})"
  workdir="$(mktemp -d)"
  # shellcheck disable=SC2064
  trap "rm -rf '$workdir'" EXIT

  local archive="lumen-science-${version}-${platform}.tar.gz"
  info "downloading ${archive}"
  gh release download "$tag" --repo "$REPO" --dir "$workdir" \
    --pattern "$archive" --pattern 'SHA256SUMS' --pattern 'MANIFEST.json' \
    || die "download failed — does release ${tag} publish an asset for ${platform}?"

  [[ -f "$workdir/$archive" ]] || die "release ${tag} has no asset for ${platform}"
  [[ -f "$workdir/SHA256SUMS" ]] \
    || die "release ${tag} has no SHA256SUMS; refusing to install unverifiable bytes"

  # 1. archive bytes match the published checksum
  info "verifying archive digest"
  local want got
  want="$(awk -v f="$archive" '$2 == f || $2 == "*"f {print $1}' "$workdir/SHA256SUMS" | head -1)"
  [[ -n "$want" ]] || die "SHA256SUMS does not cover ${archive}"
  got="$(sha256_of "$workdir/$archive")"
  [[ "$want" == "$got" ]] || die "digest mismatch for ${archive}
    published:  ${want}
    downloaded: ${got}"
  info "digest ok (${got:0:16}…)"

  # 2. the manifest must describe this same tag and agree with SHA256SUMS. A
  #    manifest that disagrees means the release was assembled from mixed
  #    builds — which the hardened release workflow now refuses to produce.
  if [[ -f "$workdir/MANIFEST.json" ]]; then
    info "verifying manifest binding"
    python3 - "$workdir/MANIFEST.json" "$tag" "$archive" "$got" <<'PY' \
      || die "manifest verification failed"
import json, sys
path, tag, asset, digest = sys.argv[1:5]
m = json.load(open(path))
if m.get("tag") != tag:
    sys.exit(f"manifest tag {m.get('tag')} != {tag}")
if not m.get("git_commit"):
    sys.exit("manifest has no git_commit")
entry = next((a for a in m.get("assets", []) if a["name"] == asset), None)
if entry is None:
    sys.exit(f"manifest does not list {asset}")
if entry["sha256"] != digest:
    sys.exit(f"manifest digest {entry['sha256']} != downloaded {digest}")
print(f"  manifest ok: commit {m['git_commit'][:12]}, version {m.get('version')}")
PY
  else
    echo "  WARN: release ${tag} has no MANIFEST.json — tag/commit binding unverified" >&2
  fi

  # Release assets are not signed yet: no minisign/cosign, no per-asset SBOM,
  # no provenance attestation (docs/science/status/current.json ->
  # release.openGaps). Digest verification proves the bytes match what the
  # release publishes; it does NOT establish who published them. When signing
  # lands, verify it here, before extraction.

  # 3. extract into a versioned directory
  local target="$VERSIONS_DIR/$version"
  info "extracting to ${target}"
  rm -rf "$target"
  mkdir -p "$target"
  tar -xzf "$workdir/$archive" -C "$target"

  local binary
  binary="$(find "$target" -maxdepth 2 -type f \
              \( -name "lumen-science-${platform}" -o -name 'lumen-science' \) | head -1)"
  [[ -n "$binary" ]] || { rm -rf "$target"; die "archive contains no lumen-science binary"; }
  # Normalise the name so <prefix>/current/lumen-science is stable per platform.
  [[ "$(basename "$binary")" == "lumen-science" ]] || mv "$binary" "$target/lumen-science"
  chmod +x "$target/lumen-science"
  chmod +x "$target"/lumen-mcp-* 2>/dev/null || true

  # 4. smoke-test BEFORE it becomes current, so a broken or mislabelled build
  #    never replaces a working install.
  info "smoke-testing the extracted binary"
  local reported
  reported="$("$target/lumen-science" version 2>&1 | head -1)" \
    || { rm -rf "$target"; die "extracted binary failed to run: ${reported}"; }
  if [[ "$reported" != *"$version"* ]]; then
    rm -rf "$target"
    die "binary reports '${reported}' but this is release ${version}; refusing to install a mislabelled build"
  fi
  info "smoke ok: ${reported}"

  # 5. atomic switch, keeping the outgoing version for rollback
  if [[ -L "$CURRENT_LINK" ]]; then
    local outgoing
    outgoing="$(readlink "$CURRENT_LINK")"
    if [[ "$outgoing" != "$target" ]]; then
      ln -sfn "$outgoing" "$PREVIOUS_LINK"
      info "previous -> $(basename "$outgoing")"
    fi
  fi
  ln -sfn "$target" "$CURRENT_LINK"
  mkdir -p "$BIN_DIR"
  ln -sfn "$CURRENT_LINK/lumen-science" "$BIN_LINK"

  echo
  echo "Installed ${tag} -> ${BIN_LINK}"
  case ":$PATH:" in
    *":$BIN_DIR:"*) ;;
    *) echo "NOTE: ${BIN_DIR} is not on PATH. Add: export PATH=\"${BIN_DIR}:\$PATH\"" ;;
  esac
  echo "Verify anytime:  $0 verify"
  echo "Roll back:       $0 rollback"
}

# ── verify what is actually installed ────────────────────────────
#
# The CLI's own `doctor` inspects the SOURCE CHECKOUT (it stats
# packs/science/...), so it reports health on a machine with no install at all
# and cannot run for a user who only downloaded a release. This inspects the
# installed product.
cmd_verify() {
  [[ -L "$CURRENT_LINK" ]] || die "nothing installed at ${PREFIX}"
  local dir binary
  dir="$(readlink "$CURRENT_LINK")"
  binary="$dir/lumen-science"
  [[ -x "$binary" ]] || die "current install has no executable lumen-science"

  echo "Installed:  $(basename "$dir")"
  echo "Path:       ${binary}"
  echo "Digest:     $(sha256_of "$binary")"
  echo "Reports:    $("$binary" version 2>&1 | head -1)"

  if [[ -L "$BIN_LINK" ]]; then
    echo "Launcher:   ${BIN_LINK} -> $(readlink "$BIN_LINK")"
  else
    echo "Launcher:   MISSING (expected ${BIN_LINK})"
  fi

  # A different lumen-science earlier on PATH silently wins; say so rather than
  # reporting a healthy install the user is not actually running.
  local first
  first="$(command -v lumen-science 2>/dev/null || true)"
  if [[ -n "$first" && "$first" != "$BIN_LINK" ]]; then
    echo "WARNING:    a different lumen-science shadows this install: ${first}"
    return 1
  fi
  echo "OK"
}

cmd_rollback() {
  [[ -L "$PREVIOUS_LINK" ]] || die "no previous version to roll back to"
  local prev
  prev="$(readlink "$PREVIOUS_LINK")"
  [[ -x "$prev/lumen-science" ]] || die "previous version at ${prev} is not usable"
  "$prev/lumen-science" version >/dev/null 2>&1 \
    || die "previous version does not run; refusing to roll back onto it"

  local outgoing=""
  [[ -L "$CURRENT_LINK" ]] && outgoing="$(readlink "$CURRENT_LINK")"
  ln -sfn "$prev" "$CURRENT_LINK"
  [[ -n "$outgoing" ]] && ln -sfn "$outgoing" "$PREVIOUS_LINK"
  echo "Rolled back to $(basename "$prev")"
  [[ -n "$outgoing" ]] && echo "previous is now $(basename "$outgoing")"
  return 0
}

cmd_status() {
  if [[ -L "$CURRENT_LINK" ]]; then
    echo "current:  $(basename "$(readlink "$CURRENT_LINK")")"
  else
    echo "current:  (none)"
  fi
  if [[ -L "$PREVIOUS_LINK" ]]; then
    echo "previous: $(basename "$(readlink "$PREVIOUS_LINK")")"
  else
    echo "previous: (none)"
  fi
  if [[ -d "$VERSIONS_DIR" ]]; then
    echo "installed versions:"
    ls -1 "$VERSIONS_DIR" 2>/dev/null | sed 's/^/  /'
  fi
}

cmd_uninstall() {
  [[ -e "$CURRENT_LINK" || -d "$VERSIONS_DIR" ]] || die "nothing installed at ${PREFIX}"
  rm -f "$BIN_LINK" "$CURRENT_LINK" "$PREVIOUS_LINK"
  rm -rf "$VERSIONS_DIR"
  rmdir "$PREFIX" 2>/dev/null || true
  echo "Removed the Lumen Science CLI from ${PREFIX} and ${BIN_LINK}"
  echo "NOTE: project data and stores were NOT touched."
}

case "${1:-install}" in
  install)   shift || true; cmd_install "${1:-}" ;;
  verify)    cmd_verify ;;
  rollback)  cmd_rollback ;;
  status)    cmd_status ;;
  uninstall) cmd_uninstall ;;
  -h|--help|help) sed -n '27,39p' "$0" | sed 's/^# \{0,1\}//' ;;
  *) die "unknown command: $1 (try: install, verify, rollback, status, uninstall)" ;;
esac
