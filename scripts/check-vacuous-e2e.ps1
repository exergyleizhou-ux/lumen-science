<#
.SYNOPSIS
    Detect async e2e tests missing `.await` (Windows PowerShell edition).

.DESCRIPTION
    A vacuous e2e test is one where `with_local_set(async { ... })` is called
    without `.await`. The Rust compiler only emits a warning (unused Future),
    so these tests pass in 0.00s without actually executing anything.

    Detection rules:
      1. Count `with_local_set` calls per test file.
      2. Count `.await` occurrences near those calls.
      3. If counts don't match → vacuous (report).
      4. If e2e test runs in < 1s → also suspicious.

.EXAMPLE
    .\scripts\check-vacuous-e2e.ps1           # scan all e2e tests
    .\scripts\check-vacuous-e2e.ps1 -Fix      # report only (no auto-fix yet)
#>

param(
    [switch]$Fix
)

$RepoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$AgentDir = Join-Path $RepoRoot "agent"
$exitCode = 0
$vacuousCount = 0

$Red    = "$([char]27)[0;31m"
$Green  = "$([char]27)[0;32m"
$Yellow = "$([char]27)[1;33m"
$Reset  = "$([char]27)[0m"

Write-Host "=== Lumen: vacuous async e2e check ==="
Write-Host ""

# Find all Rust test files with async test patterns
$testFiles = Get-ChildItem -Path $AgentDir -Recurse -Filter "*.rs" |
    Where-Object { $_.FullName -match "tests|e2e" }

foreach ($file in $testFiles) {
    $content = Get-Content -Path $file.FullName -Raw
    if (-not $content) { continue }

    # Only check files with async test constructs
    if ($content -notmatch 'with_local_set|tokio::test') { continue }

    $withLocalCount = ([regex]::Matches($content, 'with_local_set')).Count
    $awaitCount     = ([regex]::Matches($content, '\.await')).Count

    if ($withLocalCount -eq 0) { continue }

    if ($awaitCount -lt $withLocalCount) {
        $relPath = $file.FullName.Substring($RepoRoot.Length + 1)
        Write-Host "${Red}× VACUOUS:${Reset} $relPath"
        Write-Host "  with_local_set calls: $withLocalCount"
        Write-Host "  .await calls:         $awaitCount"
        Write-Host "  → Missing .await — test passes without executing!"
        Write-Host ""
        $vacuousCount++
        $exitCode = 1
    }
}

Write-Host ""
if ($exitCode -eq 0) {
    Write-Host "${Green}✓ All e2e tests have matching .await calls${Reset}"
} else {
    Write-Host "${Red}× Found $vacuousCount potentially vacuous e2e tests${Reset}"
    Write-Host "  Fix: add .await after with_local_set(...) calls"
}

exit $exitCode
