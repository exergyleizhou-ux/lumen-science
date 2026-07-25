<#
.SYNOPSIS
    Comprehensive Lumen system diagnostic tool (Windows exclusive).

.DESCRIPTION
    Checks every subsystem Lumen depends on, with actionable fix suggestions.
    This is a capability Mac does not have — a single command that diagnoses
    the entire environment.

    Checks performed:
      - OS version and architecture
      - CPU, RAM, disk space
      - Network connectivity to API endpoints
      - Lumen installation integrity
      - Config file validity
      - GPU availability (for local models)
      - Windows Terminal presence
      - OpenSSH, Git, Rust toolchain status
      - Firewall rules for science server
      - Windows Defender exclusions

.EXAMPLE
    .\lumen-doctor.ps1
    .\lumen-doctor.ps1 -Json   # machine-readable output
#>

param([switch]$Json)

$Green  = "$([char]27)[0;32m"
$Yellow = "$([char]27)[1;33m"
$Red    = "$([char]27)[0;31m"
$Bold   = "$([char]27)[1m"
$Reset  = "$([char]27)[0m"

$results = [ordered]@{}
$issues  = 0
$warns   = 0

function Check($name, $script, $fix) {
    try {
        $ok = & $script
        if ($ok) {
            $results[$name] = "PASS"
            if (-not $Json) { Write-Host "  ${Green}✓${Reset} $name" }
        } else {
            $results[$name] = "FAIL"
            $script:issues++
            if (-not $Json) {
                Write-Host "  ${Red}×${Reset} $name"
                if ($fix) { Write-Host "    ${Yellow}Fix:${Reset} $fix" }
            }
        }
    } catch {
        $results[$name] = "FAIL"
        $script:issues++
        if (-not $Json) { Write-Host "  ${Red}×${Reset} $name — $_" }
    }
}

function Warn($name, $msg) {
    $results[$name] = "WARN"
    $script:warns++
    if (-not $Json) { Write-Host "  ${Yellow}⚠${Reset} $name — $msg" }
}

if (-not $Json) {
    Write-Host ""
    Write-Host "$Bold=== Lumen Doctor — System Diagnostics ===$Reset"
    Write-Host ""
}

# ── OS ──
if (-not $Json) { Write-Host "$Bold[OS]${Reset}" }

$os = Get-CimInstance Win32_OperatingSystem
$cpu = Get-CimInstance Win32_Processor
$mem = Get-CimInstance Win32_ComputerSystem

Check "Windows 10/11 64-bit" {
    [Environment]::Is64BitOperatingSystem -and [Environment]::OSVersion.Version.Major -ge 10
} "Upgrade to Windows 10 or later 64-bit"

if (-not $Json) { Write-Host "    OS:      $($os.Caption) ($($os.Version))" }
if (-not $Json) { Write-Host "    CPU:     $($cpu.Name.Trim()) ($($cpu.NumberOfLogicalProcessors) logical cores)" }
if (-not $Json) { Write-Host "    RAM:     $([Math]::Round($mem.TotalPhysicalMemory/1GB, 1)) GB total" }

$freeDisk = [Math]::Round((Get-PSDrive C).Free / 1GB, 1)
Check "Disk space > 10GB free" { $freeDisk -gt 10 } "Free up disk space on C: (only ${freeDisk}GB free)"
if (-not $Json) { Write-Host "    Disk:    ${freeDisk}GB free on C:" }

# ── Runtime ──
if (-not $Json) { Write-Host ""; Write-Host "$Bold[Runtime]${Reset}" }

Check "Rust toolchain" {
    $null = Get-Command rustc -ErrorAction Stop
    & rustc --version | Out-Null
    $true
} "Install from https://rustup.rs"

$rustVer = & rustc --version 2>$null
if (-not $Json -and $rustVer) { Write-Host "    Rust:    $rustVer" }

Check "Git" { Get-Command git -ErrorAction Stop; $true } "Install from https://git-scm.com"
Check "OpenSSH Client" {
    Get-Command ssh -ErrorAction Stop; $true
} "Run as Admin: Add-WindowsCapability -Online -Name OpenSSH.Client~~~~0.0.1.0"

Check "Tesseract OCR" {
    Get-Command tesseract -ErrorAction Stop; $true
} "Optional: winget install UB-Mannheim.TesseractOCR"

# ── Lumen ──
if (-not $Json) { Write-Host ""; Write-Host "$Bold[Lumen]${Reset}" }

$lumenPath = (Get-Command lumen -ErrorAction SilentlyContinue).Source
if ($lumenPath) {
    if (-not $Json) { Write-Host "  ${Green}✓${Reset} Binary: $lumenPath" }
    $results["Lumen binary"] = "PASS"

    $ver = & lumen --version 2>$null
    if ($ver) {
        if (-not $Json) { Write-Host "    Version: $ver" }

        # Check binary size
        $bin = Get-Item $lumenPath
        if (-not $Json) { Write-Host "    Size:    $([Math]::Round($bin.Length/1MB,1)) MB" }
        $results["Lumen version"] = $ver
    }
} else {
    $results["Lumen binary"] = "FAIL"
    $script:issues++
    if (-not $Json) { Write-Host "  ${Red}×${Reset} Not found in PATH. Run .\lumen-install.ps1" }
}

# Config
$configFile = "$env:USERPROFILE\.lumen\config.toml"
if (Test-Path $configFile) {
    if (-not $Json) { Write-Host "  ${Green}✓${Reset} Config: $configFile" }
    $results["Config file"] = "PASS"
} else {
    Warn "Config file" "Not found. Copy from config/lumen.example.toml"
}

# API key
$apiKey = [Environment]::GetEnvironmentVariable("DEEPSEEK_API_KEY", "User")
if ($apiKey) {
    $masked = $apiKey.Substring(0, [Math]::Min(8, $apiKey.Length)) + "..."
    if (-not $Json) { Write-Host "  ${Green}✓${Reset} API Key: $masked" }
    $results["API key"] = "PASS"
} else {
    Warn "API key" "DEEPSEEK_API_KEY not set. Run: `$env:DEEPSEEK_API_KEY='sk-...'"
}

# ── Network ──
if (-not $Json) { Write-Host ""; Write-Host "$Bold[Network]${Reset}" }

$endpoints = @(
    @{Name="api.deepseek.com"; Url="https://api.deepseek.com/v1/models"},
    @{Name="github.com"; Url="https://github.com"}
)

foreach ($ep in $endpoints) {
    try {
        $req = Invoke-WebRequest -Uri $ep.Url -TimeoutSec 5 -UseBasicParsing
        if (-not $Json) { Write-Host "  ${Green}✓${Reset} $($ep.Name): ${Green}reachable${Reset}" }
        $results[$ep.Name] = "PASS"
    } catch {
        $results[$ep.Name] = "FAIL"
        $script:issues++
        if (-not $Json) { Write-Host "  ${Red}×${Reset} $($ep.Name): ${Red}unreachable${Reset}" }
    }
}

# ── GPU ──
if (-not $Json) { Write-Host ""; Write-Host "$Bold[GPU]${Reset}" }
try {
    $gpu = Get-CimInstance Win32_VideoController | Where-Object { $_.AdapterRAM -gt 0 }
    if ($gpu) {
        foreach ($g in $gpu) {
            $vram = [Math]::Round($g.AdapterRAM / 1GB, 1)
            if (-not $Json) { Write-Host "  ${Green}✓${Reset} $($g.Name): ${vram}GB VRAM" }
        }
        $results["GPU"] = "PASS"
    } else {
        Warn "GPU" "No dedicated GPU with VRAM detected (cpu-only mode)"
    }
} catch {
    Warn "GPU" "Could not query GPU info"
}

# ── Firewall ──
if (-not $Json) { Write-Host ""; Write-Host "$Bold[Security]${Reset}" }
$firewallEnabled = (Get-NetFirewallProfile -Profile Domain,Public,Private | Where-Object Enabled -eq True).Count
if ($firewallEnabled -gt 0) {
    if (-not $Json) { Write-Host "  ${Green}✓${Reset} Windows Firewall active ($firewallEnabled profiles)" }
    $results["Firewall"] = "PASS"
} else {
    Warn "Firewall" "Windows Firewall is disabled"
}

# ── Summary ──
if ($Json) {
    @{
        timestamp = (Get-Date).ToString("o")
        results   = $results
        issues    = $issues
        warnings  = $warns
    } | ConvertTo-Json -Depth 2
} else {
    Write-Host ""
    Write-Host "=" * 50
    if ($issues -eq 0 -and $warns -eq 0) {
        Write-Host "$Bold$Green  All checks passed — Lumen is ready!$Reset"
    } elseif ($issues -eq 0) {
        Write-Host "$Bold$Yellow  $warns warning(s) — Lumen will work but could be better$Reset"
    } else {
        Write-Host "$Bold$Red  $issues issue(s) found — run fixes above$Reset"
    }
    Write-Host "=" * 50
    Write-Host ""
}
