<#
.SYNOPSIS
    Run Lumen e2e tests with guaranteed fresh binary (Windows PowerShell edition).

.DESCRIPTION
    The e2e test harness spawns target/debug/lumen from xai-grok-pager-bin.
    If the binary is stale, tests fail with "Method not found" (404).
    This wrapper ensures the binary is rebuilt before running e2e tests.

.PARAMETER NoBuild
    Skip the build step if you already have a fresh binary.

.PARAMETER Test
    Run only the specified test (passed to cargo test -- <name>).

.PARAMETER Release
    Build and test the release binary instead of debug.

.EXAMPLE
    .\scripts\lumen-e2e.ps1
    .\scripts\lumen-e2e.ps1 -Test my_e2e_test
    .\scripts\lumen-e2e.ps1 -NoBuild
    .\scripts\lumen-e2e.ps1 -Release
#>

param(
    [switch]$NoBuild,
    [string]$Test,
    [switch]$Release
)

$ErrorActionPreference = "Stop"

$RepoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$AgentDir = Join-Path $RepoRoot "agent"
$TestThreads = if ($env:LUMEN_E2E_TEST_THREADS) { $env:LUMEN_E2E_TEST_THREADS } else { "4" }
$Profile = if ($Release) { "release" } else { "debug" }

$Green  = "$([char]27)[0;32m"
$Yellow = "$([char]27)[1;33m"
$Reset  = "$([char]27)[0m"

Write-Host "=== Lumen E2E Test Runner ==="
Write-Host ""

if (-not $NoBuild) {
    Write-Host "${Yellow}Building pager binary ($Profile)...${Reset}"
    Push-Location $AgentDir
    try {
        if ($Release) {
            cargo build -p xai-grok-pager-bin --release
        } else {
            cargo build -p xai-grok-pager-bin
        }
    } finally {
        Pop-Location
    }
    Write-Host "${Green}✓ Binary built${Reset}"
    Write-Host ""
}

$Binary = Join-Path $AgentDir "target" $Profile "lumen.exe"
if (-not (Test-Path $Binary)) {
    Write-Host "ERROR: Binary not found at $Binary"
    Write-Host "Run without -NoBuild to build it first."
    exit 1
}

Write-Host "Binary: $Binary"
if (Test-Path $Binary) {
    $binInfo = Get-Item $Binary
    Write-Host "Modified: $($binInfo.LastWriteTime.ToString('s'))"
    Write-Host "Size:     $($binInfo.Length.ToString('N0')) bytes"
}
Write-Host ""

Write-Host "${Yellow}Running e2e tests...${Reset}"

Push-Location $AgentDir
try {
    $testArgs = @("test", "--test-threads=$TestThreads")
    if ($Test) {
        $testArgs += "--", $Test
    }
    & cargo @testArgs
    $exitCode = $LASTEXITCODE
} finally {
    Pop-Location
}

Write-Host ""
if ($exitCode -eq 0) {
    Write-Host "${Green}✓ All e2e tests passed${Reset}"
} else {
    Write-Host "× E2e tests failed (exit: $exitCode)"
}
exit $exitCode
