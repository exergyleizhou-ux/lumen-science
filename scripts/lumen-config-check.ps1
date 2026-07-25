<#
.SYNOPSIS
    Validate Lumen config consistency (Windows PowerShell edition).

.DESCRIPTION
    Checks for conflicts between ~/.lumen/config.toml (Lumen product config)
    and ~/.grok/config.toml (grok-build upstream config).

    Lumen uses TWO config files:
      ~/.lumen/config.toml  — Lumen product config (FINAL-5UX). Primary home.
      ~/.grok/config.toml   — grok-build upstream config. Override layer.

    Rules:
      1. ~/.lumen/config.toml is the authoritative source for model defaults.
      2. ~/.grok/config.toml provides UI/runtime overrides (permissions, auto_update).
      3. If both define [models].default, ~/.lumen wins.
      4. Model definitions with the same key in both → ~/.lumen wins.

.EXAMPLE
    .\scripts\lumen-config-check.ps1
#>

param()

$ErrorActionPreference = "Stop"

$Red    = "$([char]27)[0;31m"
$Yellow = "$([char]27)[1;33m"
$Green  = "$([char]27)[0;32m"
$Reset  = "$([char]27)[0m"

$LumenDir   = if ($env:LUMEN_DIR) { $env:LUMEN_DIR } else { "$env:USERPROFILE\.lumen" }
$GrokDir    = if ($env:GROK_DIR)  { $env:GROK_DIR  } else { "$env:USERPROFILE\.grok"  }
$LumenConf  = Join-Path $LumenDir "config.toml"
$GrokConf   = Join-Path $GrokDir  "config.toml"

Write-Host "=== Lumen Config Check ==="
Write-Host ""

$issues = 0

# Check files exist
foreach ($f in @($LumenConf, $GrokConf)) {
    if (-not (Test-Path $f)) {
        Write-Host "${Red}×${Reset} Missing: $f"
        $issues++
    }
}

if ($issues -gt 0) {
    Write-Host ""
    Write-Host "Fix: create the missing config file(s)"
    exit 1
}

Write-Host "${Green}✓${Reset} Both config files exist"
Write-Host ""

# Check for conflicting model defaults
Write-Host "--- Model defaults ---"
$lumenDefault = ""
$grokDefault  = ""

$lumenMatch = Select-String -Path $LumenConf -Pattern '^default\s*=' -SimpleMatch | Select-Object -First 1
if ($lumenMatch) {
    if ($lumenMatch.Line -match 'default\s*=\s*"([^"]*)"') {
        $lumenDefault = $Matches[1]
    }
}

$grokMatch = Select-String -Path $GrokConf -Pattern '^default\s*=' -SimpleMatch | Select-Object -First 1
if ($grokMatch) {
    if ($grokMatch.Line -match 'default\s*=\s*"([^"]*)"') {
        $grokDefault = $Matches[1]
    }
}

if ($lumenDefault -and $grokDefault) {
    if ($lumenDefault -ne $grokDefault) {
        Write-Host "${Yellow}⚠${Reset}  Default model mismatch:"
        Write-Host "   ~/.lumen:  $lumenDefault (authoritative)"
        Write-Host "   ~/.grok:   $grokDefault (overridden)"
        Write-Host "   → Using $lumenDefault from ~/.lumen/config.toml"
        $issues++
    } else {
        Write-Host "${Green}✓${Reset}  Default model: $lumenDefault (consistent)"
    }
} else {
    Write-Host "${Yellow}⚠${Reset}  Could not parse default model from one or both configs"
}

# Check for duplicate model definitions
Write-Host ""
Write-Host "--- Model definitions ---"

$lumenModels = Select-String -Path $LumenConf -Pattern '^\[model\.' | ForEach-Object {
    if ($_.Line -match '\[model\.(.*)\]') { $Matches[1] }
}
$grokModels  = Select-String -Path $GrokConf  -Pattern '^\[model\.' | ForEach-Object {
    if ($_.Line -match '\[model\.(.*)\]') { $Matches[1] }
}

$duplicates = Compare-Object $lumenModels $grokModels -IncludeEqual -ExcludeDifferent | Select-Object -ExpandProperty InputObject
if ($duplicates) {
    Write-Host "${Yellow}⚠${Reset}  Models defined in both configs (Lumen wins):"
    foreach ($model in $duplicates) {
        Write-Host "   - $model"
    }
} else {
    Write-Host "${Green}✓${Reset}  No duplicate model definitions"
}

# Check auto_update consistency
Write-Host ""
Write-Host "--- Update settings ---"
$grokUpdate = Select-String -Path $GrokConf -Pattern "auto_update" -SimpleMatch
if ($grokUpdate) {
    Write-Host "  ~/.grok: $($grokUpdate.Line.Trim()) (should be false for Lumen fork)"
} else {
    Write-Host "  ~/.grok: not set"
}

Write-Host ""
if ($issues -eq 0) {
    Write-Host "${Green}✓ Config check passed — no issues found${Reset}"
} else {
    Write-Host "${Yellow}⚠ Found $issues potential issue(s)${Reset}"
}

exit 0
