<#
.SYNOPSIS
    Lumen performance benchmark — measure system throughput for AI workloads.

.DESCRIPTION
    Benchmarks CPU, memory, disk I/O, and network latency to give Lumen
    users a clear picture of expected performance. Produces a score that
    can be compared across machines. Mac has no equivalent one-command benchmark.

.PARAMETER Quick
    Run a fast 5-second benchmark instead of the full 30-second suite.

.PARAMETER Json
    Output results as JSON for machine consumption.

.EXAMPLE
    .\lumen-bench.ps1
    .\lumen-bench.ps1 -Quick -Json
#>

param(
    [switch]$Quick,
    [switch]$Json
)

$Green  = "$([char]27)[0;32m"
$Yellow = "$([char]27)[1;33m"
$Blue   = "$([char]27)[0;34m"
$Bold   = "$([char]27)[1m"
$Reset  = "$([char]27)[0m"

$Duration = if ($Quick) { 3 } else { 10 }
$results  = [ordered]@{}

if (-not $Json) {
    Write-Host ""
    Write-Host "$Bold=== Lumen Performance Benchmark ===$Reset"
    Write-Host ""
}

# ── CPU ──
if (-not $Json) { Write-Host "$Bold[CPU]${Reset} Running ${Duration}s compute test..." }

$cpu = Get-CimInstance Win32_Processor
$cpuName = $cpu.Name.Trim()
$cores   = $cpu.NumberOfLogicalProcessors

$primeCount = 0
$sw = [Diagnostics.Stopwatch]::StartNew()
while ($sw.Elapsed.TotalSeconds -lt $Duration) {
    $n = 9999991
    $i = 2
    while ($i * $i -le $n) { if ($n % $i -eq 0) { break }; $i++ }
    $primeCount++
}
$sw.Stop()
$cpuScore = [Math]::Round($primeCount / $Duration, 0)
$results["cpu_cores"] = $cores
$results["cpu_name"] = $cpuName
$results["cpu_score"] = $cpuScore
if (-not $Json) { Write-Host "  ${Green}✓${Reset} $cpuScore primes/sec ($cores cores — $cpuName)" }

# ── Memory ──
if (-not $Json) { Write-Host "$Bold[Memory]${Reset} Testing bandwidth..." }

$mem = Get-CimInstance Win32_ComputerSystem
$totalGB = [Math]::Round($mem.TotalPhysicalMemory / 1GB, 1)

$size = if ($Quick) { 50MB } else { 200MB }
$data = New-Object byte[] $size
$rng = [Random]::new()
$sw = [Diagnostics.Stopwatch]::StartNew()
for ($i = 0; $i -lt $data.Length; $i++) { $data[$i] = $rng.Next(256) }
$sw.Stop()
$memScore = [Math]::Round($size / $sw.Elapsed.TotalSeconds / 1MB, 0)
$results["memory_gb"] = $totalGB
$results["memory_score"] = $memScore
if (-not $Json) { Write-Host "  ${Green}✓${Reset} ${memScore} MB/sec write ($totalGB GB total)" }

# ── Disk ──
if (-not $Json) { Write-Host "$Bold[Disk]${Reset} Testing I/O..." }

$tempFile = "$env:TEMP\lumen-bench-temp.bin"
try {
    $sw = [Diagnostics.Stopwatch]::StartNew()
    [IO.File]::WriteAllBytes($tempFile, $data)
    $sw.Stop()
    $writeScore = [Math]::Round($size / $sw.Elapsed.TotalSeconds / 1MB, 0)

    $sw.Restart()
    $null = [IO.File]::ReadAllBytes($tempFile)
    $sw.Stop()
    $readScore = [Math]::Round($size / $sw.Elapsed.TotalSeconds / 1MB, 0)

    $results["disk_write"] = $writeScore
    $results["disk_read"]  = $readScore
    if (-not $Json) { Write-Host "  ${Green}✓${Reset} Write: ${writeScore} MB/sec | Read: ${readScore} MB/sec" }
} finally {
    Remove-Item $tempFile -Force -ErrorAction SilentlyContinue
}

# ── Network ──
if (-not $Json) { Write-Host "$Bold[Network]${Reset} Testing latency..." }

$apiUrl = "https://api.deepseek.com/v1/models"
try {
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $null = Invoke-WebRequest -Uri $apiUrl -TimeoutSec 10 -UseBasicParsing
    $sw.Stop()
    $netScore = [Math]::Round($sw.Elapsed.TotalMilliseconds, 0)
    $results["network_latency_ms"] = $netScore
    if (-not $Json) { Write-Host "  ${Green}✓${Reset} ${netScore}ms to api.deepseek.com" }
} catch {
    $results["network_latency_ms"] = -1
    if (-not $Json) { Write-Host "  ${Yellow}⚠${Reset} Unreachable (offline?)" }
}

# ── Composite Score ──
$totalScore = [Math]::Round(($cpuScore * 0.4) + ($memScore * 0.1) + ($readScore * 0.1) + ([Math]::Max(0, 1000 - ($results["network_latency_ms"] ?? 500)) * 0.4), 0)
$results["composite_score"] = $totalScore

# Rating
$rating = if ($totalScore -gt 5000) { "🚀 Exceptional — ready for heavy AI workloads" }
    elseif ($totalScore -gt 2000) { "✅ Excellent — handles most tasks with ease" }
    elseif ($totalScore -gt 1000) { "👍 Good — suitable for daily development" }
    else { "⚠ Basic — consider hardware upgrade for large models" }

$results["rating"] = $rating

if ($Json) {
    @{
        timestamp = (Get-Date).ToString("o")
        os        = "$((Get-CimInstance Win32_OperatingSystem).Caption)"
        results   = $results
    } | ConvertTo-Json -Depth 2
} else {
    Write-Host ""
    Write-Host "=" * 55
    Write-Host "$Bold  Composite Score: $totalScore$Reset"
    Write-Host "  $rating"
    Write-Host "=" * 55
    Write-Host ""
    Write-Host "  Score breakdown:"
    Write-Host "    CPU:     ${cpuScore}/sec  (40%)"
    Write-Host "    Memory:  ${memScore} MB/sec (10%)"
    Write-Host "    Disk:    ${readScore} MB/sec  (10%)"
    Write-Host "    Network: $($results['network_latency_ms'])ms      (40%)"
    Write-Host ""
}
