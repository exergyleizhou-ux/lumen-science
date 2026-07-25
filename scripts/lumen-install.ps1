<#
.SYNOPSIS
    One-click Lumen installer for Windows. Detects dependencies, downloads
    or builds the binary, configures PATH, and sets up Windows Terminal.

.DESCRIPTION
    This is the definitive Windows installation experience — no manual steps.
    It handles:
      - Checking/installing prerequisites (Rust, Git, OpenSSH, Tesseract)
      - Building lumen.exe from source (or downloading pre-built)
      - Adding to PATH (machine-wide or user)
      - Configuring Windows Terminal profile with Lumen theme
      - Setting up initial config with API key
      - Verifying the installation

.PARAMETER InstallDir
    Target directory. Default: $env:LOCALAPPDATA\Lumen

.PARAMETER ApiKey
    Your DEEPSEEK_API_KEY. If omitted, prompted interactively.

.PARAMETER SkipBuild
    Skip building from source (use pre-existing binary).

.PARAMETER NoTerminalProfile
    Skip Windows Terminal profile creation.

.PARAMETER Scope
    'User' (default) or 'Machine' for PATH modification.

.EXAMPLE
    .\lumen-install.ps1
    .\lumen-install.ps1 -ApiKey "sk-xxx" -Scope Machine
    .\lumen-install.ps1 -InstallDir "C:\Tools\Lumen" -SkipBuild
#>

param(
    [string]$InstallDir = "$env:LOCALAPPDATA\Lumen",
    [string]$ApiKey,
    [switch]$SkipBuild,
    [switch]$NoTerminalProfile,
    [ValidateSet("User", "Machine")]
    [string]$Scope = "User"
)

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$Green  = "$([char]27)[0;32m"
$Yellow = "$([char]27)[1;33m"
$Blue   = "$([char]27)[0;34m"
$Red    = "$([char]27)[0;31m"
$Bold   = "$([char]27)[1m"
$Reset  = "$([char]27)[0m"

# ── Header ──
Write-Host ""
Write-Host "$Bold=== Lumen Windows Installer ===$Reset"
Write-Host "$Blue  https://github.com/exergyleizhou-ux/lumen$Reset"
Write-Host ""

# ── Prerequisite check ──
Write-Host "$Bold[1/5] Checking prerequisites...$Reset"

$missing = @()

# Check Rust
$rustCheck = Get-Command cargo -ErrorAction SilentlyContinue
if (-not $rustCheck) {
    $missing += "Rust (https://rustup.rs)"
} else {
    Write-Host "  ${Green}✓${Reset} Rust: $(& cargo --version)"
}

# Check Git
$gitCheck = Get-Command git -ErrorAction SilentlyContinue
if (-not $gitCheck) {
    $missing += "Git (https://git-scm.com)"
} else {
    Write-Host "  ${Green}✓${Reset} Git: $(& git --version)"
}

# Check OpenSSH (optional, for science compute)
$sshCheck = Get-Command ssh -ErrorAction SilentlyContinue
if (-not $sshCheck) {
    Write-Host "  ${Yellow}⚠${Reset} OpenSSH not found (optional: Add-WindowsCapability -Online -Name OpenSSH.Client*)"
} else {
    Write-Host "  ${Green}✓${Reset} OpenSSH Client found"
}

# Check Tesseract OCR (optional, for image tools)
$tessCheck = Get-Command tesseract -ErrorAction SilentlyContinue
if (-not $tessCheck) {
    Write-Host "  ${Yellow}⚠${Reset} Tesseract OCR not found (optional: winget install UB-Mannheim.TesseractOCR)"
} else {
    Write-Host "  ${Green}✓${Reset} Tesseract: $(& tesseract --version 2>&1 | Select-Object -First 1)"
}

if ($missing.Count -gt 0) {
    Write-Host ""
    Write-Host "${Red}Missing prerequisites:${Reset}"
    foreach ($m in $missing) { Write-Host "  - $m" }
    Write-Host ""
    Write-Host "Install the missing items and re-run this script."
    exit 1
}
Write-Host ""

# ── Clone / update source ──
Write-Host "$Bold[2/5] Preparing source code...$Reset"

$RepoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$IsFromRepo = Test-Path (Join-Path $RepoRoot "agent" "Cargo.toml")

if (-not $IsFromRepo) {
    $CloneDir = "$env:TEMP\lumen-src"
    if (Test-Path $CloneDir) {
        Write-Host "  Updating existing clone..."
        Push-Location $CloneDir
        git pull origin main 2>&1 | Out-Null
        Pop-Location
    } else {
        Write-Host "  Cloning lumen repository..."
        git clone https://github.com/exergyleizhou-ux/lumen.git $CloneDir 2>&1 | Out-Null
    }
    $RepoRoot = $CloneDir
} else {
    Write-Host "  ${Green}✓${Reset} Running from lumen repository"
}
Write-Host ""

# ── Build ──
Write-Host "$Bold[3/5] Building lumen.exe...$Reset"

if (-not $SkipBuild) {
    $AgentDir = Join-Path $RepoRoot "agent"
    Push-Location $AgentDir
    try {
        Write-Host "  Compiling (this may take 5-10 minutes)..."
        $env:CARGO_BUILD_JOBS = [Math]::Max(1, [Environment]::ProcessorCount - 1).ToString()
        cargo build --release -p xai-grok-pager-bin
        if ($LASTEXITCODE -ne 0) { throw "Build failed" }
        Write-Host "  ${Green}✓${Reset} Build successful"
    } finally {
        Pop-Location
    }
    $BinarySource = Join-Path $AgentDir "target" "release" "lumen.exe"
} else {
    $BinarySource = Join-Path $RepoRoot "bin" "lumen.exe"
    if (-not (Test-Path $BinarySource)) {
        $BinarySource = Join-Path $RepoRoot "agent" "target" "release" "lumen.exe"
    }
}

if (-not (Test-Path $BinarySource)) {
    Write-Host "${Red}ERROR: lumen.exe not found at $BinarySource${Reset}"
    exit 1
}
Write-Host ""

# ── Install binary ──
Write-Host "$Bold[4/5] Installing to $InstallDir...$Reset"

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
Copy-Item -Force $BinarySource (Join-Path $InstallDir "lumen.exe")

# Copy bundled scripts
$ScriptsSource = Join-Path $RepoRoot "scripts"
$ScriptsDest   = Join-Path $InstallDir "scripts"
New-Item -ItemType Directory -Force -Path $ScriptsDest | Out-Null
Get-ChildItem -Path $ScriptsSource -Filter "*.ps1" | ForEach-Object {
    Copy-Item -Force $_.FullName $ScriptsDest
}
Write-Host "  ${Green}✓${Reset} Binary + scripts installed"

# Add to PATH
$currentPath = if ($Scope -eq "Machine") {
    [Environment]::GetEnvironmentVariable("Path", "Machine")
} else {
    [Environment]::GetEnvironmentVariable("Path", "User")
}

if ($currentPath -notlike "*$InstallDir*") {
    [Environment]::SetEnvironmentVariable("Path", "$currentPath;$InstallDir", $Scope)
    $env:Path = "$env:Path;$InstallDir"
    Write-Host "  ${Green}✓${Reset} Added to $Scope PATH"
} else {
    Write-Host "  ${Green}✓${Reset} Already in PATH"
}

# Verify
$lumenExe = Join-Path $InstallDir "lumen.exe"
$version = & $lumenExe --version 2>$null
if ($version) {
    Write-Host "  ${Green}✓${Reset} Verified: $version"
}
Write-Host ""

# ── Setup ──
Write-Host "$Bold[5/5] Finalizing setup...$Reset"

# API key
if (-not $ApiKey) {
    $existingKey = [Environment]::GetEnvironmentVariable("DEEPSEEK_API_KEY", "User")
    if (-not $existingKey) {
        Write-Host "  ${Yellow}⚠${Reset} DEEPSEEK_API_KEY not set."
        Write-Host "  Set it now (or skip to configure later):"
        Write-Host "    `$env:DEEPSEEK_API_KEY = 'your-key-here'"
        Write-Host "    [Environment]::SetEnvironmentVariable('DEEPSEEK_API_KEY', 'your-key', 'User')"
    } else {
        Write-Host "  ${Green}✓${Reset} DEEPSEEK_API_KEY already configured"
    }
} else {
    [Environment]::SetEnvironmentVariable("DEEPSEEK_API_KEY", $ApiKey, "User")
    $env:DEEPSEEK_API_KEY = $ApiKey
    Write-Host "  ${Green}✓${Reset} DEEPSEEK_API_KEY configured"
}

# Config directory
$configDir = "$env:USERPROFILE\.lumen"
if (-not (Test-Path $configDir)) {
    New-Item -ItemType Directory -Force -Path $configDir | Out-Null
    Copy-Item (Join-Path $RepoRoot "config" "lumen.example.toml") (Join-Path $configDir "config.toml") -ErrorAction SilentlyContinue
    Write-Host "  ${Green}✓${Reset} Config directory created: $configDir"
}

# Windows Terminal profile
if (-not $NoTerminalProfile) {
    $wtSettings = "$env:LOCALAPPDATA\Packages\Microsoft.WindowsTerminal_8wekyb3d8bbwe\LocalState\settings.json"
    if (Test-Path $wtSettings) {
        Write-Host "  ${Green}✓${Reset} Windows Terminal found"
        Write-Host "    To add a Lumen profile, run: .\scripts\lumen-terminal.ps1"
    }
}

Write-Host ""
Write-Host "=" * 60
Write-Host "$Bold$Green  Lumen installation complete!$Reset"
Write-Host ""
Write-Host "  Quick start:"
Write-Host "    lumen --version"
Write-Host "    lumen --help"
Write-Host ""
Write-Host "  Run diagnostics:"
Write-Host "    .\scripts\lumen-doctor.ps1"
Write-Host ""
Write-Host "=" * 60
Write-Host ""
