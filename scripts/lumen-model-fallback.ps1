<#
.SYNOPSIS
    Monitor API health and suggest model switches (Windows PowerShell edition).

.DESCRIPTION
    Watches Lumen's proxy log for consecutive upstream failures and suggests
    switching to a fallback model when a threshold is reached.

.PARAMETER Watch
    Continuously monitor the proxy log for failures.

.PARAMETER Reset
    Clear the failure counter state file.

.EXAMPLE
    .\scripts\lumen-model-fallback.ps1           # one-shot check
    .\scripts\lumen-model-fallback.ps1 -Watch     # continuous monitoring
    .\scripts\lumen-model-fallback.ps1 -Reset     # clear failure counters
#>

param(
    [switch]$Watch,
    [switch]$Reset
)

$Red    = "$([char]27)[0;31m"
$Yellow = "$([char]27)[1;33m"
$Green  = "$([char]27)[0;32m"
$Reset  = "$([char]27)[0m"

$LumenDir  = if ($env:LUMEN_DIR) { $env:LUMEN_DIR } else { "$env:USERPROFILE\.lumen" }
$ProxyLog  = Join-Path $LumenDir "science" "proxy.log"
$StateFile = Join-Path $LumenDir ".api_failure_state"
$Threshold = if ($env:LUMEN_API_FAILURE_THRESHOLD) { [int]$env:LUMEN_API_FAILURE_THRESHOLD } else { 3 }

function Reset-State {
    if (Test-Path $StateFile) {
        Remove-Item $StateFile -Force
    }
    Write-Host "${Green}✓${Reset} API failure state reset"
}

function Check-Health {
    if (-not (Test-Path $ProxyLog)) {
        Write-Host "${Green}✓${Reset} No proxy log found — API appears healthy"
        return 0
    }

    $allFailures = (Select-String -Path $ProxyLog -Pattern "context canceled|upstream jitter.*retry" -SimpleMatch).Count

    if ($allFailures -eq 0) {
        Write-Host "${Green}✓${Reset} API healthy — 0 recent failures"
        return 0
    }

    # Count consecutive failures from the end of the log
    $lines = Get-Content -Path $ProxyLog
    $consecutive = 0
    for ($i = $lines.Count - 1; $i -ge 0; $i--) {
        if ($lines[$i] -match "context canceled|upstream jitter") {
            $consecutive++
        } else {
            break
        }
    }

    if ($consecutive -ge $Threshold) {
        Write-Host "${Red}×${Reset} API degraded — $consecutive consecutive failures (threshold: $Threshold)"
        Write-Host ""
        Write-Host "  Suggested actions:"
        Write-Host "  1. Switch model:  lumen -m deepseek-v4-pro"
        Write-Host "  2. Check network: curl -I https://api.deepseek.com"
        Write-Host "  3. Increase timeout: `$env:LUMEN_API_TIMEOUT_SECS = 60"
        Write-Host "  4. Retry later: the API may be experiencing temporary issues"
        return 2
    } elseif ($consecutive -gt 0) {
        Write-Host "${Yellow}⚠${Reset} API has $consecutive recent failures (below threshold $Threshold)"
        return 1
    }

    Write-Host "${Green}✓${Reset} API healthy"
    return 0
}

function Watch-Log {
    Write-Host "Watching $ProxyLog for API failures (Ctrl+C to stop)..."
    Write-Host "Threshold: $Threshold consecutive failures"
    Write-Host ""

    if (-not (Test-Path $ProxyLog)) {
        New-Item -Path $ProxyLog -ItemType File -Force | Out-Null
    }

    $lastSize = (Get-Item $ProxyLog).Length

    while ($true) {
        Start-Sleep -Seconds 2
        if (-not (Test-Path $ProxyLog)) { continue }

        $currentSize = (Get-Item $ProxyLog).Length
        if ($currentSize -gt $lastSize) {
            $newContent = Get-Content -Path $ProxyLog -Tail 20
            $recentCount = ($newContent | Select-String -Pattern "context canceled|upstream jitter.*retry").Count

            if ($recentCount -gt 0) {
                $ts = Get-Date -Format "HH:mm:ss"
                Write-Host "${Yellow}[$ts]${Reset} API failure detected ($recentCount recent)"

                if ($recentCount -ge $Threshold) {
                    Write-Host "${Red}[$ts] THRESHOLD EXCEEDED — consider switching model${Reset}"
                    Write-Host "  → lumen -m deepseek-v4-pro"
                }
            }
            $lastSize = $currentSize
        }
    }
}

# ── Main ──
if ($Reset) {
    Reset-State
} elseif ($Watch) {
    Watch-Log
} else {
    $code = Check-Health
    exit $code
}
